//! Fresh-context envelope, child authentication and the shared admission gates.
//!
//! Order (D4-LM-MODEL-001): envelope → child authentication → Closed shell → parked Busy →
//! shared fence → API state → entry Cancelled → entry Timeout → environment → admit.
use super::super::input_schema::{RawIdentity, ResolvedInput};
use super::super::schema::*;
use super::super::{Authenticated, Check, ContextEvent, Controls, EdgeClass, Exclusion};
use super::CheckFailure;
use super::expect::{self, Expect, Row};
use super::ident::{self, pack, unpack};

/// Read-only inputs of one check.
pub(super) struct Cx<'a> {
    pub pre: &'a State,
    pub input: &'a ResolvedInput,
}

/// Caller frame of a fresh-context call.
#[derive(Clone, Copy)]
pub(super) struct Fresh {
    pub actor: ActorRecord,
    pub engine: EngineRecord,
    /// The exact fresh caller context the call must carry.
    pub context: Identity,
}

impl Fresh {
    pub(super) fn context_ref(&self) -> ContextRef {
        ContextRef(self.context.nonce)
    }
    pub(super) fn admitting(&self) -> bool {
        self.engine.admission == EngineAdmission::Admitting
    }
}

/// Caller actor of a non-recovery input.
pub(super) fn caller(cx: &Cx) -> Result<ActorRecord, CheckFailure> {
    if cx.input.actor > 1 {
        return Err(CheckFailure::Input("actor outside 0..1"));
    }
    ident::actor(cx.pre, cx.input.actor)
        .copied()
        .ok_or(CheckFailure::PreState("missing caller actor"))
}

/// Fresh envelope gate; `None` records `Excluded(ContextFresh)`.
pub(super) fn fresh(cx: &Cx, exp: &mut Expect) -> Result<Option<Fresh>, CheckFailure> {
    let actor = caller(cx)?;
    let engine = *ident::engine(cx.pre, actor.engine.0)
        .ok_or(CheckFailure::PreState("missing attached engine"))?;
    let next = ident::next(cx.pre, IdentityKind::Context);
    let context = ident::ident(
        IdentityKind::Context,
        actor.actor,
        255,
        actor.engine.0,
        actor.session.0,
        next,
        0,
    );
    exp.caller = Some(actor.actor);
    if next > 6 || cx.input.context != Some(pack(context)) {
        exp.exclude(Exclusion::ContextFresh);
        return Ok(None);
    }
    Ok(Some(Fresh {
        actor,
        engine,
        context,
    }))
}

/// Record the fresh context with its final status and advance the caller counter.
pub(super) fn register(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    data: ContextData,
) -> Result<(), CheckFailure> {
    exp.delta.add_context(ContextRecord {
        id: f.context,
        issue_ordinal: cx.pre.contexts.len() as u16 + 1,
        data,
    })?;
    exp.delta.bump(IdentityKind::Context, 1);
    exp.context_event = Some(ContextEvent {
        identity: f.context,
        before: None,
        after: data.status(),
    });
    Ok(())
}

/// Rejection before admission: the manager frame is kept, the fresh context completes.
pub(super) fn reject(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    row: Row,
    controls: Controls,
    fetch_like: bool,
) -> Result<(), CheckFailure> {
    exp.class = EdgeClass::Rejected;
    if exp.authenticated.is_none() {
        exp.authenticated = exp.child;
    }
    let mut r = expect::response(row);
    if fetch_like {
        expect::fetch_shape(&mut r, 0, false);
    }
    exp.response = Some(r);
    exp.controls = controls;
    register(cx, exp, f, ContextData::Completed)
}

/// Child-authentication failure of a fresh call.
pub(super) fn reject_handle(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    fetch_like: bool,
) -> Result<(), CheckFailure> {
    exp.authenticated = Some(Authenticated::default());
    exp.operated = None;
    reject(
        cx,
        exp,
        f,
        expect::entry(ReturnCode::InvalidHandle, EngineCause::None),
        expect::unchecked(),
        fetch_like,
    )
}

/// Shell, parked and fence gates; `fence_exempt` for CloseStatement/CloseSession.
pub(super) fn frame_gate(f: &Fresh, fence_exempt: bool) -> Option<ReturnCode> {
    if f.actor.shell == Shell::Closed {
        Some(ReturnCode::SessionState)
    } else if f.actor.parked_context.is_some() {
        Some(ReturnCode::Busy)
    } else if !fence_exempt && !f.admitting() {
        Some(ReturnCode::EngineFenced)
    } else {
        None
    }
}

/// Reject on an API/frame gate code with unchecked controls.
pub(super) fn reject_gate(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    code: ReturnCode,
    fetch_like: bool,
) -> Result<(), CheckFailure> {
    reject(
        cx,
        exp,
        f,
        expect::entry(code, f.engine.first_cause),
        expect::unchecked(),
        fetch_like,
    )
}

/// Entry Cancelled before entry Timeout; `true` when the call was rejected here.
pub(super) fn entry_gate(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    fetch_like: bool,
) -> Result<bool, CheckFailure> {
    if cx.input.cancel {
        let controls = Controls {
            entry_cancel: Check::Yes,
            entry_deadline: Check::NotChecked,
            observed_cancel: true,
            observed_deadline: false,
        };
        let row = expect::entry(ReturnCode::Cancelled, EngineCause::None);
        reject(cx, exp, f, row, controls, fetch_like)?;
        return Ok(true);
    }
    if cx.input.deadline_reached {
        let controls = Controls {
            entry_cancel: Check::No,
            entry_deadline: Check::Yes,
            observed_cancel: false,
            observed_deadline: true,
        };
        let row = expect::entry(ReturnCode::Timeout, EngineCause::None);
        reject(cx, exp, f, row, controls, fetch_like)?;
        return Ok(true);
    }
    Ok(false)
}

/// Admitted call that returns without consulting the entry controls (cached/idempotent).
pub(super) fn admit_cached(
    cx: &Cx,
    exp: &mut Expect,
    f: &Fresh,
    r: super::super::Response,
) -> Result<(), CheckFailure> {
    exp.class = EdgeClass::Admitted;
    exp.response = Some(r);
    exp.controls = expect::unchecked();
    register(cx, exp, f, ContextData::Completed)
}

/// Statement-targeted child gate passed: later rejections carry the authenticated statement
/// and the transaction the API selects (wire-v2 §4): Execute the caller's pre-state
/// transaction, CloseStatement the slot descriptor's transaction, BindAll/InspectStatement
/// none (owner decision on v5 §1).
pub(super) fn child_statement(
    pre: &State,
    exp: &mut Expect,
    st: &StatementRecord,
    transaction: Option<TransactionRef>,
) {
    exp.child = Some(Authenticated {
        statement: Some(st.id),
        result: None,
        transaction: transaction.and_then(|t| ident::txn(pre, t.0)).map(|t| t.id),
    });
}

/// Result-targeted child gate passed: later rejections carry the authenticated result and
/// its descriptor transaction.
pub(super) fn child_result(pre: &State, exp: &mut Expect, r: &ResultRecord, d: &Descriptor) {
    exp.child = Some(Authenticated {
        statement: None,
        result: Some(r.id),
        transaction: d
            .transaction
            .and_then(|t| ident::txn(pre, t.0))
            .map(|t| t.id),
    });
}

/// Exact current statement of the caller: Current or CurrentClosed with its slot backlink.
pub(super) fn auth_statement(
    pre: &State,
    actor: &ActorRecord,
    target: Option<RawIdentity>,
) -> Option<StatementRecord> {
    let id = unpack(target?)?;
    if id.kind != IdentityKind::Statement {
        return None;
    }
    let st = ident::statement(pre, id.nonce).filter(|s| s.id == id)?;
    let slot = ident::slot(pre, id.actor, id.slot)?;
    (id.actor == actor.actor
        && id.session == actor.session.0
        && id.engine == actor.engine.0
        && st.session == actor.session
        && matches!(
            st.life,
            StatementLife::Current | StatementLife::CurrentClosed
        )
        && slot.statement == Some(StatementRef(id.nonce)))
    .then_some(*st)
}

/// Exact public live result of the caller with its descriptor and slot backlink.
pub(super) fn auth_result(
    pre: &State,
    actor: &ActorRecord,
    target: Option<RawIdentity>,
) -> Option<(ResultRecord, Descriptor)> {
    let id = unpack(target?)?;
    if id.kind != IdentityKind::Result {
        return None;
    }
    let r = ident::result(pre, id.nonce).filter(|r| r.id == id)?;
    let d = ident::descriptor(pre, id.nonce)?;
    let slot = ident::slot(pre, id.actor, id.slot)?;
    (id.actor == actor.actor
        && id.session == actor.session.0
        && id.engine == actor.engine.0
        && r.retirement == Retirement::None
        && r.public_generation_live
        && d.public_generation_live
        && slot.descriptor_result == Some(ResultRef(id.nonce)))
    .then_some((*r, *d))
}
