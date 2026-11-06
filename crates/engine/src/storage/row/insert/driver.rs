//! A's INSERT statement driver over the C0 Row write boundary.
//!
//! A owns the statement flow; C owns every source, reservation, stage,
//! install, rollback head and terminal effect. Per row the driver calls
//! preflight → validation lane → A `validate_insert` → RowReserve → producer
//! lane → A `produce_insert` → prepare → apply. A preflight that answers
//! FlushRequired holds nothing: the driver flushes C's target with the
//! call's dedicated flusher Owner, continues the flush until the flusher is
//! Ready, and tries the same row again from preflight. A row failure releases what
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
    row_record::{MAX_ROW_COLUMNS, ValueRef, canonical_values_len},
    semantic_row::Operation,
};
use foundation::{
    error::Effect,
    identity::{ObjectHandle, RowId},
};
use std::convert::Infallible;

pub(in crate::storage) mod autocommit;
#[cfg(test)]
mod budget_tests;
pub(in crate::storage) mod explicit;
pub(in crate::storage) mod live;
pub(in crate::storage) mod recovered;
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
/// The flush entries use the flusher with the dedicated flusher Owner
/// (FLUSHER_SLOT, `require_flusher`), never the statement's Owner.
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
    /// C's DELETE preflight of `row` (`preflight_delete`,
    /// dev3-row-delete.md#api): `preflight`'s answers, and `NoRow` for a
    /// target that is no row under the statement snapshot.
    fn preflight_delete<'r, 'i>(
        &'r mut self,
        statement: &'r Self::Statement,
        table: &'r Self::Table,
        row: RowId,
        loan: &'i mut Loan,
    ) -> Result<PreflightNext<Self::Preflight<'r, 'i>, Self::Target>, RetentionError>;
    /// C's UPDATE preflight of `row` (`preflight_update`,
    /// dev3-row-update.md#api): `preflight_delete`'s answers by the same
    /// classification table.
    fn preflight_update<'r, 'i>(
        &'r mut self,
        statement: &'r Self::Statement,
        table: &'r Self::Table,
        row: RowId,
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
    /// `try_flush(flush_owner, &target)` on the call's flusher.
    fn try_flush(&mut self, target: &Self::Target) -> Result<(), RetentionError>;
    /// `continue_flush(flush_owner)` on the call's flusher, and whether it
    /// moved the flusher's phase on.
    fn continue_flush(&mut self) -> Result<FlushStep, RetentionError>;
    /// The flusher's phase is Ready; preflight refuses any other with Busy.
    fn flush_ready(&self) -> bool;
    /// C's next live Row cleanup step (Job ordinal, group) outside `skip`,
    /// or None (`row_cleanup_next`).
    fn cleanup_next(&self, skip: &[usize]) -> Result<Option<(usize, Operation)>, RetentionError>;
    /// One live Row cleanup group (`retire_row`) with the statement Owner and
    /// the call's flusher as its flush lease.
    fn retire(
        &mut self,
        ordinal: usize,
        operation: Operation,
        loan: &mut Loan,
    ) -> Result<RetireNext<Self::Target>, RetentionError>;
    /// C's live split for a row of `table` refused for its leaf's room: A's
    /// value check refused `InsertError::Capacity`, or C refused its Reserve
    /// `Unchanged(Prepare, Capacity)` (`split_row_live`,
    /// dev3-tree-smo.md#automatic-split). `row` is an UPDATE's target and
    /// None an INSERT's, `length` the row value's encoded length. It runs
    /// with the statement Owner and the call's flusher as its flush lease.
    fn split(
        &mut self,
        table: &Self::Table,
        row: Option<RowId>,
        length: usize,
        loan: &mut Loan,
    ) -> Result<SplitNext<Self::Target>, RetentionError>;
    /// C's first kept unlink note, as (table, the RowId whose removal left
    /// its routed RowLeaf empty), or None (`unlink_next`,
    /// dev3-tree-smo.md#empty-leaf-unlink). C first drops the notes of a
    /// table with Catalog debt.
    fn unlink_next(&mut self) -> Result<Option<(ObjectHandle, RowId)>, RetentionError>;
    /// C's live unlink for that note (`unlink_leaf_live`) with the statement
    /// Owner and the call's flusher as its flush lease.
    fn unlink(
        &mut self,
        table: ObjectHandle,
        key: RowId,
        loan: &mut Loan,
    ) -> Result<UnlinkNext<Self::Target>, RetentionError>;
}

/// What C answered for one live split (dev3-tree-smo.md#automatic-split).
#[derive(Debug)]
enum SplitNext<F> {
    /// One RootSplit or ChildSplit is installed and durable: the same row is
    /// tried again from preflight.
    Split,
    /// The residency of its target set needs this dirty victim flushed
    /// first; nothing was prepared, staged or pinned.
    Flush(F),
    /// The routed leaf already holds the row beside its reservation by C's
    /// rule: the row's failure stands.
    NotNeeded,
}

/// The splits one row may ask before its Capacity failure ends it
/// (dev3-tree-smo.md#automatic-split). Each split at most halves a full
/// leaf or lifts a full parent's split one level, so a row of a valid size
/// needs far fewer.
const MAX_ROW_SPLITS: usize = 16;

/// What C answered for one live Row cleanup group.
#[derive(Debug)]
enum RetireNext<F> {
    /// The group is installed and durable.
    Retired,
    /// The residency of its target set needs this dirty victim flushed
    /// first; nothing was prepared, staged or pinned.
    Flush(F),
    /// C refused it unchanged with `Visibility(Busy)`: a live reference pin
    /// or an older snapshot still holds its target.
    Held,
}

/// What C answered for one live unlink (dev3-tree-smo.md#empty-leaf-unlink).
#[derive(Debug)]
enum UnlinkNext<F> {
    /// One LeafUnlink or RootCollapse is installed and durable; C dropped
    /// its note.
    Unlinked,
    /// The residency of its pages needs this dirty victim flushed first;
    /// nothing was prepared or staged and the note stays.
    Flush(F),
    /// The tree needs no unlink there; C dropped the note.
    NotNeeded,
    /// C refused it unchanged with a Busy-class cause
    /// (`RetentionError::unlink_busy`); the note stays.
    Held,
}

/// What one `continue_flush` did to the flusher's phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlushStep {
    /// The phase moved on (to Ready when the flush finished).
    Advanced,
    /// The phase is unchanged: the flush waits on another thread's progress
    /// (the WAL owner's sync, a WAL writer, file or replay lock, a retryable
    /// finalize).
    Waiting,
}

/// What C's preflight returned.
enum PreflightNext<P, F> {
    Ready(P),
    /// C needs `F` flushed before the same row is tried again from
    /// preflight. Nothing is held: C has returned its source, gate and flush
    /// lease, and nothing took effect.
    Flush(F),
    /// A DELETE's or an UPDATE's target is no row under the statement
    /// snapshot: C has registered the table read and returned its source,
    /// gate and lease, and nothing took effect
    /// (dev3-row-delete.md#classification, dev3-row-update.md
    /// #classification). An INSERT's preflight never answers it.
    NoRow,
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
    /// A DELETE preflight's only first effect (`reserve_delete`): C reserves
    /// the target it classified, and A validates nothing.
    fn reserve_delete(self) -> Result<Self::Reserved, RowOwnedError<Self>>;
    /// An UPDATE preflight's only first effect (`reserve_update`): C
    /// reserves the target it classified for the new value `input`, which A
    /// checked over the same validation lane as an INSERT's; the plan is
    /// untrusted.
    fn reserve_update(self, input: RowInputPlan) -> Result<Self::Reserved, RowOwnedError<Self>>;
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
    /// C's flush of a `next` FlushRequired target, through the flusher the
    /// rollback holds, until that flusher is Ready (`RowRollback::flush`);
    /// the autocommit executor's flush-through (`autocommit`).
    fn flush(&mut self, target: &Self::Target) -> Result<(), RetentionError>;
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
    /// `try_flush` of a preflight's FlushRequired target.
    Flush,
    /// `continue_flush` until the flusher is Ready.
    ContinueFlush,
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
    /// C's `row_cleanup_next` in a live Row cleanup drain.
    CleanupNext,
    /// C's `retire_row` of one live Row cleanup group.
    Retire,
    /// C's Row Begin of an autocommit transaction (`autocommit`).
    Begin,
    /// The caller's issue of the statement's checked table (`autocommit`).
    Table,
    /// A rollback's flush of C's `next` FlushRequired target (`autocommit`).
    RollbackFlush,
    /// C's finish of a complete rollback (`autocommit`).
    RollbackFinish,
    /// C's live split of a row's full leaf (`split_row_live`).
    Split,
    /// C's `unlink_next` or `unlink_leaf_live` in a live Row cleanup drain
    /// (dev3-tree-smo.md#empty-leaf-unlink).
    Unlink,
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
    fn flush(&mut self, target: &K::Target) -> Result<(), RetentionError> {
        self.rollback.flush(target)
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
    'rows: for (values, slot) in rows.iter().zip(slots.iter_mut()) {
        // A row whose Reserve found its leaf full leaves its length in
        // `full`; C's live split is then asked and the same row tried again
        // (`split_leaf`, dev3-tree-smo.md#automatic-split).
        let (mut full, mut splits) = (None, 0);
        loop {
            match insert_row(call, &statement, table, loan, values, &mut full) {
                Ok(applied) => {
                    *slot = Some(applied);
                    applied_rows += 1;
                    break;
                }
                Err(failure) => {
                    match split_leaf(call, table, None, (&mut full, &mut splits), loan) {
                        Ok(true) => continue,
                        Ok(false) => failed = Some(failure),
                        Err(split) => failed = Some(split),
                    }
                    break 'rows;
                }
            }
        }
    }
    end_statement(call, statement, failed, applied_rows, rows.len())
}

/// The statement after its rows: finished when every row was applied,
/// otherwise handed back with the failed row's Failure as `After` says. It
/// is never inlined into `insert_statement`, so the frame below every row's
/// calls (the preflight's among them) holds none of its slots.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn end_statement<B: Boundary>(
    call: &mut B,
    statement: B::Statement,
    failed: Option<Failure>,
    applied_rows: usize,
    rows: usize,
) -> Result<usize, DriveError<B::Statement>> {
    let Some(failure) = failed else {
        return finish_statement(call, statement).map(|()| rows);
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
/// unless C answered Fatal/CallerMustResolve or refused the release. A
/// FlushRequired preflight is flushed and the same row tried again from
/// preflight (`flush`); a Ready preflight leaves no flush step in the row.
/// The steps after a Ready preflight run in `write_row`'s own frame, so the
/// frame live at the preflight and its flush holds none of their slots.
#[inline(never)]
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
    full: &mut Option<usize>,
) -> Result<B::Applied, Failure> {
    let preflight = loop {
        // C's answer is dropped at the end of this statement, so the flush
        // below may borrow the call again.
        let target = match call.preflight(statement, table, loan) {
            Ok(PreflightNext::Ready(preflight)) => break preflight,
            Ok(PreflightNext::Flush(target)) => target,
            // Outside `preflight_row`'s contract: nothing is held, so the
            // row ends as a refusal.
            Ok(PreflightNext::NoRow) => {
                return Err(Failure::new(
                    Stage::Preflight,
                    InsertError::IdentityMismatch.into(),
                ));
            }
            // A refusal, Busy included (a LOAD claim held by another owner,
            // a moved WAL prefix), ends the row: C returned what it took, and
            // only a FlushRequired is retried.
            Err(error) => return Err(Failure::new(Stage::Preflight, error.into())),
        };
        // Nothing is held and nothing took effect, so the same row is tried
        // again from preflight in this statement (MCA05-V31).
        flush(call, &target)?;
    };
    write_row(preflight, values, full)
}

/// The row after a Ready preflight: validation and the Reserve
/// (`reserve_row`), then A's Forward producer, prepare and apply on the
/// reservation (`forward_row`), with the finish or Cancel of a failed step.
/// It is never inlined into `insert_row`: its wrappers, lanes and results
/// are laid out only once C's preflight has returned Ready, so the frame on
/// the stack at the preflight and its flush (C's 6144-byte caller bound)
/// does not hold them (S2-e, MCA00-D15 frame-static reading). Each step runs
/// in its own frame, never inlined, so at each C call only the frames of the
/// steps in progress are on the stack: `write_row` keeps the reservation in
/// `reserve_row`'s result and nothing else, and no step's frame holds a
/// later or earlier step's slots.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn write_row<P: Preflight>(
    preflight: P,
    values: &[ValueRef<'_>],
    full: &mut Option<usize>,
) -> Result<P::Applied, Failure> {
    match reserve_row(preflight, values, full) {
        Ok(reserved) => forward_row(reserved),
        Err(failure) => Err(failure),
    }
}

/// The row's input checked over C's validation lane, then its one Reserve.
/// A refusal finishes the preflight. A value check refused Capacity
/// (`refuse_validation`) or a Reserve refused `Unchanged(Prepare,
/// Capacity)`, whose preflight finished, leaves the row's encoded length in
/// `full`, for C's live split (`split_leaf`).
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn reserve_row<P: Preflight>(
    mut preflight: P,
    values: &[ValueRef<'_>],
    full: &mut Option<usize>,
) -> Result<P::Reserved, Failure> {
    let input = match validate_row(&mut preflight, values) {
        Ok(input) => input,
        Err(failure) => return Err(refuse_validation(preflight, failure, values, full)),
    };
    let length = input.encoded_len();
    match preflight.reserve(input) {
        Ok(reserved) => Ok(reserved),
        Err(RowOwnedError::Before { error, owned }) => {
            let capacity = error.row_capacity();
            let failure = finish_preflight(owned, Failure::new(Stage::Reserve, error.into()));
            note_full(&failure, capacity.then_some(length), full);
            Err(failure)
        }
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::Reserve, error)),
    }
}

/// A validation refusal finishes the preflight. A's value check refused
/// Capacity (the routed leaf's free bytes, read by the preflight, cannot
/// take the encoded row) notes the row's canonical length in `full`, as a
/// Reserve's Capacity does (`note_full`). Never inlined: its slots are laid
/// out only on a refusal.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn refuse_validation<P: Preflight>(
    preflight: P,
    failure: Failure,
    values: &[ValueRef<'_>],
    full: &mut Option<usize>,
) -> Failure {
    let capacity = matches!(failure.error, DriverError::Insert(InsertError::Capacity));
    let failure = finish_preflight(preflight, failure);
    note_full(
        &failure,
        capacity.then(|| canonical_values_len(values)),
        full,
    );
    failure
}

/// A row refused for its leaf's room (`length`) leaves the length in `full`
/// for C's live split (`split_leaf`) once its preflight finished with no
/// refused cleanup and nothing to resolve; any other refusal leaves `full`
/// empty and ends the row.
fn note_full(failure: &Failure, length: Option<usize>, full: &mut Option<usize>) {
    if length.is_some() && failure.cleanup.is_none() && !failure.must_resolve {
        *full = length;
    }
}

/// A's value check over C's validation lane.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn validate_row<P: Preflight>(
    preflight: &mut P,
    values: &[ValueRef<'_>],
) -> Result<RowInputPlan, Failure> {
    match preflight.validation_lane() {
        Ok(lane) => lane
            .validate(values)
            .map_err(|error| Failure::new(Stage::Validate, error.into())),
        Err(error) => Err(Failure::new(Stage::ValidationLane, error.into())),
    }
}

/// One DELETE statement of one target row (dev3-row-delete.md#api). The
/// statement and fence are checked first, with no C call; the RowId needs no
/// argument check (0 is not representable). Then the row (`delete_row`): on
/// success `slot` holds the deleted row's observation, the statement is
/// finished and the answer is 1; a target that is no row under the statement
/// snapshot finishes the statement with 0 and leaves `slot` as it was. A
/// failed row ends the statement as an INSERT's (`end_statement`), and since
/// a DELETE applies at most its one row, that is `Returned` (or
/// `MustResolve`), never `RollbackRequired`.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn delete_statement<B: Boundary>(
    call: &mut B,
    statement: B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    row: RowId,
    slot: &mut Option<B::Applied>,
) -> Result<usize, DriveError<B::Statement>> {
    if let Err(error) = statement.check() {
        let failure = Failure::new(Stage::Statement, error.into());
        return Err(DriveError::new(failure, After::Returned(statement)));
    }
    let (failed, deleted) = match delete_row(call, &statement, table, loan, row) {
        Ok(Some(applied)) => {
            *slot = Some(applied);
            (None, 1)
        }
        Ok(None) => (None, 0),
        Err(failure) => (Some(failure), 0),
    };
    end_statement(call, statement, failed, deleted, deleted)
}

/// The DELETE's one row, as `insert_row`'s: C's preflight of the target,
/// flushed and tried again on FlushRequired; None when C answered NoRow
/// (the table read is registered and nothing is held); otherwise the
/// delete Reserve and A's DeleteForward producer, prepare and apply on the
/// reservation (`erase_row`), in frames that are not laid out at the
/// preflight and its flush.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn delete_row<B: Boundary>(
    call: &mut B,
    statement: &B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    row: RowId,
) -> Result<Option<B::Applied>, Failure> {
    let preflight = loop {
        // C's answer is dropped at the end of this statement, so the flush
        // below may borrow the call again.
        let target = match call.preflight_delete(statement, table, row, loan) {
            Ok(PreflightNext::Ready(preflight)) => break preflight,
            Ok(PreflightNext::Flush(target)) => target,
            Ok(PreflightNext::NoRow) => return Ok(None),
            // A refusal (Busy, Unsupported, SerializationConflict among
            // them) ends the row: C returned what it took.
            Err(error) => return Err(Failure::new(Stage::Preflight, error.into())),
        };
        flush(call, &target)?;
    };
    erase_row(preflight).map(Some)
}

/// The DELETE after a Ready preflight: its Reserve (`reserve_delete_row`),
/// then A's producer, prepare and apply on the reservation (`forward_row`,
/// the INSERT's: C's lane hands A the DeleteForward). Never inlined, as
/// `write_row`.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn erase_row<P: Preflight>(preflight: P) -> Result<P::Applied, Failure> {
    match reserve_delete_row(preflight) {
        Ok(reserved) => forward_row(reserved),
        Err(failure) => Err(failure),
    }
}

/// The DELETE's one Reserve. A refusal finishes the preflight.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn reserve_delete_row<P: Preflight>(preflight: P) -> Result<P::Reserved, Failure> {
    match preflight.reserve_delete() {
        Ok(reserved) => Ok(reserved),
        Err(RowOwnedError::Before { error, owned }) => {
            let failure = Failure::new(Stage::Reserve, error.into());
            Err(finish_preflight(owned, failure))
        }
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::Reserve, error)),
    }
}

/// One UPDATE statement of one target row to the new `values`
/// (dev3-row-update.md#api), with `delete_statement`'s checks, answers and
/// end: on success `slot` holds the updated row's observation and the
/// answer is 1; a target that is no row under the statement snapshot
/// finishes the statement with 0. The new values are checked by C's
/// validation lane as an INSERT's (`InvalidInput` for values the table's
/// schema does not take) after a Ready preflight, so a refusal before it
/// (Busy, Unsupported, SerializationConflict) does not depend on them.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn update_statement<B: Boundary>(
    call: &mut B,
    statement: B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    (row, values): (RowId, &[ValueRef<'_>]),
    slot: &mut Option<B::Applied>,
) -> Result<usize, DriveError<B::Statement>> {
    if let Err(error) = statement.check() {
        let failure = Failure::new(Stage::Statement, error.into());
        return Err(DriveError::new(failure, After::Returned(statement)));
    }
    // As an INSERT's row: a full leaf asks C's live split and tries the
    // same row again (`split_leaf`).
    let (mut full, mut splits) = (None, 0);
    let (failed, updated) = loop {
        match update_row(call, &statement, table, loan, (row, values), &mut full) {
            Ok(Some(applied)) => {
                *slot = Some(applied);
                break (None, 1);
            }
            Ok(None) => break (None, 0),
            Err(failure) => {
                match split_leaf(call, table, Some(row), (&mut full, &mut splits), loan) {
                    Ok(true) => continue,
                    Ok(false) => break (Some(failure), 0),
                    Err(split) => break (Some(split), 0),
                }
            }
        }
    };
    end_statement(call, statement, failed, updated, updated)
}

/// The UPDATE's one row, as `delete_row`'s: C's preflight of the target,
/// flushed and tried again on FlushRequired; None when C answered NoRow;
/// otherwise the new value's validation and the update Reserve, then A's
/// UpdateForward producer, prepare and apply on the reservation
/// (`rewrite_row`), in frames that are not laid out at the preflight and
/// its flush.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn update_row<B: Boundary>(
    call: &mut B,
    statement: &B::Statement,
    table: &B::Table,
    loan: &mut Loan,
    (row, values): (RowId, &[ValueRef<'_>]),
    full: &mut Option<usize>,
) -> Result<Option<B::Applied>, Failure> {
    let preflight = loop {
        // C's answer is dropped at the end of this statement, so the flush
        // below may borrow the call again.
        let target = match call.preflight_update(statement, table, row, loan) {
            Ok(PreflightNext::Ready(preflight)) => break preflight,
            Ok(PreflightNext::Flush(target)) => target,
            Ok(PreflightNext::NoRow) => return Ok(None),
            // A refusal (Busy, Unsupported, SerializationConflict among
            // them) ends the row: C returned what it took.
            Err(error) => return Err(Failure::new(Stage::Preflight, error.into())),
        };
        flush(call, &target)?;
    };
    rewrite_row(preflight, values, full).map(Some)
}

/// The UPDATE after a Ready preflight: the new value's validation and the
/// update Reserve (`reserve_update_row`), then A's producer, prepare and
/// apply on the reservation (`forward_row`, the INSERT's: C's lane hands A
/// the UpdateForward). Never inlined, as `write_row`.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn rewrite_row<P: Preflight>(
    preflight: P,
    values: &[ValueRef<'_>],
    full: &mut Option<usize>,
) -> Result<P::Applied, Failure> {
    match reserve_update_row(preflight, values, full) {
        Ok(reserved) => forward_row(reserved),
        Err(failure) => Err(failure),
    }
}

/// The new value checked over C's validation lane, then the UPDATE's one
/// Reserve, as `reserve_row`. A refusal finishes the preflight; a full leaf
/// leaves the new value's length in `full`, as `reserve_row`'s.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn reserve_update_row<P: Preflight>(
    mut preflight: P,
    values: &[ValueRef<'_>],
    full: &mut Option<usize>,
) -> Result<P::Reserved, Failure> {
    let input = match validate_row(&mut preflight, values) {
        Ok(input) => input,
        Err(failure) => return Err(refuse_validation(preflight, failure, values, full)),
    };
    let length = input.encoded_len();
    match preflight.reserve_update(input) {
        Ok(reserved) => Ok(reserved),
        Err(RowOwnedError::Before { error, owned }) => {
            let capacity = error.row_capacity();
            let failure = finish_preflight(owned, Failure::new(Stage::Reserve, error.into()));
            note_full(&failure, capacity.then_some(length), full);
            Err(failure)
        }
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::Reserve, error)),
    }
}

/// The reservation's proposal from A's Forward producer, then its prepare
/// and apply (`prepare_row`). A failed producer cancels the reservation.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn forward_row<R: Reserved>(mut reserved: R) -> Result<R::Applied, Failure> {
    let proposal = match produce_row(&mut reserved) {
        Ok(proposal) => proposal,
        Err(failure) => return Err(cancel(reserved, failure)),
    };
    prepare_row(reserved, proposal)
}

/// A's Forward producer over C's producer lane.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn produce_row<R: Reserved>(reserved: &mut R) -> Result<RowProposal, Failure> {
    match reserved.producer_lane() {
        Ok(lane) => lane
            .forward()
            .map_err(|error| Failure::new(Stage::Produce, error.into())),
        Err(error) => Err(Failure::new(Stage::ProducerLane, error.into())),
    }
}

/// The proposal prepared on the reservation, then applied (`apply_row`). A
/// Before prepare cancels the reservation.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn prepare_row<R: Reserved>(reserved: R, proposal: RowProposal) -> Result<R::Applied, Failure> {
    match reserved.prepare(proposal) {
        Ok(prepared) => apply_row(prepared),
        Err(RowOwnedError::Before { error, owned }) => {
            Err(cancel(owned, Failure::new(Stage::Prepare, error.into())))
        }
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::Prepare, error)),
    }
}

/// The prepared row applied. A Before apply reopens the same reservation and
/// cancels it.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn apply_row<D: Prepared>(prepared: D) -> Result<D::Applied, Failure>
where
    D::Reserved: Reserved,
{
    match prepared.apply() {
        Ok(applied) => Ok(applied),
        Err(RowOwnedError::Before { error, owned }) => Err(cancel(
            owned.reopen(),
            Failure::new(Stage::Apply, error.into()),
        )),
        Err(RowOwnedError::Fatal(error)) => Err(Failure::fatal(Stage::Apply, error)),
    }
}

/// Flushes a preflight's FlushRequired target and continues the flush until
/// the flusher is Ready, since preflight refuses a flusher in any other phase
/// with Busy. Each round flushes one dirty victim, which no other foreground
/// writer dirties again under the exclusive MutationPort, so the preflight
/// retries are bounded by the dirty pages of that moment (S2-0 Row write
/// amendment, rejected alternative "separate residency load"). The flush is
/// completed to Ready here, as the contract requires, without a cap; a
/// continue that leaves the phase unchanged yields the thread before the
/// next one, so the thread the flush waits on can run. A refusal ends
/// the row with nothing held; the flusher stays in the phase C left it in,
/// and its next preflight answers Busy until the caller continues the flush.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn flush<B: Boundary>(call: &mut B, target: &B::Target) -> Result<(), Failure> {
    call.try_flush(target)
        .map_err(|error| Failure::new(Stage::Flush, error.into()))?;
    while !call.flush_ready() {
        let step = call
            .continue_flush()
            .map_err(|error| Failure::new(Stage::ContinueFlush, error.into()))?;
        if step == FlushStep::Waiting {
            std::thread::yield_now();
        }
    }
    Ok(())
}

/// After a row's failed attempt, C's live split
/// (dev3-tree-smo.md#automatic-split) when the attempt left the row's length
/// in `full` (its value check refused Capacity or its Reserve was refused
/// `Unchanged(Prepare, Capacity)`, and the preflight finished) and the row
/// has split fewer than `MAX_ROW_SPLITS`
/// times. `Ok(true)`: a split is installed and durable, and the same row is
/// tried again from preflight. `Ok(false)`: the attempt's failure ends the
/// row (no split was asked, C answered NotNeeded, or the row's splits are
/// used up). A FlushRequired victim is flushed as a preflight's (`flush`)
/// and the same split asked again, which is not counted; a refused split
/// (`Stage::Split`) or flush ends the row with its own Failure. Nothing of
/// the row is held here: its preflight is finished and nothing is reserved,
/// and a split applies no row, so the statement's applied rows stay as they
/// were.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn split_leaf<B: Boundary>(
    call: &mut B,
    table: &B::Table,
    row: Option<RowId>,
    (full, splits): (&mut Option<usize>, &mut usize),
    loan: &mut Loan,
) -> Result<bool, Failure> {
    let Some(length) = full.take() else {
        return Ok(false);
    };
    if *splits >= MAX_ROW_SPLITS {
        return Ok(false);
    }
    loop {
        // C's answer is dropped at the end of this statement, so the flush
        // below may borrow the call again.
        let target = match call.split(table, row, length, loan) {
            Ok(SplitNext::Split) => {
                *splits += 1;
                return Ok(true);
            }
            Ok(SplitNext::NotNeeded) => return Ok(false),
            Ok(SplitNext::Flush(target)) => target,
            Err(error) => return Err(Failure::new(Stage::Split, error.into())),
        };
        flush(call, &target)?;
    }
}

/// Explicit no-effect release before the first Reserve. After a
/// CallerMustResolve answer no further C call is made; the preflight is
/// dropped, which C documents as the same no-effect release.
#[inline(never)]
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
#[inline(never)]
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
#[inline(never)]
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
#[inline(never)]
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
#[inline(never)]
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
    /// holds the session's foreground Owner borrow. The live adapter exposes
    /// `flush_rollback_live` through its disjoint original flusher, and the
    /// rollback's own `Rollback::flush` is the same continuation (the
    /// autocommit executor's). The step passes the target out and the caller
    /// keeps both; nothing is dropped here.
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
/// what C returned, borrowing the caller's rollback. Never inlined, like the
/// two starts above: the census counts their own slots at their own sites
/// only, never under a caller's later C call.
#[inline(never)]
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
#[inline(never)]
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
    map_commit_result(call.commit(transaction, loan))
}

/// Mapping runs after the C call has returned. It shares exactly the same
/// Before/RollbackRequired/Fatal interpretation with the direct live adapter.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "the original capsule is returned inline"
)]
fn map_commit_result<T, C>(
    result: Result<C, TransactionFinishError<T>>,
) -> Result<C, DriveError<T>> {
    match result {
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

/// Most `Visibility(Busy)` steps one cleanup drain skips. A further held
/// step ends the drain, since C would select it again.
const CLEANUP_HELD: usize = 8;
/// Most groups one cleanup drain installs. The rest waits for the next
/// drain, so one drain does a bounded number of C calls also behind a
/// backlog. A transaction of n inserted rows leaves 5n + 1 groups after its
/// commit and 4n + 1 after its abort, so a commit of more than 12 rows takes
/// more than one drain: the caller drains again while `complete` is false.
const CLEANUP_GROUPS: usize = 64;
/// Most live unlink answers one complete cleanup drain asks for
/// (dev3-tree-smo.md#empty-leaf-unlink, 호출자): C's `Unlinked` and
/// `NotNeeded` count, a FlushRequired does not. C keeps at most eight notes.
const CLEANUP_UNLINKS: u8 = 8;

/// What one live Row cleanup drain did (`retire_finished`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Retired {
    /// Groups C installed.
    pub(crate) groups: usize,
    /// Steps C refused `Visibility(Busy)`, whose Job ordinals the drain
    /// skipped. Each is left for a later drain.
    pub(crate) held: usize,
    /// C selected no further step outside the held ones. False when the
    /// drain stopped at `CLEANUP_GROUPS` or at a held step past
    /// `CLEANUP_HELD`.
    pub(crate) complete: bool,
    /// Live unlinks C installed after a complete drain
    /// (dev3-tree-smo.md#empty-leaf-unlink), at most `CLEANUP_UNLINKS`.
    /// One byte, like `unlink_held`: both fit in the padding after
    /// `complete`, so the drain's carriers keep their size.
    pub(crate) unlinked: u8,
    /// 1 when C refused the drain's last unlink with a Busy-class cause,
    /// which ended its unlinks and kept the note for a later drain.
    pub(crate) unlink_held: u8,
}

/// A live Row cleanup drain that C refused other than with Busy: the
/// groups installed before, and the refusal. An unchanged refusal leaves
/// the session as it was and a later drain may go on; a Fatal one is C's
/// fence (`Failure::must_resolve`).
#[derive(Debug)]
pub(crate) struct RetireStop {
    pub(crate) retired: Retired,
    pub(crate) failure: Failure,
}

/// One group's end: held by Busy, or failed.
#[derive(Debug)]
enum GroupStop {
    Held,
    Failed(Failure),
}

/// Drains C's live Row cleanup (reclamation `#live-row-cleanup-caller`):
/// asks C for the next step outside the held ones and installs it, until C
/// selects none, up to `CLEANUP_GROUPS` groups. A residency victim is
/// flushed with the call's flusher and the same group tried again, bounded
/// as a preflight's flush is (`flush`). A `Visibility(Busy)` step (a Prune
/// beside an older snapshot) is skipped by its Job ordinal, so one held
/// actor does not hide another; the list stays valid for the whole drain
/// under C's rule (`row_cleanup_next`): the drain holds the call, so no Job
/// is allocated between its calls. Any other refusal ends the drain with
/// what it installed; an unchanged one that C keeps selecting first blocks
/// the later steps until its cause is gone. Once C selects none, the drain
/// asks C's live unlinks of the RowLeafs its groups emptied
/// (`unlink_notes`, dev3-tree-smo.md#empty-leaf-unlink).
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage and the error inline, without allocation"
)]
fn retire_finished<B: Boundary>(call: &mut B, loan: &mut Loan) -> Result<Retired, RetireStop> {
    let mut skip = [0; CLEANUP_HELD];
    let mut retired = Retired::default();
    while retired.groups < CLEANUP_GROUPS {
        let (ordinal, operation) = match call.cleanup_next(&skip[..retired.held]) {
            Ok(Some(step)) => step,
            Ok(None) => {
                retired.complete = true;
                return unlink_notes(call, loan, retired);
            }
            Err(error) => {
                return Err(RetireStop {
                    retired,
                    failure: Failure::new(Stage::CleanupNext, error.into()),
                });
            }
        };
        match retire_group(call, ordinal, operation, loan) {
            Ok(()) => retired.groups += 1,
            Err(GroupStop::Held) if retired.held < CLEANUP_HELD => {
                skip[retired.held] = ordinal;
                retired.held += 1;
            }
            Err(GroupStop::Held) => return Ok(retired),
            Err(GroupStop::Failed(failure)) => return Err(RetireStop { retired, failure }),
        }
    }
    Ok(retired)
}

/// After a complete drain, C's live unlinks of emptied RowLeafs
/// (dev3-tree-smo.md#empty-leaf-unlink, 호출자): C's next kept note and
/// its unlink, until C keeps none, up to `CLEANUP_UNLINKS` answers. A
/// residency victim is flushed (`flush`) and the same note asked again,
/// which is not counted. A Busy-class refusal keeps its note and ends the
/// unlinks (`unlink_held`); any other refusal ends the drain with
/// `Stage::Unlink` (C dropped an unchanged one's note) and what it did
/// before. Never inlined, and it returns the drain's result in the drain's
/// place: the drain's own frame keeps its size.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage and the error inline, without allocation"
)]
fn unlink_notes<B: Boundary>(
    call: &mut B,
    loan: &mut Loan,
    mut retired: Retired,
) -> Result<Retired, RetireStop> {
    let mut asked = 0;
    while asked < CLEANUP_UNLINKS {
        let (table, key) = match call.unlink_next() {
            Ok(Some(note)) => note,
            Ok(None) => return Ok(retired),
            Err(error) => {
                return Err(RetireStop {
                    retired,
                    failure: Failure::new(Stage::Unlink, error.into()),
                });
            }
        };
        match call.unlink(table, key, loan) {
            Ok(UnlinkNext::Unlinked) => {
                asked += 1;
                retired.unlinked += 1;
            }
            Ok(UnlinkNext::NotNeeded) => asked += 1,
            // The target stays in C's answer: no copy of it in this frame.
            Ok(UnlinkNext::Flush(ref target)) => {
                if let Err(failure) = flush(call, target) {
                    return Err(RetireStop { retired, failure });
                }
            }
            Ok(UnlinkNext::Held) => {
                retired.unlink_held = 1;
                return Ok(retired);
            }
            Err(error) => {
                return Err(RetireStop {
                    retired,
                    failure: Failure::new(Stage::Unlink, error.into()),
                });
            }
        }
    }
    Ok(retired)
}

/// One live Row cleanup group, with its residency victims flushed.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage and the error inline, without allocation"
)]
fn retire_group<B: Boundary>(
    call: &mut B,
    ordinal: usize,
    operation: Operation,
    loan: &mut Loan,
) -> Result<(), GroupStop> {
    loop {
        match call.retire(ordinal, operation, loan) {
            Ok(RetireNext::Retired) => return Ok(()),
            Ok(RetireNext::Flush(target)) => {
                if let Err(failure) = flush(call, &target) {
                    return Err(GroupStop::Failed(failure));
                }
            }
            Ok(RetireNext::Held) => return Err(GroupStop::Held),
            Err(error) => {
                return Err(GroupStop::Failed(Failure::new(Stage::Retire, error.into())));
            }
        }
    }
}

/// Nonrecursive size_of inventory, in the repository's source-bound sense, of
/// the driver's carriers at the deepest point of the INSERT path (A's
/// Forward producer under `insert_statement` → `insert_row` → `write_row` →
/// `forward_row` → `produce_row` → the forwarding `forward`) and of the CLR
/// path (A's CLR producer under the caller's held rollback → `rollback_step`
/// → `compensate` → the forwarding `clr`).
/// Wrapper results passed by value count twice (the driver's slot and the
/// forwarding frame's return), the lane moved into the forwarding frame is
/// counted there, and the standard pointer/result/scalar slack is added once
/// per path. It is not a machine-stack measurement: unoptimized frames keep a
/// slot for every temporary and are several times larger (r1 record). The
/// wrappers carry their final cuts, read gate and flush lease and the lane
/// its eighth part (S2-a), so this count reads the actual types. A's
/// producer frame and C's internals are not included. The caller bounds of
/// the RowWrite census are compared with the share at each call site
/// (`call_sites`, MCA00-D15), not with this path inventory. The flush of a
/// FlushRequired preflight is never on the stack with the producer, so it
/// does not reach these deepest points.
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
        + 2 * size_of::<usize>()
        + size_of::<Option<usize>>()
        + size_of::<usize>()
        + size_of::<Result<bool, Failure>>();
    // insert_row and the write_row steps (the values argument of write_row,
    // reserve_row and validate_row; the split state reference of insert_row,
    // write_row and reserve_row, and reserve_row's length and capacity flag;
    // the preflight and the reservation they take by value count once, where
    // they are: the reservation in reserve_row's result).
    let row = 2 * size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
        + 3 * size_of::<&[ValueRef<'static>]>()
        + 3 * size_of::<&mut Option<usize>>()
        + size_of::<usize>()
        + size_of::<bool>()
        + size_of::<P<B>>()
        + size_of::<Result<V<B>, RetentionError>>()
        + 2 * size_of::<Result<RowInputPlan, Failure>>()
        + 2 * size_of::<Result<R<B>, RowOwnedError<P<B>>>>()
        + size_of::<Result<R<B>, Failure>>()
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

/// What bounds A's share at a call site (MCA00-D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bound {
    /// The preflight call: `ROW_PREFLIGHT_CALLER_BYTES`.
    Preflight,
    /// Every other C call: `ROW_WRITE_CALLER_BYTES`.
    Write,
    /// An A stage (validate, Forward, CLR): the share plus A's own frames
    /// is that phase, compared with the charge.
    A,
    /// The live split call (dev3-tree-smo.md#automatic-split):
    /// `ROW_SPLIT_CALLER_BYTES`.
    Split,
}

/// A's share at one call site of the driver (S2-0 Row write amendment §4
/// M3, S2-0b decision 3), with the frame-static share there.
#[derive(Clone, Copy, Debug)]
pub(super) struct CallSite {
    pub(super) name: &'static str,
    pub(super) bound: Bound,
    pub(super) bytes: usize,
    /// The own slots of the driver's frames on the path that are laid out
    /// but hold no value at this call. `bytes` leaves them out (M3), but a
    /// frame is laid out on entry, so `bytes + dead` is the frame-static
    /// share there.
    pub(super) dead: usize,
}

impl CallSite {
    /// `frame` is the frame-static share, which holds the per-call share.
    const fn new(name: &'static str, bound: Bound, bytes: usize, frame: usize) -> Self {
        assert!(
            frame >= bytes,
            "a frame-static share below the per-call share"
        );
        Self {
            name,
            bound,
            bytes,
            dead: frame - bytes,
        }
    }
}

pub(super) const CALL_SITES: usize = 62;

/// A's share at each call site, in the size_of inventory sense of
/// `carrier_bytes` and with its conventions: a C call's result counts twice
/// (the driver's slot and the forwarding frame's return; three times for a
/// rollback `next`, which `StartedRollback` forwards once more), a value
/// moved into a call counts once where it is, and the standard slack is
/// added at every site. Unlike `carrier_bytes`, which adds every slot of the
/// path, a site counts only what holds a value at that call: the values a
/// frame still uses after the call returns, the result slot of each call in
/// progress (the statement's, the row's, a helper's), and the arguments of
/// the forwarding frame. A slot that is dead there (a moved-from wrapper, an
/// earlier or later result, the `Result<(), RetentionError>` of a check
/// already passed) is not counted (M3). C's own return carrier (for example
/// `Result<RowPreflightStep>`) belongs to C's census. The flush entries are
/// MaintenancePort calls; their share is held to the write bound as well.
/// The rollback path starts from the caller's frame holding the call, the
/// transaction, the pending statement and the loan, then the rollback. As in
/// `carrier_bytes`, what the driver's own caller keeps by value (the
/// transaction capsule, a pending handle) is outside the driver's frames
/// and not counted here; `caller_bytes` is that term for `insert_live`'s
/// caller.
///
/// The frame-static share (`CallSite::dead`, A-W1 review A2; S2-e) adds the
/// slots that are dead at the call. A frame is laid out on entry, so every
/// driver frame on the path holds all its own slots there: as C's census
/// does, a function's own slots count for its whole body, and only callee
/// frames are max-combined, here by counting just the frames on the path to
/// the call. A frame's own slots are its reference and slice arguments,
/// each `let` binding, and its slot for each call's result (C's, through the
/// forwarding frame, or another driver frame's, `map_err` and `map`
/// included) unless the call is its tail and writes its own result. A value
/// it takes by value counts once, in its caller, where it is, also when it
/// passes the value on by value (`write_row` passes the preflight to
/// `reserve_row`); a value a match arm moves out of a result stays in that
/// result's slot, also when the arm passes it on (`write_row` passes the
/// reservation in `reserve_row`'s result to `forward_row`); a value
/// built in place (a Failure, a handle, a `DriveError`) counts where it is
/// put: a binding, a call's argument, or the frame's result, which its
/// caller counts. The frame-static share at a call is the slack, those
/// frames' slots, and the forwarding frame's arguments and return.
///
/// It relies on which frames are separate: `insert_row`, `end_statement`,
/// `write_row` and its steps (`reserve_row`, `validate_row`, `forward_row`,
/// `produce_row`, `prepare_row`, `apply_row`), the DELETE's `delete_row`,
/// `erase_row` and `reserve_delete_row`, the UPDATE's `update_row`,
/// `rewrite_row` and `reserve_update_row`, `split_leaf`, `flush`,
/// `finish_preflight`,
/// `cancel`, `finish_statement`, `compensate`, the two rollback starts and
/// `rollback_step` are never inlined, so none
/// of their slots is laid out in their caller's frame: none of `write_row`'s
/// or `end_statement`'s is on the stack at the preflight and its flush, and
/// at each C call of a row only the frames of the steps in progress are. A
/// frame that may be inlined holds nothing of its own but its arguments and
/// result, which are counted where they are (`insert_live` and the other
/// forwards to the driver, the live.rs forwards to C, `StartedRollback`'s
/// `next`, `held`, the constructors), or holds only references and scalars
/// within the slack (`check_arguments`).
/// MCA00-D15 bounds the per-call share (the contract's reading) and the
/// frame-static share (a coordinator decision on D15's reading, stricter
/// than M3).
const fn call_sites<B: Boundary + 'static>() -> [CallSite; CALL_SITES] {
    use std::{
        iter::Zip,
        mem::size_of,
        slice::{Iter, IterMut},
    };
    type P<B> = <B as Boundary>::Preflight<'static, 'static>;
    type G<B> = <B as Boundary>::Target;
    type V<B> = <P<B> as Preflight>::Lane<'static>;
    type R<B> = <P<B> as Preflight>::Reserved;
    type L<B> = <R<B> as Reserved>::Lane<'static>;
    type D<B> = <R<B> as Reserved>::Prepared;
    type A<B> = <B as Boundary>::Applied;
    type S<B> = <B as Boundary>::Statement;
    type X<B> = <B as Boundary>::Transaction;
    type C<B> = <B as Boundary>::Committed;
    type J<B> = <B as Boundary>::Rollback<'static, 'static>;
    type K<B> = StartedRollback<'static, J<B>>;
    type F<B> = <K<B> as Rollback>::Target;
    type T<B> = <K<B> as Rollback>::Task<'static>;
    type M<B> = <T<B> as RollbackTask>::Lane<'static>;
    type Q<B> = <T<B> as RollbackTask>::Prepared;
    type Rows = &'static [&'static [ValueRef<'static>]];
    type Values = &'static [ValueRef<'static>];
    let r = size_of::<&()>();
    let slack = 32 * r + 16 * size_of::<Result<(), RetentionError>>() + 512;
    let failure = size_of::<Failure>();
    use CallSite as Site;

    // insert_statement: its arguments and its own result.
    let statement = slack
        + size_of::<&mut B>()
        + size_of::<&B::Table>()
        + size_of::<&mut Loan>()
        + size_of::<Rows>()
        + size_of::<&mut [Option<A<B>>]>()
        + size_of::<S<B>>()
        + size_of::<Result<usize, DriveError<S<B>>>>();
    // The row loop's split state (dev3-tree-smo.md#automatic-split): the
    // length a full leaf left and the row's split count, live through the
    // row's attempts and splits.
    let split_state = size_of::<Option<usize>>() + size_of::<usize>();
    // ... while insert_row runs: the loop state and the row's result slot,
    // and insert_row's arguments (the split state's reference among them).
    let rows_state = size_of::<Option<Failure>>()
        + size_of::<usize>()
        + size_of::<Zip<Iter<'static, Values>, IterMut<'static, Option<A<B>>>>>()
        + size_of::<&Values>()
        + size_of::<&mut Option<A<B>>>()
        + size_of::<Result<A<B>, Failure>>()
        + split_state;
    let row_arguments = 5 * r + size_of::<Values>();
    let row = statement + rows_state + row_arguments;
    // ... and, after a Ready preflight, write_row's values and split state
    // reference (the preflight it takes by value stays in insert_row's frame
    // and counts there).
    let writing = row + size_of::<Values>() + r;
    // ... while reserve_row runs: the preflight, its values, its split state
    // reference, the row's length and the capacity flag, and its result slot
    // in write_row; while validate_row runs, also its arguments and its
    // result slot in reserve_row.
    let reserving = writing
        + size_of::<P<B>>()
        + size_of::<Values>()
        + r
        + size_of::<usize>()
        + size_of::<bool>()
        + size_of::<Result<R<B>, Failure>>();
    let validating =
        reserving + r + size_of::<Values>() + size_of::<Result<RowInputPlan, Failure>>();
    // ... after a refused value check: refuse_validation's arguments (the
    // Failure, the values and the split state reference; the preflight it
    // takes is the one `reserving` counts) and its result slot in
    // reserve_row, and the capacity flag.
    let refusing = reserving + 2 * failure + size_of::<Values>() + r + size_of::<bool>();
    // ... after the Reserve: the reservation forward_row takes (in
    // reserve_row's result slot); while produce_row runs, also its argument
    // and its result slot in forward_row.
    let forwarding = writing + size_of::<R<B>>();
    let producing = forwarding + r + size_of::<Result<RowProposal, Failure>>();
    // `flush`: the target insert_row holds, flush's arguments and its result
    // slot in insert_row.
    let flushing = row + size_of::<G<B>>() + 2 * r + size_of::<Result<(), Failure>>();
    // `split_leaf` after a row's failed attempt: the row's result, which
    // holds the attempt's Failure, stays in the statement's slot (rows_state
    // or the UPDATE's), with split_leaf's result slot and its arguments
    // (the call, the table, the target, the split state's two references
    // and the loan) and the length it takes out of `full`; its flush as
    // `flushing`.
    let split_arguments = 5 * r + size_of::<Option<RowId>>() + size_of::<usize>();
    let splitting = statement + rows_state + size_of::<Result<bool, Failure>>() + split_arguments;
    let split_flushing = splitting + size_of::<G<B>>() + 2 * r + size_of::<Result<(), Failure>>();
    // C's split forward: its receiver, the table, the target, the length and
    // the loan.
    let split_forward = 3 * r + size_of::<Option<RowId>>() + size_of::<usize>();
    // The caller's held rollback path.
    let held = slack + 4 * r;
    let stepping =
        held + size_of::<K<B>>() + size_of::<Result<Stepped, RollbackStop<T<B>, F<B>>>>() + r;
    let compensating = stepping + size_of::<Result<(), RollbackStop<T<B>, F<B>>>>();

    // The frames' own slots for the frame-static share.
    // insert_statement: its arguments and result; the checks' results and
    // the Failure each refusal binds; the rows' loop state and the row's
    // result. end_statement's result is its own (a tail call).
    let statement_frame = statement - slack
        + size_of::<Result<(), RetentionError>>()
        + size_of::<Result<(), InsertError>>()
        + rows_state
        + size_of::<Result<bool, Failure>>()
        + 2 * failure;
    // end_statement: its call and its two counts (the statement and the
    // failed row's Failure it takes by value count in insert_statement);
    // finish_statement's result (moved into `map`); the Failure it binds out
    // of `failed` and the handle's `After`.
    let end_frame = r
        + 2 * size_of::<usize>()
        + size_of::<Result<(), DriveError<S<B>>>>()
        + failure
        + size_of::<After<S<B>>>();
    // insert_row: its arguments; C's preflight answer; the Ready preflight
    // and the flush target it takes out of the answer, which is dropped at
    // the end of its statement; flush's result. write_row's result is its
    // own (a tail call), and a refused preflight's Failure is built in it.
    let row_frame = row_arguments
        + size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
        + size_of::<P<B>>()
        + size_of::<G<B>>()
        + size_of::<Result<(), Failure>>();
    // flush: its arguments; try_flush's and continue_flush's results and
    // their `map_err` results; the step.
    let flush_frame = 2 * r
        + size_of::<Result<(), RetentionError>>()
        + size_of::<Result<(), Failure>>()
        + size_of::<Result<FlushStep, RetentionError>>()
        + size_of::<Result<FlushStep, Failure>>()
        + size_of::<FlushStep>();
    // write_row: its values, its split state reference and reserve_row's
    // result, which holds the reservation forward_row takes; forward_row's
    // result is its own (a tail call), and so is a refused row's Failure.
    let write_frame = size_of::<Values>() + r + size_of::<Result<R<B>, Failure>>();
    // reserve_row: its values and split state reference; validate_row's
    // result and the plan; the row's length; the Reserve's result; the
    // capacity flag; the refused Reserve's Failure, its finish's result and
    // refuse_validation's result.
    let reserve_frame = size_of::<Values>()
        + r
        + size_of::<Result<RowInputPlan, Failure>>()
        + size_of::<RowInputPlan>()
        + size_of::<usize>()
        + size_of::<Result<R<B>, RowOwnedError<P<B>>>>()
        + size_of::<bool>()
        + 3 * failure;
    // validate_row: its arguments; the results of validation_lane and of
    // validate (moved into `map_err`).
    let validate_frame = r
        + size_of::<Values>()
        + size_of::<Result<V<B>, RetentionError>>()
        + size_of::<Result<RowInputPlan, InsertError>>();
    // refuse_validation: its arguments (the preflight, the Failure, the
    // values and the split state reference), the capacity flag,
    // finish_preflight's result and the noted length. The preflight and
    // the Failure it takes are counted again here (`refusing` counts them
    // in the callers): a conservative reading, as a by-value argument may
    // be copied into the callee's frame.
    let refuse_frame = size_of::<P<B>>()
        + 2 * failure
        + size_of::<Values>()
        + r
        + size_of::<bool>()
        + size_of::<Option<usize>>();
    // forward_row: produce_row's result and the proposal; the Cancel's
    // result. prepare_row's result is its own (a tail call).
    let forward_frame =
        size_of::<Result<RowProposal, Failure>>() + size_of::<RowProposal>() + failure;
    // produce_row: its argument; the results of producer_lane and of Forward
    // (moved into `map_err`).
    let produce_frame = r
        + size_of::<Result<L<B>, RetentionError>>()
        + size_of::<Result<RowProposal, InsertError>>();
    // prepare_row: prepare's result, which holds the prepared row apply_row
    // takes (apply_row's result is its own); the Failure a refused prepare
    // passes to `cancel` and the Cancel's result.
    let prepare_frame = size_of::<Result<D<B>, RowOwnedError<R<B>>>>() + 2 * failure;
    // apply_row: apply's result; reopen's result, the Failure a refused
    // apply passes to `cancel` and the Cancel's result.
    let apply_frame =
        size_of::<Result<A<B>, RowOwnedError<D<B>>>>() + size_of::<R<B>>() + 2 * failure;
    // finish_preflight, cancel: C's answer (their arguments are counted in
    // their callers, where they are). finish_statement: its call and C's
    // answer.
    let finish_preflight_frame = size_of::<Result<(), RetentionError>>();
    let cancel_frame = size_of::<Result<(), RowOwnedError<R<B>>>>();
    let finish_statement_frame = r + size_of::<Result<(), StatementFinishError<S<B>>>>();
    // split_leaf: its arguments and the length; C's split answer and the
    // flush target it binds out of it; flush's result (a refused split's
    // Failure is built in its result, the caller's).
    let split_frame = split_arguments
        + size_of::<Result<SplitNext<G<B>>, RetentionError>>()
        + size_of::<G<B>>()
        + size_of::<Result<(), Failure>>();
    // The frames below each kind of insert call. A Cancel is called from
    // forward_row, prepare_row and apply_row; apply_row's path is the
    // deepest.
    let in_statement = slack + statement_frame;
    let in_end = in_statement + end_frame;
    let in_row = in_statement + row_frame;
    let in_flush = in_row + flush_frame;
    let in_split = in_statement + split_frame;
    let in_split_flush = in_split + flush_frame;
    let in_write = in_row + write_frame;
    let in_reserve = in_write + reserve_frame;
    let in_validate = in_reserve + validate_frame;
    let in_refuse = in_reserve + refuse_frame;
    let in_forward = in_write + forward_frame;
    let in_produce = in_forward + produce_frame;
    let in_prepare = in_forward + prepare_frame;
    let in_apply = in_prepare + apply_frame;
    // The DELETE statement (`delete_statement`, dev3-row-delete.md#resources):
    // its arguments (the target and its slot in place of the rows and their
    // slots) and its own result; while delete_row runs, the row's result
    // slot and delete_row's arguments.
    let delete_statement = slack
        + size_of::<&mut B>()
        + size_of::<&B::Table>()
        + size_of::<&mut Loan>()
        + size_of::<RowId>()
        + size_of::<&mut Option<A<B>>>()
        + size_of::<S<B>>()
        + size_of::<Result<usize, DriveError<S<B>>>>();
    let delete_row =
        delete_statement + size_of::<Result<Option<A<B>>, Failure>>() + 4 * r + size_of::<RowId>();
    // ... after a Ready preflight: erase_row's result slot in delete_row (it
    // is mapped, not a tail); while reserve_delete_row runs, the preflight
    // (in delete_row's frame) and reserve_delete_row's result slot.
    let erasing = delete_row + size_of::<Result<A<B>, Failure>>();
    let delete_reserving = erasing + size_of::<P<B>>() + size_of::<Result<R<B>, Failure>>();
    // ... after the Reserve, as `forwarding` and `producing`.
    let delete_forwarding = erasing + size_of::<R<B>>();
    let delete_producing = delete_forwarding + r + size_of::<Result<RowProposal, Failure>>();
    let delete_flushing = delete_row + size_of::<G<B>>() + 2 * r + size_of::<Result<(), Failure>>();
    // delete_statement's own slots: its arguments and result; the check's
    // result and the Failure its refusal binds; delete_row's result and the
    // (failure, count) it binds. end_statement's result is its own.
    let delete_statement_frame = delete_statement - slack
        + size_of::<Result<(), RetentionError>>()
        + failure
        + size_of::<Result<Option<A<B>>, Failure>>()
        + size_of::<Option<Failure>>()
        + size_of::<usize>();
    // delete_row: its arguments; C's preflight answer; the Ready preflight
    // and the flush target it takes out of the answer; flush's result;
    // erase_row's result before `map` (the mapped result is its own).
    let delete_row_frame = 4 * r
        + size_of::<RowId>()
        + size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
        + size_of::<P<B>>()
        + size_of::<G<B>>()
        + size_of::<Result<(), Failure>>()
        + size_of::<Result<A<B>, Failure>>();
    // erase_row: reserve_delete_row's result, which holds the reservation
    // forward_row takes (forward_row's result is its own).
    let erase_frame = size_of::<Result<R<B>, Failure>>();
    // reserve_delete_row: the Reserve's result; the refused Reserve's
    // Failure and finish_preflight's result.
    let reserve_delete_frame = size_of::<Result<R<B>, RowOwnedError<P<B>>>>() + 2 * failure;
    let in_delete = slack + delete_statement_frame;
    let in_delete_end = in_delete + end_frame;
    let in_delete_row = in_delete + delete_row_frame;
    let in_delete_flush = in_delete_row + flush_frame;
    let in_erase = in_delete_row + erase_frame;
    let in_delete_reserve = in_erase + reserve_delete_frame;
    let in_delete_forward = in_erase + forward_frame;
    let in_delete_produce = in_delete_forward + produce_frame;
    let in_delete_prepare = in_delete_forward + prepare_frame;
    let in_delete_apply = in_delete_prepare + apply_frame;
    // The UPDATE statement (`update_statement`, dev3-row-update.md
    // #resources): its arguments (the target, its new values and its slot)
    // and its own result; while update_row runs, the row's result slot and
    // update_row's arguments.
    let update_statement = slack
        + size_of::<&mut B>()
        + size_of::<&B::Table>()
        + size_of::<&mut Loan>()
        + size_of::<(RowId, Values)>()
        + size_of::<&mut Option<A<B>>>()
        + size_of::<S<B>>()
        + size_of::<Result<usize, DriveError<S<B>>>>();
    // update_statement holds the split state through the row's attempts
    // and splits, as insert_statement.
    let update_row = update_statement
        + split_state
        + size_of::<Result<Option<A<B>>, Failure>>()
        + 5 * r
        + size_of::<RowId>()
        + size_of::<Values>();
    // ... after a Ready preflight: rewrite_row's result slot in update_row
    // (it is mapped, not a tail), its values and split state reference;
    // while reserve_update_row runs, the preflight (in update_row's frame),
    // its values and split state reference, the row's length and the
    // capacity flag, and its result slot; while validate_row runs, also its
    // arguments and result.
    let rewriting = update_row + size_of::<Result<A<B>, Failure>>() + size_of::<Values>() + r;
    let update_reserving = rewriting
        + size_of::<P<B>>()
        + size_of::<Values>()
        + r
        + size_of::<usize>()
        + size_of::<bool>()
        + size_of::<Result<R<B>, Failure>>();
    let update_validating =
        update_reserving + r + size_of::<Values>() + size_of::<Result<RowInputPlan, Failure>>();
    let update_refusing =
        update_reserving + 2 * failure + size_of::<Values>() + r + size_of::<bool>();
    // ... after the Reserve, as `forwarding` and `producing`.
    let update_forwarding = rewriting + size_of::<R<B>>();
    let update_producing = update_forwarding + r + size_of::<Result<RowProposal, Failure>>();
    let update_flushing = update_row + size_of::<G<B>>() + 2 * r + size_of::<Result<(), Failure>>();
    // ... and its split: the row's result, which holds the failed attempt's
    // Failure, and the split state in update_statement's frame, with
    // split_leaf's as at the INSERT's.
    let update_splitting = update_statement
        + split_state
        + size_of::<Result<Option<A<B>>, Failure>>()
        + size_of::<Result<bool, Failure>>()
        + split_arguments;
    let update_split_flushing =
        update_splitting + size_of::<G<B>>() + 2 * r + size_of::<Result<(), Failure>>();
    // update_statement's own slots: as delete_statement's, with the new
    // values beside the target.
    let update_statement_frame = update_statement - slack
        + size_of::<Result<(), RetentionError>>()
        + failure
        + size_of::<Result<Option<A<B>>, Failure>>()
        + size_of::<Option<Failure>>()
        + size_of::<usize>()
        + split_state
        + size_of::<Result<bool, Failure>>();
    // update_row: its arguments; C's preflight answer; the Ready preflight
    // and the flush target; flush's result; rewrite_row's result before
    // `map`.
    let update_row_frame = 5 * r
        + size_of::<RowId>()
        + size_of::<Values>()
        + size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
        + size_of::<P<B>>()
        + size_of::<G<B>>()
        + size_of::<Result<(), Failure>>()
        + size_of::<Result<A<B>, Failure>>();
    // rewrite_row: as write_row. reserve_update_row: as reserve_row.
    let rewrite_frame = write_frame;
    let reserve_update_frame = reserve_frame;
    let in_update = slack + update_statement_frame;
    let in_update_end = in_update + end_frame;
    let in_update_row = in_update + update_row_frame;
    let in_update_flush = in_update_row + flush_frame;
    let in_update_split = in_update + split_frame;
    let in_update_split_flush = in_update_split + flush_frame;
    let in_rewrite = in_update_row + rewrite_frame;
    let in_update_reserve = in_rewrite + reserve_update_frame;
    let in_update_validate = in_update_reserve + validate_frame;
    let in_update_refuse = in_update_reserve + refuse_frame;
    let in_update_forward = in_rewrite + forward_frame;
    let in_update_produce = in_update_forward + produce_frame;
    let in_update_prepare = in_update_forward + prepare_frame;
    let in_update_apply = in_update_prepare + apply_frame;
    // rollback_step's slots beside its share: the task it binds out of C's
    // answer and compensate's result while `next` runs, C's answer while
    // compensate runs.
    let next_dead = size_of::<T<B>>() + size_of::<Result<(), RollbackStop<T<B>, F<B>>>>();
    let step_dead = size_of::<Result<RollbackNext<T<B>, F<B>>, RetentionError>>();
    // compensate: the results of producer_lane, the CLR, prepare, apply,
    // reopen and the three `held`; the proposal as `map_err` left it and the
    // proposal; the prepared task (the task it takes is counted in
    // rollback_step, where it is).
    let compensate_frame = size_of::<Result<M<B>, RetentionError>>()
        + size_of::<Result<RowProposal, InsertError>>()
        + size_of::<Result<RowProposal, (Stage, DriverError)>>()
        + size_of::<RowProposal>()
        + size_of::<Result<Q<B>, RowOwnedError<T<B>>>>()
        + size_of::<Q<B>>()
        + size_of::<Result<(), RowOwnedError<Q<B>>>>()
        + size_of::<T<B>>()
        + 3 * size_of::<RollbackStop<T<B>, F<B>>>();
    let in_compensate = compensating + size_of::<T<B>>() + step_dead + compensate_frame;
    // start_*_rollback: the pending's answer and what it binds out of it,
    // the `map_err` result of C's answer and the rollback it binds.
    let start_dead = size_of::<Result<(&S<B>, &mut Progress), Failure>>()
        + size_of::<(&S<B>, &mut Progress)>()
        + size_of::<Result<J<B>, Failure>>()
        + size_of::<J<B>>();
    // Shares that the frame-static reading of the same site reuses.
    let start = held
        + size_of::<Result<K<B>, Failure>>()
        + 6 * r
        + 4 * r
        + size_of::<Option<&S<B>>>()
        + size_of::<RollbackScope>()
        + 2 * size_of::<Result<J<B>, RetentionError>>();
    let next = stepping + 2 * r + 3 * size_of::<Result<RollbackNext<T<B>, F<B>>, RetentionError>>();
    let finish = held
        + 2 * size_of::<Result<(), RowOwnedError<K<B>>>>()
        + size_of::<J<B>>()
        + r
        + 2 * size_of::<Result<(), RowOwnedError<J<B>>>>();
    let finish_statement = slack
        + 2 * r
        + size_of::<Result<(), DriveError<S<B>>>>()
        + size_of::<S<B>>()
        + 2 * size_of::<Result<(), StatementFinishError<S<B>>>>();
    let commit = slack
        + 4 * r
        + size_of::<Result<C<B>, DriveError<X<B>>>>()
        + size_of::<X<B>>()
        + 2 * size_of::<Result<C<B>, TransactionFinishError<X<B>>>>();
    let finish_aborted = slack
        + 2 * r
        + size_of::<Result<(), DriveError<X<B>>>>()
        + size_of::<X<B>>()
        + 2 * size_of::<Result<(), TransactionFinishError<X<B>>>>();
    [
        Site::new(
            "insert.check",
            Bound::Write,
            statement + 2 * size_of::<Result<(), RetentionError>>() + r,
            in_statement + size_of::<Result<(), RetentionError>>() + r,
        ),
        Site::new(
            "insert.preflight",
            Bound::Preflight,
            row + 2 * size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>() + 4 * r,
            in_row + size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>() + 4 * r,
        ),
        Site::new(
            "insert.try_flush",
            Bound::Write,
            flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        Site::new(
            "insert.continue_flush",
            Bound::Write,
            flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
        Site::new(
            "insert.split",
            Bound::Split,
            splitting + 2 * size_of::<Result<SplitNext<G<B>>, RetentionError>>() + split_forward,
            in_split + size_of::<Result<SplitNext<G<B>>, RetentionError>>() + split_forward,
        ),
        Site::new(
            "insert.split_try_flush",
            Bound::Write,
            split_flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_split_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        Site::new(
            "insert.split_continue_flush",
            Bound::Write,
            split_flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_split_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
        Site::new(
            "insert.validation_lane",
            Bound::Write,
            validating + 2 * size_of::<Result<V<B>, RetentionError>>() + r,
            in_validate + size_of::<Result<V<B>, RetentionError>>() + r,
        ),
        Site::new(
            "insert.validate",
            Bound::A,
            validating
                + size_of::<V<B>>()
                + size_of::<Values>()
                + 2 * size_of::<Result<RowInputPlan, InsertError>>(),
            in_validate + size_of::<Values>() + size_of::<Result<RowInputPlan, InsertError>>(),
        ),
        Site::new(
            "insert.reserve",
            Bound::Write,
            reserving
                + size_of::<RowInputPlan>()
                + 2 * size_of::<Result<R<B>, RowOwnedError<P<B>>>>(),
            in_reserve + size_of::<Result<R<B>, RowOwnedError<P<B>>>>(),
        ),
        Site::new(
            "insert.finish_preflight",
            Bound::Write,
            reserving + 2 * failure + 2 * size_of::<Result<(), RetentionError>>(),
            in_reserve + finish_preflight_frame + size_of::<Result<(), RetentionError>>(),
        ),
        Site::new(
            "insert.validate_finish",
            Bound::Write,
            refusing + 2 * failure + 2 * size_of::<Result<(), RetentionError>>(),
            in_refuse + finish_preflight_frame + size_of::<Result<(), RetentionError>>(),
        ),
        Site::new(
            "insert.producer_lane",
            Bound::Write,
            producing + 2 * size_of::<Result<L<B>, RetentionError>>() + r,
            in_produce + size_of::<Result<L<B>, RetentionError>>() + r,
        ),
        Site::new(
            "insert.forward",
            Bound::A,
            producing + size_of::<L<B>>() + 2 * size_of::<Result<RowProposal, InsertError>>(),
            in_produce + size_of::<Result<RowProposal, InsertError>>(),
        ),
        Site::new(
            "insert.prepare",
            Bound::Write,
            forwarding
                + size_of::<RowProposal>()
                + 2 * size_of::<Result<D<B>, RowOwnedError<R<B>>>>(),
            in_prepare + size_of::<Result<D<B>, RowOwnedError<R<B>>>>(),
        ),
        Site::new(
            "insert.apply",
            Bound::Write,
            writing + size_of::<D<B>>() + 2 * size_of::<Result<A<B>, RowOwnedError<D<B>>>>(),
            in_apply + size_of::<Result<A<B>, RowOwnedError<D<B>>>>(),
        ),
        Site::new(
            "insert.reopen",
            Bound::Write,
            writing + size_of::<D<B>>() + size_of::<RetentionError>() + 2 * size_of::<R<B>>(),
            in_apply + size_of::<R<B>>(),
        ),
        Site::new(
            "insert.cancel",
            Bound::Write,
            writing
                + size_of::<R<B>>()
                + 2 * failure
                + 2 * size_of::<Result<(), RowOwnedError<R<B>>>>(),
            in_apply + cancel_frame + size_of::<Result<(), RowOwnedError<R<B>>>>(),
        ),
        Site::new(
            "insert.finish_statement",
            Bound::Write,
            slack
                + size_of::<&mut B>()
                + size_of::<usize>()
                + size_of::<Result<usize, DriveError<S<B>>>>()
                + size_of::<Result<(), DriveError<S<B>>>>()
                + size_of::<S<B>>()
                + 2 * size_of::<Result<(), StatementFinishError<S<B>>>>()
                + r,
            in_end
                + finish_statement_frame
                + size_of::<Result<(), StatementFinishError<S<B>>>>()
                + r,
        ),
        Site::new(
            "delete.check",
            Bound::Write,
            delete_statement + 2 * size_of::<Result<(), RetentionError>>() + r,
            in_delete + size_of::<Result<(), RetentionError>>() + r,
        ),
        Site::new(
            "delete.preflight",
            Bound::Preflight,
            delete_row
                + 2 * size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
                + 4 * r
                + size_of::<RowId>(),
            in_delete_row
                + size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
                + 4 * r
                + size_of::<RowId>(),
        ),
        Site::new(
            "delete.try_flush",
            Bound::Write,
            delete_flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_delete_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        Site::new(
            "delete.continue_flush",
            Bound::Write,
            delete_flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_delete_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
        Site::new(
            "delete.reserve",
            Bound::Write,
            delete_reserving + 2 * size_of::<Result<R<B>, RowOwnedError<P<B>>>>(),
            in_delete_reserve + size_of::<Result<R<B>, RowOwnedError<P<B>>>>(),
        ),
        Site::new(
            "delete.finish_preflight",
            Bound::Write,
            delete_reserving + 2 * failure + 2 * size_of::<Result<(), RetentionError>>(),
            in_delete_reserve + finish_preflight_frame + size_of::<Result<(), RetentionError>>(),
        ),
        Site::new(
            "delete.producer_lane",
            Bound::Write,
            delete_producing + 2 * size_of::<Result<L<B>, RetentionError>>() + r,
            in_delete_produce + size_of::<Result<L<B>, RetentionError>>() + r,
        ),
        Site::new(
            "delete.forward",
            Bound::A,
            delete_producing
                + size_of::<L<B>>()
                + 2 * size_of::<Result<RowProposal, InsertError>>(),
            in_delete_produce + size_of::<Result<RowProposal, InsertError>>(),
        ),
        Site::new(
            "delete.prepare",
            Bound::Write,
            delete_forwarding
                + size_of::<RowProposal>()
                + 2 * size_of::<Result<D<B>, RowOwnedError<R<B>>>>(),
            in_delete_prepare + size_of::<Result<D<B>, RowOwnedError<R<B>>>>(),
        ),
        Site::new(
            "delete.apply",
            Bound::Write,
            erasing + size_of::<D<B>>() + 2 * size_of::<Result<A<B>, RowOwnedError<D<B>>>>(),
            in_delete_apply + size_of::<Result<A<B>, RowOwnedError<D<B>>>>(),
        ),
        Site::new(
            "delete.reopen",
            Bound::Write,
            erasing + size_of::<D<B>>() + size_of::<RetentionError>() + 2 * size_of::<R<B>>(),
            in_delete_apply + size_of::<R<B>>(),
        ),
        Site::new(
            "delete.cancel",
            Bound::Write,
            erasing
                + size_of::<R<B>>()
                + 2 * failure
                + 2 * size_of::<Result<(), RowOwnedError<R<B>>>>(),
            in_delete_apply + cancel_frame + size_of::<Result<(), RowOwnedError<R<B>>>>(),
        ),
        // As `insert.finish_statement`: end_statement's share is the same,
        // below the DELETE statement's frame.
        Site::new(
            "delete.finish_statement",
            Bound::Write,
            slack
                + size_of::<&mut B>()
                + size_of::<usize>()
                + size_of::<Result<usize, DriveError<S<B>>>>()
                + size_of::<Result<(), DriveError<S<B>>>>()
                + size_of::<S<B>>()
                + 2 * size_of::<Result<(), StatementFinishError<S<B>>>>()
                + r,
            in_delete_end
                + finish_statement_frame
                + size_of::<Result<(), StatementFinishError<S<B>>>>()
                + r,
        ),
        Site::new(
            "update.check",
            Bound::Write,
            update_statement + 2 * size_of::<Result<(), RetentionError>>() + r,
            in_update + size_of::<Result<(), RetentionError>>() + r,
        ),
        Site::new(
            "update.preflight",
            Bound::Preflight,
            update_row
                + 2 * size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
                + 4 * r
                + size_of::<RowId>(),
            in_update_row
                + size_of::<Result<PreflightNext<P<B>, G<B>>, RetentionError>>()
                + 4 * r
                + size_of::<RowId>(),
        ),
        Site::new(
            "update.try_flush",
            Bound::Write,
            update_flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_update_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        Site::new(
            "update.continue_flush",
            Bound::Write,
            update_flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_update_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
        Site::new(
            "update.split",
            Bound::Split,
            update_splitting
                + 2 * size_of::<Result<SplitNext<G<B>>, RetentionError>>()
                + split_forward,
            in_update_split + size_of::<Result<SplitNext<G<B>>, RetentionError>>() + split_forward,
        ),
        Site::new(
            "update.split_try_flush",
            Bound::Write,
            update_split_flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_update_split_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        Site::new(
            "update.split_continue_flush",
            Bound::Write,
            update_split_flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_update_split_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
        Site::new(
            "update.validation_lane",
            Bound::Write,
            update_validating + 2 * size_of::<Result<V<B>, RetentionError>>() + r,
            in_update_validate + size_of::<Result<V<B>, RetentionError>>() + r,
        ),
        Site::new(
            "update.validate",
            Bound::A,
            update_validating
                + size_of::<V<B>>()
                + size_of::<Values>()
                + 2 * size_of::<Result<RowInputPlan, InsertError>>(),
            in_update_validate
                + size_of::<Values>()
                + size_of::<Result<RowInputPlan, InsertError>>(),
        ),
        Site::new(
            "update.reserve",
            Bound::Write,
            update_reserving
                + size_of::<RowInputPlan>()
                + 2 * size_of::<Result<R<B>, RowOwnedError<P<B>>>>(),
            in_update_reserve + size_of::<Result<R<B>, RowOwnedError<P<B>>>>(),
        ),
        Site::new(
            "update.finish_preflight",
            Bound::Write,
            update_reserving + 2 * failure + 2 * size_of::<Result<(), RetentionError>>(),
            in_update_reserve + finish_preflight_frame + size_of::<Result<(), RetentionError>>(),
        ),
        Site::new(
            "update.validate_finish",
            Bound::Write,
            update_refusing + 2 * failure + 2 * size_of::<Result<(), RetentionError>>(),
            in_update_refuse + finish_preflight_frame + size_of::<Result<(), RetentionError>>(),
        ),
        Site::new(
            "update.producer_lane",
            Bound::Write,
            update_producing + 2 * size_of::<Result<L<B>, RetentionError>>() + r,
            in_update_produce + size_of::<Result<L<B>, RetentionError>>() + r,
        ),
        Site::new(
            "update.forward",
            Bound::A,
            update_producing
                + size_of::<L<B>>()
                + 2 * size_of::<Result<RowProposal, InsertError>>(),
            in_update_produce + size_of::<Result<RowProposal, InsertError>>(),
        ),
        Site::new(
            "update.prepare",
            Bound::Write,
            update_forwarding
                + size_of::<RowProposal>()
                + 2 * size_of::<Result<D<B>, RowOwnedError<R<B>>>>(),
            in_update_prepare + size_of::<Result<D<B>, RowOwnedError<R<B>>>>(),
        ),
        Site::new(
            "update.apply",
            Bound::Write,
            rewriting + size_of::<D<B>>() + 2 * size_of::<Result<A<B>, RowOwnedError<D<B>>>>(),
            in_update_apply + size_of::<Result<A<B>, RowOwnedError<D<B>>>>(),
        ),
        Site::new(
            "update.reopen",
            Bound::Write,
            rewriting + size_of::<D<B>>() + size_of::<RetentionError>() + 2 * size_of::<R<B>>(),
            in_update_apply + size_of::<R<B>>(),
        ),
        Site::new(
            "update.cancel",
            Bound::Write,
            rewriting
                + size_of::<R<B>>()
                + 2 * failure
                + 2 * size_of::<Result<(), RowOwnedError<R<B>>>>(),
            in_update_apply + cancel_frame + size_of::<Result<(), RowOwnedError<R<B>>>>(),
        ),
        // As `delete.finish_statement`, below the UPDATE statement's frame.
        Site::new(
            "update.finish_statement",
            Bound::Write,
            slack
                + size_of::<&mut B>()
                + size_of::<usize>()
                + size_of::<Result<usize, DriveError<S<B>>>>()
                + size_of::<Result<(), DriveError<S<B>>>>()
                + size_of::<S<B>>()
                + 2 * size_of::<Result<(), StatementFinishError<S<B>>>>()
                + r,
            in_update_end
                + finish_statement_frame
                + size_of::<Result<(), StatementFinishError<S<B>>>>()
                + r,
        ),
        Site::new("rollback.start", Bound::Write, start, start + start_dead),
        Site::new("rollback.next", Bound::Write, next, next + next_dead),
        Site::new(
            "rollback.clr_lane",
            Bound::Write,
            compensating + size_of::<T<B>>() + 2 * size_of::<Result<M<B>, RetentionError>>() + r,
            in_compensate + size_of::<Result<M<B>, RetentionError>>() + r,
        ),
        Site::new(
            "rollback.clr",
            Bound::A,
            compensating
                + size_of::<T<B>>()
                + size_of::<M<B>>()
                + 2 * size_of::<Result<RowProposal, InsertError>>(),
            in_compensate + size_of::<Result<RowProposal, InsertError>>(),
        ),
        Site::new(
            "rollback.clr_prepare",
            Bound::Write,
            compensating
                + size_of::<T<B>>()
                + size_of::<RowProposal>()
                + 2 * size_of::<Result<Q<B>, RowOwnedError<T<B>>>>(),
            in_compensate + size_of::<Result<Q<B>, RowOwnedError<T<B>>>>(),
        ),
        Site::new(
            "rollback.clr_apply",
            Bound::Write,
            compensating + size_of::<Q<B>>() + 2 * size_of::<Result<(), RowOwnedError<Q<B>>>>(),
            in_compensate + size_of::<Result<(), RowOwnedError<Q<B>>>>(),
        ),
        Site::new(
            "rollback.clr_reopen",
            Bound::Write,
            compensating + size_of::<Q<B>>() + size_of::<DriverError>() + 2 * size_of::<T<B>>(),
            in_compensate + size_of::<T<B>>(),
        ),
        // StartedRollback's finish binds the rollback and the progress out of
        // itself and builds a refusal in its result; finish_rollback's call
        // is its tail. Nothing of theirs is dead at C's finish.
        Site::new("rollback.finish", Bound::Write, finish, finish),
        // finish_statement, commit and finish_aborted hold nothing but their
        // arguments and C's answer, whose arms build their results in place.
        Site::new(
            "rollback.finish_statement",
            Bound::Write,
            finish_statement,
            finish_statement,
        ),
        Site::new("transaction.commit", Bound::Write, commit, commit),
        Site::new(
            "transaction.finish_aborted",
            Bound::Write,
            finish_aborted,
            finish_aborted,
        ),
    ]
}

#[cfg(test)]
const CLEANUP_SITES: usize = 8;

/// A's share at each C call of the live Row cleanup drain (reclamation
/// `#live-row-cleanup-caller`) as (site, per-call share, frame-static
/// share), each compared with C's fixed `LIVE_ROW_CLEANUP` ceiling.
/// `caller` is the term of the drain's entry and of that entry's caller
/// (`live::cleanup_caller_source`). The conventions are `call_sites`': the
/// standard slack once, a C call's result twice (the driver's slot and the
/// forwarding frame's return), a forwarding frame's arguments, and for the
/// frame-static share every own slot of each driver frame on the path. C's
/// own return carriers (`Result<RowRetireStep>`, `Result<RowUnlinkStep>`)
/// belong to C's census. The unlinks (dev3-tree-smo.md#empty-leaf-unlink)
/// run in `unlink_notes`' frame below the drain's, with the drain's
/// arguments and its moved `Retired` as the call's packet; it returns in
/// the drain's place, which is the caller's.
#[cfg(test)]
const fn cleanup_sites<B: Boundary + 'static>(
    caller: usize,
) -> [(&'static str, usize, usize); CLEANUP_SITES] {
    use std::mem::size_of;
    type G<B> = <B as Boundary>::Target;
    let r = size_of::<&()>();
    let slack = 32 * r + 16 * size_of::<Result<(), RetentionError>>() + 512;
    let base = slack + caller;
    let step = size_of::<(usize, Operation)>();
    let next = size_of::<Result<Option<(usize, Operation)>, RetentionError>>();
    let retire = size_of::<Result<RetireNext<G<B>>, RetentionError>>();
    let group = size_of::<Result<(), GroupStop>>();
    // retire_finished: its arguments, the held ordinals and the count, live
    // through the drain; also C's next answer, the step it binds and
    // retire_group's result (a Failure moved out of it stays in its slot; a
    // refused next's Failure is built in the drain's result, the caller's).
    let drain_live = 2 * r + size_of::<[usize; CLEANUP_HELD]>() + size_of::<Retired>();
    let drain_frame = drain_live + next + step + group;
    // retire_group: its arguments; C's answer, the flush target it binds and
    // flush's result (a refused group's Failure is built in its result).
    let group_live = 2 * r + size_of::<usize>() + size_of::<Operation>();
    let group_frame = group_live + retire + size_of::<G<B>>() + size_of::<Result<(), Failure>>();
    // flush: as in `call_sites`.
    let flush_frame = 2 * r
        + size_of::<Result<(), RetentionError>>()
        + size_of::<Result<(), Failure>>()
        + size_of::<Result<FlushStep, RetentionError>>()
        + size_of::<Result<FlushStep, Failure>>()
        + size_of::<FlushStep>();
    // The forwarding frames' arguments: `cleanup_next(&self, skip)`;
    // `retire(&mut self, ordinal, operation, loan)`.
    let next_arguments = r + size_of::<&[usize]>();
    let retire_arguments = 2 * r + size_of::<usize>() + size_of::<Operation>();
    // At a flush: the group's target and flush's arguments and result slot.
    let flushing = base
        + drain_live
        + step
        + group
        + group_live
        + size_of::<G<B>>()
        + 2 * r
        + size_of::<Result<(), Failure>>();
    let in_flush = base + drain_frame + group_frame + flush_frame;
    // unlink_notes: the call's packet (its arguments, the moved Retired),
    // its count, C's next answer, the note it binds, C's unlink answer, a
    // reference to the flush target in it and flush's result (a Failure is
    // built in the drain's result).
    let unlink_call = 2 * r + size_of::<Retired>();
    let unlink_live = 2 * r + size_of::<Retired>() + size_of::<u8>();
    let unlink_next = size_of::<Result<Option<(ObjectHandle, RowId)>, RetentionError>>();
    let note = size_of::<(ObjectHandle, RowId)>();
    let unlink = size_of::<Result<UnlinkNext<G<B>>, RetentionError>>();
    let unlink_frame =
        unlink_live + unlink_next + note + unlink + r + size_of::<Result<(), Failure>>();
    // `unlink_next(&mut self)`; `unlink(&mut self, table, key, loan)`.
    let unlink_arguments = 2 * r + note;
    let unlinking = base + drain_live + unlink_call + unlink_live;
    // At a flush: C's answer holding the target, the reference, and flush's
    // arguments and result slot.
    let unlink_flushing = unlinking + unlink + r + 2 * r + size_of::<Result<(), Failure>>();
    let in_unlink = base + drain_frame + unlink_call + unlink_frame;
    let in_unlink_flush = in_unlink + flush_frame;
    [
        (
            "cleanup.next",
            base + drain_live + next_arguments + 2 * next,
            base + drain_frame + next_arguments + next,
        ),
        (
            "cleanup.retire",
            base + drain_live + step + group + group_live + retire_arguments + 2 * retire,
            base + drain_frame + group_frame + retire_arguments + retire,
        ),
        (
            "cleanup.try_flush",
            flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        (
            "cleanup.continue_flush",
            flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
        (
            "cleanup.unlink_next",
            unlinking + r + 2 * unlink_next,
            in_unlink + r + unlink_next,
        ),
        (
            "cleanup.unlink",
            unlinking + note + unlink_arguments + 2 * unlink,
            in_unlink + unlink_arguments + unlink,
        ),
        (
            "cleanup.unlink_try_flush",
            unlink_flushing + 2 * size_of::<Result<(), RetentionError>>() + 2 * r,
            in_unlink_flush + size_of::<Result<(), RetentionError>>() + 2 * r,
        ),
        (
            "cleanup.unlink_continue_flush",
            unlink_flushing + 2 * size_of::<Result<FlushStep, RetentionError>>() + r,
            in_unlink_flush + size_of::<Result<FlushStep, RetentionError>>() + r,
        ),
    ]
}

/// What the driver's caller holds below the driver's frames at every
/// `insert.*` call site of one statement of `rows` rows of `values` values
/// each (A-W1 review A3: the unit that supplies the caller counts its frame;
/// S2-e supplies `insert_live`'s callers). In `call_sites`' inventory sense
/// and conventions: the statement's result, which `insert_statement`'s share
/// counts once, counts twice more, as the forwarding frame's return and the
/// caller's slot; the forwarding frame's arguments (the statement view moved
/// through it is counted in `insert_statement`'s); and what the caller keeps
/// by value across the call: the transaction capsule the view borrows, the
/// CheckedTable, the call itself (its references to the ports and Owners),
/// the Owners the call refers to (`owners`, their bytes: the Boundary does
/// not name their type; the live call's statement Owner and dedicated
/// flusher Owner, S2-e review A3) and, per row, its values, the row's slice
/// and the row's slot. The ports the call refers to and the loan are not on
/// this stack: the split ports are in C's shared census and the loan is the
/// private raw loan. No slack is added; each share already has it. Returns
/// (the term, the part each further row adds).
const fn caller_bytes<B: Boundary + 'static>(
    rows: usize,
    values: usize,
    owners: usize,
) -> (usize, usize) {
    use std::mem::size_of;
    type A<B> = <B as Boundary>::Applied;
    type S<B> = <B as Boundary>::Statement;
    type X<B> = <B as Boundary>::Transaction;
    type Rows = &'static [&'static [ValueRef<'static>]];
    let row = size_of::<&[ValueRef<'static>]>()
        + size_of::<Option<A<B>>>()
        + values * size_of::<ValueRef<'static>>();
    let held = 2 * size_of::<Result<usize, DriveError<S<B>>>>()
        + size_of::<&mut B>()
        + size_of::<&B::Table>()
        + size_of::<&mut Loan>()
        + size_of::<Rows>()
        + size_of::<&mut [Option<A<B>>]>()
        + size_of::<X<B>>()
        + size_of::<B::Table>()
        + size_of::<B>()
        + owners;
    (held + rows * row, row)
}
