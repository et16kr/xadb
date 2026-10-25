//! Small state edits shared by several families.
use super::super::schema::*;
use super::CheckFailure;
use super::expect::Expect;
use super::gates::Cx;
use super::ident;
use super::owners::EnvPlan;

/// Prospective transaction identity in the caller's current scope.
pub(super) fn prospective_txn(pre: &State, actor: &ActorRecord) -> Identity {
    ident::ident(
        IdentityKind::Transaction,
        actor.actor,
        255,
        actor.engine.0,
        actor.session.0,
        ident::next(pre, IdentityKind::Transaction),
        0,
    )
}

/// Insert a new Active transaction and attach it to the caller.
pub(super) fn add_txn(
    cx: &Cx,
    exp: &mut Expect,
    actor: &ActorRecord,
    id: Identity,
    origin: TransactionOrigin,
    access: Access,
) -> Result<(), CheckFailure> {
    exp.delta.add_txn(TransactionRecord {
        id,
        origin,
        access,
        life: TransactionLife::Active,
        prior_successful_write: false,
        progress: CommitProgress::None,
        decision: if access == Access::ReadWrite {
            Decision::NotAttempted
        } else {
            Decision::NotApplicable
        },
        client_knowledge: ClientKnowledge::NoDecisionObserved,
        delivery: Delivery::NotApplicable,
        birth_session: actor.session,
        receipt: None,
        parked_context: None,
        obligation: None,
        completion_fact: None,
    })?;
    exp.delta.bump(IdentityKind::Transaction, 1);
    exp.delta.actor(cx.pre, actor.actor)?.transaction = Some(TransactionRef(id.nonce));
    Ok(())
}

/// Detach `txn` from the caller when it is the caller's current transaction.
pub(super) fn detach(cx: &Cx, exp: &mut Expect, actor: u8, txn: u16) -> Result<(), CheckFailure> {
    let current = exp
        .delta
        .view_actor(cx.pre, actor)
        .and_then(|a| a.transaction);
    if current == Some(TransactionRef(txn)) {
        exp.delta.actor(cx.pre, actor)?.transaction = None;
    }
    Ok(())
}

/// End an Active implicit read-only transaction after its cursor is released.
pub(super) fn end_read_only(
    cx: &Cx,
    exp: &mut Expect,
    actor: u8,
    txn: Option<TransactionRef>,
    fact: FactRef,
) -> Result<(), CheckFailure> {
    let Some(t) = txn else { return Ok(()) };
    let Some(view) = exp.delta.view_txn(cx.pre, t.0) else {
        return Ok(());
    };
    if view.origin == TransactionOrigin::ImplicitReadOnly && view.life == TransactionLife::Active {
        let row = exp.delta.txn(cx.pre, t.0)?;
        row.life = TransactionLife::Ended;
        row.completion_fact = Some(fact);
        detach(cx, exp, actor, t.0)?;
    }
    Ok(())
}

/// Whole abort: RW transactions become Aborted/KnownAborted, read-only ones simply end.
pub(super) fn whole_abort(
    cx: &Cx,
    exp: &mut Expect,
    actor: u8,
    txn: u16,
    fact: FactRef,
) -> Result<(), CheckFailure> {
    let row = exp.delta.txn(cx.pre, txn)?;
    row.life = TransactionLife::Ended;
    row.completion_fact = Some(fact);
    if row.access == Access::ReadWrite {
        row.decision = Decision::Aborted;
        row.client_knowledge = ClientKnowledge::KnownAborted;
    }
    detach(cx, exp, actor, txn)
}

/// Public Open descriptors whose transaction is `txn`.
pub(super) fn open_results_of(pre: &State, txn: u16) -> Records<u16, 4> {
    let mut out = Records::default();
    for d in pre.private_descriptors.as_slice() {
        if d.transaction == Some(TransactionRef(txn))
            && d.state == ResultState::Open
            && d.public_generation_live
        {
            let _ = out.try_push(d.result.0);
        }
    }
    out
}

/// Retire a current result token once (counter +1) and record the reason/context.
pub(super) fn retire_result(
    cx: &Cx,
    exp: &mut Expect,
    result: u16,
    reason: Retirement,
    context: ContextRef,
) -> Result<(), CheckFailure> {
    let Some(view) = exp.delta.view_result(cx.pre, result) else {
        return Err(CheckFailure::PreState("dangling result"));
    };
    if view.retirement != Retirement::None {
        return Ok(());
    }
    let r = exp.delta.result(cx.pre, result)?;
    r.retirement = reason;
    r.public_generation_live = false;
    r.retirement_context = Some(context);
    let slot = exp.delta.slot(cx.pre, view.id.actor, view.id.slot)?;
    slot.next_result_generation = slot.next_result_generation.wrapping_add(1);
    Ok(())
}

/// Retire and discard an Open public result (Rollback / whole-abort Complete).
pub(super) fn discard_open_result(
    cx: &Cx,
    exp: &mut Expect,
    result: u16,
    context: ContextRef,
) -> Result<(), CheckFailure> {
    let view = exp
        .delta
        .view_result(cx.pre, result)
        .ok_or(CheckFailure::PreState("dangling result"))?;
    retire_result(cx, exp, result, Retirement::Rollback, context)?;
    exp.delta.remove_descriptor(ResultRef(result))?;
    let slot = exp.delta.slot(cx.pre, view.id.actor, view.id.slot)?;
    if slot.descriptor_result == Some(ResultRef(result)) {
        slot.descriptor_result = None;
        if slot.state == SlotState::ResultOpen {
            slot.state = SlotState::ReadyBound;
        }
    }
    let a = exp.delta.view_actor(cx.pre, view.id.actor);
    if a.and_then(|a| a.active_public_result) == Some(ResultRef(result)) {
        exp.delta.actor(cx.pre, view.id.actor)?.active_public_result = None;
    }
    Ok(())
}

/// New receipt identity in the transaction's original scope, with an anchor when durable.
pub(super) fn plan_receipt(pre: &State, plan: &mut EnvPlan, txn: Identity, durable: bool) {
    plan.receipt = Some(ident::scoped(
        IdentityKind::Receipt,
        txn,
        ident::next(pre, IdentityKind::Receipt),
    ));
    plan.new_receipt = true;
    if durable {
        plan.anchor = Some(ident::engine_wide(
            IdentityKind::Anchor,
            txn.engine,
            ident::next(pre, IdentityKind::Anchor),
        ));
        plan.new_anchor = true;
    }
}

/// Existing receipt and its anchor of a transaction, as supplied by later facts.
pub(super) fn plan_existing_receipt(pre: &State, plan: &mut EnvPlan, txn: &TransactionRecord) {
    if let Some(r) = txn.receipt.and_then(|r| ident::receipt(pre, r.0)) {
        plan.receipt = Some(r.id);
        plan.anchor = r.anchor;
    }
}

/// Record a newly issued receipt with its delivery state.
pub(super) fn add_receipt(
    exp: &mut Expect,
    plan: &EnvPlan,
    txn: u16,
    delivered: Option<ContextRef>,
) -> Result<(), CheckFailure> {
    let Some(id) = plan.receipt else {
        return Ok(());
    };
    exp.delta.add_receipt(ReceiptRecord {
        id,
        transaction: TransactionRef(txn),
        engine: EngineRef(id.engine),
        kind: if plan.anchor.is_some() {
            ReceiptKind::DurableWrite
        } else {
            ReceiptKind::NoWriteCompletion
        },
        delivery: if delivered.is_some() {
            Delivery::Delivered
        } else {
            Delivery::NotDelivered
        },
        anchor: plan.anchor,
        source_fact: Some(FactRef(plan.fact.nonce)),
        delivery_context: delivered,
    })
}

/// Cleanup value recorded by a fact that consumed (or not) its supplied cleanup.
pub(super) fn cleanup_of(consumed: u8, supplied: u8) -> Cleanup {
    if consumed & 2 == 0 {
        return Cleanup::NotConsumed;
    }
    Cleanup::try_from(supplied).unwrap_or(Cleanup::NotConsumed)
}
