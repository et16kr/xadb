//! The autocommit INSERT executor's steps over the driver (contract
//! `dev3-module-boundaries.md#autocommit-insert`): one INSERT statement in
//! its own transaction, from the statement to the transaction's end.
//!
//! The live entry (`live::autocommit_insert_live`) begins the transaction,
//! begins the statement, gets the table from its caller and calls C's
//! Commit itself; the steps here run the statement (`run_statement`), end a
//! failed statement (`end_failed_statement`: its rollback to the mark and
//! its finish), read C's commit answer (`commit_answer`) and end an aborted
//! transaction (`abort_transaction`: the full rollback and
//! `finish_aborted`). The result goes to one caller-owned slot
//! (`Autocommit`), so no step returns a Failure through a frame that sits
//! below a later C call, and each step is its own never-inlined frame, so
//! none of its slots is laid out below a C call of another step
//! (`live::autocommit_sites`).
//!
//! Every handle has one next step. A refusal on the way to the end of a failed
//! statement or transaction has no retry here (the steps have no wait
//! policy): the handles are dropped into C's fence and recovery obligations
//! and the slot says `Fenced`. Only C's FlushRequired is continued, as the
//! preflight's is (`flush`, `Rollback::flush`). The cleanup of the ended
//! transaction is the caller's next step (`retire_finished`), after the
//! executor returned `Committed` or `Aborted` (after `NotBegun` there is
//! no transaction; after `Fenced` the rest is C's recovery): a transaction
//! of n rows leaves 5n + 1 groups after its commit and 4n + 1 after its
//! abort, so a commit of more than 12 rows, or an abort of more than 15,
//! takes a second drain (`Retired::complete`). After `Aborted`, `slots`
//! still names the rows applied before the failure; they were rolled back.
use super::*;

/// What one autocommit statement did; the executor writes it once its
/// outcome is known and the caller reads it after the call.
#[derive(Debug)]
pub(crate) enum Autocommit<C> {
    /// The executor has not written it.
    Idle,
    /// Every row was applied, the statement finished and C committed the
    /// transaction.
    Committed { committed: C, rows: usize },
    /// C refused Begin unchanged: no transaction exists.
    NotBegun(Failure),
    /// The statement or the commit failed (the Failure is the first one).
    /// The statement's rows were rolled back, the statement finished and the
    /// transaction was rolled back and ended: nothing of it remains but the
    /// cleanup of the ended transaction.
    Aborted(Failure),
    /// C answered Fatal or CallerMustResolve, or refused a step on the way
    /// to the end; the handles went to C's fence and recovery obligations
    /// (`Failure::must_resolve`). The Failure is the first one; a later
    /// refusal is its `cleanup` unless one is already there.
    Fenced(Failure),
}

impl Failure {
    /// The first failure, now with the handles in C's fence.
    fn fenced(mut self, (stage, error): (Stage, DriverError)) -> Self {
        self.must_resolve = true;
        if self.cleanup.is_none() {
            self.cleanup = Some((stage, error));
        }
        self
    }
}

/// The slot after a refusal that left a handle with no next step. Never
/// inlined, so its own slots are not laid out under any C call.
#[inline(never)]
fn fence<C>(out: &mut Autocommit<C>, refusal: (Stage, DriverError)) {
    let first = std::mem::replace(out, Autocommit::Idle);
    *out = Autocommit::Fenced(match first {
        Autocommit::NotBegun(failure)
        | Autocommit::Aborted(failure)
        | Autocommit::Fenced(failure) => failure.fenced(refusal),
        Autocommit::Idle | Autocommit::Committed { .. } => {
            let (stage, error) = refusal;
            Failure {
                must_resolve: true,
                ..Failure::new(stage, error)
            }
        }
    });
}

/// The failed step's stage and error, kept when its handle is dropped.
fn refusal<H>(stop: DriveError<H>) -> (Stage, DriverError) {
    (stop.failure.stage, stop.failure.error)
}

/// C's refusal of a step at `stage`: the slot becomes `Fenced` when C
/// answered CallerMustResolve, otherwise `unchanged` (`NotBegun` for Begin,
/// `Aborted` for a later step). Never inlined, so its Failure is not laid
/// out under any C call.
#[inline(never)]
pub(super) fn refused<C>(
    out: &mut Autocommit<C>,
    stage: Stage,
    error: RetentionError,
    unchanged: fn(Failure) -> Autocommit<C>,
) {
    let failure = Failure::new(stage, error.into());
    *out = if failure.must_resolve {
        Autocommit::Fenced(failure)
    } else {
        unchanged(failure)
    };
}

/// The statement of an autocommit transaction (`insert_statement`). Returns
/// the row count when every row was applied and the statement finished;
/// otherwise None, after the failed statement's end (`end_failed_statement`)
/// wrote the slot (`Aborted`: the transaction is to be ended; `Fenced`).
#[inline(never)]
#[allow(
    clippy::too_many_arguments,
    reason = "the call, the statement's transaction, view, table and loan, the rows and their slots, and the result slot"
)]
pub(super) fn run_statement<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    statement: B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<B::Applied>],
    out: &mut Autocommit<B::Committed>,
) -> Option<usize> {
    match insert_statement(call, statement, table, loan, rows, slots) {
        Ok(rows) => Some(rows),
        Err(DriveError { failure, after }) => {
            end_failed_statement(call, transaction, after, failure, loan, out);
            None
        }
    }
}

/// A statement whose table the caller did not issue: `issued` is the
/// issue's answer that is not a table, C's refusal of the issue or None (no
/// visible table answers the statement: `InvalidInput`). It is taken by
/// value, so it stays in the caller's slot. It is the statement's failure,
/// ended as any other (`end_failed_statement`): the statement has no row,
/// so it is finished, or dropped into C's fence after a CallerMustResolve
/// refusal. An issued table is never passed here; it would be dropped.
#[inline(never)]
pub(super) fn table_failed<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    statement: B::Statement,
    issued: Result<Option<B::Table>, RetentionError>,
    loan: &mut Loan,
    out: &mut Autocommit<B::Committed>,
) {
    let failure = Failure::new(
        Stage::Table,
        match issued {
            Err(error) => error.into(),
            Ok(_) => InsertError::InvalidInput.into(),
        },
    );
    let after = if failure.must_resolve {
        After::MustResolve(Some(statement))
    } else {
        After::Returned(statement)
    };
    end_failed_statement(call, transaction, after, failure, loan, out);
}

/// The end of a failed statement, as its `After` says: a `Returned` or
/// `FinishPending` statement is finished (`finish_failed`); a
/// `RollbackRequired` one is rolled back to its mark and then finished
/// (`roll_back_statement`); a `Closed` one has nothing left; a
/// `MustResolve` one is C's. The slot becomes `Aborted` with the
/// statement's Failure, or `Fenced`. A Failure C must resolve calls C no
/// more, whatever `After` holds: the driver hands a statement whose own
/// check C answered CallerMustResolve back as `Returned`, and it is dropped
/// into C's fence here, as the transaction is after it.
#[inline(never)]
pub(super) fn end_failed_statement<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    after: After<B::Statement>,
    failure: Failure,
    loan: &mut Loan,
    out: &mut Autocommit<B::Committed>,
) {
    if failure.must_resolve {
        *out = Autocommit::Fenced(failure);
        drop(after);
        return;
    }
    *out = Autocommit::Aborted(failure);
    match after {
        After::Returned(statement) | After::FinishPending(statement) => {
            finish_failed(call, statement, out);
        }
        After::RollbackRequired(pending) => {
            roll_back_statement(call, transaction, pending, loan, out);
        }
        After::Closed | After::MustResolve(_) => {}
    }
}

/// C's finish of a failed statement (`finish_statement`); a refusal drops
/// the statement into C's fence.
#[inline(never)]
fn finish_failed<B: Boundary>(
    call: &mut B,
    statement: B::Statement,
    out: &mut Autocommit<B::Committed>,
) {
    if let Err(stop) = finish_statement(call, statement) {
        fence(out, refusal(stop));
    }
}

/// The statement's rollback to its mark (`start_statement_rollback`,
/// `roll`, `finish_roll`), then its finish (`finish_rolled_statement`).
/// The rollback stays in this frame's slot; `roll` steps it by reference,
/// so `roll`'s frame is not laid out under C's finish.
#[inline(never)]
fn roll_back_statement<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    mut pending: PendingRollback<B::Statement>,
    loan: &mut Loan,
    out: &mut Autocommit<B::Committed>,
) {
    let rolled = match start_statement_rollback(call, transaction, &mut pending, loan) {
        Ok(mut rollback) => match roll(&mut rollback) {
            Ok(()) => finish_roll(rollback),
            Err(refused) => Err(refused),
        },
        Err(failure) => Err((failure.stage, failure.error)),
    };
    match rolled {
        Ok(()) => finish_rolled_statement(call, pending, out),
        Err(refused) => fence(out, refused),
    }
}

/// The finish of a statement whose rollback finished.
#[inline(never)]
fn finish_rolled_statement<B: Boundary>(
    call: &mut B,
    pending: PendingRollback<B::Statement>,
    out: &mut Autocommit<B::Committed>,
) {
    match pending.rolled_back() {
        Ok(statement) => finish_failed(call, statement, out),
        Err(_pending) => fence(
            out,
            (Stage::RollbackFinish, InsertError::OrderMismatch.into()),
        ),
    }
}

/// A started rollback's steps until C reports it complete: each step
/// (`rollback_step`), and C's flush targets flushed through the rollback
/// (`Rollback::flush`). Any other stop ends it; its caller drops the
/// rollback (and the step drops a task C handed back) into C's fence. The
/// finish is the caller's (`finish_roll`).
#[inline(never)]
fn roll<K: Rollback>(rollback: &mut K) -> Result<(), (Stage, DriverError)> {
    loop {
        let target = match rollback_step(rollback) {
            Ok(Stepped::Compensated) => continue,
            Ok(Stepped::Complete) => return Ok(()),
            Err(RollbackStop::Flush(target)) => target,
            Err(RollbackStop::Next(error)) => return Err((Stage::RollbackNext, error.into())),
            Err(RollbackStop::Task { stage, error, .. }) => return Err((stage, error)),
            Err(RollbackStop::MustResolve { stage, error }) => return Err((stage, error)),
        };
        if let Err(error) = rollback.flush(&target) {
            return Err((Stage::RollbackFlush, error.into()));
        }
    }
}

/// C's finish of a complete rollback (`finish_rollback`), in its own frame
/// so that its answer is not laid out under the rollback's steps.
#[inline(never)]
fn finish_roll<K: Rollback>(rollback: K) -> Result<(), (Stage, DriverError)> {
    match finish_rollback(rollback) {
        Ok(()) => Ok(()),
        Err(RowOwnedError::Before { error, .. }) => Err((Stage::RollbackFinish, error.into())),
        Err(RowOwnedError::Fatal(error)) => Err((Stage::RollbackFinish, error.into())),
    }
}

/// C's answer to the Commit of a transaction whose statement applied `rows`
/// rows. A commit C refuses unchanged or answers `RollbackRequired` leaves
/// the transaction to the full rollback (`abort_transaction`); a
/// `Rejected` one leaves nothing (C closed the handle); a Fatal one is C's.
#[inline(never)]
pub(super) fn commit_answer<B: Boundary>(
    call: &mut B,
    answer: Result<B::Committed, TransactionFinishError<B::Transaction>>,
    loan: &mut Loan,
    rows: usize,
    out: &mut Autocommit<B::Committed>,
) {
    match answer {
        Ok(committed) => *out = Autocommit::Committed { committed, rows },
        Err(
            TransactionFinishError::Before { error, transaction }
            | TransactionFinishError::RollbackRequired { error, transaction },
        ) => {
            refused(out, Stage::Commit, error, Autocommit::Aborted);
            abort_transaction(call, transaction, loan, out);
        }
        Err(TransactionFinishError::Rejected(error)) => {
            refused(out, Stage::Commit, error, Autocommit::Aborted);
        }
        Err(TransactionFinishError::Fatal(error)) => {
            *out = Autocommit::Fenced(Failure::fatal(Stage::Commit, error));
        }
    }
}

/// The end of a transaction whose statement or commit failed: when the slot
/// is `Aborted`, the full rollback (`start_full_rollback`, `roll`,
/// `finish_roll`) and
/// `finish_aborted` (`finish_rolled_transaction`); when it is `Fenced`, the
/// transaction is dropped into C's fence. The slot stays `Aborted`, or
/// becomes `Fenced`.
#[inline(never)]
pub(super) fn abort_transaction<B: Boundary>(
    call: &mut B,
    transaction: B::Transaction,
    loan: &mut Loan,
    out: &mut Autocommit<B::Committed>,
) {
    if !matches!(out, Autocommit::Aborted(_)) {
        debug_assert!(matches!(out, Autocommit::Fenced(_)));
        drop(transaction);
        return;
    }
    let mut pending = PendingRollback {
        handle: transaction,
        progress: Progress::Required,
    };
    let rolled = match start_full_rollback(call, &mut pending, loan) {
        Ok(mut rollback) => match roll(&mut rollback) {
            Ok(()) => finish_roll(rollback),
            Err(refused) => Err(refused),
        },
        Err(failure) => Err((failure.stage, failure.error)),
    };
    match rolled {
        Ok(()) => finish_rolled_transaction(call, pending, out),
        Err(refused) => fence(out, refused),
    }
}

/// `finish_aborted` of a transaction whose full rollback finished; a
/// refusal drops the transaction into C's fence.
#[inline(never)]
fn finish_rolled_transaction<B: Boundary>(
    call: &mut B,
    pending: PendingRollback<B::Transaction>,
    out: &mut Autocommit<B::Committed>,
) {
    let ended = match pending.rolled_back() {
        Ok(transaction) => finish_aborted(call, transaction).map_err(refusal),
        Err(_pending) => Err((Stage::RollbackFinish, InsertError::OrderMismatch.into())),
    };
    if let Err(refused) = ended {
        fence(out, refused);
    }
}
