//! MCA00-D01..12 are C0_ONLY. The driver runs over literal stand-ins of C's
//! Row write boundary: a non-persistent call log, a row high-water, the
//! actor's chain of applied rows, the statement mark and scripted
//! refusals. Refusals are C0's own value: `Unsupported` or another choice
//! with `Unchanged`, or a `CallerMustResolve` fatal. No PreflightRow,
//! ReservedRow, RowRollback, lane, pin, WAL, receipt, ACK or READY exists
//! here, and nothing counts toward MCA01–05. The validation stand-in runs
//! A's pure `encode_input` into the loan's candidate area. The producer
//! stand-in returns literal proposals, because A's producer needs C's lane
//! (it is covered by MCA00-V06). Expected bytes, digests and frame lengths
//! are independent literals from the contract: canonical rows and SHA-256
//! by `n2-tools/make_literals.py` (stdlib struct/hashlib), Forward
//! `91304+C+128h` and CLR `91096+C+128h` with `C=180+L`.
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
    Apply, Cancel, Check, Clr, ClrApply, ClrLane, ClrPrepare, ClrReopen, Commit, DropReserved,
    DropRollback, FinishAborted, FinishPreflight, FinishStatement, Forward, Next, Prepare, Reopen,
    Reserve, RollbackFinish, RollbackStart, Validate,
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
    Flush,
    RollbackRequired,
    Rejected,
}

/// The C0 wrappers' phases: Prepare for the Row write/rollback steps,
/// Finalize for commit_row/finish_aborted, Census for statement checks.
fn refusal(call: Call, cause: Cause) -> RetentionError {
    let phase = match call {
        Commit | FinishAborted => RetentionPhase::Finalize,
        Check | FinishStatement => RetentionPhase::Census,
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
    ) -> Result<LiteralPreflight<'r, 'i>, RetentionError> {
        let fault = self.state.borrow_mut().note(Call::Preflight);
        factory(Call::Preflight, fault)?;
        Ok(LiteralPreflight {
            state: self.state,
            loan: Some(loan),
            done: false,
        })
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
            Some(Fault::Fatal) => Err(TransactionFinishError::Fatal(fatal())),
            Some(other) => panic!("unscripted {other:?}"),
        }
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
    type Task<'x>
        = LiteralTask<'x, 'r, 'i>
    where
        Self: 'x;
    fn next(&mut self) -> Result<RollbackNext<LiteralTask<'_, 'r, 'i>>, RetentionError> {
        let mut state = self.state.borrow_mut();
        match state.note(Next) {
            Some(Fault::Flush) => return Ok(RollbackNext::Flush),
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

fn run<'s>(
    state: &'s RefCell<State>,
    rows: &[&[ValueRef<'_>]],
    slots: &mut [Option<LiteralApplied>],
    loan: &mut Loan,
) -> Outcome<'s> {
    insert_statement(
        &mut LiteralCall { state },
        &LiteralTransaction(1),
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

fn returned<H>(after: After<H>) -> H {
    match after {
        After::Returned(handle) => handle,
        After::Closed => panic!("expected Returned, got Closed"),
        After::Open(_) => panic!("expected Returned, got Open"),
        After::MustResolve(_) => panic!("expected Returned, got MustResolve"),
    }
}

fn open<H>(after: After<H>) -> H {
    match after {
        After::Open(handle) => handle,
        After::Closed => panic!("expected Open, got Closed"),
        After::Returned(_) => panic!("expected Open, got Returned"),
        After::MustResolve(_) => panic!("expected Open, got MustResolve"),
    }
}

fn must_resolve<H>(after: After<H>) -> Option<H> {
    match after {
        After::MustResolve(handle) => handle,
        After::Closed => panic!("expected MustResolve, got Closed"),
        After::Returned(_) => panic!("expected MustResolve, got Returned"),
        After::Open(_) => panic!("expected MustResolve, got Open"),
    }
}

/// No reservation or rollback is left behind and no rule was broken.
fn settled(state: &RefCell<State>) {
    let state = state.borrow();
    assert_eq!(state.outstanding, None);
    assert!(state.violations.is_empty(), "{:?}", state.violations);
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
        &LiteralTransaction(1),
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
        assert_eq!(error.rolled_back, 0);
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

    // A rejection in row 2 compensates row 1 and finishes the statement.
    let state = self::state(&[], &[]);
    let mut slots = [None, None];
    let error = failed(run(&state, &[&V1, &BAD], &mut slots, &mut loan()));
    assert_eq!(error.failure.stage, Stage::Validate);
    assert_eq!(
        insert(&error.failure.error),
        InsertError::Row(RowError::SchemaMismatch)
    );
    assert_eq!(error.rolled_back, 1);
    assert!(matches!(error.after, After::Closed));
    assert_eq!(slots, [None, None]);
    let state = state.into_inner();
    assert_eq!(
        state.log,
        calls(&[
            &[Check],
            &applied(7),
            &[
                Call::Preflight,
                Call::ValidationLane,
                Validate,
                FinishPreflight
            ],
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
        assert_eq!(error.rolled_back, 0);
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
    let rows: [&[ValueRef<'_>]; 3] = [&V1, &V2, &V3];
    let error = failed(run(&state, &rows, &mut slots, &mut loan()));
    assert_eq!(error.failure.stage, Stage::Apply);
    assert_eq!(
        disk(&error.failure.error),
        (RetentionPhase::Prepare, Cause::Busy, Effect::Unchanged)
    );
    assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
    assert_eq!(error.rolled_back, 2);
    assert!(matches!(error.after, After::Closed));
    assert_eq!(slots, [None, None, None]);
    let state = state.into_inner();
    assert_eq!(
        state.log,
        calls(&[
            &[Check],
            &applied(7),
            &applied(8),
            &applied(9),
            &[
                Reopen(9),
                Cancel(9),
                RollbackStart(RollbackScope::Statement)
            ],
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
    // preflight_row as C0 supplies it.
    let state = self::state(&[], &[(Call::Preflight, Fault::Before(Cause::Unsupported))]);
    let error = failed(run(&state, &[&V1], &mut [None], &mut loan()));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Preflight, unsupported)
    );
    returned(error.after);
    assert_eq!(state.borrow().log, [Check, Call::Preflight]);

    // A refused rollback leaves this call's row applied: Open, then retry.
    let faults = [
        (Reserve(8), Fault::Before(Cause::Unsupported)),
        (
            RollbackStart(RollbackScope::Statement),
            Fault::Before(Cause::Unsupported),
        ),
    ];
    let state = self::state(&[], &faults);
    let mut slots = [None, None];
    let mut loan = loan();
    let error = failed(run(&state, &[&V1, &V2], &mut slots, &mut loan));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Reserve, unsupported)
    );
    let (stage, cleanup) = error.failure.cleanup.as_ref().unwrap();
    assert_eq!((*stage, disk(cleanup)), (Stage::RollbackStart, unsupported));
    assert!(!error.failure.must_resolve);
    assert_eq!(error.rolled_back, 0);
    let view = open(error.after);
    assert_eq!(slots, [Some(LiteralApplied(7)), None]);
    assert_eq!(
        std::mem::take(&mut state.borrow_mut().log),
        calls(&[
            &[Check],
            &applied(7),
            &[Call::Preflight, Call::ValidationLane, Validate, Reserve(8)],
            &[FinishPreflight, RollbackStart(RollbackScope::Statement)],
        ])
    );
    state.borrow_mut().faults.clear();
    let retried = rollback_statement(
        &mut LiteralCall { state: &state },
        &LiteralTransaction(1),
        view,
        &mut loan,
    );
    assert_eq!(retried.unwrap(), 1);
    assert_eq!(
        state.borrow().log,
        calls(&[
            &[RollbackStart(RollbackScope::Statement)],
            &compensated(7),
            &[Next, RollbackFinish, FinishStatement],
        ])
    );
    settled(&state);

    // commit_row and a full abort's start as C0 supplies them.
    let state = self::state(&[(5, 22)], &[(Commit, Fault::Before(Cause::Unsupported))]);
    let error = commit(
        &mut LiteralCall { state: &state },
        LiteralTransaction(2),
        &mut loan,
    )
    .unwrap_err();
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::Commit, finalize)
    );
    assert_eq!(returned(error.after), LiteralTransaction(2));
    assert_eq!(state.borrow().log, [Commit]);
    let start = RollbackStart(RollbackScope::Transaction);
    let state = self::state(&[(5, 22)], &[(start, Fault::Before(Cause::Unsupported))]);
    let error = failed(abort(
        &mut LiteralCall { state: &state },
        LiteralTransaction(3),
        &mut loan,
    ));
    assert_eq!(
        (error.failure.stage, disk(&error.failure.error)),
        (Stage::RollbackStart, unsupported)
    );
    assert_eq!(returned(error.after), LiteralTransaction(3));
    assert_eq!(state.borrow().log, [start]);
}

#[test]
fn mca00_d09_fatal_and_unreleased_reservations_stop_all_calls() {
    let front = [Check, Call::Preflight, Call::ValidationLane, Validate];
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
        // A CallerMustResolve lane answer: no Cancel effect after it.
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
        assert_eq!(error.rolled_back, 0);
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
        (
            Stage::Apply,
            (
                RetentionPhase::Install,
                Cause::Fenced,
                Effect::CallerMustResolve
            )
        )
    );
    assert!(must_resolve(error.after).is_some());
    assert_eq!(slots, [Some(LiteralApplied(7)), None]);
    assert_eq!(
        state.borrow().log,
        calls(&[&[Check], &applied(7), &applied(8)])
    );
}

#[test]
fn mca00_d10_rollback_stops_without_retry_and_returns_the_statement() {
    let failed_row = [
        Call::Preflight,
        Call::ValidationLane,
        Validate,
        FinishPreflight,
    ];
    let start = RollbackStart(RollbackScope::Statement);
    let cases = [
        (
            (Next, Fault::Flush),
            Stage::RollbackNext,
            0,
            vec![start, Next, DropRollback],
        ),
        (
            (Clr(7), Fault::Producer(InsertError::BudgetMismatch)),
            Stage::Clr,
            0,
            vec![start, Next, ClrLane(7), Clr(7), DropRollback],
        ),
        (
            (ClrApply(7), Fault::Before(Cause::Busy)),
            Stage::ClrApply,
            0,
            calls(&[&[start], &compensated(7), &[ClrReopen(7), DropRollback]]),
        ),
        (
            (RollbackFinish, Fault::Before(Cause::Busy)),
            Stage::RollbackFinish,
            1,
            calls(&[
                &[start],
                &compensated(7),
                &[Next, RollbackFinish, DropRollback],
            ]),
        ),
        (
            (FinishStatement, Fault::Before(Cause::Busy)),
            Stage::FinishStatement,
            1,
            calls(&[
                &[start],
                &compensated(7),
                &[Next, RollbackFinish, FinishStatement],
            ]),
        ),
    ];
    for ((at, fault), stage, rolled_back, tail) in cases {
        let state = self::state(&[], &[(at, fault)]);
        let mut slots = [None, None];
        let mut loan = loan();
        let error = failed(run(&state, &[&V1, &BAD], &mut slots, &mut loan));
        assert_eq!(error.failure.stage, Stage::Validate);
        let (cleanup, cleanup_error) = error.failure.cleanup.as_ref().unwrap();
        assert_eq!(*cleanup, stage);
        match fault {
            Fault::Flush => assert!(matches!(
                cleanup_error,
                DriverError::Gap(Gap::RollbackFlush)
            )),
            Fault::Producer(expected) => assert_eq!(insert(cleanup_error), expected),
            _ => assert_eq!(disk(cleanup_error).1, Cause::Busy),
        }
        assert!(!error.failure.must_resolve);
        assert_eq!(error.rolled_back, rolled_back);
        let view = open(error.after);
        assert_eq!(slots, [Some(LiteralApplied(7)), None]);
        let log = std::mem::take(&mut state.borrow_mut().log);
        assert_eq!(log, calls(&[&[Check], &applied(7), &failed_row, &tail]));
        // The caller's retry with the returned statement.
        state.borrow_mut().faults.clear();
        let retried = rollback_statement(
            &mut LiteralCall { state: &state },
            &LiteralTransaction(1),
            view,
            &mut loan,
        );
        assert_eq!(retried.unwrap(), 1 - rolled_back);
        let compensations: &[Call] = if rolled_back == 0 {
            &compensated(7)
        } else {
            &[]
        };
        assert_eq!(
            state.borrow().log,
            calls(&[
                &[start],
                compensations,
                &[Next, RollbackFinish, FinishStatement]
            ])
        );
        assert!(state.borrow().chain.is_empty());
        settled(&state);
    }
    // A refused finish after a successful statement keeps its rows.
    let state = self::state(&[], &[(FinishStatement, Fault::Before(Cause::Busy))]);
    let mut slots = [None];
    let error = failed(run(&state, &[&V1], &mut slots, &mut loan()));
    assert_eq!(error.failure.stage, Stage::FinishStatement);
    assert_eq!(
        disk(&error.failure.error),
        (RetentionPhase::Census, Cause::Busy, Effect::Unchanged)
    );
    open(error.after);
    assert_eq!(slots, [Some(LiteralApplied(7))]);
    assert_eq!(state.borrow().chain, [(7, 22)]);
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

    // A write conflict leaves only A's full rollback.
    let state = self::state(&[(5, 22), (7, 18)], &[(Commit, Fault::RollbackRequired)]);
    let error = commit(
        &mut LiteralCall { state: &state },
        LiteralTransaction(1),
        &mut loan,
    )
    .unwrap_err();
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
    assert_eq!(error.rolled_back, 2);
    assert!(matches!(error.after, After::Closed));
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
    for (fault, closed) in [(Fault::Rejected, true), (Fault::Fatal, false)] {
        let state = self::state(&[(5, 22)], &[(Commit, fault)]);
        let error = commit(
            &mut LiteralCall { state: &state },
            LiteralTransaction(1),
            &mut loan,
        )
        .unwrap_err();
        assert_eq!(error.failure.must_resolve, !closed);
        if closed {
            assert!(matches!(error.after, After::Closed));
        } else {
            assert!(must_resolve(error.after).is_none());
        }
        assert_eq!(state.borrow().log, [Commit]);
    }
    // A conflict whose abort is refused at its start is Open, not Returned.
    let state = self::state(
        &[(5, 22)],
        &[
            (Commit, Fault::RollbackRequired),
            (full, Fault::Before(Cause::Busy)),
        ],
    );
    let error = commit(
        &mut LiteralCall { state: &state },
        LiteralTransaction(4),
        &mut loan,
    )
    .unwrap_err();
    assert_eq!(
        error.failure.cleanup.as_ref().unwrap().0,
        Stage::RollbackStart
    );
    assert_eq!(open(error.after), LiteralTransaction(4));
    assert_eq!(state.borrow().log, [Commit, full]);

    // Full abort, and a finish_aborted refused as C0 does.
    let state = self::state(&[(5, 22)], &[]);
    let aborted = abort(
        &mut LiteralCall { state: &state },
        LiteralTransaction(1),
        &mut loan,
    );
    assert_eq!(aborted.unwrap(), 1);
    assert_eq!(
        state.borrow().log,
        calls(&[
            &[full],
            &compensated(5),
            &[Next, RollbackFinish, FinishAborted]
        ])
    );
    settled(&state);
    let state = self::state(&[], &[(FinishAborted, Fault::Before(Cause::Unsupported))]);
    let error = failed(abort(
        &mut LiteralCall { state: &state },
        LiteralTransaction(5),
        &mut loan,
    ));
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
    assert_eq!(open(error.after), LiteralTransaction(5));
    assert_eq!(
        state.borrow().log,
        [full, Next, RollbackFinish, FinishAborted]
    );
}

/// Stack below the test frame down to the producer stand-in, for the
/// INSERT path and for the CLR path of a statement rollback.
fn measured() -> (usize, usize) {
    let state = self::state(&[], &[]);
    let top = probe();
    run(&state, &[&V1], &mut [None], &mut loan()).unwrap();
    let insert = top - state.borrow().deepest;
    let state = self::state(&[], &[]);
    let top = probe();
    run(&state, &[&V1, &BAD], &mut [None, None], &mut loan()).unwrap_err();
    let clr = top - state.borrow().deepest;
    (insert, clr)
}

#[test]
fn mca00_d12_driver_frames_stay_within_the_driver_source_bound() {
    let (live_insert, live_clr) =
        live::live_carrier_bytes::<platform::namespace::LinuxWriterLease>();
    let (stand_insert, stand_clr) = carrier_bytes::<LiteralCall<'static>>();
    let (measured_insert, measured_clr) = measured();
    let (validation, producer) = super::super::local_source_carrier_bytes();
    // The measured stand-in frames with the live carriers in place of the
    // stand-in ones: an estimate, not a measurement of the live types.
    let estimate_insert = (measured_insert + live_insert).saturating_sub(stand_insert);
    let estimate_clr = (measured_clr + live_clr).saturating_sub(stand_clr);
    println!(
        "MCA00_D12 bound={DRIVER_SOURCE_BYTES} live_insert={live_insert} live_clr={live_clr} \
         stand_insert={stand_insert} stand_clr={stand_clr} measured_insert={measured_insert} \
         measured_clr={measured_clr} estimate_insert={estimate_insert} \
         estimate_clr={estimate_clr} a_validation={validation} a_producer={producer}"
    );
    assert!(measured_insert > 0 && measured_clr > 0);
    for bytes in [live_insert, live_clr, measured_insert, measured_clr] {
        assert!(bytes <= DRIVER_SOURCE_BYTES, "{bytes}");
    }
}
