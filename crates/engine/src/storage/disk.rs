//! Canonical internal API for consumers of the disk engine.
//! Currently exposes C0 producer observations/loans and untrusted candidates;
//! the original session/ports are supplied; Row reservation/install supply is pending.

#[allow(
    unused_imports,
    reason = "C0 boundary awaits the separately owned Row consumers"
)]
pub(crate) use super::wal_io::disk_api::{
    CallerCredit, CatalogCallError, CatalogCallScope, CatalogCancelled, CatalogCandidate,
    CatalogCommit, CatalogCreateError, CatalogCreated, CatalogReadOperation, CatalogStatementCall,
    CheckedTable, DiskSession, EmptyRetentionControlLoan, ISOLATION_BYTES, IntoRetentionError,
    IsolationLevel, MaintenancePort, MissingCatalogEvidence, MissingRowEvidence, MutationPort,
    Plan4Recovered, Plan4RecoveryError, Plan4RecoveryProgress, Plan4ResidencyCursor, Plan4Target,
    PreflightRow, PreparedCatalog, PreparedRecoveryRow, PreparedRollbackTask, PreparedRow,
    ROW_PREFLIGHT_CALLER_BYTES, ROW_WRITE_CALLER_BYTES, ReadOnlyCommit, ReadPort, ReadStep,
    RecoveryAction, RecoveryOwnedError, RecoveryRowTask, ReservedCatalogCreate, ReservedRow,
    RetentionCause, RetentionError, RetentionFlushPhase, RetentionPhase, RollbackScope,
    RollbackStep, RowApplied, RowCancelled, RowCandidate, RowCommitObservation,
    RowControlObservation, RowCopyObservation, RowInputPlan, RowMutationObservation, RowOperation,
    RowOwnedError, RowPageRole, RowPreflightContext, RowPreflightStep, RowProducerContext,
    RowProducerInput, RowProducerLane, RowProposal, RowReadOperation, RowRollback, RowRollbackDone,
    RowRollbackTask, RowSearchPage, RowSourceBinding, RowSourceRole, RowStatementView,
    RowTransaction, RowValidationLane, StatementFinishError, TableStep, TransactionFinishError,
    TransactionOptions, row_write_charge,
};

#[allow(unused_imports)]
pub(in crate::storage) use super::wal_io::disk_api::{
    EPOCH_BYTES, VISIBILITY_BYTES, VisibilityLoan,
};

pub(in crate::storage) use super::wal_io::disk_api::InstallGrant;
pub(in crate::storage) use super::wal_io::disk_api::Owner;

pub(crate) use super::wal_io::disk_api::{
    DurableCommit, InstalledCommit, StagedCommit, UnstagedCommit,
};
