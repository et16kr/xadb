//! Full-state observations and unpublished frame/page production for transactions.
//! Contract: `devdoc/contracts/dev3-obligation-format.md`, sections 13–17.
//! No append, live page mutation, source pin, reservation restoration or apply permit.
#![allow(dead_code)] // Awaiting the separately reviewed runtime consumer.

use super::{
    allocation_page::{ExtentAddress, ReservationOwner, SystemWorkRef},
    control::{ControlKind, ControlRef, PROFILE_BYTES, Profile},
    obligation::{
        DescriptorSpec, ObligationError, OwnerState, ResourcePeak, decode_descriptor,
        encode_descriptor, validate_descriptor,
    },
    page::{DIGEST_OFFSET, PAGE_BYTES, PageError, PageIdentity, PageKind},
    status_home::{MAX_HOME_SLOTS, STATUS_HOME_BYTES, StatusHome, decode_status_home_page},
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell,
        TxnRef,
    },
    wal_frame::{
        FrameEnvelope, FrameExpectation, FrameKind, RedoKind, RedoRecordView, RedoTarget, WalError,
        WalFrameHeader, decode_redo_record, decode_semantic_envelope, encode_patch_record,
        finish_semantic_envelope,
    },
};
use xadb_foundation::codec::content_digest;

const FRAME_BYTES: usize = 3040;
const GROUP_BYTES: usize = 2848;
const DESCRIPTOR: std::ops::Range<usize> = 192..1728;
const OPERATION: std::ops::Range<usize> = 1728..1984;
const NATIVE: std::ops::Range<usize> = 1984..2560;
const REDO: std::ops::Range<usize> = 2560..2848;

#[path = "semantic_begin.rs"]
pub(crate) mod begin;

#[path = "semantic_empty_abort.rs"]
pub(crate) mod empty_abort;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticError {
    InvalidInput,
    Truncated,
    UnsupportedFormat,
    UnsupportedOperation,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
}
type Result<T> = std::result::Result<T, SemanticError>;

/// Trusted selected context, never recovered by copying identities from a frame.
/// The caller owns adopted-prefix and before-state lifetimes across observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RollbackStartExpected {
    pub(crate) frame: FrameExpectation,
    pub(crate) profile: Profile,
    pub(crate) actor: TxnRef,
    pub(crate) location: StatusSlotRef,
    pub(crate) page: PageIdentity,
}

/// Immutable observation of this call's bytes; not a runtime capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RollbackStartObservation {
    header: WalFrameHeader,
    after: DescriptorSpec,
    location: StatusSlotRef,
}
impl RollbackStartObservation {
    pub(crate) fn header(self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn after_descriptor(self) -> DescriptorSpec {
        self.after
    }
    pub(crate) fn status_location(self) -> StatusSlotRef {
        self.location
    }
}

impl From<WalError> for SemanticError {
    fn from(error: WalError) -> Self {
        match error {
            WalError::InvalidInput => Self::InvalidInput,
            WalError::Truncated => Self::Truncated,
            WalError::UnsupportedFormat => Self::UnsupportedFormat,
            WalError::IdentityMismatch => Self::IdentityMismatch,
            WalError::OrderMismatch => Self::OrderMismatch,
            WalError::Corrupt | WalError::NoSpace | WalError::TooLarge => Self::Corrupt,
        }
    }
}
impl From<PageError> for SemanticError {
    fn from(error: PageError) -> Self {
        match error {
            PageError::UnsupportedFormat => Self::UnsupportedFormat,
            PageError::IdentityMismatch => Self::IdentityMismatch,
            PageError::InvalidInput
            | PageError::Corrupt
            | PageError::KindMismatch
            | PageError::NoSpace => Self::Corrupt,
        }
    }
}
impl From<ObligationError> for SemanticError {
    fn from(error: ObligationError) -> Self {
        match error {
            ObligationError::UnsupportedFormat => Self::UnsupportedFormat,
            ObligationError::IdentityMismatch => Self::IdentityMismatch,
            ObligationError::InvalidInput
            | ObligationError::Corrupt
            | ObligationError::UnsupportedWorkBody => Self::Corrupt,
        }
    }
}
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let mut value = [0; 4];
    value.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(value)
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(value)
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn validate_expected(expected: RollbackStartExpected) -> Result<ControlRef> {
    let namespace = expected.profile.spec().namespace;
    let stream = expected.frame.stream;
    let actor = expected.actor.id();
    if stream.database != namespace.database
        || stream.timeline != namespace.timeline
        || stream.writer_epoch == 0
        || actor.database != namespace.database
        || actor.timeline != namespace.timeline
        || expected.page.database != namespace.database
        || expected.location.page() != expected.page.page
        || expected.location.generation() != expected.actor.generation()
        || usize::from(expected.location.slot()) >= MAX_HOME_SLOTS
        || expected.page.page.page_no.is_multiple_of(64)
        || ExtentAddress::containing(expected.page.page).is_err()
    {
        return Err(SemanticError::InvalidInput);
    }
    Ok(ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: expected.profile.spec().generation,
        encoded_len: PROFILE_BYTES as u32,
        digest: expected.profile.digest(),
    })
}

fn group_owner(bytes: &[u8]) -> Result<ReservationOwner> {
    if !zero(&bytes[1..8]) {
        return Err(SemanticError::Corrupt);
    }
    match bytes[0] {
        1 => TxnRef::decode(&bytes[8..56])
            .map(ReservationOwner::Transaction)
            .map_err(|_| SemanticError::Corrupt),
        2 => SystemWorkRef::decode(&bytes[8..56])
            .map(ReservationOwner::System)
            .map_err(|_| SemanticError::Corrupt),
        _ => Err(SemanticError::Corrupt),
    }
}

fn validate_group_header(
    group: &[u8],
    header: WalFrameHeader,
    actor: TxnRef,
    before: &DescriptorSpec,
    before_bytes: &[u8],
) -> Result<()> {
    if u16_at(group, 8) != 192
        || !zero(&group[10..12])
        || u32_at(group, 12) as usize != GROUP_BYTES
        || u32_at(group, 120) != 1536
        || u16_at(group, 124) != 1
        || !zero(&group[126..132])
        || u16_at(group, 132) != 1
        || !zero(&group[134..136])
        || u32_at(group, 136) != 576
        || !zero(&group[140..152])
        || u32_at(group, 152) != 288
        || !zero(&group[156..192])
    {
        return Err(SemanticError::Corrupt);
    }
    if group_owner(&group[32..88])? != ReservationOwner::Transaction(actor)
        || header.txn != Some(actor.id())
    {
        return Err(SemanticError::IdentityMismatch);
    }
    if u64_at(group, 16) != header.lsn
        || u64_at(group, 24) != before.chain.apply
        || header.previous_txn_lsn != before.chain.apply
        || group[88..120] != content_digest(before_bytes)
    {
        return Err(SemanticError::OrderMismatch);
    }
    Ok(())
}

fn validate_transition(
    before: &DescriptorSpec,
    after: &DescriptorSpec,
    old_status: TransactionStatus,
    header: WalFrameHeader,
    operation: &[u8],
) -> Result<()> {
    if after.chain.apply != header.lsn
        || after.chain.previous != before.chain.apply
        || after.chain.birth != before.chain.birth
        || before.chain.groups.checked_add(1) != Some(after.chain.groups)
    {
        return Err(SemanticError::OrderMismatch);
    }
    let next = old_status.undo_head();
    let status = TransactionStatus::new(
        old_status.reference(),
        TransactionState::Aborting(RollbackState::Transaction { next }),
        next,
    )
    .map_err(|_| SemanticError::Corrupt)?;
    if after.owner != OwnerState::Transaction(status) {
        return Err(SemanticError::Corrupt);
    }
    // One exact opcode-specific core avoids accepting unused fields or an absent
    // ref's hidden bytes. This fixed local is not a public encoder or apply plan.
    let mut core = [0; 256];
    core[..2].copy_from_slice(&3_u16.to_le_bytes());
    core[4..8].copy_from_slice(&256_u32.to_le_bytes());
    if let Some(reference) = next {
        core[2..4].copy_from_slice(&2_u16.to_le_bytes());
        reference
            .encode(&mut core[72..112])
            .map_err(|_| SemanticError::Corrupt)?;
    }
    if operation != core {
        return Err(SemanticError::Corrupt);
    }
    Ok(())
}

fn validate_budget(before: &DescriptorSpec, after: &DescriptorSpec) -> Result<()> {
    let mut remaining = before.remaining;
    let status = &mut remaining[3];
    status.steps = status
        .steps
        .checked_sub(1)
        .ok_or(SemanticError::BudgetMismatch)?;
    status.wal = status
        .wal
        .checked_sub(FRAME_BYTES as u64)
        .ok_or(SemanticError::BudgetMismatch)?;
    status.protected = status
        .protected
        .checked_sub(FRAME_BYTES as u64)
        .ok_or(SemanticError::BudgetMismatch)?;
    if status.steps == 0 {
        status.peak = ResourcePeak::default();
    }
    if remaining != after.remaining
        || before.consumed_wal.checked_add(FRAME_BYTES as u64) != Some(after.consumed_wal)
        || before.consumed_control != after.consumed_control
        || before.inventory != after.inventory
        || before.minimum_wal_lsn != after.minimum_wal_lsn
        || before.last_mutation_sequence != after.last_mutation_sequence
    {
        return Err(SemanticError::BudgetMismatch);
    }
    Ok(())
}

fn rollback_envelope(frame: &[u8], expected: FrameExpectation) -> Result<FrameEnvelope<'_>> {
    let envelope = decode_semantic_envelope(frame, expected)?;
    let header = envelope.header;
    if header.kind != FrameKind::Update {
        return Err(SemanticError::Corrupt);
    }
    let group = envelope.payload;
    if group.len() < 8 || &group[..4] != b"XSG1" {
        return Err(SemanticError::Corrupt);
    }
    if u16_at(group, 4) != 1 {
        return Err(SemanticError::UnsupportedFormat);
    }
    if u16_at(group, 6) != 3 {
        return Err(SemanticError::UnsupportedOperation);
    }
    if frame.len() != FRAME_BYTES || group.len() != GROUP_BYTES || envelope.record_count != 1 {
        return Err(SemanticError::Corrupt);
    }
    Ok(envelope)
}

fn status_redo<'a>(
    header: WalFrameHeader,
    group: &'a [u8],
    expected: RollbackStartExpected,
) -> Result<RedoRecordView<'a>> {
    let redo = decode_redo_record(header, &group[REDO])?;
    if redo.kind != RedoKind::Patch
        || redo.target.kind != PageKind::TransactionStatus
        || redo.target.level != 0
    {
        return Err(SemanticError::Corrupt);
    }
    if redo.target.page != expected.page.page
        || redo.target.owner != expected.page.owner
        || redo.target.owner_generation != expected.page.owner_generation
    {
        return Err(SemanticError::IdentityMismatch);
    }
    Ok(redo)
}

/// Produce the exact RollbackStart frame and its unpublished after page.
/// The caller owns both fixed outputs and must discard both on any error.
/// Immutable before inputs survive; success is only an observation, not an
/// append, reservation, live-frame or recovery capability. No I/O occurs.
pub(crate) fn produce_rollback_start(
    expected: RollbackStartExpected,
    lsn: u64,
    before_descriptor: &[u8],
    before_page: &[u8],
    frame_output: &mut [u8; FRAME_BYTES],
    page_output: &mut [u8; PAGE_BYTES],
) -> Result<RollbackStartObservation> {
    let profile = validate_expected(expected)?;
    if lsn == 0 || lsn <= expected.frame.previous_lsn {
        return Err(SemanticError::InvalidInput);
    }
    // End the descriptor/encoder local scope before entering materialization.
    build_rollback_frame(
        expected,
        profile,
        lsn,
        before_descriptor,
        before_page,
        frame_output,
    )?;
    materialize_rollback_start(
        frame_output,
        expected,
        before_descriptor,
        before_page,
        page_output,
    )
}

fn build_rollback_frame(
    expected: RollbackStartExpected,
    profile: ControlRef,
    lsn: u64,
    before_descriptor: &[u8],
    before_page: &[u8],
    output: &mut [u8; FRAME_BYTES],
) -> Result<()> {
    let before = decode_descriptor(
        before_descriptor,
        ReservationOwner::Transaction(expected.actor),
        profile,
    )?;
    let OwnerState::Transaction(old_status) = before.owner else {
        return Err(SemanticError::UnsupportedOperation);
    };
    if old_status.state() != TransactionState::Active {
        return Err(SemanticError::UnsupportedOperation);
    }
    if before.chain.apply > expected.frame.previous_lsn {
        return Err(SemanticError::OrderMismatch);
    }
    let old_page = decode_status_home_page(
        before_page,
        expected.page,
        expected.profile.spec().namespace,
    )?;
    let old_home = old_page.lookup(expected.actor, expected.location)?;
    if old_home.core() != TransactionStatusCell::Occupied(old_status) {
        return Err(SemanticError::Corrupt);
    }
    if old_page.lsn() > expected.frame.previous_lsn {
        return Err(SemanticError::OrderMismatch);
    }
    let range = old_page
        .cell_range(usize::from(expected.location.slot()))
        .ok_or(SemanticError::IdentityMismatch)?;
    let next = old_status.undo_head();
    let status = TransactionStatus::new(
        expected.actor,
        TransactionState::Aborting(RollbackState::Transaction { next }),
        next,
    )
    .map_err(|_| SemanticError::Corrupt)?;
    let mut after = before;
    after.owner = OwnerState::Transaction(status);
    after.chain.apply = lsn;
    after.chain.previous = before.chain.apply;
    after.chain.groups = before
        .chain
        .groups
        .checked_add(1)
        .ok_or(SemanticError::OrderMismatch)?;
    let remaining = &mut after.remaining[3];
    remaining.steps = remaining
        .steps
        .checked_sub(1)
        .ok_or(SemanticError::BudgetMismatch)?;
    remaining.wal = remaining
        .wal
        .checked_sub(FRAME_BYTES as u64)
        .ok_or(SemanticError::BudgetMismatch)?;
    remaining.protected = remaining
        .protected
        .checked_sub(FRAME_BYTES as u64)
        .ok_or(SemanticError::BudgetMismatch)?;
    if remaining.steps == 0 {
        remaining.peak = ResourcePeak::default();
    }
    after.consumed_wal = before
        .consumed_wal
        .checked_add(FRAME_BYTES as u64)
        .ok_or(SemanticError::BudgetMismatch)?;
    // A valid before descriptor may leave nonzero consumables at step zero.
    // Do not erase them or confuse this impossible budget with corrupt input.
    validate_descriptor(&after).map_err(|_| SemanticError::BudgetMismatch)?;

    output.fill(0);
    let group = &mut output[160..FRAME_BYTES - 32];
    group[..4].copy_from_slice(b"XSG1");
    group[4..6].copy_from_slice(&1_u16.to_le_bytes());
    group[6..8].copy_from_slice(&3_u16.to_le_bytes());
    group[8..10].copy_from_slice(&192_u16.to_le_bytes());
    group[12..16].copy_from_slice(&(GROUP_BYTES as u32).to_le_bytes());
    group[16..24].copy_from_slice(&lsn.to_le_bytes());
    group[24..32].copy_from_slice(&before.chain.apply.to_le_bytes());
    group[32] = 1;
    expected.actor.encode(&mut group[40..88])?;
    group[88..120].copy_from_slice(&content_digest(before_descriptor));
    group[120..124].copy_from_slice(&1536_u32.to_le_bytes());
    group[124..126].copy_from_slice(&1_u16.to_le_bytes());
    group[132..134].copy_from_slice(&1_u16.to_le_bytes());
    group[136..140].copy_from_slice(&576_u32.to_le_bytes());
    group[152..156].copy_from_slice(&288_u32.to_le_bytes());
    encode_descriptor(&after, &mut group[DESCRIPTOR])?;
    let operation = &mut group[OPERATION];
    operation[..2].copy_from_slice(&3_u16.to_le_bytes());
    operation[4..8].copy_from_slice(&256_u32.to_le_bytes());
    if let Some(reference) = next {
        operation[2..4].copy_from_slice(&2_u16.to_le_bytes());
        reference.encode(&mut operation[72..112])?;
    }
    let native = &mut group[NATIVE];
    native[0] = 5;
    native[1] = 3;
    for offset in [4, 6] {
        native[offset..offset + 2].copy_from_slice(&expected.location.slot().to_le_bytes());
    }
    for offset in [8, 10] {
        native[offset..offset + 2].copy_from_slice(&(range.start as u16).to_le_bytes());
    }
    for offset in [12, 16] {
        native[offset..offset + 4].copy_from_slice(&(STATUS_HOME_BYTES as u32).to_le_bytes());
    }
    native[20..24].copy_from_slice(&576_u32.to_le_bytes());
    native[128..352].copy_from_slice(&before_page[range.clone()]);
    StatusHome::new(TransactionStatusCell::Occupied(status), old_home.incoming())?
        .encode(&mut native[352..576])?;
    let old_digest = content_digest(&native[128..352]);
    let new_digest = content_digest(&native[352..576]);
    native[24..56].copy_from_slice(&old_digest);
    native[56..88].copy_from_slice(&new_digest);
    // Borrow the prepared new home directly; no second home/frame/page array.
    let (prepared, redo) = group.split_at_mut(REDO.start);
    encode_patch_record(
        RedoTarget {
            page: expected.page.page,
            owner: expected.page.owner,
            owner_generation: expected.page.owner_generation,
            kind: PageKind::TransactionStatus,
            level: 0,
        },
        old_page.lsn(),
        range.start as u16,
        &prepared[NATIVE.start + 352..NATIVE.end],
        redo,
    )?;
    finish_semantic_envelope(
        WalFrameHeader {
            stream: expected.frame.stream,
            kind: FrameKind::Update,
            lsn,
            previous_lsn: expected.frame.previous_lsn,
            txn: Some(expected.actor.id()),
            previous_txn_lsn: before.chain.apply,
        },
        1,
        output,
    )?;
    Ok(())
}

/// Construct an unpublished status-page image from the bound RollbackStart patch.
/// Success returns only a byte observation; it grants no live-frame or WAL authority.
/// Immutable inputs are preserved. On any error the caller must discard output,
/// which may already contain an image rejected by the final semantic/cost checks.
/// Uses the caller's one 16 KiB output, with no page-sized local copy or I/O.
pub(crate) fn materialize_rollback_start(
    frame: &[u8],
    expected: RollbackStartExpected,
    before_descriptor: &[u8],
    before_page: &[u8],
    output: &mut [u8; PAGE_BYTES],
) -> Result<RollbackStartObservation> {
    validate_expected(expected)?;
    let envelope = rollback_envelope(frame, expected.frame)?;
    let redo = status_redo(envelope.header, envelope.payload, expected)?;
    let old = decode_status_home_page(
        before_page,
        expected.page,
        expected.profile.spec().namespace,
    )?;
    old.lookup(expected.actor, expected.location)?;
    if old.lsn() > expected.frame.previous_lsn || redo.prior_lsn != old.lsn() {
        return Err(SemanticError::OrderMismatch);
    }
    let range = old
        .cell_range(usize::from(expected.location.slot()))
        .ok_or(SemanticError::IdentityMismatch)?;
    if range.len() != STATUS_HOME_BYTES
        || usize::from(redo.offset) != range.start
        || redo.data.len() != range.len()
    {
        return Err(SemanticError::Corrupt);
    }
    output.copy_from_slice(before_page);
    output[range].copy_from_slice(redo.data);
    output[64..72].copy_from_slice(&envelope.header.lsn.to_le_bytes());
    let digest = content_digest(&output[..DIGEST_OFFSET]);
    output[DIGEST_OFFSET..].copy_from_slice(&digest);
    // Byte construction alone does not bind the owner transition or its budget.
    // The existing full-state observer remains the final authority for this result.
    observe_rollback_start(frame, expected, before_descriptor, before_page, output)
}

/// Observe the exact Active -> full-transaction Aborting transition. Every page
/// cell is validated; only the selected home, page LSN and checksum may differ.
/// Inputs remain unchanged on success and error. No I/O/allocation/locks occur.
///
/// Caller-supplied expected context and adopted before-state must remain stable;
/// this call does not prove undo liveness or future resource sufficiency. Errors
/// are known/no-effect and ordered as the section-13 fixed corpus specifies.
pub(crate) fn observe_rollback_start(
    frame: &[u8],
    expected: RollbackStartExpected,
    before_descriptor: &[u8],
    before_page: &[u8],
    after_page: &[u8],
) -> Result<RollbackStartObservation> {
    let profile = validate_expected(expected)?;
    let owner = ReservationOwner::Transaction(expected.actor);
    let before = decode_descriptor(before_descriptor, owner, profile)?;
    let OwnerState::Transaction(old_status) = before.owner else {
        return Err(SemanticError::UnsupportedOperation);
    };
    if old_status.state() != TransactionState::Active {
        return Err(SemanticError::UnsupportedOperation);
    }
    if before.chain.apply > expected.frame.previous_lsn {
        return Err(SemanticError::OrderMismatch);
    }
    let envelope = rollback_envelope(frame, expected.frame)?;
    let header = envelope.header;
    let group = envelope.payload;
    validate_group_header(group, header, expected.actor, &before, before_descriptor)?;
    let after = decode_descriptor(&group[DESCRIPTOR], owner, profile)?;
    validate_transition(&before, &after, old_status, header, &group[OPERATION])?;
    let OwnerState::Transaction(new_status) = after.owner else {
        return Err(SemanticError::Corrupt);
    };
    observe_status_patch(
        header,
        group,
        expected,
        before_page,
        after_page,
        old_status,
        new_status,
    )?;
    validate_budget(&before, &after)?;
    Ok(RollbackStartObservation {
        header,
        after,
        location: expected.location,
    })
}

/// Common page/native/redo binding for one Status home. Owner state and budget
/// transitions remain opcode-specific; this returns no live installation right.
fn observe_status_patch(
    header: WalFrameHeader,
    group: &[u8],
    expected: RollbackStartExpected,
    before_page: &[u8],
    after_page: &[u8],
    old_status: TransactionStatus,
    new_status: TransactionStatus,
) -> Result<(StatusHome, StatusHome)> {
    let native = &group[NATIVE];
    let slot = expected.location.slot();
    if native[0] != 5
        || native[1] != 3
        || u16_at(native, 2) != 0
        || u16_at(native, 4) != slot
        || u16_at(native, 6) != slot
        || u32_at(native, 12) != STATUS_HOME_BYTES as u32
        || u32_at(native, 16) != STATUS_HOME_BYTES as u32
        || u32_at(native, 20) != 576
        || !zero(&native[88..128])
        || content_digest(&native[128..352]) != native[24..56]
        || content_digest(&native[352..576]) != native[56..88]
    {
        return Err(SemanticError::Corrupt);
    }
    let old_home = StatusHome::decode(&native[128..352])?;
    let new_home = StatusHome::decode(&native[352..576])?;
    if old_home.core() != TransactionStatusCell::Occupied(old_status)
        || new_home.core() != TransactionStatusCell::Occupied(new_status)
        || old_home.incoming() != new_home.incoming()
    {
        return Err(SemanticError::Corrupt);
    }
    let redo = status_redo(header, group, expected)?;
    let namespace = expected.profile.spec().namespace;
    let old_page = decode_status_home_page(before_page, expected.page, namespace)?;
    let new_page = decode_status_home_page(after_page, expected.page, namespace)?;
    let actual_old = old_page.lookup(expected.actor, expected.location)?;
    let actual_new = new_page.lookup(expected.actor, expected.location)?;
    if old_page.lsn() > expected.frame.previous_lsn
        || new_page.lsn() != header.lsn
        || redo.prior_lsn != old_page.lsn()
    {
        return Err(SemanticError::OrderMismatch);
    }
    let range = old_page
        .cell_range(usize::from(slot))
        .ok_or(SemanticError::IdentityMismatch)?;
    if new_page.cell_range(usize::from(slot)) != Some(range.clone())
        || range.len() != STATUS_HOME_BYTES
        || usize::from(u16_at(native, 8)) != range.start
        || usize::from(u16_at(native, 10)) != range.start
        || usize::from(redo.offset) != range.start
        || redo.data != &native[352..576]
        || actual_old != old_home
        || actual_new != new_home
        || old_page.cell_bytes(usize::from(slot)) != Some(&native[128..352])
        || new_page.cell_bytes(usize::from(slot)) != Some(&native[352..576])
    {
        return Err(SemanticError::Corrupt);
    }
    if before_page[..64] != after_page[..64]
        || before_page[72..range.start] != after_page[72..range.start]
        || before_page[range.end..DIGEST_OFFSET] != after_page[range.end..DIGEST_OFFSET]
    {
        return Err(SemanticError::Corrupt);
    }
    Ok((old_home, new_home))
}

#[cfg(test)]
#[path = "semantic_group_tests.rs"]
mod tests;
