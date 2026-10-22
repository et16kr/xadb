//! One reserved Status page, atomically installed in a closed two-frame pool.
//! The immutable bootstrap role, mutable allocator membership and retained WAL
//! requirement are distinct authorities. The separate consuming flush adapter
//! preserves this owner's existing layout and one-shot binding.

use super::super::allocation_reservation::{
    AllocationObservation, budget,
    operation::{Batch, SemanticError},
};
use super::{
    ABANDONED, AVAILABLE, BOUND, BindingClaim, CLAIMED, Cause as StorageCause, MutableStorageError,
    MutableStorageOwner,
    dirty_allocation::{self, AllocationWriter, WriterControl},
    status_init_operation as operation, unchanged,
};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry, ReservationOwner, SystemWorkRef},
        control::Profile,
        dirty_frame::{
            self, DirtyError, DirtyFrame, FrameStamp, InitClaimKey, ReadTargets, SharedPool,
            TargetRequest,
        },
        page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
        page_io::PageFileReader,
        session_fence::SessionFence,
        wal_io::{StatusRole, WalIoError, WalPrefix, WalRequirement},
    },
    transaction::concurrency::{Owner, PageConcurrency},
};
use std::{
    mem::size_of,
    sync::atomic::{AtomicU8, Ordering},
};
use xadb_foundation::{error::Effect, namespace::WriterNamespace};

pub(super) const OPERATION_BYTES: usize = 114688;
const FRAME_BYTES: usize = 65536;
const BEFORE_END: usize = FRAME_BYTES + PAGE_BYTES;
const INIT_EXCLUSIVE: usize = 8192;
const READ_CALL: usize = 3584;

#[cfg(test)]
#[path = "status_init_tests.rs"]
mod tests;

#[path = "status_home_flush.rs"]
mod status_flush;
pub(in crate::storage) use status_flush::StatusFlushAuthority;
pub(crate) use status_flush::{
    BoundStatusWithFlush, IntoFlushError, StatusFlushError, StatusFlusher,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Bind,
    Reserve,
    Prepare,
    Revalidate,
    Stage,
    Install,
    Release,
    Sync,
    Observe,
}
#[derive(Debug)]
pub(crate) enum Cause {
    Storage(MutableStorageError),
    Allocation(dirty_allocation::DirtyAllocationError),
    AllocationPage(allocation_page::AllocationError),
    Frame(DirtyError),
    Wal(WalIoError),
    Semantic(SemanticError),
    UnsupportedPlan,
    NotInitialized,
    AlreadyInitialized,
    BindingMismatch,
    StaleReservation,
    Capacity,
    Invariant,
}
#[derive(Debug)]
pub(crate) struct StatusInitError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, StatusInitError>;
fn error(phase: Phase, effect: Effect, cause: Cause) -> StatusInitError {
    StatusInitError {
        phase,
        effect,
        cause,
    }
}
fn frame_error(phase: Phase, source: DirtyError) -> StatusInitError {
    error(phase, source.effect, Cause::Frame(source))
}
fn merge_cleanup<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Err(first), _) if first.effect == Effect::CallerMustResolve => Err(first),
        (_, Err(cleanup)) => Err(cleanup),
        (result, Ok(())) => result,
    }
}

/// An unsuccessful attempt may return the very same reservation only while
/// cleanup proves it was unchanged relative to the already durable reserve.
pub(crate) struct BeforeEffectError<R> {
    pub(crate) error: StatusInitError,
    pub(crate) reservation: R,
}
pub(crate) enum InitAttemptError<R> {
    Before(BeforeEffectError<R>),
    Fatal(StatusInitError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InitInstallPhase {
    StageOwned,
    DescriptorInstalled,
    RegistryInstalled,
    Published,
}
struct InitObligation<'c, 's> {
    key: InitClaimKey<'c, 's>,
    requirement: WalRequirement<'s>,
    phase: InitInstallPhase,
}
struct StatusControl<'c, 's, 'o> {
    base: WriterControl<'s, 'o>,
    status_after: &'o mut [u8; PAGE_BYTES],
    role: StatusRole<'s>,
    installation: Option<InitObligation<'c, 's>>,
}

/// Only the actual reserved workflow can construct this origin-bound authority.
/// Native claim admission reads these values; it cannot manufacture the token.
pub(in crate::storage) struct InitAuthority<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    batch: Batch,
    allocator: TargetRequest,
    payload: TargetRequest,
    ordinal: u16,
}
impl<'c, 's> InitAuthority<'c, 's> {
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    pub(in crate::storage) fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    pub(in crate::storage) fn work(&self) -> SystemWorkRef {
        self.batch.owner
    }
    pub(in crate::storage) fn birth(&self) -> PageLsn {
        PageLsn(self.batch.birth)
    }
    pub(in crate::storage) fn ordinal(&self) -> u16 {
        self.ordinal
    }
    pub(in crate::storage) fn allocator(&self) -> TargetRequest {
        self.allocator
    }
    pub(in crate::storage) fn payload(&self) -> TargetRequest {
        self.payload
    }
}

/// Constructed only from prepared immutable images and their successful stage.
pub(in crate::storage) struct InitInstallPermit<'i, 'c, 's> {
    key: &'i InitClaimKey<'c, 's>,
    allocator_before: FrameStamp,
    requirement: WalRequirement<'s>,
    allocator_after: &'i [u8; PAGE_BYTES],
    status_after: &'i [u8; PAGE_BYTES],
}
impl<'i, 'c, 's> InitInstallPermit<'i, 'c, 's> {
    pub(in crate::storage) fn claim(&self) -> &InitClaimKey<'c, 's> {
        self.key
    }
    pub(in crate::storage) fn allocator_before(&self) -> FrameStamp {
        self.allocator_before
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn allocator_after(&self) -> &'i [u8; PAGE_BYTES] {
        self.allocator_after
    }
    pub(in crate::storage) fn status_after(&self) -> &'i [u8; PAGE_BYTES] {
        self.status_after
    }
}

/// Destinations are separately borrowed; installation cannot call user code or
/// borrow the entire writer while its output images remain sealed.
pub(in crate::storage) struct InitControlCommit<'i> {
    descriptor: &'i mut [u8; 1536],
    after_descriptor: &'i [u8; 1536],
    phase: &'i mut InitInstallPhase,
}
impl InitControlCommit<'_> {
    pub(in crate::storage) fn install_descriptor(&mut self) {
        self.descriptor.copy_from_slice(self.after_descriptor);
        *self.phase = InitInstallPhase::DescriptorInstalled;
    }
    pub(in crate::storage) fn mark_registry_installed(&mut self) {
        *self.phase = InitInstallPhase::RegistryInstalled;
    }
    pub(in crate::storage) fn mark_published(&mut self) {
        *self.phase = InitInstallPhase::Published;
    }
}

pub(in crate::storage) struct CompletedInitAuthority<'c, 's> {
    authority: InitAuthority<'c, 's>,
}
impl CompletedInitAuthority<'_, '_> {
    pub(in crate::storage) fn matches(&self, key: &InitClaimKey<'_, '_>) -> bool {
        key.matches_authority(&self.authority)
    }
}

pub(crate) struct BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: SharedPool<'c, 's, 'p, 'd>,
    control: StatusControl<'c, 's, 'o>,
}
impl<'m, 'w, W: WriterNamespace> MutableStorageOwner<'m, 'w, W> {
    /// The one-shot claim winner reads only the allocator. The closed payload's
    /// caller bytes are untouched until the actual, fully staged INIT install.
    pub(crate) fn bind_status_bootstrap<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        core: &'c PageConcurrency<'s>,
        frames: &'p mut [DirtyFrame<'d>; 2],
        operation: &'o mut [u8; OPERATION_BYTES],
    ) -> super::Result<BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.health()?;
        if self.fresh.plan.kind != super::super::super::BootstrapPlanKind::StatusBootstrap {
            return Err(unchanged(StorageCause::UnsupportedPlan));
        }
        let role = self.status_role()?;
        if !core.uses_fence(self.session_fence()) || !role.uses_fence(self.session_fence()) {
            return Err(unchanged(StorageCause::BindingMismatch));
        }
        if !fits::<W>() {
            return Err(unchanged(StorageCause::Capacity));
        }
        for frame in frames.iter_mut() {
            frame
                .check_empty()
                .map_err(|e| self.fatal(StorageCause::Frame(e)))?;
        }
        self.health()?;
        match self
            .binding
            .compare_exchange(AVAILABLE, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(CLAIMED | BOUND) => {
                self.health()?;
                return Err(unchanged(StorageCause::AlreadyBound));
            }
            Err(ABANDONED) => return Err(self.fatal(StorageCause::Fenced)),
            Err(_) => return Err(self.fatal(StorageCause::Invariant)),
        }
        let mut claim = BindingClaim {
            storage: self,
            complete: false,
        };
        let (base, status_after) = operation.split_at_mut(dirty_allocation::OPERATION_BYTES);
        let base: &mut [u8; dirty_allocation::OPERATION_BYTES] =
            base.try_into().expect("fixed base workspace");
        let status_after: &mut [u8; PAGE_BYTES] =
            status_after.try_into().expect("fixed Status workspace");
        let identity = self.read_allocator(&mut frames[0], base)?;
        frames[0]
            .register(allocator_request(identity))
            .map_err(|e| self.fatal(StorageCause::Frame(e)))?;
        let pool =
            SharedPool::new_with_closed_init(core, self.session_fence(), &self.binding, frames)
                .map_err(|e| self.fatal(StorageCause::Frame(e)))?;
        let bound = BoundStatusBootstrap {
            storage: self,
            pool,
            control: StatusControl {
                base: WriterControl::new_status(base, identity),
                status_after,
                role,
                installation: None,
            },
        };
        self.health()?;
        self.binding.store(BOUND, Ordering::Release);
        claim.complete = true;
        Ok(bound)
    }
}
fn allocator_request(identity: PageIdentity) -> TargetRequest {
    TargetRequest {
        identity,
        kind: PageKind::Allocation,
        level: 0,
    }
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn split<'b>(
        &'b mut self,
    ) -> (
        StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        StatusReader<'b, 'c, 's, 'p, 'd>,
    ) {
        let StatusControl {
            base,
            status_after,
            role,
            installation,
        } = &mut self.control;
        (
            StatusBootstrapWriter {
                base,
                status_after,
                role,
                installation,
                pool: &self.pool,
                storage: self.storage,
            },
            StatusReader {
                pool: &self.pool,
                role,
                profile: &self.storage.fresh.plan.profile,
                fence: self.storage.session_fence(),
            },
        )
    }
}
impl<W: WriterNamespace> Drop for BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        self.storage.session_fence().fence();
        self.storage.binding.store(ABANDONED, Ordering::Release);
        let _ = self.pool.has_pins();
    }
}

pub(crate) struct StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    base: &'b mut WriterControl<'s, 'o>,
    status_after: &'b mut &'o mut [u8; PAGE_BYTES],
    role: &'b StatusRole<'s>,
    installation: &'b mut Option<InitObligation<'c, 's>>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    storage: &'s MutableStorageOwner<'m, 'w, W>,
}
/// A linear reservation. Dropping it fences its owner; cancellation is explicit.
pub(crate) struct ReservedStatus<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    writer: &'r mut StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'r mut Owner<'c, 's>,
    batch: Batch,
    page: PageRef,
    ordinal: u16,
    active: bool,
}
impl<W: WriterNamespace> Drop for ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        if self.active {
            self.writer.storage.session_fence().fence();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InitializedStatusObservation {
    pub(crate) page: PageRef,
    pub(crate) lsn: PageLsn,
    pub(crate) allocator: FrameStamp,
    pub(crate) status: FrameStamp,
    pub(crate) wal: WalPrefix,
}

impl<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn fatal(&self, phase: Phase, cause: Cause) -> StatusInitError {
        self.storage.session_fence().fence();
        error(phase, Effect::CallerMustResolve, cause)
    }
    fn health(&self, phase: Phase) -> Result<()> {
        self.storage
            .health()
            .map_err(|e| error(phase, e.effect, Cause::Storage(e)))
    }
    fn allocator(&mut self) -> AllocationWriter<'_, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        AllocationWriter {
            control: self.base,
            pool: self.pool,
            storage: self.storage,
        }
    }
    fn uninitialized(&self, phase: Phase) -> Result<()> {
        self.health(phase)?;
        if self
            .pool
            .initialized_request()
            .map_err(|e| frame_error(phase, e))?
            .is_some()
        {
            return Err(error(phase, Effect::Unchanged, Cause::AlreadyInitialized));
        }
        if self.installation.is_some()
            || self.base.installation.is_some()
            || self.base.terminal_pending
        {
            return Err(self.fatal(phase, Cause::Invariant));
        }
        Ok(())
    }
    pub(crate) fn reserve_one<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
    ) -> Result<ReservedStatus<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let _unwind = self.storage.session_fence().unwind_guard();
        self.uninitialized(Phase::Reserve)?;
        self.allocator()
            .reserve_pages(owner, 1)
            .map_err(|e| error(Phase::Reserve, e.effect, Cause::Allocation(e)))?;
        // reserve_pages finished with a checked, current allocator snapshot in
        // private before-workspace. Extract its exact key without another pin
        // attempt after durability; no wrapperless Unchanged exit is possible.
        let batch = self
            .base
            .batch
            .ok_or_else(|| self.fatal(Phase::Reserve, Cause::Invariant))?;
        let page = self.retained_reserved_page(batch)?;
        Ok(ReservedStatus {
            writer: self,
            owner,
            batch,
            page,
            ordinal: 0,
            active: true,
        })
    }
    fn retained_reserved_page(&self, batch: Batch) -> Result<PageRef> {
        let before: &[u8; PAGE_BYTES] = self.base.operation[FRAME_BYTES..BEFORE_END]
            .try_into()
            .expect("fixed private snapshot");
        let page = allocation_page::decode_allocation_page(
            before,
            self.base.identity,
            self.storage.profile().spec().namespace,
        )
        .map_err(|e| self.fatal(Phase::Prepare, Cause::AllocationPage(e)))?;
        if batch.count() != 1 || batch.mask & 1 != 0 {
            return Err(self.fatal(Phase::Prepare, Cause::Invariant));
        }
        let index = batch.mask.trailing_zeros() as usize;
        let entry = page
            .entry(index)
            .map_err(|e| self.fatal(Phase::Prepare, Cause::AllocationPage(e)))?;
        let AllocationEntry::Reserved {
            generation,
            claimant,
        } = entry
        else {
            return Err(self.fatal(Phase::Prepare, Cause::Invariant));
        };
        if claimant != ReservationOwner::System(batch.owner)
            || page.header().lsn != PageLsn(batch.birth)
        {
            return Err(self.fatal(Phase::Prepare, Cause::Invariant));
        }
        Ok(PageRef {
            page_no: index as u64,
            incarnation: generation,
            ..self.base.identity.page
        })
    }
    fn authority(
        &self,
        batch: Batch,
        page: PageRef,
        ordinal: u16,
    ) -> Result<InitAuthority<'c, 's>> {
        self.uninitialized(Phase::Prepare)?;
        if self.base.batch != Some(batch)
            || batch.count() != 1
            || ordinal != 0
            || page.page_no >= 64
            || batch.mask != 1u64 << page.page_no
        {
            return Err(error(
                Phase::Prepare,
                Effect::Unchanged,
                Cause::StaleReservation,
            ));
        }
        if page != self.retained_reserved_page(batch)? {
            return Err(error(
                Phase::Prepare,
                Effect::Unchanged,
                Cause::StaleReservation,
            ));
        }
        if !self.role.uses_fence(self.storage.session_fence()) {
            return Err(error(
                Phase::Prepare,
                Effect::Unchanged,
                Cause::BindingMismatch,
            ));
        }
        Ok(InitAuthority {
            core: self.pool.bound_core(),
            binding: &self.storage.binding,
            fence: self.storage.session_fence(),
            batch,
            ordinal,
            allocator: allocator_request(self.base.identity),
            payload: TargetRequest {
                identity: PageIdentity {
                    page,
                    owner: self.role.object(),
                    owner_generation: self.role.generation(),
                    ..self.base.identity
                },
                kind: self.role.kind(),
                level: self.role.level(),
            },
        })
    }
}

impl<W: WriterNamespace> ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(crate) fn page(&self) -> PageRef {
        self.page
    }
    #[allow(
        clippy::result_large_err,
        reason = "before-effect failure owns the exact reservation; its inline size is admission-bounded without heap allocation"
    )]
    pub(crate) fn cancel(
        mut self,
    ) -> std::result::Result<AllocationObservation, InitAttemptError<Self>> {
        let result = self
            .writer
            .authority(self.batch, self.page, self.ordinal)
            .and_then(|_| {
                self.writer
                    .allocator()
                    .cancel_reserved(self.owner)
                    .map_err(|e| error(Phase::Reserve, e.effect, Cause::Allocation(e)))
            });
        match result {
            Ok(observed) => {
                self.active = false;
                Ok(observed)
            }
            Err(e) => Err(self.attempt_error(e)),
        }
    }
    #[allow(
        clippy::result_large_err,
        reason = "before-effect failure owns the exact reservation; its inline size is admission-bounded without heap allocation"
    )]
    pub(crate) fn initialize(
        mut self,
    ) -> std::result::Result<InitializedStatusObservation, InitAttemptError<Self>> {
        let result = self
            .writer
            .initialize(self.owner, self.batch, self.page, self.ordinal);
        match result {
            Ok(observed) => {
                self.active = false;
                Ok(observed)
            }
            Err(e) => Err(self.attempt_error(e)),
        }
    }
    fn attempt_error(self, mut e: StatusInitError) -> InitAttemptError<Self> {
        if e.effect == Effect::Unchanged && self.writer.installation.is_none() {
            if let Err(cleanup) = self.writer.health(Phase::Release) {
                return InitAttemptError::Fatal(cleanup);
            }
            return InitAttemptError::Before(BeforeEffectError {
                error: e,
                reservation: self,
            });
        }
        self.writer.storage.session_fence().fence();
        e.effect = Effect::CallerMustResolve;
        InitAttemptError::Fatal(e)
    }
}

impl<'s, 'c, W: WriterNamespace> StatusBootstrapWriter<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    fn initialize(
        &mut self,
        owner: &mut Owner<'c, 's>,
        batch: Batch,
        page: PageRef,
        ordinal: u16,
    ) -> Result<InitializedStatusObservation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let authority = self.authority(batch, page, ordinal)?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| error(Phase::Prepare, e.effect, Cause::Wal(e)))?;
        let mut write: dirty_frame::InitWriteTargets<'_, '_, '_, '_, '_, '_> = self
            .pool
            .claim_init_targets(owner, &authority)
            .map_err(|e| frame_error(Phase::Revalidate, e))?
            .try_write()
            .map_err(|e| frame_error(Phase::Revalidate, e))?;
        let installed = (|| -> Result<()> {
            let (frame, rest) = self.base.operation.split_at_mut(FRAME_BYTES);
            let (before, allocator_after) = rest.split_at_mut(PAGE_BYTES);
            let frame: &mut [u8; FRAME_BYTES] = frame.try_into().expect("fixed frame workspace");
            let before: &mut [u8; PAGE_BYTES] = before.try_into().expect("fixed before workspace");
            let allocator_after: &mut [u8; PAGE_BYTES] = allocator_after
                .try_into()
                .expect("fixed allocator-after workspace");
            let stamp = write
                .copy_allocator_snapshot(before)
                .map_err(|e| frame_error(Phase::Prepare, e))?;
            let prepared = {
                let plan = operation::InitPlan {
                    profile: storage.profile(),
                    allocator: self.base.identity,
                    payload: authority.payload.identity,
                    prefix,
                    batch,
                    before_descriptor: &self.base.descriptor,
                    role: self.role,
                };
                operation::prepare_init(&plan, before, frame, allocator_after, self.status_after)
                    .map_err(|e| {
                        storage.session_fence().fence();
                        error(
                            Phase::Prepare,
                            Effect::CallerMustResolve,
                            Cause::Semantic(e),
                        )
                    })?
            };
            let after_descriptor = prepared.descriptor_bytes();
            let allocator_after = prepared.allocator_after();
            let status_after = prepared.status_after();
            write
                .preflight(&stamp)
                .map_err(|e| frame_error(Phase::Revalidate, e))?;
            let requirement = prepared
                .stage_required(storage.wal())
                .map_err(|e| error(Phase::Stage, e.effect, Cause::Wal(e)))?;
            // C1 contains only infallible ownership moves. No hook, Result,
            // lock or destructor separates the stage from its retained owner.
            let key = write.retain_claim();
            *self.installation = Some(InitObligation {
                key,
                requirement,
                phase: InitInstallPhase::StageOwned,
            });
            self.base.terminal_pending = true;
            #[cfg(test)]
            dirty_frame::init_test_support::stage_owned(self.pool);
            let obligation = self
                .installation
                .as_mut()
                .expect("actual stage retains obligation");
            let permit = InitInstallPermit {
                key: &obligation.key,
                allocator_before: stamp,
                requirement,
                allocator_after,
                status_after,
            };
            let commit = InitControlCommit {
                descriptor: &mut self.base.descriptor,
                after_descriptor,
                phase: &mut obligation.phase,
            };
            write
                .prepare_install(&permit, commit)
                .map_err(|e| frame_error(Phase::Install, e))?
                .install();
            Ok(())
        })();
        let installed = installed.map_err(|mut e| {
            if self.installation.is_some() {
                storage.session_fence().fence();
                e.effect = Effect::CallerMustResolve;
            }
            e
        });
        merge_cleanup(
            installed,
            write
                .release_checked()
                .map_err(|e| frame_error(Phase::Release, e)),
        )?;
        // The only durable proof accepted below is the same stage requirement.
        // No content/version/admission/pin guard survives this boundary.
        let requirement = self
            .installation
            .as_ref()
            .expect("actual stage retains obligation")
            .requirement;
        #[cfg(test)]
        status_flush::before_init_force(&storage.binding);
        storage.wal().wait_durable(&requirement).map_err(|e| {
            storage.session_fence().fence();
            error(Phase::Sync, Effect::CallerMustResolve, Cause::Wal(e))
        })?;
        self.health(Phase::Sync)?;
        let (allocator_stamp, status_stamp) = {
            let reader = StatusReader {
                pool: self.pool,
                role: self.role,
                profile: &storage.fresh.plan.profile,
                fence: storage.session_fence(),
            };
            let read = reader.read(owner)?;
            let observed = read.stamps();
            merge_cleanup(observed, read.release_checked())?
        };
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| self.fatal(Phase::Observe, Cause::Wal(e)))?;
        let location = requirement.location();
        if allocator_stamp.page_lsn.0 != location.lsn
            || status_stamp.page_lsn.0 != location.lsn
            || prefix.complete_lsn != location.lsn
            || prefix.synced_lsn != location.lsn
            || prefix.complete_offset != prefix.synced_offset
            || self
                .installation
                .as_ref()
                .is_none_or(|o| o.phase != InitInstallPhase::Published)
            || self.base.batch != Some(batch)
            || !self.base.terminal_pending
        {
            return Err(self.fatal(Phase::Observe, Cause::Invariant));
        }
        // Descriptor decoder temporaries do not overlap the reader's codecs.
        operation::validate_finished(
            storage.profile(),
            batch,
            &self.base.descriptor,
            PageLsn(location.lsn),
        )
        .map_err(|e| self.fatal(Phase::Observe, Cause::Semantic(e)))?;
        let completed = CompletedInitAuthority { authority };
        let obligation = self
            .installation
            .as_mut()
            .expect("retained until checked finish");
        self.pool
            .finish_init_claim(&mut obligation.key, &completed)
            .map_err(|e| {
                storage.session_fence().fence();
                frame_error(Phase::Release, e)
            })?;
        *self.installation = None;
        self.base.batch = None;
        self.base.terminal_pending = false;
        Ok(InitializedStatusObservation {
            page,
            lsn: PageLsn(location.lsn),
            allocator: allocator_stamp,
            status: status_stamp,
            wal: prefix,
        })
    }
}

/// Shared reads acquire the exact allocator and published payload together.
/// Unpublished checks never lock the DPT or content, including the C5 interval.
pub(crate) struct StatusReader<'b, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    role: &'b StatusRole<'s>,
    profile: &'s Profile,
    fence: &'s SessionFence,
}
pub(crate) struct StatusReadGuard<'a, 'b, 'c, 's, 'p, 'd> {
    targets: ReadTargets<'a, 'b, 'c, 's, 'p, 'd>,
    _role: &'b StatusRole<'s>,
}
impl<'b, 'c, 's, 'p, 'd> StatusReader<'b, 'c, 's, 'p, 'd> {
    pub(crate) fn read<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
    ) -> Result<StatusReadGuard<'a, 'b, 'c, 's, 'p, 'd>> {
        let read = {
            // Admission-only Copy requests end before any codec/hash runs.
            let payload = self
                .pool
                .initialized_request()
                .map_err(|e| frame_error(Phase::Observe, e))?
                .ok_or_else(|| error(Phase::Observe, Effect::Unchanged, Cause::NotInitialized))?;
            let allocator = *self
                .pool
                .target_request(0)
                .map_err(|e| frame_error(Phase::Observe, e))?;
            let requests = [allocator, payload];
            self.pool
                .reserve_targets(owner, &requests)
                .map_err(|e| frame_error(Phase::Observe, e))?
                .try_read()
                .map_err(|e| frame_error(Phase::Observe, e))?
        };
        let checked = self.validate(&read);
        if let Err(e) = checked {
            return merge_cleanup(
                Err(e),
                read.release_checked()
                    .map_err(|e| frame_error(Phase::Release, e)),
            );
        }
        Ok(StatusReadGuard {
            targets: read,
            _role: self.role,
        })
    }
    fn validate(&self, read: &ReadTargets<'_, '_, '_, '_, '_, '_>) -> Result<()> {
        let allocator_request = read
            .target_request(0)
            .map_err(|e| frame_error(Phase::Observe, e))?;
        let payload_request = read
            .target_request(1)
            .map_err(|e| frame_error(Phase::Observe, e))?;
        let allocator = read.bytes(0).map_err(|e| frame_error(Phase::Observe, e))?;
        let status = read.bytes(1).map_err(|e| frame_error(Phase::Observe, e))?;
        let lsn = operation::observe_initialized(
            *self.profile,
            allocator_request.identity,
            payload_request.identity,
            self.role,
            allocator,
            status,
        )
        .map_err(|e| {
            self.fence.fence();
            error(
                Phase::Observe,
                Effect::CallerMustResolve,
                Cause::Semantic(e),
            )
        })?;
        let allocator = read
            .observe(0)
            .map_err(|e| frame_error(Phase::Observe, e))?;
        let status = read
            .observe(1)
            .map_err(|e| frame_error(Phase::Observe, e))?;
        if allocator.frame.page_lsn != lsn
            || status.frame.page_lsn != lsn
            || allocator.frame.identity != allocator_request.identity
            || status.frame.identity != payload_request.identity
        {
            self.fence.fence();
            return Err(error(
                Phase::Observe,
                Effect::CallerMustResolve,
                Cause::Invariant,
            ));
        }
        Ok(())
    }
}
impl StatusReadGuard<'_, '_, '_, '_, '_, '_> {
    /// The one canonical Free cell was checked while both guards were acquired.
    /// Mutation and future transaction admission are deliberately absent here.
    pub(crate) fn bytes(&self) -> Result<&[u8; PAGE_BYTES]> {
        self.targets
            .bytes(1)
            .map_err(|e| frame_error(Phase::Observe, e))
    }
    pub(crate) fn stamps(&self) -> Result<(FrameStamp, FrameStamp)> {
        let allocator = self
            .targets
            .observe(0)
            .map_err(|e| frame_error(Phase::Observe, e))?
            .frame;
        let status = self
            .targets
            .observe(1)
            .map_err(|e| frame_error(Phase::Observe, e))?
            .frame;
        Ok((allocator, status))
    }
    pub(crate) fn release_checked(self) -> Result<()> {
        self.targets
            .release_checked()
            .map_err(|e| frame_error(Phase::Release, e))
    }
}

fn sum_fits(parts: &[usize]) -> bool {
    parts
        .iter()
        .try_fold(0usize, |sum, part| sum.checked_add(*part))
        .is_some_and(|sum| sum <= 65536)
}

/// Upper values live across native INIT calls. The native source ledger reserves
/// one 768-byte slot for these values; plan/codec scopes end before installation.
fn init_caller_bytes<W: WriterNamespace>() -> usize {
    size_of::<ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<InitAuthority<'_, '_>>()
        + size_of::<WalPrefix>()
        + size_of::<Batch>()
        + size_of::<PageRef>()
        + 8 // ordinal and return discriminant carriers
        + 8 * size_of::<&()>() // writer/owner/storage/unwind and branch carriers
}

/// Conservative source-value bound. It is neither an ABI-stack nor an RSS
/// measurement; generic owner, reader and File padding are charged before CAS.
fn fits<W: WriterNamespace>() -> bool {
    let owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    let control = size_of::<StatusControl<'_, '_, '_>>();
    let reader = size_of::<PageFileReader<'_, W>>();
    let file = size_of::<W::File>();
    let plans = operation::plan_bytes();
    let temporaries = operation::temporary_bytes();
    let pool_delta = dirty_frame::init_pool_delta_bytes();
    let control_delta = control.saturating_sub(dirty_allocation::baseline_control_bytes());
    let hot = 60672 + pool_delta + control_delta + 34 * READ_CALL + INIT_EXCLUSIVE;
    let cold = 60672
        + pool_delta
        + control_delta
        + 90112
        + (5 * pool_delta + 2048).max(3 * pool_delta + 8 * control_delta + 2048);
    pool_delta <= 512
        && dirty_allocation::base_delta() <= 64
        && control_delta <= 1536
        && hot <= 196608
        && cold <= 196608
        && operation::reader_bytes() <= READ_CALL
        && dirty_frame::fits()
        && dirty_frame::init_fits()
        && dirty_allocation::typed_fits()
        && operation::fits()
        && super::super::super::super::durability_types_fit()
        && owner <= 32768
        && file <= 16384
        && size_of::<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>() <= 28672
        && size_of::<super::Result<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>>() <= 28672
        && size_of::<InitObligation<'_, '_>>() <= 768
        && size_of::<Option<InitObligation<'_, '_>>>() <= 768
        && size_of::<InitInstallPermit<'_, '_, '_>>() <= 768
        && size_of::<Result<InitInstallPermit<'_, '_, '_>>>() <= 768
        && size_of::<ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>() <= 256
        && size_of::<InitAttemptError<ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>()
            <= 512
        && size_of::<InitializedStatusObservation>() <= 768
        && size_of::<Result<InitializedStatusObservation>>() <= 768
        && size_of::<InitializedStatusObservation>()
            + size_of::<Result<InitializedStatusObservation>>()
            <= 768
        && init_caller_bytes::<W>() <= 768
        && size_of::<InitInstallPermit<'_, '_, '_>>()
            + size_of::<Result<InitInstallPermit<'_, '_, '_>>>()
            + size_of::<InitControlCommit<'_>>()
            <= 768
        && size_of::<StatusReadGuard<'_, '_, '_, '_, '_, '_>>()
            .saturating_sub(size_of::<ReadTargets<'_, '_, '_, '_, '_, '_>>())
            <= 128
        && budget::CONSTRUCTOR <= 16384
        && plans <= 4096
        && temporaries <= 16384
        && sum_fits(&[owner, reader, file, budget::CONSTRUCTOR, 32])
        && sum_fits(&[owner, control, plans, temporaries, INIT_EXCLUSIVE, 2048])
        && sum_fits(&[owner, control, plans, INIT_EXCLUSIVE, 4096, 2048])
        && sum_fits(&[
            owner,
            control,
            INIT_EXCLUSIVE,
            READ_CALL.max(budget::TEMPORARIES),
        ])
        && sum_fits(&[owner, control, READ_CALL])
        && sum_fits(&[owner, control, 4096, 2048])
}
