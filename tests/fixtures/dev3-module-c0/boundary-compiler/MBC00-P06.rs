#![allow(dead_code)]
//! MBC00-P06: live statement rollback drives CLR tasks through the rollback's
//! own reborrow; a full rollback ends before finish_aborted consumes the same
//! transaction.
use crate::storage::disk::{
    MutationPort, Plan4Target, RetentionError, RollbackScope, RollbackStep, RowOperation,
    RowOwnedError, RowPageRole, RowProducerLane, RowProposal, RowStatementView, RowTransaction,
    TransactionFinishError,
};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;

fn produce(lane: RowProducerLane<'_>) -> RowProposal {
    let (_context, _control, _before, _schema, frame, _after, _candidate) = lane.parts();
    frame[0] = 1;
    RowProposal::new(RowOperation::Clr, 1, 1, [RowPageRole::RowLeaf; 5])
}

#[allow(clippy::too_many_arguments)]
fn rollback<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    transaction: &RowTransaction<'_, 'a, 'c, 's>,
    view: Option<&RowStatementView<'_, '_, 'a, 'c, 's>>,
    scope: RollbackScope,
    private: &mut [u8; 425_984],
    flush: &mut dyn FnMut(Plan4Target),
) -> Result<u64, RetentionError> {
    let mut applied = 0;
    let mut rollback = mutation.start_row_rollback(owner, transaction, view, scope, private)?;
    loop {
        match rollback.next()? {
            RollbackStep::Candidate(mut task) => {
                let proposal = produce(task.producer_lane()?);
                let prepared = match task.prepare(proposal) {
                    Ok(prepared) => prepared,
                    Err(RowOwnedError::Before { error, owned }) => {
                        drop(owned);
                        return Err(error);
                    }
                    Err(RowOwnedError::Fatal(error)) => return Err(error),
                };
                match prepared.apply() {
                    Ok(observation) => {
                        let _ = (observation.operation(), observation.row(), observation.undo());
                        applied += 1;
                    }
                    Err(RowOwnedError::Before { error, owned }) => {
                        let _task = owned.reopen();
                        return Err(error);
                    }
                    Err(RowOwnedError::Fatal(error)) => return Err(error),
                }
            }
            RollbackStep::FlushRequired { target } => flush(target),
            RollbackStep::Complete => break,
        }
    }
    let done = match rollback.finish() {
        Ok(done) => done,
        Err(RowOwnedError::Before { error, owned }) => {
            drop(owned);
            return Err(error);
        }
        Err(RowOwnedError::Fatal(error)) => return Err(error),
    };
    let (finished, private) = done.into_parts();
    private[0] = u8::from(finished == scope);
    Ok(applied)
}

#[allow(clippy::too_many_arguments)]
fn flow<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    transaction: RowTransaction<'_, 'a, 'c, 's>,
    private: &mut [u8; 425_984],
    flush: &mut dyn FnMut(Plan4Target),
) -> Result<u64, RetentionError> {
    let view = mutation.begin_statement(owner, &transaction)?;
    let mut applied = rollback(
        mutation,
        owner,
        &transaction,
        Some(&view),
        RollbackScope::Statement,
        private,
        flush,
    )?;
    if mutation.finish_statement(owner, view).is_err() {
        return Ok(applied);
    }
    applied += rollback(
        mutation,
        owner,
        &transaction,
        None,
        RollbackScope::Transaction,
        private,
        flush,
    )?;
    match mutation.finish_aborted(owner, transaction) {
        Ok(()) => Ok(applied),
        Err(TransactionFinishError::Before { error, transaction }) => {
            drop(transaction);
            Err(error)
        }
        Err(TransactionFinishError::RollbackRequired { error, .. })
        | Err(TransactionFinishError::Rejected(error))
        | Err(TransactionFinishError::Fatal(error)) => Err(error),
    }
}
