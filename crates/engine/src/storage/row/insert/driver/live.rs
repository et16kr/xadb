//! A's product driver consumes C's sealed Row operation states and borrowed
//! lanes. Associated next states retain the original owner, statement, loan,
//! admission and Drop obligation; A neither constructs nor erases them.
//! Reserve creates the durable obligation; successful apply or cancel ends
//! it, while dropping an active reservation fences the session. Commit and
//! rollback use their existing live C adapters. Only FlushRequired retries
//! the row after a flush; other preflight refusals end the attempt.
use super::{
    Boundary, CALL_SITES, CallSite, DriveError, Failure, FlushStep, Loan, PendingRollback,
    Preflight, PreflightNext, Prepared, PreparedTask, ProducerLane, Reserved, RetireNext,
    RetireStop, Retired, Rollback, RollbackNext, RollbackStop, RollbackTask, Stage,
    StartedRollback, StatementView, Stepped, ValidationLane,
    autocommit::{
        Autocommit, abort_transaction, commit_answer, refused, statement_ended, statement_refused,
        table_not_issued,
    },
    call_sites, caller_bytes, carrier_bytes, finish_aborted, finish_rollback, finish_statement,
    insert_statement, map_commit_result, retire_finished, retry_task, rollback_step,
    start_full_rollback, start_statement_rollback,
};
use crate::storage::{
    disk::{
        CheckedTable, IsolationLevel, MaintenancePort, MutationPort, Owner, Plan4Target,
        PreflightRow, PreparedRollbackTask, RetentionError, RetentionFlushPhase, RollbackScope,
        RollbackStep, RowCommitObservation, RowInputPlan, RowMutationObservation, RowOwnedError,
        RowPreflightOperation, RowPreflightStep, RowPreparedOperation, RowProducerLane,
        RowProposal, RowReservedOperation, RowRetireStep, RowRollback, RowRollbackTask,
        RowStatementView, RowTransaction, RowValidationLane, StatementFinishError,
        TransactionFinishError,
    },
    row::insert::{InsertError, produce_clr, produce_insert, validate_insert},
    row_record::ValueRef,
    semantic_row::Operation,
};
use foundation::namespace::WriterNamespace;
use std::{convert::Infallible, marker::PhantomData};

impl StatementView for RowStatementView<'_, '_, '_, '_, '_> {
    fn check(&self) -> Result<(), RetentionError> {
        RowStatementView::q(self).map(|_| ())
    }
}

impl ValidationLane for RowValidationLane<'_> {
    fn validate(self, values: &[ValueRef<'_>]) -> Result<RowInputPlan, InsertError> {
        validate_insert(values, self)
    }
}

impl ProducerLane for RowProducerLane<'_> {
    fn forward(self) -> Result<RowProposal, InsertError> {
        produce_insert(self)
    }
    fn clr(self) -> Result<RowProposal, InsertError> {
        produce_clr(self)
    }
}

// C owns the associated state family. A no longer repeats its borrow graph at
// each transition; the same sealed wrappers implement the driver's private API.
impl<P: RowPreflightOperation> Preflight for P {
    type Applied = RowMutationObservation;
    type Lane<'l>
        = RowValidationLane<'l>
    where
        Self: 'l;
    type Reserved = P::Reserved;
    fn validation_lane(&mut self) -> Result<RowValidationLane<'_>, RetentionError> {
        RowPreflightOperation::validation_lane(self)
    }
    fn reserve(self, input: RowInputPlan) -> Result<Self::Reserved, RowOwnedError<Self>> {
        RowPreflightOperation::reserve(self, input)
    }
    fn finish(self) -> Result<(), RetentionError> {
        RowPreflightOperation::finish(self)
    }
}

impl<R: RowReservedOperation> Reserved for R {
    type Applied = RowMutationObservation;
    type Lane<'l>
        = RowProducerLane<'l>
    where
        Self: 'l;
    type Prepared = R::Prepared;
    fn producer_lane(&mut self) -> Result<RowProducerLane<'_>, RetentionError> {
        RowReservedOperation::producer_lane(self)
    }
    fn prepare(self, proposal: RowProposal) -> Result<Self::Prepared, RowOwnedError<Self>> {
        RowReservedOperation::prepare(self, proposal)
    }
    fn cancel(self) -> Result<(), RowOwnedError<Self>> {
        RowReservedOperation::cancel(self)
    }
}

impl<P: RowPreparedOperation> Prepared for P {
    type Reserved = P::Reserved;
    type Applied = RowMutationObservation;
    fn apply(self) -> Result<RowMutationObservation, RowOwnedError<Self>> {
        RowPreparedOperation::apply(self)
    }
    fn reopen(self) -> Self::Reserved {
        RowPreparedOperation::reopen(self)
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Rollback
    for RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Target = Plan4Target;
    type Task<'x>
        = RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
    where
        Self: 'x;
    /// FlushRequired's target is passed out unchanged with the rollback
    /// still held. The live caller uses `flush_rollback_live` with C's original
    /// disjoint flusher; an unfinished rollback still fences on Drop.
    fn next(&mut self) -> Result<RollbackNext<Self::Task<'_>, Plan4Target>, RetentionError> {
        Ok(match RowRollback::next(self)? {
            RollbackStep::Candidate(task) => RollbackNext::Candidate(task),
            RollbackStep::FlushRequired { target } => RollbackNext::Flush(target),
            RollbackStep::Complete => RollbackNext::Complete,
        })
    }
    /// C's disjoint continuation with the original flusher (the same call
    /// as `flush_rollback_live`).
    fn flush(&mut self, target: &Plan4Target) -> Result<(), RetentionError> {
        RowRollback::flush(self, target)
    }
    fn finish(self) -> Result<(), RowOwnedError<Self>> {
        RowRollback::finish(self).map(|_| ())
    }
}

impl<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> RollbackTask
    for RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Lane<'l>
        = RowProducerLane<'l>
    where
        Self: 'l;
    type Prepared = PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn producer_lane(&mut self) -> Result<RowProducerLane<'_>, RetentionError> {
        RowRollbackTask::producer_lane(self)
    }
    fn prepare(self, proposal: RowProposal) -> Result<Self::Prepared, RowOwnedError<Self>> {
        RowRollbackTask::prepare(self, proposal)
    }
}

impl<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> PreparedTask
    for PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Task = RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn apply(self) -> Result<(), RowOwnedError<Self>> {
        PreparedRollbackTask::apply(self).map(|_| ())
    }
    fn reopen(self) -> Self::Task {
        PreparedRollbackTask::reopen(self)
    }
}

/// The caller's original port, Owner, flusher and dedicated flusher Owner
/// (FLUSHER_SLOT) for one driver call; `capsule` names the transaction
/// lifetime the statement view borrows. Nothing is kept. A Row preflight
/// borrows the flusher as its flush lease until the row ends; after a
/// FlushRequired the driver flushes with `flush_owner` before retrying.
pub(in crate::storage) struct LiveCall<
    'x,
    't,
    'v,
    'b,
    'a,
    's,
    'c,
    'p,
    'd,
    'o,
    'm,
    'w,
    W: WriterNamespace,
> {
    port: &'x mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'x mut Owner<'c, 's>,
    flusher: &'x mut MaintenancePort<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    flush_owner: &'x mut Owner<'c, 's>,
    capsule: PhantomData<&'t RowTransaction<'v, 'a, 'c, 's>>,
}

impl<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    LiveCall<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn new(
        port: &'x mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        owner: &'x mut Owner<'c, 's>,
        flusher: &'x mut MaintenancePort<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
        flush_owner: &'x mut Owner<'c, 's>,
    ) -> Self {
        Self {
            port,
            owner,
            flusher,
            flush_owner,
            capsule: PhantomData,
        }
    }
}

impl<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Boundary
    for LiveCall<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Transaction = RowTransaction<'v, 'a, 'c, 's>;
    type Statement = RowStatementView<'t, 'v, 'a, 'c, 's>;
    type Table = CheckedTable<'c, 's>;
    type Applied = RowMutationObservation;
    type Committed = RowCommitObservation;
    type Target = Plan4Target;
    type Preflight<'r, 'i>
        = PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
    where
        Self: 'r;
    type Rollback<'r, 'i>
        = RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
    where
        Self: 'r;
    fn preflight<'r, 'i>(
        &'r mut self,
        statement: &'r Self::Statement,
        table: &'r CheckedTable<'c, 's>,
        loan: &'i mut Loan,
    ) -> Result<PreflightNext<Self::Preflight<'r, 'i>, Plan4Target>, RetentionError> {
        Ok(
            match self
                .port
                .preflight_row(self.owner, statement, table, loan, self.flusher)?
            {
                RowPreflightStep::Ready(preflight) => PreflightNext::Ready(preflight),
                RowPreflightStep::FlushRequired { target } => PreflightNext::Flush(target),
            },
        )
    }
    fn start_rollback<'r, 'i>(
        &'r mut self,
        transaction: &'r Self::Transaction,
        statement: Option<&'r Self::Statement>,
        scope: RollbackScope,
        loan: &'i mut Loan,
    ) -> Result<Self::Rollback<'r, 'i>, RetentionError> {
        self.port.start_row_rollback(
            self.owner,
            transaction,
            statement,
            scope,
            loan,
            self.flusher,
            self.flush_owner,
        )
    }
    fn finish_statement(
        &mut self,
        statement: Self::Statement,
    ) -> Result<(), StatementFinishError<Self::Statement>> {
        self.port.finish_statement(self.owner, statement)
    }
    fn commit(
        &mut self,
        transaction: Self::Transaction,
        loan: &mut Loan,
    ) -> Result<RowCommitObservation, TransactionFinishError<Self::Transaction>> {
        self.port.commit_row(self.owner, transaction, loan)
    }
    fn finish_aborted(
        &mut self,
        transaction: Self::Transaction,
    ) -> Result<(), TransactionFinishError<Self::Transaction>> {
        self.port.finish_aborted(self.owner, transaction)
    }
    /// The step is not needed: the driver reads the phase after each call.
    fn try_flush(&mut self, target: &Plan4Target) -> Result<(), RetentionError> {
        self.flusher.try_flush(self.flush_owner, target).map(|_| ())
    }
    /// C's step is the phase it leaves the flusher in; its type is not
    /// exported (it carries storage-private finalize data), so the phase
    /// before and after the call is compared.
    fn continue_flush(&mut self) -> Result<FlushStep, RetentionError> {
        let before = self.flusher.phase();
        self.flusher.continue_flush(self.flush_owner)?;
        Ok(if self.flusher.phase() == before {
            FlushStep::Waiting
        } else {
            FlushStep::Advanced
        })
    }
    fn flush_ready(&self) -> bool {
        self.flusher.phase() == RetentionFlushPhase::Ready
    }
    fn cleanup_next(&self, skip: &[usize]) -> Result<Option<(usize, Operation)>, RetentionError> {
        self.port.row_cleanup_next(skip)
    }
    /// C's `Visibility(Busy)` refusal is the one held answer; every other
    /// refusal is passed on as it is.
    fn retire(
        &mut self,
        ordinal: usize,
        operation: Operation,
        loan: &mut Loan,
    ) -> Result<RetireNext<Plan4Target>, RetentionError> {
        match self
            .port
            .retire_row(self.owner, ordinal, operation, loan, self.flusher)
        {
            Ok(RowRetireStep::Retired(_)) => Ok(RetireNext::Retired),
            Ok(RowRetireStep::FlushRequired(target)) => Ok(RetireNext::Flush(target)),
            Err(error) if error.visibility_busy() => Ok(RetireNext::Held),
            Err(error) => Err(error),
        }
    }
}

type Live<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> =
    LiveCall<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;

/// The rollback `start_*_rollback_live` hands to the caller, which steps it;
/// its successful finish releases the `PendingRollback` it was started from.
type LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> =
    StartedRollback<'r, RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>;

/// One INSERT statement over the original statement view: see
/// `insert_statement`. Every row's input is rejected before its Reserve.
#[allow(
    clippy::result_large_err,
    reason = "the unchanged or rollback-required statement view is returned inline"
)]
pub(in crate::storage) fn insert_live<'t, 'v, 'a, 'c, 's, W: WriterNamespace>(
    call: &mut Live<'_, 't, 'v, '_, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    statement: RowStatementView<'t, 'v, 'a, 'c, 's>,
    table: &CheckedTable<'c, 's>,
    private: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<RowMutationObservation>],
) -> Result<usize, DriveError<RowStatementView<'t, 'v, 'a, 'c, 's>>> {
    insert_statement(call, statement, table, private, rows, slots)
}

/// `finish_statement` of a statement whose rows all applied, or that
/// `PendingRollback::rolled_back` released; also the only retry of a
/// `FinishPending` statement.
#[allow(
    clippy::result_large_err,
    reason = "the finish-pending statement view is returned inline"
)]
pub(in crate::storage) fn finish_statement_live<'t, 'v, 'a, 'c, 's, W: WriterNamespace>(
    call: &mut Live<'_, 't, 'v, '_, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    statement: RowStatementView<'t, 'v, 'a, 'c, 's>,
) -> Result<(), DriveError<RowStatementView<'t, 'v, 'a, 'c, 's>>> {
    finish_statement(call, statement)
}

/// RollbackStart for a `RollbackRequired` statement, borrowing its
/// `PendingRollback`; see `rollback_step_live`.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
pub(in crate::storage) fn start_statement_rollback_live<
    'r,
    'i,
    'x,
    't,
    'v,
    'b,
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
    call: &'r mut Live<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    transaction: &'r RowTransaction<'v, 'a, 'c, 's>,
    pending: &'r mut PendingRollback<RowStatementView<'t, 'v, 'a, 'c, 's>>,
    private: &'i mut Loan,
) -> Result<LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, Failure> {
    start_statement_rollback(call, transaction, pending, private)
}

/// RollbackStart of the full rollback for a `RollbackRequired` transaction,
/// borrowing its `PendingRollback`.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
pub(in crate::storage) fn start_full_rollback_live<
    'r,
    'i,
    'x,
    't,
    'v,
    'b,
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
    call: &'r mut Live<'x, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    pending: &'r mut PendingRollback<RowTransaction<'v, 'a, 'c, 's>>,
    private: &'i mut Loan,
) -> Result<LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, Failure> {
    start_full_rollback(call, pending, private)
}

/// One step of the caller's held rollback; a stop hands back what C
/// returned (the same rollback, the same task, or the flush target).
#[allow(
    clippy::result_large_err,
    reason = "a stopped step returns C's same task, with its draft cut, inline"
)]
pub(in crate::storage) fn rollback_step_live<
    'x,
    'r,
    't,
    'v,
    'i,
    'b,
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
    rollback: &'x mut LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) -> Result<
    Stepped,
    RollbackStop<
        RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        Plan4Target,
    >,
> {
    rollback_step(rollback)
}

/// Retry of the same task a step handed back.
#[allow(
    clippy::result_large_err,
    reason = "a stopped retry returns C's same task, with its draft cut, inline"
)]
pub(in crate::storage) fn retry_task_live<
    'x,
    'r,
    't,
    'v,
    'i,
    'b,
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
    task: RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) -> Result<
    (),
    RollbackStop<
        RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        Infallible,
    >,
> {
    retry_task(task)
}

/// C's finish of a complete rollback; Before returns the same rollback.
#[allow(
    clippy::result_large_err,
    reason = "an unchanged refusal returns the same rollback inline"
)]
pub(in crate::storage) fn finish_rollback_live<
    'r,
    't,
    'v,
    'i,
    'b,
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
    rollback: LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) -> Result<(), RowOwnedError<LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>>
{
    finish_rollback(rollback)
}

/// Row Commit; a write conflict hands the transaction back, inside a
/// `PendingRollback`, for the full rollback.
#[allow(
    clippy::result_large_err,
    reason = "the unchanged or rollback-required transaction capsule is returned inline"
)]
pub(in crate::storage) fn commit_live<'v, 'a, 'c, 's, W: WriterNamespace>(
    call: &mut Live<'_, '_, 'v, '_, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    transaction: RowTransaction<'v, 'a, 'c, 's>,
    private: &mut Loan,
) -> Result<RowCommitObservation, DriveError<RowTransaction<'v, 'a, 'c, 's>>> {
    map_commit_result(call.port.commit_row(call.owner, transaction, private))
}

/// `finish_aborted` of the transaction `PendingRollback::rolled_back`
/// released; also its only retry. C's `RollbackRequired` here is kept for
/// C's fence (G5).
#[allow(
    clippy::result_large_err,
    reason = "the finish-pending transaction capsule is returned inline"
)]
pub(in crate::storage) fn finish_aborted_live<'v, 'a, 'c, 's, W: WriterNamespace>(
    call: &mut Live<'_, '_, 'v, '_, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    transaction: RowTransaction<'v, 'a, 'c, 's>,
) -> Result<(), DriveError<RowTransaction<'v, 'a, 'c, 's>>> {
    finish_aborted(call, transaction)
}

/// The live Row cleanup drain (`retire_finished`, reclamation
/// `#live-row-cleanup-caller`): every cleanup group of the finished
/// transactions C selects, up to the drain's bound, with residency victims
/// flushed and Busy steps left for a later drain. The statement path calls
/// it after `commit_live` or `finish_aborted_live` returned, and from a
/// frame that does not hold them: the census is per frame, so a caller
/// that calls both from one frame also holds the drain's result and
/// arguments during the commit, whose A frame is at its fixed ceiling
/// (`row_caller_budget::COMMIT`, 3,024 B). Such a caller calls the drain
/// from a separate never-inlined frame or redoes the commit census with
/// those slots. Its own term is the `caller` of `live_cleanup_sites`.
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage and the error inline, without allocation"
)]
pub(in crate::storage) fn retire_finished_live<W: WriterNamespace>(
    call: &mut Live<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
    private: &mut Loan,
) -> Result<Retired, RetireStop> {
    retire_finished(call, private)
}

/// The caller's issue of an autocommit statement's checked table
/// (`autocommit_insert_live`). A cannot name C's table issuance
/// (`catalog_statement_call` → `finish_table`, module `catalog_call`), so
/// the caller supplies it over the statement's view, with the statement
/// Owner and the call's flusher and dedicated flusher Owner for a
/// FlushRequired, which the source flushes itself.
pub(in crate::storage) trait LiveTables<'a, 'c, 's, W: WriterNamespace> {
    /// The statement's checked table, or None when no visible table answers
    /// it (an unchanged `InvalidInput` of the statement).
    fn issue(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        flusher: &mut MaintenancePort<'_, 's, 'c, '_, '_, '_, '_, W>,
        flush_owner: &mut Owner<'c, 's>,
    ) -> Result<Option<CheckedTable<'c, 's>>, RetentionError>;
}

/// One autocommit INSERT (`dev3-module-boundaries.md#autocommit-insert`):
/// C's Row Begin on `status`, one statement of `rows` into the table
/// `tables` issues for it, then C's Commit. A failed statement is rolled
/// back to its mark and finished, and a failed statement or a commit C
/// refuses is followed by the full rollback and `finish_aborted`
/// (`autocommit`). The outcome is written to `out`; `slots` names the
/// applied rows of a committed statement. The statement's steps reborrow
/// the call's ports and Owners (`statement_live`: the statement view
/// borrows the transaction), and the end's use the call itself (the
/// transaction's lifetime is the port's), which the caller keeps for its
/// cleanup. The cleanup of the ended
/// transaction is the caller's next step (`retire_finished_live`, again
/// while a drain is not complete). Its share at each C call is
/// `autocommit_sites`'.
#[inline(never)]
#[allow(
    clippy::too_many_arguments,
    reason = "the call, Begin's status and isolation, the table source, the loan, the rows and their slots, and the result slot"
)]
pub(in crate::storage) fn autocommit_insert_live<
    'b,
    'a,
    's,
    'c,
    W: WriterNamespace,
    T: LiveTables<'a, 'c, 's, W>,
>(
    call: &mut Live<'_, '_, 'b, 'b, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    status: &Plan4Target,
    isolation: IsolationLevel,
    tables: &mut T,
    private: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<RowMutationObservation>],
    out: &mut Autocommit<RowCommitObservation>,
) {
    match call
        .port
        .begin_row_transaction(call.owner, isolation, status, private, call.flusher)
    {
        Ok(transaction) => {
            match statement_live(call, &transaction, tables, private, rows, slots, out) {
                Some(rows) => commit_live_step(call, transaction, private, rows, out),
                None => abort_transaction(call, transaction, private, out),
            }
        }
        Err(error) => refused(out, Stage::Begin, error, Autocommit::NotBegun),
    }
}

/// The autocommit statement over a call that reborrows the executor's for
/// the statement view's borrow of the transaction: the run
/// (`start_statement_live`), then its end (`statement_ended`), which a
/// failed statement's rollback is under. The run's answer is this frame's
/// one slot; the run's own frame has returned before the end, so its slots
/// (the view's and the table's) are not laid out under the rollback.
#[inline(never)]
fn statement_live<'b, 'a, 's, 'c, W: WriterNamespace, T: LiveTables<'a, 'c, 's, W>>(
    call: &mut Live<'_, '_, 'b, 'b, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    transaction: &RowTransaction<'b, 'a, 'c, 's>,
    tables: &mut T,
    private: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<RowMutationObservation>],
    out: &mut Autocommit<RowCommitObservation>,
) -> Option<usize> {
    let mut call = LiveCall::new(
        &mut *call.port,
        &mut *call.owner,
        &mut *call.flusher,
        &mut *call.flush_owner,
    );
    let ran = start_statement_live(&mut call, transaction, tables, private, rows, slots);
    statement_ended(&mut call, transaction, ran, private, out)
}

/// The statement's run: C's statement start, the caller's table and the
/// driver's statement (`insert_statement`, this frame's tail, so it writes
/// the answer in the caller's slot). A refused start (`statement_refused`)
/// or a table not issued (`table_not_issued`) is the run's failure.
#[inline(never)]
#[allow(
    clippy::result_large_err,
    reason = "Failure keeps the stage, the error and any refused cleanup inline, without allocation"
)]
fn start_statement_live<'t, 'b, 'a, 's, 'c, W: WriterNamespace, T: LiveTables<'a, 'c, 's, W>>(
    call: &mut Live<'_, 't, 'b, 'b, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    transaction: &'t RowTransaction<'b, 'a, 'c, 's>,
    tables: &mut T,
    private: &mut Loan,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<RowMutationObservation>],
) -> Result<usize, DriveError<RowStatementView<'t, 'b, 'a, 'c, 's>>> {
    match call.port.begin_statement(call.owner, transaction) {
        Ok(statement) => match tables.issue(call.owner, &statement, call.flusher, call.flush_owner)
        {
            Ok(Some(table)) => insert_statement(call, statement, &table, private, rows, slots),
            issued => table_not_issued(statement, issued),
        },
        Err(error) => statement_refused(error),
    }
}

/// C's Commit of the autocommit transaction, called on the port itself
/// (`commit_row`), so that its answer has one slot, this frame's, and is
/// read in the next frame (`commit_answer`).
#[inline(never)]
fn commit_live_step<'b, 'a, 's, 'c, W: WriterNamespace>(
    call: &mut Live<'_, '_, 'b, 'b, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
    transaction: RowTransaction<'b, 'a, 'c, 's>,
    private: &mut Loan,
    rows: usize,
    out: &mut Autocommit<RowCommitObservation>,
) {
    let answer = call.port.commit_row(call.owner, transaction, private);
    commit_answer(call, answer, private, rows, out);
}

/// `carrier_bytes` for the actual C0 types (INSERT path, CLR path).
pub(super) const fn live_carrier_bytes<W: WriterNamespace + 'static>() -> (usize, usize) {
    carrier_bytes::<
        Live<
            'static,
            'static,
            'static,
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
        >,
    >()
}

/// `caller_bytes` for the actual C0 types and the `LiveCall`'s two Owners:
/// (the term for a statement of `rows` rows of `values` values, the part
/// each further row adds).
pub(super) const fn live_caller_bytes<W: WriterNamespace + 'static>(
    rows: usize,
    values: usize,
) -> (usize, usize) {
    caller_bytes::<
        Live<
            'static,
            'static,
            'static,
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
        >,
    >(
        rows,
        values,
        2 * std::mem::size_of::<Owner<'static, 'static>>(),
    )
}

/// `call_sites` for the actual C0 types.
pub(super) const fn live_call_sites<W: WriterNamespace + 'static>() -> [CallSite; CALL_SITES] {
    call_sites::<
        Live<
            'static,
            'static,
            'static,
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
        >,
    >()
}

/// S3 direct live call: the separate result mapper has returned only after
/// C returns. Inventory counts the unique moved capsule, both C result slots,
/// the outer caller's output, LiveCall, two Owners, and actual reference args.
/// Tail-forwarded outputs have one named owner, not duplicate placeholders.
#[cfg(test)]
pub(in crate::storage) fn commit_caller_source<W: WriterNamespace + 'static>() -> (usize, usize) {
    use std::mem::size_of;
    type T = RowTransaction<'static, 'static, 'static, 'static>;
    let slack = 32 * size_of::<&()>() + 16 * size_of::<Result<(), RetentionError>>() + 512;
    let held = slack
        + size_of::<
            LiveCall<
                'static,
                'static,
                'static,
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
            >,
        >()
        + 2 * size_of::<Owner<'static, 'static>>()
        + 2 * size_of::<&()>()
        + size_of::<T>()
        + size_of::<Result<RowCommitObservation, DriveError<T>>>()
        + 2 * size_of::<Result<RowCommitObservation, TransactionFinishError<T>>>();
    (held, held)
}

/// The term of the live Row cleanup drain's entry and of its caller
/// (reclamation `#live-row-cleanup-caller`), in `commit_caller_source`'s
/// sense: the caller's LiveCall and two Owners, the loan reference, the
/// caller's result slot and `retire_finished_live`'s forwarding return,
/// and that frame's two arguments. The slack is `cleanup_sites`'.
#[cfg(test)]
pub(in crate::storage) fn cleanup_caller_source<W: WriterNamespace + 'static>() -> usize {
    use std::mem::size_of;
    let r = size_of::<&()>();
    size_of::<
        LiveCall<
            'static,
            'static,
            'static,
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
        >,
    >() + 2 * size_of::<Owner<'static, 'static>>()
        + r
        + 2 * size_of::<Result<Retired, RetireStop>>()
        + 2 * r
}

/// `cleanup_sites` for the actual C0 types and `retire_finished_live`'s
/// caller term.
#[cfg(test)]
pub(in crate::storage) fn live_cleanup_sites<W: WriterNamespace + 'static>()
-> [(&'static str, usize, usize); super::CLEANUP_SITES] {
    super::cleanup_sites::<
        Live<
            'static,
            'static,
            'static,
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
        >,
    >(cleanup_caller_source::<W>())
}

/// The statement the S2-e callers of `insert_live` pass at most
/// (`catalog_memory::row_live`): two rows of T's one value. The term covers
/// these test callers only: a product caller (S3/S4, SQL) must rerun this
/// census with its own term. At S2-e the preflight's frame-static margin
/// with this term (813 B) is worth about eight more rows of one value (96 B
/// each), and its per-call margin (1584 B) about sixteen. MCA00-D15 and the
/// S9 gate-wait census below use it.
#[cfg(test)]
pub(in crate::storage) const CALLER_ROWS: usize = 2;
#[cfg(test)]
pub(in crate::storage) const CALLER_VALUES: usize = 1;

/// S9 (WRITER-PROGRESS-001, boundaries `#row-m4-s9`): A's frame-static share
/// at `insert.finish_statement`, where C's statement end waits at the gate,
/// with the term of `insert_live`'s caller for the S2-e statement
/// (`CALLER_ROWS` rows of `CALLER_VALUES` values, MCA00-D15's reading).
#[cfg(test)]
pub(in crate::storage) fn statement_end_caller_source<W: WriterNamespace + 'static>() -> usize {
    let site = live_call_sites::<W>()
        .into_iter()
        .find(|site| site.name == "insert.finish_statement")
        .expect("insert.finish_statement site");
    site.bytes + site.dead + live_caller_bytes::<W>(CALLER_ROWS, CALLER_VALUES).0
}

/// Flush continuation using the original disjoint flusher held by C.
pub(in crate::storage) fn flush_rollback_live<
    'r,
    't,
    'v,
    'i,
    'b,
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
    rollback: &mut LiveRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    target: &Plan4Target,
) -> Result<(), RetentionError> {
    rollback.rollback.flush(target)
}

/// Actual S4 caller holds the PendingRollback/view, original capsule, LiveCall
/// and both Owners below the driver. Existing call-sites retain their own slack.
#[cfg(test)]
pub(in crate::storage) fn rollback_caller_source<W: WriterNamespace + 'static>()
-> (usize, Vec<(&'static str, usize, usize)>, usize) {
    use std::mem::size_of;
    type T = RowTransaction<'static, 'static, 'static, 'static>;
    type V = RowStatementView<'static, 'static, 'static, 'static, 'static>;
    let caller = size_of::<T>()
        + size_of::<PendingRollback<V>>().max(size_of::<PendingRollback<T>>())
        + size_of::<
            Live<
                'static,
                'static,
                'static,
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
            >,
        >()
        + 2 * size_of::<Owner<'static, 'static>>()
        + 4 * size_of::<&()>();
    let sites = live_call_sites::<W>()
        .into_iter()
        .filter(|s| s.name.starts_with("rollback.") || s.name == "transaction.finish_aborted")
        .map(|s| (s.name, s.bytes + caller, s.bytes + s.dead + caller))
        .collect();
    let (_, producer) = super::super::local_source_carrier_bytes();
    (caller, sites, producer)
}

/// The references and slice headers the census lets the autocommit
/// executor's caller hold besides its call, Owners, result slot and status
/// target: a table source of references and the storage of the rows, their
/// values and their slots (three vector headers), whose contents are not on
/// this stack. A caller that holds more redoes `autocommit_sites` with its
/// own term.
#[cfg(test)]
pub(in crate::storage) const AUTOCOMMIT_CALLER_REFS: usize = 12;

/// The autocommit executor's share at each C call (`autocommit_sites`).
#[cfg(test)]
pub(in crate::storage) struct AutocommitCensus {
    /// What the executor's caller holds by value below it.
    pub(in crate::storage) caller: usize,
    /// The frame-static share at C's `commit_row`.
    pub(in crate::storage) commit: usize,
    /// The frame-static share at `insert.finish_statement`.
    pub(in crate::storage) statement_end: usize,
    /// A's term below the rollback at C's flush of a rollback target
    /// (`RowRollback::flush`): what C's flush step counts as its caller.
    pub(in crate::storage) rollback_caller: usize,
    /// (driver site, path, per-call share, frame-static share) at each
    /// `insert.*` site and at each rollback site on the statement rollback
    /// path and on the full rollback path.
    pub(in crate::storage) sites: Vec<(&'static str, &'static str, usize, usize)>,
    /// The term of the cleanup's caller (the executor's caller after the
    /// executor returned, calling `retire_finished_live` while a drain is
    /// not complete) and the cleanup sites with it.
    pub(in crate::storage) drain_caller: usize,
    pub(in crate::storage) cleanup: [(&'static str, usize, usize); super::CLEANUP_SITES],
    /// The shares at C's Begin, statement start and the caller's table
    /// issue, for the record: C's census of these entries has no A term.
    pub(in crate::storage) unbounded: [(&'static str, usize); 3],
}

/// The autocommit executor's share (`dev3-module-boundaries.md
/// #autocommit-insert`) at every C call under `autocommit_insert_live`, in
/// `call_sites`' size_of inventory sense and with its conventions: the
/// driver's share at each site (`live_call_sites`, slack included) plus the
/// frames of the executor's steps below the driver and the caller's term in
/// place of the term of the driver's test callers. A step's frame counts its
/// reference and slice arguments, its bindings and its slot for each call's
/// result; a value it takes by value counts once, where it is; a value a
/// match arm moves out of a result stays in that result's slot. Every step
/// is never inlined, so only the steps on the path to a call are counted,
/// each with all its own slots (frame-static). Slots the driver's share
/// already counts at a site are not counted again: the four references of
/// the rollback's holder (`held`), the started rollback where the site
/// counts it, the rollback step's result, and the result of the driver's
/// `finish_statement` and `finish_aborted` in their callers.
#[cfg(test)]
pub(in crate::storage) fn autocommit_sites<W: WriterNamespace + 'static>() -> AutocommitCensus {
    use std::mem::size_of;
    type L<W> = Live<
        'static,
        'static,
        'static,
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
    type T = RowTransaction<'static, 'static, 'static, 'static>;
    type V = RowStatementView<'static, 'static, 'static, 'static, 'static>;
    type J<W> = <L<W> as Boundary>::Rollback<'static, 'static>;
    type K<W> = StartedRollback<'static, J<W>>;
    type Obs = RowCommitObservation;
    type Sd = (Stage, super::DriverError);
    type Rows = &'static [&'static [ValueRef<'static>]];
    type Slots = &'static mut [Option<RowMutationObservation>];
    type Retire = Result<Retired, RetireStop>;
    let r = size_of::<&()>();
    let slack = 32 * r + 16 * size_of::<Result<(), RetentionError>>() + 512;
    let sd = size_of::<Result<(), Sd>>();
    let (rows, slots) = (size_of::<Rows>(), size_of::<Slots>());

    // The caller: the call, the Owners the call refers to, the result
    // slot, the status target and its references.
    let caller = size_of::<L<W>>()
        + 2 * size_of::<Owner<'static, 'static>>()
        + size_of::<Autocommit<Obs>>()
        + size_of::<Plan4Target>()
        + AUTOCOMMIT_CALLER_REFS * r;
    // autocommit_insert_live: its arguments (five references, the
    // isolation and the two slices), Begin's result, which holds the
    // transaction, and statement_live's result.
    let executor = 5 * r
        + size_of::<IsolationLevel>()
        + rows
        + slots
        + size_of::<Result<T, RetentionError>>()
        + size_of::<Option<usize>>();
    // statement_live: its arguments, the call it reborrows and the run's
    // answer, which holds a failed statement's After and Failure while the
    // statement is ended (start_statement_live's tail writes it).
    let statement =
        5 * r + rows + slots + size_of::<L<W>>() + size_of::<Result<usize, DriveError<V>>>();
    // start_statement_live: its arguments, the statement start's result,
    // which holds the view, and the issue's result, which holds the table
    // (or, not a table, is passed on by value). Its frame has returned
    // before the statement is ended.
    let start = 4 * r
        + rows
        + slots
        + size_of::<Result<V, RetentionError>>()
        + size_of::<Result<Option<CheckedTable<'static, 'static>>, RetentionError>>();
    // statement_ended: its arguments (the answer counts in statement_live).
    let ended = 4 * r;
    // end_failed_statement: its arguments (After and Failure count in the
    // run's answer).
    let end_failed = 4 * r;
    // finish_failed: its arguments and fence's argument (the driver's
    // finish_statement's result is the site's).
    let finish_failed = 2 * r + size_of::<Sd>();
    // roll_back_statement / abort_transaction beside their four references
    // (the site's `held`): the start's result, which holds the rollback,
    // roll's result and the end's binding; abort_transaction also binds the
    // pending transaction.
    let rolled = size_of::<Result<K<W>, Failure>>() + 2 * sd;
    let abort = size_of::<PendingRollback<T>>() + rolled;
    // roll: its reference, the flush target and the flush's result (the
    // step's result is the site's).
    let roll = r + size_of::<Plan4Target>() + size_of::<Result<(), RetentionError>>();
    // finish_rolled_statement / _transaction: their arguments, the
    // pending's answer, and fence's argument or the end's binding.
    let finish_rolled_statement =
        2 * r + size_of::<Result<V, PendingRollback<V>>>() + size_of::<Sd>();
    let finish_rolled_transaction = 2 * r + size_of::<Result<T, PendingRollback<T>>>() + sd;
    // commit_live_step: its arguments and C's answer; commit_answer: its
    // arguments (the answer counts in commit_live_step).
    let commit_step =
        3 * r + size_of::<usize>() + size_of::<Result<Obs, TransactionFinishError<T>>>();
    let commit_answer = 3 * r + size_of::<usize>();

    // Below the run's C calls (the statement start, the issue, every
    // insert site) and below the failed statement's end.
    let below_statement = caller + executor + statement + start;
    let below_ended = caller + executor + statement + ended;
    let below_statement_rollback = below_ended + end_failed;
    let below_full_rollback = caller + executor + commit_step + commit_answer;
    let k = size_of::<K<W>>();
    let started = size_of::<Result<K<W>, Failure>>();
    let mut sites = Vec::new();
    let driver = live_call_sites::<W>();
    let site = |name: &str| {
        *driver
            .iter()
            .find(|site| site.name == name)
            .expect("driver site")
    };
    for s in driver.iter().filter(|s| s.name.starts_with("insert.")) {
        let term = below_statement;
        sites.push((s.name, "statement", s.bytes + term, s.bytes + s.dead + term));
    }
    for (path, below, own) in [
        ("statement", below_statement_rollback, rolled),
        ("transaction", below_full_rollback, abort),
    ] {
        for s in driver.iter().filter(|s| s.name.starts_with("rollback.")) {
            let term = match s.name {
                // After the rollback; a Returned or FinishPending statement
                // (or one whose table was not issued) is finished with no
                // rollback frames.
                "rollback.finish_statement" if path == "statement" => {
                    (below + 4 * r + own + finish_rolled_statement + finish_failed)
                        .max(below_ended + end_failed + finish_failed)
                }
                "rollback.finish_statement" => continue,
                "rollback.start" => below + own - started,
                "rollback.finish" => below + own,
                _ => below + own - k + roll,
            };
            sites.push((s.name, path, s.bytes + term, s.bytes + s.dead + term));
        }
    }
    let finish_aborted = site("transaction.finish_aborted");
    let term = below_full_rollback + 4 * r + abort + finish_rolled_transaction;
    sites.push((
        finish_aborted.name,
        "transaction",
        finish_aborted.bytes + term,
        finish_aborted.bytes + finish_aborted.dead + term,
    ));
    // C's flush step counts the rollback, the step's result, the target and
    // the flush's results itself; A's term is what lies below the rollback.
    let rollback_caller = (below_statement_rollback + 4 * r + rolled - k + r)
        .max(below_full_rollback + 4 * r + abort - k + r);
    // C's commit_row: its three references (the transaction counts in
    // Begin's result).
    let commit = slack + caller + executor + commit_step + 3 * r;
    let end = site("insert.finish_statement");
    let statement_end = end.bytes + end.dead + below_statement;
    // The cleanup after the executor returned: the executor's caller with
    // its slot for `retire_finished_live`'s result, the forward's return
    // and arguments (as `cleanup_caller_source`), and the sum of its drains.
    let drain_caller = caller + 2 * size_of::<Retire>() + 2 * r + size_of::<Retired>();
    let cleanup = super::cleanup_sites::<L<W>>(drain_caller);
    // Begin: C's five arguments; the statement start: its two; the issue:
    // the source's four (the source's own frame is the caller's).
    let unbounded = [
        ("begin_row_transaction", slack + caller + executor + 5 * r),
        ("begin_statement", slack + below_statement + 2 * r),
        ("table_issue", slack + below_statement + 4 * r),
    ];
    AutocommitCensus {
        caller,
        commit,
        statement_end,
        rollback_caller,
        sites,
        drain_caller,
        cleanup,
        unbounded,
    }
}
