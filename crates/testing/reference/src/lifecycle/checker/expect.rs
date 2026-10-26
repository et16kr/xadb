//! The expected observation of one edge and the response-row helpers of D4-LM-MODEL-001.
use super::super::schema::*;
use super::super::{
    Authenticated, Check, Complete, ContextEvent, Controls, EdgeClass, Exclusion,
    PublicTransaction, Response, Rows,
};
use super::delta::Delta;

/// Everything the specification fixes for one `(pre, input)` pair.
pub(super) struct Expect {
    /// Expected class unless `excluded` is non-zero.
    pub class: EdgeClass,
    /// Bit set of environment exclusions that justify an `Excluded` edge.
    pub excluded: u16,
    pub response: Option<Response>,
    pub context_event: Option<ContextEvent>,
    pub controls: Controls,
    pub consumed: u8,
    pub authenticated: Option<Authenticated>,
    /// Targets authenticated by a passed child gate, recorded on every later rejection.
    pub child: Option<Authenticated>,
    /// Transaction whose post record supplies the response decision fields.
    pub operated: Option<u16>,
    /// Actor whose post transaction supplies `public_transaction` and the decision fallback.
    pub caller: Option<u8>,
    /// Fact awaiting the post-state decision/delivery of its transaction.
    pub fact: Option<FactRecord>,
    pub delta: Delta,
}

/// Bit of one exclusion rule in `Expect::excluded`.
pub(super) fn bit(rule: Exclusion) -> u16 {
    1 << match rule {
        Exclusion::ContextFresh => 0,
        Exclusion::SignalNoDeadline => 1,
        Exclusion::BindN0 => 2,
        Exclusion::ExecuteCommit => 3,
        Exclusion::ReadOnlyCleanup => 4,
        Exclusion::CommitNoWrite => 5,
        Exclusion::CommitProgress => 6,
        Exclusion::CommitPreattemptGate => 7,
        Exclusion::RecoveryPre => 8,
        Exclusion::RecoverySet => 9,
        Exclusion::Envelope => 10,
        Exclusion::ResumeWork => 11,
        Exclusion::FencedClose => 12,
        Exclusion::PeerFence => 13,
    }
}

impl Expect {
    pub(super) fn new(pre: &State) -> Self {
        Self {
            class: EdgeClass::Admitted,
            excluded: 0,
            response: None,
            context_event: None,
            controls: Controls::default(),
            consumed: 0,
            authenticated: None,
            child: None,
            operated: None,
            caller: None,
            fact: None,
            delta: Delta::new(pre),
        }
    }

    pub(super) fn exclude(&mut self, rule: Exclusion) {
        self.excluded |= bit(rule);
    }

    pub(super) fn exclude_if(&mut self, condition: bool, rule: Exclusion) {
        if condition {
            self.exclude(rule);
        }
    }

    /// Fill the decision fields and the pending fact from the expected post-state.
    pub(super) fn finish(&mut self, pre: &State) {
        let caller_txn = self
            .caller
            .and_then(|a| self.delta.view_actor(pre, a))
            .and_then(|a| a.transaction);
        let source = self.operated.or(caller_txn.map(|t| t.0));
        let source = source.and_then(|t| self.delta.view_txn(pre, t));
        if let Some(r) = self.response.as_mut() {
            r.public_transaction = match caller_txn.and_then(|t| self.delta.view_txn(pre, t.0)) {
                None => PublicTransaction::Idle,
                Some(t) => match t.life {
                    TransactionLife::Active => PublicTransaction::Active,
                    TransactionLife::Ended => PublicTransaction::Ended,
                    TransactionLife::RecoveryPending => PublicTransaction::RecoveryPending,
                },
            };
            if let Some(t) = source {
                r.transaction_decision = t.decision;
                r.client_knowledge = t.client_knowledge;
                r.commit_progress = t.progress;
                r.completion = t
                    .receipt
                    .and_then(|n| self.delta.view_receipt(pre, n.0))
                    .map(|x| x.id);
                r.delivery = t.delivery;
            } else {
                r.transaction_decision = Decision::NotApplicable;
                r.client_knowledge = ClientKnowledge::NoDecisionObserved;
                r.commit_progress = CommitProgress::None;
                r.completion = None;
                r.delivery = Delivery::NotApplicable;
            }
        }
        if let Some(mut f) = self.fact.take() {
            if let Some(t) = f.transaction.and_then(|t| self.delta.view_txn(pre, t.0)) {
                f.decision = t.decision;
                f.delivery = t.delivery;
            }
            // The fact record is unique per edge; capacity one cannot overflow here.
            let _ = self.delta.add_fact(f);
        }
    }
}

/// Code/phase/effect/certainty/retry/causes of one response row.
#[derive(Clone, Copy)]
pub(super) struct Row {
    pub code: ReturnCode,
    pub phase: Phase,
    pub effect: StatementEffect,
    pub certainty: Certainty,
    pub retry: Retry,
    pub engine_cause: EngineCause,
    pub origin_cause: OriginCause,
}

impl Row {
    pub(super) fn diagnostic(self) -> Diagnostic {
        Diagnostic {
            code: self.code,
            phase: self.phase,
            effect: self.effect,
            certainty: self.certainty,
            retry: self.retry,
            engine_cause: self.engine_cause,
            origin_cause: self.origin_cause,
        }
    }
    pub(super) fn from_diagnostic(d: Diagnostic) -> Self {
        Self {
            code: d.code,
            phase: d.phase,
            effect: d.effect,
            certainty: d.certainty,
            retry: d.retry,
            engine_cause: d.engine_cause,
            origin_cause: d.origin_cause,
        }
    }
}

/// Successful row with the given phase and statement effect.
pub(super) fn ok(phase: Phase, effect: StatementEffect) -> Row {
    Row {
        code: ReturnCode::Ok,
        phase,
        effect,
        certainty: Certainty::Known,
        retry: Retry::NotApplicable,
        engine_cause: EngineCause::None,
        origin_cause: OriginCause::None,
    }
}

/// Error row with `Known` certainty.
pub(super) fn error(
    code: ReturnCode,
    phase: Phase,
    effect: StatementEffect,
    retry: Retry,
    origin_cause: OriginCause,
) -> Row {
    Row {
        code,
        phase,
        effect,
        certainty: Certainty::Known,
        retry,
        engine_cause: EngineCause::None,
        origin_cause,
    }
}

/// Entry-gate rejection rows (handle/state, busy, fenced, entry control).
pub(super) fn entry(code: ReturnCode, engine_cause: EngineCause) -> Row {
    let (retry, origin) = match code {
        ReturnCode::Busy | ReturnCode::ResultBusy | ReturnCode::SlotBusy => {
            (Retry::Operation, OriginCause::None)
        }
        ReturnCode::EngineFenced => (Retry::AfterRecovery, OriginCause::None),
        ReturnCode::Cancelled => (Retry::Operation, OriginCause::Cancelled),
        ReturnCode::Timeout => (Retry::Operation, OriginCause::Timeout),
        _ => (Retry::Never, OriginCause::None),
    };
    let mut row = error(code, Phase::Entry, StatementEffect::NoEffect, retry, origin);
    if code == ReturnCode::EngineFenced {
        row.engine_cause = engine_cause;
    }
    row
}

/// Control-plane responses without a certainty.
pub(super) fn control_plane(code: ReturnCode) -> Row {
    let retry = match code {
        ReturnCode::NoPendingOperation
        | ReturnCode::ContinuationKind
        | ReturnCode::InvalidContext => Retry::Never,
        _ => Retry::NotApplicable,
    };
    Row {
        code,
        phase: Phase::NotApplicable,
        effect: StatementEffect::NoEffect,
        certainty: Certainty::NotApplicable,
        retry,
        engine_cause: EngineCause::None,
        origin_cause: OriginCause::None,
    }
}

/// Response for one row; every non-success code carries the same diagnostic.
pub(super) fn response(row: Row) -> Response {
    let success = matches!(
        row.code,
        ReturnCode::Ok | ReturnCode::Eof | ReturnCode::IgnoredCompleted
    );
    Response {
        code: row.code,
        phase: row.phase,
        statement_effect: row.effect,
        certainty: row.certainty,
        retry: row.retry,
        engine_cause: row.engine_cause,
        origin_cause: row.origin_cause,
        diagnostic: (!success).then(|| row.diagnostic()),
        ..Response::default()
    }
}

/// Fetch-like row count and completion.
pub(super) fn fetch_shape(r: &mut Response, rows: u8, complete: bool) {
    r.rows = Rows::Count(rows);
    r.complete = if complete {
        Complete::Yes
    } else {
        Complete::No
    };
}

/// Controls of a gate that rejected before the entry checks.
pub(super) fn unchecked() -> Controls {
    Controls::default()
}

/// Controls of a fresh call that passed both entry checks.
pub(super) fn entry_passed(observed_cancel: bool, observed_deadline: bool) -> Controls {
    Controls {
        entry_cancel: Check::No,
        entry_deadline: Check::No,
        observed_cancel,
        observed_deadline,
    }
}
