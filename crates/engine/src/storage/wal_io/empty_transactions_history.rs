//! Exact selected-page history from the original live owner's complete WAL.
//! This is not a recovery analyzer: the owner has already frozen provisioning
//! and checked its whole live directory. Unselected transaction semantics are
//! not adopted here, and these observations cannot recreate an owner or READY.
use super::super::super::super::super::{
    super::super::allocation_reservation::operation, operation as init,
};
use super::*;
use crate::storage::allocation_page::{SystemWorkKind, SystemWorkRef};
use crate::storage::wal_frame::{self, FrameKind, RedoTarget};
use crate::storage::wal_io::bootstrap::{BootstrapPlan, BootstrapPlanKind};
use operation::{AllocationPopulation, AnalysisExpected, Batch, OperationBudget, TerminalOutcome};
use xadb_foundation::io::{FileIo, read_exact_at};

const MAX_FRAME: usize = 38000;
const DESCRIPTOR_START: usize = MAX_FRAME;

/// Only reconstruction below creates this value. The digests include the exact
/// INIT and every accepted transition, in order; unused entries are excluded.
pub(in super::super::super::super) struct History {
    digests: [[u8; 32]; 4],
    count: usize,
}
impl History {
    pub(in super::super::super::super) fn current(&self) -> &[u8; 32] {
        &self.digests[self.count - 1]
    }
    pub(in super::super::super::super) fn contains(&self, bytes: &[u8]) -> bool {
        self.digests[..self.count].contains(&content_digest(bytes))
    }
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        if self.count == self.digests.len() {
            return Err(unchanged(Cause::Invariant));
        }
        self.digests[self.count] = content_digest(bytes);
        self.count += 1;
        Ok(())
    }
}
fn target(request: TargetRequest) -> RedoTarget {
    RedoTarget {
        page: request.identity.page,
        owner: request.identity.owner,
        owner_generation: request.identity.owner_generation,
        kind: request.kind,
        level: request.level,
    }
}
fn invalid() -> EmptyTransactionError {
    unchanged(Cause::Invariant)
}

/// Scratch is the original operation loan; the previous descriptor occupies
/// unused frame capacity and survives reads of every supported complete frame.
/// Errors discard scratch and never return a partially accepted History.
#[allow(clippy::too_many_arguments)] // Separate immutable census witnesses and caller-owned scratch loans.
pub(super) fn reconstruct<F: FileIo>(
    file: &mut F,
    workspace: &mut [u8],
    status_output: &mut [u8; PAGE_BYTES],
    profile: Profile,
    allocator: PageIdentity,
    allocator_digest: [u8; 32],
    expected_descriptor: Option<&[u8]>,
    prefix: WalPrefix,
    frozen: PageLsn,
    request: TargetRequest,
    current_digest: [u8; 32],
) -> Result<History> {
    if workspace.len() < 98304
        || frozen.0 == 0
        || frozen.0 > prefix.complete_lsn
        || prefix.complete_lsn != prefix.synced_lsn
        || prefix.complete_offset != prefix.synced_offset
        || prefix.complete_offset < 4096
    {
        return Err(invalid());
    }
    {
        let scratch: &mut [[u8; PAGE_BYTES]; 3] = workspace[..3 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("bootstrap loans");
        BootstrapPlan::prepare_for_kind(profile, scratch, BootstrapPlanKind::StatusBootstrap)
            .map_err(|_| invalid())?;
    }
    workspace.copy_within(..PAGE_BYTES, 65536);
    let (frame_space, rest) = workspace.split_at_mut(65536);
    let (frame, rest_descriptor) = frame_space.split_at_mut(DESCRIPTOR_START);
    let (descriptor, unused) = rest_descriptor.split_at_mut(1536);
    let selected_init: &mut [u8; PAGE_BYTES] = (&mut unused[..PAGE_BYTES])
        .try_into()
        .expect("selected INIT loan");
    descriptor.fill(0);
    let (before, rest) = rest.split_at_mut(PAGE_BYTES);
    let before: &mut [u8; PAGE_BYTES] = before.try_into().expect("history before loan");
    let after: &mut [u8; PAGE_BYTES] = (&mut rest[..PAGE_BYTES])
        .try_into()
        .expect("history after loan");
    let mut history = History {
        digests: [[0; 32]; 4],
        count: 0,
    };
    let mut offset = 4096;
    let mut previous = 0u64;
    let mut actor = None;
    let mut active: Option<Batch> = None;
    let mut high_water = 0u64;
    let mut terminal = TerminalOutcome::Cancelled;
    let expected_target = target(request);
    while offset < prefix.complete_offset {
        let expected = FrameExpectation {
            stream: prefix.stream,
            previous_lsn: previous,
        };
        if prefix.complete_offset - offset < 160 {
            return Err(invalid());
        }
        read_exact_at(file, &mut frame[..160], offset)
            .map_err(|e| unchanged(Cause::HistoryIo(e)))?;
        let length =
            wal_frame::semantic_frame_length(&frame[..160], expected).map_err(|_| invalid())?;
        if length > MAX_FRAME || length as u64 > prefix.complete_offset - offset {
            return Err(invalid());
        }
        read_exact_at(file, &mut frame[160..length], offset + 160)
            .map_err(|e| unchanged(Cause::HistoryIo(e)))?;
        let bytes = &frame[..length];
        let envelope =
            wal_frame::decode_semantic_envelope(bytes, expected).map_err(|_| invalid())?;
        let header = envelope.header;
        if header.lsn != previous.checked_add(1).ok_or_else(invalid)? {
            return Err(invalid());
        }
        if header.lsn <= frozen.0 {
            if header.kind != FrameKind::System || length < 2144 || &bytes[160..164] != b"XSG1" {
                return Err(invalid());
            }
            let begin = match u16::from_le_bytes(bytes[166..168].try_into().expect("fixed opcode"))
            {
                8 => true,
                10 => false,
                _ => return Err(invalid()),
            };
            let allocation = allocation_page::decode_allocation_page(
                before,
                allocator,
                profile.spec().namespace,
            )
            .map_err(|_| invalid())?;
            operation::validate_live_in_population(
                &allocation,
                profile,
                active,
                descriptor,
                high_water,
                OperationBudget::StatusInit114688,
                AllocationPopulation::Status,
                terminal,
            )
            .map_err(|_| invalid())?;
            let batch = if begin {
                if active.is_some()
                    || u16::from_le_bytes(bytes[290..292].try_into().expect("count")) != 1
                {
                    return Err(invalid());
                }
                let mut first = None;
                for i in 1..64 {
                    if let AllocationEntry::Free(g) = allocation.entry(i).map_err(|_| invalid())?
                        && g.get() != u64::MAX
                    {
                        first = Some(i);
                        break;
                    }
                }
                let index = first.ok_or_else(invalid)?;
                Batch {
                    owner: SystemWorkRef::new(
                        profile.spec().namespace,
                        SystemWorkKind::AllocationMaintenance,
                        high_water.checked_add(1).ok_or_else(invalid)?,
                        Generation::new(1).expect("one"),
                    )
                    .map_err(|_| invalid())?,
                    mask: 1u64 << index,
                    birth: header.lsn,
                }
            } else {
                active.ok_or_else(invalid)?
            };
            if !begin && length == MAX_FRAME {
                let index = batch.mask.trailing_zeros() as usize;
                let AllocationEntry::Reserved {
                    generation,
                    claimant,
                } = allocation.entry(index).map_err(|_| invalid())?
                else {
                    return Err(invalid());
                };
                if claimant != ReservationOwner::System(batch.owner) {
                    return Err(invalid());
                }
                let payload = PageIdentity {
                    database: allocator.database,
                    page: PageRef {
                        page_no: index as u64,
                        incarnation: generation,
                        ..allocator.page
                    },
                    owner: xadb_foundation::identity::ObjectId::new(2).expect("Status object"),
                    owner_generation: Generation::new(1).expect("one"),
                };
                let plan = init::InitExpected {
                    profile,
                    allocator,
                    payload,
                    frame: expected,
                    batch,
                    before_descriptor: (&*descriptor).try_into().expect("descriptor"),
                };
                init::materialize_analysis_in_population(
                    &plan,
                    before,
                    bytes,
                    after,
                    status_output,
                    AllocationPopulation::Status,
                )
                .map_err(|_| invalid())?;
                if payload == request.identity {
                    if history.count != 0 {
                        return Err(invalid());
                    }
                    selected_init.copy_from_slice(status_output);
                    history.push(selected_init)?;
                }
                terminal = TerminalOutcome::InitializedStatus;
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
                    OperationBudget::StatusInit114688,
                    AllocationPopulation::Status,
                )
                .map_err(|_| invalid())?;
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
            if header.lsn == frozen.0 {
                if active.is_some()
                    || history.count != 1
                    || content_digest(before) != allocator_digest
                {
                    return Err(invalid());
                }
                before.copy_from_slice(selected_init);
                descriptor.fill(0);
            }
        } else {
            if length != FRAME_BYTES {
                return Err(invalid());
            }
            let opcode = u16::from_le_bytes(bytes[166..168].try_into().expect("fixed opcode"));
            if !matches!(
                (opcode, header.kind),
                (1, FrameKind::Begin) | (3, FrameKind::Update) | (7, FrameKind::AbortEnd)
            ) {
                return Err(invalid());
            }
            let redo =
                wal_frame::decode_redo_record(header, &bytes[2720..3008]).map_err(|_| invalid())?;
            if redo.target == expected_target {
                let candidate = TxnRef::decode(&bytes[200..248]).map_err(|_| invalid())?;
                let location = StatusSlotRef::new(request.identity.page, 0, candidate.generation())
                    .map_err(|_| invalid())?;
                let transition = RollbackStartExpected {
                    frame: expected,
                    profile,
                    actor: candidate,
                    location,
                    page: request.identity,
                };
                match (history.count, opcode) {
                    (1, 1) if candidate.generation().get() == 2 => {
                        begin::materialize_begin(
                            bytes,
                            begin::BeginExpected {
                                frame: expected,
                                profile,
                                actor: candidate,
                                location,
                                page: request.identity,
                                previous_local: candidate
                                    .id()
                                    .local
                                    .checked_sub(1)
                                    .ok_or_else(invalid)?,
                            },
                            before,
                            after,
                        )
                        .map_err(|e| unchanged(Cause::Semantic(e)))?;
                        actor = Some(candidate);
                    }
                    (2, 3) if actor == Some(candidate) => {
                        semantic_group::materialize_rollback_start(
                            bytes, transition, descriptor, before, after,
                        )
                        .map_err(|e| unchanged(Cause::Semantic(e)))?;
                    }
                    (3, 7) if actor == Some(candidate) => {
                        empty_abort::materialize_empty_abort(
                            bytes, transition, descriptor, before, after,
                        )
                        .map_err(|e| unchanged(Cause::Semantic(e)))?;
                    }
                    _ => return Err(invalid()),
                }
                descriptor.copy_from_slice(&bytes[352..1888]);
                before.copy_from_slice(after);
                history.push(before)?;
            }
        }
        offset += length as u64;
        previous = header.lsn;
    }
    if expected_descriptor.is_some_and(|expected| expected != descriptor) {
        return Err(invalid());
    }
    if previous != prefix.complete_lsn || history.count == 0 || *history.current() != current_digest
    {
        return Err(invalid());
    }
    Ok(history)
}
