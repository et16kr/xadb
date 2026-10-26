//! Full `ResolvedInput` assembly from one pre-state and raw action (D4-LM-ASM-001).
//!
//! The output supplies candidate identities and rows only. Admission, environment possibility,
//! consumption, transitions, responses, alias and exploration belong to the later model, so no
//! API return code or environment exclusion is predicted here.
mod rows;

use super::input_schema::{
    BindValue, Environment, InputObligation, InputOwner, RawIdentity, ResolvedInput,
};
use super::raw_action::{RawAction, RawActionError, index_of};
use super::schema::{
    Access, ActorRecord, Api, Autocommit, CommitProgress, ContextData, Deadline, Decision,
    Descriptor, EngineAdmission, EngineRecord, FactKind, Identity, IdentityKind, OwnerDisposition,
    OwnershipSummary, ParkedContext, PreparedSlot, ReceiptKind, Records, ResultRecord, Shape,
    State, StatementLife, StatementRecord, TransactionOrigin, TransactionRecord,
};
use super::target_selector::{SelectorError, TargetKind, resolve_selector};
use rows::{Cause, Role, Rows, Sym};

/// A structural assembly failure; never an API return code or environment exclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblyError {
    /// The raw descriptor has a missing, unused or out-of-range field; the state is not read.
    InvalidAction(RawActionError),
    /// The frozen selector rejected the caller or its selected namespace.
    Selector(SelectorError),
    /// A reference, scope, backlink or stored counter needed by this action is malformed.
    InvalidState,
    /// A supplied prospective definition would exceed its exploration profile cap.
    ProfileLimit,
    /// The fixed output storage cannot hold the selected rows.
    Capacity,
}

type Step<T> = Result<T, AssemblyError>;
const INVALID: AssemblyError = AssemblyError::InvalidState;

/// Assemble the complete input of `action` against `state` into the caller-owned `out`.
///
/// The state is never modified and is not fully validated: only the references this action
/// reads are checked. Prospective identities are candidates; no counter advances.
///
/// # Errors
///
/// Checks the raw descriptor before reading state, then the selector or caller, the required
/// continuation, selected references and relations, the structure of every used namespace,
/// prospective caps, and finally output capacity. On error `out` is partial and must not be
/// encoded, compared or published.
pub fn assemble_input(
    state: &State,
    action: &RawAction,
    out: &mut ResolvedInput,
) -> Result<(), AssemblyError> {
    index_of(action).map_err(AssemblyError::InvalidAction)?;
    let mut assembly = Assembly::new(state, action);
    assembly.prefix()?;
    assembly.family()?;
    assembly.write(out)
}

/// What the input's context field carries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextChoice {
    None,
    Fresh,
    Continued(Identity),
}

/// A receipt field choice; new receipts take the selected transaction scope.
#[derive(Clone, Copy)]
enum Receipt {
    None,
    New {
        actor: u8,
        engine: u16,
        session: u16,
    },
    Existing(Identity),
}

/// An anchor field choice; a new anchor follows its new durable receipt's engine.
#[derive(Clone, Copy)]
enum Anchor {
    None,
    New,
    Existing(Identity),
}

/// Environment scalars before identities are materialized.
struct EnvPlan {
    kind: FactKind,
    engine: u16,
    work_kind: u8,
    work_class: u8,
    cleanup: u8,
    commit_class: u8,
    transaction: Option<Sym>,
    receipt: Receipt,
    anchor: Anchor,
    fresh_engine: bool,
    recovery_mutation: u8,
}

/// The selected transaction of an environment, existing or prospective.
#[derive(Clone, Copy)]
enum Txn<'a> {
    None,
    Existing(&'a TransactionRecord),
    Prospective {
        origin: TransactionOrigin,
        access: Access,
    },
}
impl Txn<'_> {
    fn read_write(self) -> bool {
        match self {
            Self::None => false,
            Self::Existing(t) => t.access == Access::ReadWrite,
            Self::Prospective { access, .. } => access == Access::ReadWrite,
        }
    }
    fn implicit_read_write(self) -> bool {
        match self {
            Self::None => false,
            Self::Existing(t) => {
                t.origin == TransactionOrigin::ImplicitReadWrite && t.access == Access::ReadWrite
            }
            Self::Prospective { origin, .. } => origin == TransactionOrigin::ImplicitReadWrite,
        }
    }
    fn sym(self) -> Option<Sym> {
        match self {
            Self::None => None,
            Self::Existing(t) => Some(Sym::Known(t.id)),
            Self::Prospective { .. } => Some(Sym::NewTransaction),
        }
    }
    fn cause(&self) -> Cause<'_> {
        match *self {
            Self::None => Cause::None,
            Self::Existing(t) => Cause::Existing(t),
            Self::Prospective { access, .. } => Cause::Prospective {
                read_write: access == Access::ReadWrite,
            },
        }
    }
}

/// The prospective cursor holder of an authenticated Execute Success Select.
#[derive(Clone, Copy)]
struct NewResult {
    statement: Identity,
    stored_generation: u16,
    retirement_offset: u16,
}

struct Assembly<'a> {
    state: &'a State,
    action: &'a RawAction,
    caller: Option<&'a ActorRecord>,
    target: Option<RawIdentity>,
    context: ContextChoice,
    deadline: Deadline,
    cancel: bool,
    reached: bool,
    bind: Records<BindValue, 2>,
    env: Option<EnvPlan>,
    rows: Rows,
    new_transaction: bool,
    new_result: Option<NewResult>,
}

impl<'a> Assembly<'a> {
    fn new(state: &'a State, action: &'a RawAction) -> Self {
        Self {
            state,
            action,
            caller: None,
            target: None,
            context: ContextChoice::None,
            deadline: Deadline::Absent,
            cancel: false,
            reached: false,
            bind: Records::default(),
            env: None,
            rows: Rows::new(),
            new_transaction: false,
            new_result: None,
        }
    }

    fn caller(&self) -> Step<&'a ActorRecord> {
        self.caller.ok_or(INVALID)
    }

    /// The caller's (actor, session), the scope of any prospective transaction.
    fn prospective_scope(&self) -> Step<(u8, u16)> {
        let caller = self.caller()?;
        Ok((caller.actor, caller.session.0))
    }

    /// S1 and the control prefix: selector or caller, fresh context intent, entry controls.
    fn prefix(&mut self) -> Step<()> {
        let api = self.action.api;
        if let Some(kind) = target_kind(api) {
            let selector = self.action.selector.ok_or(INVALID)?;
            let raw = resolve_selector(self.state, self.action.actor, kind, selector)
                .map_err(AssemblyError::Selector)?;
            self.target = Some(raw);
        }
        if api != Api::RecoveryConfirmedFreshEngine {
            self.caller = Some(caller(self.state, self.action.actor)?);
        }
        let outcome = self.action.outcome;
        let entry = entry_controls(api);
        if let Some((cancelled, timed_out)) = entry {
            self.context = ContextChoice::Fresh;
            if outcome == cancelled {
                self.cancel = true;
            } else if outcome == timed_out {
                self.deadline = Deadline::ReachedAtEntry;
                self.reached = true;
            }
        }
        if matches!(api, Api::ParkFetch | Api::BeginCommit) {
            self.context = ContextChoice::Fresh;
            if outcome == 1 {
                self.deadline = Deadline::FutureAtEntry;
            }
        }
        Ok(())
    }

    /// S2/S3: family-specific continuation, authentication, transaction and rows.
    fn family(&mut self) -> Step<()> {
        let outcome = self.action.outcome;
        match self.action.api {
            Api::BindAll => self.bind(outcome),
            Api::Execute if outcome != 1 && outcome != 2 => self.execute(outcome),
            Api::Fetch if outcome != 3 && outcome != 4 => self.fetch(outcome),
            Api::CloseResult if outcome == 0 => self.close_result(),
            Api::CloseStatement if outcome == 0 => self.close_statement(),
            Api::CommitAtomic if outcome < 8 => self.commit_atomic(outcome),
            Api::Rollback if outcome < 2 => self.rollback(outcome),
            Api::CloseSession if outcome < 2 => self.close_session(outcome),
            Api::ResumeFetch => self.resume(outcome),
            Api::CommitFact => self.commit_fact(outcome),
            Api::FinishCommit => self.finish(outcome),
            Api::RecoveryConfirmedFreshEngine => self.recovery(outcome),
            _ => Ok(()),
        }
    }

    fn environment(&mut self, kind: FactKind, work_kind: u8, work_class: u8) -> &mut EnvPlan {
        let engine = match self.caller {
            Some(caller) => caller.engine.0,
            None => self.state.current_engine.0,
        };
        self.env.insert(EnvPlan {
            kind,
            engine,
            work_kind,
            work_class,
            cleanup: 255,
            commit_class: 255,
            transaction: None,
            receipt: Receipt::None,
            anchor: Anchor::None,
            fresh_engine: false,
            recovery_mutation: 0,
        })
    }

    fn set(&mut self, cleanup: u8, commit_class: u8, transaction: Txn<'_>) {
        if let Some(env) = self.env.as_mut() {
            env.cleanup = cleanup;
            env.commit_class = commit_class;
            env.transaction = transaction.sym();
        }
    }

    /// A new receipt in the selected transaction's original scope, or the caller's scope.
    fn new_receipt(&mut self, transaction: Txn<'_>, durable: bool) -> Step<()> {
        let caller = self.caller()?;
        let (actor, engine, session) = match transaction {
            Txn::Existing(t) => (t.id.actor, t.id.engine, t.id.session),
            _ => (caller.actor, caller.engine.0, caller.session.0),
        };
        let env = self.env.as_mut().ok_or(INVALID)?;
        env.receipt = Receipt::New {
            actor,
            engine,
            session,
        };
        if durable {
            env.anchor = Anchor::New;
        }
        Ok(())
    }

    /// The selected transaction's preserved receipt and anchor, or the explicit None fallback.
    fn existing_receipt(&mut self, transaction: Txn<'_>) -> Step<()> {
        let Txn::Existing(t) = transaction else {
            return Ok(());
        };
        let Some(reference) = t.receipt else {
            return Ok(());
        };
        let receipt = unique(self.state.receipts.as_slice(), |r| {
            r.id.nonce == reference.0
        })?;
        let anchor_ok = match (receipt.kind, receipt.anchor) {
            (ReceiptKind::DurableWrite, Some(a)) => {
                primitive(a, IdentityKind::Anchor) && a.engine == receipt.engine.0
            }
            (ReceiptKind::NoWriteCompletion, None) => true,
            _ => false,
        };
        if !primitive(receipt.id, IdentityKind::Receipt)
            || receipt.transaction.0 != t.id.nonce
            || receipt.engine.0 != t.id.engine
            || !same_scope(receipt.id, t.id)
            || !anchor_ok
        {
            return Err(INVALID);
        }
        let env = self.env.as_mut().ok_or(INVALID)?;
        env.receipt = Receipt::Existing(receipt.id);
        env.anchor = receipt.anchor.map_or(Anchor::None, Anchor::Existing);
        Ok(())
    }

    /// A1 fixed Bind payloads; only SuccessA/B and entry controls read the parameter count.
    fn bind(&mut self, outcome: u8) -> Step<()> {
        let values = match outcome {
            0 | 1 | 8 | 9 => {
                let n1 = match self.authenticate_statement()? {
                    Some(p) if p.parameter_count == 1 => true,
                    Some(p) if p.parameter_count == 0 => false,
                    Some(_) => return Err(INVALID),
                    None => false,
                };
                let byte = if outcome == 1 { b'b' } else { b'a' };
                if n1 {
                    Values::one(bind_value(0, &[byte]))
                } else {
                    Values::default()
                }
            }
            2 => Values::default(),
            3 => Values::one(bind_value(1, &1i64.to_le_bytes())),
            4 => Values::one(bind_value(2, &[])),
            5 => Values::one(bind_value(0, b"aa")),
            6 => Values::one(bind_value(0, &[0xff])),
            7 => Values::one(bind_value(0, b"a")),
            _ => Values {
                rows: [bind_value(0, b"a"), bind_value(0, b"b")],
                len: 2,
            },
        };
        for row in values.as_slice() {
            self.bind.try_push(*row).map_err(|_| INVALID)?;
        }
        Ok(())
    }

    fn execute(&mut self, outcome: u8) -> Step<()> {
        self.environment(FactKind::Execution, 1, outcome);
        let authenticated = self.authenticate_statement()?;
        let Some(statement) = authenticated else {
            // Authentication failure: no transaction, rows or census; raw class fallbacks remain.
            let (cleanup, commit) = match outcome {
                0 => (255, 255),
                3..=7 => (1, 255),
                8 => (2, 1),
                9 => (255, 7),
                _ => (255, 8),
            };
            self.set(cleanup, commit, Txn::None);
            if outcome == 10 {
                self.new_receipt(Txn::None, true)?;
            }
            return Ok(());
        };
        let caller = self.caller()?;
        let transaction = match caller.transaction {
            Some(reference) => {
                let t = transaction(self.state, reference.0)?;
                if !same_scope(t.id, caller_identity(caller)) {
                    return Err(INVALID);
                }
                Txn::Existing(t)
            }
            None => {
                self.new_transaction = true;
                match (caller.autocommit, statement.shape) {
                    (Autocommit::On, Shape::Select) => Txn::Prospective {
                        origin: TransactionOrigin::ImplicitReadOnly,
                        access: Access::ReadOnly,
                    },
                    (Autocommit::On, Shape::Dml) => Txn::Prospective {
                        origin: TransactionOrigin::ImplicitReadWrite,
                        access: Access::ReadWrite,
                    },
                    (Autocommit::Off, _) => Txn::Prospective {
                        origin: TransactionOrigin::OffStarted,
                        access: Access::ReadWrite,
                    },
                }
            }
        };
        let prospective = matches!(transaction, Txn::Prospective { .. });
        let engine = caller.engine.0;
        match outcome {
            0 if statement.shape == Shape::Select => {
                self.set(255, 255, transaction);
                if prospective && transaction.read_write() {
                    self.rows.rw_births(OwnerDisposition::Held)?;
                }
                self.new_result = Some(self.new_result_for(statement)?);
                self.rows.cursor_births()?;
            }
            0 => {
                if transaction.implicit_read_write() {
                    self.set(255, 4, transaction);
                    self.new_receipt(transaction, true)?;
                    match transaction {
                        Txn::Existing(t) => {
                            self.rows
                                .release_transaction(self.state, t, false, Role::Committed)?;
                        }
                        _ => self.rows.rw_births(OwnerDisposition::Released)?,
                    }
                } else {
                    self.set(255, 255, transaction);
                    if prospective {
                        self.rows.rw_births(OwnerDisposition::Held)?;
                    }
                }
            }
            3..=6 => {
                if prospective && transaction.implicit_read_write() {
                    self.set(1, 0, transaction);
                    self.rows.rw_births(OwnerDisposition::Released)?;
                } else {
                    self.set(1, 255, transaction);
                    if prospective && transaction.read_write() {
                        self.rows.rw_births(OwnerDisposition::Held)?;
                    }
                }
            }
            7 => {
                let commit = if transaction.read_write() { 0 } else { 255 };
                self.set(1, commit, transaction);
                match transaction {
                    Txn::Existing(t) => self.release_whole(t)?,
                    _ if transaction.read_write() => {
                        self.rows.rw_births(OwnerDisposition::Released)?;
                    }
                    _ => {}
                }
            }
            _ => {
                let (cleanup, commit, raw) = fence_class(outcome - 7);
                self.set(cleanup, commit, transaction);
                if outcome == 10 {
                    self.new_receipt(transaction, true)?;
                }
                self.rows.census(
                    self.state,
                    engine,
                    self.prospective_scope()?,
                    transaction.cause(),
                    raw,
                )?;
            }
        }
        Ok(())
    }

    fn fetch(&mut self, outcome: u8) -> Step<()> {
        self.environment(FactKind::FetchWorkOrCleanup, 2, outcome);
        let Some((record, descriptor)) = self.authenticate_result()? else {
            let cleanup = match outcome {
                0 | 2 => 255,
                9 => 2,
                _ => 1,
            };
            self.set(cleanup, 255, Txn::None);
            return Ok(());
        };
        let transaction = self.descriptor_transaction(descriptor)?;
        let engine = self.caller()?.engine.0;
        match outcome {
            0 | 2 => self.set(255, 255, transaction),
            8 => {
                let commit = if transaction.read_write() { 0 } else { 255 };
                self.set(1, commit, transaction);
                if let Txn::Existing(t) = transaction {
                    self.release_whole(t)?;
                } else {
                    self.rows.release_cursor(self.state, record.id.nonce)?;
                }
            }
            9 => {
                let commit = if transaction.read_write() { 1 } else { 255 };
                self.set(2, commit, transaction);
                self.rows.census(
                    self.state,
                    engine,
                    self.prospective_scope()?,
                    transaction.cause(),
                    Decision::NotCommittedRecoveryRequired,
                )?;
            }
            _ => {
                self.set(1, 255, transaction);
                self.rows.release_cursor(self.state, record.id.nonce)?;
            }
        }
        Ok(())
    }

    fn close_result(&mut self) -> Step<()> {
        self.environment(FactKind::LocalCleanup, 0, 255);
        match self.authenticate_result()? {
            Some((record, descriptor)) => {
                let transaction = self.descriptor_transaction(descriptor)?;
                self.set(1, 255, transaction);
                self.rows.release_cursor(self.state, record.id.nonce)
            }
            None => {
                self.set(1, 255, Txn::None);
                Ok(())
            }
        }
    }

    fn close_statement(&mut self) -> Step<()> {
        self.environment(FactKind::LocalCleanup, 0, 255);
        let Some(statement) = self.authenticate_statement()? else {
            self.set(1, 255, Txn::None);
            return Ok(());
        };
        let slot = slot(self.state, statement.id.actor, statement.id.slot)?;
        self.check_binding(slot, statement)?;
        let transaction = match slot.descriptor_result {
            Some(result) => {
                let descriptor = descriptor(self.state, result.0)?;
                if descriptor.statement.0 != statement.id.nonce {
                    return Err(INVALID);
                }
                self.descriptor_transaction(descriptor)?
            }
            None => Txn::None,
        };
        self.set(1, 255, transaction);
        self.rows.release_local(
            self.state,
            Some(statement.id.nonce),
            slot.descriptor_result.map(|r| r.0),
        )
    }

    fn commit_atomic(&mut self, outcome: u8) -> Step<()> {
        self.environment(FactKind::CommitAtomic, 0, 255);
        let transaction = self.caller_transaction()?;
        let engine = self.caller()?.engine.0;
        match outcome {
            0 => {
                let changed = matches!(transaction, Txn::Existing(t)
                    if t.access == Access::ReadWrite && t.prior_successful_write);
                self.set(255, if changed { 4 } else { 5 }, transaction);
                self.new_receipt(transaction, changed)?;
                let role = if changed {
                    Role::Committed
                } else {
                    Role::Retained
                };
                self.release_rw(transaction, role)?;
            }
            1 => {
                self.set(255, 6, transaction);
                self.release_rw(transaction, Role::Retained)?;
            }
            2 | 5 | 6 => {
                self.set(1, 0, transaction);
                self.release_rw(transaction, Role::Retained)?;
            }
            _ => {
                let class = match outcome {
                    3 => 1,
                    4 => 2,
                    _ => 3,
                };
                let (cleanup, commit, raw) = fence_class(class);
                self.set(cleanup, commit, transaction);
                if outcome == 7 {
                    self.new_receipt(transaction, true)?;
                }
                self.rows.census(
                    self.state,
                    engine,
                    self.prospective_scope()?,
                    transaction.cause(),
                    raw,
                )?;
            }
        }
        Ok(())
    }

    fn rollback(&mut self, outcome: u8) -> Step<()> {
        self.environment(FactKind::RollbackCleanup, 0, 255);
        let transaction = self.caller_transaction()?;
        if outcome == 0 {
            self.set(1, 0, transaction);
            if let Txn::Existing(t) = transaction {
                self.release_whole(t)?;
            }
        } else {
            self.set(2, 1, transaction);
            let engine = self.caller()?.engine.0;
            self.rows.census(
                self.state,
                engine,
                self.prospective_scope()?,
                transaction.cause(),
                Decision::NotCommittedRecoveryRequired,
            )?;
        }
        Ok(())
    }

    fn close_session(&mut self, outcome: u8) -> Step<()> {
        self.environment(FactKind::SessionCleanup, 0, 255);
        let caller = self.caller()?;
        let transaction = self.caller_transaction()?;
        if outcome == 1 {
            self.set(2, 1, transaction);
            return self.rows.census(
                self.state,
                caller.engine.0,
                self.prospective_scope()?,
                transaction.cause(),
                Decision::NotCommittedRecoveryRequired,
            );
        }
        // Only an Admitting engine takes the whole-abort path; Fenced and RecoveryConfirmedRetired
        // engines take local release and handoff (owner decision in D4-LM-ASM-001).
        let admitting =
            engine(self.state, caller.engine.0)?.admission == EngineAdmission::Admitting;
        for index in 0..2 {
            let slot = slot(self.state, caller.actor, index)?;
            let statement = match slot.statement {
                Some(reference) => {
                    let statement = statement(self.state, reference.0)?;
                    self.check_binding(slot, statement)?;
                    Some(statement.id.nonce)
                }
                None => None,
            };
            self.rows
                .release_local(self.state, statement, slot.descriptor_result.map(|r| r.0))?;
        }
        if admitting {
            let commit = if transaction.read_write() { 0 } else { 255 };
            self.set(1, commit, transaction);
            self.release_rw(transaction, Role::Retained)
        } else {
            self.set(3, 255, transaction);
            self.rows
                .handoff_session(self.state, caller.engine.0, caller.session.0)
        }
    }

    fn resume(&mut self, outcome: u8) -> Step<()> {
        let continuation = self.continuation(Api::ParkFetch)?;
        self.environment(FactKind::FetchWorkOrCleanup, 3, outcome / 2);
        let complete = outcome.is_multiple_of(2);
        let cleanup = if complete { 1 } else { 2 };
        let Some(continuation) = continuation else {
            self.set(cleanup, 255, Txn::None);
            return Ok(());
        };
        let payload = continuation;
        let result = payload.result.ok_or(INVALID)?.0;
        let transaction = match payload.transaction {
            Some(t) => Txn::Existing(transaction(self.state, t.0)?),
            None => Txn::None,
        };
        let signalled = payload.cancel_supplied_or_signalled || payload.deadline_reached;
        // Resume work: Rows0, Eof1, ReadError2, Conflict3, RowBufferTooSmall4 (NextWork - 1).
        let work = outcome / 2;
        let rw = transaction.read_write();
        let commit = match (complete, signalled, work) {
            (false, _, _) if rw => 1,
            (true, false, 3) if rw => 0,
            _ => 255,
        };
        self.set(cleanup, commit, transaction);
        // Unsignalled Rows/RowBufferTooSmall follow Fetch: no owners even with a raw Incomplete
        // cleanup, which stays in the class bytes; any other Incomplete takes the fence census.
        if !signalled && (work == 0 || work == 4) {
            return Ok(());
        }
        if !complete {
            let engine = self.caller()?.engine.0;
            return self.rows.census(
                self.state,
                engine,
                self.prospective_scope()?,
                transaction.cause(),
                Decision::NotCommittedRecoveryRequired,
            );
        }
        match (signalled, work) {
            (false, 3) => match transaction {
                Txn::Existing(t) => self.release_whole(t),
                _ => self.rows.release_cursor(self.state, result),
            },
            _ => self.rows.release_cursor(self.state, result),
        }
    }

    fn commit_fact(&mut self, outcome: u8) -> Step<()> {
        let continuation = self.continuation(Api::BeginCommit)?;
        self.environment(FactKind::CommitProgress, 0, 255);
        let transaction = match continuation {
            Some(c) => Txn::Existing(transaction(self.state, c.transaction.ok_or(INVALID)?.0)?),
            None => Txn::None,
        };
        self.set(255, outcome + 2, transaction);
        match outcome {
            1 => self.new_receipt(transaction, true)?,
            2 => {
                self.existing_receipt(transaction)?;
                if let Txn::Existing(t) = transaction {
                    self.rows
                        .release_transaction(self.state, t, true, Role::Committed)?;
                }
            }
            3 => {
                self.new_receipt(transaction, false)?;
                self.release_rw(transaction, Role::Retained)?;
            }
            4 => self.release_rw(transaction, Role::Retained)?,
            _ => {}
        }
        Ok(())
    }

    fn finish(&mut self, outcome: u8) -> Step<()> {
        let continuation = self.continuation(Api::BeginCommit)?;
        self.environment(FactKind::CommitFinish, 0, 255);
        let caller = self.caller()?;
        let transaction = match continuation {
            Some(c) => Txn::Existing(transaction(self.state, c.transaction.ok_or(INVALID)?.0)?),
            None => Txn::None,
        };
        match outcome {
            0 => {
                let commit = match transaction {
                    Txn::Existing(t) if t.progress == CommitProgress::NoWriteValidated => {
                        if t.receipt.is_some() { 5 } else { 6 }
                    }
                    _ => 4,
                };
                self.set(255, commit, transaction);
                self.existing_receipt(transaction)?;
            }
            1 => {
                self.set(1, 0, transaction);
                self.release_rw(transaction, Role::Retained)?;
            }
            2 => {
                // wire-v2: "Fenced이면3 아니면2"; a retired engine is not Fenced here.
                let fenced =
                    engine(self.state, caller.engine.0)?.admission == EngineAdmission::Fenced;
                self.set(if fenced { 3 } else { 2 }, 1, transaction);
                match transaction {
                    // A parked T that already ended (NoWriteValidated or Published) has no O to
                    // hand off: the class is environment-impossible and is supplied without
                    // rows (owner decision 2026-10-25).
                    Txn::Existing(t) if fenced && t.obligation.is_none() => {}
                    Txn::Existing(t) if fenced => self.rows.handoff_transaction(self.state, t)?,
                    Txn::Existing(_) => self.rows.census(
                        self.state,
                        caller.engine.0,
                        self.prospective_scope()?,
                        transaction.cause(),
                        Decision::NotCommittedRecoveryRequired,
                    )?,
                    _ => {}
                }
            }
            _ => {
                let (cleanup, commit, raw) = fence_class(outcome - 1);
                self.set(cleanup, commit, transaction);
                if outcome == 4 {
                    self.existing_receipt(transaction)?;
                }
                if matches!(transaction, Txn::Existing(_)) {
                    self.rows.census(
                        self.state,
                        caller.engine.0,
                        self.prospective_scope()?,
                        transaction.cause(),
                        raw,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn recovery(&mut self, outcome: u8) -> Step<()> {
        let old = self.state.current_engine.0;
        engine(self.state, old)?;
        let env = self.environment(FactKind::RecoveryConfirmed, 0, 255);
        env.cleanup = 4;
        env.fresh_engine = true;
        env.recovery_mutation = outcome;
        self.rows.recover(self.state, old)
    }

    /// Conflict or rollback: the transaction's live RW and its currently Open cursors.
    fn release_whole(&mut self, transaction: &TransactionRecord) -> Step<()> {
        self.rows
            .release_transaction(self.state, transaction, false, Role::Retained)?;
        self.rows
            .release_open_cursors(self.state, transaction.id.nonce)
    }

    fn release_rw(&mut self, transaction: Txn<'_>, role: Role) -> Step<()> {
        match transaction {
            Txn::Existing(t) => self.rows.release_transaction(self.state, t, false, role),
            _ => Ok(()),
        }
    }

    fn caller_transaction(&self) -> Step<Txn<'a>> {
        let caller = self.caller()?;
        match caller.transaction {
            Some(reference) => {
                let t = transaction(self.state, reference.0)?;
                if same_scope(t.id, caller_identity(caller)) {
                    Ok(Txn::Existing(t))
                } else {
                    Err(INVALID)
                }
            }
            None => Ok(Txn::None),
        }
    }

    fn descriptor_transaction(&self, descriptor: &Descriptor) -> Step<Txn<'a>> {
        match descriptor.transaction {
            Some(reference) => {
                let t = transaction(self.state, reference.0)?;
                if !caller_owns(self.caller()?, t.id) {
                    return Err(INVALID);
                }
                Ok(Txn::Existing(t))
            }
            None => Ok(Txn::None),
        }
    }

    /// Same-caller exact current statement (Current or CurrentClosed) with its slot backlink.
    fn authenticate_statement(&self) -> Step<Option<&'a StatementRecord>> {
        let caller = self.caller()?;
        let Some(id) = self.target.and_then(unpack) else {
            return Ok(None);
        };
        if id.kind != IdentityKind::Statement || !caller_owns(caller, id) {
            return Ok(None);
        }
        let Some(record) = lookup(self.state.issued_statements.as_slice(), |s| {
            s.id.nonce == id.nonce
        })?
        else {
            return Ok(None);
        };
        if record.id != id || record.life == StatementLife::Retired {
            return Ok(None);
        }
        let slot = slot(self.state, id.actor, id.slot)?;
        if slot.statement.is_none_or(|s| s.0 != id.nonce)
            || record.session.0 != id.session
            || !primitive(record.id, IdentityKind::Statement)
        {
            return Err(INVALID);
        }
        Ok(Some(record))
    }

    /// Same-caller exact public current result with its descriptor, statement and slot links.
    fn authenticate_result(&self) -> Step<Option<(&'a ResultRecord, &'a Descriptor)>> {
        let caller = self.caller()?;
        let Some(id) = self.target.and_then(unpack) else {
            return Ok(None);
        };
        if id.kind != IdentityKind::Result || !caller_owns(caller, id) {
            return Ok(None);
        }
        let Some(record) = lookup(self.state.issued_results.as_slice(), |r| {
            r.id.nonce == id.nonce
        })?
        else {
            return Ok(None);
        };
        if record.id != id || !record.public_generation_live {
            return Ok(None);
        }
        let descriptor = descriptor(self.state, id.nonce)?;
        let statement = statement(self.state, record.statement.0)?;
        let slot = slot(self.state, id.actor, id.slot)?;
        if descriptor.statement != record.statement
            || !descriptor.public_generation_live
            || statement.id.actor != id.actor
            || statement.id.slot != id.slot
            || !same_scope(statement.id, id)
            || statement.session.0 != id.session
            || slot.statement != Some(record.statement)
            || slot.descriptor_result.is_none_or(|r| r.0 != id.nonce)
            || !primitive(record.id, IdentityKind::Result)
        {
            return Err(INVALID);
        }
        Ok(Some((record, descriptor)))
    }

    /// The slot's current binding row must exist once and belong to the statement.
    fn check_binding(&self, slot: &PreparedSlot, statement: &StatementRecord) -> Step<()> {
        if let Some(reference) = slot.binding {
            let binding = unique(self.state.bindings.as_slice(), |b| {
                b.id.nonce == reference.0
            })?;
            if binding.statement.0 != statement.id.nonce {
                return Err(INVALID);
            }
        }
        Ok(())
    }

    /// The candidate generation is the slot's stored next plus a still-current old token's retirement.
    fn new_result_for(&self, statement: &StatementRecord) -> Step<NewResult> {
        let slot = slot(self.state, statement.id.actor, statement.id.slot)?;
        let retirement_offset = match slot.descriptor_result {
            None => 0,
            Some(reference) => {
                let descriptor = descriptor(self.state, reference.0)?;
                let record = unique(self.state.issued_results.as_slice(), |r| {
                    r.id.nonce == reference.0
                })?;
                if record.statement != descriptor.statement
                    || record.statement.0 != statement.id.nonce
                    || record.id.actor != statement.id.actor
                    || record.id.slot != statement.id.slot
                    || !same_scope(record.id, statement.id)
                    || record.public_generation_live != descriptor.public_generation_live
                {
                    return Err(INVALID);
                }
                u16::from(record.public_generation_live)
            }
        };
        Ok(NewResult {
            statement: statement.id,
            stored_generation: slot.next_result_generation,
            retirement_offset,
        })
    }

    /// The actor's pending context when it is the requested kind; other kinds are unmatched.
    fn continuation(&mut self, kind: Api) -> Step<Option<&'a ParkedContext>> {
        let caller = self.caller()?;
        let Some(reference) = caller.parked_context else {
            return Ok(None);
        };
        let context = unique(self.state.contexts.as_slice(), |c| {
            c.id.nonce == reference.0
        })?;
        let id = context.id;
        if !primitive(id, IdentityKind::Context) || !caller_owns(caller, id) {
            return Err(INVALID);
        }
        let payload = match (&context.data, kind) {
            (ContextData::Completed, _) => return Err(INVALID),
            (ContextData::ParkedFetch(p), Api::ParkFetch)
            | (ContextData::ParkedCommit(p), Api::BeginCommit) => p,
            _ => return Ok(None),
        };
        if payload.api != kind {
            return Err(INVALID);
        }
        let backlink = |t: &TransactionRecord| {
            caller_owns(caller, t.id) && t.parked_context.is_some_and(|c| c.0 == id.nonce)
        };
        if kind == Api::ParkFetch {
            let statement = statement(self.state, payload.statement.ok_or(INVALID)?.0)?;
            let result = payload.result.ok_or(INVALID)?.0;
            let descriptor = descriptor(self.state, result)?;
            let record = unique(self.state.issued_results.as_slice(), |r| {
                r.id.nonce == result
            })?;
            let slot = slot(self.state, statement.id.actor, statement.id.slot)?;
            if !caller_owns(caller, statement.id)
                || statement.session.0 != statement.id.session
                || descriptor.statement.0 != statement.id.nonce
                || descriptor.transaction != payload.transaction
                || record.statement.0 != statement.id.nonce
                || record.public_generation_live != descriptor.public_generation_live
                || record.id.actor != statement.id.actor
                || record.id.slot != statement.id.slot
                || !primitive(record.id, IdentityKind::Result)
                || !same_scope(record.id, statement.id)
                || slot.statement.is_none_or(|s| s.0 != statement.id.nonce)
                || slot.descriptor_result.is_none_or(|r| r.0 != result)
            {
                return Err(INVALID);
            }
            if let Some(t) = payload.transaction
                && !backlink(transaction(self.state, t.0)?)
            {
                return Err(INVALID);
            }
        } else {
            let t = transaction(self.state, payload.transaction.ok_or(INVALID)?.0)?;
            if payload.statement.is_some() || payload.result.is_some() || !backlink(t) {
                return Err(INVALID);
            }
        }
        self.context = ContextChoice::Continued(id);
        self.deadline = payload.original_deadline;
        self.cancel = payload.cancel_supplied_or_signalled;
        self.reached = payload.deadline_reached;
        Ok(Some(payload))
    }

    /// S4 and S5: namespace structure, prospective caps, then exact rows and capacity.
    fn write(self, out: &mut ResolvedInput) -> Step<()> {
        let state = self.state;
        let env = self.env.as_ref();
        let uses_fact = env.is_some();
        let uses_engine = env.is_some_and(|e| e.fresh_engine);
        let uses_receipt = env.is_some_and(|e| matches!(e.receipt, Receipt::New { .. }));
        let uses_anchor = env.is_some_and(|e| matches!(e.anchor, Anchor::New));
        let births = self.rows.births().len() as u16;
        let new_obligations = self.rows.new_obligations().len() as u16;
        let context = self.context == ContextChoice::Fresh;
        // S4a: every used namespace's structure, in IdentityKind order, before any cap.
        let next_engine = guard(uses_engine, state, IdentityKind::Engine)?;
        let next_result = guard(self.new_result.is_some(), state, IdentityKind::Result)?;
        if let Some(r) = self.new_result
            && !(1..=13).contains(&r.stored_generation)
        {
            return Err(INVALID);
        }
        let next_context = guard(context, state, IdentityKind::Context)?;
        let next_transaction = guard(self.new_transaction, state, IdentityKind::Transaction)?;
        let next_receipt = guard(uses_receipt, state, IdentityKind::Receipt)?;
        let next_resource = guard(births > 0, state, IdentityKind::Resource)?;
        let next_obligation = guard(new_obligations > 0, state, IdentityKind::Obligation)?;
        let next_fact = guard(uses_fact, state, IdentityKind::Fact)?;
        let next_anchor = guard(uses_anchor, state, IdentityKind::Anchor)?;
        // S4b: the whole used candidate block of each namespace.
        let within = |used: bool, next: u16, count: u16, cap: u16| {
            !used || u32::from(next) + u32::from(count) - 1 <= u32::from(cap)
        };
        let generation_ok = self
            .new_result
            .is_none_or(|r| r.stored_generation + r.retirement_offset <= 12);
        if !within(uses_engine, next_engine, 1, 7)
            || !within(self.new_result.is_some(), next_result, 1, 6)
            || !generation_ok
            || !within(context, next_context, 1, 6)
            || !within(self.new_transaction, next_transaction, 1, 6)
            || !within(uses_receipt, next_receipt, 1, 6)
            || !within(births > 0, next_resource, births, 54)
            || !within(new_obligations > 0, next_obligation, new_obligations, 6)
            || !within(uses_fact, next_fact, 1, 6)
            || !within(uses_anchor, next_anchor, 1, 6)
        {
            return Err(AssemblyError::ProfileLimit);
        }
        // S5: materialize candidates and rows.
        let caller = self.caller;
        let scope =
            |kind: IdentityKind, actor: u8, engine: u16, session: u16, nonce: u16| Identity {
                kind,
                actor,
                slot: 255,
                engine,
                session,
                nonce,
                generation: 0,
            };
        let new_transaction = caller.map(|c| {
            scope(
                IdentityKind::Transaction,
                c.actor,
                c.engine.0,
                c.session.0,
                next_transaction,
            )
        });
        let new_result = self.new_result.map(|r| Identity {
            kind: IdentityKind::Result,
            actor: r.statement.actor,
            slot: r.statement.slot,
            engine: r.statement.engine,
            session: r.statement.session,
            nonce: next_result,
            generation: r.stored_generation + r.retirement_offset,
        });
        let resolve = |sym: Sym| -> Step<Identity> {
            match sym {
                Sym::Known(id) => Ok(id),
                Sym::NewTransaction => new_transaction.ok_or(INVALID),
                Sym::NewResult => new_result.ok_or(INVALID),
                Sym::NewObligation(k) => Ok(owner_scope(
                    IdentityKind::Obligation,
                    base(self.rows.new_obligations(), k, new_transaction, new_result)?,
                    next_obligation + u16::from(k),
                )),
                Sym::NewResource(k) => Ok(owner_scope(
                    IdentityKind::Resource,
                    base(self.rows.births(), k, new_transaction, new_result)?,
                    next_resource + u16::from(k),
                )),
            }
        };
        out.api = self.action.api;
        out.actor = self.action.actor;
        out.outcome = self.action.outcome;
        let prepare = self.action.prepare;
        out.prepare_slot = prepare.map_or(255, |p| p.slot);
        out.prepare_shape = prepare.map_or(255, |p| p.shape as u8);
        out.prepare_n = prepare.map_or(255, |p| p.parameter_count);
        out.set_autocommit = self.action.autocommit.map_or(255, |m| m as u8);
        out.target = self.target;
        out.context = match self.context {
            ContextChoice::None => None,
            ContextChoice::Fresh => {
                let c = caller.ok_or(INVALID)?;
                Some(pack(scope(
                    IdentityKind::Context,
                    c.actor,
                    c.engine.0,
                    c.session.0,
                    next_context,
                )))
            }
            ContextChoice::Continued(id) => Some(pack(id)),
        };
        out.original_deadline = self.deadline;
        out.cancel = self.cancel;
        out.deadline_reached = self.reached;
        out.bind_values = self.bind;
        let Some(env) = env else {
            out.environment = None;
            return Ok(());
        };
        if self.rows.overflowed()
            || self.rows.owners().len() > 54
            || self.rows.obligations().len() > 7
        {
            return Err(AssemblyError::Capacity);
        }
        let mut environment = Environment {
            kind: env.kind,
            fact: pack(Identity {
                kind: IdentityKind::Fact,
                actor: 255,
                slot: 255,
                engine: env.engine,
                session: 0,
                nonce: next_fact,
                generation: 0,
            }),
            work_kind: env.work_kind,
            work_class: env.work_class,
            cleanup: env.cleanup,
            commit_class: env.commit_class,
            transaction: env.transaction.map(resolve).transpose()?.map(pack),
            receipt: None,
            anchor: None,
            fresh_engine: None,
            obligations: Records::default(),
            owners: Records::default(),
        };
        let receipt_engine = match env.receipt {
            Receipt::None => None,
            Receipt::New {
                actor,
                engine,
                session,
            } => {
                environment.receipt = Some(pack(scope(
                    IdentityKind::Receipt,
                    actor,
                    engine,
                    session,
                    next_receipt,
                )));
                Some(engine)
            }
            Receipt::Existing(id) => {
                environment.receipt = Some(pack(id));
                Some(id.engine)
            }
        };
        environment.anchor = match env.anchor {
            Anchor::None => None,
            Anchor::Existing(id) => Some(pack(id)),
            Anchor::New => Some(pack(Identity {
                kind: IdentityKind::Anchor,
                actor: 255,
                slot: 255,
                engine: receipt_engine.ok_or(INVALID)?,
                session: 0,
                nonce: next_anchor,
                generation: 0,
            })),
        };
        if env.fresh_engine {
            environment.fresh_engine = Some(pack(Identity {
                kind: IdentityKind::Engine,
                actor: 255,
                slot: 255,
                engine: next_engine,
                session: 0,
                nonce: next_engine,
                generation: 0,
            }));
        }
        let mut owners = [(Identity::default(), InputOwner::default()); 54];
        for (row, slot) in self.rows.owners().iter().zip(owners.iter_mut()) {
            let resource = resolve(row.resource)?;
            *slot = (
                resource,
                InputOwner {
                    resource: pack(resource),
                    resource_kind: row.kind,
                    after_disposition: row.after,
                    obligation: row.obligation.map(resolve).transpose()?.map(pack),
                    before_holder: row.before.map(pack),
                    after_holder: row.after_holder.map(resolve).transpose()?.map(pack),
                },
            );
        }
        let owners = &mut owners[..self.rows.owners().len()];
        owners.sort_unstable_by_key(|(id, _)| id.key());
        for (_, row) in owners.iter() {
            environment
                .owners
                .try_push(*row)
                .map_err(|_| AssemblyError::Capacity)?;
        }
        let mut obligations = [(Identity::default(), InputObligation::default()); 7];
        for (row, slot) in self.rows.obligations().iter().zip(obligations.iter_mut()) {
            let id = resolve(row.id)?;
            let raw = pack(id);
            let linked = environment
                .owners
                .as_slice()
                .iter()
                .filter(|o| o.obligation == Some(raw));
            let mut count = 0u16;
            let mut summary = OwnershipSummary::None;
            for owner in linked {
                let this = match owner.after_disposition {
                    OwnerDisposition::Held => OwnershipSummary::AllHeld,
                    OwnerDisposition::Transferred => OwnershipSummary::AllTransferred,
                    OwnerDisposition::Released => OwnershipSummary::AllReleased,
                };
                summary = match summary {
                    OwnershipSummary::None => this,
                    same if same == this => same,
                    _ => OwnershipSummary::Mixed,
                };
                count += 1;
            }
            *slot = (
                id,
                InputObligation {
                    obligation: raw,
                    known_decision: row.decision,
                    after_ownership_summary: summary,
                    associated_owner_count: count,
                },
            );
        }
        let obligations = &mut obligations[..self.rows.obligations().len()];
        obligations.sort_unstable_by_key(|(id, _)| id.key());
        for (_, row) in obligations.iter() {
            environment
                .obligations
                .try_push(*row)
                .map_err(|_| AssemblyError::Capacity)?;
        }
        mutate_recovery(&mut environment, env.recovery_mutation)?;
        out.environment = Some(environment);
        Ok(())
    }
}

/// A fixed value list used only while choosing Bind payload bytes.
#[derive(Clone, Copy, Default)]
struct Values {
    rows: [BindValue; 2],
    len: usize,
}
impl Values {
    fn as_slice(&self) -> &[BindValue] {
        &self.rows[..self.len]
    }
    fn one(value: BindValue) -> Self {
        Self {
            rows: [value, BindValue::default()],
            len: 1,
        }
    }
}
fn bind_value(tag: u8, bytes: &[u8]) -> BindValue {
    let mut value = BindValue {
        tag,
        byte_length: bytes.len() as u16,
        bytes: [0; 8],
    };
    value.bytes[..bytes.len()].copy_from_slice(bytes);
    value
}

/// Recovery negatives change only the obligation vector, after normal ordering, without re-sort.
fn mutate_recovery(environment: &mut Environment, mutation: u8) -> Step<()> {
    let obligations = &mut environment.obligations;
    let Some(&first) = obligations.as_slice().first() else {
        return Ok(());
    };
    match mutation {
        1 => {
            obligations.remove(0);
        }
        2 => {
            let mut extra = first;
            extra.obligation.0[8..10].copy_from_slice(&255u16.to_le_bytes());
            obligations
                .try_push(extra)
                .map_err(|_| AssemblyError::Capacity)?;
        }
        3 => {
            obligations.as_mut_slice()[0].obligation.0[4..6].copy_from_slice(&255u16.to_le_bytes());
        }
        4 => {
            let len = obligations.len();
            obligations
                .try_push(first)
                .map_err(|_| AssemblyError::Capacity)?;
            let rows = obligations.as_mut_slice();
            rows.copy_within(1..len, 2);
            rows[1] = first;
        }
        _ => {}
    }
    Ok(())
}

/// Execute/CommitAtomic/Finish fence classes: NCR, Unknown and PublicationPending.
fn fence_class(class: u8) -> (u8, u8, Decision) {
    match class {
        1 => (2, 1, Decision::NotCommittedRecoveryRequired),
        2 => (255, 7, Decision::Unknown),
        _ => (255, 8, Decision::Committed),
    }
}

/// Entry Cancelled/Timeout outcome tags of the fresh-context families.
fn entry_controls(api: Api) -> Option<(u8, u8)> {
    match api {
        Api::Prepare
        | Api::Execute
        | Api::CloseResult
        | Api::CloseStatement
        | Api::Begin
        | Api::SetAutocommit => Some((1, 2)),
        Api::BindAll | Api::CommitAtomic => Some((8, 9)),
        Api::Fetch => Some((3, 4)),
        Api::Rollback | Api::CloseSession => Some((2, 3)),
        _ => None,
    }
}

fn target_kind(api: Api) -> Option<TargetKind> {
    match api {
        Api::BindAll | Api::Execute | Api::CloseStatement | Api::InspectStatement => {
            Some(TargetKind::Statement)
        }
        Api::Fetch | Api::CloseResult | Api::ParkFetch => Some(TargetKind::Result),
        Api::SignalContext => Some(TargetKind::Context),
        _ => None,
    }
}

/// The requested caller row; presence and uniqueness precede its attached scope range.
fn caller(state: &State, actor: u8) -> Step<&ActorRecord> {
    let caller = unique(state.actors.as_slice(), |a| a.actor == actor)?;
    if !(1..=7).contains(&caller.engine.0) || !(1..=8).contains(&caller.session.0) {
        return Err(INVALID);
    }
    Ok(caller)
}

fn caller_identity(caller: &ActorRecord) -> Identity {
    Identity {
        actor: caller.actor,
        engine: caller.engine.0,
        session: caller.session.0,
        ..Identity::default()
    }
}
fn caller_owns(caller: &ActorRecord, id: Identity) -> bool {
    same_scope(id, caller_identity(caller))
}
fn same_scope(a: Identity, b: Identity) -> bool {
    a.actor == b.actor && a.engine == b.engine && a.session == b.session
}

/// Exactly one matching record; absence and duplicates are state errors.
fn unique<T>(rows: &[T], matches: impl Fn(&T) -> bool) -> Step<&T> {
    lookup(rows, matches)?.ok_or(INVALID)
}
/// At most one matching record; duplicates are state errors, absence is a normal answer.
fn lookup<T>(rows: &[T], matches: impl Fn(&T) -> bool) -> Step<Option<&T>> {
    let mut found = rows.iter().filter(|row| matches(row));
    let first = found.next();
    if found.next().is_some() {
        return Err(INVALID);
    }
    Ok(first)
}
fn transaction(state: &State, nonce: u16) -> Step<&TransactionRecord> {
    let t = unique(state.transactions.as_slice(), |t| t.id.nonce == nonce)?;
    if primitive(t.id, IdentityKind::Transaction) {
        Ok(t)
    } else {
        Err(INVALID)
    }
}
fn statement(state: &State, nonce: u16) -> Step<&StatementRecord> {
    let s = unique(state.issued_statements.as_slice(), |s| s.id.nonce == nonce)?;
    if primitive(s.id, IdentityKind::Statement) {
        Ok(s)
    } else {
        Err(INVALID)
    }
}
fn descriptor(state: &State, result: u16) -> Step<&Descriptor> {
    unique(state.private_descriptors.as_slice(), |d| {
        d.result.0 == result
    })
}
fn slot(state: &State, actor: u8, slot: u8) -> Step<&PreparedSlot> {
    unique(state.prepared_slots.as_slice(), |p| {
        p.actor == actor && p.slot == slot
    })
}
fn engine(state: &State, nonce: u16) -> Step<&EngineRecord> {
    let e = unique(state.engines.as_slice(), |e| e.id.nonce == nonce)?;
    if primitive(e.id, IdentityKind::Engine) {
        Ok(e)
    } else {
        Err(INVALID)
    }
}

/// The scope identity of a symbolic holder, used for new resources and obligations.
fn base(
    holders: &[Sym],
    index: u8,
    new_transaction: Option<Identity>,
    new_result: Option<Identity>,
) -> Step<Identity> {
    match holders.get(usize::from(index)).copied().ok_or(INVALID)? {
        Sym::Known(id) => Ok(id),
        Sym::NewTransaction => new_transaction.ok_or(INVALID),
        Sym::NewResult => new_result.ok_or(INVALID),
        _ => Err(INVALID),
    }
}
fn owner_scope(kind: IdentityKind, holder: Identity, nonce: u16) -> Identity {
    Identity {
        kind,
        actor: holder.actor,
        slot: 255,
        engine: holder.engine,
        session: holder.session,
        nonce,
        generation: 0,
    }
}

/// Structural check of one used namespace: primitives, ordinal and nonce duplicates, max-next.
fn guard(used: bool, state: &State, kind: IdentityKind) -> Step<u16> {
    if !used {
        return Ok(0);
    }
    let mut ids = [(Identity::default(), 0u16); 54];
    let mut len = 0;
    let mut push = |id: Identity, ordinal: u16| {
        ids[len] = (id, ordinal);
        len += 1;
    };
    match kind {
        IdentityKind::Engine => state.engines.as_slice().iter().for_each(|r| push(r.id, 1)),
        IdentityKind::Result => state
            .issued_results
            .as_slice()
            .iter()
            .for_each(|r| push(r.id, r.issue_ordinal)),
        IdentityKind::Context => state
            .contexts
            .as_slice()
            .iter()
            .for_each(|r| push(r.id, r.issue_ordinal)),
        IdentityKind::Transaction => state
            .transactions
            .as_slice()
            .iter()
            .for_each(|r| push(r.id, 1)),
        IdentityKind::Receipt => state.receipts.as_slice().iter().for_each(|r| push(r.id, 1)),
        IdentityKind::Resource => state.owners.as_slice().iter().for_each(|r| push(r.id, 1)),
        IdentityKind::Obligation => state
            .obligations
            .as_slice()
            .iter()
            .for_each(|r| push(r.id, 1)),
        IdentityKind::Fact => state.facts.as_slice().iter().for_each(|r| push(r.id, 1)),
        IdentityKind::Anchor => state
            .receipts
            .as_slice()
            .iter()
            .filter_map(|r| r.anchor)
            .for_each(|a| push(a, 1)),
        _ => return Err(INVALID),
    }
    let ids = &ids[..len];
    let ordered = matches!(kind, IdentityKind::Result | IdentityKind::Context);
    if ids
        .iter()
        .any(|&(id, ordinal)| !primitive(id, kind) || (ordered && !(1..=6).contains(&ordinal)))
    {
        return Err(INVALID);
    }
    let duplicate = |key: fn(&(Identity, u16)) -> u16| {
        ids.iter()
            .enumerate()
            .any(|(i, a)| ids[i + 1..].iter().any(|b| key(a) == key(b)))
    };
    if (ordered && duplicate(|r| r.1)) || duplicate(|r| r.0.nonce) {
        return Err(INVALID);
    }
    let maximum = ids.iter().map(|r| r.0.nonce).max().unwrap_or(0);
    let next = state.next_nonce[kind as usize - 1];
    if next != maximum + 1 {
        return Err(INVALID);
    }
    Ok(next)
}

/// Kind-specific identity envelope without engine/session table lookups.
fn primitive(id: Identity, kind: IdentityKind) -> bool {
    let cap = match kind {
        IdentityKind::Engine => 7,
        IdentityKind::Session => 8,
        IdentityKind::Resource => 54,
        IdentityKind::None => return false,
        _ => 6,
    };
    if id.kind != kind || id.nonce == 0 || id.nonce > cap || !(1..=7).contains(&id.engine) {
        return false;
    }
    match kind {
        IdentityKind::Engine => {
            id.actor == 255
                && id.slot == 255
                && id.session == 0
                && id.generation == 0
                && id.engine == id.nonce
        }
        IdentityKind::Fact | IdentityKind::Anchor => {
            id.actor == 255 && id.slot == 255 && id.session == 0 && id.generation == 0
        }
        _ => {
            id.actor <= 1
                && (1..=8).contains(&id.session)
                && match kind {
                    IdentityKind::Session => {
                        id.slot == 255 && id.generation == 0 && id.session == id.nonce
                    }
                    IdentityKind::Statement | IdentityKind::Result => {
                        id.slot <= 1 && (1..=12).contains(&id.generation)
                    }
                    _ => id.slot == 255 && id.generation == 0,
                }
        }
    }
}

/// Identity16 packing shared with the frozen selector: kind, actor, slot, 0, E, S, nonce, gen, 0.
fn pack(id: Identity) -> RawIdentity {
    let mut bytes = [0; 16];
    bytes[0] = id.kind as u8;
    bytes[1] = id.actor;
    bytes[2] = id.slot;
    bytes[4..6].copy_from_slice(&id.engine.to_le_bytes());
    bytes[6..8].copy_from_slice(&id.session.to_le_bytes());
    bytes[8..10].copy_from_slice(&id.nonce.to_le_bytes());
    bytes[10..12].copy_from_slice(&id.generation.to_le_bytes());
    RawIdentity(bytes)
}

/// Raw bytes as an identity only when every reserved byte is zero and the kind is known.
fn unpack(raw: RawIdentity) -> Option<Identity> {
    let b = raw.0;
    if b[3] != 0 || b[12..].iter().any(|&x| x != 0) {
        return None;
    }
    let word = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    Some(Identity {
        kind: IdentityKind::try_from(b[0]).ok()?,
        actor: b[1],
        slot: b[2],
        engine: word(4),
        session: word(6),
        nonce: word(8),
        generation: word(10),
    })
}
