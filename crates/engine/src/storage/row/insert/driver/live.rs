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
    RetireStop, Retired, Rollback, RollbackNext, RollbackStop, RollbackTask, StartedRollback,
    StatementView, Stepped, ValidationLane, call_sites, caller_bytes, carrier_bytes,
    finish_aborted, finish_rollback, finish_statement, insert_statement, map_commit_result,
    retire_finished, retry_task, rollback_step, start_full_rollback, start_statement_rollback,
};
use crate::storage::{
    disk::{
        CheckedTable, MaintenancePort, MutationPort, Owner, Plan4Target, PreflightRow,
        PreparedRollbackTask, RetentionError, RetentionFlushPhase, RollbackScope, RollbackStep,
        RowCommitObservation, RowInputPlan, RowMutationObservation, RowOwnedError,
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
