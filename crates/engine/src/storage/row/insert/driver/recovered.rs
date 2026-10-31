//! A's handler of a recovered Row loser's CLR (XADB-MODULE-ROW-RECOVERY-001,
//! boundaries `#row-recovery-s6`). The product open driver dispatches C's
//! `RecoveryAction::RowClr` here: the handler borrows the task's lane for
//! the same semantic CLR producer as live rollback (`produce_clr`), then
//! calls only C's prepare and apply. It never touches a raw writer, the
//! selected base or the recovery phase, and constructs no C value. Every
//! refusal keeps the task: Source/Producer/Before return it for retry, and
//! Fatal leaves C's original fence and recovery obligations.
use crate::storage::{
    disk::{
        Plan4RecoveryError, Plan4RecoveryProgress, PreparedRecoveryRow, RecoveryOwnedError,
        RecoveryRowTask,
    },
    row::insert::{InsertError, produce_clr},
};
use foundation::namespace::WriterNamespace;

/// Where A's recovered CLR stopped, keeping the task's ownership.
#[allow(
    clippy::large_enum_variant,
    reason = "the same consuming task is returned inline for retry"
)]
pub(crate) enum RowRecoveryHandlerError<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    /// C refused the lane; the task is unchanged.
    Source {
        error: Plan4RecoveryError,
        task: RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    },
    /// A's CLR producer refused the lane's sources; the task is unchanged.
    Producer {
        error: InsertError,
        task: RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    },
    Prepare(RecoveryOwnedError<RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>),
    Apply(RecoveryOwnedError<PreparedRecoveryRow<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>),
}

enum Stop {
    Source(Plan4RecoveryError),
    Producer(InsertError),
}

/// lane → `produce_clr` → prepare → apply, and nothing else.
#[allow(
    clippy::result_large_err,
    reason = "a refusal returns C's same task, with its draft cut, inline"
)]
pub(in crate::storage) fn handle_recovered_clr<
    'r,
    'a,
    's,
    'c,
    'p,
    'd,
    'o,
    'm,
    'w,
    W: WriterNamespace,
>(
    mut task: RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) -> Result<Plan4RecoveryProgress, RowRecoveryHandlerError<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
    // The lane ends inside this match; only then is the task handed back.
    let proposal = match task.producer_lane() {
        Ok(lane) => produce_clr(lane).map_err(Stop::Producer),
        Err(error) => Err(Stop::Source(error)),
    };
    let proposal = match proposal {
        Ok(proposal) => proposal,
        Err(Stop::Source(error)) => return Err(RowRecoveryHandlerError::Source { error, task }),
        Err(Stop::Producer(error)) => {
            return Err(RowRecoveryHandlerError::Producer { error, task });
        }
    };
    let prepared = task
        .prepare(proposal)
        .map_err(RowRecoveryHandlerError::Prepare)?;
    prepared.apply().map_err(RowRecoveryHandlerError::Apply)
}

/// The handler's own carriers for the recovery census (S6R-C01/C02): the
/// task it takes, its Result and the proposal/stop slots; A's producer
/// locals are counted separately.
#[cfg(test)]
pub(in crate::storage) fn handler_source_bytes<W: WriterNamespace + 'static>() -> usize {
    use std::mem::size_of;
    type T<W> = RecoveryRowTask<
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        W,
    >;
    type E<W> = RowRecoveryHandlerError<
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        W,
    >;
    size_of::<T<W>>()
        + size_of::<Result<Plan4RecoveryProgress, E<W>>>()
        + 2 * size_of::<Result<crate::storage::disk::RowProposal, Stop>>()
        + size_of::<Result<crate::storage::disk::RowProducerLane<'static>, Plan4RecoveryError>>()
        + 512
}
