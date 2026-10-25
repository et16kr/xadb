//! CloseSession, ReopenSession, InspectSession, SignalContext and fresh-engine Recovery.
use super::super::schema::*;
use super::super::{EdgeClass, Exclusion};
use super::CheckFailure;
use super::expect::{self, Expect, Row};
use super::fence::{self, Fence};
use super::gates::{self, Cx};
use super::ident::{self, unpack};
use super::owners::{self, EnvPlan, FactSpec, FenceClass, ObligationPlan};
use super::shared;
use super::statement::{envelope, recovery_list};

pub(super) fn close_session(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 3 {
        return Err(CheckFailure::Input("close session outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    exp.operated = f.actor.transaction.map(|t| t.0);
    if f.actor.shell == Shell::Closed {
        let r = expect::response(expect::ok(Phase::Close, StatementEffect::NoEffect));
        return gates::admit_cached(cx, exp, &f, r);
    }
    if f.actor.parked_context.is_some() {
        return gates::reject_gate(cx, exp, &f, ReturnCode::Busy, false);
    }
    if gates::entry_gate(cx, exp, &f, false)? {
        return Ok(());
    }
    if o > 1 {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let pre = cx.pre;
    let t = f
        .actor
        .transaction
        .and_then(|t| ident::txn(pre, t.0))
        .copied();
    let rw = t.is_some_and(|t| t.access == Access::ReadWrite);
    let admitting = f.admitting();
    let engine = f.actor.engine.0;
    exp.exclude_if(o == 1 && !admitting, Exclusion::FencedClose);
    exp.exclude_if(o == 1 && !rw, Exclusion::ReadOnlyCleanup);
    let mut plan = EnvPlan::new(pre, FactKind::SessionCleanup, engine);
    plan.transaction = t.map(|t| t.id);
    let slots = [
        ident::slot(pre, f.actor.actor, 0).copied(),
        ident::slot(pre, f.actor.actor, 1).copied(),
    ];
    if o == 0 {
        for s in slots.iter().flatten() {
            if let Some(st) = s.statement {
                owners::release_local(&mut plan, pre, st.0, s.descriptor_result.map(|r| r.0))?;
            }
        }
        if admitting {
            plan.cleanup = 1;
            plan.commit_class = if rw { 0 } else { 255 };
            if let Some(t) = t.filter(|_| rw) {
                owners::release_rw(&mut plan, pre, t.id.nonce)?;
            }
        } else {
            plan.cleanup = 3;
            for ob in pre.obligations.as_slice() {
                if ob.engine.0 != engine
                    || ob.origin_session != f.actor.session
                    || ob.settlement != Settlement::Outstanding
                {
                    continue;
                }
                let txn = ident::txn(pre, ob.transaction.0)
                    .ok_or(CheckFailure::PreState("obligation transaction"))?
                    .id;
                plan.obligation(ObligationPlan {
                    id: ob.id,
                    txn,
                    new: false,
                    known: ob.known_decision,
                    reason: None,
                    settle: None,
                })?;
                for owner in owners::live_of(
                    pre,
                    owners::txn_holder(txn.nonce),
                    ident::is_obligation_owner,
                ) {
                    plan.handoff(pre, owner, ob.id)?;
                }
            }
        }
    } else {
        plan.cleanup = 2;
        plan.commit_class = 1;
        let cause = t.map(|t| t.id);
        owners::census(
            &mut plan,
            pre,
            engine,
            cause,
            false,
            FenceClass::UndoIncomplete,
        )?;
    }
    plan.link_existing(pre, None)?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = 2;
    let ctx = f.context_ref();
    let fact = FactRef(plan.fact.nonce);
    let actor = f.actor.actor;
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::CloseSession,
            outcome: o,
            context: Some(ctx),
            cleanup: shared::cleanup_of(2, plan.cleanup),
            next_work: NextWork::NotConsumed,
        },
    )?;
    if o == 1 {
        // Owner decision: a failed close keeps the session Open, like Rollback Incomplete.
        // Only the first-fence effects (every current public result of the engine, the
        // caller's included, retires as EngineFence) and the cause transaction change; there is
        // no local release or SessionClose retirement, and the caller keeps its now
        // RecoveryPending transaction. A later fenced CloseSession Complete releases/hands off.
        if let Some(t) = t {
            fence::first_fence(
                cx,
                exp,
                &plan,
                Fence {
                    engine,
                    cause: t.id.nonce,
                    class: FenceClass::UndoIncomplete,
                    context: ctx,
                },
            )?;
            fence::cause_txn(cx, exp, t.id.nonce, FenceClass::UndoIncomplete, fact)?;
        }
    } else {
        if let Some(t) = t
            && (admitting || t.access == Access::ReadOnly)
        {
            shared::whole_abort(cx, exp, actor, t.id.nonce, fact)?;
        }
        close_children(cx, exp, &slots, ctx)?;
        let session = exp.delta.session(pre, f.actor.session.0)?;
        session.lifecycle = Shell::Closed;
        session.closing_context = Some(ctx);
        let a = exp.delta.actor(pre, actor)?;
        a.shell = Shell::Closed;
        a.transaction = None;
        a.active_public_result = None;
        a.parked_context = None;
    }
    let mut r = if o == 0 {
        expect::response(expect::ok(Phase::Close, StatementEffect::NoEffect))
    } else {
        expect::response(fence::row(FenceClass::UndoIncomplete, Phase::Close))
    };
    r.recovery_obligations = recovery_list(&plan);
    exp.class = EdgeClass::Admitted;
    exp.response = Some(r);
    exp.controls = expect::entry_passed(false, false);
    gates::register(cx, exp, &f, ContextData::Completed)
}

/// SessionClose retirement of the caller's current children; both slots become Vacant.
fn close_children(
    cx: &Cx,
    exp: &mut Expect,
    slots: &[Option<PreparedSlot>; 2],
    ctx: ContextRef,
) -> Result<(), CheckFailure> {
    let pre = cx.pre;
    for s in slots.iter().flatten() {
        if let Some(st) = s.statement {
            let row = exp.delta.statement(pre, st.0)?;
            if row.life != StatementLife::Retired {
                row.life = StatementLife::Retired;
                row.retirement = Retirement::SessionClose;
                row.retirement_context = Some(ctx);
                let slot = exp.delta.slot(pre, s.actor, s.slot)?;
                slot.next_statement_generation = slot.next_statement_generation.wrapping_add(1);
            }
        }
        if let Some(r) = s.descriptor_result {
            shared::retire_result(cx, exp, r.0, Retirement::SessionClose, ctx)?;
            exp.delta.remove_descriptor(r)?;
        }
        if let Some(b) = s.binding {
            exp.delta.binding(pre, b.0)?.life = BindingLife::Released;
        }
        let slot = exp.delta.slot(pre, s.actor, s.slot)?;
        slot.state = SlotState::Vacant;
        slot.statement = None;
        slot.binding = None;
        slot.descriptor_result = None;
    }
    Ok(())
}

pub(super) fn reopen(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 1 {
        return Err(CheckFailure::Input("reopen outcome"));
    }
    let actor = gates::caller(cx)?;
    exp.caller = Some(actor.actor);
    let pre = cx.pre;
    let attached = *ident::engine(pre, actor.engine.0)
        .ok_or(CheckFailure::PreState("missing attached engine"))?;
    let reject = |exp: &mut Expect, code: ReturnCode| {
        exp.class = EdgeClass::Rejected;
        exp.response = Some(expect::response(expect::entry(code, attached.first_cause)));
    };
    if actor.shell == Shell::Open {
        reject(exp, ReturnCode::SessionState);
        return Ok(());
    }
    if actor.parked_context.is_some() {
        reject(exp, ReturnCode::Busy);
        return Ok(());
    }
    let current = pre.current_engine;
    let allowed = if o == 0 {
        actor.engine == current && attached.admission == EngineAdmission::Admitting
    } else {
        attached.admission == EngineAdmission::RecoveryConfirmedRetired
            && attached.successor_engine == Some(current)
    };
    if !allowed {
        reject(exp, ReturnCode::EngineFenced);
        return Ok(());
    }
    let n = ident::next(pre, IdentityKind::Session);
    let id = ident::ident(IdentityKind::Session, actor.actor, 255, current.0, n, n, 0);
    exp.delta.add_session(SessionRecord {
        id,
        engine: current,
        actor: actor.actor,
        lifecycle: Shell::Open,
        reopen_kind: if o == 0 {
            ReopenKind::HealthySameEngine
        } else {
            ReopenKind::RecoveredFreshEngine
        },
        previous_session: Some(actor.session),
        closing_context: None,
    })?;
    exp.delta.bump(IdentityKind::Session, 1);
    let a = exp.delta.actor(pre, actor.actor)?;
    a.shell = Shell::Open;
    a.autocommit = Autocommit::On;
    a.session = SessionRef(n);
    a.engine = current;
    a.transaction = None;
    a.parked_context = None;
    a.active_public_result = None;
    for slot in 0..2 {
        let s = exp.delta.slot(pre, actor.actor, slot)?;
        s.state = SlotState::Vacant;
        s.next_statement_generation = 1;
        s.next_result_generation = 1;
        s.statement = None;
        s.binding = None;
        s.descriptor_result = None;
    }
    exp.class = EdgeClass::NoContextIssue;
    exp.response = Some(expect::response(expect::ok(
        Phase::NotApplicable,
        StatementEffect::NoEffect,
    )));
    Ok(())
}

pub(super) fn inspect_session(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let actor = gates::caller(cx)?;
    exp.caller = Some(actor.actor);
    exp.class = EdgeClass::NoEffect;
    exp.response = Some(expect::response(expect::ok(
        Phase::NotApplicable,
        StatementEffect::NoEffect,
    )));
    Ok(())
}

pub(super) fn signal(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 1 {
        return Err(CheckFailure::Input("signal outcome"));
    }
    let actor = gates::caller(cx)?;
    exp.caller = Some(actor.actor);
    let control = |exp: &mut Expect, code: ReturnCode, class: EdgeClass| {
        exp.class = class;
        exp.response = Some(expect::response(expect::control_plane(code)));
    };
    let target = cx
        .input
        .target
        .and_then(unpack)
        .filter(|id| id.kind == IdentityKind::Context && id.actor == actor.actor);
    let record = target.and_then(|id| ident::context(cx.pre, id.nonce).filter(|c| c.id == id));
    let Some(c) = record.copied() else {
        control(exp, ReturnCode::InvalidContext, EdgeClass::NoEffect);
        return Ok(());
    };
    let (mut p, fetch) = match c.data {
        ContextData::Completed => {
            control(exp, ReturnCode::IgnoredCompleted, EdgeClass::NoEffect);
            return Ok(());
        }
        ContextData::ParkedFetch(p) => (p, true),
        ContextData::ParkedCommit(p) => (p, false),
    };
    let changed = if o == 0 {
        let changed = !p.cancel_supplied_or_signalled;
        p.cancel_supplied_or_signalled = true;
        changed
    } else {
        if p.original_deadline == Deadline::Absent {
            exp.exclude(Exclusion::SignalNoDeadline);
            return Ok(());
        }
        let changed = !p.deadline_reached;
        p.deadline_reached = true;
        changed
    };
    if changed {
        exp.delta.context(cx.pre, c.id.nonce)?.data = if fetch {
            ContextData::ParkedFetch(p)
        } else {
            ContextData::ParkedCommit(p)
        };
        control(exp, ReturnCode::Ok, EdgeClass::NoContextIssue);
    } else {
        control(exp, ReturnCode::Ok, EdgeClass::NoEffect);
    }
    Ok(())
}

pub(super) fn recovery(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    if i.actor != 255 || i.outcome > 4 {
        return Err(CheckFailure::Input("recovery actor or outcome"));
    }
    let pre = cx.pre;
    let old = *ident::engine(pre, pre.current_engine.0)
        .ok_or(CheckFailure::PreState("missing current engine"))?;
    let e = old.id.nonce;
    let mut plan = EnvPlan::new(pre, FactKind::RecoveryConfirmed, e);
    plan.cleanup = 4;
    let fresh = ident::next(pre, IdentityKind::Engine);
    let fresh_id = ident::engine_wide(IdentityKind::Engine, fresh, fresh);
    plan.fresh_engine = Some(fresh_id);
    let mut outstanding: Records<ObligationRecord, 8> = Records::default();
    for ob in pre.obligations.as_slice() {
        if ob.engine.0 == e && ob.settlement == Settlement::Outstanding {
            outstanding
                .try_push(*ob)
                .map_err(|_| CheckFailure::CheckerCapacity("recovery obligation table full"))?;
        }
    }
    outstanding
        .as_mut_slice()
        .sort_unstable_by_key(|o| o.id.key());
    for ob in outstanding.as_slice() {
        let txn = ident::txn(pre, ob.transaction.0)
            .ok_or(CheckFailure::PreState("obligation transaction"))?
            .id;
        plan.obligation(ObligationPlan {
            id: ob.id,
            txn,
            new: false,
            known: ob.known_decision,
            reason: None,
            settle: Some(Settlement::RecoveryConfirmed),
        })?;
        for owner in owners::live_of(
            pre,
            owners::txn_holder(txn.nonce),
            ident::is_obligation_owner,
        ) {
            plan.release(pre, owner)?;
        }
    }
    // ENV-RECOVERY-PRE (owner decision): old engine Fenced, every shell Closed, no parked call,
    // a fresh engine within the profile. A fact/fresh-engine identity mismatch falls under both
    // ENV-RECOVERY-PRE and the general envelope rule (no priority), so either label is
    // accepted. Any other header byte is Envelope only; rows and mutations are RecoverySet.
    let env = i.environment.as_ref();
    let identity_mismatch = env.is_some_and(|x| !plan.identities_match(x));
    exp.exclude_if(
        old.admission != EngineAdmission::Fenced
            || pre
                .actors
                .as_slice()
                .iter()
                .any(|a| a.shell != Shell::Closed || a.parked_context.is_some())
            || fresh > 7
            || identity_mismatch,
        Exclusion::RecoveryPre,
    );
    exp.exclude_if(
        identity_mismatch || env.is_none_or(|x| !plan.header_matches(x)),
        Exclusion::Envelope,
    );
    exp.exclude_if(
        i.outcome != 0 || env.is_some_and(|x| !plan.rows_match(x)),
        Exclusion::RecoverySet,
    );
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = 18;
    let fact = FactRef(plan.fact.nonce);
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::RecoveryConfirmedFreshEngine,
            outcome: 0,
            context: None,
            cleanup: Cleanup::RecoveryConfirmed,
            next_work: NextWork::NotConsumed,
        },
    )?;
    let row = exp.delta.engine(pre, e)?;
    row.admission = EngineAdmission::RecoveryConfirmedRetired;
    row.successor_engine = Some(EngineRef(fresh));
    row.recovery_fact = Some(fact);
    exp.delta.add_engine(EngineRecord {
        id: fresh_id,
        admission: EngineAdmission::Admitting,
        first_cause: EngineCause::None,
        known_decision: Decision::NotApplicable,
        cause_transaction: None,
        first_fence_context: None,
        successor_engine: None,
        recovery_fact: None,
    })?;
    exp.delta.current_engine = EngineRef(fresh);
    for ob in outstanding.as_slice() {
        // Owner decision (revised): the recovery fact is engine history without a transaction,
        // so no completion_fact changes; the obligation's recovery_fact records the end.
        exp.delta.txn(pre, ob.transaction.0)?.life = TransactionLife::Ended;
    }
    exp.class = EdgeClass::NoContextIssue;
    let mut r = expect::response(Row {
        certainty: Certainty::NotApplicable,
        ..expect::ok(Phase::Recovery, StatementEffect::NoEffect)
    });
    r.recovery_obligations = recovery_list(&plan);
    exp.response = Some(r);
    Ok(())
}
