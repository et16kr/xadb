//! Thin forwarding of the driver traits to the actual C0 Row write types.
//! Nothing here constructs a C value: every wrapper, lane, observation,
//! target and error comes from the original MutationPort, Owner, statement
//! view and transaction capsule. A writable capsule exists
//! (`MutationPort::begin_catalog_transaction`), `begin_statement` accepts its
//! empty actor, the Catalog statement call issues a `CheckedTable`
//! (`finish_table`) and `MutationPort::preflight_row` runs the actual
//! preflight. Which later Row write bodies are actual is stated by the C0
//! boundary (`plan4_apply/row.rs`). `PreflightRow::reserve` (S2-b2), the
//! Forward `ReservedRow::producer_lane`, `ReservedRow::prepare` and
//! `PreparedRow::apply` (S2-c) and `ReservedRow::cancel` (S2-d) are actual: a
//! call that reaches reserve writes a durable RowReserve, and the
//! `ReservedRow` it returns fences the session when it is dropped while its
//! reservation is active. A call that ends the reservation with a durable
//! Forward apply or a Cancel discharges it and ends without the fence; a call
//! that stops after a successful reserve without either ends fenced (the
//! Drop). A body not supplied yet (the Row read source, rollback and Commit
//! adapters) returns Unchanged `Unsupported` before any effect of its own.
//! `insert_live` has no product caller in S2: S2-e runs the live driver from
//! C's tests (`catalog_memory/row_live.rs`), which closes gap G2. A preflight
//! refusal, Busy included (a LOAD claim held by another owner, a moved WAL
//! prefix), ends the row holding nothing and is not retried; only a
//! `FlushRequired` is flushed and the row tried again.
use super::{
    Boundary, CALL_SITES, CallSite, DriveError, Failure, FlushStep, Loan, PendingRollback,
    Preflight, PreflightNext, Prepared, PreparedTask, ProducerLane, Reserved, Rollback,
    RollbackNext, RollbackStop, RollbackTask, StartedRollback, StatementView, Stepped,
    ValidationLane, call_sites, caller_bytes, carrier_bytes, commit, finish_aborted,
    finish_rollback, finish_statement, insert_statement, retry_task, rollback_step,
    start_full_rollback, start_statement_rollback,
};
use crate::storage::{
    disk::{
        CheckedTable, MaintenancePort, MutationPort, Owner, Plan4Target, PreflightRow,
        PreparedRollbackTask, PreparedRow, ReservedRow, RetentionError, RetentionFlushPhase,
        RollbackScope, RollbackStep, RowCommitObservation, RowInputPlan, RowMutationObservation,
        RowOwnedError, RowPreflightStep, RowProducerLane, RowProposal, RowRollback,
        RowRollbackTask, RowStatementView, RowTransaction, RowValidationLane, StatementFinishError,
        TransactionFinishError,
    },
    row::insert::{InsertError, produce_clr, produce_insert, validate_insert},
    row_record::ValueRef,
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

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Preflight
    for PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Applied = RowMutationObservation;
    type Lane<'l>
        = RowValidationLane<'l>
    where
        Self: 'l;
    type Reserved = ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn validation_lane(&mut self) -> Result<RowValidationLane<'_>, RetentionError> {
        PreflightRow::validation_lane(self)
    }
    fn reserve(self, input: RowInputPlan) -> Result<Self::Reserved, RowOwnedError<Self>> {
        PreflightRow::reserve(self, input)
    }
    fn finish(self) -> Result<(), RetentionError> {
        PreflightRow::finish(self).map(|_| ())
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Reserved
    for ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Applied = RowMutationObservation;
    type Lane<'l>
        = RowProducerLane<'l>
    where
        Self: 'l;
    type Prepared = PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn producer_lane(&mut self) -> Result<RowProducerLane<'_>, RetentionError> {
        ReservedRow::producer_lane(self)
    }
    fn prepare(self, proposal: RowProposal) -> Result<Self::Prepared, RowOwnedError<Self>> {
        ReservedRow::prepare(self, proposal)
    }
    fn cancel(self) -> Result<(), RowOwnedError<Self>> {
        ReservedRow::cancel(self).map(|_| ())
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Prepared
    for PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Reserved = ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    type Applied = RowMutationObservation;
    fn apply(self) -> Result<RowMutationObservation, RowOwnedError<Self>> {
        PreparedRow::apply(self).map(|applied| applied.into_parts().0)
    }
    fn reopen(self) -> Self::Reserved {
        PreparedRow::reopen(self)
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
    /// still held (gaps G1 and G5).
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
        self.port
            .start_row_rollback(self.owner, transaction, statement, scope, loan)
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
    commit(call, transaction, private)
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

/// `caller_bytes` for the actual C0 types: (the term for a statement of
/// `rows` rows of `values` values, the part each further row adds).
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
    >(rows, values)
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
