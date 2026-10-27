//! Canonical internal API for consumers of the disk engine.
//! Currently exposes C0 producer observations/loans and untrusted candidates;
//! the original session/ports are supplied; Row reservation/install supply is pending.

#[allow(
    unused_imports,
    reason = "C0 boundary awaits the separately owned Row consumers"
)]
pub(crate) use super::wal_io::disk_api::{
    CatalogCallError, CatalogCallScope, CatalogCandidate, CatalogReadOperation,
    CatalogStatementCall, DiskSession, EmptyRetentionControlLoan, ISOLATION_BYTES,
    IntoRetentionError, IsolationLevel, MaintenancePort, MissingCatalogEvidence, MutationPort,
    Plan4Recovered, Plan4ResidencyCursor, Plan4Target, ReadOnlyCommit, ReadPort, ReadStep,
    RetentionCause, RetentionError, RetentionPhase, RowControlObservation, RowInputPlan,
    RowOperation, RowPageRole, RowPreflightContext, RowProducerContext, RowProducerInput,
    RowProducerLane, RowProposal, RowSourceBinding, RowSourceRole, RowStatementView,
    RowTransaction, RowValidationLane, StatementFinishError, TransactionFinishError,
    TransactionOptions,
};

#[allow(unused_imports)]
pub(in crate::storage) use super::wal_io::disk_api::{
    EPOCH_BYTES, VISIBILITY_BYTES, VisibilityLoan,
};

pub(in crate::storage) use super::wal_io::disk_api::InstallGrant;
