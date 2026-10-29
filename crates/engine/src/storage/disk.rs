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
    Plan4Recovered, Plan4ResidencyCursor, Plan4Target, PreflightRow, PreparedCatalog,
    PreparedRollbackTask, PreparedRow, ReadOnlyCommit, ReadPort, ReadStep, ReservedCatalogCreate,
    ReservedRow, RetentionCause, RetentionError, RetentionPhase, RollbackScope, RollbackStep,
    RowApplied, RowCancelled, RowCandidate, RowCommitObservation, RowControlObservation,
    RowCopyObservation, RowInputPlan, RowMutationObservation, RowOperation, RowOwnedError,
    RowPageRole, RowPreflightContext, RowProducerContext, RowProducerInput, RowProducerLane,
    RowProposal, RowReadOperation, RowRollback, RowRollbackDone, RowRollbackTask, RowSearchPage,
    RowSourceBinding, RowSourceRole, RowStatementView, RowTransaction, RowValidationLane,
    StatementFinishError, TransactionFinishError, TransactionOptions,
};

#[allow(unused_imports)]
pub(in crate::storage) use super::wal_io::disk_api::{
    EPOCH_BYTES, VISIBILITY_BYTES, VisibilityLoan,
};

pub(in crate::storage) use super::wal_io::disk_api::InstallGrant;

pub(crate) use super::wal_io::disk_api::{
    DurableCommit, InstalledCommit, StagedCommit, UnstagedCommit,
};
