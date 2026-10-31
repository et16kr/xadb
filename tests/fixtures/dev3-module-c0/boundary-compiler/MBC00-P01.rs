#![allow(dead_code)]
//! MBC00-P01: preflight -> validation lane end -> reserve -> producer lane end
//! -> prepare -> apply -> statement end -> transaction end. Invalid type, null,
//! length and index inputs take the effect-free preflight finish branch.
//! S2-0: preflight takes the flusher and returns a step. The FlushRequired arm
//! borrows the same flusher again and retries the same row from preflight; this
//! compiles only because the Row I/O lease has no Drop impl.
use crate::storage::disk::{
    CheckedTable, MaintenancePort, MutationPort, Plan4Target, RetentionError, RowApplied,
    RowInputPlan, RowOperation, RowOwnedError, RowPageRole, RowPreflightStep, RowProducerLane,
    RowProposal, RowTransaction, RowValidationLane, StatementFinishError, TransactionFinishError,
};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;

enum Input {
    Valid,
    WrongType,
    Null,
    Length2049,
    Index,
}

fn validate(lane: RowValidationLane<'_>, input: Input) -> Option<RowInputPlan> {
    let (context, _schema, _leaf, candidate) = lane.parts();
    match input {
        Input::Valid => {
            candidate[0] = 1;
            Some(RowInputPlan::new(1, context.schema_epoch(), [0; 32]))
        }
        Input::WrongType | Input::Null | Input::Length2049 | Input::Index => None,
    }
}

fn produce(lane: RowProducerLane<'_>) -> RowProposal {
    let (context, _control, _before, _schema, frame, after, _candidate, scratch) = lane.parts();
    scratch[0] = 1;
    frame[0] = scratch[0];
    after[0][0] = 1;
    let count = context.after_order().len();
    RowProposal::new(RowOperation::Forward, 1, count, [RowPageRole::RowLeaf; 5])
}

fn flush_target<'s, 'c, W: WriterNamespace>(
    flusher: &mut MaintenancePort<'_, 's, 'c, '_, '_, '_, '_, W>,
    flush_owner: &mut Owner<'c, 's>,
    target: Plan4Target,
) -> Result<(), RetentionError> {
    let _step = flusher.try_flush(flush_owner, &target)?;
    let _phase = flusher.phase();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn flow<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    transaction: RowTransaction<'b, 'a, 'c, 's>,
    table: &CheckedTable<'c, 's>,
    private: &mut [u8; 425_984],
    flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    flush_owner: &mut Owner<'c, 's>,
    input: Input,
) -> Result<u64, RetentionError> {
    let view = mutation.begin_statement(owner, &transaction)?;
    let applied: Option<RowApplied<'_>> = {
        let mut preflight = loop {
            match mutation.preflight_row(owner, &view, table, &mut *private, &mut *flusher)? {
                RowPreflightStep::Ready(preflight) => break preflight,
                RowPreflightStep::FlushRequired { target } => {
                    flush_target(flusher, flush_owner, target)?;
                }
            }
        };
        match validate(preflight.validation_lane()?, input) {
            None => {
                preflight.finish()?;
                None
            }
            Some(plan) => {
                let mut reserved = match preflight.reserve(plan) {
                    Ok(reserved) => reserved,
                    Err(RowOwnedError::Before { error, owned }) => {
                        owned.finish()?;
                        return Err(error);
                    }
                    Err(RowOwnedError::Fatal(error)) => return Err(error),
                };
                let proposal = {
                    let lane = reserved.producer_lane()?;
                    produce(lane)
                };
                let prepared = match reserved.prepare(proposal) {
                    Ok(prepared) => prepared,
                    Err(RowOwnedError::Before { error, owned }) => {
                        let _ = owned.cancel();
                        return Err(error);
                    }
                    Err(RowOwnedError::Fatal(error)) => return Err(error),
                };
                match prepared.apply() {
                    Ok(applied) => Some(applied),
                    Err(RowOwnedError::Before { error, owned }) => {
                        let _ = owned.reopen().cancel();
                        return Err(error);
                    }
                    Err(RowOwnedError::Fatal(error)) => return Err(error),
                }
            }
        }
    };
    if let Some(applied) = applied {
        let (observation, _private) = applied.into_parts();
        let _ = (observation.operation(), observation.row(), observation.undo());
    }
    match mutation.finish_statement(owner, view) {
        Ok(()) => {}
        Err(StatementFinishError::Before { error, .. } | StatementFinishError::Fatal(error)) => {
            return Err(error);
        }
    }
    match mutation.commit_row(owner, transaction, private) {
        Ok(commit) => Ok(commit.q()),
        Err(
            TransactionFinishError::Before { error, .. }
            | TransactionFinishError::RollbackRequired { error, .. }
            | TransactionFinishError::Rejected(error)
            | TransactionFinishError::Fatal(error),
        ) => Err(error),
    }
}

fn concrete() {
    let _ = flow::<crate::storage::page_io::test_support::Writer>;
}
