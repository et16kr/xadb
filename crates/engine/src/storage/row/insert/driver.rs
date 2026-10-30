//! A's INSERT statement driver over the C0 Row write boundary.
//!
//! A owns the statement flow; C owns every source, reservation, stage,
//! install, rollback head and terminal effect. Per row the driver calls
//! preflight → validation lane → A `validate_insert` → RowReserve → producer
//! lane → A `produce_insert` → prepare → apply. A row failure releases what
//! that row holds (preflight finish, or Cancel of the one reservation) before
//! anything else. A statement whose rows of this call stay applied is handed
//! back `RollbackRequired` inside a `PendingRollback`, which gives the
//! statement up only after a rollback started from it has finished, so
//! partial rows never reach `finish_statement` as a success. The rollback is
//! driven by the caller one step at a time (`start_statement_rollback` →
//! `rollback_step`… → `finish_rollback` → `PendingRollback::rolled_back` →
//! `finish_statement`), so every wrapper C hands back after a refusal reaches
//! the caller for retry and none is dropped to restart a started rollback
//! (gap G5). Commit and the full rollback use the same steps. C is reached
//! only through the private traits below: `live` forwards them to the actual
//! C0 types and the tests feed literal stand-ins. See
//! `devdoc/contracts/dev3-module-boundaries.md` (6.2, 6.6, 6.7),
//! `dev3-module-consumers.md` 7 and the F02 table in `dev3-task-06-tree-rows.md`.
use super::InsertError;
use crate::storage::{
    disk::{
        RetentionError, RollbackScope, RowInputPlan, RowOwnedError, RowProposal,
        StatementFinishError, TransactionFinishError,
    },
    row_record::{MAX_ROW_COLUMNS, ValueRef},
};
use foundation::error::Effect;
use std::convert::Infallible;

mod live;
#[cfg(test)]
mod tests;

/// The one private operation loan C lends to each preflight and rollback.
type Loan = [u8; 425_984];

/// Claimed source bound for the driver's own carriers below A's producer, in
/// the repository's size_of inventory sense (`carrier_bytes`); it is not a
/// machine-stack bound and not a grant. With A's deeper producer inventory it
/// stays within the 65536 B short-source cap; the C3 supply's census still
/// owns the aggregate.
pub(super) const DRIVER_SOURCE_BYTES: usize = 12_288;

/// The original statement view as the driver uses it.
trait StatementView {
    /// Effect-free statement identity, phase and fence check.
    fn check(&self) -> Result<(), RetentionError>;
}

/// C's entries over the caller's original MutationPort, Owner and flusher.
/// Each wrapper borrows them until it is consumed; nothing is acquired twice.
trait Boundary {
    type Transaction;
    type Statement: StatementView;
    type Table;
    type Applied;
    type Committed;
    /// C's preflight flush target, passed out unchanged.
    type Target;
    type Preflight<'r, 'i>: Preflight<Applied = Self::Applied>
    where
        Self: 'r;
    type Rollback<'r, 'i>: Rollback
    where
        Self: 'r;
    fn preflight<'r, 'i>(
        &'r mut self,
        statement: &'r Self::Statement,
        table: &'r Self::Table,
        loan: &'i mut Loan,
    ) -> Result<PreflightNext<Self::Preflight<'r, 'i>, Self::Target>, RetentionError>;
    fn start_rollback<'r, 'i>(
        &'r mut self,
        transaction: &'r Self::Transaction,
        statement: Option<&'r Self::Statement>,
        scope: RollbackScope,
        loan: &'i mut Loan,
    ) -> Result<Self::Rollback<'r, 'i>, RetentionError>;
    fn finish_statement(
        &mut self,
        statement: Self::Statement,
    ) -> Result<(), StatementFinishError<Self::Statement>>;
    fn commit(
        &mut self,
        transaction: Self::Transaction,
        loan: &mut Loan,
    ) -> Result<Self::Committed, TransactionFinishError<Self::Transaction>>;
    fn finish_aborted(
        &mut self,
        transaction: Self::Transaction,
    ) -> Result<(), TransactionFinishError<Self::Transaction>>;
}

/// What C's preflight returned.
enum PreflightNext<P, F> {
    Ready(P),
    /// C needs `F` flushed before the same row is tried again from
    /// preflight. Nothing is held: C has returned its source, gate and flush
    /// lease, and nothing took effect.
    Flush(F),
}

/// Checked source state before the row's first effect.
trait Preflight: Sized {
    type Applied;
    type Lane<'l>: ValidationLane
    where
        Self: 'l;
    type Reserved: Reserved<Applied = Self::Applied>;
    fn validation_lane(&mut self) -> Result<Self::Lane<'_>, RetentionError>;
    /// The row's only first effect; the plan is untrusted.
    fn reserve(self, input: RowInputPlan) -> Result<Self::Reserved, RowOwnedError<Self>>;
    /// Returns the admission and loan; the observed read set stays. C
    /// documents Drop as the same no-effect release.
    fn finish(self) -> Result<(), RetentionError>;
}

/// The one outstanding RowReserve: consumed by apply or released by cancel.
trait Reserved: Sized {
    type Applied;
    type Lane<'l>: ProducerLane
    where
        Self: 'l;
    type Prepared: Prepared<Reserved = Self, Applied = Self::Applied>;
    fn producer_lane(&mut self) -> Result<Self::Lane<'_>, RetentionError>;
    fn prepare(self, proposal: RowProposal) -> Result<Self::Prepared, RowOwnedError<Self>>;
    fn cancel(self) -> Result<(), RowOwnedError<Self>>;
}

trait Prepared: Sized {
    type Reserved;
    type Applied;
    fn apply(self) -> Result<Self::Applied, RowOwnedError<Self>>;
    /// Before stage only: the same reservation, never a new one.
    fn reopen(self) -> Self::Reserved;
}

/// A's value check over C's validation lane.
trait ValidationLane {
    fn validate(self, values: &[ValueRef<'_>]) -> Result<RowInputPlan, InsertError>;
}

/// A's candidate producers over C's producer lane.
trait ProducerLane {
    fn forward(self) -> Result<RowProposal, InsertError>;
    fn clr(self) -> Result<RowProposal, InsertError>;
}

/// One rollback step chosen by C.
enum RollbackNext<T, F> {
    Candidate(T),
    /// C needs `F` flushed before its next candidate.
    Flush(F),
    Complete,
}

/// C's live rollback over the actual head chain; C picks each next U.
trait Rollback: Sized {
    /// C's flush target, passed out unchanged.
    type Target;
    type Task<'x>: RollbackTask
    where
        Self: 'x;
    fn next(&mut self) -> Result<RollbackNext<Self::Task<'_>, Self::Target>, RetentionError>;
    /// Needs the actual mark/None and the StatementFinish/AbortEnd receipt.
    fn finish(self) -> Result<(), RowOwnedError<Self>>;
}

trait RollbackTask: Sized {
    type Lane<'l>: ProducerLane
    where
        Self: 'l;
    type Prepared: PreparedTask<Task = Self>;
    fn producer_lane(&mut self) -> Result<Self::Lane<'_>, RetentionError>;
    fn prepare(self, proposal: RowProposal) -> Result<Self::Prepared, RowOwnedError<Self>>;
}

trait PreparedTask: Sized {
    type Task;
    fn apply(self) -> Result<(), RowOwnedError<Self>>;
    fn reopen(self) -> Self::Task;
}

/// A driver error: A's own `InsertError`, or C's error kept by value with
/// its phase, cause (Busy/Capacity/Unsupported stay distinct) and effect.
#[derive(Debug)]
pub(crate) enum DriverError {
    Insert(InsertError),
    Disk(RetentionError),
}

impl DriverError {
    fn must_resolve(&self) -> bool {
        matches!(self, Self::Disk(error) if error.effect == Effect::CallerMustResolve)
    }
}

impl From<InsertError> for DriverError {
    fn from(error: InsertError) -> Self {
        Self::Insert(error)
    }
}

impl From<RetentionError> for DriverError {
    fn from(error: RetentionError) -> Self {
        Self::Disk(error)
    }
}

/// The call that produced an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Statement,
    Arguments,
    Preflight,
    ValidationLane,
    Validate,
    Reserve,
    FinishPreflight,
    ProducerLane,
    Produce,
    Prepare,
    Apply,
    Cancel,
    RollbackStart,
    RollbackNext,
    ClrLane,
    Clr,
    ClrPrepare,
    ClrApply,
    FinishStatement,
    Commit,
    FinishAborted,
}

/// A failed call's first error and, for a row, the one release after it.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) stage: Stage,
    pub(crate) error: DriverError,
    /// A failed row's release (finish or Cancel) that C also refused. A row
    /// holds at most one thing to release, and rollback and finish errors
    /// are returned by their own calls, so nothing here is overwritten.
    pub(crate) cleanup: Option<(Stage, DriverError)>,
    /// C reported Fatal or CallerMustResolve, or refused to release a held
    /// reservation; C keeps the fence and obligations.
    pub(crate) must_resolve: bool,
}

impl Failure {
    fn new(stage: Stage, error: DriverError) -> Self {
        let must_resolve = error.must_resolve();
        Self {
            stage,
            error,
            cleanup: None,
            must_resolve,
        }
    }

    /// C answered Fatal, or answered with no defined continuation: C keeps
    /// the fence and obligations.
    fn fatal(stage: Stage, error: RetentionError) -> Self {
        Self {
            must_resolve: true,
            ..Self::new(stage, error.into())
        }
    }

    fn released(mut self, stage: Stage, error: DriverError, must_resolve: bool) -> Self {
        self.must_resolve |= must_resolve || error.must_resolve();
        self.cleanup = Some((stage, error));
        self
    }
}

/// What a failed call leaves with its statement or transaction. Each
/// variant names the only next step.
#[derive(Debug)]
pub(crate) enum After<H> {
    /// Nothing of this call remains; the handle is returned as it was.
    Returned(H),
    /// Rows stay applied and no rollback is started: the next step is
    /// `start_statement_rollback` (`start_full_rollback` for a transaction).
    /// The handle stays inside `PendingRollback` until that rollback finished.
    RollbackRequired(PendingRollback<H>),
    /// Nothing is left to roll back (or the rollback finished): only
    /// `finish_statement` (`finish_aborted`) remains, never a new rollback.
    FinishPending(H),
    /// C closed the handle.
    Closed,
    /// Only for dropping into C's fence and recovery obligations.
    MustResolve(Option<H>),
}

impl<H> After<H> {
    fn rollback_required(handle: H) -> Self {
        Self::RollbackRequired(PendingRollback {
            handle,
            progress: Progress::Required,
        })
    }
}

/// How far the one rollback a `PendingRollback` allows has come. Only a
/// rollback started from that pending moves it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Progress {
    Required,
    Started,
    Finished,
}

/// A statement (or transaction) whose applied rows must be rolled back
/// before it is finished. The fields are private to the driver and nothing
/// outside it builds one, so the handle has three uses only:
/// `start_statement_rollback` (`start_full_rollback`) borrows it for C's one
/// RollbackStart; `rolled_back` gives it up once that rollback finished; and
/// dropping it hands it to C's fence and recovery obligations. A second
/// start is refused without calling C, so a started rollback is never
/// started again (G5).
#[derive(Debug)]
pub(crate) struct PendingRollback<H> {
    handle: H,
    progress: Progress,
}

impl<H> PendingRollback<H> {
    /// The handle, only after the rollback started from this pending
    /// finished; before that (not started, stopped, or fenced) the pending
    /// is returned unchanged.
    pub(crate) fn rolled_back(self) -> Result<H, Self> {
        match self.progress {
            Progress::Finished => Ok(self.handle),
            Progress::Required | Progress::Started => Err(self),
        }
    }

    /// The handle for C's RollbackStart and the progress the started
    /// rollback will move on. Refused, with no C call, unless no rollback
    /// was started yet; after an abandoned start it must be resolved.
    #[allow(
        clippy::result_large_err,
        reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
    )]
    fn start(&mut self) -> Result<(&H, &mut Progress), Failure> {
        match self.progress {
            Progress::Required => Ok((&self.handle, &mut self.progress)),
            progress => Err(Failure {
                must_resolve: progress == Progress::Started,
                ..Failure::new(Stage::RollbackStart, InsertError::OrderMismatch.into())
            }),
        }
    }
}

/// C's started rollback, tied to the `PendingRollback` it came from: its
/// successful finish is the only way that pending is marked finished.
pub(crate) struct StartedRollback<'r, K> {
    rollback: K,
    progress: &'r mut Progress,
}

impl<K: Rollback> Rollback for StartedRollback<'_, K> {
    type Target = K::Target;
    type Task<'x>
        = K::Task<'x>
    where
        Self: 'x;
    fn next(&mut self) -> Result<RollbackNext<Self::Task<'_>, K::Target>, RetentionError> {
        self.rollback.next()
    }
    fn finish(self) -> Result<(), RowOwnedError<Self>> {
        let Self { rollback, progress } = self;
        match rollback.finish() {
            Ok(()) => {
                *progress = Progress::Finished;
                Ok(())
            }
            Err(RowOwnedError::Before { error, owned }) => Err(RowOwnedError::Before {
                error,
                owned: Self {
                    rollback: owned,
                    progress,
                },
            }),
            Err(RowOwnedError::Fatal(error)) => Err(RowOwnedError::Fatal(error)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct DriveError<H> {
    pub(crate) failure: Failure,
    pub(crate) after: After<H>,
}

impl<H> DriveError<H> {
    fn new(failure: Failure, after: After<H>) -> Self {
        Self { failure, after }
    }

    /// A handle C gave back with its error: as `unchanged` says, unless the
    /// error left C's fence for the caller to resolve.
    fn kept(failure: Failure, handle: H, unchanged: fn(H) -> After<H>) -> Self {
        let after = if failure.must_resolve {
            After::MustResolve(Some(handle))
        } else {
            unchanged(handle)
        };
        Self::new(failure, after)
    }
}

/// Call arguments no table accepts, checked before any C call.
fn check_arguments(rows: &[&[ValueRef<'_>]], slots: usize) -> Result<(), InsertError> {
    if rows.is_empty()
        || rows.len() != slots
        || rows
            .iter()
            .any(|values| !(1..=MAX_ROW_COLUMNS).contains(&values.len()))
    {
        return Err(InsertError::InvalidInput);
    }
    Ok(())
}

/// One INSERT statement. The statement and fence are checked first, then the
/// call arguments; neither calls C. Each row's input is rejected before its
/// Reserve, and its reservation is consumed by apply or released by Cancel
/// before the next row's Reserve. On success `slots[i]` holds row i's
/// observation and the statement is finished. On failure the statement is
/// `Returned` when no row of this call was applied, otherwise
/// `RollbackRequired` with `slots` still naming the applied rows, which the
/// caller's statement rollback compensates. The driver starts no rollback.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn insert_statement<B: Boundary>(
    call: &mut B,
    statement: B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<B::Applied>],
) -> Result<usize, DriveError<B::Statement>> {
    if let Err(error) = statement.check() {
        let failure = Failure::new(Stage::Statement, error.into());
        return Err(DriveError::new(failure, After::Returned(statement)));
    }
    if let Err(error) = check_arguments(rows, slots.len()) {
        let failure = Failure::new(Stage::Arguments, error.into());
        return Err(DriveError::new(failure, After::Returned(statement)));
    }
    let mut failed = None;
    let mut applied_rows = 0;
    for (values, slot) in rows.iter().zip(slots.iter_mut()) {
        match insert_row(call, &statement, table, loan, values) {
            Ok(applied) => {
                *slot = Some(applied);
                applied_rows += 1;
            }
            Err(failure) => {
                failed = Some(failure);
                break;
            }
        }
    }
    let Some(failure) = failed else {
        return finish_statement(call, statement).map(|()| rows.len());
    };
    let after = if failure.must_resolve {
        After::MustResolve(Some(statement))
    } else if applied_rows == 0 {
        After::Returned(statement)
    } else {
        After::rollback_required(statement)
    };
    Err(DriveError::new(failure, after))
}

/// One row through the consuming wrappers. On failure nothing this row took
/// is left held: the preflight is finished, or the reservation cancelled,
/// unless C answered Fatal/CallerMustResolve or refused the release.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn insert_row<B: Boundary>(
    call: &mut B,
    statement: &B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    values: &[ValueRef<'_>],
) -> Result<B::Applied, Failure> {
    let mut preflight = match call.preflight(statement, table, loan) {
        Ok(PreflightNext::Ready(preflight)) => preflight,
        // The flush and the retry of the same row from preflight are not
        // supplied yet (A-W1): the row ends here with nothing held.
        Ok(PreflightNext::Flush(_)) => {
            return Err(Failure::new(
                Stage::Preflight,
                InsertError::Unsupported.into(),
            ));
        }
        Err(error) => return Err(Failure::new(Stage::Preflight, error.into())),
    };
    let input = match preflight.validation_lane() {
        Ok(lane) => lane
            .validate(values)
            .map_err(|error| Failure::new(Stage::Validate, error.into())),
        Err(error) => Err(Failure::new(Stage::ValidationLane, error.into())),
    };
    let input = match input {
        Ok(input) => input,
        Err(failure) => return Err(finish_preflight(preflight, failure)),
    };
    let mut reserved = match preflight.reserve(input) {
        Ok(reserved) => reserved,
        Err(RowOwnedError::Before { error, owned }) => {
            let failure = Failure::new(Stage::Reserve, error.into());
            return Err(finish_preflight(owned, failure));
        }
        Err(RowOwnedError::Fatal(error)) => return Err(Failure::fatal(Stage::Reserve, error)),
    };
    let proposal = match reserved.producer_lane() {
        Ok(lane) => lane
            .forward()
            .map_err(|error| Failure::new(Stage::Produce, error.into())),
        Err(error) => Err(Failure::new(Stage::ProducerLane, error.into())),
    };
    let proposal = match proposal {
        Ok(proposal) => proposal,
        Err(failure) => return Err(cancel(reserved, failure)),
    };
    let prepared = match reserved.prepare(proposal) {
        Ok(prepared) => prepared,
        Err(RowOwnedError::Before { error, owned }) => {
            return Err(cancel(owned, Failure::new(Stage::Prepare, error.into())));
        }
        Err(RowOwnedError::Fatal(error)) => return Err(Failure::fatal(Stage::Prepare, error)),
    };
    match prepared.apply() {
        Ok(applied) => Ok(applied),
        Err(RowOwnedError::Before { error, owned }) => Err(cancel(
            owned.reopen(),
            Failure::new(Stage::Apply, error.into()),
        )),
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::Apply, error)),
    }
}

/// Explicit no-effect release before the first Reserve. After a
/// CallerMustResolve answer no further C call is made; the preflight is
/// dropped, which C documents as the same no-effect release.
fn finish_preflight<P: Preflight>(preflight: P, failure: Failure) -> Failure {
    if failure.must_resolve {
        drop(preflight);
        return failure;
    }
    match preflight.finish() {
        Ok(()) => failure,
        Err(error) => failure.released(Stage::FinishPreflight, error.into(), false),
    }
}

/// Releases the one reservation after an unchanged failure. After a
/// CallerMustResolve failure no Cancel effect is attempted. A reservation C
/// refuses to cancel is dropped into C's fence and reported as must-resolve;
/// Drop never stands in for a successful Cancel.
fn cancel<R: Reserved>(reserved: R, failure: Failure) -> Failure {
    if failure.must_resolve {
        drop(reserved);
        return failure;
    }
    match reserved.cancel() {
        Ok(()) => failure,
        Err(RowOwnedError::Before { error, owned }) => {
            drop(owned);
            failure.released(Stage::Cancel, error.into(), true)
        }
        Err(RowOwnedError::Fatal(error)) => failure.released(Stage::Cancel, error.into(), true),
    }
}

/// Finishes a statement whose rows are all applied, or whose rollback has
/// finished (`PendingRollback::rolled_back`); a `RollbackRequired` statement
/// cannot reach it otherwise. A refusal hands it back `FinishPending`: the
/// retry is this call alone, never another rollback.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn finish_statement<B: Boundary>(
    call: &mut B,
    statement: B::Statement,
) -> Result<(), DriveError<B::Statement>> {
    match call.finish_statement(statement) {
        Ok(()) => Ok(()),
        Err(StatementFinishError::Before { error, view }) => Err(DriveError::kept(
            Failure::new(Stage::FinishStatement, error.into()),
            view,
            After::FinishPending,
        )),
        Err(StatementFinishError::Fatal(error)) => Err(DriveError::new(
            Failure::fatal(Stage::FinishStatement, error),
            After::MustResolve(None),
        )),
    }
}

/// Starts C's statement rollback to the statement's mark (RollbackStart). A
/// refusal starts nothing, so the statement stays `RollbackRequired` and the
/// start may be retried; a started rollback is never started again (the
/// pending refuses a second start without calling C).
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn start_statement_rollback<'r, 'i, B: Boundary>(
    call: &'r mut B,
    transaction: &'r B::Transaction,
    pending: &'r mut PendingRollback<B::Statement>,
    loan: &'i mut Loan,
) -> Result<StartedRollback<'r, B::Rollback<'r, 'i>>, Failure> {
    let (statement, progress) = pending.start()?;
    let rollback = call
        .start_rollback(transaction, Some(statement), RollbackScope::Statement, loan)
        .map_err(|error| Failure::new(Stage::RollbackStart, error.into()))?;
    *progress = Progress::Started;
    Ok(StartedRollback { rollback, progress })
}

/// Starts the full rollback of every remaining row, the only step after a
/// commit C answered `RollbackRequired`. Refusals are as for a statement.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn start_full_rollback<'r, 'i, B: Boundary>(
    call: &'r mut B,
    pending: &'r mut PendingRollback<B::Transaction>,
    loan: &'i mut Loan,
) -> Result<StartedRollback<'r, B::Rollback<'r, 'i>>, Failure> {
    let (transaction, progress) = pending.start()?;
    let rollback = call
        .start_rollback(transaction, None, RollbackScope::Transaction, loan)
        .map_err(|error| Failure::new(Stage::RollbackStart, error.into()))?;
    *progress = Progress::Started;
    Ok(StartedRollback { rollback, progress })
}

/// One rollback step's progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stepped {
    /// C's candidate got A's CLR, was prepared and applied.
    Compensated,
    /// C reports the mark (or None) reached; `finish_rollback` is next.
    Complete,
}

/// Where a rollback step stopped (gap G5). The contract gives a started live
/// rollback no resume or drop meaning, so the driver drops nothing C hands
/// back in order to restart: an unchanged refusal leaves the caller holding
/// the same rollback or the same task for retry, a flush target is passed
/// out unchanged, and a Fatal or CallerMustResolve answer ends every call.
pub(crate) enum RollbackStop<T, F> {
    /// C refused `next` unchanged; the caller's rollback is as it was and
    /// `rollback_step` may be called on it again.
    Next(RetentionError),
    /// C needs this target flushed before its next candidate. The rollback
    /// holds the session's only Owner borrow and offers no flush-through
    /// call (G1), so the caller keeps both; nothing is dropped here.
    Flush(F),
    /// The same task after an unchanged refusal of its lane, A's CLR, its
    /// prepare, or its apply (then reopened): retry it with `retry_task`.
    Task {
        stage: Stage,
        error: DriverError,
        task: T,
    },
    /// C answered Fatal or CallerMustResolve: no further call; the rollback
    /// and the handle go to C's fence and recovery obligations.
    MustResolve { stage: Stage, error: DriverError },
}

/// C's next candidate and, for a candidate, A's CLR through prepare and
/// apply. The caller owns the rollback and loops; a stopped step hands back
/// what C returned, borrowing the caller's rollback.
fn rollback_step<'x, K: Rollback>(
    rollback: &'x mut K,
) -> Result<Stepped, RollbackStop<K::Task<'x>, K::Target>> {
    let task = match rollback.next() {
        Ok(RollbackNext::Candidate(task)) => task,
        Ok(RollbackNext::Flush(target)) => return Err(RollbackStop::Flush(target)),
        Ok(RollbackNext::Complete) => return Ok(Stepped::Complete),
        Err(error) if error.effect == Effect::CallerMustResolve => {
            return Err(RollbackStop::MustResolve {
                stage: Stage::RollbackNext,
                error: error.into(),
            });
        }
        Err(error) => return Err(RollbackStop::Next(error)),
    };
    compensate(task).map(|()| Stepped::Compensated)
}

/// Retries the same task a step handed back: lane → A's CLR → prepare →
/// apply again. It has no flush step.
fn retry_task<T: RollbackTask>(task: T) -> Result<(), RollbackStop<T, Infallible>> {
    compensate(task)
}

/// One CLR for the head C selected.
fn compensate<T: RollbackTask, F>(mut task: T) -> Result<(), RollbackStop<T, F>> {
    let proposal = match task.producer_lane() {
        Ok(lane) => lane
            .clr()
            .map_err(|error| (Stage::Clr, DriverError::from(error))),
        Err(error) => Err((Stage::ClrLane, error.into())),
    };
    let proposal = match proposal {
        Ok(proposal) => proposal,
        Err((stage, error)) => return Err(held(stage, error, task)),
    };
    let prepared = match task.prepare(proposal) {
        Ok(prepared) => prepared,
        Err(RowOwnedError::Before { error, owned }) => {
            return Err(held(Stage::ClrPrepare, error.into(), owned));
        }
        Err(RowOwnedError::Fatal(error)) => {
            return Err(RollbackStop::MustResolve {
                stage: Stage::ClrPrepare,
                error: error.into(),
            });
        }
    };
    match prepared.apply() {
        Ok(()) => Ok(()),
        Err(RowOwnedError::Before { error, owned }) => {
            Err(held(Stage::ClrApply, error.into(), owned.reopen()))
        }
        Err(RowOwnedError::Fatal(error)) => Err(RollbackStop::MustResolve {
            stage: Stage::ClrApply,
            error: error.into(),
        }),
    }
}

/// An unchanged refusal hands the same task back; after a CallerMustResolve
/// answer it is dropped into C's fence and nothing else is called.
fn held<T, F>(stage: Stage, error: DriverError, task: T) -> RollbackStop<T, F> {
    if error.must_resolve() {
        drop(task);
        RollbackStop::MustResolve { stage, error }
    } else {
        RollbackStop::Task { stage, error, task }
    }
}

/// C's finish of a complete rollback. Before returns the same rollback for
/// retry; after it succeeds the pending gives its handle up
/// (`PendingRollback::rolled_back`) and only `finish_statement` /
/// `finish_aborted` remains.
fn finish_rollback<K: Rollback>(rollback: K) -> Result<(), RowOwnedError<K>> {
    rollback.finish()
}

/// Commit. A write conflict (`RollbackRequired`) leaves A only the full
/// rollback: the transaction is handed back inside a `PendingRollback` for
/// `start_full_rollback`.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn commit<B: Boundary>(
    call: &mut B,
    transaction: B::Transaction,
    loan: &mut Loan,
) -> Result<B::Committed, DriveError<B::Transaction>> {
    match call.commit(transaction, loan) {
        Ok(committed) => Ok(committed),
        Err(TransactionFinishError::Before { error, transaction }) => Err(DriveError::kept(
            Failure::new(Stage::Commit, error.into()),
            transaction,
            After::Returned,
        )),
        Err(TransactionFinishError::RollbackRequired { error, transaction }) => {
            Err(DriveError::kept(
                Failure::new(Stage::Commit, error.into()),
                transaction,
                After::rollback_required,
            ))
        }
        Err(TransactionFinishError::Rejected(error)) => Err(DriveError::new(
            Failure::new(Stage::Commit, error.into()),
            After::Closed,
        )),
        Err(TransactionFinishError::Fatal(error)) => Err(DriveError::new(
            Failure::fatal(Stage::Commit, error),
            After::MustResolve(None),
        )),
    }
}

/// Consumes the transaction after its full rollback finished. A Before is
/// retried by this call alone. C answering `RollbackRequired` here, after
/// that rollback finished, has no defined continuation: a second full
/// RollbackStart would restart a finished rollback (G5), so the transaction
/// is kept for C's fence (`MustResolve`).
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn finish_aborted<B: Boundary>(
    call: &mut B,
    transaction: B::Transaction,
) -> Result<(), DriveError<B::Transaction>> {
    match call.finish_aborted(transaction) {
        Ok(()) => Ok(()),
        Err(TransactionFinishError::Before { error, transaction }) => Err(DriveError::kept(
            Failure::new(Stage::FinishAborted, error.into()),
            transaction,
            After::FinishPending,
        )),
        Err(TransactionFinishError::RollbackRequired { error, transaction }) => {
            Err(DriveError::new(
                Failure::fatal(Stage::FinishAborted, error),
                After::MustResolve(Some(transaction)),
            ))
        }
        Err(TransactionFinishError::Rejected(error)) => Err(DriveError::new(
            Failure::new(Stage::FinishAborted, error.into()),
            After::Closed,
        )),
        Err(TransactionFinishError::Fatal(error)) => Err(DriveError::new(
            Failure::fatal(Stage::FinishAborted, error),
            After::MustResolve(None),
        )),
    }
}

/// Nonrecursive size_of inventory, in the repository's source-bound sense, of
/// the driver's carriers at the deepest point of the INSERT path (A's
/// Forward producer under `insert_statement` → `insert_row` → the forwarding
/// `forward`) and of the CLR path (A's CLR producer under the caller's held
/// rollback → `rollback_step` → `compensate` → the forwarding `clr`).
/// Wrapper results passed by value count twice (the driver's slot and the
/// forwarding frame's return), the lane moved into the forwarding frame is
/// counted there, and the standard pointer/result/scalar slack is added once
/// per path. It is not a machine-stack measurement: unoptimized frames keep a
/// slot for every temporary and are several times larger (r1 record). The
/// wrappers carry their final cuts, read gate and flush lease and the lane
/// its eighth part (S2-a), so this count reads the actual types. A's
/// producer frame and C's internals are not included.
const fn carrier_bytes<B: Boundary + 'static>() -> (usize, usize) {
    use std::mem::size_of;
    type P<B> = <B as Boundary>::Preflight<'static, 'static>;
    type G<B> = <B as Boundary>::Target;
    type V<B> = <P<B> as Preflight>::Lane<'static>;
    type R<B> = <P<B> as Preflight>::Reserved;
    type L<B> = <R<B> as Reserved>::Lane<'static>;
    type D<B> = <R<B> as Reserved>::Prepared;
    type A<B> = <B as Boundary>::Applied;
    type S<B> = <B as Boundary>::Statement;
    type K<B> = StartedRollback<'static, <B as Boundary>::Rollback<'static, 'static>>;
    type F<B> = <K<B> as Rollback>::Target;
    type T<B> = <K<B> as Rollback>::Task<'static>;
    type M<B> = <T<B> as RollbackTask>::Lane<'static>;
    type Q<B> = <T<B> as RollbackTask>::Prepared;
    let slack = 32 * size_of::<&()>() + 16 * size_of::<Result<(), RetentionError>>() + 512;
    let statement = size_of::<&mut B>()
        + size_of::<&B::Table>()
        + size_of::<&mut Loan>()
        + 2 * size_of::<&[&[ValueRef<'static>]]>()
        + size_of::<&[ValueRef<'static>]>()
        + size_of::<S<B>>()
        + size_of::<Result<(), RetentionError>>()
        + size_of::<Result<(), InsertError>>()
        + size_of::<Option<Failure>>()
        + size_of::<Result<A<B>, Failure>>()
        + size_of::<Result<(), DriveError<S<B>>>>()
        + size_of::<Result<usize, DriveError<S<B>>>>()
        + 2 * size_of::<usize>();
    let row = 2 * size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
        + size_of::<P<B>>()
        + size_of::<Result<V<B>, RetentionError>>()
        + 2 * size_of::<Result<RowInputPlan, Failure>>()
        + 2 * size_of::<Result<R<B>, RowOwnedError<P<B>>>>()
        + size_of::<R<B>>()
        + size_of::<Result<L<B>, RetentionError>>()
        + 2 * size_of::<Result<RowProposal, Failure>>()
        + 2 * size_of::<Result<D<B>, RowOwnedError<R<B>>>>()
        + 2 * size_of::<Result<A<B>, RowOwnedError<D<B>>>>()
        + 2 * size_of::<Failure>()
        + size_of::<L<B>>()
        + size_of::<Result<RowProposal, InsertError>>();
    let held = size_of::<&mut B>()
        + 2 * size_of::<&()>()
        + size_of::<&mut Loan>()
        + size_of::<Result<K<B>, Failure>>()
        + size_of::<K<B>>()
        + size_of::<Result<Stepped, RollbackStop<T<B>, F<B>>>>();
    let step = size_of::<&mut K<B>>()
        + 2 * size_of::<Result<RollbackNext<T<B>, F<B>>, RetentionError>>()
        + size_of::<T<B>>()
        + size_of::<Result<M<B>, RetentionError>>()
        + 2 * size_of::<Result<RowProposal, (Stage, DriverError)>>()
        + 2 * size_of::<Result<Q<B>, RowOwnedError<T<B>>>>()
        + 2 * size_of::<Result<(), RowOwnedError<Q<B>>>>()
        + 2 * size_of::<Result<(), RollbackStop<T<B>, F<B>>>>()
        + size_of::<M<B>>()
        + size_of::<Result<RowProposal, InsertError>>();
    (statement + row + slack, held + step + slack)
}
