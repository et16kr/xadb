//! Empty-transaction Begin preparation, not identity issuance or live admission.
//! The future owner must prove the directory census, ephemeral slot claim and
//! complete termination reservation before staging. See obligation contract §16.

use super::{
    DESCRIPTOR, FRAME_BYTES, GROUP_BYTES, NATIVE, OPERATION, REDO, Result, RollbackStartExpected,
    SemanticError, group_owner, u16_at, u32_at, u64_at, validate_expected, zero,
};
use crate::storage::{
    allocation_page::ReservationOwner,
    control::{ControlRef, Profile},
    obligation::{
        DescriptorSpec, InventoryClaim, OwnerChain, OwnerState, RemainingBudget, ResourcePeak,
        decode_descriptor, encode_descriptor,
    },
    page::{DIGEST_OFFSET, PAGE_BYTES, PageIdentity, PageKind},
    status_home::{STATUS_HOME_BYTES, StatusHome, StatusHomePage, decode_status_home_page},
    transaction_record::{
        StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell, TxnRef,
    },
    wal_frame::{
        FrameEnvelope, FrameExpectation, FrameKind, RedoKind, RedoRecordView, RedoTarget,
        WalFrameHeader, decode_redo_record, decode_semantic_envelope, encode_patch_record,
        finish_semantic_envelope,
    },
};
use foundation::codec::content_digest;
use std::{mem::size_of, ops::Range};

/// Trusted expected context supplied by a separately serialized owner. Values
/// here do not certify global uniqueness, reclaim eligibility or reservations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BeginExpected {
    pub(crate) frame: FrameExpectation,
    pub(crate) profile: Profile,
    pub(crate) actor: TxnRef,
    pub(crate) location: StatusSlotRef,
    pub(crate) page: PageIdentity,
    pub(crate) previous_local: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BeginObservation {
    header: WalFrameHeader,
    after: DescriptorSpec,
    location: StatusSlotRef,
}
impl BeginObservation {
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

fn expected_profile(expected: BeginExpected) -> Result<ControlRef> {
    let profile = validate_expected(RollbackStartExpected {
        frame: expected.frame,
        profile: expected.profile,
        actor: expected.actor,
        location: expected.location,
        page: expected.page,
    })?;
    if expected.actor.id().writer_epoch != expected.frame.stream.writer_epoch
        || expected.previous_local.checked_add(1) != Some(expected.actor.id().local)
        || expected.frame.previous_lsn == 0
    {
        return Err(SemanticError::InvalidInput);
    }
    Ok(profile)
}

fn before<'a>(
    bytes: &'a [u8],
    expected: BeginExpected,
) -> Result<(StatusHomePage<'a>, Range<usize>)> {
    let page = decode_status_home_page(bytes, expected.page, expected.profile.spec().namespace)?;
    if page.lsn() == 0 || page.lsn() > expected.frame.previous_lsn {
        return Err(SemanticError::OrderMismatch);
    }
    let slot = usize::from(expected.location.slot());
    let home = page.cell(slot)?;
    let TransactionStatusCell::Free(generation) = home.core() else {
        return Err(SemanticError::UnsupportedOperation);
    };
    if generation.get().checked_add(1) != Some(expected.actor.generation().get()) {
        return Err(SemanticError::OrderMismatch);
    }
    for at in 0..page.len() {
        if let TransactionStatusCell::Occupied(status) = page.cell(at)?.core()
            && status.reference().id() == expected.actor.id()
        {
            return Err(SemanticError::IdentityMismatch);
        }
    }
    let range = page
        .cell_range(slot)
        .ok_or(SemanticError::IdentityMismatch)?;
    Ok((page, range))
}

/// One retained completion step of the exact empty lifecycle. Shared with the
/// terminal observer and the actual admission ledger; no resources are issued.
pub(crate) fn empty_step_budget() -> RemainingBudget {
    RemainingBudget {
        steps: 1,
        wal: FRAME_BYTES as u64,
        protected: FRAME_BYTES as u64,
        peak: ResourcePeak {
            frames: 2,
            memory: 114688,
            locks: 0,
            registry: 2,
            files: 5,
            doublewrite: 1,
        },
        ..RemainingBudget::default()
    }
}

fn initial_descriptor(
    expected: BeginExpected,
    profile: ControlRef,
    lsn: u64,
) -> Result<DescriptorSpec> {
    let status = TransactionStatus::new(expected.actor, TransactionState::Active, None)?;
    let mut remaining = [RemainingBudget::default(); 5];
    // A preparation plan for empty rollback/terminal only. The live consumer
    // must validate their actual F05 and prove that no handoff work is pending.
    let step = empty_step_budget();
    remaining[2] = step;
    remaining[3] = step;
    Ok(DescriptorSpec {
        owner: OwnerState::Transaction(status),
        chain: OwnerChain {
            apply: lsn,
            previous: 0,
            birth: lsn,
            groups: 1,
        },
        profile,
        inventory: InventoryClaim {
            change_lsn: lsn,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        },
        consumed_wal: FRAME_BYTES as u64,
        consumed_control: 0,
        remaining,
        minimum_wal_lsn: lsn,
        last_mutation_sequence: 0,
    })
}

fn envelope(frame: &[u8], expected: BeginExpected) -> Result<FrameEnvelope<'_>> {
    let value = decode_semantic_envelope(frame, expected.frame)?;
    if value.header.kind != FrameKind::Begin {
        return Err(SemanticError::UnsupportedOperation);
    }
    let group = value.payload;
    if group.len() < 8 || &group[..4] != b"XSG1" {
        return Err(SemanticError::Corrupt);
    }
    if u16_at(group, 4) != 1 {
        return Err(SemanticError::UnsupportedFormat);
    }
    if u16_at(group, 6) != 1 {
        return Err(SemanticError::UnsupportedOperation);
    }
    if frame.len() != FRAME_BYTES || group.len() != GROUP_BYTES || value.record_count != 1 {
        return Err(SemanticError::Corrupt);
    }
    if value.header.txn != Some(expected.actor.id()) {
        return Err(SemanticError::IdentityMismatch);
    }
    Ok(value)
}

fn redo<'a>(value: &FrameEnvelope<'a>, expected: BeginExpected) -> Result<RedoRecordView<'a>> {
    let record = decode_redo_record(value.header, &value.payload[REDO])?;
    if record.kind != RedoKind::Patch
        || record.target.kind != PageKind::TransactionStatus
        || record.target.level != 0
    {
        return Err(SemanticError::Corrupt);
    }
    if record.target.page != expected.page.page
        || record.target.owner != expected.page.owner
        || record.target.owner_generation != expected.page.owner_generation
    {
        return Err(SemanticError::IdentityMismatch);
    }
    Ok(record)
}

/// Check the full frame and both actual pages. Success is an immutable local
/// observation; global directory/admission/pin authority remains with the caller.
/// Inputs are unchanged on error. No I/O, allocation or lock is performed.
pub(crate) fn observe_begin(
    frame: &[u8],
    expected: BeginExpected,
    before_page: &[u8],
    after_page: &[u8],
) -> Result<BeginObservation> {
    let profile = expected_profile(expected)?;
    let value = envelope(frame, expected)?;
    let header = value.header;
    let group = value.payload;
    if u16_at(group, 8) != 192
        || !zero(&group[10..12])
        || u32_at(group, 12) != GROUP_BYTES as u32
        || u64_at(group, 16) != header.lsn
        || !zero(&group[24..32])
        || !zero(&group[88..120])
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
    if group_owner(&group[32..88])? != ReservationOwner::Transaction(expected.actor) {
        return Err(SemanticError::IdentityMismatch);
    }
    let after = decode_descriptor(
        &group[DESCRIPTOR],
        ReservationOwner::Transaction(expected.actor),
        profile,
    )?;
    if after != initial_descriptor(expected, profile, header.lsn)? {
        return Err(SemanticError::Corrupt);
    }
    let operation = &group[OPERATION];
    if u16_at(operation, 0) != 1
        || !zero(&operation[2..4])
        || u32_at(operation, 4) != 256
        || !zero(&operation[8..])
    {
        return Err(SemanticError::Corrupt);
    }
    let (old, range) = before(before_page, expected)?;
    let new =
        decode_status_home_page(after_page, expected.page, expected.profile.spec().namespace)?;
    let native = &group[NATIVE];
    let slot = expected.location.slot();
    if native[0] != 5
        || native[1] != 3
        || u16_at(native, 2) != 0
        || u16_at(native, 4) != slot
        || u16_at(native, 6) != slot
        || usize::from(u16_at(native, 8)) != range.start
        || usize::from(u16_at(native, 10)) != range.start
        || u32_at(native, 12) != STATUS_HOME_BYTES as u32
        || u32_at(native, 16) != STATUS_HOME_BYTES as u32
        || u32_at(native, 20) != 576
        || !zero(&native[88..128])
        || native[24..56] != content_digest(&native[128..352])
        || native[56..88] != content_digest(&native[352..576])
        || before_page[range.clone()] != native[128..352]
        || new.cell_range(usize::from(slot)) != Some(range.clone())
        || after_page[range.clone()] != native[352..576]
    {
        return Err(SemanticError::Corrupt);
    }
    let home = new.lookup(expected.actor, expected.location)?;
    if Some(home.core())
        != match after.owner {
            OwnerState::Transaction(status) => Some(TransactionStatusCell::Occupied(status)),
            _ => None,
        }
        || home.incoming() != 0
    {
        return Err(SemanticError::Corrupt);
    }
    let record = redo(&value, expected)?;
    if record.prior_lsn != old.lsn() || new.lsn() != header.lsn {
        return Err(SemanticError::OrderMismatch);
    }
    if usize::from(record.offset) != range.start
        || record.data != &native[352..576]
        || before_page[..64] != after_page[..64]
        || before_page[72..range.start] != after_page[72..range.start]
        || before_page[range.end..DIGEST_OFFSET] != after_page[range.end..DIGEST_OFFSET]
    {
        return Err(SemanticError::Corrupt);
    }
    Ok(BeginObservation {
        header,
        after,
        location: expected.location,
    })
}

/// Prepare one unpublished after image. Caller must discard output on error;
/// immutable inputs survive. This does not claim or install the selected slot.
pub(crate) fn materialize_begin(
    frame: &[u8],
    expected: BeginExpected,
    before_page: &[u8],
    output: &mut [u8; PAGE_BYTES],
) -> Result<BeginObservation> {
    expected_profile(expected)?;
    let value = envelope(frame, expected)?;
    let (old, range) = before(before_page, expected)?;
    let record = redo(&value, expected)?;
    if record.prior_lsn != old.lsn() {
        return Err(SemanticError::OrderMismatch);
    }
    if usize::from(record.offset) != range.start || record.data.len() != range.len() {
        return Err(SemanticError::Corrupt);
    }
    output.copy_from_slice(before_page);
    output[range].copy_from_slice(record.data);
    output[64..72].copy_from_slice(&value.header.lsn.to_le_bytes());
    let digest = content_digest(&output[..DIGEST_OFFSET]);
    output[DIGEST_OFFSET..].copy_from_slice(&digest);
    observe_begin(frame, expected, before_page, output)
}

/// Prepare the canonical empty Begin frame and after page in caller buffers.
/// This issues neither the candidate TxnId nor a stage/apply capability. Discard
/// both outputs on error. Before bytes are preserved; no I/O or heap is used.
pub(crate) fn produce_begin(
    expected: BeginExpected,
    lsn: u64,
    before_page: &[u8],
    frame_output: &mut [u8; FRAME_BYTES],
    page_output: &mut [u8; PAGE_BYTES],
) -> Result<BeginObservation> {
    let profile = expected_profile(expected)?;
    if lsn == 0 || lsn <= expected.frame.previous_lsn {
        return Err(SemanticError::InvalidInput);
    }
    build(expected, profile, lsn, before_page, frame_output)?;
    materialize_begin(frame_output, expected, before_page, page_output)
}

fn build(
    expected: BeginExpected,
    profile: ControlRef,
    lsn: u64,
    before_page: &[u8],
    output: &mut [u8; FRAME_BYTES],
) -> Result<()> {
    let (old, range) = before(before_page, expected)?;
    let after = initial_descriptor(expected, profile, lsn)?;
    let OwnerState::Transaction(status) = after.owner else {
        return Err(SemanticError::Corrupt);
    };
    output.fill(0);
    let group = &mut output[160..FRAME_BYTES - 32];
    group[..4].copy_from_slice(b"XSG1");
    for (at, value) in [(4, 1_u16), (6, 1), (8, 192), (124, 1), (132, 1)] {
        group[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    for (at, value) in [
        (12, GROUP_BYTES as u32),
        (120, 1536),
        (136, 576),
        (152, 288),
    ] {
        group[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    group[16..24].copy_from_slice(&lsn.to_le_bytes());
    group[32] = 1;
    expected.actor.encode(&mut group[40..88])?;
    encode_descriptor(&after, &mut group[DESCRIPTOR])?;
    group[OPERATION.start..OPERATION.start + 2].copy_from_slice(&1_u16.to_le_bytes());
    group[OPERATION.start + 4..OPERATION.start + 8].copy_from_slice(&256_u32.to_le_bytes());
    let native = &mut group[NATIVE];
    native[0] = 5;
    native[1] = 3;
    for at in [4, 6] {
        native[at..at + 2].copy_from_slice(&expected.location.slot().to_le_bytes());
    }
    for at in [8, 10] {
        native[at..at + 2].copy_from_slice(&(range.start as u16).to_le_bytes());
    }
    for at in [12, 16] {
        native[at..at + 4].copy_from_slice(&(STATUS_HOME_BYTES as u32).to_le_bytes());
    }
    native[20..24].copy_from_slice(&576_u32.to_le_bytes());
    native[128..352].copy_from_slice(&before_page[range.clone()]);
    StatusHome::new(TransactionStatusCell::Occupied(status), 0)?.encode(&mut native[352..576])?;
    let old_digest = content_digest(&native[128..352]);
    let new_digest = content_digest(&native[352..576]);
    native[24..56].copy_from_slice(&old_digest);
    native[56..88].copy_from_slice(&new_digest);
    let (prepared, record) = group.split_at_mut(REDO.start);
    encode_patch_record(
        RedoTarget {
            page: expected.page.page,
            owner: expected.page.owner,
            owner_generation: expected.page.owner_generation,
            kind: PageKind::TransactionStatus,
            level: 0,
        },
        old.lsn(),
        range.start as u16,
        &prepared[NATIVE.start + 352..NATIVE.end],
        record,
    )?;
    finish_semantic_envelope(
        WalFrameHeader {
            stream: expected.frame.stream,
            kind: FrameKind::Begin,
            lsn,
            previous_lsn: expected.frame.previous_lsn,
            txn: Some(expected.actor.id()),
            previous_txn_lsn: 0,
        },
        1,
        output,
    )?;
    Ok(())
}

/// Conservative simultaneous source-value census. Includes the nested observer,
/// descriptor validation/encoding and SHA helpers, never a hidden page copy.
pub(crate) const fn component_bytes() -> usize {
    4 * size_of::<BeginExpected>()
        + 4 * size_of::<BeginObservation>()
        + 4 * size_of::<DescriptorSpec>()
        + 8 * size_of::<StatusHome>()
        + 4 * size_of::<TransactionStatusCell>()
        + 4 * size_of::<StatusHomePage<'static>>()
        + 4 * size_of::<RedoRecordView<'static>>()
        + 4 * size_of::<WalFrameHeader>()
        + 8 * size_of::<Result<BeginObservation>>()
        + 6144
        + 4096
}
const _: () = assert!(component_bytes() <= 65536);

#[cfg(test)]
#[path = "semantic_begin_tests.rs"]
mod tests;
