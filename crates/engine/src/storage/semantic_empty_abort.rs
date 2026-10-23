//! Empty AbortEnd bytes for the Begin → full RollbackStart path. This does not
//! prove live queue/retirement closure or authorize releasing a reservation.

use super::{
    DESCRIPTOR, FRAME_BYTES, GROUP_BYTES, NATIVE, OPERATION, REDO, Result, RollbackStartExpected,
    SemanticError, observe_status_patch, status_redo, u16_at, u32_at, validate_expected,
    validate_group_header, zero,
};
use crate::storage::{
    allocation_page::ReservationOwner,
    obligation::{
        DescriptorSpec, OwnerState, RemainingBudget, decode_descriptor, encode_descriptor,
    },
    page::{DIGEST_OFFSET, PAGE_BYTES},
    status_home::{STATUS_HOME_BYTES, StatusHome, StatusHomePage, decode_status_home_page},
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell,
    },
    wal_frame::{
        FrameEnvelope, FrameKind, RedoRecordView, RedoTarget, WalFrameHeader,
        decode_semantic_envelope, encode_patch_record, finish_semantic_envelope,
    },
};
use std::mem::size_of;
use xadb_foundation::codec::content_digest;

/// The trusted namespace, owner and location context is the same as for rollback.
pub(crate) type EmptyAbortExpected = RollbackStartExpected;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmptyAbortObservation {
    header: WalFrameHeader,
    after: DescriptorSpec,
    location: StatusSlotRef,
}
impl EmptyAbortObservation {
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

/// Only the exact unused Begin reservation can take this terminal shortcut.
fn empty_before(expected: EmptyAbortExpected, bytes: &[u8]) -> Result<DescriptorSpec> {
    let profile = validate_expected(expected)?;
    let before = decode_descriptor(
        bytes,
        ReservationOwner::Transaction(expected.actor),
        profile,
    )?;
    let OwnerState::Transaction(status) = before.owner else {
        return Err(SemanticError::UnsupportedOperation);
    };
    if status.state() != TransactionState::Aborting(RollbackState::Transaction { next: None })
        || status.undo_head().is_some()
        || before.inventory.items != 0
        || before.inventory.growth_extents != 0
        || before.inventory.digest != content_digest(&[])
        || before.last_mutation_sequence != 0
    {
        return Err(SemanticError::UnsupportedOperation);
    }
    if before.chain.groups != 2
        || before.chain.birth == 0
        || before.chain.previous != before.chain.birth
        || before.chain.apply <= before.chain.birth
        || before.chain.apply > expected.frame.previous_lsn
        || before.inventory.change_lsn != before.chain.birth
        || before.minimum_wal_lsn != before.chain.birth
    {
        return Err(SemanticError::OrderMismatch);
    }
    let mut remaining = [RemainingBudget::default(); 5];
    remaining[2] = super::begin::empty_step_budget();
    if before.remaining != remaining
        || before.consumed_wal != 2 * FRAME_BYTES as u64
        || before.consumed_control != 0
    {
        return Err(SemanticError::BudgetMismatch);
    }
    Ok(before)
}

fn empty_page<'a>(
    expected: EmptyAbortExpected,
    before: &DescriptorSpec,
    bytes: &'a [u8],
) -> Result<StatusHomePage<'a>> {
    let page = decode_status_home_page(bytes, expected.page, expected.profile.spec().namespace)?;
    let home = page.lookup(expected.actor, expected.location)?;
    if page.lsn() < before.chain.apply || page.lsn() > expected.frame.previous_lsn {
        return Err(SemanticError::OrderMismatch);
    }
    let OwnerState::Transaction(status) = before.owner else {
        return Err(SemanticError::UnsupportedOperation);
    };
    if home.core() != TransactionStatusCell::Occupied(status) {
        return Err(SemanticError::Corrupt);
    }
    if home.incoming() != 0 {
        return Err(SemanticError::UnsupportedOperation);
    }
    Ok(page)
}

fn after_descriptor(
    before: DescriptorSpec,
    actor: crate::storage::transaction_record::TxnRef,
    lsn: u64,
) -> Result<DescriptorSpec> {
    let mut after = before;
    after.owner = OwnerState::Transaction(TransactionStatus::new(
        actor,
        TransactionState::Aborted,
        None,
    )?);
    after.chain.apply = lsn;
    after.chain.previous = before.chain.apply;
    after.chain.groups = 3;
    after.consumed_wal = 3 * FRAME_BYTES as u64;
    after.remaining = [RemainingBudget::default(); 5];
    Ok(after)
}

fn envelope(frame: &[u8], expected: EmptyAbortExpected) -> Result<FrameEnvelope<'_>> {
    let value = decode_semantic_envelope(frame, expected.frame)?;
    if value.header.kind != FrameKind::AbortEnd {
        return Err(SemanticError::UnsupportedOperation);
    }
    let group = value.payload;
    if group.len() < 8 || &group[..4] != b"XSG1" {
        return Err(SemanticError::Corrupt);
    }
    if u16_at(group, 4) != 1 {
        return Err(SemanticError::UnsupportedFormat);
    }
    if u16_at(group, 6) != 7 {
        return Err(SemanticError::UnsupportedOperation);
    }
    if frame.len() != FRAME_BYTES || group.len() != GROUP_BYTES || value.record_count != 1 {
        return Err(SemanticError::Corrupt);
    }
    Ok(value)
}

/// Validate the full frame and actual before/after pages without live effects.
/// Empty native state does not certify absence of external retained obligations.
pub(crate) fn observe_empty_abort(
    frame: &[u8],
    expected: EmptyAbortExpected,
    before_descriptor: &[u8],
    before_page: &[u8],
    after_page: &[u8],
) -> Result<EmptyAbortObservation> {
    let before = empty_before(expected, before_descriptor)?;
    empty_page(expected, &before, before_page)?;
    let value = envelope(frame, expected)?;
    let group = value.payload;
    validate_group_header(
        group,
        value.header,
        expected.actor,
        &before,
        before_descriptor,
    )?;
    let after = decode_descriptor(
        &group[DESCRIPTOR],
        ReservationOwner::Transaction(expected.actor),
        before.profile,
    )?;
    if after != after_descriptor(before, expected.actor, value.header.lsn)? {
        return Err(SemanticError::Corrupt);
    }
    let operation = &group[OPERATION];
    if u16_at(operation, 0) != 7
        || !zero(&operation[2..4])
        || u32_at(operation, 4) != 256
        || !zero(&operation[8..])
    {
        return Err(SemanticError::Corrupt);
    }
    let OwnerState::Transaction(old_status) = before.owner else {
        return Err(SemanticError::Corrupt);
    };
    let OwnerState::Transaction(new_status) = after.owner else {
        return Err(SemanticError::Corrupt);
    };
    observe_status_patch(
        value.header,
        group,
        expected,
        before_page,
        after_page,
        old_status,
        new_status,
    )?;
    Ok(EmptyAbortObservation {
        header: value.header,
        after,
        location: expected.location,
    })
}

/// Patch the caller's unpublished output, then perform the full observer check.
/// On error discard output; immutable input bytes are preserved.
pub(crate) fn materialize_empty_abort(
    frame: &[u8],
    expected: EmptyAbortExpected,
    before_descriptor: &[u8],
    before_page: &[u8],
    output: &mut [u8; PAGE_BYTES],
) -> Result<EmptyAbortObservation> {
    let before = empty_before(expected, before_descriptor)?;
    let old = empty_page(expected, &before, before_page)?;
    let value = envelope(frame, expected)?;
    let redo = status_redo(value.header, value.payload, expected)?;
    let range = old
        .cell_range(usize::from(expected.location.slot()))
        .ok_or(SemanticError::IdentityMismatch)?;
    if redo.prior_lsn != old.lsn() {
        return Err(SemanticError::OrderMismatch);
    }
    if usize::from(redo.offset) != range.start || redo.data.len() != STATUS_HOME_BYTES {
        return Err(SemanticError::Corrupt);
    }
    output.copy_from_slice(before_page);
    output[range].copy_from_slice(redo.data);
    output[64..72].copy_from_slice(&value.header.lsn.to_le_bytes());
    let digest = content_digest(&output[..DIGEST_OFFSET]);
    output[DIGEST_OFFSET..].copy_from_slice(&digest);
    observe_empty_abort(frame, expected, before_descriptor, before_page, output)
}

/// Produce an unpublished exact empty AbortEnd and page. No append, sync, live
/// install or reservation release occurs. Discard both outputs on any error.
pub(crate) fn produce_empty_abort(
    expected: EmptyAbortExpected,
    lsn: u64,
    before_descriptor: &[u8],
    before_page: &[u8],
    frame_output: &mut [u8; FRAME_BYTES],
    page_output: &mut [u8; PAGE_BYTES],
) -> Result<EmptyAbortObservation> {
    validate_expected(expected)?;
    if lsn == 0 || lsn <= expected.frame.previous_lsn {
        return Err(SemanticError::InvalidInput);
    }
    build_frame(expected, lsn, before_descriptor, before_page, frame_output)?;
    materialize_empty_abort(
        frame_output,
        expected,
        before_descriptor,
        before_page,
        page_output,
    )
}

fn build_frame(
    expected: EmptyAbortExpected,
    lsn: u64,
    before_descriptor: &[u8],
    before_page: &[u8],
    output: &mut [u8; FRAME_BYTES],
) -> Result<()> {
    let before = empty_before(expected, before_descriptor)?;
    let old = empty_page(expected, &before, before_page)?;
    let range = old
        .cell_range(usize::from(expected.location.slot()))
        .ok_or(SemanticError::IdentityMismatch)?;
    let after = after_descriptor(before, expected.actor, lsn)?;
    let OwnerState::Transaction(status) = after.owner else {
        return Err(SemanticError::Corrupt);
    };
    output.fill(0);
    let group = &mut output[160..FRAME_BYTES - 32];
    group[..4].copy_from_slice(b"XSG1");
    group[4..6].copy_from_slice(&1_u16.to_le_bytes());
    group[6..8].copy_from_slice(&7_u16.to_le_bytes());
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
    group[OPERATION.start..OPERATION.start + 2].copy_from_slice(&7_u16.to_le_bytes());
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
            kind: crate::storage::page::PageKind::TransactionStatus,
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
            kind: FrameKind::AbortEnd,
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

/// Source-value bound, including nested typed observations and encoder/hash
/// scratch. Caller frame/page loans are separate; this is not a stack/RSS bound.
pub(crate) const fn component_bytes() -> usize {
    5 * size_of::<EmptyAbortExpected>()
        + 5 * size_of::<EmptyAbortObservation>()
        + 7 * size_of::<DescriptorSpec>()
        + 10 * size_of::<StatusHome>()
        + 6 * size_of::<TransactionStatusCell>()
        + 6 * size_of::<StatusHomePage<'static>>()
        + 4 * size_of::<RedoRecordView<'static>>()
        + 4 * size_of::<WalFrameHeader>()
        + 8 * size_of::<Result<EmptyAbortObservation>>()
        + 4 * size_of::<FrameEnvelope<'static>>()
        + 4 * size_of::<Result<DescriptorSpec>>()
        + 4 * size_of::<Result<(StatusHome, StatusHome)>>()
        + 6144
        + 4096
}
const _: () = assert!(component_bytes() <= 65536);

#[cfg(test)]
#[path = "semantic_empty_abort_tests.rs"]
mod tests;
