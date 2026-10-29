//! A's INSERT statement driver over the C0 Row write boundary.
//!
//! A owns the statement flow; C owns every source, reservation, stage,
//! install, rollback head and terminal effect. Per row the driver calls
//! preflight → validation lane → A `validate_insert` → RowReserve → producer
//! lane → A `produce_insert` → prepare → apply. A row failure releases what
//! that row holds (preflight finish, or Cancel of the one reservation) before
//! anything else; a statement with applied rows is then compensated to its
//! mark by C's rollback with A `produce_clr` candidates and finished. Commit
//! and full abort follow the same consuming wrappers. C is reached only
//! through the private traits below: `live` forwards them to the actual C0
//! types and the tests feed literal stand-ins. See
//! `devdoc/contracts/dev3-module-boundaries.md` (6.2, 6.6, 6.7) and the F02
//! table in `dev3-task-06-tree-rows.md`.
use super::InsertError;
use crate::storage::{
    disk::{
        RetentionError, RollbackScope, RowInputPlan, RowOwnedError, RowProposal,
        StatementFinishError, TransactionFinishError,
    },
    row_record::{MAX_ROW_COLUMNS, ValueRef},
};
use foundation::error::Effect;

mod live;

/// The one private operation loan C lends to each preflight and rollback.
type Loan = [u8; 425_984];

/// Claimed bound for the driver's own frames below A's producer. The source
/// census of the C3 supply adds it to the producer frame; it is not a grant.
pub(super) const DRIVER_SOURCE_BYTES: usize = 16_384;

/// The original statement view as the driver uses it.
trait StatementView {
    /// Effect-free statement identity, phase and fence check.
    fn check(&self) -> Result<(), RetentionError>;
}

/// C's entries over the caller's original MutationPort and Owner. Each
/// wrapper borrows them until it is consumed; nothing is acquired twice.
trait Boundary {
    type Transaction;
    type Statement: StatementView;
    type Table;
    type Applied;
    type Committed;
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
    ) -> Result<Self::Preflight<'r, 'i>, RetentionError>;
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
    /// Returns the admission and loan; the observed read set stays.
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
enum RollbackNext<T> {
    Candidate(T),
    /// C needs a page flushed before the next candidate (see `Gap`).
    Flush,
    Complete,
}

/// C's live rollback over the actual head chain; C picks each next U.
trait Rollback: Sized {
    type Task<'x>: RollbackTask
    where
        Self: 'x;
    fn next(&mut self) -> Result<RollbackNext<Self::Task<'_>>, RetentionError>;
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

/// A step the C0 boundary gives A no means to perform; left unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gap {
    /// RowRollback holds the session's only Owner borrow, which a flush
    /// needs, and offers no flush-through call.
    RollbackFlush,
}

/// A driver error: A's own `InsertError`, or C's error kept by value with
/// its phase, cause (Busy/Capacity/Unsupported stay distinct) and effect.
#[derive(Debug)]
pub(crate) enum DriverError {
    Insert(InsertError),
    Disk(RetentionError),
    Gap(Gap),
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
    RollbackFinish,
    FinishStatement,
    Commit,
    FinishAborted,
}

/// The first error and, if a release or rollback after it also failed,
/// that later error.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) stage: Stage,
    pub(crate) error: DriverError,
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

    fn fatal(stage: Stage, error: RetentionError) -> Self {
        Self {
            must_resolve: true,
            ..Self::new(stage, error.into())
        }
    }

    fn then(mut self, stage: Stage, error: DriverError, must_resolve: bool) -> Self {
        self.must_resolve |= must_resolve || error.must_resolve();
        self.cleanup.get_or_insert((stage, error));
        self
    }

    /// A later failure of the same call keeps only its first error.
    fn then_failure(self, later: Failure) -> Self {
        self.then(later.stage, later.error, later.must_resolve)
    }
}

/// What a failed call leaves with its statement or transaction.
#[derive(Debug)]
pub(crate) enum After<H> {
    /// Nothing of this call remains; the handle is returned as it was.
    Returned(H),
    /// C closed the handle; no row of this call remains.
    Closed,
    /// Rows or a rollback of this call remain; returned for the caller to
    /// retry the rollback or the finish named by the failure's stage.
    Open(H),
    /// Only for dropping into C's fence and recovery obligations.
    MustResolve(Option<H>),
}

#[derive(Debug)]
pub(crate) struct DriveError<H> {
    pub(crate) failure: Failure,
    /// CLRs C applied for this call before it ended.
    pub(crate) rolled_back: usize,
    pub(crate) after: After<H>,
}

impl<H> DriveError<H> {
    fn new(failure: Failure, rolled_back: usize, after: After<H>) -> Self {
        Self {
            failure,
            rolled_back,
            after,
        }
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
/// observation and the statement is finished. A failure with no row of this
/// call applied returns the statement as it was; applied rows are
/// compensated and the statement finished, and the slots are cleared.
fn insert_statement<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    statement: B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<B::Applied>],
) -> Result<usize, DriveError<B::Statement>> {
    if let Err(error) = statement.check() {
        let failure = Failure::new(Stage::Statement, error.into());
        return Err(DriveError::new(failure, 0, After::Returned(statement)));
    }
    if let Err(error) = check_arguments(rows, slots.len()) {
        let failure = Failure::new(Stage::Arguments, error.into());
        return Err(DriveError::new(failure, 0, After::Returned(statement)));
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
        return match call.finish_statement(statement) {
            Ok(()) => Ok(rows.len()),
            Err(StatementFinishError::Before { error, view }) => Err(DriveError::new(
                Failure::new(Stage::FinishStatement, error.into()),
                0,
                After::Open(view),
            )),
            Err(StatementFinishError::Fatal(error)) => Err(DriveError::new(
                Failure::fatal(Stage::FinishStatement, error),
                0,
                After::MustResolve(None),
            )),
        };
    };
    if failure.must_resolve {
        return Err(DriveError::new(
            failure,
            0,
            After::MustResolve(Some(statement)),
        ));
    }
    if applied_rows == 0 {
        return Err(DriveError::new(failure, 0, After::Returned(statement)));
    }
    match rollback_statement(call, transaction, statement, loan) {
        Ok(rolled_back) => {
            slots.iter_mut().for_each(|slot| *slot = None);
            Err(DriveError::new(failure, rolled_back, After::Closed))
        }
        Err(rollback) => {
            // Rows of this call stay applied until a rollback completes.
            let after = match rollback.after {
                After::Returned(statement) => After::Open(statement),
                after => after,
            };
            let failure = failure.then_failure(rollback.failure);
            Err(DriveError::new(failure, rollback.rolled_back, after))
        }
    }
}

/// One row through the consuming wrappers. On failure nothing this row took
/// is left held: the preflight is finished, or the reservation cancelled,
/// unless C answered Fatal/CallerMustResolve or refused the release.
fn insert_row<B: Boundary>(
    call: &mut B,
    statement: &B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    values: &[ValueRef<'_>],
) -> Result<B::Applied, Failure> {
    let mut preflight = call
        .preflight(statement, table, loan)
        .map_err(|error| Failure::new(Stage::Preflight, error.into()))?;
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

/// Explicit no-effect release before the first Reserve.
fn finish_preflight<P: Preflight>(preflight: P, failure: Failure) -> Failure {
    match preflight.finish() {
        Ok(()) => failure,
        Err(error) => failure.then(Stage::FinishPreflight, error.into(), false),
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
            failure.then(Stage::Cancel, error.into(), true)
        }
        Err(RowOwnedError::Fatal(error)) => failure.then(Stage::Cancel, error.into(), true),
    }
}

/// Compensates the statement's rows to its mark, then finishes it. Also the
/// caller's retry entry for a statement returned `Open`.
fn rollback_statement<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    statement: B::Statement,
    loan: &mut Loan,
) -> Result<usize, DriveError<B::Statement>> {
    let rolled_back = match roll_back(
        call,
        transaction,
        Some(&statement),
        RollbackScope::Statement,
        loan,
    ) {
        Ok(rolled_back) => rolled_back,
        Err((rolled_back, failure)) => {
            let after = stopped(&failure, rolled_back, statement);
            return Err(DriveError::new(failure, rolled_back, after));
        }
    };
    match call.finish_statement(statement) {
        Ok(()) => Ok(rolled_back),
        Err(StatementFinishError::Before { error, view }) => Err(DriveError::new(
            Failure::new(Stage::FinishStatement, error.into()),
            rolled_back,
            After::Open(view),
        )),
        Err(StatementFinishError::Fatal(error)) => Err(DriveError::new(
            Failure::fatal(Stage::FinishStatement, error),
            rolled_back,
            After::MustResolve(None),
        )),
    }
}

/// The handle after a rollback that did not complete.
fn stopped<H>(failure: &Failure, rolled_back: usize, handle: H) -> After<H> {
    if failure.must_resolve {
        After::MustResolve(Some(handle))
    } else if rolled_back == 0 && failure.stage == Stage::RollbackStart {
        After::Returned(handle)
    } else {
        After::Open(handle)
    }
}

/// C's rollback loop: each candidate C selects gets A's CLR, prepare and
/// apply; `finish` requires C's mark/None and terminal receipt. Any refusal
/// stops the loop without retry and returns the CLR count so far.
fn roll_back<B: Boundary>(
    call: &mut B,
    transaction: &B::Transaction,
    statement: Option<&B::Statement>,
    scope: RollbackScope,
    loan: &mut Loan,
) -> Result<usize, (usize, Failure)> {
    let mut rollback = call
        .start_rollback(transaction, statement, scope, loan)
        .map_err(|error| (0, Failure::new(Stage::RollbackStart, error.into())))?;
    let mut rolled_back = 0;
    loop {
        let task = match rollback.next() {
            Ok(RollbackNext::Candidate(task)) => task,
            Ok(RollbackNext::Flush) => {
                let gap = DriverError::Gap(Gap::RollbackFlush);
                return Err((rolled_back, Failure::new(Stage::RollbackNext, gap)));
            }
            Ok(RollbackNext::Complete) => break,
            Err(error) => {
                return Err((rolled_back, Failure::new(Stage::RollbackNext, error.into())));
            }
        };
        compensate(task).map_err(|failure| (rolled_back, failure))?;
        rolled_back += 1;
    }
    match rollback.finish() {
        Ok(()) => Ok(rolled_back),
        Err(RowOwnedError::Before { error, owned }) => {
            drop(owned);
            Err((
                rolled_back,
                Failure::new(Stage::RollbackFinish, error.into()),
            ))
        }
        Err(RowOwnedError::Fatal(error)) => {
            Err((rolled_back, Failure::fatal(Stage::RollbackFinish, error)))
        }
    }
}

/// One CLR for the head C selected.
fn compensate<T: RollbackTask>(mut task: T) -> Result<(), Failure> {
    let proposal = match task.producer_lane() {
        Ok(lane) => lane
            .clr()
            .map_err(|error| Failure::new(Stage::Clr, error.into())),
        Err(error) => Err(Failure::new(Stage::ClrLane, error.into())),
    }?;
    let prepared = match task.prepare(proposal) {
        Ok(prepared) => prepared,
        Err(RowOwnedError::Before { error, .. }) => {
            return Err(Failure::new(Stage::ClrPrepare, error.into()));
        }
        Err(RowOwnedError::Fatal(error)) => return Err(Failure::fatal(Stage::ClrPrepare, error)),
    };
    match prepared.apply() {
        Ok(()) => Ok(()),
        Err(RowOwnedError::Before { error, owned }) => {
            drop(owned.reopen());
            Err(Failure::new(Stage::ClrApply, error.into()))
        }
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::ClrApply, error)),
    }
}

/// Commit; a write conflict leaves A only the full rollback, which this
/// call then performs.
fn commit<B: Boundary>(
    call: &mut B,
    transaction: B::Transaction,
    loan: &mut Loan,
) -> Result<B::Committed, DriveError<B::Transaction>> {
    match call.commit(transaction, loan) {
        Ok(committed) => Ok(committed),
        Err(TransactionFinishError::Before { error, transaction }) => Err(DriveError::new(
            Failure::new(Stage::Commit, error.into()),
            0,
            After::Returned(transaction),
        )),
        Err(TransactionFinishError::RollbackRequired { error, transaction }) => {
            let failure = Failure::new(Stage::Commit, error.into());
            match abort(call, transaction, loan) {
                Ok(rolled_back) => Err(DriveError::new(failure, rolled_back, After::Closed)),
                Err(abort) => {
                    let after = match abort.after {
                        After::Returned(transaction) => After::Open(transaction),
                        after => after,
                    };
                    let failure = failure.then_failure(abort.failure);
                    Err(DriveError::new(failure, abort.rolled_back, after))
                }
            }
        }
        Err(TransactionFinishError::Rejected(error)) => Err(DriveError::new(
            Failure::new(Stage::Commit, error.into()),
            0,
            After::Closed,
        )),
        Err(TransactionFinishError::Fatal(error)) => Err(DriveError::new(
            Failure::fatal(Stage::Commit, error),
            0,
            After::MustResolve(None),
        )),
    }
}

/// Full abort: every remaining row of the transaction is compensated, then
/// the capsule is consumed by `finish_aborted`.
fn abort<B: Boundary>(
    call: &mut B,
    transaction: B::Transaction,
    loan: &mut Loan,
) -> Result<usize, DriveError<B::Transaction>> {
    let rolled_back = match roll_back(call, &transaction, None, RollbackScope::Transaction, loan) {
        Ok(rolled_back) => rolled_back,
        Err((rolled_back, failure)) => {
            let after = stopped(&failure, rolled_back, transaction);
            return Err(DriveError::new(failure, rolled_back, after));
        }
    };
    match call.finish_aborted(transaction) {
        Ok(()) => Ok(rolled_back),
        Err(
            TransactionFinishError::Before { error, transaction }
            | TransactionFinishError::RollbackRequired { error, transaction },
        ) => Err(DriveError::new(
            Failure::new(Stage::FinishAborted, error.into()),
            rolled_back,
            After::Open(transaction),
        )),
        Err(TransactionFinishError::Rejected(error)) => Err(DriveError::new(
            Failure::new(Stage::FinishAborted, error.into()),
            rolled_back,
            After::Closed,
        )),
        Err(TransactionFinishError::Fatal(error)) => Err(DriveError::new(
            Failure::fatal(Stage::FinishAborted, error),
            rolled_back,
            After::MustResolve(None),
        )),
    }
}

/// Nonrecursive inventory of the driver's own carriers at the deepest point
/// of the INSERT path (A's Forward producer running) and of the CLR path
/// (A's CLR producer running inside a statement rollback). Every local and
/// temporary is counted once without slot sharing, so this bounds an
/// unoptimized frame from above; A's producer frame and C's internals are
/// not included and are added by the source census of the C3 supply.
fn carrier_bytes<B: Boundary + 'static>() -> (usize, usize) {
    use std::mem::size_of;
    type P<B> = <B as Boundary>::Preflight<'static, 'static>;
    type R<B> = <P<B> as Preflight>::Reserved;
    type D<B> = <R<B> as Reserved>::Prepared;
    type A<B> = <B as Boundary>::Applied;
    type S<B> = <B as Boundary>::Statement;
    type K<B> = <B as Boundary>::Rollback<'static, 'static>;
    type T<B> = <K<B> as Rollback>::Task<'static>;
    type Q<B> = <T<B> as RollbackTask>::Prepared;
    let arguments = size_of::<&mut B>()
        + size_of::<&B::Transaction>()
        + size_of::<&B::Table>()
        + size_of::<&mut Loan>()
        + 2 * size_of::<&[&[ValueRef<'static>]]>()
        + size_of::<&[ValueRef<'static>]>();
    let statement = arguments
        + size_of::<S<B>>()
        + size_of::<Result<(), RetentionError>>()
        + size_of::<Result<(), InsertError>>()
        + size_of::<Option<Failure>>()
        + size_of::<Result<A<B>, Failure>>()
        + size_of::<Result<(), StatementFinishError<S<B>>>>()
        + size_of::<Result<usize, DriveError<S<B>>>>()
        + 2 * size_of::<usize>();
    let row = size_of::<Result<P<B>, RetentionError>>()
        + size_of::<P<B>>()
        + size_of::<Result<<P<B> as Preflight>::Lane<'static>, RetentionError>>()
        + 2 * size_of::<Result<RowInputPlan, Failure>>()
        + size_of::<Result<R<B>, RowOwnedError<P<B>>>>()
        + size_of::<R<B>>()
        + size_of::<Result<<R<B> as Reserved>::Lane<'static>, RetentionError>>()
        + 2 * size_of::<Result<RowProposal, Failure>>()
        + size_of::<Result<D<B>, RowOwnedError<R<B>>>>()
        + size_of::<Result<A<B>, RowOwnedError<D<B>>>>()
        + 2 * size_of::<Failure>();
    let rollback = size_of::<Result<usize, (usize, Failure)>>()
        + size_of::<Result<(), StatementFinishError<S<B>>>>()
        + size_of::<After<S<B>>>()
        + size_of::<Result<K<B>, RetentionError>>()
        + size_of::<K<B>>()
        + size_of::<Result<RollbackNext<T<B>>, RetentionError>>()
        + size_of::<Result<(), RowOwnedError<K<B>>>>()
        + size_of::<T<B>>()
        + size_of::<Result<<T<B> as RollbackTask>::Lane<'static>, RetentionError>>()
        + size_of::<Result<RowProposal, Failure>>()
        + size_of::<Result<Q<B>, RowOwnedError<T<B>>>>()
        + size_of::<Result<(), RowOwnedError<Q<B>>>>()
        + 3 * size_of::<Failure>();
    (statement + row, statement + rollback)
}
