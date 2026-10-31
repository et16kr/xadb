//! One original mutable owner and pool for plan4's exact clean genesis graph.
//! No dirty issuer, flusher, selector promotion or transaction authority escapes.

use super::super::super::{BootstrapPlanKind, plan4_genesis as genesis};
use super::{
    ABANDONED, AVAILABLE, BOUND, BindingClaim, CLAIMED, MutableStorageError, MutableStorageOwner,
    wal,
};
use crate::{
    storage::{
        allocation_page::AllocationError,
        dirty_frame::{self, DirtyError, DirtyFrame, FrameStamp, SharedPool, TargetRequest},
        page::{PAGE_BYTES, PageError, PageIdentity, PageKind, PageLsn},
        page_io::PageIoError,
        retention_genesis::RetentionGenesisError,
        tree_page::TreeError,
    },
    transaction::concurrency::{Owner, PageConcurrency},
};
use foundation::{error::Effect, identity::ObjectId, namespace::WriterNamespace};
use std::sync::{TryLockError, atomic::Ordering};

#[cfg(not(test))]
#[path = "plan4_apply.rs"]
pub(in crate::storage::wal_io) mod apply;
#[cfg(test)]
#[path = "plan4_apply.rs"]
pub(in crate::storage::wal_io) mod retention;
// Compile the same source and visibility under the original unit-test module
// name so frozen selectors retain their source identity. No behavior differs.
#[cfg(test)]
pub(in crate::storage::wal_io) use retention as apply;

#[path = "plan4_pages/load.rs"]
mod load;
#[path = "plan4_pages/read.rs"]
mod read;
#[path = "plan4_pages/resources.rs"]
mod resources;
pub(in crate::storage) use load::Plan4LoadPermit;
#[cfg(test)]
pub(in crate::storage) use resources::source_census;
pub(in crate::storage) use resources::source_census as plan4_recovery_clean_source_census;

#[cfg(all(test, target_os = "linux"))]
#[path = "plan4_pages/native_tests.rs"]
pub(in crate::storage) mod native;
#[cfg(test)]
#[path = "plan4_pages/tests.rs"]
pub(in crate::storage) mod tests;

#[cfg(not(test))]
use apply as retention;
pub(in crate::storage) use retention::CatalogRetirementRejected;
#[allow(
    unused_imports,
    reason = "internal live owner export awaits recovered integration"
)]
pub(crate) use retention::Plan4Retention;
pub(in crate::storage) use retention::{
    EmptyRetentionControlLoan, RetentionCause, RetentionFlushAuthority, RetentionFlushStep,
    RetentionFrameObservation, RetentionInitAuthority, RetentionInitCommit, RetentionInitCompleted,
    RetentionInitInstall, RetentionInstall, RetentionLoadPermit, RetentionRecoveryStep,
    RetentionReplayCursor, VerifiedLookupLoad, observe_recovery_frame, retention_replay_bank,
    retention_replay_cells, retention_replay_digest, retention_replay_image,
    retention_replay_losers, retention_replay_provenance, retention_replay_row_scope,
    seed_recovery_replay,
};

#[cfg(test)]
pub(in crate::storage) use retention::{
    CatalogLookupStep, IntoRetentionError, RETENTION_CONTROL_BYTES, RETENTION_REPLAY_BYTES,
    RetentionFlushPhase, RetentionPhase, RetentionReservationError, RetentionResidencyStep,
    retention_ledger_check, retention_replay_closure,
};

pub(in crate::storage) use retention::source_census as retention_recovery_source_census;

const OPERATION_BYTES: usize = 7 * PAGE_BYTES;
const FLUSH_BYTES: usize = 4_194_304;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4Page {
    Root,
    Jobs { index: u8 },
    CatalogAnchor,
    CatalogLeaf,
}

/// An expected value. The owning bound revalidates membership on every use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan4Target {
    request: TargetRequest,
}
impl Plan4Target {
    pub(crate) fn identity(&self) -> PageIdentity {
        self.request.identity
    }
    pub(crate) fn kind(&self) -> PageKind {
        self.request.kind
    }
    pub(crate) fn level(&self) -> u8 {
        self.request.level
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan4ResidencyCursor {
    next: usize,
}
impl Plan4ResidencyCursor {
    pub(crate) const fn new() -> Self {
        Self { next: 1 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan4Resident {
    pub(crate) index: usize,
    pub(crate) frame: FrameStamp,
    pub(crate) loaded: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan4LoadObservation {
    pub(crate) before: Option<FrameStamp>,
    pub(crate) after: FrameStamp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4CatalogLookup {
    NotFound,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan4GenesisCensus {
    pub(crate) revision: u64,
    pub(crate) free_jobs: u16,
    pub(crate) job_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4PagePhase {
    Preflight,
    Bind,
    Load,
    Read,
    Census,
}
#[derive(Debug)]
pub(crate) enum Plan4PageCause {
    UnsupportedPlan,
    BindingMismatch,
    AlreadyBound,
    Shape,
    Capacity,
    Busy,
    AlreadyResident,
    Stale,
    Membership,
    Invariant,
    Fenced,
    Poisoned,
    Storage(MutableStorageError),
    Genesis(genesis::Plan4GenesisCause),
    Frame(DirtyError),
    PageIo(PageIoError),
    Allocation(AllocationError),
    Page(PageError),
    Retention(RetentionGenesisError),
    Tree(TreeError),
}
#[derive(Debug)]
pub(crate) struct Plan4PageError {
    pub(crate) phase: Plan4PagePhase,
    pub(crate) cause: Plan4PageCause,
    pub(crate) effect: Effect,
}
type Result<T> = std::result::Result<T, Plan4PageError>;

fn unchanged(phase: Plan4PagePhase, cause: Plan4PageCause) -> Plan4PageError {
    Plan4PageError {
        phase,
        cause,
        effect: Effect::Unchanged,
    }
}
fn fatal<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    phase: Plan4PagePhase,
    cause: Plan4PageCause,
) -> Plan4PageError {
    storage.session_fence().fence();
    Plan4PageError {
        phase,
        cause,
        effect: Effect::CallerMustResolve,
    }
}
fn frame(phase: Plan4PagePhase, error: DirtyError) -> Plan4PageError {
    Plan4PageError {
        phase,
        effect: error.effect,
        cause: Plan4PageCause::Frame(error),
    }
}
fn storage_error(phase: Plan4PagePhase, error: MutableStorageError) -> Plan4PageError {
    Plan4PageError {
        phase,
        effect: error.effect,
        cause: Plan4PageCause::Storage(error),
    }
}

/// No pool/page guard borrows this struct itself. All backing remains caller owned.
pub(crate) struct Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: SharedPool<'c, 's, 'p, 'd>,
    operation: &'o mut [u8; OPERATION_BYTES],
    // Kept once for a future consuming activation; this unit cannot flush.
    _flush_workspace: Option<&'w mut [u8; FLUSH_BYTES]>,
}

fn pristine<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    phase: Plan4PagePhase,
) -> Result<wal::WalPrefix> {
    // This live bound retains the original Fresh/WAL and writer exclusion.
    // Its checked in-memory prefix is revalidated; this is not a reopen selector.
    storage
        .health()
        .map_err(|error| storage_error(phase, error))?;
    let prefix = storage.wal().prefix().map_err(|error| {
        fatal(
            storage,
            phase,
            Plan4PageCause::Genesis(genesis::Plan4GenesisCause::Wal(error)),
        )
    })?;
    if storage.core.plan.kind != BootstrapPlanKind::RetentionCatalogBootstrap
        || storage.core.reader_active.load(Ordering::Acquire)
        || prefix.complete_lsn != 0
        || prefix.synced_lsn != 0
        || prefix.complete_offset != 4096
        || prefix.synced_offset != 4096
        || prefix.stream.writer_epoch != 1
        || storage.profile() != storage.plan.profile
        || storage.core.plan.selected != storage.plan.filemap
    {
        return Err(fatal(storage, phase, Plan4PageCause::Invariant));
    }
    let append = storage.wal().state(wal::Phase::Observe).map_err(|error| {
        fatal(
            storage,
            phase,
            Plan4PageCause::Genesis(genesis::Plan4GenesisCause::Wal(error)),
        )
    })?;
    if append.state != wal::SlotState::Idle || append.slot.is_none() {
        return Err(fatal(storage, phase, Plan4PageCause::Invariant));
    }
    Ok(prefix)
}

impl<'m, 'w, W: WriterNamespace> MutableStorageOwner<'m, 'w, W> {
    pub(crate) fn bind_plan4_pages<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        core: &'c PageConcurrency<'s>,
        frames: &'p mut [DirtyFrame<'d>],
        operation: &'o mut [u8; OPERATION_BYTES],
    ) -> Result<Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let p = Plan4PagePhase::Preflight;
        self.health().map_err(|error| storage_error(p, error))?;
        if self.core.plan.kind != BootstrapPlanKind::RetentionCatalogBootstrap {
            return Err(unchanged(p, Plan4PageCause::UnsupportedPlan));
        }
        if !core.uses_fence(self.session_fence()) {
            return Err(unchanged(p, Plan4PageCause::BindingMismatch));
        }
        if !(4..=dirty_frame::MAX_FRAMES).contains(&frames.len()) {
            return Err(unchanged(p, Plan4PageCause::Shape));
        }
        if !resources::fits::<W>() {
            return Err(unchanged(p, Plan4PageCause::Capacity));
        }
        for value in frames.iter_mut() {
            value
                .check_empty()
                .map_err(|error| fatal(self, p, Plan4PageCause::Frame(error)))?;
        }
        self.health().map_err(|error| storage_error(p, error))?;
        match self
            .binding
            .compare_exchange(AVAILABLE, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(CLAIMED | BOUND) => {
                self.health().map_err(|error| storage_error(p, error))?;
                return Err(unchanged(p, Plan4PageCause::AlreadyBound));
            }
            Err(ABANDONED) => return Err(fatal(self, p, Plan4PageCause::Fenced)),
            Err(_) => return Err(fatal(self, p, Plan4PageCause::Invariant)),
        }
        let mut claim = BindingClaim {
            storage: self,
            complete: false,
        };
        // This helper returns before the large pool constructor/return values exist.
        let identity = read_bound_genesis(self, &mut frames[0], operation)?;
        frames[0]
            .register(TargetRequest {
                identity,
                kind: PageKind::Allocation,
                level: 0,
            })
            .map_err(|error| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Frame(error)))?;
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Poisoned))?
            .take()
            .ok_or_else(|| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Invariant))?;
        let pool = SharedPool::new_plan4(core, self.session_fence(), &self.binding, frames)
            .map_err(|error| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Frame(error)))?;
        let bound = Plan4Pages {
            storage: self,
            pool,
            operation,
            _flush_workspace: Some(workspace),
        };
        self.health()
            .map_err(|error| storage_error(Plan4PagePhase::Bind, error))?;
        self.binding.store(BOUND, Ordering::Release);
        claim.complete = true;
        Ok(bound)
    }
}

fn read_bound_genesis<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    frame: &mut DirtyFrame<'_>,
    operation: &mut [u8; OPERATION_BYTES],
) -> Result<PageIdentity> {
    let p = Plan4PagePhase::Bind;
    let prefix = pristine(storage, p)?;
    let identity = storage.core.plan.allocation_identity().map_err(|error| {
        fatal(
            storage,
            p,
            Plan4PageCause::Genesis(genesis::Plan4GenesisCause::Control(error)),
        )
    })?;
    let output = frame
        .initial_payload()
        .map_err(|error| fatal(storage, p, Plan4PageCause::Frame(error)))?;
    {
        let scratch = operation[..3 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("three bind pages");
        let mut writer = match storage.wal()._writer.try_lock() {
            Ok(writer) => writer,
            Err(TryLockError::WouldBlock) => return Err(fatal(storage, p, Plan4PageCause::Busy)),
            Err(TryLockError::Poisoned(_)) => {
                return Err(fatal(storage, p, Plan4PageCause::Poisoned));
            }
        };
        genesis::verify_owned_genesis(&storage.core.plan, &mut *writer, scratch, output)
            .map_err(|cause| fatal(storage, p, Plan4PageCause::Genesis(cause)))?;
    }
    if pristine(storage, p)? != prefix {
        return Err(fatal(storage, p, Plan4PageCause::Invariant));
    }
    Ok(identity)
}

impl<W: WriterNamespace> Plan4Pages<'_, '_, '_, '_, '_, '_, '_, W> {
    fn ready(&self, phase: Plan4PagePhase) -> Result<()> {
        self.storage
            .health()
            .map_err(|error| storage_error(phase, error))?;
        if !self
            .pool
            .plan4_payloads()
            .map_err(|error| frame(phase, error))?
            || !self
                .pool
                .uses_storage_origin(&self.storage.binding, self.storage.session_fence())
            || self.storage.binding.load(Ordering::Acquire) != BOUND
        {
            return Err(fatal(self.storage, phase, Plan4PageCause::Invariant));
        }
        if !resources::fits::<W>() {
            return Err(unchanged(phase, Plan4PageCause::Capacity));
        }
        Ok(())
    }
    pub(crate) fn target(&self, page: Plan4Page) -> Result<Plan4Target> {
        self.ready(Plan4PagePhase::Preflight)?;
        let index = match page {
            Plan4Page::Root => 1,
            Plan4Page::Jobs { index } if index < 5 => usize::from(index) + 2,
            Plan4Page::Jobs { .. } => {
                return Err(unchanged(Plan4PagePhase::Preflight, Plan4PageCause::Shape));
            }
            Plan4Page::CatalogAnchor => 7,
            Plan4Page::CatalogLeaf => 8,
        };
        let header = genesis::owned_header(&self.storage.core.plan, index).map_err(|cause| {
            fatal(
                self.storage,
                Plan4PagePhase::Preflight,
                Plan4PageCause::Genesis(cause),
            )
        })?;
        Ok(Plan4Target {
            request: TargetRequest {
                identity: header.identity,
                kind: header.kind,
                level: header.level,
            },
        })
    }
    fn checked_request(
        &self,
        expected: &Plan4Target,
        phase: Plan4PagePhase,
    ) -> Result<TargetRequest> {
        let request = expected.request;
        let Ok(index) = usize::try_from(request.identity.page.page_no) else {
            return Err(unchanged(phase, Plan4PageCause::Membership));
        };
        let header = genesis::owned_header(&self.storage.core.plan, index)
            .map_err(|_| unchanged(phase, Plan4PageCause::Membership))?;
        if request
            != (TargetRequest {
                identity: header.identity,
                kind: header.kind,
                level: header.level,
            })
        {
            return Err(unchanged(phase, Plan4PageCause::Membership));
        }
        Ok(request)
    }
    fn allocator_request(&self, phase: Plan4PagePhase) -> Result<TargetRequest> {
        let identity = self
            .storage
            .core
            .plan
            .allocation_identity()
            .map_err(|error| {
                fatal(
                    self.storage,
                    phase,
                    Plan4PageCause::Genesis(genesis::Plan4GenesisCause::Control(error)),
                )
            })?;
        Ok(TargetRequest {
            identity,
            kind: PageKind::Allocation,
            level: 0,
        })
    }
}

impl<W: WriterNamespace> Drop for Plan4Pages<'_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        self.storage.session_fence().fence();
        self.storage.binding.store(ABANDONED, Ordering::Release);
        let _ = self.pool.has_pins();
    }
}

#[path = "plan4_pages/recovered_bind.rs"]
mod recovered_bind;

#[cfg(test)]
pub(in crate::storage) use retention::FreeUndoOracle;
#[cfg(test)]
pub(in crate::storage) use retention::install_after_capture;

#[allow(unused_imports)] // Same original owner capabilities across the private module chain.
pub(in crate::storage) use retention::{
    AppliedRetirement, BoundVisibility, CapturedCatalogRefs, CatalogLookupLoan, CatalogSource,
    EPOCH_BYTES, HorizonObservation, InstalledFrontier, ReferenceLease, RetirementIntent,
    RetirementReject, RetirementScope, RollbackSource, SnapshotGuard, SnapshotReservation,
    SourceFrontier, StagedRetirement, TxPairAuthority, TxPairCancelKind, TxPairCancelPermit,
    TxPairControlCommit, TxPairInstallPermit, TxPairRetirePermit, TxPairStageRejection, TxnPin,
    UndoPin, VISIBILITY_BYTES, VerifiedRetirement, VisibilityCause, VisibilityError,
    VisibilityLoan, VisibilityResult,
};
