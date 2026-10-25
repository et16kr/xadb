//! Prepare, BindAll, Execute, CloseStatement and InspectStatement.
use super::super::schema::*;
use super::super::{EdgeClass, Exclusion, Response};
use super::CheckFailure;
use super::expect::{self, Expect};
use super::fence::{self, Fence};
use super::gates::{self, Cx, Fresh};
use super::ident;
use super::owners::{self, EnvMatch, EnvPlan, FactSpec, FenceClass};
use super::shared;

const MISSING_SLOT: CheckFailure = CheckFailure::PreState("missing prepared slot");

/// Record an envelope mismatch as `Excluded(Envelope)`.
pub(super) fn envelope(exp: &mut Expect, plan: &EnvPlan, cx: &Cx) {
    if plan.matches(cx.input.environment.as_ref()) != EnvMatch::Exact {
        exp.exclude(Exclusion::Envelope);
    }
}

fn admit(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    r: Response,
    controls: super::super::Controls,
) -> Result<(), CheckFailure> {
    exp.class = EdgeClass::Admitted;
    exp.response = Some(r);
    exp.controls = controls;
    gates::register(cx, exp, f, ContextData::Completed)
}

pub(super) fn prepare(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    if i.prepare_slot > 1 || i.prepare_shape > 1 || i.prepare_n > 1 || i.outcome > 2 {
        return Err(CheckFailure::Input("prepare fields outside profile"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    if let Some(code) = gates::frame_gate(&f, false) {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    let slot = *ident::slot(cx.pre, f.actor.actor, i.prepare_slot).ok_or(MISSING_SLOT)?;
    if f.actor.active_public_result.is_some() {
        return gates::reject_gate(cx, exp, &f, ReturnCode::ResultBusy, false);
    }
    if !matches!(slot.state, SlotState::Vacant | SlotState::Closed) {
        return gates::reject_gate(cx, exp, &f, ReturnCode::SlotBusy, false);
    }
    if gates::entry_gate(cx, exp, &f, false)? {
        return Ok(());
    }
    if i.outcome != 0 {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let pre = cx.pre;
    let ctx = f.context_ref();
    let mut next_generation = slot.next_statement_generation;
    if slot.state == SlotState::Closed
        && let Some(old) = slot.statement
    {
        let st = exp.delta.statement(pre, old.0)?;
        st.life = StatementLife::Retired;
        st.retirement = Retirement::SlotReuse;
        st.retirement_context = Some(ctx);
        next_generation = next_generation.wrapping_add(1);
    }
    let shape = if i.prepare_shape == 0 {
        Shape::Select
    } else {
        Shape::Dml
    };
    let metadata = Metadata::try_from(i.prepare_shape * 2 + i.prepare_n)
        .map_err(|_| CheckFailure::Input("prepare metadata"))?;
    let p = ident::ident(
        IdentityKind::Statement,
        f.actor.actor,
        i.prepare_slot,
        f.actor.engine.0,
        f.actor.session.0,
        ident::next(pre, IdentityKind::Statement),
        next_generation,
    );
    exp.delta.add_statement(StatementRecord {
        id: p,
        issue_ordinal: pre.issued_statements.len() as u16 + 1,
        shape,
        parameter_count: i.prepare_n,
        metadata,
        life: StatementLife::Current,
        retirement: Retirement::None,
        session: f.actor.session,
        creation_context: Some(ctx),
        retirement_context: None,
    })?;
    exp.delta.bump(IdentityKind::Statement, 1);
    let mut resource = ident::next(pre, IdentityKind::Resource);
    let mut local_owner = |exp: &mut Expect, kind: ResourceKind| -> Result<(), CheckFailure> {
        exp.delta.add_owner(OwnerRecord {
            id: ident::scoped(IdentityKind::Resource, p, resource),
            resource_kind: kind,
            disposition: OwnerDisposition::Held,
            transfer_count: 0,
            original_holder: ident::href(p),
            current_holder: Some(ident::href(p)),
            previous_holder: None,
            birth_context: Some(ctx),
            last_fact: None,
        })?;
        exp.delta.bump(IdentityKind::Resource, 1);
        resource = resource.wrapping_add(1);
        Ok(())
    };
    local_owner(exp, ResourceKind::Plan)?;
    let binding = if i.prepare_n == 0 {
        let b = ident::scoped(
            IdentityKind::Binding,
            p,
            ident::next(pre, IdentityKind::Binding),
        );
        exp.delta.add_binding(BindingRecord {
            id: b,
            statement: StatementRef(p.nonce),
            issue_ordinal: pre.bindings.len() as u16 + 1,
            value: BindingValue::Empty,
            life: BindingLife::Current,
            replaced_by: None,
            creation_context: Some(ctx),
        })?;
        exp.delta.bump(IdentityKind::Binding, 1);
        local_owner(exp, ResourceKind::Binding)?;
        Some(BindingRef(b.nonce))
    } else {
        None
    };
    let s = exp.delta.slot(pre, f.actor.actor, i.prepare_slot)?;
    s.state = if binding.is_some() {
        SlotState::ReadyBound
    } else {
        SlotState::ReadyUnbound
    };
    s.next_statement_generation = next_generation.wrapping_add(1);
    s.statement = Some(StatementRef(p.nonce));
    s.binding = binding;
    s.descriptor_result = None;
    let mut r = expect::response(expect::ok(Phase::Prepare, StatementEffect::NoEffect));
    r.returned_statement = Some(p);
    admit(cx, exp, &f, r, expect::entry_passed(false, false))
}

pub(super) fn bind(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    if i.outcome > 10 {
        return Err(CheckFailure::Input("bind outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    let Some(st) = gates::auth_statement(cx.pre, &f.actor, i.target) else {
        return gates::reject_handle(cx, exp, &f, false);
    };
    gates::child_statement(cx.pre, exp, &st, None);
    if let Some(code) = gates::frame_gate(&f, false) {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    let slot = *ident::slot(cx.pre, st.id.actor, st.id.slot).ok_or(MISSING_SLOT)?;
    let code = if f.actor.active_public_result.is_some() {
        Some(ReturnCode::ResultBusy)
    } else if st.life == StatementLife::CurrentClosed {
        Some(ReturnCode::HandleClosed)
    } else if slot.state == SlotState::Invalid {
        Some(ReturnCode::StalePlan)
    } else {
        None
    };
    if let Some(code) = code {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    if gates::entry_gate(cx, exp, &f, false)? {
        return Ok(());
    }
    let n = st.parameter_count;
    let value = match (n, i.outcome) {
        (_, 8 | 9) => {
            exp.exclude(Exclusion::Envelope);
            return Ok(());
        }
        (1, 0) => BindingValue::A61,
        (1, 1) => BindingValue::B62,
        (0, 0..=2) => BindingValue::Empty,
        (0, 3..=7) => {
            exp.exclude(Exclusion::BindN0);
            return Ok(());
        }
        (_, outcome) => {
            let row = match outcome {
                3 => (ReturnCode::Type, Retry::Never),
                4 => (ReturnCode::NotNull, Retry::Never),
                5 => (ReturnCode::Range, Retry::Never),
                6 => (ReturnCode::SyntaxUtf8, Retry::Never),
                7 => (ReturnCode::Limit, Retry::Operation),
                _ => (ReturnCode::BindCount, Retry::Never),
            };
            let row = expect::error(
                row.0,
                Phase::Bind,
                StatementEffect::NoEffect,
                row.1,
                OriginCause::None,
            );
            return admit(
                cx,
                exp,
                &f,
                expect::response(row),
                expect::entry_passed(false, false),
            );
        }
    };
    let pre = cx.pre;
    let ctx = f.context_ref();
    let b = ident::scoped(
        IdentityKind::Binding,
        st.id,
        ident::next(pre, IdentityKind::Binding),
    );
    if let Some(old) = slot.binding {
        let row = exp.delta.binding(pre, old.0)?;
        row.life = BindingLife::Replaced;
        row.replaced_by = Some(BindingRef(b.nonce));
    }
    let holder = ident::href(st.id);
    for o in owners::live_of(pre, holder, |k| k == ResourceKind::Binding) {
        let row = exp.delta.owner(pre, o.id.nonce)?;
        row.previous_holder = row.current_holder;
        row.current_holder = None;
        row.disposition = OwnerDisposition::Released;
    }
    exp.delta.add_binding(BindingRecord {
        id: b,
        statement: StatementRef(st.id.nonce),
        issue_ordinal: pre.bindings.len() as u16 + 1,
        value,
        life: BindingLife::Current,
        replaced_by: None,
        creation_context: Some(ctx),
    })?;
    exp.delta.add_owner(OwnerRecord {
        id: ident::scoped(
            IdentityKind::Resource,
            st.id,
            ident::next(pre, IdentityKind::Resource),
        ),
        resource_kind: ResourceKind::Binding,
        disposition: OwnerDisposition::Held,
        transfer_count: 0,
        original_holder: holder,
        current_holder: Some(holder),
        previous_holder: None,
        birth_context: Some(ctx),
        last_fact: None,
    })?;
    exp.delta.bump(IdentityKind::Binding, 1);
    exp.delta.bump(IdentityKind::Resource, 1);
    let s = exp.delta.slot(pre, st.id.actor, st.id.slot)?;
    s.binding = Some(BindingRef(b.nonce));
    s.state = SlotState::ReadyBound;
    let r = expect::response(expect::ok(Phase::Bind, StatementEffect::NoEffect));
    admit(cx, exp, &f, r, expect::entry_passed(false, false))
}

pub(super) fn execute(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    let o = i.outcome;
    if o > 10 {
        return Err(CheckFailure::Input("execute outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    exp.operated = f.actor.transaction.map(|t| t.0);
    let Some(st) = gates::auth_statement(cx.pre, &f.actor, i.target) else {
        return gates::reject_handle(cx, exp, &f, false);
    };
    gates::child_statement(cx.pre, exp, &st, f.actor.transaction);
    if let Some(code) = gates::frame_gate(&f, false) {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    let pre = cx.pre;
    let slot = *ident::slot(pre, st.id.actor, st.id.slot).ok_or(MISSING_SLOT)?;
    let code = if f.actor.active_public_result.is_some() {
        Some(ReturnCode::ResultBusy)
    } else if st.life == StatementLife::CurrentClosed {
        Some(ReturnCode::HandleClosed)
    } else if slot.state == SlotState::Invalid {
        Some(ReturnCode::StalePlan)
    } else if slot.state == SlotState::ReadyUnbound {
        Some(ReturnCode::BindCount)
    } else {
        None
    };
    if let Some(code) = code {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    if gates::entry_gate(cx, exp, &f, false)? {
        return Ok(());
    }
    if matches!(o, 1 | 2) {
        exp.exclude(Exclusion::Envelope);
        return Ok(());
    }
    let ctx = f.context_ref();
    let dml = st.shape == Shape::Dml;
    let existing = f
        .actor
        .transaction
        .and_then(|t| ident::txn(pre, t.0))
        .copied();
    let (t_id, t_new, origin, access) = match existing {
        Some(t) => (t.id, false, t.origin, t.access),
        None => {
            let origin = if f.actor.autocommit == Autocommit::Off {
                TransactionOrigin::OffStarted
            } else if dml {
                TransactionOrigin::ImplicitReadWrite
            } else {
                TransactionOrigin::ImplicitReadOnly
            };
            let access = if origin == TransactionOrigin::ImplicitReadOnly {
                Access::ReadOnly
            } else {
                Access::ReadWrite
            };
            (shared::prospective_txn(pre, &f.actor), true, origin, access)
        }
    };
    let rw = access == Access::ReadWrite;
    let implicit_rw = rw && origin == TransactionOrigin::ImplicitReadWrite;
    let implicit = matches!(
        origin,
        TransactionOrigin::ImplicitReadOnly | TransactionOrigin::ImplicitReadWrite
    );
    let implicit_commit = o == 0 && dml && implicit_rw;
    exp.exclude_if(
        matches!(o, 9 | 10) && !(t_new && implicit_rw && dml),
        Exclusion::ExecuteCommit,
    );
    exp.exclude_if(o == 8 && !rw, Exclusion::ReadOnlyCleanup);

    let mut plan = EnvPlan::new(pre, FactKind::Execution, f.actor.engine.0);
    plan.work_kind = 1;
    plan.work_class = o;
    plan.cleanup = match o {
        3..=7 => 1,
        8 => 2,
        _ => 255,
    };
    plan.commit_class = match o {
        0 if implicit_commit => 4,
        3..=6 if t_new && implicit_rw => 0,
        7 if rw => 0,
        8 => 1,
        9 => 7,
        10 => 8,
        _ => 255,
    };
    plan.transaction = Some(t_id);
    let aborts_new = t_new && implicit_rw && matches!(o, 3..=6);
    if t_new && rw {
        plan.birth_rw(t_id, implicit_commit || aborts_new || o == 7)?;
    }
    if !t_new && rw && (o == 7 || implicit_commit) {
        owners::release_rw(&mut plan, pre, t_id.nonce)?;
    }
    if o == 7 {
        for r in shared::open_results_of(pre, t_id.nonce).as_slice() {
            owners::release_cursor(&mut plan, pre, *r)?;
        }
    }
    // Result generation: an admitted Execute first retires the slot's live terminal result.
    let old = slot.descriptor_result;
    let old_live = old
        .and_then(|r| ident::result(pre, r.0))
        .is_some_and(|r| r.retirement == Retirement::None);
    let generation = slot
        .next_result_generation
        .wrapping_add(u16::from(old_live));
    let result_id = ident::ident(
        IdentityKind::Result,
        st.id.actor,
        st.id.slot,
        st.id.engine,
        st.id.session,
        ident::next(pre, IdentityKind::Result),
        generation,
    );
    let select = o == 0 && !dml;
    if select {
        plan.birth_cursor(result_id)?;
    }
    if implicit_commit || o == 10 {
        shared::plan_receipt(pre, &mut plan, t_id, true);
    }
    let class = match o {
        8 => Some(FenceClass::UndoIncomplete),
        9 => Some(FenceClass::Unknown),
        10 => Some(FenceClass::PublicationPending),
        _ => None,
    };
    if let Some(class) = class {
        owners::census(
            &mut plan,
            pre,
            f.actor.engine.0,
            Some(t_id),
            t_new && rw,
            class,
        )?;
    }
    plan.link_existing(pre, implicit_commit.then_some(Decision::Committed))?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }

    exp.operated = Some(t_id.nonce);
    exp.consumed = match o {
        0 if implicit_commit => 13,
        0 => 1,
        3 | 4 | 7 | 8 => 3,
        5 | 6 => 2,
        9 => 9,
        _ => 13,
    };
    if let Some(r) = old {
        if old_live {
            shared::retire_result(cx, exp, r.0, Retirement::Reexecute, ctx)?;
        }
        exp.delta.remove_descriptor(r)?;
        exp.delta
            .slot(pre, st.id.actor, st.id.slot)?
            .descriptor_result = None;
    }
    if t_new {
        shared::add_txn(cx, exp, &f.actor, t_id, origin, access)?;
    }
    let fact = FactRef(plan.fact.nonce);
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::Execute,
            outcome: o,
            context: Some(ctx),
            cleanup: shared::cleanup_of(exp.consumed, plan.cleanup),
            next_work: NextWork::NotConsumed,
        },
    )?;
    let actor = f.actor.actor;
    let mut r = match o {
        0 if select => {
            exp.delta.add_result(ResultRecord {
                id: result_id,
                issue_ordinal: pre.issued_results.len() as u16 + 1,
                statement: StatementRef(st.id.nonce),
                retirement: Retirement::None,
                public_generation_live: true,
                creation_context: Some(ctx),
                retirement_context: None,
            })?;
            exp.delta.add_descriptor(Descriptor {
                result: ResultRef(result_id.nonce),
                statement: StatementRef(st.id.nonce),
                state: ResultState::Open,
                public_generation_live: true,
                blocked_by_fence: false,
                position: 0,
                transaction: Some(TransactionRef(t_id.nonce)),
                terminal: None,
            })?;
            exp.delta.bump(IdentityKind::Result, 1);
            let s = exp.delta.slot(pre, st.id.actor, st.id.slot)?;
            s.state = SlotState::ResultOpen;
            s.descriptor_result = Some(ResultRef(result_id.nonce));
            s.next_result_generation = generation.wrapping_add(1);
            exp.delta.actor(pre, actor)?.active_public_result = Some(ResultRef(result_id.nonce));
            let mut r = expect::response(expect::ok(Phase::Execute, StatementEffect::NoEffect));
            r.rows = super::super::Rows::Count(0);
            r.returned_result = Some(result_id);
            r
        }
        0 => {
            let t = exp.delta.txn(pre, t_id.nonce)?;
            t.prior_successful_write = true;
            if implicit_commit {
                t.life = TransactionLife::Ended;
                t.progress = CommitProgress::Published;
                t.decision = Decision::Committed;
                t.client_knowledge = ClientKnowledge::KnownCommitted;
                t.delivery = Delivery::Delivered;
                t.receipt = plan.receipt.map(|x| ReceiptRef(x.nonce));
                t.completion_fact = Some(fact);
                shared::add_receipt(exp, &plan, t_id.nonce, Some(ctx))?;
                shared::detach(cx, exp, actor, t_id.nonce)?;
            }
            let mut r = expect::response(expect::ok(
                Phase::Execute,
                StatementEffect::StatementApplied,
            ));
            r.affected_count = Some(1);
            r
        }
        3..=6 => {
            if t_new && implicit {
                if rw {
                    shared::whole_abort(cx, exp, actor, t_id.nonce, fact)?;
                } else {
                    shared::end_read_only(cx, exp, actor, Some(TransactionRef(t_id.nonce)), fact)?;
                }
            }
            if o == 3 {
                exp.delta.slot(pre, st.id.actor, st.id.slot)?.state = SlotState::Invalid;
            }
            let (code, effect, retry, origin) = match o {
                3 => (
                    ReturnCode::StalePlan,
                    StatementEffect::NoEffect,
                    Retry::Never,
                    OriginCause::None,
                ),
                4 => (
                    ReturnCode::ValueError,
                    StatementEffect::StatementRestored,
                    Retry::Operation,
                    OriginCause::ValueError,
                ),
                5 => (
                    ReturnCode::Cancelled,
                    StatementEffect::StatementRestored,
                    Retry::Operation,
                    OriginCause::Cancelled,
                ),
                _ => (
                    ReturnCode::Timeout,
                    StatementEffect::StatementRestored,
                    Retry::Operation,
                    OriginCause::Timeout,
                ),
            };
            expect::response(expect::error(code, Phase::Execute, effect, retry, origin))
        }
        7 => {
            for r in shared::open_results_of(pre, t_id.nonce).as_slice() {
                shared::discard_open_result(cx, exp, *r, ctx)?;
            }
            shared::whole_abort(cx, exp, actor, t_id.nonce, fact)?;
            expect::response(expect::error(
                ReturnCode::Conflict,
                Phase::Execute,
                StatementEffect::StatementRestored,
                Retry::WholeTransactionDecision,
                OriginCause::Conflict,
            ))
        }
        _ => {
            let class = class.unwrap_or(FenceClass::UndoIncomplete);
            fence::first_fence(
                cx,
                exp,
                &plan,
                Fence {
                    engine: f.actor.engine.0,
                    cause: t_id.nonce,
                    class,
                    context: ctx,
                },
            )?;
            fence::cause_txn(cx, exp, t_id.nonce, class, fact)?;
            let t = exp.delta.txn(pre, t_id.nonce)?;
            match class {
                FenceClass::Unknown => {
                    t.prior_successful_write = true;
                    t.progress = CommitProgress::Attempted;
                }
                FenceClass::PublicationPending => {
                    t.prior_successful_write = true;
                    t.progress = CommitProgress::DurableConfirmed;
                    t.receipt = plan.receipt.map(|x| ReceiptRef(x.nonce));
                    t.delivery = Delivery::NotDelivered;
                    shared::add_receipt(exp, &plan, t_id.nonce, None)?;
                }
                FenceClass::UndoIncomplete => {}
            }
            let phase = if class == FenceClass::UndoIncomplete {
                Phase::Execute
            } else {
                Phase::Commit
            };
            expect::response(fence::row(class, phase))
        }
    };
    r.recovery_obligations = recovery_list(&plan);
    admit(cx, exp, &f, r, expect::entry_passed(o == 5, o == 6))
}

/// Obligations supplied and applied by this call, in identity order.
pub(super) fn recovery_list(plan: &EnvPlan) -> Records<Identity, 7> {
    let mut ids: Records<Identity, 7> = Records::default();
    for o in plan.obligations.as_slice() {
        let _ = ids.try_push(o.id);
    }
    ids.as_mut_slice().sort_unstable_by_key(|i| i.key());
    ids
}

pub(super) fn close_statement(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let i = cx.input;
    if i.outcome > 2 {
        return Err(CheckFailure::Input("close statement outcome"));
    }
    let Some(f) = gates::fresh(cx, exp)? else {
        return Ok(());
    };
    let Some(st) = gates::auth_statement(cx.pre, &f.actor, i.target) else {
        return gates::reject_handle(cx, exp, &f, false);
    };
    let pre = cx.pre;
    let slot = *ident::slot(pre, st.id.actor, st.id.slot).ok_or(MISSING_SLOT)?;
    let descriptor = slot
        .descriptor_result
        .and_then(|r| ident::descriptor(pre, r.0))
        .copied();
    gates::child_statement(pre, exp, &st, descriptor.and_then(|d| d.transaction));
    if let Some(t) = descriptor.and_then(|d| d.transaction) {
        exp.operated = Some(t.0);
    }
    if let Some(code) = gates::frame_gate(&f, true) {
        return gates::reject_gate(cx, exp, &f, code, false);
    }
    if st.life == StatementLife::CurrentClosed {
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
    let mut plan = EnvPlan::new(pre, FactKind::LocalCleanup, f.actor.engine.0);
    plan.cleanup = 1;
    plan.transaction = descriptor
        .and_then(|d| d.transaction)
        .and_then(|t| ident::txn(pre, t.0))
        .map(|t| t.id);
    owners::release_local(
        &mut plan,
        pre,
        st.id.nonce,
        slot.descriptor_result.map(|r| r.0),
    )?;
    plan.link_existing(pre, None)?;
    envelope(exp, &plan, cx);
    if exp.excluded != 0 {
        return Ok(());
    }
    exp.consumed = 2;
    let ctx = f.context_ref();
    let fact = FactRef(plan.fact.nonce);
    let actor = f.actor.actor;
    let mut was_open = false;
    if let (Some(r), Some(d)) = (slot.descriptor_result, descriptor) {
        was_open = d.state == ResultState::Open;
        shared::retire_result(cx, exp, r.0, Retirement::StatementClose, ctx)?;
        exp.delta.remove_descriptor(r)?;
        if f.actor.active_public_result == Some(r) {
            exp.delta.actor(pre, actor)?.active_public_result = None;
        }
        if was_open {
            shared::end_read_only(cx, exp, actor, d.transaction, fact)?;
        }
    }
    if let Some(b) = slot.binding {
        exp.delta.binding(pre, b.0)?.life = BindingLife::Released;
    }
    exp.delta.statement(pre, st.id.nonce)?.life = StatementLife::CurrentClosed;
    let s = exp.delta.slot(pre, st.id.actor, st.id.slot)?;
    s.state = SlotState::Closed;
    s.binding = None;
    s.descriptor_result = None;
    plan.apply(
        pre,
        exp,
        FactSpec {
            api: Api::CloseStatement,
            outcome: 0,
            context: Some(ctx),
            cleanup: Cleanup::Complete,
            next_work: NextWork::NotConsumed,
        },
    )?;
    let effect = if was_open {
        StatementEffect::CursorReleased
    } else {
        StatementEffect::NoEffect
    };
    let mut r = expect::response(expect::ok(Phase::Close, effect));
    r.recovery_obligations = recovery_list(&plan);
    admit(cx, exp, &f, r, expect::entry_passed(false, false))
}

pub(super) fn inspect_statement(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    let actor = gates::caller(cx)?;
    exp.caller = Some(actor.actor);
    let reject = |exp: &mut Expect, code: ReturnCode| {
        exp.class = EdgeClass::Rejected;
        exp.response = Some(expect::response(expect::entry(code, EngineCause::None)));
        if exp.authenticated.is_none() {
            exp.authenticated = exp.child;
        }
    };
    let Some(st) = gates::auth_statement(cx.pre, &actor, cx.input.target) else {
        exp.authenticated = Some(Default::default());
        reject(exp, ReturnCode::InvalidHandle);
        return Ok(());
    };
    gates::child_statement(cx.pre, exp, &st, None);
    if actor.shell == Shell::Closed {
        reject(exp, ReturnCode::SessionState);
    } else if actor.parked_context.is_some() {
        reject(exp, ReturnCode::Busy);
    } else if st.life == StatementLife::CurrentClosed {
        reject(exp, ReturnCode::HandleClosed);
    } else {
        exp.class = EdgeClass::NoEffect;
        exp.response = Some(expect::response(expect::ok(
            Phase::NotApplicable,
            StatementEffect::NoEffect,
        )));
    }
    Ok(())
}
