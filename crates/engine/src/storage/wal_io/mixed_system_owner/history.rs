//! Exact plan3 system-only history for a selected immutable payload image.
//! The caller lends the original locked WAL handle and operation scratch. This
//! module neither opens a source nor issues a load, flush or recovery permit.

use super::super::super::{
    super::super::allocation_reservation::{budget, operation},
    operation as init,
};
use crate::storage::{
    allocation_page::{
        self, AllocationEntry, AllocationPage, ReservationOwner, SystemWorkKind, SystemWorkRef,
    },
    control::Profile,
    dirty_frame::TargetRequest,
    page::{PAGE_BYTES, PageIdentity, PageLsn, PageRef},
    wal_frame::{self, FrameEnvelope, FrameExpectation, FrameKind, WalFrameHeader},
    wal_io::{
        self as wal, WalPrefix,
        bootstrap::{BootstrapPlan, BootstrapPlanKind, FixedSystemRole},
    },
};
use foundation::{
    Error as IoFailure,
    codec::content_digest,
    identity::Generation,
    io::{FileIo, read_exact_at},
    namespace::WriterNamespace,
};
use operation::{AllocationPopulation, AnalysisExpected, Batch, OperationBudget, TerminalOutcome};
use std::mem::size_of;

const FRAME_SPACE: usize = 65536;
const MAX_FRAME: usize = 38208;
const DESCRIPTOR_BYTES: usize = 1536;
const WORKSPACE_BYTES: usize = FRAME_SPACE + 2 * PAGE_BYTES;

#[derive(Debug)]
pub(crate) enum HistoryError {
    Invalid,
    Read(IoFailure),
}
type Result<T> = std::result::Result<T, HistoryError>;

/// Only a complete canonical replay creates this observation. It has no raw
/// constructor or public fields and grants no page-installation authority.
pub(super) struct History {
    lsn: PageLsn,
    digest: [u8; 32],
    allocator_digest: [u8; 32],
    high_water: u64,
    terminal: TerminalOutcome,
}
impl History {
    pub(super) fn lsn(&self) -> PageLsn {
        self.lsn
    }
    pub(super) fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub(super) fn allocator_digest(&self) -> &[u8; 32] {
        &self.allocator_digest
    }
    pub(super) fn high_water(&self) -> u64 {
        self.high_water
    }
    pub(super) fn terminal(&self) -> TerminalOutcome {
        self.terminal
    }
}

fn invalid<T>(_: T) -> HistoryError {
    HistoryError::Invalid
}
fn read_error(source: IoFailure) -> HistoryError {
    // EOF inside a retained complete prefix and invalid adapter counts cannot
    // be retried as an ordinary unchanged read. Preserve every other original
    // cause/effect/certainty/category for the owning caller's decision.
    if source.io_kind.is_none() || source.io_kind == Some(std::io::ErrorKind::UnexpectedEof) {
        HistoryError::Invalid
    } else {
        HistoryError::Read(source)
    }
}

/// Reconstruct every system operation from the exact plan3 genesis. The caller
/// supplies its authentic selected role, frozen complete prefix, allocator
/// snapshot digest and retained descriptor, then rechecks source identity,
/// allocator snapshot, prefix and the final disk image before installation.
///
/// The saved selected page and previous descriptor occupy unused frame space;
/// two allocator images and payload_output use the original 114688-byte loan.
/// Failure discards scratch. Success also leaves the selected canonical INIT
/// image in payload_output, but neither those bytes nor History are a permit.
#[allow(
    clippy::too_many_arguments,
    reason = "independent original role, prefix, snapshot and scratch witnesses"
)]
pub(super) fn reconstruct<F: FileIo>(
    file: &mut F,
    workspace: &mut [u8],
    payload_output: &mut [u8; PAGE_BYTES],
    role: &FixedSystemRole<'_>,
    profile: Profile,
    allocator: PageIdentity,
    allocator_digest: [u8; 32],
    expected_descriptor: &[u8; DESCRIPTOR_BYTES],
    prefix: WalPrefix,
    request: TargetRequest,
) -> Result<History> {
    if workspace.len() < WORKSPACE_BYTES
        || !role.mixed()
        || role.namespace() != profile.spec().namespace
        || prefix.stream.database != role.namespace().database
        || prefix.stream.timeline != role.namespace().timeline
        || prefix.stream.writer_epoch != 1
        || prefix.complete_lsn == 0
        || prefix.complete_lsn != prefix.synced_lsn
        || prefix.complete_offset != prefix.synced_offset
        || prefix.complete_offset <= wal::CONTROL_BYTES as u64
        || prefix.complete_offset > wal::WAL_FILE_CAP
    {
        return Err(HistoryError::Invalid);
    }
    let selected_kind = match (request.identity.owner.get(), request.kind) {
        (2, crate::storage::page::PageKind::TransactionStatus) => init::mixed::Kind::Status,
        (3, crate::storage::page::PageKind::Undo) => init::mixed::Kind::UndoRoot,
        _ => return Err(HistoryError::Invalid),
    };
    if init::fixed::request(role, request.identity.page, selected_kind).map_err(invalid)? != request
    {
        return Err(HistoryError::Invalid);
    }
    read_exact_at(file, &mut workspace[..wal::CONTROL_BYTES], 0).map_err(read_error)?;
    wal::decode_epoch(&workspace[..wal::CONTROL_BYTES], profile).map_err(invalid)?;
    {
        let scratch: &mut [[u8; PAGE_BYTES]; 3] = workspace[..3 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("bootstrap page loans");
        let genesis =
            BootstrapPlan::prepare_for_kind(profile, scratch, BootstrapPlanKind::UndoBootstrap)
                .map_err(invalid)?;
        if genesis.allocation_identity().map_err(invalid)? != allocator {
            return Err(HistoryError::Invalid);
        }
    }
    workspace.copy_within(..PAGE_BYTES, FRAME_SPACE);
    let (frame_space, allocator_space) = workspace.split_at_mut(FRAME_SPACE);
    let (frame, rest) = frame_space.split_at_mut(MAX_FRAME);
    let (descriptor, unused) = rest.split_at_mut(DESCRIPTOR_BYTES);
    let descriptor: &mut [u8; DESCRIPTOR_BYTES] = descriptor.try_into().expect("descriptor loan");
    let selected_init: &mut [u8; PAGE_BYTES] = (&mut unused[..PAGE_BYTES])
        .try_into()
        .expect("selected INIT loan");
    descriptor.fill(0);
    let (before, rest) = allocator_space.split_at_mut(PAGE_BYTES);
    let before: &mut [u8; PAGE_BYTES] = before.try_into().expect("allocator before loan");
    let after: &mut [u8; PAGE_BYTES] = (&mut rest[..PAGE_BYTES])
        .try_into()
        .expect("allocator after loan");
    let mut selected_lsn = None;
    let mut offset = wal::CONTROL_BYTES as u64;
    let mut previous = 0u64;
    let mut active: Option<Batch> = None;
    let mut high_water = 0u64;
    let mut terminal = TerminalOutcome::Cancelled;
    while offset < prefix.complete_offset {
        if prefix.complete_offset - offset < 160 {
            return Err(HistoryError::Invalid);
        }
        let expected = FrameExpectation {
            stream: prefix.stream,
            previous_lsn: previous,
        };
        read_exact_at(file, &mut frame[..160], offset).map_err(read_error)?;
        let length = wal_frame::semantic_frame_length(&frame[..160], expected).map_err(invalid)?;
        if !(2144..=MAX_FRAME).contains(&length) || length as u64 > prefix.complete_offset - offset
        {
            return Err(HistoryError::Invalid);
        }
        read_exact_at(file, &mut frame[160..length], offset + 160).map_err(read_error)?;
        let bytes = &frame[..length];
        let envelope = wal_frame::decode_semantic_envelope(bytes, expected).map_err(invalid)?;
        let header = envelope.header;
        if header.kind != FrameKind::System
            || header.lsn != previous.checked_add(1).ok_or(HistoryError::Invalid)?
            || &bytes[160..164] != b"XSG1"
        {
            return Err(HistoryError::Invalid);
        }
        let begin = match u16::from_le_bytes(bytes[166..168].try_into().expect("opcode")) {
            8 => true,
            10 => false,
            _ => return Err(HistoryError::Invalid),
        };
        let allocation =
            allocation_page::decode_allocation_page(before, allocator, profile.spec().namespace)
                .map_err(invalid)?;
        operation::validate_live_in_population(
            &allocation,
            profile,
            active,
            descriptor,
            high_water,
            OperationBudget::MixedInit114688,
            AllocationPopulation::MixedSystem,
            terminal,
        )
        .map_err(invalid)?;
        let batch = if begin {
            if active.is_some()
                || u16::from_le_bytes(bytes[290..292].try_into().expect("count")) != 1
            {
                return Err(HistoryError::Invalid);
            }
            let mut first = None;
            for index in 1..64 {
                if let AllocationEntry::Free(generation) =
                    allocation.entry(index).map_err(invalid)?
                    && generation.get() != u64::MAX
                {
                    first = Some(index);
                    break;
                }
            }
            Batch {
                owner: SystemWorkRef::new(
                    profile.spec().namespace,
                    SystemWorkKind::AllocationMaintenance,
                    high_water.checked_add(1).ok_or(HistoryError::Invalid)?,
                    Generation::new(1).expect("fixed generation"),
                )
                .map_err(invalid)?,
                mask: 1u64 << first.ok_or(HistoryError::Invalid)?,
                birth: header.lsn,
            }
        } else {
            active.ok_or(HistoryError::Invalid)?
        };
        // Only the original plan3 role admits these two candidate geometries.
        // Full canonical materialization below must validate the selected class;
        // a length or raw native tag never supplies installation authority.
        let init_kind = if begin {
            None
        } else {
            match length {
                38000 => Some(init::mixed::Kind::Status),
                MAX_FRAME => Some(init::mixed::Kind::UndoRoot),
                _ => None,
            }
        };
        if let Some(kind) = init_kind {
            let index = batch.mask.trailing_zeros() as usize;
            let AllocationEntry::Reserved {
                generation,
                claimant,
            } = allocation.entry(index).map_err(invalid)?
            else {
                return Err(HistoryError::Invalid);
            };
            if claimant != ReservationOwner::System(batch.owner) {
                return Err(HistoryError::Invalid);
            }
            let payload_request = init::fixed::request(
                role,
                PageRef {
                    page_no: index as u64,
                    incarnation: generation,
                    ..allocator.page
                },
                kind,
            )
            .map_err(invalid)?;
            let plan = init::mixed::Expected::new(
                profile,
                allocator,
                payload_request.identity,
                expected,
                batch,
                descriptor,
                kind,
            );
            init::mixed::materialize(&plan, before, bytes, after, payload_output)
                .map_err(invalid)?;
            if payload_request == request {
                if selected_lsn.is_some() {
                    return Err(HistoryError::Invalid);
                }
                selected_lsn = Some(PageLsn(header.lsn));
                selected_init.copy_from_slice(payload_output);
            }
            terminal = kind.terminal();
        } else {
            operation::materialize_analysis_in_population(
                AnalysisExpected {
                    profile,
                    identity: allocator,
                    frame: expected,
                    batch,
                    cancel: !begin,
                    before_descriptor: (!begin).then_some(&*descriptor),
                },
                before,
                bytes,
                after,
                OperationBudget::MixedInit114688,
                AllocationPopulation::MixedSystem,
            )
            .map_err(invalid)?;
            terminal = TerminalOutcome::Cancelled;
        }
        before.copy_from_slice(after);
        descriptor.copy_from_slice(&bytes[352..1888]);
        if begin {
            active = Some(batch);
            high_water = batch.owner.id();
        } else {
            active = None;
        }
        offset += length as u64;
        previous = header.lsn;
    }
    let allocation =
        allocation_page::decode_allocation_page(before, allocator, profile.spec().namespace)
            .map_err(invalid)?;
    operation::validate_live_in_population(
        &allocation,
        profile,
        active,
        descriptor,
        high_water,
        OperationBudget::MixedInit114688,
        AllocationPopulation::MixedSystem,
        terminal,
    )
    .map_err(invalid)?;
    let reconstructed_digest = content_digest(before);
    if active.is_some()
        || previous != prefix.complete_lsn
        || offset != prefix.complete_offset
        || descriptor != expected_descriptor
        || reconstructed_digest != allocator_digest
    {
        return Err(HistoryError::Invalid);
    }
    let lsn = selected_lsn.ok_or(HistoryError::Invalid)?;
    // Membership is checked against the final canonical allocator, including
    // the full mixed census, not merely the selected page's historical INIT.
    init::fixed::membership(
        &profile,
        &allocator,
        &request.identity,
        role,
        before,
        AllocationPopulation::MixedSystem,
        selected_kind,
    )
    .map_err(invalid)?;
    payload_output.copy_from_slice(selected_init);
    Ok(History {
        lsn,
        digest: content_digest(selected_init),
        allocator_digest: reconstructed_digest,
        high_water,
        terminal,
    })
}

/// Source-owned values only, not physical stack/RSS. The caller separately
/// charges its retained owner, lock wrapper and the original 114688-byte loans.
/// Header validation, genesis preparation and replay are sequential phases;
/// their scratch pages are reused, so their nested contributions form a max.
pub(super) fn call_bytes<W: WriterNamespace>() -> usize {
    let retained = 4 * size_of::<History>()
        + 4 * size_of::<Result<History>>()
        + 4 * size_of::<HistoryError>()
        + 4 * size_of::<WalPrefix>()
        + 4 * size_of::<Profile>()
        + 4 * size_of::<TargetRequest>()
        + 4 * size_of::<PageIdentity>()
        + 4 * size_of::<FrameExpectation>()
        + 4 * size_of::<WalFrameHeader>()
        + 2 * size_of::<FrameEnvelope<'static>>()
        + 4 * size_of::<AllocationPage<'static>>()
        + 4 * size_of::<Option<Batch>>()
        + 4 * size_of::<W::File>()
        + 8192; // digest/scalar/reference/iterator/read-result and mapping carriers
    let system = init::plan_bytes() + init::temporary_bytes();
    let bootstrap = wal::bootstrap::recovery_preparation_bytes::<W>();
    retained + system.max(bootstrap).max(budget::HASH + wal::CONTROL_BYTES)
}
