//! MCA00-D01..14 and AW1-F01..02 are C0_ONLY. The driver runs over literal
//! stand-ins of C's Row write boundary and flusher: a non-persistent call
//! log, a row high-water, the actor's chain of applied rows, the statement
//! mark, the flusher's phase and scripted refusals. Refusals are C0's own
//! value: `Unsupported` or another choice with `Unchanged`, or a
//! `CallerMustResolve` fatal. No PreflightRow,
//! ReservedRow, RowRollback, lane, pin, WAL, receipt, ACK or READY exists
//! here, and nothing counts toward MCA01–05. The validation stand-in runs
//! A's pure `encode_input` into the loan's candidate area. The producer
//! stand-in returns literal proposals, because A's producer needs C's lane
//! (it is covered by MCA00-V06). A rollback stand-in only goes on from what
//! it handed back (the same rollback or task); none is started twice.
//! Expected bytes, digests and frame lengths are independent literals from
//! the contract: canonical rows and SHA-256 by
//! `tests/fixtures/module-row-insert/make_driver_literals.py` (stdlib
//! struct/hashlib), Forward `91304+C+128h` and CLR `91096+C+128h` with
//! `C=180+L`.
use super::*;
use crate::{
    api::{Column, DataType},
    storage::{
        disk::{RetentionCause, RetentionPhase, RowOperation, RowPageRole},
        row::insert::encode_input,
        row_record::{RowError, RowSchemaInput},
    },
};
use Call::{
    Apply, Cancel, Check, Clr, ClrApply, ClrLane, ClrPrepare, ClrReopen, Commit, ContinueFlush,
    DropReserved, DropRollback, FinishAborted, FinishPreflight, FinishStatement, Forward, Next,
    Prepare, Reopen, Reserve, RollbackFinish, RollbackStart, TryFlush, Validate,
};
use foundation::identity::SchemaEpoch;
use std::cell::RefCell;

const CANDIDATE: usize = 16_384;
const FILL: u8 = 0xa5;
const ROLES: [RowPageRole; 5] = [
    RowPageRole::Root,
    RowPageRole::RootJob,
    RowPageRole::Status,
    RowPageRole::Undo,
    RowPageRole::RowLeaf,
];

// Canonical rows: u32 LE column count, Int64 tag 2 + i64 LE, Utf8 tag 3 +
// u32 LE length + bytes. R1 = (42, "xadb") L22, R2 = (-1, "") L18,
// R3 = (7, "한") L21.
const R1: [u8; 22] = [
    0x02, 0x00, 0x00, 0x00, 0x02, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x04, 0x00,
    0x00, 0x00, 0x78, 0x61, 0x64, 0x62,
];
const R1_SHA: [u8; 32] = [
    0x47, 0xd3, 0x56, 0x02, 0x22, 0x3d, 0x43, 0xbd, 0x5e, 0xcd, 0x01, 0x4f, 0x2a, 0x3e, 0xea, 0x0e,
    0x0d, 0xf1, 0x89, 0xf6, 0x63, 0x55, 0xbb, 0xb9, 0x3a, 0x22, 0x81, 0x80, 0x88, 0x70, 0x61, 0x21,
];
const R2: [u8; 18] = [
    0x02, 0x00, 0x00, 0x00, 0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x03, 0x00, 0x00,
    0x00, 0x00,
];
const R2_SHA: [u8; 32] = [
    0xd2, 0x9c, 0x11, 0x20, 0x1e, 0x27, 0xe6, 0xe5, 0xcd, 0xbf, 0xaa, 0x59, 0x07, 0x8b, 0xf5, 0x21,
    0x3e, 0x79, 0x5a, 0x27, 0x84, 0xff, 0x4f, 0x54, 0x7d, 0x7e, 0x39, 0x17, 0xcf, 0x3e, 0x5a, 0xbf,
];
const R3: [u8; 21] = [
    0x02, 0x00, 0x00, 0x00, 0x02, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x03, 0x00,
    0x00, 0x00, 0xed, 0x95, 0x9c,
];
const R3_SHA: [u8; 32] = [
    0x67, 0xcf, 0x0e, 0x0f, 0x8d, 0xfd, 0x01, 0xc0, 0x74, 0xb9, 0xdc, 0x12, 0x24, 0xfc, 0x64, 0x86,
    0x94, 0xfb, 0xe4, 0xf4, 0xf6, 0xd7, 0x61, 0xb1, 0x7d, 0x6e, 0x83, 0x5c, 0x04, 0x4f, 0xb1, 0xe6,
];
const V1: [ValueRef<'static>; 2] = [ValueRef::Int64(42), ValueRef::Utf8("xadb")];
const V2: [ValueRef<'static>; 2] = [ValueRef::Int64(-1), ValueRef::Utf8("")];
const V3: [ValueRef<'static>; 2] = [ValueRef::Int64(7), ValueRef::Utf8("한")];
const BAD: [ValueRef<'static>; 2] = [ValueRef::Utf8("a"), ValueRef::Int64(1)];

/// C's calls in order, with the RowId a reservation or CLR is for, as the
/// literal stand-ins record them. Drop entries are wrappers the driver
/// dropped while they still held a reservation or a rollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Check,
    Preflight,
    ValidationLane,
    Validate,
    Reserve(u64),
    FinishPreflight,
    DropPreflight,
    ProducerLane(u64),
    Forward(u64),
    Prepare(u64),
    Apply(u64),
    Reopen(u64),
    Cancel(u64),
    DropReserved(u64),
    RollbackStart(RollbackScope),
    Next,
    ClrLane(u64),
    Clr(u64),
    ClrPrepare(u64),
    ClrApply(u64),
    ClrReopen(u64),
    RollbackFinish,
    DropRollback,
    FinishStatement,
    Commit,
    FinishAborted,
    /// The dedicated flusher's `try_flush` of a target, and its continues.
    TryFlush(u64),
    ContinueFlush,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cause {
    Busy,
    Capacity,
    Unsupported,
    WrongState,
    SerializationConflict,
    Fenced,
}

/// A scripted answer at one exact call.
#[derive(Clone, Copy, Debug)]
enum Fault {
    Before(Cause),
    Fatal,
    Producer(InsertError),
    /// C's FlushRequired with a literal target (rollback `next`; a
    /// preflight's comes from `State::flushes`).
    Flush(u64),
    /// `try_flush` succeeds with the flusher this many `continue_flush`
    /// calls away from Ready (WaitingWal, WaitingSource, finalize).
    Steps(u8),
    RollbackRequired,
    Rejected,
}

/// The C0 wrappers' phases: Prepare for the Row write/rollback steps,
/// Finalize for commit_row/finish_aborted, Census for statement checks.
fn refusal(call: Call, cause: Cause) -> RetentionError {
    let phase = match call {
        Commit | FinishAborted => RetentionPhase::Finalize,
        Check | FinishStatement => RetentionPhase::Census,
        TryFlush(_) | ContinueFlush => RetentionPhase::Flush,
        _ => RetentionPhase::Prepare,
    };
    let cause = match cause {
        Cause::Busy => RetentionCause::Busy,
        Cause::Capacity => RetentionCause::Capacity,
        Cause::Unsupported => RetentionCause::Unsupported,
        Cause::WrongState => RetentionCause::WrongState,
        Cause::SerializationConflict => RetentionCause::SerializationConflict,
        Cause::Fenced => RetentionCause::Fenced,
    };
    RetentionError {
        phase,
        cause,
        effect: Effect::Unchanged,
    }
}

fn fatal() -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Install,
        cause: RetentionCause::Fenced,
        effect: Effect::CallerMustResolve,
    }
}

/// A factory or lane step: no wrapper is consumed.
fn factory(call: Call, fault: Option<Fault>) -> Result<(), RetentionError> {
    match fault {
        None => Ok(()),
        Some(Fault::Before(cause)) => Err(refusal(call, cause)),
        Some(Fault::Fatal) => Err(fatal()),
        Some(other) => panic!("unscripted {other:?} at {call:?}"),
    }
}

/// A consuming step: Before returns the same wrapper; Fatal leaves C
/// holding the obligation, so the stand-in disarms its drop record.
trait Disarm {
    fn disarm(self);
}

fn step<T: Disarm>(call: Call, fault: Option<Fault>, owned: T) -> Result<T, RowOwnedError<T>> {
    match fault {
        None => Ok(owned),
        Some(Fault::Before(cause)) => Err(RowOwnedError::Before {
            error: refusal(call, cause),
            owned,
        }),
        Some(Fault::Fatal) => {
            owned.disarm();
            Err(RowOwnedError::Fatal(fatal()))
        }
        Some(other) => panic!("unscripted {other:?} at {call:?}"),
    }
}

#[inline(never)]
fn probe() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

#[derive(Debug)]
struct State {
    log: Vec<Call>,
    faults: Vec<(Call, Fault)>,
    columns: Vec<Column>,
    free: usize,
    row_high: u64,
    /// Applied rows of the actor (RowId, canonical length), head last.
    chain: Vec<(u64, usize)>,
    sealed: usize,
    outstanding: Option<u64>,
    violations: Vec<&'static str>,
    inputs: Vec<String>,
    proposals: Vec<String>,
    deepest: usize,
    /// Dirty victims, as (RowId the preflight is for, target): a preflight
    /// for that row answers FlushRequired once per entry, in order.
    flushes: Vec<(u64, u64)>,
    /// The FlushRequired target not flushed yet.
    dirty: Option<u64>,
    /// `continue_flush` calls left before the flusher is Ready.
    waiting: u8,
}

impl State {
    fn note(&mut self, call: Call) -> Option<Fault> {
        self.log.push(call);
        self.faults
            .iter()
            .find(|(at, _)| *at == call)
            .map(|(_, fault)| *fault)
    }
}

fn columns() -> Vec<Column> {
    [("id", DataType::Int64), ("name", DataType::Utf8)]
        .into_iter()
        .map(|(name, data_type)| Column {
            name: name.into(),
            data_type,
            nullable: false,
        })
        .collect()
}

/// Fresh literal C state: RowId high-water 6 and the given prior chain.
fn state(chain: &[(u64, usize)], faults: &[(Call, Fault)]) -> RefCell<State> {
    RefCell::new(State {
        log: Vec::new(),
        faults: faults.to_vec(),
        columns: columns(),
        free: 16_188,
        row_high: 6,
        chain: chain.to_vec(),
        sealed: 0,
        outstanding: None,
        violations: Vec::new(),
        inputs: Vec::new(),
        proposals: Vec::new(),
        deepest: usize::MAX,
        flushes: Vec::new(),
        dirty: None,
        waiting: 0,
    })
}

#[derive(Debug)]
struct LiteralStatement<'s> {
    state: &'s RefCell<State>,
    ended: bool,
    mark: usize,
}

impl StatementView for LiteralStatement<'_> {
    fn check(&self) -> Result<(), RetentionError> {
        let fault = self.state.borrow_mut().note(Check);
        if self.ended {
            return Err(refusal(Check, Cause::WrongState));
        }
        factory(Check, fault)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LiteralTransaction(u32);

struct LiteralTable;

/// A literal observation: the applied RowId.
#[derive(Debug, PartialEq, Eq)]
struct LiteralApplied(u64);

struct LiteralCall<'s> {
    state: &'s RefCell<State>,
}

impl<'s> Boundary for LiteralCall<'s> {
    type Transaction = LiteralTransaction;
    type Statement = LiteralStatement<'s>;
    type Table = LiteralTable;
    type Applied = LiteralApplied;
    type Committed = u64;
    type Target = u64;
    type Preflight<'r, 'i>
        = LiteralPreflight<'r, 'i>
    where
        Self: 'r;
    type Rollback<'r, 'i>
        = LiteralRollback<'r, 'i>
    where
        Self: 'r;
    fn preflight<'r, 'i>(
        &'r mut self,
        _statement: &'r LiteralStatement<'s>,
        _table: &'r LiteralTable,
        loan: &'i mut Loan,
    ) -> Result<PreflightNext<LiteralPreflight<'r, 'i>, u64>, RetentionError> {
        let mut state = self.state.borrow_mut();
        let fault = state.note(Call::Preflight);
        // C refuses a flusher that is not Ready before anything else.
        if state.waiting > 0 {
            state.violations.push("preflight with a flusher not Ready");
            return Err(refusal(Call::Preflight, Cause::Busy));
        }
        if state.dirty.is_some() {
            state
                .violations
                .push("preflight before the named victim was flushed");
        }
        let row = state.row_high + 1;
        if let Some(at) = state.flushes.iter().position(|(at, _)| *at == row) {
            let (_, target) = state.flushes.remove(at);
            state.dirty = Some(target);
            return Ok(PreflightNext::Flush(target));
        }
        drop(state);
        factory(Call::Preflight, fault)?;
        Ok(PreflightNext::Ready(LiteralPreflight {
            state: self.state,
            loan: Some(loan),
            done: false,
        }))
    }
    fn start_rollback<'r, 'i>(
        &'r mut self,
        _transaction: &'r LiteralTransaction,
        statement: Option<&'r LiteralStatement<'s>>,
        scope: RollbackScope,
        loan: &'i mut Loan,
    ) -> Result<LiteralRollback<'r, 'i>, RetentionError> {
        let call = RollbackStart(scope);
        let fault = self.state.borrow_mut().note(call);
        factory(call, fault)?;
        let target = match (scope, statement) {
            (RollbackScope::Statement, Some(statement)) => statement.mark,
            (RollbackScope::Transaction, None) => 0,
            _ => {
                let mut state = self.state.borrow_mut();
                state.violations.push("rollback scope and statement differ");
                0
            }
        };
        Ok(LiteralRollback {
            state: self.state,
            target,
            _loan: loan,
            done: false,
        })
    }
    fn finish_statement(
        &mut self,
        statement: LiteralStatement<'s>,
    ) -> Result<(), StatementFinishError<LiteralStatement<'s>>> {
        match self.state.borrow_mut().note(FinishStatement) {
            None => Ok(()),
            Some(Fault::Before(cause)) => Err(StatementFinishError::Before {
                error: refusal(FinishStatement, cause),
                view: statement,
            }),
            Some(Fault::Fatal) => Err(StatementFinishError::Fatal(fatal())),
            Some(other) => panic!("unscripted {other:?}"),
        }
    }
    fn commit(
        &mut self,
        transaction: LiteralTransaction,
        _loan: &mut Loan,
    ) -> Result<u64, TransactionFinishError<LiteralTransaction>> {
        let conflict = || refusal(Commit, Cause::SerializationConflict);
        match self.state.borrow_mut().note(Commit) {
            None => Ok(42),
            Some(Fault::Before(cause)) => Err(TransactionFinishError::Before {
                error: refusal(Commit, cause),
                transaction,
            }),
            Some(Fault::RollbackRequired) => Err(TransactionFinishError::RollbackRequired {
                error: conflict(),
                transaction,
            }),
            Some(Fault::Rejected) => Err(TransactionFinishError::Rejected(conflict())),
            Some(Fault::Fatal) => Err(TransactionFinishError::Fatal(fatal())),
            Some(other) => panic!("unscripted {other:?}"),
        }
    }
    fn finish_aborted(
        &mut self,
        transaction: LiteralTransaction,
    ) -> Result<(), TransactionFinishError<LiteralTransaction>> {
        match self.state.borrow_mut().note(FinishAborted) {
            None => Ok(()),
            Some(Fault::Before(cause)) => Err(TransactionFinishError::Before {
                error: refusal(FinishAborted, cause),
                transaction,
            }),
            Some(Fault::RollbackRequired) => Err(TransactionFinishError::RollbackRequired {
                error: refusal(FinishAborted, Cause::SerializationConflict),
                transaction,
            }),
            Some(Fault::Fatal) => Err(TransactionFinishError::Fatal(fatal())),
            Some(other) => panic!("unscripted {other:?}"),
        }
    }
    /// Flushing the named victim cleans it whether the flush completes here
    /// or after `Steps` continues; any other target is a driver error.
    fn try_flush(&mut self, target: &u64) -> Result<(), RetentionError> {
        let mut state = self.state.borrow_mut();
        let fault = state.note(TryFlush(*target));
        if state.waiting > 0 {
            state.violations.push("try_flush with a flusher not Ready");
        }
        if state.dirty != Some(*target) {
            state.violations.push("flush of a target C did not name");
        }
        if let Some(Fault::Steps(steps)) = fault {
            state.dirty = None;
            state.waiting = steps;
            return Ok(());
        }
        drop(state);
        factory(TryFlush(*target), fault)?;
        self.state.borrow_mut().dirty = None;
        Ok(())
    }
    fn continue_flush(&mut self) -> Result<(), RetentionError> {
        let mut state = self.state.borrow_mut();
        let fault = state.note(ContinueFlush);
        if state.waiting == 0 {
            state.violations.push("continue_flush with a Ready flusher");
            return Err(refusal(ContinueFlush, Cause::WrongState));
        }
        drop(state);
        factory(ContinueFlush, fault)?;
        self.state.borrow_mut().waiting -= 1;
        Ok(())
    }
    fn flush_ready(&self) -> bool {
        self.state.borrow().waiting == 0
    }
}

struct LiteralPreflight<'r, 'i> {
    state: &'r RefCell<State>,
    loan: Option<&'i mut Loan>,
    done: bool,
}

impl Drop for LiteralPreflight<'_, '_> {
    fn drop(&mut self) {
        if !self.done {
            self.state.borrow_mut().note(Call::DropPreflight);
        }
    }
}

impl Disarm for LiteralPreflight<'_, '_> {
    fn disarm(mut self) {
        self.done = true;
    }
}

impl<'r, 'i> Preflight for LiteralPreflight<'r, 'i> {
    type Applied = LiteralApplied;
    type Lane<'l>
        = LiteralValidation<'l>
    where
        Self: 'l;
    type Reserved = LiteralReserved<'r, 'i>;
    fn validation_lane(&mut self) -> Result<LiteralValidation<'_>, RetentionError> {
        let fault = self.state.borrow_mut().note(Call::ValidationLane);
        factory(Call::ValidationLane, fault)?;
        let loan = self.loan.as_deref_mut().expect("loan held until release");
        Ok(LiteralValidation {
            state: self.state,
            candidate: &mut loan[..CANDIDATE],
        })
    }
    fn reserve(self, input: RowInputPlan) -> Result<LiteralReserved<'r, 'i>, RowOwnedError<Self>> {
        let row = self.state.borrow().row_high + 1;
        let fault = self.state.borrow_mut().note(Reserve(row));
        let mut this = step(Reserve(row), fault, self)?;
        let mut state = this.state.borrow_mut();
        if state.outstanding.replace(row).is_some() {
            state.violations.push("second outstanding RowReserve");
        }
        state.row_high = row;
        state.inputs.push(format!("{input:?}"));
        let reserved = LiteralReserved {
            state: this.state,
            loan: this.loan.take(),
            row,
            cell: 180 + state.sealed,
            h: !state.chain.is_empty(),
            active: true,
        };
        drop(state);
        this.done = true;
        Ok(reserved)
    }
    fn finish(mut self) -> Result<(), RetentionError> {
        let fault = self.state.borrow_mut().note(FinishPreflight);
        self.done = true;
        factory(FinishPreflight, fault)
    }
}

struct LiteralValidation<'l> {
    state: &'l RefCell<State>,
    candidate: &'l mut [u8],
}

impl ValidationLane for LiteralValidation<'_> {
    fn validate(self, values: &[ValueRef<'_>]) -> Result<RowInputPlan, InsertError> {
        let (columns, free) = {
            let mut state = self.state.borrow_mut();
            state.note(Validate);
            (state.columns.clone(), state.free)
        };
        let schema = RowSchemaInput::Columns(&columns);
        let (length, digest) = encode_input(schema, values, free, self.candidate)?;
        self.state.borrow_mut().sealed = length;
        Ok(RowInputPlan::new(
            length,
            SchemaEpoch::new(1).unwrap(),
            digest,
        ))
    }
}

struct LiteralReserved<'r, 'i> {
    state: &'r RefCell<State>,
    loan: Option<&'i mut Loan>,
    row: u64,
    cell: usize,
    h: bool,
    active: bool,
}

impl Drop for LiteralReserved<'_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.state.borrow_mut().note(DropReserved(self.row));
        }
    }
}

impl Disarm for LiteralReserved<'_, '_> {
    fn disarm(mut self) {
        self.active = false;
    }
}

impl<'r, 'i> Reserved for LiteralReserved<'r, 'i> {
    type Applied = LiteralApplied;
    type Lane<'l>
        = LiteralProducer<'l>
    where
        Self: 'l;
    type Prepared = LiteralPrepared<'r, 'i>;
    fn producer_lane(&mut self) -> Result<LiteralProducer<'_>, RetentionError> {
        let call = Call::ProducerLane(self.row);
        let fault = self.state.borrow_mut().note(call);
        factory(call, fault)?;
        Ok(LiteralProducer {
            state: self.state,
            operation: RowOperation::Forward,
            row: self.row,
            cell: self.cell,
            h: self.h,
            _loan: self.loan.as_deref_mut(),
        })
    }
    fn prepare(
        self,
        proposal: RowProposal,
    ) -> Result<LiteralPrepared<'r, 'i>, RowOwnedError<Self>> {
        let call = Prepare(self.row);
        let fault = {
            let mut state = self.state.borrow_mut();
            state.proposals.push(format!("{proposal:?}"));
            state.note(call)
        };
        Ok(LiteralPrepared {
            reserved: step(call, fault, self)?,
        })
    }
    fn cancel(self) -> Result<(), RowOwnedError<Self>> {
        let call = Cancel(self.row);
        let fault = self.state.borrow_mut().note(call);
        let mut this = step(call, fault, self)?;
        this.state.borrow_mut().outstanding = None;
        this.active = false;
        Ok(())
    }
}

struct LiteralPrepared<'r, 'i> {
    reserved: LiteralReserved<'r, 'i>,
}

impl Disarm for LiteralPrepared<'_, '_> {
    fn disarm(self) {
        self.reserved.disarm();
    }
}

impl<'r, 'i> Prepared for LiteralPrepared<'r, 'i> {
    type Reserved = LiteralReserved<'r, 'i>;
    type Applied = LiteralApplied;
    fn apply(self) -> Result<LiteralApplied, RowOwnedError<Self>> {
        let call = Apply(self.reserved.row);
        let fault = self.reserved.state.borrow_mut().note(call);
        let mut reserved = step(call, fault, self)?.reserved;
        let mut state = reserved.state.borrow_mut();
        state.chain.push((reserved.row, reserved.cell - 180));
        state.outstanding = None;
        drop(state);
        reserved.active = false;
        Ok(LiteralApplied(reserved.row))
    }
    fn reopen(self) -> LiteralReserved<'r, 'i> {
        let reserved = self.reserved;
        reserved.state.borrow_mut().note(Reopen(reserved.row));
        reserved
    }
}

/// Stands in for A's producer over C's lane: a literal proposal from the
/// contract frame formula. It refuses the other operation, as A does.
struct LiteralProducer<'l> {
    state: &'l RefCell<State>,
    operation: RowOperation,
    row: u64,
    cell: usize,
    h: bool,
    _loan: Option<&'l mut Loan>,
}

impl LiteralProducer<'_> {
    fn produce(self, operation: RowOperation) -> Result<RowProposal, InsertError> {
        let (call, base) = match operation {
            RowOperation::Forward => (Forward(self.row), 91_304),
            RowOperation::Clr => (Clr(self.row), 91_096),
        };
        let mut state = self.state.borrow_mut();
        let fault = state.note(call);
        state.deepest = state.deepest.min(probe());
        if operation != self.operation {
            return Err(InsertError::Unsupported);
        }
        match fault {
            None => {}
            Some(Fault::Producer(error)) => return Err(error),
            Some(other) => panic!("unscripted {other:?}"),
        }
        let frame = base + self.cell + 128 * usize::from(self.h);
        Ok(RowProposal::new(operation, frame, 5, ROLES))
    }
}

impl ProducerLane for LiteralProducer<'_> {
    fn forward(self) -> Result<RowProposal, InsertError> {
        self.produce(RowOperation::Forward)
    }
    fn clr(self) -> Result<RowProposal, InsertError> {
        self.produce(RowOperation::Clr)
    }
}

struct LiteralRollback<'r, 'i> {
    state: &'r RefCell<State>,
    target: usize,
    _loan: &'i mut Loan,
    done: bool,
}

impl Drop for LiteralRollback<'_, '_> {
    fn drop(&mut self) {
        if !self.done {
            self.state.borrow_mut().note(DropRollback);
        }
    }
}

impl Disarm for LiteralRollback<'_, '_> {
    fn disarm(mut self) {
        self.done = true;
    }
}

impl<'r, 'i> Rollback for LiteralRollback<'r, 'i> {
    type Target = u64;
    type Task<'x>
        = LiteralTask<'x, 'r, 'i>
    where
        Self: 'x;
    fn next(&mut self) -> Result<RollbackNext<LiteralTask<'_, 'r, 'i>, u64>, RetentionError> {
        let mut state = self.state.borrow_mut();
        match state.note(Next) {
            Some(Fault::Flush(target)) => return Ok(RollbackNext::Flush(target)),
            fault => factory(Next, fault)?,
        }
        if state.chain.len() <= self.target {
            return Ok(RollbackNext::Complete);
        }
        let (row, length) = *state.chain.last().unwrap();
        let h = state.chain.len() >= 2;
        drop(state);
        Ok(RollbackNext::Candidate(LiteralTask {
            rollback: self,
            row,
            cell: 180 + length,
            h,
        }))
    }
    fn finish(self) -> Result<(), RowOwnedError<Self>> {
        let fault = self.state.borrow_mut().note(RollbackFinish);
        let mut this = step(RollbackFinish, fault, self)?;
        if this.state.borrow().chain.len() != this.target {
            this.state
                .borrow_mut()
                .violations
                .push("rollback finished off its mark");
        }
        this.done = true;
        Ok(())
    }
}

struct LiteralTask<'x, 'r, 'i> {
    rollback: &'x mut LiteralRollback<'r, 'i>,
    row: u64,
    cell: usize,
    h: bool,
}

impl Disarm for LiteralTask<'_, '_, '_> {
    fn disarm(self) {}
}

impl<'x, 'r, 'i> RollbackTask for LiteralTask<'x, 'r, 'i> {
    type Lane<'l>
        = LiteralProducer<'l>
    where
        Self: 'l;
    type Prepared = LiteralPreparedTask<'x, 'r, 'i>;
    fn producer_lane(&mut self) -> Result<LiteralProducer<'_>, RetentionError> {
        let call = ClrLane(self.row);
        let fault = self.rollback.state.borrow_mut().note(call);
        factory(call, fault)?;
        Ok(LiteralProducer {
            state: self.rollback.state,
            operation: RowOperation::Clr,
            row: self.row,
            cell: self.cell,
            h: self.h,
            _loan: Some(&mut *self.rollback._loan),
        })
    }
    fn prepare(
        self,
        proposal: RowProposal,
    ) -> Result<LiteralPreparedTask<'x, 'r, 'i>, RowOwnedError<Self>> {
        let call = ClrPrepare(self.row);
        let fault = {
            let mut state = self.rollback.state.borrow_mut();
            state.proposals.push(format!("{proposal:?}"));
            state.note(call)
        };
        Ok(LiteralPreparedTask {
            task: step(call, fault, self)?,
        })
    }
}

struct LiteralPreparedTask<'x, 'r, 'i> {
    task: LiteralTask<'x, 'r, 'i>,
}

impl Disarm for LiteralPreparedTask<'_, '_, '_> {
    fn disarm(self) {}
}

impl<'x, 'r, 'i> PreparedTask for LiteralPreparedTask<'x, 'r, 'i> {
    type Task = LiteralTask<'x, 'r, 'i>;
    fn apply(self) -> Result<(), RowOwnedError<Self>> {
        let call = ClrApply(self.task.row);
        let fault = self.task.rollback.state.borrow_mut().note(call);
        let this = step(call, fault, self)?;
        this.task.rollback.state.borrow_mut().chain.pop();
        Ok(())
    }
    fn reopen(self) -> LiteralTask<'x, 'r, 'i> {
        let task = self.task;
        task.rollback.state.borrow_mut().note(ClrReopen(task.row));
        task
    }
}

fn loan() -> Box<Loan> {
    vec![FILL; size_of::<Loan>()]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}

/// A live statement whose mark is the current chain length.
fn statement(state: &RefCell<State>) -> LiteralStatement<'_> {
    let mark = state.borrow().chain.len();
    LiteralStatement {
        state,
        ended: false,
        mark,
    }
}

type Outcome<'s> = Result<usize, DriveError<LiteralStatement<'s>>>;

#[allow(
    clippy::result_large_err,
    reason = "the test harness returns the driver Failure unchanged"
)]
fn run<'s>(
    state: &'s RefCell<State>,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<LiteralApplied>],
    loan: &mut Loan,
) -> Outcome<'s> {
    insert_statement(
        &mut LiteralCall { state },
        statement(state),
        &LiteralTable,
        loan,
        rows,
        slots,
    )
}

fn plan(length: usize, digest: [u8; 32]) -> String {
    let plan = RowInputPlan::new(length, SchemaEpoch::new(1).unwrap(), digest);
    format!("{plan:?}")
}

fn proposal(operation: RowOperation, frame: usize) -> String {
    format!("{:?}", RowProposal::new(operation, frame, 5, ROLES))
}

/// The calls of one applied row.
fn applied(row: u64) -> [Call; 8] {
    [
        Call::Preflight,
        Call::ValidationLane,
        Validate,
        Reserve(row),
        Call::ProducerLane(row),
        Forward(row),
        Prepare(row),
        Apply(row),
    ]
}

/// The calls of one CLR.
fn compensated(row: u64) -> [Call; 5] {
    [Next, ClrLane(row), Clr(row), ClrPrepare(row), ClrApply(row)]
}

fn calls(parts: &[&[Call]]) -> Vec<Call> {
    parts.concat()
}

fn disk(error: &DriverError) -> (RetentionPhase, Cause, Effect) {
    let DriverError::Disk(error) = error else {
        panic!("expected a C error, got {error:?}");
    };
    let cause = match error.cause {
        RetentionCause::Busy => Cause::Busy,
        RetentionCause::Capacity => Cause::Capacity,
        RetentionCause::Unsupported => Cause::Unsupported,
        RetentionCause::WrongState => Cause::WrongState,
        RetentionCause::SerializationConflict => Cause::SerializationConflict,
        RetentionCause::Fenced => Cause::Fenced,
        ref other => panic!("unexpected cause {other:?}"),
    };
    (error.phase, cause, error.effect)
}

fn insert(error: &DriverError) -> InsertError {
    match error {
        DriverError::Insert(error) => *error,
        other => panic!("expected an A error, got {other:?}"),
    }
}

fn failed<H: std::fmt::Debug>(result: Result<usize, DriveError<H>>) -> DriveError<H> {
    result.expect_err("the call must fail")
}

fn name<H>(after: &After<H>) -> &'static str {
    match after {
        After::Returned(_) => "Returned",
        After::RollbackRequired(_) => "RollbackRequired",
        After::FinishPending(_) => "FinishPending",
        After::Closed => "Closed",
        After::MustResolve(_) => "MustResolve",
    }
}

fn returned<H>(after: After<H>) -> H {
    match after {
        After::Returned(handle) => handle,
        other => panic!("expected Returned, got {}", name(&other)),
    }
}

fn rollback_required<H>(after: After<H>) -> PendingRollback<H> {
    match after {
        After::RollbackRequired(pending) => pending,
        other => panic!("expected RollbackRequired, got {}", name(&other)),
    }
}

/// The handle a finished rollback released; anything else fails the test.
fn released<H>(pending: PendingRollback<H>) -> H {
    match pending.rolled_back() {
        Ok(handle) => handle,
        Err(pending) => panic!("handle withheld at {:?}", pending.progress),
    }
}

/// The pending kept back: its rollback has not finished.
fn withheld<H>(pending: PendingRollback<H>) -> PendingRollback<H> {
    match pending.rolled_back() {
        Ok(_) => panic!("the handle must stay inside the pending"),
        Err(pending) => pending,
    }
}

fn finish_pending<H>(after: After<H>) -> H {
    match after {
        After::FinishPending(handle) => handle,
        other => panic!("expected FinishPending, got {}", name(&other)),
    }
}

fn must_resolve<H>(after: After<H>) -> Option<H> {
    match after {
        After::MustResolve(handle) => handle,
        other => panic!("expected MustResolve, got {}", name(&other)),
    }
}

fn closed<H>(after: After<H>) {
    if !matches!(after, After::Closed) {
        panic!("expected Closed, got {}", name(&after));
    }
}

fn cause(error: RetentionError) -> (RetentionPhase, Cause, Effect) {
    disk(&DriverError::Disk(error))
}

/// No reservation or rollback is left behind and no rule was broken.
fn settled(state: &RefCell<State>) {
    let state = state.borrow();
    assert_eq!(state.outstanding, None);
    assert!(state.violations.is_empty(), "{:?}", state.violations);
}

/// The calls logged so far; the log restarts empty.
fn take_log(state: &RefCell<State>) -> Vec<Call> {
    std::mem::take(&mut state.borrow_mut().log)
}

/// Row 7 applied, row 2 rejected by A: the statement handed back
/// `RollbackRequired`, with the log cleared.
fn after_failed_row<'s>(
    state: &'s RefCell<State>,
    loan: &mut Loan,
) -> PendingRollback<LiteralStatement<'s>> {
    let error = failed(run(state, &[&V1, &BAD], &mut [None, None], loan));
    let pending = rollback_required(error.after);
    take_log(state);
    pending
}

/// A commit C answered `RollbackRequired`: the transaction handed back for
/// the full rollback only.
fn after_conflict(
    state: &RefCell<State>,
    transaction: u32,
    loan: &mut Loan,
) -> PendingRollback<LiteralTransaction> {
    let mut call = LiteralCall { state };
    let error = commit(&mut call, LiteralTransaction(transaction), loan).unwrap_err();
    assert_eq!(error.failure.stage, Stage::Commit);
    rollback_required(error.after)
}

fn started<K>(result: Result<K, Failure>) -> K {
    match result {
        Ok(rollback) => rollback,
        Err(failure) => panic!("rollback start refused: {failure:?}"),
    }
}

fn refused<K>(result: Result<K, Failure>) -> Failure {
    match result {
        Ok(_) => panic!("the rollback start must be refused"),
        Err(failure) => failure,
    }
}

fn stop_name<T, F>(stop: &RollbackStop<T, F>) -> &'static str {
    match stop {
        RollbackStop::Next(_) => "Next",
        RollbackStop::Flush(_) => "Flush",
        RollbackStop::Task { .. } => "Task",
        RollbackStop::MustResolve { .. } => "MustResolve",
    }
}

/// Steps the caller's rollback until C reports Complete; any stop fails.
fn complete<K: Rollback>(rollback: &mut K) -> usize {
    let mut compensated = 0;
    loop {
        match rollback_step(rollback) {
            Ok(Stepped::Compensated) => compensated += 1,
            Ok(Stepped::Complete) => return compensated,
            Err(stop) => panic!("the rollback stopped: {}", stop_name(&stop)),
        }
    }
}

/// The next step's stop; a step that goes on fails the test.
fn stopped<'x, K: Rollback>(rollback: &'x mut K) -> RollbackStop<K::Task<'x>, K::Target> {
    match rollback_step(rollback) {
        Err(stop) => stop,
        Ok(step) => panic!("expected a stop, got {step:?}"),
    }
}

/// The same task a stop handed back.
fn held_task<T, F>(stop: RollbackStop<T, F>) -> (Stage, DriverError, T) {
    match stop {
        RollbackStop::Task { stage, error, task } => (stage, error, task),
        other => panic!("expected the same task, got {}", stop_name(&other)),
    }
}

fn finished<K: Rollback>(rollback: K) {
    if finish_rollback(rollback).is_err() {
        panic!("the rollback finish was refused");
    }
}

/// The caller's statement rollback of a `RollbackRequired` statement, then
/// its finish; returns the CLR count.
fn roll_back_statement<'s>(
    state: &'s RefCell<State>,
    mut pending: PendingRollback<LiteralStatement<'s>>,
    loan: &mut Loan,
) -> usize {
    let mut call = LiteralCall { state };
    let transaction = LiteralTransaction(1);
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut pending,
        loan,
    ));
    let compensated = complete(&mut rollback);
    finished(rollback);
    finish_statement(&mut call, released(pending)).unwrap();
    compensated
}

#[test]
fn mca00_d01_one_row_statement_reserves_produces_applies_and_finishes() {
    let state = self::state(&[], &[]);
    let mut loan = loan();
    let mut slots = [None];
    assert_eq!(run(&state, &[&V1], &mut slots, &mut loan).unwrap(), 1);
    assert_eq!(slots, [Some(LiteralApplied(7))]);
    let state = state.into_inner();
    assert_eq!(
        state.log,
        calls(&[&[Check], &applied(7), &[FinishStatement]])
    );
    assert_eq!(state.inputs, [plan(22, R1_SHA)]);
    assert_eq!(state.proposals, [proposal(RowOperation::Forward, 91_506)]);
    assert_eq!(state.chain, [(7, 22)]);
    assert_eq!((state.outstanding, state.violations.len()), (None, 0));
    // A's canonical bytes in the candidate lane; the rest of the loan is untouched.
    assert_eq!(loan[..22], R1);
    assert!(loan[22..].iter().all(|byte| *byte == FILL));
}

#[test]
fn mca00_d02_rows_take_one_reservation_at_a_time() {
    let state = self::state(&[], &[]);
    let mut loan = loan();
    let mut slots = [None, None, None];
    let rows: [&[ValueRef<'_>]; 3] = [&V1, &V2, &V3];
    assert_eq!(run(&state, &rows, &mut slots, &mut loan).unwrap(), 3);
    assert_eq!(
        slots,
        [
            Some(LiteralApplied(7)),
            Some(LiteralApplied(8)),
            Some(LiteralApplied(9))
        ]
    );
    let state = state.into_inner();
    // Each Reserve follows the previous row's Apply.
    assert_eq!(
        state.log,
        calls(&[
            &[Check],
            &applied(7),
            &applied(8),
            &applied(9),
            &[FinishStatement]
        ])
    );
    assert_eq!(
        state.inputs,
        [plan(22, R1_SHA), plan(18, R2_SHA), plan(21, R3_SHA)]
    );
    assert_eq!(
        state.proposals,
        [
            proposal(RowOperation::Forward, 91_506),
            proposal(RowOperation::Forward, 91_630),
            proposal(RowOperation::Forward, 91_633),
        ]
    );
    assert_eq!(state.chain, [(7, 22), (8, 18), (9, 21)]);
    assert_eq!((state.outstanding, state.violations.len()), (None, 0));
    // The last candidate is R3; A's encoder keeps the longer R1's tail byte.
    assert_eq!(loan[..21], R3);
    assert_eq!(loan[21], R1[21]);
}

#[test]
fn mca00_d03_statement_and_arguments_are_checked_before_any_c_call() {
    let thirty_three = [ValueRef::Null; 33];
    let mut loan = loan();
    // An ended statement wins over bad arguments.
    let state = self::state(&[], &[]);
    let ended = LiteralStatement {
        state: &state,
        ended: true,
        mark: 0,
    };
    let error = failed(insert_statement(
        &mut LiteralCall { state: &state },
        ended,
        &LiteralTable,
        &mut loan,
        &[],
        &mut [],
    ));
    assert_eq!(error.failure.stage, Stage::Statement);
    assert_eq!(
        disk(&error.failure.error),
        (RetentionPhase::Census, Cause::WrongState, Effect::Unchanged)
    );
    assert!(returned(error.after).ended);
    assert_eq!(state.borrow().log, [Check]);

    let cases: [(&[&[ValueRef<'_>]], usize); 4] = [
        (&[], 0),
        (&[&[]], 1),
        (&[&V1, &thirty_three], 2),
        (&[&V1], 2),
    ];
    for (rows, slot_count) in cases {
        let state = self::state(&[], &[]);
        let mut slots: Vec<Option<LiteralApplied>> = (0..slot_count).map(|_| None).collect();
        let error = failed(run(&state, rows, &mut slots, &mut loan));
        assert_eq!(error.failure.stage, Stage::Arguments);
        assert_eq!(insert(&error.failure.error), InsertError::InvalidInput);
        assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
        assert!(!returned(error.after).ended);
        assert_eq!(state.borrow().log, [Check]);
        assert!(slots.iter().all(Option::is_none));
    }
    assert!(loan.iter().all(|byte| *byte == FILL));
}

#[test]
fn mca00_d04_value_rejections_release_the_preflight_without_reserve() {
    let long = "x".repeat(2031); // 4 + 9 + 5 + 2031 = L2049
    let at_limit = "x".repeat(2030); // L2048
    let null: [ValueRef<'_>; 2] = [ValueRef::Null, ValueRef::Utf8("a")];
    let too_long: [ValueRef<'_>; 2] = [ValueRef::Int64(1), ValueRef::Utf8(&long)];
    let cases: [(&[ValueRef<'_>], usize, InsertError); 4] = [
        (&BAD, 16_188, InsertError::Row(RowError::SchemaMismatch)),
        (&null, 16_188, InsertError::Row(RowError::SchemaMismatch)),
        (&too_long, 16_188, InsertError::Unsupported),
        // Entry cost 184+L: R1 needs 206 bytes of leaf space.
        (&V1, 205, InsertError::Capacity),
    ];
    for (values, free, expected) in cases {
        let state = self::state(&[], &[]);
        state.borrow_mut().free = free;
        let mut loan = loan();
        let mut slots = [None];
        let error = failed(run(&state, &[values], &mut slots, &mut loan));
        assert_eq!(error.failure.stage, Stage::Validate);
        assert_eq!(insert(&error.failure.error), expected);
        assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
        assert!(!returned(error.after).ended);
        assert_eq!(
            state.borrow().log,
            [
                Check,
                Call::Preflight,
                Call::ValidationLane,
                Validate,
                FinishPreflight
            ]
        );
        assert_eq!(state.borrow().row_high, 6);
        settled(&state);
        assert!(loan.iter().all(|byte| *byte == FILL));
    }
    // The limits themselves pass: L2048, and exactly 206 free bytes for R1.
    let state = self::state(&[], &[]);
    let ok: [ValueRef<'_>; 2] = [ValueRef::Int64(1), ValueRef::Utf8(&at_limit)];
    assert_eq!(run(&state, &[&ok], &mut [None], &mut loan()).unwrap(), 1);
    let state = self::state(&[], &[]);
    state.borrow_mut().free = 206;
    assert_eq!(run(&state, &[&V1], &mut [None], &mut loan()).unwrap(), 1);

    // A rejection in row 2 hands the statement back with row 7 applied; the
    // driver starts no rollback. The caller's statement rollback compensates
    // it with the R1 h=0 CLR literal and finishes the statement.
    let state = self::state(&[], &[]);
    let mut slots = [None, None];
    let mut loan = loan();
    let error = failed(run(&state, &[&V1, &BAD], &mut slots, &mut loan));
    assert_eq!(error.failure.stage, Stage::Validate);
    assert_eq!(
        insert(&error.failure.error),
        InsertError::Row(RowError::SchemaMismatch)
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    let view = rollback_required(error.after);
    assert_eq!(slots, [Some(LiteralApplied(7)), None]);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check],
            &applied(7),
            &[
                Call::Preflight,
                Call::ValidationLane,
                Validate,
                FinishPreflight
            ],
        ])
    );
    assert_eq!(roll_back_statement(&state, view, &mut loan), 1);
    let state = state.into_inner();
    assert_eq!(
        state.log,
        calls(&[
            &[RollbackStart(RollbackScope::Statement)],
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement],
        ])
    );
    assert_eq!(state.proposals[1], proposal(RowOperation::Clr, 91_298));
    assert!(state.chain.is_empty() && state.violations.is_empty());
}

#[test]
fn mca00_d05_c_refusals_keep_busy_capacity_unsupported_distinct() {
    let front = [Check, Call::Preflight, Call::ValidationLane, Validate];
    let cases = [
        (
            Call::Preflight,
            Cause::Busy,
            Stage::Preflight,
            vec![Check, Call::Preflight],
        ),
        (
            Call::ValidationLane,
            Cause::Unsupported,
            Stage::ValidationLane,
            vec![
                Check,
                Call::Preflight,
                Call::ValidationLane,
                FinishPreflight,
            ],
        ),
        (
            Reserve(7),
            Cause::Capacity,
            Stage::Reserve,
            calls(&[&front, &[Reserve(7), FinishPreflight]]),
        ),
        (
            Reserve(7),
            Cause::Unsupported,
            Stage::Reserve,
            calls(&[&front, &[Reserve(7), FinishPreflight]]),
        ),
        (
            Reserve(7),
            Cause::Busy,
            Stage::Reserve,
            calls(&[&front, &[Reserve(7), FinishPreflight]]),
        ),
    ];
    for (at, cause, stage, log) in cases {
        let state = self::state(&[], &[(at, Fault::Before(cause))]);
        let mut slots = [None];
        let error = failed(run(&state, &[&V1], &mut slots, &mut loan()));
        assert_eq!(error.failure.stage, stage);
        assert_eq!(
            disk(&error.failure.error),
            (RetentionPhase::Prepare, cause, Effect::Unchanged)
        );
        assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
        assert!(!returned(error.after).ended);
        assert_eq!(state.borrow().log, log);
        assert_eq!((state.borrow().row_high, slots), (6, [None]));
        settled(&state);
    }
    // A failed release after a refusal is kept beside it.
    let faults = [
        (Reserve(7), Fault::Before(Cause::Busy)),
        (FinishPreflight, Fault::Before(Cause::Busy)),
    ];
    let state = self::state(&[], &faults);
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan()));
    assert_eq!(disk(&error.failure.error).1, Cause::Busy);
    let (stage, cleanup) = error.failure.cleanup.as_ref().unwrap();
    assert_eq!(*stage, Stage::FinishPreflight);
    assert_eq!(disk(cleanup).1, Cause::Busy);
    assert!(!error.failure.must_resolve);
    returned(error.after);
}

#[test]
fn mca00_d06_failures_after_reserve_cancel_the_one_reservation() {
    let reserved = [
        Check,
        Call::Preflight,
        Call::ValidationLane,
        Validate,
        Reserve(7),
    ];
    let cases = [
        (
            Call::ProducerLane(7),
            Fault::Before(Cause::Busy),
            Stage::ProducerLane,
            vec![Call::ProducerLane(7), Cancel(7)],
        ),
        (
            Forward(7),
            Fault::Producer(InsertError::BudgetMismatch),
            Stage::Produce,
            vec![Call::ProducerLane(7), Forward(7), Cancel(7)],
        ),
        (
            Prepare(7),
            Fault::Before(Cause::Unsupported),
            Stage::Prepare,
            vec![Call::ProducerLane(7), Forward(7), Prepare(7), Cancel(7)],
        ),
        (
            Apply(7),
            Fault::Before(Cause::Busy),
            Stage::Apply,
            vec![
                Call::ProducerLane(7),
                Forward(7),
                Prepare(7),
                Apply(7),
                Reopen(7),
                Cancel(7),
            ],
        ),
    ];
    for (at, fault, stage, tail) in cases {
        let state = self::state(&[], &[(at, fault)]);
        let mut slots = [None];
        let error = failed(run(&state, &[&V1], &mut slots, &mut loan()));
        assert_eq!(error.failure.stage, stage);
        match fault {
            Fault::Producer(expected) => assert_eq!(insert(&error.failure.error), expected),
            Fault::Before(cause) => assert_eq!(
                disk(&error.failure.error),
                (RetentionPhase::Prepare, cause, Effect::Unchanged)
            ),
            _ => unreachable!(),
        }
        assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
        returned(error.after);
        assert_eq!(state.borrow().log, calls(&[&reserved, &tail]));
        assert!(state.borrow().chain.is_empty());
        assert_eq!(slots, [None]);
        settled(&state);
    }
}

#[test]
fn mca00_d07_failed_row_after_applied_rows_rolls_back_to_the_mark() {
    // Row 5 of an earlier statement stays; this statement's mark is 1.
    let state = self::state(&[(5, 22)], &[(Apply(9), Fault::Before(Cause::Busy))]);
    let mut slots = [None, None, None];
    let mut loan = loan();
    let rows: [&[ValueRef<'_>]; 3] = [&V1, &V2, &V3];
    let error = failed(run(&state, &rows, &mut slots, &mut loan));
    assert_eq!(error.failure.stage, Stage::Apply);
    assert_eq!(
        disk(&error.failure.error),
        (RetentionPhase::Prepare, Cause::Busy, Effect::Unchanged)
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    let view = rollback_required(error.after);
    // The slots name the applied rows the caller's rollback compensates.
    assert_eq!(
        slots,
        [Some(LiteralApplied(7)), Some(LiteralApplied(8)), None]
    );
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check],
            &applied(7),
            &applied(8),
            &applied(9),
            &[Reopen(9), Cancel(9)],
        ])
    );
    // Head first, down to the mark: rows 8 and 7, never row 5.
    assert_eq!(roll_back_statement(&state, view, &mut loan), 2);
    let state = state.into_inner();
    assert_eq!(
        state.log,
        calls(&[
            &[RollbackStart(RollbackScope::Statement)],
            &compensated(8),
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement],
        ])
    );
    assert_eq!(
        state.proposals,
        [
            proposal(RowOperation::Forward, 91_634),
            proposal(RowOperation::Forward, 91_630),
            proposal(RowOperation::Forward, 91_633),
            proposal(RowOperation::Clr, 91_422),
            proposal(RowOperation::Clr, 91_426),
        ]
    );
    assert_eq!(state.chain, [(5, 22)]);
    assert_eq!((state.outstanding, state.violations.len()), (None, 0));
}

#[test]
fn mca00_d08_unsupported_c0_wrappers_propagate_unchanged() {
    let unsupported = (
        RetentionPhase::Prepare,
        Cause::Unsupported,
        Effect::Unchanged,
    );
    let finalize = (
        RetentionPhase::Finalize,
        Cause::Unsupported,
        Effect::Unchanged,
    );
    let refuse = Fault::Before(Cause::Unsupported);
    // preflight_row as C0 supplies it.
    let state = self::state(&[], &[(Call::Preflight, refuse)]);
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan()));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Preflight, unsupported)
    );
    returned(error.after);
    assert_eq!(state.borrow().log, [Check, Call::Preflight]);

    // A refused RollbackStart starts nothing: the statement stays
    // RollbackRequired and the start alone is retried.
    let start = RollbackStart(RollbackScope::Statement);
    let state = self::state(&[], &[(Reserve(8), refuse), (start, refuse)]);
    let mut slots = [None, None];
    let mut loan = loan();
    let error = failed(run(&state, &[&V1, &V2], &mut slots, &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Reserve, unsupported)
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    let mut view = rollback_required(error.after);
    assert_eq!(slots, [Some(LiteralApplied(7)), None]);
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let failure = refused(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(
        (failure.stage, disk(&failure.error)),
        (Stage::RollbackStart, unsupported)
    );
    assert!(failure.cleanup.is_none() && !failure.must_resolve);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check],
            &applied(7),
            &[Call::Preflight, Call::ValidationLane, Validate, Reserve(8)],
            &[FinishPreflight, start],
        ])
    );
    // C0's next and finish: an unchanged refusal keeps the same rollback,
    // which is stepped and finished again.
    state.borrow_mut().faults = vec![(Next, refuse)];
    let mut view = withheld(view);
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    let RollbackStop::Next(error) = stopped(&mut rollback) else {
        panic!("expected C's refusal of next");
    };
    assert_eq!(cause(error), unsupported);
    state.borrow_mut().faults = vec![(RollbackFinish, refuse)];
    assert_eq!(complete(&mut rollback), 1);
    let Err(RowOwnedError::Before { error, owned }) = finish_rollback(rollback) else {
        panic!("expected the same rollback back");
    };
    assert_eq!(cause(error), unsupported);
    state.borrow_mut().faults.clear();
    finished(owned);
    finish_statement(&mut call, released(view)).unwrap();
    assert_eq!(
        take_log(&state),
        calls(&[
            &[start, Next],
            &compensated(7),
            &[Next, RollbackFinish, RollbackFinish, FinishStatement],
        ])
    );
    settled(&state);

    // commit_row and the full rollback's start as C0 supplies them.
    let state = self::state(&[(5, 22)], &[(Commit, refuse)]);
    let mut call = LiteralCall { state: &state };
    let error = commit(&mut call, LiteralTransaction(2), &mut loan).unwrap_err();
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Commit, finalize)
    );
    assert_eq!(returned(error.after), LiteralTransaction(2));
    let full = RollbackStart(RollbackScope::Transaction);
    state.borrow_mut().faults = vec![(Commit, Fault::RollbackRequired), (full, refuse)];
    let mut transaction = after_conflict(&state, 3, &mut loan);
    let failure = refused(start_full_rollback(&mut call, &mut transaction, &mut loan));
    assert_eq!(
        (failure.stage, disk(&failure.error)),
        (Stage::RollbackStart, unsupported)
    );
    withheld(transaction);
    assert_eq!(state.borrow().log, [Commit, Commit, full]);
}

/// A fresh literal C state whose preflights answer FlushRequired for the
/// given (RowId, target) victims.
fn flushing(flushes: &[(u64, u64)], faults: &[(Call, Fault)]) -> RefCell<State> {
    let state = state(&[], faults);
    state.borrow_mut().flushes = flushes.to_vec();
    state
}

/// AW1-F01 (A-W1 unit test, not in the contract's table; replaces S2A-T02).
/// C's preflight FlushRequired holds nothing (S2-0 Row write amendment §2):
/// the driver flushes the named victim with the dedicated flusher Owner,
/// continues the flush until the flusher is Ready, and tries the same row
/// again from preflight in the same statement (MCA05-V31 reading). No
/// issuance is used up: the row gets the RowId, input and proposal it gets
/// without a flush (MCA00-D01), and no later preflight sees a flusher that
/// is not Ready (the stand-in records that as a violation).
#[test]
fn aw1_f01_flush_required_flushes_and_retries_the_same_row_from_preflight() {
    let mut loan = loan();
    // The flush completes in try_flush (Complete or NoDirty).
    let state = flushing(&[(7, 9)], &[]);
    let mut slots = [None];
    assert_eq!(run(&state, &[&V1], &mut slots, &mut loan).unwrap(), 1);
    assert_eq!(slots, [Some(LiteralApplied(7))]);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check, Call::Preflight, TryFlush(9)],
            &applied(7),
            &[FinishStatement]
        ])
    );
    {
        let state = state.borrow();
        assert_eq!(state.inputs, [plan(22, R1_SHA)]);
        assert_eq!(state.proposals, [proposal(RowOperation::Forward, 91_506)]);
        assert_eq!(state.chain, [(7, 22)]);
        assert_eq!((state.dirty, state.waiting), (None, 0));
    }
    settled(&state);

    // The flush waits (WaitingWal, WaitingSource, finalize): continue_flush
    // runs until the flusher is Ready, and only then is preflight retried.
    let state = flushing(&[(7, 9)], &[(TryFlush(9), Fault::Steps(3))]);
    assert_eq!(run(&state, &[&V1], &mut [None], &mut loan).unwrap(), 1);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check, Call::Preflight, TryFlush(9)],
            &[ContinueFlush, ContinueFlush, ContinueFlush],
            &applied(7),
            &[FinishStatement]
        ])
    );
    assert_eq!(state.borrow().chain, [(7, 22)]);
    settled(&state);

    // One flush per dirty victim, on any row of the statement.
    let state = flushing(
        &[(7, 9), (7, 10), (8, 11)],
        &[(TryFlush(10), Fault::Steps(1))],
    );
    let mut slots = [None, None];
    assert_eq!(run(&state, &[&V1, &V2], &mut slots, &mut loan).unwrap(), 2);
    assert_eq!(slots, [Some(LiteralApplied(7)), Some(LiteralApplied(8))]);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check, Call::Preflight, TryFlush(9)],
            &[Call::Preflight, TryFlush(10), ContinueFlush],
            &applied(7),
            &[Call::Preflight, TryFlush(11)],
            &applied(8),
            &[FinishStatement]
        ])
    );
    assert_eq!(state.borrow().chain, [(7, 22), (8, 18)]);
    settled(&state);
}

/// AW1-F02 (A-W1 unit test). A refused flush ends the row with nothing held,
/// as any refusal before Reserve: no finish, Cancel or drop record follows,
/// C's error is kept with its phase, cause and effect, and the statement is
/// handed back as the refusal table and the applied rows decide. A refused
/// continue leaves the flusher in the phase C left it in; the caller
/// continues it and the returned statement retries the row.
#[test]
fn aw1_f02_flush_refusals_end_the_row_holding_nothing() {
    let mut loan = loan();
    let busy = (RetentionPhase::Flush, Cause::Busy, Effect::Unchanged);
    let fenced = (
        RetentionPhase::Install,
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    let front = [Check, Call::Preflight, TryFlush(9)];

    let state = flushing(&[(7, 9)], &[(TryFlush(9), Fault::Before(Cause::Busy))]);
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Flush, busy)
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    returned(error.after);
    assert_eq!(take_log(&state), front);
    assert_eq!(state.borrow().row_high, 6);
    settled(&state);

    let state = flushing(&[(7, 9)], &[(TryFlush(9), Fault::Fatal)]);
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Flush, fenced)
    );
    assert!(error.failure.cleanup.is_none() && error.failure.must_resolve);
    assert!(must_resolve(error.after).is_some());
    assert_eq!(take_log(&state), front);
    settled(&state);

    // A refused continue: the flusher still waits, so the driver stops
    // without another preflight. The caller continues the flush, and the
    // statement handed back retries the row with the same RowId.
    let state = flushing(
        &[(7, 9)],
        &[
            (TryFlush(9), Fault::Steps(2)),
            (ContinueFlush, Fault::Before(Cause::Busy)),
        ],
    );
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::ContinueFlush, busy)
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    let view = returned(error.after);
    assert_eq!(take_log(&state), calls(&[&front, &[ContinueFlush]]));
    assert_eq!(state.borrow().waiting, 2);
    state.borrow_mut().faults.clear();
    let mut call = LiteralCall { state: &state };
    while !call.flush_ready() {
        call.continue_flush().unwrap();
    }
    let mut slots = [None];
    let rows: [&[ValueRef<'_>]; 1] = [&V1];
    assert_eq!(
        insert_statement(&mut call, view, &LiteralTable, &mut loan, &rows, &mut slots).unwrap(),
        1
    );
    assert_eq!(slots, [Some(LiteralApplied(7))]);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[ContinueFlush, ContinueFlush, Check],
            &applied(7),
            &[FinishStatement]
        ])
    );
    settled(&state);

    // A refused flush on a later row: the applied row stays, so the
    // statement is RollbackRequired, as after any later row's refusal.
    let state = flushing(&[(8, 11)], &[(TryFlush(11), Fault::Before(Cause::Busy))]);
    let mut slots = [None, None];
    let error = failed(run(&state, &[&V1, &V2], &mut slots, &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Flush, busy)
    );
    rollback_required(error.after);
    assert_eq!(slots, [Some(LiteralApplied(7)), None]);
    assert_eq!(
        take_log(&state),
        calls(&[&[Check], &applied(7), &[Call::Preflight, TryFlush(11)]])
    );
    settled(&state);
}

#[test]
fn mca00_d09_fatal_and_unreleased_reservations_stop_all_calls() {
    let front = [Check, Call::Preflight, Call::ValidationLane, Validate];
    let fenced = (
        RetentionPhase::Install,
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    let cases = [
        (
            vec![(Apply(7), Fault::Fatal)],
            Stage::Apply,
            None,
            calls(&[&[Check], &applied(7)]),
        ),
        (
            vec![(Reserve(7), Fault::Fatal)],
            Stage::Reserve,
            None,
            calls(&[&front, &[Reserve(7)]]),
        ),
        (
            vec![(Call::Preflight, Fault::Fatal)],
            Stage::Preflight,
            None,
            vec![Check, Call::Preflight],
        ),
        // A CallerMustResolve validation lane: no finish call; the preflight
        // is dropped, C's documented no-effect release.
        (
            vec![(Call::ValidationLane, Fault::Fatal)],
            Stage::ValidationLane,
            None,
            vec![
                Check,
                Call::Preflight,
                Call::ValidationLane,
                Call::DropPreflight,
            ],
        ),
        // A CallerMustResolve producer lane: no Cancel effect after it.
        (
            vec![(Call::ProducerLane(7), Fault::Fatal)],
            Stage::ProducerLane,
            None,
            calls(&[
                &front,
                &[Reserve(7), Call::ProducerLane(7), DropReserved(7)],
            ]),
        ),
        // A Cancel C refuses: the reservation goes to C's fence.
        (
            vec![
                (Prepare(7), Fault::Before(Cause::Busy)),
                (Cancel(7), Fault::Before(Cause::Busy)),
            ],
            Stage::Prepare,
            Some(Stage::Cancel),
            calls(&[
                &front,
                &[Reserve(7), Call::ProducerLane(7), Forward(7), Prepare(7)],
                &[Cancel(7), DropReserved(7)],
            ]),
        ),
    ];
    for (faults, stage, cleanup, log) in cases {
        let state = self::state(&[], &faults);
        let mut slots = [None];
        let error = failed(run(&state, &[&V1], &mut slots, &mut loan()));
        assert_eq!(error.failure.stage, stage);
        assert_eq!(
            error.failure.cleanup.as_ref().map(|(stage, _)| *stage),
            cleanup
        );
        assert!(error.failure.must_resolve);
        assert!(must_resolve(error.after).is_some());
        assert_eq!(state.borrow().log, log);
        assert_eq!(slots, [None]);
    }
    // With an applied row: still no Cancel, rollback or finish.
    let state = self::state(&[], &[(Apply(8), Fault::Fatal)]);
    let mut slots = [None, None];
    let error = failed(run(&state, &[&V1, &V2], &mut slots, &mut loan()));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Apply, fenced)
    );
    assert!(must_resolve(error.after).is_some());
    assert_eq!(slots, [Some(LiteralApplied(7)), None]);
    assert_eq!(
        state.borrow().log,
        calls(&[&[Check], &applied(7), &applied(8)])
    );

    // A started rollback C fences: no call after the answer, and the driver
    // drops neither the rollback nor anything C consumed.
    let start = RollbackStart(RollbackScope::Statement);
    let cases = [
        (Next, Stage::RollbackNext, vec![start, Next]),
        (ClrLane(7), Stage::ClrLane, vec![start, Next, ClrLane(7)]),
        (
            ClrPrepare(7),
            Stage::ClrPrepare,
            vec![start, Next, ClrLane(7), Clr(7), ClrPrepare(7)],
        ),
        (
            ClrApply(7),
            Stage::ClrApply,
            calls(&[&[start], &compensated(7)]),
        ),
    ];
    for (at, stage, log) in cases {
        let state = self::state(&[], &[]);
        let mut loan = loan();
        let mut view = after_failed_row(&state, &mut loan);
        state.borrow_mut().faults = vec![(at, Fault::Fatal)];
        let mut call = LiteralCall { state: &state };
        let transaction = LiteralTransaction(1);
        let mut rollback = started(start_statement_rollback(
            &mut call,
            &transaction,
            &mut view,
            &mut loan,
        ));
        let RollbackStop::MustResolve { stage: at, error } = stopped(&mut rollback) else {
            panic!("expected MustResolve");
        };
        assert_eq!((at, disk(&error)), (stage, fenced));
        assert_eq!(take_log(&state), log);
    }
    // A Fatal finish: C keeps the rollback; nothing is dropped or called.
    let state = self::state(&[], &[]);
    let mut loan = loan();
    let mut view = after_failed_row(&state, &mut loan);
    state.borrow_mut().faults = vec![(RollbackFinish, Fault::Fatal)];
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(complete(&mut rollback), 1);
    let Err(RowOwnedError::Fatal(error)) = finish_rollback(rollback) else {
        panic!("expected a Fatal finish");
    };
    assert_eq!(cause(error), fenced);
    assert_eq!(
        take_log(&state),
        calls(&[&[start], &compensated(7), &[Next, RollbackFinish]])
    );
}

#[test]
fn mca00_d10_rollback_stops_hand_back_the_same_rollback_or_task() {
    let start = RollbackStart(RollbackScope::Statement);
    let busy = (RetentionPhase::Prepare, Cause::Busy, Effect::Unchanged);
    let clr7 = [ClrLane(7), Clr(7), ClrPrepare(7), ClrApply(7)];
    // A task refused unchanged comes back as the same task; only it is
    // retried, and the rollback is never started again.
    let cases = [
        (
            ClrLane(7),
            Fault::Before(Cause::Busy),
            Stage::ClrLane,
            vec![start, Next, ClrLane(7)],
        ),
        (
            Clr(7),
            Fault::Producer(InsertError::BudgetMismatch),
            Stage::Clr,
            vec![start, Next, ClrLane(7), Clr(7)],
        ),
        (
            ClrPrepare(7),
            Fault::Before(Cause::Busy),
            Stage::ClrPrepare,
            vec![start, Next, ClrLane(7), Clr(7), ClrPrepare(7)],
        ),
        (
            ClrApply(7),
            Fault::Before(Cause::Busy),
            Stage::ClrApply,
            calls(&[&[start], &compensated(7), &[ClrReopen(7)]]),
        ),
    ];
    for (at, fault, stage, log) in cases {
        let state = self::state(&[], &[]);
        let mut loan = loan();
        let mut view = after_failed_row(&state, &mut loan);
        state.borrow_mut().faults = vec![(at, fault)];
        let mut call = LiteralCall { state: &state };
        let transaction = LiteralTransaction(1);
        let mut rollback = started(start_statement_rollback(
            &mut call,
            &transaction,
            &mut view,
            &mut loan,
        ));
        let (held, error, task) = held_task(stopped(&mut rollback));
        assert_eq!(held, stage);
        match fault {
            Fault::Producer(expected) => assert_eq!(insert(&error), expected),
            _ => assert_eq!(disk(&error), busy),
        }
        assert_eq!(take_log(&state), log);
        state.borrow_mut().faults.clear();
        assert!(retry_task(task).is_ok());
        assert_eq!(complete(&mut rollback), 0);
        finished(rollback);
        finish_statement(&mut call, released(view)).unwrap();
        assert_eq!(
            take_log(&state),
            calls(&[&clr7, &[Next, RollbackFinish, FinishStatement]])
        );
        assert!(state.borrow().chain.is_empty());
        settled(&state);
    }

    // C's unchanged refusal of next keeps the same rollback: step it again.
    let state = self::state(&[], &[]);
    let mut loan = loan();
    let mut view = after_failed_row(&state, &mut loan);
    state.borrow_mut().faults = vec![(Next, Fault::Before(Cause::Busy))];
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    let RollbackStop::Next(error) = stopped(&mut rollback) else {
        panic!("expected C's refusal of next");
    };
    assert_eq!(cause(error), busy);
    state.borrow_mut().faults.clear();
    assert_eq!(complete(&mut rollback), 1);
    finished(rollback);
    finish_statement(&mut call, released(view)).unwrap();
    assert_eq!(
        take_log(&state),
        calls(&[
            &[start, Next],
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement]
        ])
    );
    settled(&state);

    // An unchanged refusal of finish returns the same rollback.
    let state = self::state(&[], &[]);
    let mut view = after_failed_row(&state, &mut loan);
    state.borrow_mut().faults = vec![(RollbackFinish, Fault::Before(Cause::Busy))];
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(complete(&mut rollback), 1);
    let Err(RowOwnedError::Before { error, owned }) = finish_rollback(rollback) else {
        panic!("expected the same rollback back");
    };
    assert_eq!(cause(error), busy);
    state.borrow_mut().faults.clear();
    finished(owned);
    finish_statement(&mut call, released(view)).unwrap();
    assert_eq!(
        take_log(&state),
        calls(&[
            &[start],
            &compensated(7),
            &[Next, RollbackFinish, RollbackFinish, FinishStatement]
        ])
    );
    settled(&state);

    // FlushRequired: C's target is carried out with the rollback still the
    // caller's; nothing is dropped or called (G1: no flush-through while the
    // rollback holds the Owner; G5: no defined continuation).
    let state = self::state(&[], &[]);
    let mut view = after_failed_row(&state, &mut loan);
    state.borrow_mut().faults = vec![(Next, Fault::Flush(0x4d2))];
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    let RollbackStop::Flush(target) = stopped(&mut rollback) else {
        panic!("expected C's flush target");
    };
    assert_eq!(target, 0x4d2);
    assert_eq!(take_log(&state), [start, Next]);
}

#[test]
fn mca00_d11_commit_and_full_abort_close_the_transaction() {
    let mut loan = loan();
    let full = RollbackStart(RollbackScope::Transaction);
    let state = self::state(&[(5, 22)], &[]);
    let committed = commit(
        &mut LiteralCall { state: &state },
        LiteralTransaction(1),
        &mut loan,
    );
    assert_eq!(committed.unwrap(), 42);
    assert_eq!(state.borrow().log, [Commit]);

    // A write conflict hands the transaction back for A's full rollback
    // only; the caller runs it and finish_aborted closes the transaction.
    let state = self::state(&[(5, 22), (7, 18)], &[(Commit, Fault::RollbackRequired)]);
    let mut call = LiteralCall { state: &state };
    let error = commit(&mut call, LiteralTransaction(1), &mut loan).unwrap_err();
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (
            Stage::Commit,
            (
                RetentionPhase::Finalize,
                Cause::SerializationConflict,
                Effect::Unchanged
            )
        )
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    let mut pending = rollback_required(error.after);
    let mut rollback = started(start_full_rollback(&mut call, &mut pending, &mut loan));
    assert_eq!(complete(&mut rollback), 2);
    finished(rollback);
    let transaction = released(pending);
    assert_eq!(transaction, LiteralTransaction(1));
    finish_aborted(&mut call, transaction).unwrap();
    let state = state.into_inner();
    assert_eq!(
        state.log,
        calls(&[
            &[Commit, full],
            &compensated(7),
            &compensated(5),
            &[Next, RollbackFinish, FinishAborted],
        ])
    );
    assert_eq!(
        state.proposals,
        [
            proposal(RowOperation::Clr, 91_422),
            proposal(RowOperation::Clr, 91_298),
        ]
    );
    assert!(state.chain.is_empty() && state.violations.is_empty());

    // Rejected closes, Fatal must be resolved; neither calls anything else.
    for (fault, rejected) in [(Fault::Rejected, true), (Fault::Fatal, false)] {
        let state = self::state(&[(5, 22)], &[(Commit, fault)]);
        let error = commit(
            &mut LiteralCall { state: &state },
            LiteralTransaction(1),
            &mut loan,
        )
        .unwrap_err();
        assert_eq!(error.failure.must_resolve, !rejected);
        if rejected {
            closed(error.after);
        } else {
            assert!(must_resolve(error.after).is_none());
        }
        assert_eq!(state.borrow().log, [Commit]);
    }

    // A full rollback stopping partway: after row 7's CLR, row 5's apply is
    // refused; the same task is retried and nothing restarts.
    let busy = (RetentionPhase::Prepare, Cause::Busy, Effect::Unchanged);
    let state = self::state(
        &[(5, 22), (7, 18)],
        &[
            (Commit, Fault::RollbackRequired),
            (ClrApply(5), Fault::Before(Cause::Busy)),
        ],
    );
    let mut transaction = after_conflict(&state, 4, &mut loan);
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_full_rollback(&mut call, &mut transaction, &mut loan));
    assert_eq!(
        rollback_step(&mut rollback).ok(),
        Some(Stepped::Compensated)
    );
    let (stage, error, task) = held_task(stopped(&mut rollback));
    assert_eq!((stage, disk(&error)), (Stage::ClrApply, busy));
    state.borrow_mut().faults.clear();
    assert!(retry_task(task).is_ok());
    assert_eq!(complete(&mut rollback), 0);
    finished(rollback);
    finish_aborted(&mut call, released(transaction)).unwrap();
    assert_eq!(
        state.borrow().log,
        calls(&[
            &[Commit, full],
            &compensated(7),
            &compensated(5),
            &[ClrReopen(5), ClrLane(5), Clr(5), ClrPrepare(5), ClrApply(5)],
            &[Next, RollbackFinish, FinishAborted],
        ])
    );
    assert!(state.borrow().chain.is_empty());
    settled(&state);
}

/// Machine stack below the test frame down to the producer stand-in, for the
/// INSERT path and for the CLR path of the caller's statement rollback. It
/// covers the stand-in instantiation in this unoptimized test build only.
fn measured() -> (usize, usize) {
    let state = self::state(&[], &[]);
    let top = probe();
    run(&state, &[&V1], &mut [None], &mut loan()).unwrap();
    let insert = top - state.borrow().deepest;
    let state = self::state(&[], &[]);
    let mut loan = loan();
    let view = after_failed_row(&state, &mut loan);
    state.borrow_mut().deepest = usize::MAX;
    let top = probe();
    assert_eq!(roll_back_statement(&state, view, &mut loan), 1);
    let clr = top - state.borrow().deepest;
    (insert, clr)
}

/// The driver's source inventory, for the live C0 types and the stand-ins,
/// is within its bound, and the bound with A's deeper producer inventory is
/// within the 65536 B short-source cap. This is an inventory check, not a
/// stack proof. The machine depth of the stand-in run is printed as an
/// observation only: unoptimized frames keep a slot for every temporary, so
/// it is not comparable with a source bound (r1 FAIL record).
#[test]
fn mca00_d12_driver_frames_stay_within_the_driver_source_bound() {
    const SHORT_SOURCE_CAP: usize = 65_536;
    let (live_insert, live_clr) =
        live::live_carrier_bytes::<platform::namespace::LinuxWriterLease>();
    let (stand_insert, stand_clr) = carrier_bytes::<LiteralCall<'static>>();
    let (validation, producer) = super::super::local_source_carrier_bytes();
    let (machine_insert, machine_clr) = measured();
    let deepest_a = validation.max(producer);
    println!(
        "MCA00_D12 bound={DRIVER_SOURCE_BYTES} cap={SHORT_SOURCE_CAP} live_insert={live_insert} \
         live_clr={live_clr} stand_insert={stand_insert} stand_clr={stand_clr} \
         a_validation={validation} a_producer={producer} \
         live_plus_a={} bound_plus_a={} machine_debug_insert={machine_insert} \
         machine_debug_clr={machine_clr}",
        live_clr.max(live_insert) + deepest_a,
        DRIVER_SOURCE_BYTES + deepest_a,
    );
    for bytes in [live_insert, live_clr, stand_insert, stand_clr] {
        assert!(bytes <= DRIVER_SOURCE_BYTES, "{bytes}");
    }
    // A's validation and producer are never on the stack together.
    assert!(DRIVER_SOURCE_BYTES + deepest_a <= SHORT_SOURCE_CAP);
    // Both paths reached the producer stand-in.
    assert!(machine_insert > 0 && machine_clr > 0);
}

#[test]
fn mca00_d13_a_finished_rollback_leaves_only_the_finish_to_retry() {
    let busy = (RetentionPhase::Census, Cause::Busy, Effect::Unchanged);
    let fenced = (
        RetentionPhase::Install,
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    let start = RollbackStart(RollbackScope::Statement);
    // A refused finish after the statement rollback finished: FinishPending,
    // and the retry is finish_statement alone (one RollbackStart in all).
    let state = self::state(&[], &[]);
    let mut loan = loan();
    let mut view = after_failed_row(&state, &mut loan);
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(complete(&mut rollback), 1);
    finished(rollback);
    state.borrow_mut().faults = vec![(FinishStatement, Fault::Before(Cause::Busy))];
    let error = finish_statement(&mut call, released(view)).unwrap_err();
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::FinishStatement, busy)
    );
    let view = finish_pending(error.after);
    state.borrow_mut().faults.clear();
    finish_statement(&mut call, view).unwrap();
    assert_eq!(
        take_log(&state),
        calls(&[
            &[start],
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement, FinishStatement],
        ])
    );

    // The same after a fully applied INSERT; its row stays.
    let state = self::state(&[], &[(FinishStatement, Fault::Before(Cause::Busy))]);
    let mut slots = [None];
    let error = failed(run(&state, &[&V1], &mut slots, &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::FinishStatement, busy)
    );
    let view = finish_pending(error.after);
    state.borrow_mut().faults.clear();
    finish_statement(&mut LiteralCall { state: &state }, view).unwrap();
    assert_eq!(slots, [Some(LiteralApplied(7))]);
    assert_eq!(
        state.borrow().log,
        calls(&[&[Check], &applied(7), &[FinishStatement, FinishStatement]])
    );
    assert_eq!(state.borrow().chain, [(7, 22)]);

    // A Fatal finish leaves nothing to retry, with or without a rollback.
    let state = self::state(&[], &[(FinishStatement, Fault::Fatal)]);
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan));
    assert!(error.failure.must_resolve);
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::FinishStatement, fenced)
    );
    assert!(must_resolve(error.after).is_none());
    let state = self::state(&[], &[]);
    let mut view = after_failed_row(&state, &mut loan);
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(complete(&mut rollback), 1);
    finished(rollback);
    state.borrow_mut().faults = vec![(FinishStatement, Fault::Fatal)];
    let error = finish_statement(&mut call, released(view)).unwrap_err();
    assert!(error.failure.must_resolve);
    assert!(must_resolve(error.after).is_none());
    assert_eq!(
        take_log(&state),
        calls(&[
            &[start],
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement]
        ])
    );

    // finish_aborted refused after the full rollback finished: only it is
    // retried.
    let full = RollbackStart(RollbackScope::Transaction);
    let state = self::state(
        &[(5, 22)],
        &[
            (Commit, Fault::RollbackRequired),
            (FinishAborted, Fault::Before(Cause::Unsupported)),
        ],
    );
    let mut pending = after_conflict(&state, 5, &mut loan);
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_full_rollback(&mut call, &mut pending, &mut loan));
    assert_eq!(complete(&mut rollback), 1);
    finished(rollback);
    let error = finish_aborted(&mut call, released(pending)).unwrap_err();
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (
            Stage::FinishAborted,
            (
                RetentionPhase::Finalize,
                Cause::Unsupported,
                Effect::Unchanged
            )
        )
    );
    let transaction = finish_pending(error.after);
    assert_eq!(transaction, LiteralTransaction(5));
    state.borrow_mut().faults.clear();
    finish_aborted(&mut call, transaction).unwrap();
    assert_eq!(
        state.borrow().log,
        calls(&[
            &[Commit, full],
            &compensated(5),
            &[Next, RollbackFinish, FinishAborted, FinishAborted]
        ])
    );
    settled(&state);

    // C answering RollbackRequired to finish_aborted after the full rollback
    // finished has no defined continuation (G5): the transaction is kept for
    // C's fence and no second RollbackStart follows.
    let state = self::state(
        &[(5, 22)],
        &[
            (Commit, Fault::RollbackRequired),
            (FinishAborted, Fault::RollbackRequired),
        ],
    );
    let mut pending = after_conflict(&state, 6, &mut loan);
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_full_rollback(&mut call, &mut pending, &mut loan));
    assert_eq!(complete(&mut rollback), 1);
    finished(rollback);
    let error = finish_aborted(&mut call, released(pending)).unwrap_err();
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (
            Stage::FinishAborted,
            (
                RetentionPhase::Finalize,
                Cause::SerializationConflict,
                Effect::Unchanged
            )
        )
    );
    assert!(error.failure.must_resolve && error.failure.cleanup.is_none());
    assert_eq!(must_resolve(error.after), Some(LiteralTransaction(6)));
    assert_eq!(
        state.borrow().log,
        calls(&[
            &[Commit, full],
            &compensated(5),
            &[Next, RollbackFinish, FinishAborted]
        ])
    );
}

/// A `RollbackRequired` handle leaves its `PendingRollback` only after the
/// rollback started from that pending finished, so applied rows never reach
/// `finish_statement` (`finish_aborted`) as a success. The type-level part is
/// that the fields are private to the driver, nothing outside it builds a
/// pending, and `rolled_back` is its only way out; this test covers every
/// reachable path: not before a start, not after an abandoned or fenced
/// rollback, and a second start is refused without calling C.
#[test]
fn mca00_d14_a_rollback_required_handle_is_released_only_by_its_finished_rollback() {
    let start = RollbackStart(RollbackScope::Statement);
    let second = |failure: Failure| {
        assert_eq!(failure.stage, Stage::RollbackStart);
        assert_eq!(insert(&failure.error), InsertError::OrderMismatch);
        assert!(failure.cleanup.is_none());
        failure.must_resolve
    };
    // Abandoned after a stop: the caller dropped the started rollback. The
    // handle stays inside, and a restart is refused as must-resolve.
    let state = self::state(&[], &[(Next, Fault::Before(Cause::Busy))]);
    let mut loan = loan();
    let mut view = withheld(after_failed_row(&state, &mut loan));
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(stop_name(&stopped(&mut rollback)), "Next");
    drop(rollback);
    let mut view = withheld(view);
    let failure = refused(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert!(second(failure));
    withheld(view);
    assert_eq!(take_log(&state), [start, Next, DropRollback]);

    // A Fatal rollback finish: the handle stays inside for C's fence.
    let state = self::state(&[], &[(RollbackFinish, Fault::Fatal)]);
    let mut view = after_failed_row(&state, &mut loan);
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(complete(&mut rollback), 1);
    assert!(matches!(
        finish_rollback(rollback),
        Err(RowOwnedError::Fatal(_))
    ));
    let mut view = withheld(view);
    assert!(second(refused(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ))));
    withheld(view);
    assert_eq!(
        take_log(&state),
        calls(&[&[start], &compensated(7), &[Next, RollbackFinish]])
    );

    // Finished: a second start is still refused, now without must-resolve,
    // and the released statement is finished.
    let state = self::state(&[], &[]);
    let mut view = after_failed_row(&state, &mut loan);
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ));
    assert_eq!(complete(&mut rollback), 1);
    finished(rollback);
    assert!(!second(refused(start_statement_rollback(
        &mut call,
        &transaction,
        &mut view,
        &mut loan,
    ))));
    finish_statement(&mut call, released(view)).unwrap();
    assert_eq!(
        take_log(&state),
        calls(&[
            &[start],
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement]
        ])
    );
    settled(&state);

    // The transaction of a conflicting commit: withheld before its full
    // rollback, and after an abandoned one with no restart.
    let full = RollbackStart(RollbackScope::Transaction);
    let state = self::state(
        &[(5, 22), (7, 18)],
        &[
            (Commit, Fault::RollbackRequired),
            (ClrApply(5), Fault::Before(Cause::Busy)),
        ],
    );
    let mut pending = withheld(after_conflict(&state, 2, &mut loan));
    let mut call = LiteralCall { state: &state };
    let mut rollback = started(start_full_rollback(&mut call, &mut pending, &mut loan));
    assert_eq!(
        rollback_step(&mut rollback).ok(),
        Some(Stepped::Compensated)
    );
    assert_eq!(stop_name(&stopped(&mut rollback)), "Task");
    drop(rollback);
    let mut pending = withheld(pending);
    assert!(second(refused(start_full_rollback(
        &mut call,
        &mut pending,
        &mut loan
    ))));
    withheld(pending);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Commit, full],
            &compensated(7),
            &compensated(5),
            &[ClrReopen(5), DropRollback],
        ])
    );
}
