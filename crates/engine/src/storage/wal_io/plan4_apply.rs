//! The original plan4 disk session and its restricted borrowed ports.
//! Existing control/history/flush helpers remain private descendants during
//! migration. Row producer data types carry observations and untrusted
//! candidates; their actual source/reservation/install adapters are pending.

pub(crate) use super::super::plan4_recovery::Plan4Recovered;
use super::{Plan4Page, Plan4Pages, Plan4ResidencyCursor, Plan4Target};
use crate::storage::session_fence::SessionFence;
use crate::{
    storage::{
        dirty_frame::{self, FrameStamp, SharedPool, TargetRequest},
        page::{PAGE_BYTES, PageIdentity, PageLsn},
        retention_page::{self, JobSlotRef},
        semantic_retention::RetentionSemanticError,
        transaction_record::{StatusSlotRef, TxnRef},
        wal_io::{DurableWal, MutableStorageOwner, WalIoError, WalRequirement, WalRuntime},
    },
    transaction::concurrency::{InstallationKey, Owner, OwnerIdentity, PageConcurrency},
};
use foundation::{error::Effect, namespace::WriterNamespace};
use std::sync::{Mutex, atomic::AtomicU8};

/// Borrow the original physical WAL; semantic terminal authority belongs here.
struct TerminalWal<'s, 'm, W: WriterNamespace> {
    wal: &'s WalRuntime<'m, W>,
}
#[derive(Clone, Copy)]
struct Binding<'c, 'f> {
    core: &'c PageConcurrency<'f>,
    key: InstallationKey,
    requirement: WalRequirement<'f>,
    semantic_digest: [u8; 32],
}
pub(crate) struct StagedCommit<'c, 'f> {
    binding: Binding<'c, 'f>,
}
pub(crate) struct DurableCommit<'c, 'f> {
    binding: Binding<'c, 'f>,
    durable: DurableWal<'f>,
}
pub(crate) struct InstalledCommit<'c, 'f> {
    binding: Binding<'c, 'f>,
}
/// The original stage call alone seals an Unchanged rejection; an upper error
/// code cannot clear the core stage-call marker.
pub(crate) struct UnstagedCommit<'c, 'f> {
    core: &'c PageConcurrency<'f>,
    key: InstallationKey,
}
/// Original staged binding retained during PinSet suspension, without locks.
struct CatalogCommitWait<'c, 's> {
    binding: Binding<'c, 's>,
}
struct CatalogStageError<'c, 's> {
    error: WalIoError,
    unstaged: Option<UnstagedCommit<'c, 's>>,
}
/// Actual input created only after the original core enters StageCallStarted.
struct CatalogCommitAttempt<'a, 'i, 'c, 's> {
    prepared: &'a PreparedCatalog<'i, 'c, 's>,
    key: InstallationKey,
}
/// Actual whole page/control/history installation, without a public factory.
struct CatalogCommitInstalled<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    key: InstallationKey,
    requirement: WalRequirement<'s>,
    digest: [u8; 32],
    q: u64,
}
#[path = "plan4_apply/terminal.rs"]
mod terminal;

/// An immutable image sealed by an actual apply producer after complete WAL
/// stage. The installer can observe it but cannot construct or clone it.
pub(in crate::storage) struct InstallGrant<'i, 'c, 's> {
    kind: InstallKind<'i, 'c, 's>,
}
enum InstallKind<'i, 'c, 's> {
    Plan4 {
        core: &'c crate::transaction::concurrency::PageConcurrency<'s>,
        stamp: FrameStamp,
        requirement: WalRequirement<'s>,
        after_counter: u64,
        after: &'i [u8; PAGE_BYTES],
    },
    Legacy(super::super::dirty_allocation::InstallPermit<'i, 'c, 's>),
}
#[path = "plan4_apply/install.rs"]
mod install;
#[path = "plan4_apply/legacy.rs"]
pub(in crate::storage::wal_io) mod legacy;

#[path = "plan4_apply/isolation.rs"]
mod isolation;
#[path = "plan4_apply/source.rs"]
mod source;
#[path = "plan4_apply/transaction.rs"]
mod transaction;
#[allow(unused_imports)]
pub(crate) use transaction::{
    IsolationLevel, ReadOnlyCommit, RowStatementView, RowTransaction, StatementFinishError,
    TransactionFinishError, TransactionOptions,
};

#[path = "plan4_retention/create_pair.rs"]
mod create_pair;
#[path = "plan4_retention/visibility.rs"]
mod visibility;
pub(in crate::storage) use create_pair::CatalogRetirementRejected;
use create_pair::catalog_live::CatalogPayload;

/// No Clone/Copy and no raw constructor. This borrows immutable private
/// operation bytes, not replay/control/page guards or a second storage owner.
pub(crate) struct PreparedCatalog<'i, 'c, 's> {
    payload: CatalogPayload<'i, 'c, 's>,
    owner: OwnerIdentity,
    requests: [TargetRequest; 9],
    roles: [u8; 9],
    count: usize,
    stamps: [Option<FrameStamp>; 9],
    before: &'i [[u8; PAGE_BYTES]; 8],
}

pub(in crate::storage) use create_pair::{
    TxPairAuthority, TxPairCancelKind, TxPairCancelPermit, TxPairControlCommit,
    TxPairInstallPermit, TxPairRetirePermit, TxPairStageRejection,
};
#[allow(unused_imports)]
pub(in crate::storage) use visibility::{
    AppliedRetirement, BoundVisibility, CapturedCatalogRefs, CatalogLookupLoan, CatalogSource,
    EPOCH_BYTES, HorizonObservation, InstalledFrontier, ReferenceLease, RetirementIntent,
    RetirementReject, RetirementScope, RollbackSource, SnapshotGuard, SnapshotReservation,
    SourceFrontier, StagedRetirement, TxnPin, UndoPin, VISIBILITY_BYTES, VerifiedRetirement,
    VisibilityCause, VisibilityError, VisibilityLoan, VisibilityResult,
};

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
#[path = "plan4_retention/lookup_load.rs"]
mod lookup_load;
pub(in crate::storage) use lookup_load::VerifiedLookupLoad;
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
pub(crate) use flush::{RetentionFlushPhase, RetentionFlushStep};
pub(in crate::storage) use history::RetentionFlushAuthority;
pub(in crate::storage) use live::RetentionInstall;
pub(in crate::storage) use load::RetentionLoadPermit;
#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(crate) use population::{
    ReservedRetentionStatus, ReservedRetentionSystem, RetentionReservationError,
};
pub(in crate::storage) use population::{
    RetentionInitAuthority, RetentionInitCommit, RetentionInitCompleted, RetentionInitInstall,
};
#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(crate) use read::{
    CatalogCallError, CatalogCallScope, CatalogCandidate, CatalogLookupStep, CatalogReadOperation,
    CatalogStatementCall, MissingCatalogEvidence, ReadStep, RetentionReadGuard,
};
pub(in crate::storage) use recovery::{
    RetentionFrameObservation, RetentionRecoveryStep, RetentionReplayCursor,
    observe_recovery_frame, retention_replay_bank, retention_replay_cells, retention_replay_digest,
    retention_replay_image, retention_replay_provenance, seed_recovery_replay,
};
#[allow(
    unused_imports,
    reason = "internal typed consumer exports retained for follow-on integration"
)]
pub(in crate::storage) use resources::{RetentionSourceCensus, source_census};

pub(crate) const RETENTION_CONTROL_BYTES: usize = 196_608;
pub(crate) const RETENTION_REPLAY_BYTES: usize = 1_572_864;
pub(crate) const ISOLATION_BYTES: usize = 4_160;
const FRAME_BYTES: usize = 65_536;
const DWB_REPLAY_OFFSET: usize = 196_608;

/// Exclusive parts of the original maintenance loan, separate from the WAL
/// slot, operation images, DWB backing and pool pages. No allocation is hidden.
pub(crate) struct EmptyRetentionControlLoan<'a> {
    pub(crate) control: &'a mut [u8; RETENTION_CONTROL_BYTES],
    pub(crate) replay: &'a mut [u8; RETENTION_REPLAY_BYTES],
    pub(crate) isolation: &'a mut [u8; ISOLATION_BYTES],
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
    SerializationConflict,
    Invariant,
    Unsupported,
    Fenced,
    Poisoned,
    Frame(dirty_frame::DirtyError),
    Concurrency(crate::transaction::concurrency::ConcurrencyError),
    Page(crate::storage::page::PageError),
    Tree(crate::storage::tree_page::TreeError),
    Catalog(crate::storage::catalog_record::CatalogError),
    Undo(crate::storage::undo_record::UndoError),
    Allocation(crate::storage::allocation_page::AllocationError),
    AllocationOperation(super::super::super::allocation_reservation::operation::SemanticError),
    Retention(retention_page::RetentionError),
    Semantic(RetentionSemanticError),
    CatalogSemantic(crate::storage::semantic_catalog::Error),
    #[allow(
        private_interfaces,
        reason = "preserve the original storage visibility failure"
    )]
    Visibility(visibility::VisibilityError),
    #[allow(
        private_interfaces,
        reason = "preserve the original inventory/base codec cause without exposing its authority scope"
    )]
    Base(crate::storage::plan4_recovery_base::Error),
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

pub(crate) struct DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    control: RetentionControl<'a, 'c, 's>,
    history: history::runtime::RuntimeHistory<'a, 'c, 's>,
    flush: flush::RetentionFlushControl<'c, 's, 'w>,
    catalog_visibility: Option<visibility::BoundVisibility<'a, 'c, 's>>,
    recovery_binding: Option<super::super::plan4_recovery::RecoveryRetentionBinding<'s, 's>>,
}

/// The original durable ledger and its separate caller-backed isolation state.
/// Splitting the session lends the ledger exclusively and the mutex jointly.
struct RetentionControl<'a, 'c, 's> {
    ledger: control::RetentionLedger<'a, 'c, 's>,
    isolation: Mutex<IsolationLedger<'a, 'c, 's>>,
}

#[allow(
    dead_code,
    reason = "C1 backing/origin supply; transaction and credit operations follow"
)]
struct IsolationLedger<'a, 'c, 's> {
    bytes: &'a mut [u8; ISOLATION_BYTES],
    core: &'c crate::transaction::concurrency::PageConcurrency<'s>,
    binding: &'s std::sync::atomic::AtomicU8,
    fence: &'s crate::storage::session_fence::SessionFence,
}

pub(crate) struct ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    source_charges: source::CatalogSourceCharges,
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    #[allow(
        dead_code,
        reason = "shared original isolation backing for pending Row read adapter"
    )]
    isolation: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
    visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
}

pub(crate) struct MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    commit_charges: source::CatalogCommitCharges,
    retirement_charge: source::CatalogRetirementCharge,
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    operation: &'b mut &'o mut [u8; super::OPERATION_BYTES],
    control: &'b mut control::RetentionLedger<'a, 'c, 's>,
    history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    isolation: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
    visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
}

pub(crate) struct MaintenancePort<'b, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    control: &'b mut flush::RetentionFlushControl<'c, 's, 'w>,
}

// Compatibility names for unchanged internal helpers and existing callers.
// New Catalog/Row consumers import only the canonical storage::disk names.
pub(crate) use self::{
    DiskSession as Plan4Retention, MaintenancePort as RetentionFlusher,
    MutationPort as RetentionWriter, ReadPort as RetentionReader,
};

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
    pub(in crate::storage) fn into_disk_session<'a>(
        mut self,
        owner: &mut Owner<'c, 's>,
        loan: EmptyRetentionControlLoan<'a>,
    ) -> std::result::Result<DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, IntoRetentionError<Self>>
    {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let seed_prefix = storage.wal().prefix().map_err(|error| {
            IntoRetentionError::Fatal(fatal(
                storage,
                RetentionPhase::Activate,
                RetentionCause::Wal(error),
            ))
        })?;
        let prepared = self.preflight_empty_retention(owner);
        match prepared {
            Ok(()) => {
                // No fallible call follows the mode change. These are moves of
                // existing exclusive loans, not new bindings or provenance.
                let workspace = self
                    ._flush_workspace
                    .take()
                    .expect("checked original DWB loan");
                let mut ledger = control::RetentionLedger::new(loan.control);
                let history = history::runtime::RuntimeHistory::new(
                    storage,
                    self.pool.bound_core(),
                    recovery::HistorySource::Genesis,
                    seed_prefix,
                    loan.replay,
                );
                ledger.initialize_jobs(storage.profile().spec().namespace);
                let control = RetentionControl {
                    ledger,
                    isolation: Mutex::new(IsolationLedger::new(
                        storage,
                        self.pool.bound_core(),
                        loan.isolation,
                    )),
                };
                let flush = flush::RetentionFlushControl::new(storage, workspace);
                Ok(DiskSession {
                    inner: self,
                    control,
                    history,
                    flush,
                    catalog_visibility: None,
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

// Existing callers retain their source API while moving the same owner through
// the canonical conversion. No second owner or duplicate loan is constructed.
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the original inline owner"
    )]
    pub(crate) fn into_empty_retention<'a>(
        self,
        owner: &mut Owner<'c, 's>,
        loan: EmptyRetentionControlLoan<'a>,
    ) -> std::result::Result<DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, IntoRetentionError<Self>>
    {
        self.into_disk_session(owner, loan)
    }
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn split<'b>(
        &'b mut self,
    ) -> (
        MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
        MaintenancePort<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        // No previously split port can coexist with this exclusive session
        // borrow. Complete the recursive type census before lending any port.
        let source_charges = source::CatalogSourceCharges::new::<W>();
        let commit_charges = source::CatalogCommitCharges::new::<W>();
        let retirement_charge = source::CatalogRetirementCharge::new();
        (
            MutationPort {
                commit_charges,
                retirement_charge,
                storage: self.inner.storage,
                pool: &self.inner.pool,
                operation: &mut self.inner.operation,
                control: &mut self.control.ledger,
                history: &self.history,
                isolation: &self.control.isolation,
                visibility: self.catalog_visibility.as_ref(),
            },
            ReadPort {
                source_charges,
                storage: self.inner.storage,
                pool: &self.inner.pool,
                history: &self.history,
                isolation: &self.control.isolation,
                visibility: self.catalog_visibility.as_ref(),
            },
            MaintenancePort {
                storage: self.inner.storage,
                pool: &self.inner.pool,
                control: &mut self.flush,
            },
        )
    }
    pub(crate) fn target(&self, role: Plan4Page) -> Result<Plan4Target> {
        target_from_source(self.inner.storage, role)
    }
    fn health(&self) -> Result<()> {
        health(self.inner.storage, &self.inner.pool)?;
        if self.control.ledger.pending.is_some() || self.control.ledger.population.reserved() {
            return Err(fatal(
                self.inner.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
}
fn target_from_source<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    role: Plan4Page,
) -> Result<Plan4Target> {
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
    let h = super::genesis::owned_header(&storage.core.plan, index)
        .map_err(|_| fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant))?;
    Ok(Plan4Target {
        request: TargetRequest {
            identity: h.identity,
            kind: h.kind,
            level: h.level,
        },
    })
}

fn health<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    pool: &SharedPool<'_, '_, '_, '_>,
) -> Result<()> {
    storage
        .health()
        .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Storage(e)))?;
    if !pool
        .retention_payloads()
        .map_err(|error| frame(RetentionPhase::Prepare, error))?
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
#[cfg(test)]
pub(in crate::storage) use tests::install_after_capture;

#[cfg(test)]
#[path = "plan4_retention/free_undo_literal_tests.rs"]
mod free_undo_literals;
#[cfg(test)]
pub(in crate::storage) use free_undo_literals::Oracle as FreeUndoOracle;

#[cfg(test)]
#[path = "plan4_retention/free_undo_tests.rs"]
mod free_undo_tests;

#[cfg(test)]
#[path = "plan4_retention/catalog_source_tests.rs"]
mod catalog_source_tests;

use crate::{
    api::Namespace,
    storage::{
        catalog_record::CatalogExpectation,
        control::Profile,
        page::PageKind,
        tree_page::TreeExpectation,
        undo_record::UndoLink,
        undo_ref::UndoRef,
        wal_frame::{FrameExpectation, WalFrameHeader},
    },
};
use foundation::identity::{ObjectHandle, RowId, SchemaEpoch};

/// Metadata cut observed by the trusted preflight adapter. A Catalog decoder
/// alone cannot construct this cut, a checked table or a reservation.
#[derive(Debug)]
pub(crate) struct RowPreflightContext {
    profile: Profile,
    table: ObjectHandle,
    catalog_expected: CatalogExpectation,
    schema_cell: (u16, u16),
    leaf_expected: TreeExpectation,
    leaf_free_bytes: usize,
    canonical_capacity: usize,
    index_count: u16,
    catalog_source: (FrameStamp, [u8; 32]),
}

impl RowPreflightContext {
    pub(in crate::storage) fn profile(&self) -> Profile {
        self.profile
    }
    pub(in crate::storage) fn namespace(&self) -> Namespace {
        self.catalog_expected.namespace
    }
    pub(in crate::storage) fn table(&self) -> ObjectHandle {
        self.table
    }
    pub(in crate::storage) fn schema_epoch(&self) -> SchemaEpoch {
        self.table.schema_epoch
    }
    pub(in crate::storage) fn catalog_expected(&self) -> CatalogExpectation {
        self.catalog_expected
    }
    pub(in crate::storage) fn schema_cell(&self) -> (u16, u16) {
        self.schema_cell
    }
    pub(in crate::storage) fn leaf_expected(&self) -> TreeExpectation {
        self.leaf_expected
    }
    pub(in crate::storage) fn leaf_free_bytes(&self) -> usize {
        self.leaf_free_bytes
    }
    pub(in crate::storage) fn canonical_capacity(&self) -> usize {
        self.canonical_capacity
    }
    pub(in crate::storage) fn index_count(&self) -> u16 {
        self.index_count
    }
    pub(in crate::storage) fn catalog_source(&self) -> (FrameStamp, [u8; 32]) {
        self.catalog_source
    }
}

/// Logical roles can alias one physical before image, with distinct cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowSourceRole {
    Root,
    RootJob,
    TerminalJob,
    Status,
    Undo,
    RowLeaf,
    Allocator,
    TableAnchor,
    PreviousUndo,
}

impl RowSourceRole {
    const fn index(self) -> usize {
        match self {
            Self::Root => 0,
            Self::RootJob => 1,
            Self::TerminalJob => 2,
            Self::Status => 3,
            Self::Undo => 4,
            Self::RowLeaf => 5,
            Self::Allocator => 6,
            Self::TableAnchor => 7,
            Self::PreviousUndo => 8,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RowSourceBinding {
    before_index: u8,
    identity: PageIdentity,
    kind: PageKind,
    level: u8,
    // All three are present together; partial cell coordinates are impossible.
    cell: Option<(u16, u16, u16)>,
}

impl RowSourceBinding {
    pub(in crate::storage) fn before_index(&self) -> u8 {
        self.before_index
    }
    pub(in crate::storage) fn expected(&self) -> (PageIdentity, PageKind, u8) {
        (self.identity, self.kind, self.level)
    }
    pub(in crate::storage) fn cell(&self) -> Option<(u16, u16, u16)> {
        self.cell
    }
}

/// The two mutations produced by A. These discriminants are not WAL opcodes;
/// common Begin/Reserve/terminal/maintenance effects remain with the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowOperation {
    Forward,
    Clr,
}

/// Changed physical-page roles for a Forward/CLR proposal, not wire tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowPageRole {
    Root,
    RootJob,
    Status,
    Undo,
    RowLeaf,
}

#[derive(Debug)]
pub(crate) enum RowProducerInput {
    Insert {
        canonical_len: usize,
        canonical_digest: [u8; 32],
    },
    Clr {
        current_row_len: usize,
        current_projection: [u8; 32],
        rollback_target: Option<UndoRef>,
        next: Option<UndoLink>,
    },
}

/// Read-only scalar cut. No field, getter or byte digest is an install grant.
#[derive(Debug)]
pub(crate) struct RowProducerContext {
    schema: RowPreflightContext,
    operation: RowOperation,
    frame_expected: FrameExpectation,
    header: WalFrameHeader,
    actor: TxnRef,
    status: StatusSlotRef,
    row: RowId,
    undo: UndoLink,
    statement_sequence: Option<u64>,
    planned_mutation: u64,
    statement_mark: Option<UndoRef>,
    predecessor: Option<UndoLink>,
    root_job_ordinal: u8,
    terminal_job_ordinal: u8,
    before_count: u8,
    sources: [Option<RowSourceBinding>; 9],
    after_order: [RowPageRole; 5],
    // Actual adapters must establish this bound before constructing the cut.
    after_count: u8,
    input: RowProducerInput,
}

impl RowProducerContext {
    pub(in crate::storage) fn schema(&self) -> &RowPreflightContext {
        &self.schema
    }
    pub(in crate::storage) fn operation(&self) -> RowOperation {
        self.operation
    }
    pub(in crate::storage) fn frame_expected(&self) -> FrameExpectation {
        self.frame_expected
    }
    pub(in crate::storage) fn header(&self) -> WalFrameHeader {
        self.header
    }
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    pub(in crate::storage) fn status(&self) -> StatusSlotRef {
        self.status
    }
    pub(in crate::storage) fn row(&self) -> RowId {
        self.row
    }
    pub(in crate::storage) fn undo(&self) -> UndoLink {
        self.undo
    }
    pub(in crate::storage) fn statement_sequence(&self) -> Option<u64> {
        self.statement_sequence
    }
    pub(in crate::storage) fn planned_mutation(&self) -> u64 {
        self.planned_mutation
    }
    pub(in crate::storage) fn statement_mark(&self) -> Option<UndoRef> {
        self.statement_mark
    }
    pub(in crate::storage) fn predecessor(&self) -> Option<UndoLink> {
        self.predecessor
    }
    pub(in crate::storage) fn root_job_ordinal(&self) -> u8 {
        self.root_job_ordinal
    }
    pub(in crate::storage) fn terminal_job_ordinal(&self) -> u8 {
        self.terminal_job_ordinal
    }
    pub(in crate::storage) fn before_count(&self) -> u8 {
        self.before_count
    }
    pub(in crate::storage) fn source(&self, role: RowSourceRole) -> Option<&RowSourceBinding> {
        self.sources[role.index()].as_ref()
    }
    pub(in crate::storage) fn after_order(&self) -> &[RowPageRole] {
        &self.after_order[..usize::from(self.after_count)]
    }
    pub(in crate::storage) fn input(&self) -> &RowProducerInput {
        &self.input
    }
}

/// Borrows the original ledger, not a cloned 64 KiB bank or a second owner.
#[derive(Debug)]
pub(crate) struct RowControlObservation<'l> {
    jobs: &'l [[u8; 1_024]; 64],
    transaction: Option<&'l [u8; 1_536]>,
    aging: Option<&'l [u8; 1_536]>,
}

impl<'l> RowControlObservation<'l> {
    pub(in crate::storage) fn jobs(&self) -> &'l [[u8; 1_024]; 64] {
        self.jobs
    }
    pub(in crate::storage) fn transaction(&self) -> Option<&'l [u8; 1_536]> {
        self.transaction
    }
    pub(in crate::storage) fn aging(&self) -> Option<&'l [u8; 1_536]> {
        self.aging
    }
}

/// Consumed to split a short preflight loan. This is not a reservation.
pub(crate) struct RowValidationLane<'l> {
    context: RowPreflightContext,
    schema: &'l [u8; PAGE_BYTES],
    leaf: &'l [u8; PAGE_BYTES],
    candidate: &'l mut [u8; PAGE_BYTES],
}

impl<'l> RowValidationLane<'l> {
    pub(in crate::storage) fn parts(
        self,
    ) -> (
        RowPreflightContext,
        &'l [u8; PAGE_BYTES],
        &'l [u8; PAGE_BYTES],
        &'l mut [u8; PAGE_BYTES],
    ) {
        (self.context, self.schema, self.leaf, self.candidate)
    }
}

/// An untrusted candidate. The actual reserve adapter must recheck these fields
/// against source/schema/leaf state and the caller's encoded bytes.
#[derive(Debug)]
pub(crate) struct RowInputPlan {
    encoded_len: usize,
    schema_epoch: SchemaEpoch,
    digest: [u8; 32],
}

impl RowInputPlan {
    pub(in crate::storage) fn new(
        encoded_len: usize,
        schema_epoch: SchemaEpoch,
        digest: [u8; 32],
    ) -> Self {
        Self {
            encoded_len,
            schema_epoch,
            digest,
        }
    }
}

/// Disjoint slices borrowed from one private loan. Consuming this lane neither
/// advances WAL nor installs pages; those effects require the original owner.
pub(crate) struct RowProducerLane<'l> {
    context: RowProducerContext,
    control: RowControlObservation<'l>,
    before: &'l [[u8; PAGE_BYTES]; 9],
    schema: &'l [u8; PAGE_BYTES],
    frame: &'l mut [u8; 98_304],
    after: &'l mut [[u8; PAGE_BYTES]; 5],
    candidate: &'l mut [u8; PAGE_BYTES],
}

impl<'l> RowProducerLane<'l> {
    #[allow(
        clippy::type_complexity,
        reason = "one consuming split preserves all disjoint loan lifetimes"
    )]
    pub(in crate::storage) fn parts(
        self,
    ) -> (
        RowProducerContext,
        RowControlObservation<'l>,
        &'l [[u8; PAGE_BYTES]; 9],
        &'l [u8; PAGE_BYTES],
        &'l mut [u8; 98_304],
        &'l mut [[u8; PAGE_BYTES]; 5],
        &'l mut [u8; PAGE_BYTES],
    ) {
        (
            self.context,
            self.control,
            self.before,
            self.schema,
            self.frame,
            self.after,
            self.candidate,
        )
    }
}

/// Producer-declared metadata, deliberately not validated by this constructor.
/// Trusted prepare/stage must validate bytes, physical order, count and source.
#[derive(Debug)]
pub(crate) struct RowProposal {
    operation: RowOperation,
    frame_len: usize,
    after_count: usize,
    after_roles: [RowPageRole; 5],
}

impl RowProposal {
    pub(in crate::storage) fn new(
        operation: RowOperation,
        frame_len: usize,
        after_count: usize,
        after_roles: [RowPageRole; 5],
    ) -> Self {
        Self {
            operation,
            frame_len,
            after_count,
            after_roles,
        }
    }
}

#[cfg(test)]
#[path = "plan4_apply/owner_tests.rs"]
mod owner_tests;
