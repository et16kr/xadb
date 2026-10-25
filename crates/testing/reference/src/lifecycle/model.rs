//! Abstract lifecycle model transition (D4-LM-MODEL-001 draft).
//!
//! One resolved input applied to one pre-state yields an edge observation and a successor.
//! The model is a test oracle candidate only: it does not implement SQL, engine durability or
//! exploration, and an independent edge checker must not reuse it to build expectations.
mod common;
mod edit;
mod result;
mod session;
mod statement;

use super::input_schema::{RawIdentity, ResolvedInput};
use super::schema::{
    ActorRecord, Api, Certainty, ClientKnowledge, CommitProgress, ContextStatus, Decision,
    Delivery, Diagnostic, EngineAdmission, EngineCause, Identity, IdentityKind, OriginCause, Phase,
    Records, Retry, ReturnCode, State, StatementEffect, TransactionLife,
};
use edit::Editor;

/// A pre-state that the model cannot interpret, or a full fixed table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// A reference needed by this transition is dangling, duplicated or out of scope.
    InvalidState,
    /// A fixed bounded table cannot hold a new record.
    Capacity,
}

/// Environment exclusion rule; the successor is the unchanged pre-state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exclusion {
    /// The fresh caller context envelope is not the actor's next identity.
    ContextFresh,
    /// DeadlineReached signalled to a parked call without an original deadline.
    SignalNoDeadline,
    /// An N0 binding cannot carry a typed/NULL/range/UTF-8/limit value error.
    BindN0,
    /// Commit-only Execute classes outside an Idle ON implicit RW DML.
    ExecuteCommit,
    /// Undo failure claimed for a read-only or transaction-free cleanup.
    ReadOnlyCleanup,
    /// Commit classes impossible for a no-write transaction.
    CommitNoWrite,
    /// A commit progress fact not allowed from the current progress.
    CommitProgress,
    /// A pre-attempt commit fact behind a pending control or a peer fence.
    CommitPreattemptGate,
    /// Recovery preconditions: old engine Fenced, both shells Closed, no parked call.
    RecoveryPre,
    /// Recovery set differs from the exact outstanding obligations.
    RecoverySet,
    /// Supplied rows disagree with the pre-state they claim to change.
    Envelope,
    /// Resume work and cleanup that cannot occur together without a pending control.
    ResumeWork,
    /// A close on an already fenced engine supplies only local release and exact handoff.
    FencedClose,
    /// A pre-attempt commit under a peer fence can end only through the exact NCR handoff.
    PeerFence,
}

/// Edge classification of one raw slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EdgeClass {
    /// Admitted fresh call, including its value and execution errors.
    #[default]
    Admitted,
    /// Rejected before admission; the manager frame is preserved.
    Rejected,
    /// A context-free call that leaves the full state unchanged.
    NoEffect,
    /// A context-free call that changes state without issuing a context.
    NoContextIssue,
    /// A matched continuation completing its original parked context.
    Continuation,
    /// Environment exclusion; no response and no successor change.
    Excluded(Exclusion),
}

/// A control check that was not reached, or its observed value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Check {
    /// The gate did not consult this control.
    #[default]
    NotChecked,
    /// Consulted and absent.
    No,
    /// Consulted and present.
    Yes,
}

/// Entry checks and controls actually observed by this call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    /// Entry cancellation check.
    pub entry_cancel: Check,
    /// Entry deadline check.
    pub entry_deadline: Check,
    /// Cancellation observed anywhere in this call.
    pub observed_cancel: bool,
    /// Deadline observed anywhere in this call.
    pub observed_deadline: bool,
}

/// The context record touched by this edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextEvent {
    /// Fresh or reused context identity.
    pub identity: Identity,
    /// Status before the edge; None for a fresh identity.
    pub before: Option<ContextStatus>,
    /// Status after the edge.
    pub after: ContextStatus,
}

/// Row count of a fetch-like response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rows {
    /// Not a fetch-like response.
    #[default]
    NotApplicable,
    /// Rows returned by this call.
    Count(u8),
}

/// Completion flag of a fetch-like response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Complete {
    /// Not a fetch-like response.
    #[default]
    NotApplicable,
    /// The result is not complete.
    No,
    /// The result reached EOF.
    Yes,
}

/// Public transaction state reported to the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PublicTransaction {
    /// No current transaction.
    #[default]
    Idle,
    /// Active current transaction.
    Active,
    /// Current transaction awaiting recovery.
    RecoveryPending,
    /// Current transaction already ended but still referenced until Finish.
    Ended,
}

/// Model response of one call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    /// Model return code.
    pub code: ReturnCode,
    /// Phase reported with the code.
    pub phase: Phase,
    /// Statement-local effect.
    pub statement_effect: StatementEffect,
    /// Certainty reported to the caller.
    pub certainty: Certainty,
    /// Retry category.
    pub retry: Retry,
    /// Engine fence cause.
    pub engine_cause: EngineCause,
    /// Operation cause.
    pub origin_cause: OriginCause,
    /// Bounded diagnostic of an error response.
    pub diagnostic: Option<Diagnostic>,
    /// Fetch-like row count.
    pub rows: Rows,
    /// Fetch-like completion.
    pub complete: Complete,
    /// DML affected rows.
    pub affected_count: Option<u8>,
    /// Statement handle returned by Prepare.
    pub returned_statement: Option<Identity>,
    /// Result handle returned by Execute.
    pub returned_result: Option<Identity>,
    /// Caller transaction state after the call.
    pub public_transaction: PublicTransaction,
    /// Decision of the operated transaction.
    pub transaction_decision: Decision,
    /// Client knowledge of the operated transaction.
    pub client_knowledge: ClientKnowledge,
    /// Commit progress of the operated transaction.
    pub commit_progress: CommitProgress,
    /// Supplied completion receipt delivered or retained for the operated transaction.
    pub completion: Option<Identity>,
    /// Receipt delivery of the operated transaction.
    pub delivery: Delivery,
    /// Obligations supplied and applied by this call, in identity order.
    pub recovery_obligations: Records<Identity, 7>,
}

/// Typed targets authenticated before a rejection; None before child authentication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Authenticated {
    /// Authenticated statement.
    pub statement: Option<Identity>,
    /// Authenticated result.
    pub result: Option<Identity>,
    /// Selected transaction.
    pub transaction: Option<Identity>,
}

/// Canonical edge observation of one raw slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edge {
    /// Classification.
    pub class: EdgeClass,
    /// Response; None only for an excluded edge.
    pub response: Option<Response>,
    /// Context issued, parked or completed by this edge.
    pub context_event: Option<ContextEvent>,
    /// Entry and observed controls.
    pub controls: Controls,
    /// Applied environment domains: Work1, Cleanup2, Receipt4, CommitProgress8, RecoverySet16.
    pub consumed_parts: u8,
    /// Present for child-authentication rejections only.
    pub authenticated: Option<Authenticated>,
}

/// Internal failure: a model error, or a supplied envelope that disagrees with the pre-state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Model(ModelError),
    Envelope,
}
type Step<T> = Result<T, Failure>;
const INVALID: Failure = Failure::Model(ModelError::InvalidState);
const CAPACITY: Failure = Failure::Model(ModelError::Capacity);
const ENVELOPE: Failure = Failure::Envelope;

/// Apply `input` to `pre`, writing the successor and the edge observation.
///
/// # Errors
///
/// Returns `InvalidState` when a needed pre-state reference is broken and `Capacity` when a
/// fixed table cannot hold a new record. Neither is an API rejection or an exclusion.
pub fn transition(
    pre: &State,
    input: &ResolvedInput,
    post: &mut State,
    edge: &mut Edge,
) -> Result<(), ModelError> {
    post.copy_from(pre);
    *edge = Edge::default();
    let outcome = {
        let mut run = Run {
            edit: Editor::new(post),
            input,
            edge,
            context: None,
            fact: None,
            new_result: None,
            authenticated: None,
        };
        match run.dispatch() {
            Ok(Flow::Done) => run.edit.canonicalize().map(|()| Flow::Done),
            other => other,
        }
    };
    let rule = match outcome {
        Ok(Flow::Done) => return Ok(()),
        Ok(Flow::Exclude(rule)) => rule,
        Err(Failure::Envelope) => Exclusion::Envelope,
        Err(Failure::Model(error)) => {
            // A model error publishes nothing: the workspace returns to the pre-state.
            post.copy_from(pre);
            *edge = Edge::default();
            return Err(error);
        }
    };
    post.copy_from(pre);
    *edge = Edge {
        class: EdgeClass::Excluded(rule),
        ..Edge::default()
    };
    Ok(())
}

/// Control flow of one transition: finished, or excluded with a rule.
enum Flow {
    Done,
    Exclude(Exclusion),
}

/// One transition run over the successor workspace.
struct Run<'a> {
    edit: Editor<'a>,
    input: &'a ResolvedInput,
    edge: &'a mut Edge,
    /// The call's context identity, fresh or continued.
    context: Option<Identity>,
    /// The consumed fact identity, once a fact is recorded.
    fact: Option<Identity>,
    /// The result handle issued by an admitted Execute Success Select.
    new_result: Option<Identity>,
    /// Targets authenticated by the child gate, observed on a later rejection (v5 §1).
    authenticated: Option<Authenticated>,
}

impl Run<'_> {
    fn dispatch(&mut self) -> Step<Flow> {
        match self.input.api {
            Api::Prepare => self.prepare(),
            Api::BindAll => self.bind(),
            Api::Execute => self.execute(),
            Api::Fetch => self.fetch(false),
            Api::ParkFetch => self.fetch(true),
            Api::CloseResult => self.close_result(),
            Api::CloseStatement => self.close_statement(),
            Api::Begin => self.begin(),
            Api::SetAutocommit => self.set_autocommit(),
            Api::CommitAtomic => self.commit_atomic(),
            Api::BeginCommit => self.begin_commit(),
            Api::Rollback => self.rollback(),
            Api::CloseSession => self.close_session(),
            Api::ReopenSession => self.reopen(),
            Api::InspectStatement => self.inspect_statement(),
            Api::InspectSession => self.inspect_session(),
            Api::SignalContext => self.signal(),
            Api::ResumeFetch => self.resume(),
            Api::CommitFact => self.commit_fact(),
            Api::FinishCommit => self.finish(),
            Api::RecoveryConfirmedFreshEngine => self.recovery(),
        }
    }

    fn caller(&self) -> Step<ActorRecord> {
        let actor = self.input.actor;
        self.edit
            .state()
            .actors
            .as_slice()
            .iter()
            .find(|a| a.actor == actor)
            .copied()
            .ok_or(INVALID)
    }

    /// Gate 0: the supplied fresh context must be the caller's next identity.
    fn fresh_context(&mut self) -> Step<Option<Exclusion>> {
        let caller = self.caller()?;
        let expected = Identity {
            kind: IdentityKind::Context,
            actor: caller.actor,
            slot: 255,
            engine: caller.engine.0,
            session: caller.session.0,
            nonce: self.edit.next(IdentityKind::Context),
            generation: 0,
        };
        if self.input.context.and_then(unpack) != Some(expected) || expected.nonce > 6 {
            return Ok(Some(Exclusion::ContextFresh));
        }
        self.context = Some(expected);
        Ok(None)
    }

    /// Gates 2 and 3 of a fresh call: Closed shell, parked Busy, then shared fence.
    fn session_gates(&mut self, fence_exempt: bool) -> Step<Option<ReturnCode>> {
        let caller = self.caller()?;
        if caller.shell == super::schema::Shell::Closed {
            return Ok(Some(ReturnCode::SessionState));
        }
        if caller.parked_context.is_some() {
            return Ok(Some(ReturnCode::Busy));
        }
        if !fence_exempt
            && self.edit.engine(caller.engine.0)?.admission != EngineAdmission::Admitting
        {
            return Ok(Some(ReturnCode::EngineFenced));
        }
        Ok(None)
    }

    /// Gate 5: entry Cancelled before entry Timeout.
    fn entry_controls(&mut self) -> Option<ReturnCode> {
        let cancel = self.input.cancel;
        self.edge.controls.entry_cancel = if cancel { Check::Yes } else { Check::No };
        if cancel {
            self.edge.controls.observed_cancel = true;
            return Some(ReturnCode::Cancelled);
        }
        let reached = self.input.deadline_reached;
        self.edge.controls.entry_deadline = if reached { Check::Yes } else { Check::No };
        if reached {
            self.edge.controls.observed_deadline = true;
            return Some(ReturnCode::Timeout);
        }
        None
    }

    /// Record a fresh call that ends before admission, preserving the manager frame.
    fn reject(&mut self, code: ReturnCode) -> Step<Flow> {
        self.complete_fresh()?;
        self.edge.class = EdgeClass::Rejected;
        if let Some(targets) = self.authenticated {
            self.edge.authenticated = Some(targets);
        }
        let phase = Phase::Entry;
        let (retry, origin) = match code {
            ReturnCode::Busy | ReturnCode::ResultBusy | ReturnCode::SlotBusy => {
                (Retry::Operation, OriginCause::None)
            }
            ReturnCode::EngineFenced => (Retry::AfterRecovery, OriginCause::None),
            ReturnCode::Cancelled => (Retry::Operation, OriginCause::Cancelled),
            ReturnCode::Timeout => (Retry::Operation, OriginCause::Timeout),
            _ => (Retry::Never, OriginCause::None),
        };
        let mut response = self.error(code, phase, StatementEffect::NoEffect, retry, origin);
        self.fenced_cause(&mut response)?;
        if self.fetch_like() {
            response.rows = Rows::Count(0);
            response.complete = Complete::No;
        }
        // Decision fields follow the transaction the authenticated child selects (owner
        // decision 2026-10-25); without one, the caller's current transaction.
        let selected = self
            .authenticated
            .and_then(|a| a.transaction)
            .map(|t| t.nonce);
        self.respond(response, selected)
    }

    /// `EngineFenced` reports the attached engine's first cause (owner decision 2026-10-25).
    fn fenced_cause(&self, response: &mut Response) -> Step<()> {
        if response.code != ReturnCode::EngineFenced {
            return Ok(());
        }
        let cause = self.edit.engine(self.caller()?.engine.0)?.first_cause;
        response.engine_cause = cause;
        if let Some(d) = response.diagnostic.as_mut() {
            d.engine_cause = cause;
        }
        Ok(())
    }

    /// Child authentication passed: a later state/control rejection records these targets
    /// (owner decision 2026-10-25). The transaction is the call's wire-v2 §4 selection.
    fn authenticated_child(
        &mut self,
        statement: Option<Identity>,
        result: Option<Identity>,
        transaction: Option<u16>,
    ) -> Step<()> {
        let transaction = match transaction {
            Some(t) => Some(self.edit.transaction(t)?.id),
            None => None,
        };
        self.authenticated = Some(Authenticated {
            statement,
            result,
            transaction,
        });
        Ok(())
    }

    /// Child authentication failure: typed targets are recorded as None.
    fn reject_handle(&mut self) -> Step<Flow> {
        self.edge.authenticated = Some(Authenticated::default());
        self.reject(ReturnCode::InvalidHandle)
    }

    fn fetch_like(&self) -> bool {
        matches!(
            self.input.api,
            Api::Fetch | Api::ParkFetch | Api::ResumeFetch
        )
    }

    /// Register the fresh context as Completed and advance the caller counter.
    fn complete_fresh(&mut self) -> Step<()> {
        let context = self.context.ok_or(INVALID)?;
        self.edit.add_context(context, None)?;
        self.edge.context_event = Some(ContextEvent {
            identity: context,
            before: None,
            after: ContextStatus::Completed,
        });
        Ok(())
    }

    fn error(
        &self,
        code: ReturnCode,
        phase: Phase,
        effect: StatementEffect,
        retry: Retry,
        origin: OriginCause,
    ) -> Response {
        let diagnostic = Diagnostic {
            code,
            phase,
            effect,
            certainty: Certainty::Known,
            retry,
            engine_cause: EngineCause::None,
            origin_cause: origin,
        };
        Response {
            code,
            phase,
            statement_effect: effect,
            certainty: Certainty::Known,
            retry,
            origin_cause: origin,
            diagnostic: Some(diagnostic),
            ..Response::default()
        }
    }

    fn success(&self, phase: Phase, effect: StatementEffect) -> Response {
        Response {
            code: ReturnCode::Ok,
            phase,
            statement_effect: effect,
            certainty: Certainty::Known,
            retry: Retry::NotApplicable,
            ..Response::default()
        }
    }

    /// Finish the response from the operated transaction's post record and publish it.
    fn respond(&mut self, mut response: Response, operated: Option<u16>) -> Step<Flow> {
        let caller = self.caller().ok();
        let current = caller.and_then(|c| c.transaction).map(|t| t.0);
        response.public_transaction = match current {
            None => PublicTransaction::Idle,
            Some(t) => match self.edit.transaction(t)?.life {
                TransactionLife::Active => PublicTransaction::Active,
                TransactionLife::RecoveryPending => PublicTransaction::RecoveryPending,
                TransactionLife::Ended => PublicTransaction::Ended,
            },
        };
        if let Some(t) = operated.or(current) {
            let record = self.edit.transaction(t)?;
            response.transaction_decision = record.decision;
            response.client_knowledge = record.client_knowledge;
            response.commit_progress = record.progress;
            response.delivery = record.delivery;
            response.completion = match record.receipt {
                Some(r) => Some(self.edit.receipt(r.0)?.id),
                None => None,
            };
        }
        if let (Some(_), Some(env)) = (self.fact, self.input.environment.as_ref()) {
            for row in env.obligations.as_slice() {
                if let Some(id) = unpack(row.obligation) {
                    response
                        .recovery_obligations
                        .try_push(id)
                        .map_err(|_| CAPACITY)?;
                }
            }
        }
        self.edge.response = Some(response);
        Ok(Flow::Done)
    }
}

/// Raw bytes as an identity when every reserved byte is zero and the kind is known.
pub(super) fn unpack(raw: RawIdentity) -> Option<Identity> {
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
