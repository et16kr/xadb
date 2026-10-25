//! Shared engine fence (design-v3 §2, identity-transition, owner v5 §4).
//!
//! The first fence marks the engine Fenced with the cause class, retires every current public
//! result of that engine once (EngineFence, counter +1), keeps private descriptors and owners
//! (`blocked_by_fence`), clears public Open results, and moves every census transaction to
//! RecoveryPending without changing a peer's progress or decision.
use super::super::schema::*;
use super::CheckFailure;
use super::expect::Expect;
use super::gates::Cx;
use super::ident;
use super::owners::{EnvPlan, FenceClass};

/// One first fence: engine, cause transaction, class and the causing call's context.
#[derive(Clone, Copy)]
pub(super) struct Fence {
    pub engine: u16,
    pub cause: u16,
    pub class: FenceClass,
    pub context: ContextRef,
}

/// Apply a first fence.
pub(super) fn first_fence(
    cx: &Cx,
    exp: &mut Expect,
    plan: &EnvPlan,
    fence: Fence,
) -> Result<(), CheckFailure> {
    let Fence {
        engine,
        cause,
        class,
        context,
    } = fence;
    let pre = cx.pre;
    let e = exp.delta.engine(pre, engine)?;
    e.admission = EngineAdmission::Fenced;
    e.first_cause = class.engine_cause();
    e.known_decision = class.decision();
    e.cause_transaction = Some(TransactionRef(cause));
    e.first_fence_context = Some(context);
    for r in pre.issued_results.as_slice() {
        if r.id.engine != engine {
            continue;
        }
        let current = exp.delta.view_result(pre, r.id.nonce).unwrap_or(*r);
        if current.retirement != Retirement::None {
            continue;
        }
        let row = exp.delta.result(pre, r.id.nonce)?;
        row.retirement = Retirement::EngineFence;
        row.public_generation_live = false;
        row.retirement_context = Some(context);
        let slot = exp.delta.slot(pre, r.id.actor, r.id.slot)?;
        slot.next_result_generation = slot.next_result_generation.wrapping_add(1);
        if slot.descriptor_result == Some(ResultRef(r.id.nonce))
            && slot.state == SlotState::ResultOpen
        {
            slot.state = SlotState::ReadyBound;
        }
        if exp.delta.view_descriptor(pre, r.id.nonce).is_some() {
            let d = exp.delta.descriptor(pre, r.id.nonce)?;
            d.public_generation_live = false;
            d.blocked_by_fence = true;
        }
    }
    for a in pre.actors.as_slice() {
        if a.engine.0 == engine {
            let current = exp.delta.view_actor(pre, a.actor).unwrap_or(*a);
            if current.active_public_result.is_some() {
                exp.delta.actor(pre, a.actor)?.active_public_result = None;
            }
        }
    }
    for o in plan.obligations.as_slice() {
        if o.new {
            exp.delta.txn(pre, o.txn.nonce)?.life = TransactionLife::RecoveryPending;
        }
    }
    Ok(())
}

/// Cause transaction of a fence or cause update: class decision, knowledge and RecoveryPending.
pub(super) fn cause_txn(
    cx: &Cx,
    exp: &mut Expect,
    txn: u16,
    class: FenceClass,
    fact: FactRef,
) -> Result<(), CheckFailure> {
    let t = exp.delta.txn(cx.pre, txn)?;
    t.life = TransactionLife::RecoveryPending;
    t.decision = class.decision();
    t.client_knowledge = class.knowledge();
    t.completion_fact = Some(fact);
    Ok(())
}

/// Response row of a fence/cause class at `phase`.
pub(super) fn row(class: FenceClass, phase: Phase) -> super::expect::Row {
    let (code, certainty) = match class {
        FenceClass::UndoIncomplete => (ReturnCode::RecoveryRequired, Certainty::Known),
        FenceClass::Unknown | FenceClass::PublicationPending => {
            (ReturnCode::ResultUnknown, Certainty::Unknown)
        }
    };
    super::expect::Row {
        code,
        phase,
        effect: StatementEffect::RecoveryRequired,
        certainty,
        retry: Retry::AfterRecovery,
        engine_cause: class.engine_cause(),
        origin_cause: class.origin_cause(),
    }
}

/// Whether the caller's attached engine still admits new work.
pub(super) fn admitting(pre: &State, engine: u16) -> bool {
    ident::engine(pre, engine).is_some_and(|e| e.admission == EngineAdmission::Admitting)
}
