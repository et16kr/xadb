//! Canonical internal API for consumers of the disk engine.
//! Currently exposes C0 producer observations/loans and untrusted candidates;
//! the original session/ports are supplied; Row reservation/install supply is pending.

#[allow(
    unused_imports,
    reason = "C0 boundary awaits the separately owned Row consumers"
)]
pub(crate) use super::wal_io::disk_api::{
    DiskSession, EmptyRetentionControlLoan, IntoRetentionError, MaintenancePort, MutationPort,
    Plan4Recovered, ReadPort, RetentionCause, RetentionError, RetentionPhase,
    RowControlObservation, RowInputPlan, RowOperation, RowPageRole, RowPreflightContext,
    RowProducerContext, RowProducerInput, RowProducerLane, RowProposal, RowSourceBinding,
    RowSourceRole, RowValidationLane,
};
