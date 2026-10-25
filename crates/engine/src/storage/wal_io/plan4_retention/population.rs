//! One linear count-one reservation and the original INIT claim/install path.
//! The sealed adapters below cannot be constructed from a decoded observation.
use super::super::super::super::allocation_reservation::operation::{
    self as allocation, Batch, retention as operation,
};
use super::super::super::status_init_operation::retention as init;
use super::super::super::{
    CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit, InitInstallPhase,
};
use super::control::{self, ALLOCATION_SLOT, PHASE, STABLE};
use super::*;
use crate::{
    storage::{
        allocation_page::SystemWorkRef, dirty_frame::InitClaimKey, page::PageRef,
        session_fence::SessionFence,
    },
    transaction::concurrency::PageConcurrency,
};
pub(super) use init::RetentionPageKind;
use std::sync::atomic::AtomicU8;

/// A non-copy loan of the actual installed plan4 identity and session fence.
/// A decoded kind or copied namespace can never construct this live role.
struct Plan4SystemRole<'s> {
    plan: &'s super::super::super::super::super::BootstrapPlan,
    fence: &'s SessionFence,
    kind: RetentionPageKind,
}
impl<'s> Plan4SystemRole<'s> {
    fn new<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, '_, W>,
        kind: RetentionPageKind,
    ) -> Result<Self> {
        if storage.core.plan.kind
            != super::super::super::super::super::BootstrapPlanKind::RetentionCatalogBootstrap
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        Ok(Self {
            plan: &storage.core.plan,
            fence: storage.session_fence(),
            kind,
        })
    }
    fn matches<W: WriterNamespace>(&self, storage: &MutableStorageOwner<'_, '_, W>) -> bool {
        std::ptr::eq(self.plan, &storage.core.plan)
            && std::ptr::eq(self.fence, storage.session_fence())
            && self.plan.profile == storage.profile()
            && self.plan.kind
                == super::super::super::super::super::BootstrapPlanKind::RetentionCatalogBootstrap
    }
}

pub(in crate::storage) struct RetentionInitAuthority<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    batch: Batch,
    allocator: TargetRequest,
    payload: TargetRequest,
    generations: [u64; 2],
    payload_index: usize,
}
impl<'c, 's> RetentionInitAuthority<'c, 's> {
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    pub(in crate::storage) fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    pub(in crate::storage::wal_io::bootstrap::doublewrite_install) fn batch(&self) -> Batch {
        self.batch
    }
    pub(in crate::storage) fn allocator(&self) -> TargetRequest {
        self.allocator
    }
    pub(in crate::storage) fn payload(&self) -> TargetRequest {
        self.payload
    }
    pub(in crate::storage) fn ordinal(&self) -> u16 {
        0
    }
    pub(in crate::storage) fn generations(&self) -> [u64; 2] {
        self.generations
    }
    pub(in crate::storage) fn payload_index(&self) -> usize {
        self.payload_index
    }
}
pub(in crate::storage) struct RetentionInitInstall<'a, 'c, 's> {
    key: &'a InitClaimKey<'c, 's>,
    allocator_before: FrameStamp,
    requirement: WalRequirement<'s>,
    allocator_after: &'a [u8; PAGE_BYTES],
    status_after: &'a [u8; PAGE_BYTES],
}
impl<'a, 'c, 's> RetentionInitInstall<'a, 'c, 's> {
    pub(in crate::storage) fn claim(&self) -> &'a InitClaimKey<'c, 's> {
        self.key
    }
    pub(in crate::storage) fn allocator_before(&self) -> FrameStamp {
        self.allocator_before
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn allocator_after(&self) -> &'a [u8; PAGE_BYTES] {
        self.allocator_after
    }
    pub(in crate::storage) fn status_after(&self) -> &'a [u8; PAGE_BYTES] {
        self.status_after
    }
}
pub(in crate::storage) struct RetentionInitCommit<'a> {
    descriptor: &'a mut [u8; 1536],
    after_descriptor: &'a [u8; 1536],
    phase: &'a mut InitInstallPhase,
}
impl<'a> RetentionInitCommit<'a> {
    pub(in crate::storage) fn into_parts(
        self,
    ) -> (&'a mut [u8; 1536], &'a [u8; 1536], &'a mut InitInstallPhase) {
        (self.descriptor, self.after_descriptor, self.phase)
    }
}
pub(in crate::storage) struct RetentionInitCompleted<'c, 's> {
    authority: InitAuthority<'c, 's>,
}
impl<'c, 's> RetentionInitCompleted<'c, 's> {
    pub(in crate::storage) fn into_authority(self) -> InitAuthority<'c, 's> {
        self.authority
    }
}

struct Installation<'c, 's> {
    key: InitClaimKey<'c, 's>,
    phase: InitInstallPhase,
}
pub(super) struct Population<'c, 's> {
    high_water: u64,
    terminal: u8,
    reservation: Option<(SystemWorkRef, PageRef, u64)>,
    installation: Option<Installation<'c, 's>>,
}
impl Population<'_, '_> {
    pub(super) fn new() -> Self {
        Self {
            high_water: 0,
            terminal: 0,
            reservation: None,
            installation: None,
        }
    }
    pub(super) fn reserved(&self) -> bool {
        self.reservation.is_some() || self.installation.is_some()
    }
    #[cfg(test)]
    pub(super) fn test_state(&self) -> (bool, bool) {
        (self.reservation.is_some(), self.installation.is_some())
    }
    fn adopt(&mut self, state: operation::RetentionAllocationState) {
        self.high_water = state.high_water;
        self.terminal = state.terminal;
        self.reservation = state.reserved.map(|r| (r.owner, r.page, r.birth));
    }
}

pub(crate) enum RetentionReservationError<R> {
    Before {
        error: RetentionError,
        reservation: R,
    },
    Fatal(RetentionError),
}
/// Existing Status API keeps its source-level name; both wrappers use v2 scope.
pub(crate) type ReservedRetentionStatus<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> =
    ReservedRetentionSystem<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
pub(crate) struct ReservedRetentionSystem<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    bound: &'r mut Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'r mut Owner<'c, 's>,
    page: PageRef,
    role: Plan4SystemRole<'s>,
    active: bool,
}
impl<W: WriterNamespace> Drop for ReservedRetentionSystem<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        if self.active {
            self.bound.inner.storage.session_fence().fence();
            self.bound.inner.storage.wal().completion.notify_all();
        }
    }
}
impl<W: WriterNamespace> ReservedRetentionSystem<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(crate) fn page(&self) -> PageRef {
        self.page
    }
}

impl<'s, 'c, W: WriterNamespace> ReservedRetentionSystem<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W> {
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the same linear reservation"
    )]
    pub(crate) fn cancel(mut self) -> std::result::Result<(), RetentionReservationError<Self>> {
        match self.bound.allocate(self.owner, true) {
            Ok(()) => {
                self.active = false;
                Ok(())
            }
            Err(error) => Err(self.failure(error)),
        }
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the same linear reservation"
    )]
    pub(crate) fn initialize(
        mut self,
    ) -> std::result::Result<Plan4Target, RetentionReservationError<Self>> {
        match self.initialize_inner() {
            Ok(target) => {
                self.active = false;
                Ok(target)
            }
            Err(error) => Err(self.failure(error)),
        }
    }
    fn failure(self, error: RetentionError) -> RetentionReservationError<Self> {
        if error.effect == Effect::Unchanged
            && self.bound.control.pending.is_none()
            && self.bound.control.population.installation.is_none()
        {
            RetentionReservationError::Before {
                error,
                reservation: self,
            }
        } else {
            let error = fatal(self.bound.inner.storage, error.phase, error.cause);
            RetentionReservationError::Fatal(error)
        }
    }
    fn initialize_inner(&mut self) -> Result<Plan4Target> {
        let storage = self.bound.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let pool = &self.bound.inner.pool;
        health(storage, pool)?;
        pool.check_retention_reader(self.owner)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let control = &mut self.bound.control;
        if control.pending.is_some() || control.population.installation.is_some() {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let (work, page, birth) = control
            .population
            .reservation
            .ok_or_else(|| fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant))?;
        if page != self.page || !self.role.matches(storage) {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let allocator = pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let payload = TargetRequest {
            identity: PageIdentity {
                database: allocator.identity.database,
                page,
                owner: self.role.kind.object(),
                owner_generation: foundation::identity::Generation::new(1)
                    .expect("Status generation"),
            },
            kind: self.role.kind.kind(),
            level: 0,
        };
        let authority = InitAuthority::from_retention(RetentionInitAuthority {
            core: pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            batch: Batch {
                owner: work,
                mask: 1u64 << page.page_no,
                birth,
            },
            allocator,
            payload,
            generations: pool
                .init_generations()
                .map_err(|e| frame(RetentionPhase::Prepare, e))?,
            payload_index: pool
                .init_payload_index()
                .map_err(|e| frame(RetentionPhase::Prepare, e))?,
        });
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        if prefix.complete_lsn != birth {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let (bank, before_space) = control.bytes.split_at_mut(131_072);
        let before: &mut [u8; PAGE_BYTES] = (&mut before_space[..PAGE_BYTES])
            .try_into()
            .expect("allocator before");
        let (frame_bytes, after) = self.bound.inner.operation.split_at_mut(FRAME_BYTES);
        let frame_bytes: &mut [u8; FRAME_BYTES] = frame_bytes.try_into().expect("frame loan");
        let (allocator_after, rest) = after.split_at_mut(PAGE_BYTES);
        let allocator_after: &mut [u8; PAGE_BYTES] =
            allocator_after.try_into().expect("allocator after");
        let status_after: &mut [u8; PAGE_BYTES] =
            (&mut rest[..PAGE_BYTES]).try_into().expect("Status after");
        let mut write = pool
            .claim_init_targets(self.owner, &authority)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let installed = (|| {
            let stamp = write
                .copy_allocator_snapshot(before)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let plan = operation::RetentionAllocationPlan {
                profile: storage.profile(),
                allocator: allocator.identity,
                prefix,
                high_water: control.population.high_water,
                terminal: control.population.terminal,
                before_descriptor: descriptor(control::row(bank, ALLOCATION_SLOT)),
                fence: storage.session_fence(),
            };
            let prepared = init::prepare_retention_init(
                &plan,
                self.role.kind,
                before,
                frame_bytes,
                allocator_after,
                status_after,
            )
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Prepare,
                    RetentionCause::AllocationOperation(e),
                )
            })?;
            let after_descriptor = prepared.descriptor_bytes();
            let allocator_after = prepared.allocator_after();
            let status_after = prepared.status_after();
            let state = prepared.state();
            write
                .preflight(&stamp)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            let requirement =
                prepared
                    .stage_required(storage.wal())
                    .map_err(|e| RetentionError {
                        phase: RetentionPhase::Stage,
                        effect: e.effect,
                        cause: RetentionCause::Wal(e),
                    })?;
            control.pending = Some(requirement);
            control.population.installation = Some(Installation {
                key: write.retain_claim(),
                phase: InitInstallPhase::StageOwned,
            });
            let installed = control
                .population
                .installation
                .as_mut()
                .expect("retained claim");
            let permit = InitInstallPermit::from_retention(RetentionInitInstall {
                key: &installed.key,
                allocator_before: stamp,
                requirement,
                allocator_after,
                status_after,
            });
            let descriptor: &mut [u8; 1536] = (&mut control::row_mut(bank, ALLOCATION_SLOT)
                [..1536])
                .try_into()
                .expect("allocation row");
            let commit = InitControlCommit::from_retention(RetentionInitCommit {
                descriptor,
                after_descriptor,
                phase: &mut installed.phase,
            });
            write
                .prepare_install(&permit, commit)
                .map_err(|e| frame(RetentionPhase::Install, e))?
                .install();
            remember(control::row_mut(bank, ALLOCATION_SLOT), state);
            // Preserve the reservation until the genuine durable proof and
            // checked original claim completion below have both succeeded.
            control.population.high_water = state.high_water;
            control.population.terminal = state.terminal;
            Ok(())
        })();
        merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
        .map_err(|e| {
            if control.pending.is_some() {
                fatal(storage, RetentionPhase::Install, e.cause)
            } else {
                e
            }
        })?;
        let requirement = control.pending.expect("retained INIT stage");
        let durable = storage
            .wal()
            .wait_durable(&requirement)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        if !durable.confirms(&requirement)
            || control
                .population
                .installation
                .as_ref()
                .is_none_or(|i| i.phase != InitInstallPhase::Published)
        {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        let read = pool
            .reserve_target_set(self.owner, &[allocator, payload])
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?
            .try_read()
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?;
        let checked = (|| {
            if read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Durable, e))?
                != allocator_after
                || read
                    .bytes(1)
                    .map_err(|e| frame(RetentionPhase::Durable, e))?
                    != status_after
            {
                return Err(fatal(
                    storage,
                    RetentionPhase::Durable,
                    RetentionCause::Invariant,
                ));
            }
            Ok(())
        })();
        merge(
            checked,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Durable, e)),
        )
        .map_err(|e| fatal(storage, RetentionPhase::Durable, e.cause))?;
        let completed =
            CompletedInitAuthority::from_retention(RetentionInitCompleted { authority });
        pool.finish_init_claim(
            &mut control
                .population
                .installation
                .as_mut()
                .expect("retained claim")
                .key,
            &completed,
        )
        .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?;
        control.population.installation = None;
        control.population.reservation = None;
        control.durable = Some(durable);
        control.pending = None;
        Ok(Plan4Target { request: payload })
    }
}

// The actual prepare/materialize entry points below are linked to the new
// plan4 population module. Old plan2/3 role factories are never invoked here.
fn remember(row: &mut [u8], state: operation::RetentionAllocationState) {
    row[1536..1544].copy_from_slice(&state.high_water.to_le_bytes());
    row[1544] = state.terminal;
    row[PHASE] = STABLE;
}
fn descriptor(row: &[u8]) -> Option<&[u8; 1536]> {
    (row[PHASE] != 0).then(|| row[..1536].try_into().expect("allocation descriptor"))
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Prepare a real empty payload slot; a dirty outsider is returned to the
    /// caller's original flusher. The following reserve performs no eviction.
    pub(crate) fn prepare_status_slot(
        &mut self,
        owner: &Owner<'c, 's>,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Option<RetentionResidencyStep>> {
        self.prepare_system_slot(owner, cursor, RetentionPageKind::Status)
    }
    pub(crate) fn prepare_undo_slot(
        &mut self,
        owner: &Owner<'c, 's>,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Option<RetentionResidencyStep>> {
        self.prepare_system_slot(owner, cursor, RetentionPageKind::FreeUndo)
    }
    fn prepare_system_slot(
        &mut self,
        owner: &Owner<'c, 's>,
        cursor: &mut Plan4ResidencyCursor,
        kind: RetentionPageKind,
    ) -> Result<Option<RetentionResidencyStep>> {
        self.health()?;
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if self.control.population.reserved() || self.flush.phase != RetentionFlushPhase::Ready {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let storage = self.inner.storage;
        let role = Plan4SystemRole::new(storage, kind)?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        history::validate_live(storage, &mut self.control, prefix)?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let page = crate::storage::allocation_page::decode_allocation_page(
            history::arena_page(self.control.replay, 0),
            allocator.identity,
            storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Allocation(e),
            )
        })?;
        let mut target = None;
        for index in 9..64 {
            if let crate::storage::allocation_page::AllocationEntry::Free(g) =
                page.entry(index).map_err(|e| {
                    fatal(
                        storage,
                        RetentionPhase::Prepare,
                        RetentionCause::Allocation(e),
                    )
                })?
                && g.get() != u64::MAX
            {
                target = Some(TargetRequest {
                    identity: PageIdentity {
                        database: allocator.identity.database,
                        page: PageRef {
                            page_no: index as u64,
                            incarnation: foundation::identity::Generation::new(g.get() + 1)
                                .expect("checked generation"),
                            ..allocator.identity.page
                        },
                        owner: role.kind.object(),
                        owner_generation: foundation::identity::Generation::new(1)
                            .expect("Status generation"),
                    },
                    kind: role.kind.kind(),
                    level: 0,
                });
                break;
            }
        }
        let target =
            target.ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))?;
        let candidate = self
            .inner
            .pool
            .retention_candidate(target, cursor.next, &[allocator])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if candidate.kind == dirty_frame::StatusCandidateKind::Flush {
            let request = self
                .inner
                .pool
                .target_request(candidate.index)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            return Ok(Some(RetentionResidencyStep::FlushRequired {
                target: Plan4Target { request },
                rec_lsn: candidate.frame.and_then(|s| s.rec_lsn).ok_or_else(|| {
                    fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant)
                })?,
            }));
        }
        if candidate.kind == dirty_frame::StatusCandidateKind::Resident {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        if !(candidate.frame.is_none()
            && self.inner.pool.init_payload_index().ok() == Some(candidate.index))
        {
            self.inner
                .pool
                .prepare_retention_slot(candidate.index)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        }
        cursor.next = candidate.next;
        Ok(None)
    }
    pub(crate) fn reserve_status<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
    ) -> Result<ReservedRetentionStatus<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.reserve_system(owner, RetentionPageKind::Status)
    }
    pub(crate) fn reserve_undo<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
    ) -> Result<ReservedRetentionSystem<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.reserve_system(owner, RetentionPageKind::FreeUndo)
    }
    fn reserve_system<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        kind: RetentionPageKind,
    ) -> Result<ReservedRetentionSystem<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.health()?;
        if self.control.population.reserved() {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        self.inner
            .pool
            .init_payload_index()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let role = Plan4SystemRole::new(self.inner.storage, kind)?;
        self.allocate(owner, false)?;
        let page = self
            .control
            .population
            .reservation
            .ok_or_else(|| {
                fatal(
                    self.inner.storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                )
            })?
            .1;
        Ok(ReservedRetentionSystem {
            bound: self,
            owner,
            page,
            role,
            active: true,
        })
    }
    pub(super) fn allocate(&mut self, owner: &mut Owner<'c, 's>, cancel: bool) -> Result<()> {
        let storage = self.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        health(storage, &self.inner.pool)?;
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if self.control.pending.is_some() || self.control.population.installation.is_some() {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        history::validate_live(storage, &mut self.control, prefix)?;
        let (existing_wal, existing_steps) =
            self.control
                .check_ledger(storage.profile(), prefix)
                .map_err(|e| fatal(storage, RetentionPhase::Census, e.cause))?;
        let extra = if cancel { 0 } else { 21_200 + 38_208 };
        if prefix
            .complete_offset
            .checked_add(existing_wal)
            .and_then(|n| n.checked_add(extra))
            .is_none_or(|n| n > crate::storage::wal_io::WAL_FILE_CAP)
            || prefix
                .complete_lsn
                .checked_add(existing_steps)
                .and_then(|n| n.checked_add(if cancel { 0 } else { 2 }))
                .is_none()
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
        }
        let request = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let (bank, before_space) = self.control.bytes.split_at_mut(131_072);
        let before: &mut [u8; PAGE_BYTES] = (&mut before_space[..PAGE_BYTES])
            .try_into()
            .expect("before allocator");
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &[request])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let copied = read
            .copy_snapshot(0, before)
            .map_err(|e| frame(RetentionPhase::Prepare, e));
        let stamp = merge(
            copied,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )?;
        if before != history::arena_page(self.control.replay, 0) {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        if !cancel && stamp.modification_counter.checked_add(2).is_none() {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Exhausted,
            ));
        }
        let (frame_bytes, after) = self.inner.operation.split_at_mut(FRAME_BYTES);
        let frame_bytes: &mut [u8; FRAME_BYTES] = frame_bytes.try_into().expect("frame loan");
        let allocator_after: &mut [u8; PAGE_BYTES] = (&mut after[..PAGE_BYTES])
            .try_into()
            .expect("allocator after");
        let plan = operation::RetentionAllocationPlan {
            profile: storage.profile(),
            allocator: request.identity,
            prefix,
            high_water: self.control.population.high_water,
            terminal: self.control.population.terminal,
            before_descriptor: descriptor(control::row(bank, ALLOCATION_SLOT)),
            fence: storage.session_fence(),
        };
        let prepared = if cancel {
            operation::prepare_retention_cancel(&plan, before, frame_bytes, allocator_after)
        } else {
            operation::prepare_retention_reserve(&plan, before, frame_bytes, allocator_after)
        }
        .map_err(|e| {
            fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::AllocationOperation(e),
            )
        })?;
        let state = prepared.state();
        let after = prepared.allocator_after();
        let after_descriptor = prepared.descriptor_bytes();
        let mut write = self
            .inner
            .pool
            .reserve_target_set(owner, &[request])
            .map_err(|e| frame(RetentionPhase::Install, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Install, e))?;
        let installed = (|| {
            write
                .matches_stamp(0, &stamp)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            let dpt = write
                .preflight_dpt(0, &stamp)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            let requirement =
                prepared
                    .stage_required(storage.wal())
                    .map_err(|e| RetentionError {
                        phase: RetentionPhase::Stage,
                        effect: e.effect,
                        cause: RetentionCause::Wal(e),
                    })?;
            self.control.pending = Some(requirement);
            self.control.population.adopt(state);
            let permit = crate::storage::wal_io::DirtyInstallPermit::from_retention(
                live::RetentionInstall::allocation(
                    self.inner.pool.bound_core(),
                    stamp,
                    requirement,
                    after,
                ),
            );
            write
                .install_page(0, &permit)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            write
                .install_metadata(0, &permit)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            let row = control::row_mut(bank, ALLOCATION_SLOT);
            row[..1536].copy_from_slice(after_descriptor);
            remember(row, state);
            write
                .install_dpt(dpt, &permit)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            Ok(())
        })();
        merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
        .map_err(|e| {
            if self.control.pending.is_some() {
                fatal(storage, RetentionPhase::Install, e.cause)
            } else {
                e
            }
        })?;
        let requirement = self.control.pending.expect("retained allocation stage");
        let durable = storage
            .wal()
            .wait_durable(&requirement)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        if !durable.confirms(&requirement) {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &[request])
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?
            .try_read()
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?;
        let checked = read
            .bytes(0)
            .map_err(|e| frame(RetentionPhase::Durable, e))
            .and_then(|actual| {
                if actual == after {
                    Ok(())
                } else {
                    Err(fatal(
                        storage,
                        RetentionPhase::Durable,
                        RetentionCause::Invariant,
                    ))
                }
            });
        merge(
            checked,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Durable, e)),
        )
        .map_err(|e| fatal(storage, RetentionPhase::Durable, e.cause))?;
        self.control.durable = Some(durable);
        self.control.pending = None;
        Ok(())
    }
}

/// Scalar replay cursor. Its outputs are canonical bytes and observations only;
/// no initializer claim, live reservation or WalRequirement is returned.
pub(super) struct ReplayAllocation {
    state: operation::RetentionAllocationState,
}
pub(super) struct ReplayAllocationObservation {
    pub(super) header: crate::storage::wal_frame::WalFrameHeader,
    pub(super) initialized: Option<TargetRequest>,
}
impl ReplayAllocation {
    pub(super) fn new() -> Self {
        Self {
            state: operation::RetentionAllocationState {
                high_water: 0,
                terminal: 0,
                reserved: None,
            },
        }
    }
    pub(super) fn reserved(&self) -> bool {
        self.state.reserved.is_some()
    }
    pub(super) fn from_verified_base(
        profile: crate::storage::control::Profile,
        seed: &super::super::super::plan4_recovery::VerifiedPlan4Base<'_>,
        bank: &mut [u8],
    ) -> std::result::Result<Self, RetentionCause> {
        use crate::storage::{
            allocation_page::{self, AllocationEntry, ReservationOwner},
            plan4_recovery_base::AllocationTerminal,
        };
        let view = seed.view();
        let h = view.header();
        if h.format != crate::storage::plan4_recovery_base::BaseFormat::V2System {
            return Err(RetentionCause::Unsupported);
        }
        let high_water = h.system_high_water[0];
        let terminal = match h.allocation_terminal {
            AllocationTerminal::None => 0,
            AllocationTerminal::Cancelled => 1,
            AllocationTerminal::InitializedStatus => 2,
            AllocationTerminal::InitializedUndo => 3,
        };
        let reserved = if let Some(page) = view.census().reserved {
            let mut identity = crate::storage::retention_genesis::expected_header(
                profile.spec().namespace,
                crate::storage::retention_genesis::RetentionGenesisPage::Root,
            )
            // Fixed Root recipe on the already selected, validated profile.
            // Failure here denotes an internal reconstruction invariant.
            .map_err(|_| RetentionCause::Invariant)?
            .identity;
            identity.page.page_no = 0;
            identity.owner = foundation::identity::ObjectId::new(1).expect("allocator object");
            let allocation = allocation_page::decode_allocation_page(
                view.allocator(),
                identity,
                profile.spec().namespace,
            )
            .map_err(RetentionCause::Allocation)?;
            let AllocationEntry::Reserved {
                generation,
                claimant: ReservationOwner::System(owner),
            } = allocation
                .entry(page.page_no as usize)
                .map_err(RetentionCause::Allocation)?
            else {
                return Err(RetentionCause::Invariant);
            };
            if generation != page.incarnation
                || owner.id() != high_water
                || terminal != 0
                || allocation.header().lsn.0 != h.capture_lsn
            {
                return Err(RetentionCause::Invariant);
            }
            Some(operation::RetentionReservation {
                owner,
                page,
                birth: h.capture_lsn,
            })
        } else {
            None
        };
        let state = operation::RetentionAllocationState {
            high_water,
            terminal,
            reserved,
        };
        let row = control::row_mut(bank, ALLOCATION_SLOT);
        if high_water == 0 {
            if row.iter().any(|b| *b != 0) || terminal != 0 || reserved.is_some() {
                return Err(RetentionCause::Invariant);
            }
        } else {
            if row[..1536].iter().all(|b| *b == 0) {
                return Err(RetentionCause::Invariant);
            }
            remember(row, state);
        }
        Ok(Self { state })
    }
    pub(super) fn restore_population(&self, population: &mut Population<'_, '_>) {
        population.adopt(self.state);
    }
    pub(super) fn apply(
        &mut self,
        profile: crate::storage::control::Profile,
        frame: crate::storage::wal_frame::FrameExpectation,
        bytes: &[u8],
        arena: &mut [u8],
        bank: &mut [u8],
        after: &mut [[u8; PAGE_BYTES]; 3],
    ) -> std::result::Result<ReplayAllocationObservation, allocation::SemanticError> {
        use allocation::SemanticError as E;
        let envelope = crate::storage::wal_frame::decode_semantic_envelope(bytes, frame)
            .map_err(|_| E::Corrupt)?;
        if envelope.header.kind != crate::storage::wal_frame::FrameKind::System
            || bytes.len() < 2144
            || bytes[160..164] != *b"XSG1"
            || bytes[164..166] != [2, 0]
            || bytes[170..172] != [1, 0]
        {
            return Err(E::UnsupportedOperation);
        }
        let mut allocator = crate::storage::retention_genesis::expected_header(
            profile.spec().namespace,
            crate::storage::retention_genesis::RetentionGenesisPage::Root,
        )
        .map_err(|_| E::IdentityMismatch)?
        .identity;
        allocator.owner = foundation::identity::ObjectId::new(1).expect("allocator object");
        allocator.page.page_no = 0;
        let expected = operation::RetentionAllocationExpected {
            profile,
            allocator,
            frame,
            high_water: self.state.high_water,
            terminal: self.state.terminal,
            before_descriptor: descriptor(control::row(bank, ALLOCATION_SLOT)),
        };
        let before: &[u8; PAGE_BYTES] = arena[..PAGE_BYTES].try_into().expect("allocator arena");
        let opcode = u16::from_le_bytes(bytes[166..168].try_into().expect("opcode"));
        let [allocator_after, status_after, _] = after;
        let mut initialized = None;
        let state = if bytes.len() == operation::RETENTION_RESERVE_BYTES {
            match opcode {
                8 if self.state.reserved.is_none() => operation::materialize_retention_reserve(
                    &expected,
                    before,
                    bytes,
                    allocator_after,
                )?,
                10 if self.state.reserved.is_some() => operation::materialize_retention_cancel(
                    &expected,
                    before,
                    bytes,
                    allocator_after,
                )?,
                _ => return Err(E::StateMismatch),
            }
        } else if matches!(
            bytes.len(),
            init::RETENTION_INIT_BYTES | init::RETENTION_UNDO_INIT_BYTES
        ) && opcode == 10
        {
            let kind = if bytes.len() == init::RETENTION_INIT_BYTES {
                RetentionPageKind::Status
            } else {
                RetentionPageKind::FreeUndo
            };
            let reserved = self.state.reserved.ok_or(E::StateMismatch)?;
            let state = init::materialize_retention_init(
                &expected,
                kind,
                before,
                bytes,
                allocator_after,
                status_after,
            )?;
            let request = TargetRequest {
                identity: PageIdentity {
                    database: profile.spec().namespace.database,
                    page: reserved.page,
                    owner: kind.object(),
                    owner_generation: foundation::identity::Generation::new(1)
                        .expect("Status generation"),
                },
                kind: kind.kind(),
                level: 0,
            };
            let index = reserved.page.page_no as usize;
            if !(9..64).contains(&index)
                || arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                    .iter()
                    .any(|b| *b != 0)
            {
                return Err(E::StateMismatch);
            }
            arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES].copy_from_slice(status_after);
            initialized = Some(request);
            state
        } else {
            return Err(E::UnsupportedOperation);
        };
        arena[..PAGE_BYTES].copy_from_slice(allocator_after);
        let row = control::row_mut(bank, ALLOCATION_SLOT);
        row[..1536].copy_from_slice(&bytes[352..1888]);
        remember(row, state);
        self.state = state;
        Ok(ReplayAllocationObservation {
            header: envelope.header,
            initialized,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PopulationSourceCensus {
    pub(super) allocation_prepare: usize,
    pub(super) init_prepare: usize,
    pub(super) publication: usize,
    pub(super) durable: usize,
    pub(super) replay: usize,
}
impl PopulationSourceCensus {
    pub(super) fn producer(self) -> usize {
        self.allocation_prepare
            .max(self.init_prepare)
            .max(self.publication)
            .max(self.durable)
    }
}

/// Actual call phases, excluding the whole live owner already retained by the
/// caller. The encoded descriptor, pages and frame are borrowed caller loans.
/// A retained Installation/Population and pool INIT slot are already fields of
/// that owner/pool; only temporary construction/move carriers are added here.
pub(super) fn reservation_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    type Token<'a, W> = ReservedRetentionSystem<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    // reserve returns the linear token; initialize/cancel return it only on an
    // unchanged failure. Count the owned token and both move/return carriers.
    2 * size_of::<Token<'_, W>>()
        + size_of::<Result<Token<'_, W>>>()
        + size_of::<std::result::Result<Plan4Target, RetentionReservationError<Token<'_, W>>>>()
        + size_of::<std::result::Result<(), RetentionReservationError<Token<'_, W>>>>()
}

pub(super) fn phase_source_bytes() -> PopulationSourceCensus {
    use crate::storage::{
        dirty_frame::{InitWriteTargets, ReadTargets},
        wal_io::{DurableWal, WalPrefix},
    };
    use std::mem::size_of;
    type Write<'a> = InitWriteTargets<'a, 'a, 'a, 'a, 'a, 'a>;
    let outer = 3 * size_of::<Plan4SystemRole<'_>>()
        + 3 * size_of::<Result<Plan4SystemRole<'_>>>()
        + 8 * size_of::<RetentionPageKind>()
        + size_of::<WalPrefix>()
        + 2 * size_of::<TargetRequest>()
        + size_of::<FrameStamp>()
        + 2 * size_of::<operation::RetentionAllocationState>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 32 * size_of::<&[u8]>()
        + 32 * size_of::<u64>()
        + 512;
    // allocate retains its Plan while the borrowed Prepared return is made.
    // The later original write/DPT/stage scope does not run during this call.
    let allocation_prepare = outer
        + size_of::<operation::RetentionAllocationPlan<'_, '_>>()
        + size_of::<operation::PreparedRetentionAllocation<'_, '_>>()
        + size_of::<Result<operation::PreparedRetentionAllocation<'_, '_>>>()
        + operation::retention_allocation_source_bytes();
    // initialize_inner really holds the original INIT write targets while the
    // producer runs. Count that guard and its typed return, not just references.
    let init_prepare = outer
        + size_of::<InitAuthority<'_, '_>>()
        + size_of::<Write<'_>>()
        + size_of::<Result<Write<'_>>>()
        + size_of::<operation::RetentionAllocationPlan<'_, '_>>()
        + size_of::<init::PreparedRetentionInit<'_, '_>>()
        + size_of::<Result<init::PreparedRetentionInit<'_, '_>>>()
        + init::retention_init_source_bytes();
    // The pure producer has returned before original stage/publication. Keep
    // all bridge moves, retained-claim construction and small return carriers.
    let publication = outer
        + size_of::<InitAuthority<'_, '_>>()
        + size_of::<Write<'_>>()
        + 2 * size_of::<RetentionInitInstall<'_, '_, '_>>()
        + 2 * size_of::<InitInstallPermit<'_, '_, '_>>()
        + 2 * size_of::<RetentionInitCommit<'_>>()
        + 2 * size_of::<InitControlCommit<'_>>()
        + size_of::<Installation<'_, '_>>()
        + size_of::<Option<Installation<'_, '_>>>()
        + 3 * size_of::<Result<FrameStamp>>()
        + 2 * size_of::<Result<WalRequirement<'_>>>()
        + dirty_frame::target_set_call_bytes::<2>()
        + 2048;
    // write.release_checked consumes its guards before wait_durable and the
    // checked readback/claim completion; the original authority survives both.
    let durable = outer
        + size_of::<InitAuthority<'_, '_>>()
        + size_of::<RetentionInitCompleted<'_, '_>>()
        + size_of::<CompletedInitAuthority<'_, '_>>()
        + 2 * size_of::<Result<DurableWal<'_>>>()
        + crate::storage::wal_io::DURABILITY_CALL_BYTES
        + size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, 2>>()
        + dirty_frame::target_set_call_bytes::<2>();
    // ReplayAllocation::apply has no live claim, permit, commit or completed
    // authority. Its state lives across materialize; all borrowed expected and
    // envelope/header/return values are charged in addition to that codec.
    let replay = 2 * size_of::<ReplayAllocation>()
        + 2 * size_of::<ReplayAllocationObservation>()
        + 2 * size_of::<Result<ReplayAllocationObservation>>()
        + 2 * size_of::<operation::RetentionAllocationExpected<'_>>()
        + 2 * size_of::<operation::RetentionAllocationState>()
        + 2 * size_of::<Result<operation::RetentionAllocationState>>()
        + 2 * size_of::<crate::storage::wal_frame::FrameEnvelope<'_>>()
        + 2 * size_of::<crate::storage::wal_frame::WalFrameHeader>()
        + 2 * size_of::<Option<TargetRequest>>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>()
        + 512
        + operation::retention_allocation_source_bytes().max(init::retention_init_source_bytes());
    PopulationSourceCensus {
        allocation_prepare,
        init_prepare,
        publication,
        durable,
        replay,
    }
}
pub(super) fn source_bytes() -> usize {
    let c = phase_source_bytes();
    c.producer().max(c.replay)
}
