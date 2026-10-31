#![allow(dead_code)]
//! MBC00-P02: a Before returns the same wrapper; reopen keeps the same
//! reservation; cancel returns the private loan. A FlushRequired step holds
//! nothing and ends the call.
use crate::storage::disk::{
    CheckedTable, MaintenancePort, MutationPort, RetentionError, RowInputPlan, RowOperation,
    RowOwnedError, RowPageRole, RowPreflightStep, RowProducerLane, RowProposal, RowStatementView,
};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;

fn produce(lane: RowProducerLane<'_>) -> RowProposal {
    let (_context, _control, _before, _schema, frame, _after, _candidate, _scratch) = lane.parts();
    frame[0] = 1;
    RowProposal::new(RowOperation::Forward, 1, 1, [RowPageRole::RowLeaf; 5])
}

fn flow<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    view: &RowStatementView<'_, '_, 'a, 'c, 's>,
    table: &CheckedTable<'c, 's>,
    private: &mut [u8; 425_984],
    flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
) -> Result<(), RetentionError> {
    let mut preflight = match mutation.preflight_row(owner, view, table, private, flusher)? {
        RowPreflightStep::Ready(preflight) => preflight,
        RowPreflightStep::FlushRequired { .. } => return Ok(()),
    };
    let plan = {
        let (context, _schema, _leaf, _candidate) = preflight.validation_lane()?.parts();
        RowInputPlan::new(1, context.schema_epoch(), [0; 32])
    };
    let mut reserved = match preflight.reserve(plan) {
        Ok(reserved) => reserved,
        Err(RowOwnedError::Before { error, owned }) => {
            owned.finish()?;
            return Err(error);
        }
        Err(RowOwnedError::Fatal(error)) => return Err(error),
    };
    let proposal = produce(reserved.producer_lane()?);
    let mut reserved = match reserved.prepare(proposal) {
        Ok(prepared) => prepared.reopen(),
        Err(RowOwnedError::Before { owned, .. }) => owned,
        Err(RowOwnedError::Fatal(error)) => return Err(error),
    };
    let proposal = produce(reserved.producer_lane()?);
    let reserved = match reserved.prepare(proposal) {
        Ok(prepared) => match prepared.apply() {
            Ok(applied) => {
                let (_observation, _private) = applied.into_parts();
                return Ok(());
            }
            Err(RowOwnedError::Before { owned, .. }) => owned.reopen(),
            Err(RowOwnedError::Fatal(error)) => return Err(error),
        },
        Err(RowOwnedError::Before { owned, .. }) => owned,
        Err(RowOwnedError::Fatal(error)) => return Err(error),
    };
    match reserved.cancel() {
        Ok(cancelled) => {
            let _private: &mut [u8; 425_984] = cancelled.into_private();
            Ok(())
        }
        Err(RowOwnedError::Before { error, owned }) => {
            drop(owned);
            Err(error)
        }
        Err(RowOwnedError::Fatal(error)) => Err(error),
    }
}
