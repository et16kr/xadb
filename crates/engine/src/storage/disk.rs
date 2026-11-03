//! Canonical internal API for consumers of the disk engine.
//! Supplies the session and ports, the Catalog read/create/commit calls, the Row
//! preflight, reservation, prepare/install, rollback and recovery operations,
//! commit states, C0 producer observations/loans and untrusted candidates.

#[allow(
    unused_imports,
    reason = "full consumer surface; some items are named only by tests or by one \
              Row module, or reach consumers as unnamed return values"
)]
pub(crate) use super::wal_io::disk_api::{
    CATALOG_LOOKUP_RESULT_BYTES, CATALOG_LOOKUP_STEP_BYTES, CATALOG_PRODUCT_SOURCE_BYTES,
    CallerCredit, CatalogCallError, CatalogCallScope, CatalogCancelled, CatalogCandidate,
    CatalogCommit, CatalogCreateError, CatalogCreated, CatalogReadOperation, CatalogReadStep,
    CatalogStatementCall, CatalogVisibility, CheckedTable, DiskSession, EmptyRetentionControlLoan,
    ISOLATION_BYTES, IntoRetentionError, IsolationLevel, MaintenancePort, MissingCatalogEvidence,
    MissingRowEvidence, MutationPort, Plan4Recovered, Plan4RecoveryError, Plan4RecoveryProgress,
    Plan4ResidencyCursor, Plan4Target, PreflightRow, PreparedCatalog, PreparedRecoveryRow,
    PreparedRollbackTask, PreparedRow, ROW_PREFLIGHT_CALLER_BYTES, ROW_WRITE_CALLER_BYTES,
    ReadOnlyCommit, ReadPort, ReadStep, RecoveryAction, RecoveryOwnedError, RecoveryRowTask,
    ReservedCatalogCreate, ReservedRow, RetentionCause, RetentionError, RetentionFlushPhase,
    RetentionPhase, RollbackScope, RollbackStep, RowApplied, RowCancelled, RowCandidate,
    RowCommitObservation, RowControlObservation, RowCopyObservation, RowDeleteStep, RowInputPlan,
    RowMutationObservation, RowOperation, RowOwnedError, RowPageRole, RowPreflightContext,
    RowPreflightOperation, RowPreflightStep, RowPreparedOperation, RowProducerContext,
    RowProducerInput, RowProducerLane, RowProposal, RowReadOperation, RowReservedOperation,
    RowRollback, RowRollbackDone, RowRollbackTask, RowSearchPage, RowSourceBinding, RowSourceRole,
    RowStatementView, RowTransaction, RowValidationLane, StatementFinishError, TableStep,
    TransactionFinishError, TransactionOptions, row_write_charge,
};

#[allow(unused_imports)]
pub(in crate::storage) use super::wal_io::disk_api::{
    EPOCH_BYTES, VISIBILITY_BYTES, VisibilityLoan,
};

#[cfg(test)]
#[allow(unused_imports, reason = "used by the C+A selection only")]
pub(in crate::storage) use super::wal_io::disk_api::{MCA01_V12_P_TABLE, mca01_v12_p_census};

pub(in crate::storage) use super::wal_io::disk_api::InstallGrant;
pub(in crate::storage) use super::wal_io::disk_api::Owner;
#[allow(
    unused_imports,
    reason = "named only by A's cleanup drain; the C+B export has no A"
)]
pub(in crate::storage) use super::wal_io::disk_api::RowRetireStep;

pub(crate) use super::wal_io::disk_api::{
    DurableCommit, InstalledCommit, StagedCommit, UnstagedCommit,
};

#[cfg(test)]
pub(in crate::storage) use super::wal_io::disk_api::SnapshotGuard;
