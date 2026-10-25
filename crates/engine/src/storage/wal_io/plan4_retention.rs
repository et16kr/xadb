//! Live N=0 retention over the original plan4 owner. The consuming conversion
//! keeps the pool, binding, WAL runtime, lease and every original caller loan.
//! A decoder observation never constructs this owner.

use super::{Plan4Page, Plan4Pages, Plan4ResidencyCursor, Plan4Target};
use crate::{
    storage::{
        dirty_frame::{self, FrameStamp, SharedPool, TargetRequest},
        page::{PAGE_BYTES, PageIdentity, PageLsn},
        retention_page::{self, JobSlotRef},
        semantic_retention::RetentionSemanticError,
        transaction_record::{StatusSlotRef, TxnRef},
        wal_io::{DurableWal, MutableStorageOwner, WalIoError, WalRequirement},
    },
    transaction::concurrency::Owner,
};
use foundation::{error::Effect, namespace::WriterNamespace};

#[path = "plan4_retention/control.rs"]
mod control;
#[path = "plan4_retention/flush.rs"]
mod flush;
#[path = "plan4_retention/history.rs"]
mod history;
#[path = "plan4_retention/live.rs"]
mod live;
#[path = "plan4_retention/load.rs"]
mod load;
#[path = "plan4_retention/population.rs"]
mod population;
#[path = "plan4_retention/read.rs"]
mod read;
#[path = "plan4_retention/recovery.rs"]
mod recovery;
#[cfg(test)]
#[path = "plan4_retention/recovery_observation_tests.rs"]
mod recovery_test_observation;
#[path = "plan4_retention/resources.rs"]
mod resources;

#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(crate) use flush::{RetentionFlushPhase, RetentionFlushStep, RetentionFlusher};
pub(in crate::storage) use history::RetentionFlushAuthority;
pub(in crate::storage) use live::RetentionInstall;
pub(crate) use live::RetentionWriter;
pub(in crate::storage) use load::RetentionLoadPermit;
#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(crate) use population::{ReservedRetentionStatus, RetentionReservationError};
pub(in crate::storage) use population::{
    RetentionInitAuthority, RetentionInitCommit, RetentionInitCompleted, RetentionInitInstall,
};
#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(crate) use read::{RetentionReadGuard, RetentionReader};
pub(in crate::storage) use recovery::{
    RetentionFrameObservation, RetentionRecoveryStep, RetentionReplayCursor,
    observe_recovery_frame, retention_replay_bank, retention_replay_image, seed_recovery_replay,
};
#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(in crate::storage) use resources::{RetentionSourceCensus, source_census};

pub(crate) const RETENTION_CONTROL_BYTES: usize = 196_608;
pub(crate) const RETENTION_REPLAY_BYTES: usize = 1_572_864;
const FRAME_BYTES: usize = 65_536;
const DWB_REPLAY_OFFSET: usize = 196_608;

/// Exclusive parts of the original maintenance loan, separate from the WAL
/// slot, operation images, DWB backing and pool pages. No allocation is hidden.
pub(crate) struct EmptyRetentionControlLoan<'a> {
    pub(crate) control: &'a mut [u8; RETENTION_CONTROL_BYTES],
    pub(crate) replay: &'a mut [u8; RETENTION_REPLAY_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionPhase {
    Activate,
    Prepare,
    Census,
    Stage,
    Install,
    Durable,
    Source,
    Load,
    Flush,
    Finalize,
}
#[derive(Debug)]
pub(crate) enum RetentionCause {
    Busy,
    Capacity,
    Exhausted,
    BindingMismatch,
    Membership,
    NotResident,
    WrongState,
    MissingTransaction,
    Invariant,
    Fenced,
    Poisoned,
    Frame(dirty_frame::DirtyError),
    Page(crate::storage::page::PageError),
    Tree(crate::storage::tree_page::TreeError),
    Allocation(crate::storage::allocation_page::AllocationError),
    AllocationOperation(super::super::super::allocation_reservation::operation::SemanticError),
    Retention(retention_page::RetentionError),
    Semantic(RetentionSemanticError),
    Rollback(crate::storage::semantic_group::SemanticError),
    Wal(WalIoError),
    Storage(crate::storage::wal_io::MutableStorageError),
    Plan4(super::Plan4PageError),
    PageIo(crate::storage::page_io::PageIoError),
    Namespace(foundation::namespace::NamespaceError),
    Io(foundation::Error),
    #[allow(
        private_interfaces,
        reason = "retain original private executor error; no shared authority visibility expansion"
    )]
    Flush(super::super::flush_io::FlushIoError),
}
#[derive(Debug)]
pub(crate) struct RetentionError {
    pub(crate) phase: RetentionPhase,
    pub(crate) cause: RetentionCause,
    pub(crate) effect: Effect,
}
type Result<T> = std::result::Result<T, RetentionError>;
fn unchanged(phase: RetentionPhase, cause: RetentionCause) -> RetentionError {
    RetentionError {
        phase,
        cause,
        effect: Effect::Unchanged,
    }
}
fn fatal<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    phase: RetentionPhase,
    cause: RetentionCause,
) -> RetentionError {
    storage.session_fence().fence();
    storage.wal().completion.notify_all();
    RetentionError {
        phase,
        cause,
        effect: Effect::CallerMustResolve,
    }
}
fn frame(phase: RetentionPhase, error: dirty_frame::DirtyError) -> RetentionError {
    RetentionError {
        phase,
        effect: error.effect,
        cause: RetentionCause::Frame(error),
    }
}
fn merge<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Err(e), _) if e.effect == Effect::CallerMustResolve => Err(e),
        (_, Err(e)) => Err(e),
        (result, Ok(())) => result,
    }
}

pub(crate) enum IntoRetentionError<B> {
    Before { error: RetentionError, bound: B },
    Fatal(RetentionError),
}

pub(crate) struct Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    control: control::RetentionControl<'a, 'c, 's>,
    flush: flush::RetentionFlushControl<'c, 's, 'w>,
    recovery_binding: Option<super::super::plan4_recovery::RecoveryRetentionBinding<'s, 's>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetentionObservation {
    pub(crate) actor: Option<TxnRef>,
    pub(crate) status: Option<StatusSlotRef>,
    pub(crate) job: Option<JobSlotRef>,
    pub(crate) lsn: PageLsn,
    pub(crate) issued_local: u64,
    pub(crate) adopted_local: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionResidencyStep {
    Resident {
        index: usize,
        frame: FrameStamp,
        loaded: bool,
    },
    FlushRequired {
        target: Plan4Target,
        rec_lsn: PageLsn,
    },
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    /// The borrowed validation phase ends with a small Result. No new owner,
    /// control, flusher or wide conversion Result exists during its queue scan.
    fn preflight_empty_retention(&mut self, owner: &mut Owner<'c, 's>) -> Result<()> {
        let storage = self.storage;
        if !resources::fits::<W>() {
            return Err(unchanged(
                RetentionPhase::Activate,
                RetentionCause::Capacity,
            ));
        }
        self.ready(super::Plan4PagePhase::Preflight)
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Activate,
                effect: e.effect,
                cause: RetentionCause::Plan4(e),
            })?;
        if self._flush_workspace.is_none() {
            return Err(fatal(
                storage,
                RetentionPhase::Activate,
                RetentionCause::Invariant,
            ));
        }
        // The actual old consumer checks every queue cell through the same
        // four/four/three-target windows before any new mode is activated.
        self.inspect_genesis_retention(owner, &mut Plan4ResidencyCursor::new())
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Activate,
                effect: e.effect,
                cause: RetentionCause::Plan4(e),
            })?;
        super::pristine(storage, super::Plan4PagePhase::Preflight).map_err(|e| RetentionError {
            phase: RetentionPhase::Activate,
            effect: e.effect,
            cause: RetentionCause::Plan4(e),
        })?;
        self.pool
            .activate_plan4_retention()
            .map_err(|e| frame(RetentionPhase::Activate, e))
    }

    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the exact inline owner"
    )]
    pub(crate) fn into_empty_retention<'a>(
        mut self,
        owner: &mut Owner<'c, 's>,
        loan: EmptyRetentionControlLoan<'a>,
    ) -> std::result::Result<
        Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        IntoRetentionError<Self>,
    > {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let prepared = self.preflight_empty_retention(owner);
        match prepared {
            Ok(()) => {
                // No fallible call follows the mode change. These are moves of
                // existing exclusive loans, not new bindings or provenance.
                let workspace = self
                    ._flush_workspace
                    .take()
                    .expect("checked original DWB loan");
                let mut control = control::RetentionControl::new(loan);
                control.initialize_jobs(storage.profile().spec().namespace);
                let flush = flush::RetentionFlushControl::new(storage, workspace);
                Ok(Plan4Retention {
                    inner: self,
                    control,
                    flush,
                    recovery_binding: None,
                })
            }
            Err(error) if error.effect == Effect::Unchanged => {
                Err(IntoRetentionError::Before { error, bound: self })
            }
            Err(error) => Err(IntoRetentionError::Fatal(error)),
        }
    }
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn split<'b>(
        &'b mut self,
    ) -> (
        RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RetentionReader<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
        RetentionFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        (
            RetentionWriter {
                storage: self.inner.storage,
                pool: &self.inner.pool,
                operation: &mut self.inner.operation,
                control: &mut self.control,
            },
            RetentionReader {
                storage: self.inner.storage,
                pool: &self.inner.pool,
            },
            RetentionFlusher {
                storage: self.inner.storage,
                pool: &self.inner.pool,
                control: &mut self.flush,
            },
        )
    }
    pub(crate) fn target(&self, role: Plan4Page) -> Result<Plan4Target> {
        let index = match role {
            Plan4Page::Root => 1,
            Plan4Page::Jobs { index } if index < 5 => usize::from(index) + 2,
            Plan4Page::CatalogAnchor => 7,
            Plan4Page::CatalogLeaf => 8,
            _ => {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Membership,
                ));
            }
        };
        let h =
            super::genesis::owned_header(&self.inner.storage.core.plan, index).map_err(|_| {
                fatal(
                    self.inner.storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                )
            })?;
        Ok(Plan4Target {
            request: TargetRequest {
                identity: h.identity,
                kind: h.kind,
                level: h.level,
            },
        })
    }
    fn health(&self) -> Result<()> {
        health(self.inner.storage, &self.inner.pool)?;
        if self.control.pending.is_some() || self.control.population.reserved() {
            return Err(fatal(
                self.inner.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
}
fn health<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    pool: &SharedPool<'_, '_, '_, '_>,
) -> Result<()> {
    storage
        .health()
        .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Storage(e)))?;
    if !pool.retention_payloads()
        || !pool.uses_storage_origin(&storage.binding, storage.session_fence())
    {
        return Err(fatal(
            storage,
            RetentionPhase::Prepare,
            RetentionCause::BindingMismatch,
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "plan4_retention/tests.rs"]
mod tests;
