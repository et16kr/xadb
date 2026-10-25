//! Begin, SetAutocommit, CommitAtomic, BeginCommit, Rollback, CommitFact and FinishCommit.
use super::super::schema::*;
use super::super::{Check, Controls, EdgeClass, Exclusion};
use super::CheckFailure;
use super::expect::{self, Expect, Row};
use super::fence::{self, Fence};
use super::gates::{self, Cx, Fresh};
use super::ident;
use super::owners::{self, EnvPlan, FactSpec, FenceClass, ObligationPlan};
use super::result::{self, Pending};
use super::shared;
use super::statement::{envelope, recovery_list};

fn admit(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    r: super::super::Response,
    controls: Controls,
) -> Result<(), CheckFailure> {
    exp.class = EdgeClass::Admitted;
    exp.response = Some(r);
    exp.controls = controls;
    gates::register(cx, exp, f, ContextData::Completed)
}

/// Fresh gates of the session-transaction family; `Some` after the entry checks pass.
fn session_gates(
    cx: &Cx,
    exp: &mut Expect,
    result_busy: bool,
    need_txn: Option<bool>,
) -> Result<Option<Fresh>, CheckFailure> {
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(None);
    };
    exp.operated = f.actor.transaction.map(|t| t.0);
    if let Some(code) = gates::frame_gate(&f, false) {
        gates::reject_gate(cx, exp, &f, code, false)?;
        return Ok(None);
    }
    if result_busy && f.actor.active_public_result.is_some() {
        gates::reject_gate(cx, exp, &f, ReturnCode::ResultBusy, false)?;
        return Ok(None);
    }
    if let Some(need) = need_txn
        && f.actor.transaction.is_some() != need
    {
        gates::reject_gate(cx, exp, &f, ReturnCode::TxnState, false)?;
        return Ok(None);
    }
    if gates::entry_gate(cx, exp, &f, false)? {
        return Ok(None);
    }
    Ok(Some(f))
}

fn current_txn(pre: &State, f: &Fresh) -> Result<TransactionRecord, CheckFailure> {
    f.actor
        .transaction
        .and_then(|t| ident::txn(pre, t.0))
        .copied()
        .ok_or(CheckFailure::PreState("missing caller transaction"))
}

pub(super) fn begin(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    if cx.input.outcome > 2 {
        return Err(CheckFailure::Input("begin outcome"));
    }
    let Some(f) = session_gates(cx, exp, false, Some(false))? else {
        return Ok(());
    };
    if cx.input.outcome != 0 {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let pre = cx.pre;
    let t = shared::prospective_txn(pre, &f.actor);
    shared::add_txn(
        cx,
        exp,
        &f.actor,
        t,
        TransactionOrigin::ExplicitBegin,
        Access::ReadWrite,
    )?;
    let first = ident::next(pre, IdentityKind::Resource);
    for (k, kind) in [
        ResourceKind::Undo,
        ResourceKind::Locks,
        ResourceKind::AbortReserve,
    ]
    .into_iter()
    .enumerate()
    {
        exp.delta.add_owner(OwnerRecord {
            id: ident::scoped(IdentityKind::Resource, t, first.wrapping_add(k as u16)),
            resource_kind: kind,
            disposition: OwnerDisposition::Held,
            transfer_count: 0,
            original_holder: ident::href(t),
            current_holder: Some(ident::href(t)),
            previous_holder: None,
            birth_context: Some(f.context_ref()),
            last_fact: None,
        })?;
    }
    exp.delta.bump(IdentityKind::Resource, 3);
    exp.operated = Some(t.nonce);
    let r = expect::response(expect::ok(Phase::Entry, StatementEffect::NoEffect));
    admit(cx, exp, &f, r, expect::entry_passed(false, false))
}

pub(super) fn set_autocommit(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    if i.outcome > 2 || i.set_autocommit > 1 {
        return Err(CheckFailure::Input("set autocommit fields"));
    }
    let Some(f) = session_gates(cx, exp, false, Some(false))? else {
        return Ok(());
    };
    if i.outcome != 0 {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let mode = if i.set_autocommit == 0 {
        Autocommit::On
    } else {
        Autocommit::Off
    };
    if f.actor.autocommit != mode {
        exp.delta.actor(cx.pre, f.actor.actor)?.autocommit = mode;
    }
    let r = expect::response(expect::ok(Phase::Entry, StatementEffect::NoEffect));
    admit(cx, exp, &f, r, expect::entry_passed(false, false))
}

/// Committed atomic/continuation end: Ended, decision and knowledge, receipt delivery.
fn commit_end(
    cx: &Cx,
    exp: &mut Expect,
    plan: &EnvPlan,
    txn: u16,
    progress: CommitProgress,
    delivered: Option<ContextRef>,
) -> Result<(), CheckFailure> {
    let t = exp.delta.txn(cx.pre, txn)?;
    t.life = TransactionLife::Ended;
    t.progress = progress;
    t.decision = Decision::Committed;
    t.completion_fact = Some(FactRef(plan.fact.nonce));
    t.receipt = plan.receipt.map(|r| ReceiptRef(r.nonce));
    if delivered.is_some() {
        t.client_knowledge = ClientKnowledge::KnownCommitted;
    }
    t.delivery = match (plan.receipt.is_some(), delivered.is_some()) {
        (false, _) => Delivery::NotApplicable,
        (true, true) => Delivery::Delivered,
        (true, false) => Delivery::NotDelivered,
    };
    if plan.new_receipt {
        shared::add_receipt(exp, plan, txn, delivered)?;
    }
    Ok(())
}

fn abort_row(code: ReturnCode, origin: OriginCause) -> Row {
    expect::error(
        code,
        Phase::Commit,
        StatementEffect::StatementRestored,
        Retry::WholeTransactionDecision,
        origin,
    )
}

pub(super) fn commit_atomic(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 9 {
        return Err(CheckFailure::Input("commit outcome"));
    }
    let Some(f) = session_gates(cx, exp, true, Some(true))? else {
        return Ok(());
    };
    if matches!(o, 8 | 9) {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let pre = cx.pre;
    let t = current_txn(pre, &f)?;
    let changed = t.prior_successful_write;
    exp.exclude_if(
        (!changed && matches!(o, 4 | 7)) || (changed && o == 1),
        Exclusion::CommitNoWrite,
    );
    let engine = f.actor.engine.0;
    let mut plan = EnvPlan::new(pre, FactKind::CommitAtomic, engine);
    plan.transaction = Some(t.id);
    plan.cleanup = match o {
        2 | 5 | 6 => 1,
        3 => 2,
        _ => 255,
    };
    plan.commit_class = match o {
        0 if changed => 4,
        0 => 5,
        1 => 6,
        2 | 5 | 6 => 0,
        3 => 1,
        4 => 7,
        _ => 8,
    };
    match o {
        0 => shared::plan_receipt(pre, &mut plan, t.id, changed),
        7 => shared::plan_receipt(pre, &mut plan, t.id, true),
        _ => {}
    }
    let class = match o {
        3 => Some(FenceClass::UndoIncomplete),
        4 => Some(FenceClass::Unknown),
        7 => Some(FenceClass::PublicationPending),
        _ => None,
    };
    match class {
        Some(class) => owners::census(&mut plan, pre, engine, Some(t.id), false, class)?,
        None => owners::release_rw(&mut plan, pre, t.id.nonce)?,
    }
    plan.link_existing(pre, (o == 0 && changed).then_some(Decision::Committed))?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = match o {
        0 | 7 => 12,
        1 | 4 => 8,
        _ => 2,
    };
    let ctx = f.context_ref();
    let fact = FactRef(plan.fact.nonce);
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::CommitAtomic,
            outcome: o,
            context: Some(ctx),
            cleanup: shared::cleanup_of(exp.consumed, plan.cleanup),
            next_work: NextWork::NotConsumed,
        },
    )?;
    let actor = f.actor.actor;
    let mut r = match o {
        0 | 1 => {
            let progress = if o == 0 && changed {
                CommitProgress::Published
            } else {
                CommitProgress::NoWriteValidated
            };
            commit_end(cx, exp, &plan, t.id.nonce, progress, Some(ctx))?;
            exp.delta.txn(pre, t.id.nonce)?.client_knowledge = ClientKnowledge::KnownCommitted;
            shared::detach(cx, exp, actor, t.id.nonce)?;
            expect::response(expect::ok(Phase::Commit, StatementEffect::NoEffect))
        }
        2 | 5 | 6 => {
            shared::whole_abort(cx, exp, actor, t.id.nonce, fact)?;
            expect::response(match o {
                2 => abort_row(ReturnCode::Conflict, OriginCause::Conflict),
                5 => abort_row(ReturnCode::Cancelled, OriginCause::Cancelled),
                _ => abort_row(ReturnCode::Timeout, OriginCause::Timeout),
            })
        }
        _ => {
            let class = class.unwrap_or(FenceClass::UndoIncomplete);
            fence::first_fence(
                cx,
                exp,
                &plan,
                Fence {
                    engine,
                    cause: t.id.nonce,
                    class,
                    context: ctx,
                },
            )?;
            fence::cause_txn(cx, exp, t.id.nonce, class, fact)?;
            match class {
                FenceClass::Unknown => {
                    exp.delta.txn(pre, t.id.nonce)?.progress = CommitProgress::Attempted;
                }
                FenceClass::PublicationPending => {
                    let row = exp.delta.txn(pre, t.id.nonce)?;
                    row.progress = CommitProgress::DurableConfirmed;
                    row.receipt = plan.receipt.map(|x| ReceiptRef(x.nonce));
                    row.delivery = Delivery::NotDelivered;
                    shared::add_receipt(exp, &plan, t.id.nonce, None)?;
                }
                FenceClass::UndoIncomplete => {}
            }
            expect::response(fence::row(class, Phase::Commit))
        }
    };
    r.recovery_obligations = recovery_list(&plan);
    admit(cx, exp, &f, r, expect::entry_passed(o == 5, o == 6))
}

pub(super) fn begin_commit(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    if cx.input.outcome > 1 {
        return Err(CheckFailure::Input("begin commit outcome"));
    }
    let Some(f) = session_gates(cx, exp, true, Some(true))? else {
        return Ok(());
    };
    let pre = cx.pre;
    let t = current_txn(pre, &f)?;
    let ctx = f.context_ref();
    let row = exp.delta.txn(pre, t.id.nonce)?;
    row.progress = CommitProgress::BeforeAttempt;
    row.parked_context = Some(ctx);
    exp.delta.actor(pre, f.actor.actor)?.parked_context = Some(ctx);
    let parked = ParkedContext {
        api: Api::BeginCommit,
        original_deadline: cx.input.original_deadline,
        cancel_supplied_or_signalled: cx.input.cancel,
        deadline_reached: cx.input.deadline_reached,
        cancel_observed_at_completion: false,
        deadline_observed_at_completion: false,
        completion_phase: CompletionPhase::NotCompleted,
        statement: None,
        result: None,
        transaction: Some(TransactionRef(t.id.nonce)),
    };
    exp.class = EdgeClass::Admitted;
    exp.response = Some(expect::response(expect::ok(
        Phase::Commit,
        StatementEffect::NoEffect,
    )));
    exp.controls = expect::entry_passed(false, false);
    gates::register(cx, exp, &f, ContextData::ParkedCommit(parked))
}

pub(super) fn rollback(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 3 {
        return Err(CheckFailure::Input("rollback outcome"));
    }
    let Some(f) = session_gates(cx, exp, false, Some(true))? else {
        return Ok(());
    };
    if o > 1 {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let pre = cx.pre;
    let t = current_txn(pre, &f)?;
    let rw = t.access == Access::ReadWrite;
    exp.exclude_if(o == 1 && !rw, Exclusion::ReadOnlyCleanup);
    let engine = f.actor.engine.0;
    let mut plan = EnvPlan::new(pre, FactKind::RollbackCleanup, engine);
    plan.transaction = Some(t.id);
    plan.cleanup = 1 + o;
    plan.commit_class = o;
    if o == 0 {
        owners::release_rw(&mut plan, pre, t.id.nonce)?;
        for r in shared::open_results_of(pre, t.id.nonce).as_slice() {
            owners::release_cursor(&mut plan, pre, *r)?;
        }
    } else {
        owners::census(
            &mut plan,
            pre,
            engine,
            Some(t.id),
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
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::Rollback,
            outcome: o,
            context: Some(ctx),
            cleanup: shared::cleanup_of(2, plan.cleanup),
            next_work: NextWork::NotConsumed,
        },
    )?;
    let mut r = if o == 0 {
        for r in shared::open_results_of(pre, t.id.nonce).as_slice() {
            shared::discard_open_result(cx, exp, *r, ctx)?;
        }
        shared::whole_abort(cx, exp, f.actor.actor, t.id.nonce, fact)?;
        expect::response(expect::ok(
            Phase::Rollback,
            StatementEffect::StatementRestored,
        ))
    } else {
        let class = FenceClass::UndoIncomplete;
        fence::first_fence(
            cx,
            exp,
            &plan,
            Fence {
                engine,
                cause: t.id.nonce,
                class,
                context: ctx,
            },
        )?;
        fence::cause_txn(cx, exp, t.id.nonce, class, fact)?;
        expect::response(fence::row(class, Phase::Rollback))
    };
    r.recovery_obligations = recovery_list(&plan);
    admit(cx, exp, &f, r, expect::entry_passed(false, false))
}

fn parked_txn(pre: &State, p: &ParkedContext) -> Result<TransactionRecord, CheckFailure> {
    p.transaction
        .and_then(|t| ident::txn(pre, t.0))
        .copied()
        .ok_or(CheckFailure::PreState("parked commit transaction"))
}

pub(super) fn commit_fact(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 4 {
        return Err(CheckFailure::Input("commit fact outcome"));
    }
    let actor = gates::caller(cx)?;
    let (c, p) = match result::pending(cx, exp, &actor, false)? {
        Pending::Matched(c, p) => (c, p),
        Pending::Done => return Ok(()),
    };
    let pre = cx.pre;
    let t = parked_txn(pre, &p)?;
    let progress = t.progress;
    let changed = t.prior_successful_write;
    let engine = actor.engine.0;
    let pre_attempt = progress == CommitProgress::BeforeAttempt && matches!(o, 0 | 3 | 4);
    let in_order = pre_attempt
        || (progress == CommitProgress::Attempted && o == 1)
        || (progress == CommitProgress::DurableConfirmed && o == 2);
    exp.exclude_if(
        (!changed && o <= 2) || (changed && o >= 3),
        Exclusion::CommitNoWrite,
    );
    exp.exclude_if(!in_order, Exclusion::CommitProgress);
    exp.exclude_if(
        pre_attempt
            && (p.cancel_supplied_or_signalled
                || p.deadline_reached
                || !fence::admitting(pre, engine)),
        Exclusion::CommitPreattemptGate,
    );
    let mut plan = EnvPlan::new(pre, FactKind::CommitProgress, engine);
    plan.transaction = Some(t.id);
    plan.commit_class = o + 2;
    match o {
        1 => shared::plan_receipt(pre, &mut plan, t.id, true),
        2 => shared::plan_existing_receipt(pre, &mut plan, &t),
        3 => shared::plan_receipt(pre, &mut plan, t.id, false),
        _ => {}
    }
    let obligation = t
        .obligation
        .and_then(|n| ident::obligation(pre, n.0))
        .copied();
    if o == 2 {
        owners::release_rw(&mut plan, pre, t.id.nonce)?;
        owners::release_recovery(&mut plan, pre, t.id.nonce)?;
        if let Some(ob) = obligation.filter(|x| x.settlement == Settlement::Outstanding) {
            plan.obligation(ObligationPlan {
                id: ob.id,
                txn: t.id,
                new: false,
                known: Decision::Committed,
                reason: None,
                settle: Some(Settlement::DrainedConfirmed),
            })?;
        }
        plan.link_existing(pre, Some(Decision::Committed))?;
    } else if o >= 3 {
        owners::release_rw(&mut plan, pre, t.id.nonce)?;
        plan.link_existing(pre, None)?;
    }
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = match o {
        0 | 4 => 8,
        _ => 12,
    };
    let fact = FactRef(plan.fact.nonce);
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::CommitFact,
            outcome: o,
            context: Some(ContextRef(c.id.nonce)),
            cleanup: Cleanup::NotConsumed,
            next_work: NextWork::NotConsumed,
        },
    )?;
    match o {
        0 => {
            let row = exp.delta.txn(pre, t.id.nonce)?;
            row.progress = CommitProgress::Attempted;
            row.decision = Decision::Unknown;
        }
        1 => {
            let row = exp.delta.txn(pre, t.id.nonce)?;
            row.progress = CommitProgress::DurableConfirmed;
            row.decision = Decision::Committed;
            row.receipt = plan.receipt.map(|x| ReceiptRef(x.nonce));
            row.delivery = Delivery::NotDelivered;
            shared::add_receipt(exp, &plan, t.id.nonce, None)?;
        }
        2 => {
            let row = exp.delta.txn(pre, t.id.nonce)?;
            row.progress = CommitProgress::Published;
            row.life = TransactionLife::Ended;
            row.decision = Decision::Committed;
            row.completion_fact = Some(fact);
        }
        _ => commit_end(
            cx,
            exp,
            &plan,
            t.id.nonce,
            CommitProgress::NoWriteValidated,
            None,
        )?,
    }
    exp.class = EdgeClass::NoContextIssue;
    let mut r = expect::response(Row {
        certainty: Certainty::NotApplicable,
        ..expect::ok(Phase::Commit, StatementEffect::NoEffect)
    });
    r.recovery_obligations = recovery_list(&plan);
    exp.response = Some(r);
    Ok(())
}

pub(super) fn finish(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let o = cx.input.outcome;
    if o > 4 {
        return Err(CheckFailure::Input("finish outcome"));
    }
    let actor = gates::caller(cx)?;
    let (c, p) = match result::pending(cx, exp, &actor, false)? {
        Pending::Matched(c, p) => (c, p),
        Pending::Done => return Ok(()),
    };
    let pre = cx.pre;
    let t = parked_txn(pre, &p)?;
    let progress = t.progress;
    let engine = actor.engine.0;
    let admission = ident::engine(pre, engine).map_or(EngineAdmission::Fenced, |e| e.admission);
    let admitting = admission == EngineAdmission::Admitting;
    let fenced = admission == EngineAdmission::Fenced;
    let allowed = match o {
        0 => matches!(
            progress,
            CommitProgress::Published | CommitProgress::NoWriteValidated
        ),
        1 | 2 => progress == CommitProgress::BeforeAttempt,
        3 => progress == CommitProgress::Attempted,
        _ => progress == CommitProgress::DurableConfirmed,
    };
    exp.exclude_if(!allowed, Exclusion::CommitProgress);
    exp.exclude_if(allowed && o == 1 && !admitting, Exclusion::PeerFence);
    let obligation = t
        .obligation
        .and_then(|n| ident::obligation(pre, n.0))
        .copied();
    let mut plan = EnvPlan::new(pre, FactKind::CommitFinish, engine);
    plan.transaction = Some(t.id);
    let class = match o {
        2 => Some(FenceClass::UndoIncomplete),
        3 => Some(FenceClass::Unknown),
        4 => Some(FenceClass::PublicationPending),
        _ => None,
    };
    let handoff = o == 2 && fenced;
    match o {
        0 => {
            plan.commit_class = match (progress, t.receipt.is_some()) {
                (CommitProgress::NoWriteValidated, true) => 5,
                (CommitProgress::NoWriteValidated, false) => 6,
                _ => 4,
            };
            shared::plan_existing_receipt(pre, &mut plan, &t);
        }
        1 => {
            plan.cleanup = 1;
            plan.commit_class = 0;
            owners::release_rw(&mut plan, pre, t.id.nonce)?;
        }
        _ => {
            plan.commit_class = match o {
                2 => 1,
                3 => 7,
                _ => 8,
            };
            if o == 2 {
                plan.cleanup = if fenced { 3 } else { 2 };
            }
            if o == 4 {
                shared::plan_existing_receipt(pre, &mut plan, &t);
            }
            let class = class.unwrap_or(FenceClass::UndoIncomplete);
            if handoff {
                if let Some(ob) = obligation {
                    plan.obligation(ObligationPlan {
                        id: ob.id,
                        txn: t.id,
                        new: false,
                        known: ob.known_decision,
                        reason: None,
                        settle: None,
                    })?;
                    for owner in owners::live_of(
                        pre,
                        owners::txn_holder(t.id.nonce),
                        ident::is_obligation_owner,
                    ) {
                        plan.handoff(pre, owner, ob.id)?;
                    }
                }
            } else {
                owners::census(&mut plan, pre, engine, Some(t.id), false, class)?;
            }
        }
    }
    plan.link_existing(pre, None)?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = match o {
        0 if plan.receipt.is_some() => 12,
        0 | 3 => 8,
        1 | 2 => 2,
        _ => 12,
    };
    let ctx = ContextRef(c.id.nonce);
    let fact = FactRef(plan.fact.nonce);
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::FinishCommit,
            outcome: o,
            context: Some(ctx),
            cleanup: shared::cleanup_of(exp.consumed, plan.cleanup),
            next_work: NextWork::NotConsumed,
        },
    )?;
    let cancel = p.cancel_supplied_or_signalled;
    let deadline = p.deadline_reached;
    let observes = matches!(o, 1 | 2);
    let mut r = match o {
        0 => {
            let row = exp.delta.txn(pre, t.id.nonce)?;
            row.client_knowledge = ClientKnowledge::KnownCommitted;
            if let Some(receipt) = t.receipt {
                row.delivery = Delivery::Delivered;
                let rr = exp.delta.receipt(pre, receipt.0)?;
                rr.delivery = Delivery::Delivered;
                rr.delivery_context = Some(ctx);
            }
            shared::detach(cx, exp, actor.actor, t.id.nonce)?;
            expect::response(expect::ok(Phase::Commit, StatementEffect::NoEffect))
        }
        1 => {
            shared::whole_abort(cx, exp, actor.actor, t.id.nonce, fact)?;
            expect::response(if cancel {
                abort_row(ReturnCode::Cancelled, OriginCause::Cancelled)
            } else if deadline {
                abort_row(ReturnCode::Timeout, OriginCause::Timeout)
            } else {
                abort_row(ReturnCode::Conflict, OriginCause::Conflict)
            })
        }
        2 if handoff => {
            let row = exp.delta.txn(pre, t.id.nonce)?;
            row.decision = Decision::NotCommittedRecoveryRequired;
            row.client_knowledge = ClientKnowledge::KnownNotCommittedRecoveryRequired;
            row.completion_fact = Some(fact);
            row.life = TransactionLife::RecoveryPending;
            expect::response(Row {
                code: ReturnCode::RecoveryRequired,
                phase: Phase::Commit,
                effect: StatementEffect::RecoveryRequired,
                certainty: Certainty::Known,
                retry: Retry::AfterRecovery,
                engine_cause: EngineCause::PeerFence,
                origin_cause: OriginCause::PeerFence,
            })
        }
        _ => {
            let class = class.unwrap_or(FenceClass::UndoIncomplete);
            if admitting {
                fence::first_fence(
                    cx,
                    exp,
                    &plan,
                    Fence {
                        engine,
                        cause: t.id.nonce,
                        class,
                        context: ctx,
                    },
                )?;
            }
            fence::cause_txn(cx, exp, t.id.nonce, class, fact)?;
            expect::response(fence::row(class, Phase::Commit))
        }
    };
    r.recovery_obligations = recovery_list(&plan);
    result::complete(cx, exp, actor.actor, &c, &p)?;
    exp.response = Some(r);
    exp.controls = Controls {
        entry_cancel: Check::NotChecked,
        entry_deadline: Check::NotChecked,
        observed_cancel: observes && cancel,
        observed_deadline: observes && !cancel && deadline,
    };
    Ok(())
}
