//! Fetch, ParkFetch, CloseResult and ResumeFetch.
use super::super::schema::*;
use super::super::{Check, ContextEvent, Controls, EdgeClass, Exclusion, Response};
use super::CheckFailure;
use super::expect::{self, Expect, Row};
use super::fence::{self, Fence};
use super::gates::{self, Cx, Fresh};
use super::ident;
use super::owners::{self, EnvPlan, FactSpec, FenceClass};
use super::shared;
use super::statement::{envelope, recovery_list};

/// Applied work of a fetch-like call after admission.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Work {
    Rows,
    Buffer,
    Eof,
    /// Atomic terminal control: `true` for Cancelled, `false` for Timeout.
    Terminal(bool),
    ReadError,
    Conflict,
    /// Undo incomplete after the given consumed next-work.
    Incomplete(NextWork),
    /// Pending continuation control with Complete cleanup (`true` = Cancelled).
    PendingComplete(bool),
    PendingIncomplete,
}

impl Work {
    fn consumed(self) -> u8 {
        match self {
            Self::Rows | Self::Buffer => 1,
            Self::Terminal(_) | Self::PendingComplete(_) | Self::PendingIncomplete => 2,
            Self::Eof | Self::ReadError | Self::Conflict | Self::Incomplete(_) => 3,
        }
    }
    fn next_work(self) -> NextWork {
        match self {
            Self::Rows => NextWork::Rows,
            Self::Buffer => NextWork::RowBufferTooSmall,
            Self::Eof => NextWork::Eof,
            Self::ReadError => NextWork::ReadError,
            Self::Conflict => NextWork::Conflict,
            Self::Incomplete(w) => w,
            Self::Terminal(_) | Self::PendingComplete(_) | Self::PendingIncomplete => {
                NextWork::NotConsumed
            }
        }
    }
    fn incomplete(self) -> bool {
        matches!(self, Self::Incomplete(_) | Self::PendingIncomplete)
    }
}

/// The fetch target: result, descriptor and its transaction.
#[derive(Clone, Copy)]
struct Target {
    actor: u8,
    result: Identity,
    descriptor: Descriptor,
    txn: Option<TransactionRecord>,
}

fn terminal_row(code: ReturnCode, origin: OriginCause) -> Row {
    expect::error(
        code,
        Phase::Fetch,
        StatementEffect::CursorReleased,
        Retry::Operation,
        origin,
    )
}

/// Expected Env1 of fetch-like work (wire-v2 §3 table, assembler §6/§7).
fn plan_work(
    cx: &Cx,
    plan: &mut EnvPlan,
    t: &Target,
    work: Work,
    engine: u16,
) -> Result<(), CheckFailure> {
    let pre = cx.pre;
    let rw = t.txn.is_some_and(|x| x.access == Access::ReadWrite);
    plan.transaction = t.txn.map(|x| x.id);
    match work {
        Work::Rows | Work::Buffer => {}
        Work::Eof | Work::Terminal(_) | Work::ReadError | Work::PendingComplete(_) => {
            owners::release_cursor(plan, pre, t.result.nonce)?;
        }
        Work::Conflict => {
            if let Some(x) = t.txn {
                if rw {
                    owners::release_rw(plan, pre, x.id.nonce)?;
                }
                for r in shared::open_results_of(pre, x.id.nonce).as_slice() {
                    owners::release_cursor(plan, pre, *r)?;
                }
            }
            owners::release_cursor(plan, pre, t.result.nonce)?;
        }
        Work::Incomplete(_) | Work::PendingIncomplete => {
            let cause = t.txn.map(|x| x.id);
            owners::census(plan, pre, engine, cause, false, FenceClass::UndoIncomplete)?;
        }
    }
    plan.link_existing(pre, None)
}

/// Apply admitted fetch-like work; returns the response.
fn apply_work(
    cx: &Cx,
    exp: &mut Expect,
    plan: &EnvPlan,
    t: &Target,
    work: Work,
    spec: FactSpec,
    engine: u16,
) -> Result<Response, CheckFailure> {
    let pre = cx.pre;
    let context = spec.context.unwrap_or(ContextRef(0));
    exp.consumed = work.consumed();
    let fact = FactRef(plan.fact.nonce);
    plan.apply(pre, exp, spec)?;
    let r = t.result.nonce;
    let close_cursor = |exp: &mut Expect, state: ResultState, terminal: Option<Diagnostic>| {
        let d = exp.delta.descriptor(pre, r)?;
        d.state = state;
        d.terminal = terminal;
        let s = exp.delta.slot(pre, t.result.actor, t.result.slot)?;
        if s.descriptor_result == Some(ResultRef(r)) && s.state == SlotState::ResultOpen {
            s.state = SlotState::ReadyBound;
        }
        let a = exp.delta.view_actor(pre, t.actor);
        if a.and_then(|a| a.active_public_result) == Some(ResultRef(r)) {
            exp.delta.actor(pre, t.actor)?.active_public_result = None;
        }
        shared::end_read_only(cx, exp, t.actor, t.descriptor.transaction, fact)
    };
    let mut response = match work {
        Work::Rows => {
            let d = exp.delta.descriptor(pre, r)?;
            d.position = d.position.wrapping_add(1);
            let mut x = expect::response(expect::ok(Phase::Fetch, StatementEffect::NoEffect));
            expect::fetch_shape(&mut x, 1, false);
            x
        }
        Work::Buffer => {
            let mut x = expect::response(expect::error(
                ReturnCode::RowBufferTooSmall,
                Phase::Fetch,
                StatementEffect::NoEffect,
                Retry::Operation,
                OriginCause::None,
            ));
            expect::fetch_shape(&mut x, 0, false);
            x
        }
        Work::Eof => {
            close_cursor(exp, ResultState::Eof, None)?;
            let row = Row {
                code: ReturnCode::Eof,
                ..expect::ok(Phase::Fetch, StatementEffect::CursorReleased)
            };
            let mut x = expect::response(row);
            expect::fetch_shape(&mut x, 0, true);
            x
        }
        Work::Terminal(cancel) | Work::PendingComplete(cancel) => {
            let row = if cancel {
                terminal_row(ReturnCode::Cancelled, OriginCause::Cancelled)
            } else {
                terminal_row(ReturnCode::Timeout, OriginCause::Timeout)
            };
            close_cursor(exp, ResultState::Error, Some(row.diagnostic()))?;
            let mut x = expect::response(row);
            expect::fetch_shape(&mut x, 0, false);
            x
        }
        Work::ReadError => {
            let row = terminal_row(ReturnCode::ReadError, OriginCause::ReadError);
            close_cursor(exp, ResultState::Error, Some(row.diagnostic()))?;
            let mut x = expect::response(row);
            expect::fetch_shape(&mut x, 0, false);
            x
        }
        Work::Conflict => {
            if let Some(x) = t.txn {
                for open in shared::open_results_of(pre, x.id.nonce).as_slice() {
                    shared::discard_open_result(cx, exp, *open, context)?;
                }
                shared::whole_abort(cx, exp, t.actor, x.id.nonce, fact)?;
            }
            let mut x = expect::response(expect::error(
                ReturnCode::Conflict,
                Phase::Fetch,
                StatementEffect::CursorReleased,
                Retry::WholeTransactionDecision,
                OriginCause::Conflict,
            ));
            expect::fetch_shape(&mut x, 0, false);
            x
        }
        Work::Incomplete(_) | Work::PendingIncomplete => {
            let class = FenceClass::UndoIncomplete;
            let row = fence::row(class, Phase::Fetch);
            if let Some(x) = t.txn {
                if fence::admitting(pre, engine) {
                    fence::first_fence(
                        cx,
                        exp,
                        plan,
                        Fence {
                            engine,
                            cause: x.id.nonce,
                            class,
                            context,
                        },
                    )?;
                }
                fence::cause_txn(cx, exp, x.id.nonce, class, fact)?;
            }
            // The cause cursor keeps its private descriptor, position and owners
            // (D4-LM-MODEL-001 Fetch row: "Incomplete RW fence"); only the fence blocks it.
            let mut x = expect::response(row);
            expect::fetch_shape(&mut x, 0, false);
            x
        }
    };
    response.recovery_obligations = recovery_list(plan);
    Ok(response)
}

fn fetch_work(outcome: u8) -> Option<Work> {
    Some(match outcome {
        0 => Work::Rows,
        1 => Work::Eof,
        2 => Work::Buffer,
        5 => Work::Terminal(true),
        6 => Work::Terminal(false),
        7 => Work::ReadError,
        8 => Work::Conflict,
        9 => Work::Incomplete(NextWork::NotConsumed),
        _ => return None,
    })
}

/// Fetch-family gates up to the API state; `Some` carries the admitted target.
fn fetch_gates(cx: &Cx, exp: &mut Expect, f: &Fresh) -> Result<Option<Target>, CheckFailure> {
    let Some((result, descriptor)) = gates::auth_result(cx.pre, &f.actor, cx.input.target) else {
        gates::reject_handle(cx, exp, f, true)?;
        return Ok(None);
    };
    gates::child_result(cx.pre, exp, &result, &descriptor);
    exp.operated = descriptor.transaction.map(|t| t.0);
    if let Some(code) = gates::frame_gate(f, false) {
        gates::reject_gate(cx, exp, f, code, true)?;
        return Ok(None);
    }
    match descriptor.state {
        ResultState::Closed => {
            gates::reject_gate(cx, exp, f, ReturnCode::HandleClosed, true)?;
            return Ok(None);
        }
        ResultState::Eof => {
            let row = Row {
                code: ReturnCode::Eof,
                ..expect::ok(Phase::Fetch, StatementEffect::NoEffect)
            };
            let mut r = expect::response(row);
            expect::fetch_shape(&mut r, 0, true);
            gates::admit_cached(cx, exp, f, r)?;
            return Ok(None);
        }
        ResultState::Error => {
            let terminal = descriptor.terminal.ok_or(CheckFailure::PreState(
                "error descriptor without diagnostic",
            ))?;
            let mut r = expect::response(Row::from_diagnostic(terminal));
            r.diagnostic = Some(terminal);
            expect::fetch_shape(&mut r, 0, false);
            gates::admit_cached(cx, exp, f, r)?;
            return Ok(None);
        }
        ResultState::Open => {}
    }
    if gates::entry_gate(cx, exp, f, true)? {
        return Ok(None);
    }
    Ok(Some(Target {
        actor: f.actor.actor,
        result: result.id,
        descriptor,
        txn: descriptor
            .transaction
            .and_then(|t| ident::txn(cx.pre, t.0))
            .copied(),
    }))
}

pub(super) fn fetch(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 9 {
        return Err(CheckFailure::Input("fetch outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    let Some(t) = fetch_gates(cx, exp, &f)? else {
        return Ok(());
    };
    let Some(work) = fetch_work(o) else {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    };
    let rw = t.txn.is_some_and(|x| x.access == Access::ReadWrite);
    exp.exclude_if(work.incomplete() && !rw, Exclusion::ReadOnlyCleanup);
    let engine = f.actor.engine.0;
    let mut plan = EnvPlan::new(cx.pre, FactKind::FetchWorkOrCleanup, engine);
    plan.work_kind = 2;
    plan.work_class = o;
    plan.cleanup = match o {
        0 | 2 => 255,
        9 => 2,
        _ => 1,
    };
    plan.commit_class = match o {
        8 if rw => 0,
        9 if rw => 1,
        _ => 255,
    };
    plan_work(cx, &mut plan, &t, work, engine)?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    let spec = FactSpec {
        api: Api::Fetch,
        outcome: o,
        context: Some(f.context_ref()),
        cleanup: shared::cleanup_of(work.consumed(), plan.cleanup),
        next_work: work.next_work(),
    };
    let r = apply_work(cx, exp, &plan, &t, work, spec, engine)?;
    exp.class = EdgeClass::Admitted;
    exp.response = Some(r);
    exp.controls =
        expect::entry_passed(work == Work::Terminal(true), work == Work::Terminal(false));
    gates::register(cx, exp, &f, ContextData::Completed)
}

pub(super) fn park(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    if cx.input.outcome > 1 {
        return Err(CheckFailure::Input("park outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    let Some(t) = fetch_gates(cx, exp, &f)? else {
        return Ok(());
    };
    let pre = cx.pre;
    let parked = ParkedContext {
        api: Api::ParkFetch,
        original_deadline: cx.input.original_deadline,
        cancel_supplied_or_signalled: cx.input.cancel,
        deadline_reached: cx.input.deadline_reached,
        cancel_observed_at_completion: false,
        deadline_observed_at_completion: false,
        completion_phase: CompletionPhase::NotCompleted,
        statement: Some(t.descriptor.statement),
        result: Some(ResultRef(t.result.nonce)),
        transaction: t.descriptor.transaction,
    };
    let ctx = f.context_ref();
    exp.delta.actor(pre, f.actor.actor)?.parked_context = Some(ctx);
    if let Some(x) = t.descriptor.transaction {
        exp.delta.txn(pre, x.0)?.parked_context = Some(ctx);
    }
    exp.class = EdgeClass::Admitted;
    exp.response = Some(expect::response(expect::ok(
        Phase::Fetch,
        StatementEffect::NoEffect,
    )));
    exp.controls = expect::entry_passed(false, false);
    gates::register(cx, exp, &f, ContextData::ParkedFetch(parked))
}

pub(super) fn close_result(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    if i.outcome > 2 {
        return Err(CheckFailure::Input("close result outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    let Some((result, descriptor)) = gates::auth_result(cx.pre, &f.actor, i.target) else {
        return gates::reject_handle(cx, exp, &f, false);
    };
    gates::child_result(cx.pre, exp, &result, &descriptor);
    exp.operated = descriptor.transaction.map(|t| t.0);
    if let Some(code) = gates::frame_gate(&f, false) {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    if descriptor.state == ResultState::Closed {
        let r = expect::response(expect::ok(Phase::Close, StatementEffect::NoEffect));
        return gates::admit_cached(cx, exp, &f, r);
    }
    if gates::entry_gate(cx, exp, &f, false)? {
        return Ok(());
    }
    if i.outcome != 0 {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let pre = cx.pre;
    let mut plan = EnvPlan::new(pre, FactKind::LocalCleanup, f.actor.engine.0);
    plan.cleanup = 1;
    plan.transaction = descriptor
        .transaction
        .and_then(|t| ident::txn(pre, t.0))
        .map(|t| t.id);
    owners::release_cursor(&mut plan, pre, result.id.nonce)?;
    plan.link_existing(pre, None)?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = 2;
    let ctx = f.context_ref();
    let fact = FactRef(plan.fact.nonce);
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::CloseResult,
            outcome: 0,
            context: Some(ctx),
            cleanup: Cleanup::Complete,
            next_work: NextWork::NotConsumed,
        },
    )?;
    let was_open = descriptor.state == ResultState::Open;
    let d = exp.delta.descriptor(pre, result.id.nonce)?;
    d.state = ResultState::Closed;
    d.terminal = None;
    if was_open {
        let s = exp.delta.slot(pre, result.id.actor, result.id.slot)?;
        if s.state == SlotState::ResultOpen {
            s.state = SlotState::ReadyBound;
        }
        if f.actor.active_public_result == Some(ResultRef(result.id.nonce)) {
            exp.delta.actor(pre, f.actor.actor)?.active_public_result = None;
        }
        shared::end_read_only(cx, exp, f.actor.actor, descriptor.transaction, fact)?;
    }
    let effect = if was_open {
        StatementEffect::CursorReleased
    } else {
        StatementEffect::NoEffect
    };
    let mut r = expect::response(expect::ok(Phase::Close, effect));
    r.recovery_obligations = recovery_list(&plan);
    exp.class = EdgeClass::Admitted;
    exp.response = Some(r);
    exp.controls = expect::entry_passed(false, false);
    gates::register(cx, exp, &f, ContextData::Completed)
}

/// Matched continuation of the caller, or the control-plane response for a mismatch.
pub(super) enum Pending {
    Matched(ContextRecord, ParkedContext),
    Done,
}

/// Continuation gate shared by Resume, CommitFact and Finish.
pub(super) fn pending(
    cx: &Cx,
    exp: &mut Expect,
    actor: &ActorRecord,
    fetch: bool,
) -> Result<Pending, CheckFailure> {
    exp.caller = Some(actor.actor);
    let parked = actor
        .parked_context
        .and_then(|c| ident::context(cx.pre, c.0))
        .copied();
    let control = |exp: &mut Expect, code: ReturnCode| {
        exp.class = EdgeClass::NoEffect;
        exp.response = Some(expect::response(expect::control_plane(code)));
        Pending::Done
    };
    let Some(c) = parked else {
        return Ok(control(exp, ReturnCode::NoPendingOperation));
    };
    let payload = match (c.data, fetch) {
        (ContextData::ParkedFetch(p), true) | (ContextData::ParkedCommit(p), false) => p,
        (ContextData::Completed, _) => return Ok(control(exp, ReturnCode::NoPendingOperation)),
        _ => return Ok(control(exp, ReturnCode::ContinuationKind)),
    };
    if cx.input.context != Some(ident::pack(c.id)) {
        exp.exclude(Exclusion::Envelope);
        return Ok(Pending::Done);
    }
    exp.operated = payload.transaction.map(|t| t.0);
    Ok(Pending::Matched(c, payload))
}

/// Complete a matched continuation: Parked → Completed and parked links cleared.
pub(super) fn complete(
    cx: &Cx,
    exp: &mut Expect,
    actor: u8,
    c: &ContextRecord,
    p: &ParkedContext,
) -> Result<(), CheckFailure> {
    let pre = cx.pre;
    exp.delta.context(pre, c.id.nonce)?.data = ContextData::Completed;
    exp.delta.actor(pre, actor)?.parked_context = None;
    if let Some(t) = p.transaction {
        exp.delta.txn(pre, t.0)?.parked_context = None;
    }
    exp.class = EdgeClass::Continuation;
    exp.context_event = Some(ContextEvent {
        identity: c.id,
        before: Some(c.data.status()),
        after: ContextStatus::Completed,
    });
    Ok(())
}

pub(super) fn resume(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 9 {
        return Err(CheckFailure::Input("resume outcome"));
    }
    let actor = gates::caller(cx)?;
    let (c, p) = match pending(cx, exp, &actor, true)? {
        Pending::Matched(c, p) => (c, p),
        Pending::Done => return Ok(()),
    };
    let pre = cx.pre;
    let engine = actor.engine.0;
    if !fence::admitting(pre, engine) {
        complete(cx, exp, actor.actor, &c, &p)?;
        let first_cause = ident::engine(pre, engine).map_or(EngineCause::None, |e| e.first_cause);
        let mut r = expect::response(Row {
            code: ReturnCode::EngineFenced,
            phase: Phase::Fetch,
            effect: StatementEffect::NoEffect,
            certainty: Certainty::Known,
            retry: Retry::AfterRecovery,
            engine_cause: first_cause,
            origin_cause: OriginCause::None,
        });
        expect::fetch_shape(&mut r, 0, false);
        exp.response = Some(r);
        return Ok(());
    }
    let result = p
        .result
        .and_then(|r| ident::result(pre, r.0))
        .copied()
        .ok_or(CheckFailure::PreState("parked fetch result"))?;
    let descriptor = *ident::descriptor(pre, result.id.nonce)
        .ok_or(CheckFailure::PreState("parked fetch descriptor"))?;
    let t = Target {
        actor: actor.actor,
        result: result.id,
        descriptor,
        txn: p.transaction.and_then(|x| ident::txn(pre, x.0)).copied(),
    };
    let next = o / 2;
    let incomplete = o % 2 == 1;
    let cancel = p.cancel_supplied_or_signalled;
    let deadline = p.deadline_reached;
    let signalled = cancel || deadline;
    let rw = t.txn.is_some_and(|x| x.access == Access::ReadWrite);
    let work = if signalled {
        if incomplete {
            Work::PendingIncomplete
        } else {
            Work::PendingComplete(cancel)
        }
    } else {
        match (next, incomplete) {
            (0, false) => Work::Rows,
            (1, false) => Work::Eof,
            (2, false) => Work::ReadError,
            (3, false) => Work::Conflict,
            (4, false) => Work::Buffer,
            (2, true) => Work::Incomplete(NextWork::ReadError),
            (3, true) => Work::Incomplete(NextWork::Conflict),
            (1, true) => {
                // Eof + Incomplete without a pending control: the assembler supplies the
                // ordinary Incomplete census; the input is excluded after admission.
                exp.exclude(Exclusion::ResumeWork);
                Work::Incomplete(NextWork::NotConsumed)
            }
            _ => {
                // Rows/RowBufferTooSmall + Incomplete without a pending control: owner rows
                // stay empty and the raw cleanup byte is kept (assembler contract).
                exp.exclude(Exclusion::ResumeWork);
                Work::Rows
            }
        }
    };
    exp.exclude_if(incomplete && !rw, Exclusion::ReadOnlyCleanup);
    let mut plan = EnvPlan::new(pre, FactKind::FetchWorkOrCleanup, engine);
    plan.work_kind = 3;
    plan.work_class = next;
    plan.cleanup = 1 + o % 2;
    plan.commit_class = if rw && incomplete {
        1
    } else if rw && !signalled && next == 3 {
        0
    } else {
        255
    };
    plan_work(cx, &mut plan, &t, work, engine)?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    let spec = FactSpec {
        api: Api::ResumeFetch,
        outcome: o,
        context: Some(ContextRef(c.id.nonce)),
        cleanup: shared::cleanup_of(work.consumed(), plan.cleanup),
        next_work: work.next_work(),
    };
    let r = apply_work(cx, exp, &plan, &t, work, spec, engine)?;
    complete(cx, exp, actor.actor, &c, &p)?;
    exp.response = Some(r);
    exp.controls = Controls {
        entry_cancel: Check::NotChecked,
        entry_deadline: Check::NotChecked,
        observed_cancel: signalled && cancel,
        observed_deadline: signalled && !cancel,
    };
    Ok(())
}
