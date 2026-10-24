//! Canonical state structure and exact cross-reference validation, without transition execution.
use super::{CodecError, ensure};
use crate::lifecycle::input_schema::ResolvedInput;
use crate::lifecycle::schema::{
    ActorRecord, Api, BindingLife, BindingRecord, CommitProgress, CompletionPhase, ContextData,
    ContextRecord, Deadline, Descriptor, EngineAdmission, EngineRecord, FactKind, FactRecord,
    HolderRef, Identity, IdentityKind, ObligationReason, ObligationRecord, OwnerDisposition,
    OwnerRecord, ReceiptKind, ReceiptRecord, Records, ResourceKind, ResultRecord, ResultState,
    Retirement, SessionRecord, Settlement, Shape, SlotState, State, StatementLife, StatementRecord,
    TransactionRecord,
};
fn valid(b: bool, why: &'static str) -> Result<(), CodecError> {
    ensure(b, 0, why)
}
trait Defined {
    fn id(&self) -> Identity;
}
macro_rules! definitions {($($ty:ty),*$(,)?)=>{$(impl Defined for $ty {fn id(&self)->Identity{self.id}})*};}
definitions!(
    EngineRecord,
    SessionRecord,
    StatementRecord,
    ResultRecord,
    ContextRecord,
    TransactionRecord,
    ObligationRecord,
    BindingRecord,
    ReceiptRecord,
    OwnerRecord,
    FactRecord
);
fn lookup<T: Defined, const N: usize>(rows: &Records<T, N>, nonce: u16) -> Result<&T, CodecError> {
    valid(nonce != 0, "zero reference")?;
    rows.as_slice()
        .iter()
        .find(|r| r.id().nonce == nonce)
        .ok_or(CodecError::new(0, "dangling typed reference"))
}
fn ordered<T, K: Ord>(rows: &[T], key: impl Fn(&T) -> K) -> Result<(), CodecError> {
    for pair in rows.windows(2) {
        valid(
            key(&pair[0]) < key(&pair[1]),
            "noncanonical order or duplicate key",
        )?;
    }
    Ok(())
}
fn scope(a: Identity, b: Identity) -> Result<(), CodecError> {
    valid(
        a.engine == b.engine && a.session == b.session && a.actor == b.actor,
        "foreign identity scope",
    )
}
fn engine_scope(a: Identity, b: Identity) -> Result<(), CodecError> {
    valid(a.engine == b.engine, "foreign engine reference")
}
fn max_nonce(kind: IdentityKind) -> u16 {
    match kind {
        IdentityKind::Engine => 7,
        IdentityKind::Session => 8,
        IdentityKind::Resource => 54,
        IdentityKind::None => 0,
        _ => 6,
    }
}
fn identity(s: &State, id: Identity, kind: IdentityKind) -> Result<(), CodecError> {
    valid(
        id.kind == kind && kind != IdentityKind::None,
        "identity namespace",
    )?;
    valid(
        id.nonce > 0 && id.nonce <= max_nonce(kind),
        "identity nonce bound",
    )?;
    valid(id.engine > 0 && id.engine <= 7, "identity engine bound")?;
    lookup(&s.engines, id.engine)?;
    match kind {
        IdentityKind::Engine => valid(
            id.actor == 255
                && id.slot == 255
                && id.session == 0
                && id.generation == 0
                && id.engine == id.nonce,
            "engine identity envelope",
        ),
        IdentityKind::Fact | IdentityKind::Anchor => valid(
            id.actor == 255 && id.slot == 255 && id.session == 0 && id.generation == 0,
            "engine-wide identity envelope",
        ),
        _ => {
            valid(
                id.actor <= 1 && id.session > 0 && id.session <= 8,
                "session identity envelope",
            )?;
            let session = lookup(&s.session_incarnations, id.session)?;
            valid(
                session.id.actor == id.actor && session.id.engine == id.engine,
                "identity session scope",
            )?;
            match kind {
                IdentityKind::Session => valid(
                    id.slot == 255 && id.generation == 0 && id.session == id.nonce,
                    "session identity envelope",
                ),
                IdentityKind::Statement | IdentityKind::Result => valid(
                    id.slot <= 1 && (1..=12).contains(&id.generation),
                    "handle identity envelope",
                ),
                _ => valid(
                    id.slot == 255 && id.generation == 0,
                    "nonhandle identity envelope",
                ),
            }
        }
    }
}
fn table<T: Defined, const N: usize>(
    s: &State,
    rows: &Records<T, N>,
    kind: IdentityKind,
    sort: bool,
) -> Result<u16, CodecError> {
    if sort {
        ordered(rows.as_slice(), |r| r.id().key())?;
    }
    let mut max = 0;
    for (i, row) in rows.as_slice().iter().enumerate() {
        let id = row.id();
        identity(s, id, kind)?;
        valid(
            !rows.as_slice()[..i]
                .iter()
                .any(|old| old.id().nonce == id.nonce),
            "duplicate namespace nonce",
        )?;
        max = max.max(id.nonce);
    }
    Ok(max)
}
fn context(s: &State, owner: Identity, nonce: u16, peer_fence: bool) -> Result<(), CodecError> {
    let c = lookup(&s.contexts, nonce)?;
    engine_scope(owner, c.id)?;
    if owner.actor == 255 {
        return Ok(());
    }
    if owner.actor == c.id.actor && owner.session == c.id.session {
        return Ok(());
    }
    let e = lookup(&s.engines, owner.engine)?;
    valid(
        peer_fence && e.first_fence_context.is_some_and(|r| r.0 == nonce),
        "foreign context provenance",
    )
}
fn holder(s: &State, h: HolderRef) -> Result<Identity, CodecError> {
    match h.kind {
        IdentityKind::Engine => Ok(lookup(&s.engines, h.nonce)?.id),
        IdentityKind::Statement => Ok(lookup(&s.issued_statements, h.nonce)?.id),
        IdentityKind::Result => Ok(lookup(&s.issued_results, h.nonce)?.id),
        IdentityKind::Transaction => Ok(lookup(&s.transactions, h.nonce)?.id),
        IdentityKind::Obligation => Ok(lookup(&s.obligations, h.nonce)?.id),
        _ => Err(CodecError::new(0, "invalid holder namespace")),
    }
}
fn same_holder_scope(s: &State, owner: Identity, h: HolderRef) -> Result<(), CodecError> {
    let id = holder(s, h)?;
    if id.kind == IdentityKind::Engine {
        engine_scope(owner, id)
    } else {
        scope(owner, id)
    }
}
fn ordinal(value: u16) -> Result<(), CodecError> {
    valid((1..=6).contains(&value), "issue ordinal bound")
}
fn actor(s: &State, n: u8) -> Result<&ActorRecord, CodecError> {
    s.actors
        .as_slice()
        .iter()
        .find(|a| a.actor == n)
        .ok_or(CodecError::new(0, "actor reference"))
}
fn descriptor(s: &State, n: u16) -> Result<&Descriptor, CodecError> {
    s.private_descriptors
        .as_slice()
        .iter()
        .find(|d| d.result.0 == n)
        .ok_or(CodecError::new(0, "descriptor reference"))
}
fn outcome(api: Api, n: u8) -> bool {
    let max = match api {
        Api::Prepare | Api::CloseResult | Api::CloseStatement | Api::Begin | Api::SetAutocommit => {
            2
        }
        Api::BindAll | Api::Execute => 10,
        Api::Fetch | Api::CommitAtomic | Api::ResumeFetch => 9,
        Api::Rollback | Api::CloseSession => 3,
        Api::ReopenSession | Api::SignalContext | Api::ParkFetch | Api::BeginCommit => 1,
        Api::InspectStatement | Api::InspectSession => 0,
        Api::CommitFact | Api::FinishCommit | Api::RecoveryConfirmedFreshEngine => 4,
    };
    n <= max
}
pub(super) fn input(i: &ResolvedInput) -> Result<(), CodecError> {
    // Do not validate raw identities, UTF-8, recovery set equality/order, or API admission here.
    valid(
        if i.api == Api::RecoveryConfirmedFreshEngine {
            i.actor == 255
        } else {
            i.actor <= 1
        },
        "input actor tag",
    )?;
    valid(outcome(i.api, i.outcome), "input outcome tag")?;
    valid(
        matches!(i.prepare_slot, 0 | 1 | 255)
            && matches!(i.prepare_shape, 0 | 1 | 255)
            && matches!(i.prepare_n, 0 | 1 | 255)
            && matches!(i.set_autocommit, 0 | 1 | 255),
        "input scalar tag",
    )
}
pub(super) fn state(s: &State) -> Result<(), CodecError> {
    valid(
        s.actors.len() == 2 && s.prepared_slots.len() == 4,
        "required section count",
    )?;
    lookup(&s.engines, s.current_engine.0)?;
    let maxima = [
        table(s, &s.engines, IdentityKind::Engine, true)?,
        table(s, &s.session_incarnations, IdentityKind::Session, true)?,
        table(s, &s.issued_statements, IdentityKind::Statement, true)?,
        table(s, &s.issued_results, IdentityKind::Result, true)?,
        table(s, &s.contexts, IdentityKind::Context, true)?,
        table(s, &s.transactions, IdentityKind::Transaction, true)?,
        table(s, &s.bindings, IdentityKind::Binding, true)?,
        table(s, &s.receipts, IdentityKind::Receipt, true)?,
        table(s, &s.owners, IdentityKind::Resource, true)?,
        table(s, &s.obligations, IdentityKind::Obligation, false)?,
        table(s, &s.facts, IdentityKind::Fact, true)?,
        anchors(s)?,
    ];
    for (next, max) in s.next_nonce.iter().zip(maxima) {
        valid(
            *next == max + 1,
            "next nonce does not follow retained history",
        )?;
    }
    ordered(s.actors.as_slice(), |a| a.actor)?;
    ordered(s.prepared_slots.as_slice(), |p| (p.actor, p.slot))?;
    for (i, a) in s.actors.as_slice().iter().enumerate() {
        valid(usize::from(a.actor) == i, "actor range")?;
        let session = lookup(&s.session_incarnations, a.session.0)?;
        lookup(&s.engines, a.engine.0)?;
        valid(
            session.id.actor == a.actor
                && session.engine == a.engine
                && session.lifecycle == a.shell,
            "actor incarnation link",
        )?;
        if let Some(t) = a.transaction {
            scope(session.id, lookup(&s.transactions, t.0)?.id)?;
        }
        if let Some(c) = a.parked_context {
            let c = lookup(&s.contexts, c.0)?;
            scope(session.id, c.id)?;
            valid(
                !matches!(c.data, ContextData::Completed),
                "actor references completed continuation",
            )?;
        }
        if let Some(result) = a.active_public_result {
            let row = lookup(&s.issued_results, result.0)?;
            scope(session.id, row.id)?;
            let d = descriptor(s, result.0)?;
            valid(
                d.state == ResultState::Open
                    && d.public_generation_live
                    && row.public_generation_live,
                "active public result link",
            )?;
        }
        let opens = s
            .private_descriptors
            .as_slice()
            .iter()
            .filter(|d| {
                d.state == ResultState::Open
                    && d.public_generation_live
                    && s.issued_results
                        .as_slice()
                        .iter()
                        .any(|r| r.id.nonce == d.result.0 && r.id.actor == a.actor)
            })
            .count();
        valid(
            opens == usize::from(a.active_public_result.is_some()),
            "actor public result census",
        )?;
    }
    engines_sessions(s)?;
    slots_statements(s)?;
    results_contexts(s)?;
    transactions_receipts(s)?;
    owners_obligations(s)?;
    facts(s)
}
fn anchors(s: &State) -> Result<u16, CodecError> {
    let mut max = 0;
    for (i, r) in s.receipts.as_slice().iter().enumerate() {
        if let Some(id) = r.anchor {
            identity(s, id, IdentityKind::Anchor)?;
            valid(
                !s.receipts.as_slice()[..i]
                    .iter()
                    .any(|p| p.anchor.is_some_and(|x| x.nonce == id.nonce)),
                "duplicate anchor nonce",
            )?;
            max = max.max(id.nonce);
        }
    }
    Ok(max)
}
fn engines_sessions(s: &State) -> Result<(), CodecError> {
    for e in s.engines.as_slice() {
        if let Some(t) = e.cause_transaction {
            engine_scope(e.id, lookup(&s.transactions, t.0)?.id)?;
        }
        if let Some(c) = e.first_fence_context {
            context(s, e.id, c.0, false)?;
        }
        if let Some(next) = e.successor_engine {
            lookup(&s.engines, next.0)?;
            valid(next.0 != e.id.nonce, "engine self successor")?;
        }
        if let Some(f) = e.recovery_fact {
            engine_scope(e.id, lookup(&s.facts, f.0)?.id)?;
        }
    }
    for session in s.session_incarnations.as_slice() {
        valid(
            session.actor == session.id.actor && session.engine.0 == session.id.engine,
            "session redundant identity",
        )?;
        if let Some(previous) = session.previous_session {
            let old = lookup(&s.session_incarnations, previous.0)?;
            valid(
                old.id.actor == session.id.actor && old.id.nonce != session.id.nonce,
                "previous session link",
            )?;
        }
        if let Some(c) = session.closing_context {
            context(s, session.id, c.0, false)?;
        }
    }
    Ok(())
}
fn slots_statements(s: &State) -> Result<(), CodecError> {
    for (i, p) in s.prepared_slots.as_slice().iter().enumerate() {
        valid(
            usize::from(p.actor) == i / 2 && usize::from(p.slot) == i % 2,
            "slot range",
        )?;
        valid(
            (1..=13).contains(&p.next_statement_generation)
                && (1..=13).contains(&p.next_result_generation),
            "slot generation bound",
        )?;
        let a = actor(s, p.actor)?;
        // A current slot must not reuse a generation from its own retained incarnation.
        for old in s.issued_statements.as_slice().iter().filter(|old| {
            old.id.actor == p.actor && old.id.slot == p.slot && old.id.session == a.session.0
        }) {
            valid(
                p.next_statement_generation > old.id.generation,
                "historical statement generation regression",
            )?;
        }
        for old in s.issued_results.as_slice().iter().filter(|old| {
            old.id.actor == p.actor && old.id.slot == p.slot && old.id.session == a.session.0
        }) {
            valid(
                p.next_result_generation > old.id.generation,
                "historical result generation regression",
            )?;
        }
        if let Some(statement) = p.statement {
            let st = lookup(&s.issued_statements, statement.0)?;
            valid(
                st.id.actor == p.actor
                    && st.id.slot == p.slot
                    && st.id.session == a.session.0
                    && st.id.engine == a.engine.0
                    && st.life != StatementLife::Retired,
                "current slot statement link",
            )?;
            valid(
                p.next_statement_generation > st.id.generation,
                "statement generation regression",
            )?;
            if let Some(binding) = p.binding {
                let b = lookup(&s.bindings, binding.0)?;
                valid(
                    b.statement == statement && b.life == BindingLife::Current,
                    "slot binding link",
                )?;
            }
            if let Some(result) = p.descriptor_result {
                let d = descriptor(s, result.0)?;
                let r = lookup(&s.issued_results, result.0)?;
                valid(
                    d.statement == statement && r.statement == statement,
                    "slot descriptor link",
                )?;
                // A public Open descriptor has exactly one current ResultOpen slot.
                // Fenced private descriptors retain Open/terminal data after public retirement.
                if d.state == ResultState::Open && d.public_generation_live {
                    valid(
                        p.state == SlotState::ResultOpen,
                        "public Open descriptor slot state",
                    )?;
                }
                valid(
                    p.next_result_generation > r.id.generation,
                    "result generation regression",
                )?;
            }
            valid(
                (p.state == SlotState::Closed) == (st.life == StatementLife::CurrentClosed),
                "closed slot statement link",
            )?;
        } else {
            valid(
                p.state == SlotState::Vacant
                    && p.binding.is_none()
                    && p.descriptor_result.is_none(),
                "vacant slot payload",
            )?;
        }
        valid(
            p.state != SlotState::Vacant || p.statement.is_none(),
            "vacant slot statement",
        )?;
        if p.state == SlotState::ReadyUnbound {
            valid(p.binding.is_none(), "unbound slot binding")?;
        }
        if matches!(p.state, SlotState::ReadyBound | SlotState::ResultOpen) {
            valid(p.binding.is_some(), "bound slot missing binding")?;
        }
        if p.state == SlotState::ResultOpen {
            let d = descriptor(
                s,
                p.descriptor_result
                    .ok_or(CodecError::new(0, "result slot missing descriptor"))?
                    .0,
            )?;
            valid(
                d.state == ResultState::Open,
                "result slot terminal descriptor",
            )?;
        }
    }
    for (i, st) in s.issued_statements.as_slice().iter().enumerate() {
        ordinal(st.issue_ordinal)?;
        valid(
            !s.issued_statements.as_slice()[..i]
                .iter()
                .any(|old| old.issue_ordinal == st.issue_ordinal),
            "duplicate statement ordinal",
        )?;
        scope(st.id, lookup(&s.session_incarnations, st.session.0)?.id)?;
        valid(st.id.session == st.session.0, "statement session link")?;
        valid(st.parameter_count <= 1, "parameter count bound")?;
        let expected = (if st.shape == Shape::Dml { 2 } else { 0 }) + st.parameter_count;
        valid(st.metadata as u8 == expected, "statement metadata shape")?;
        if let Some(c) = st.creation_context {
            context(s, st.id, c.0, false)?;
        }
        if let Some(c) = st.retirement_context {
            context(s, st.id, c.0, false)?;
        }
        let count = s
            .prepared_slots
            .as_slice()
            .iter()
            .filter(|p| p.statement.is_some_and(|r| r.0 == st.id.nonce))
            .count();
        valid(
            count == usize::from(st.life != StatementLife::Retired),
            "statement slot census",
        )?;
    }
    for (i, b) in s.bindings.as_slice().iter().enumerate() {
        ordinal(b.issue_ordinal)?;
        valid(
            !s.bindings.as_slice()[..i]
                .iter()
                .any(|old| old.issue_ordinal == b.issue_ordinal),
            "duplicate binding ordinal",
        )?;
        scope(b.id, lookup(&s.issued_statements, b.statement.0)?.id)?;
        if let Some(next) = b.replaced_by {
            let next = lookup(&s.bindings, next.0)?;
            valid(
                next.id.nonce != b.id.nonce && next.statement == b.statement,
                "binding replacement link",
            )?;
        }
        if let Some(c) = b.creation_context {
            context(s, b.id, c.0, false)?;
        }
    }
    Ok(())
}
fn results_contexts(s: &State) -> Result<(), CodecError> {
    for (i, r) in s.issued_results.as_slice().iter().enumerate() {
        ordinal(r.issue_ordinal)?;
        valid(
            !s.issued_results.as_slice()[..i]
                .iter()
                .any(|old| old.issue_ordinal == r.issue_ordinal),
            "duplicate result ordinal",
        )?;
        let st = lookup(&s.issued_statements, r.statement.0)?;
        scope(r.id, st.id)?;
        valid(r.id.slot == st.id.slot, "result statement slot")?;
        if let Some(c) = r.creation_context {
            context(s, r.id, c.0, false)?;
        }
        if let Some(c) = r.retirement_context {
            context(s, r.id, c.0, r.retirement == Retirement::EngineFence)?;
        }
    }
    // Resolve before comparing numeric keys; dangling references cannot determine order.
    for d in s.private_descriptors.as_slice() {
        let r = lookup(&s.issued_results, d.result.0)?;
        let st = lookup(&s.issued_statements, d.statement.0)?;
        valid(
            r.statement == d.statement && r.public_generation_live == d.public_generation_live,
            "descriptor result identity link",
        )?;
        scope(r.id, st.id)?;
        valid(d.position <= 6, "descriptor position bound")?;
        valid(
            d.terminal.is_some() == (d.state == ResultState::Error),
            "terminal payload shape",
        )?;
        if let Some(t) = d.transaction {
            scope(r.id, lookup(&s.transactions, t.0)?.id)?;
        }
        valid(
            s.prepared_slots
                .as_slice()
                .iter()
                .filter(|p| {
                    p.descriptor_result == Some(d.result) && p.statement == Some(d.statement)
                })
                .count()
                == 1,
            "descriptor slot census",
        )?;
    }
    let ds = s.private_descriptors.as_slice();
    for pair in ds.windows(2) {
        let a = lookup(&s.issued_results, pair[0].result.0)?;
        let b = lookup(&s.issued_results, pair[1].result.0)?;
        valid(a.id.key() < b.id.key(), "descriptor numeric order")?;
    }
    for (i, c) in s.contexts.as_slice().iter().enumerate() {
        ordinal(c.issue_ordinal)?;
        valid(
            !s.contexts.as_slice()[..i]
                .iter()
                .any(|old| old.issue_ordinal == c.issue_ordinal),
            "duplicate context ordinal",
        )?;
        let (p, fetch) = match &c.data {
            ContextData::Completed => continue,
            ContextData::ParkedFetch(p) => (p, true),
            ContextData::ParkedCommit(p) => (p, false),
        };
        valid(
            p.api
                == if fetch {
                    Api::ParkFetch
                } else {
                    Api::BeginCommit
                },
            "parked API/status mismatch",
        )?;
        valid(
            !p.cancel_observed_at_completion
                && !p.deadline_observed_at_completion
                && p.completion_phase == CompletionPhase::NotCompleted,
            "parked completion payload",
        )?;
        valid(
            p.original_deadline != Deadline::Absent || !p.deadline_reached,
            "absent parked deadline reached",
        )?;
        valid(
            p.statement.is_some() == fetch
                && p.result.is_some() == fetch
                && p.transaction.is_some(),
            "parked required target",
        )?;
        let t = lookup(
            &s.transactions,
            p.transaction
                .ok_or(CodecError::new(0, "parked transaction"))?
                .0,
        )?;
        scope(c.id, t.id)?;
        let a = actor(s, c.id.actor)?;
        valid(
            a.session.0 == c.id.session
                && a.parked_context.is_some_and(|r| r.0 == c.id.nonce)
                && a.transaction == p.transaction,
            "actor parked link",
        )?;
        valid(
            !s.contexts.as_slice()[..i].iter().any(|old| {
                old.id.actor == c.id.actor && !matches!(old.data, ContextData::Completed)
            }),
            "two parked contexts for actor",
        )?;
        valid(
            t.parked_context.is_some_and(|r| r.0 == c.id.nonce),
            "transaction parked context link",
        )?;
        if fetch {
            let result = p.result.ok_or(CodecError::new(0, "parked result"))?;
            let d = descriptor(s, result.0)?;
            let r = lookup(&s.issued_results, result.0)?;
            scope(c.id, r.id)?;
            valid(
                Some(d.statement) == p.statement && d.transaction == p.transaction,
                "parked fetch descriptor link",
            )?;
        }
    }
    Ok(())
}
fn transactions_receipts(s: &State) -> Result<(), CodecError> {
    for t in s.transactions.as_slice() {
        let birth = lookup(&s.session_incarnations, t.birth_session.0)?;
        scope(t.id, birth.id)?;
        valid(
            t.id.session == t.birth_session.0,
            "transaction birth session",
        )?;
        if let Some(receipt) = t.receipt {
            let r = lookup(&s.receipts, receipt.0)?;
            valid(r.transaction.0 == t.id.nonce, "transaction receipt link")?;
        }
        if let Some(c) = t.parked_context {
            let c = lookup(&s.contexts, c.0)?;
            scope(t.id, c.id)?;
            valid(
                matches!(&c.data,ContextData::ParkedFetch(p) | ContextData::ParkedCommit(p) if p.transaction.is_some_and(|r|r.0==t.id.nonce)),
                "transaction references wrong continuation",
            )?;
        }
        if let Some(o) = t.obligation {
            let o = lookup(&s.obligations, o.0)?;
            valid(o.transaction.0 == t.id.nonce, "transaction obligation link")?;
        }
        if let Some(f) = t.completion_fact {
            let f = lookup(&s.facts, f.0)?;
            valid(
                f.transaction.is_some_and(|r| r.0 == t.id.nonce) && f.engine.0 == t.id.engine,
                "transaction completion fact link",
            )?;
        }
        if matches!(
            t.progress,
            CommitProgress::DurableConfirmed | CommitProgress::Published
        ) {
            let r = lookup(
                &s.receipts,
                t.receipt
                    .ok_or(CodecError::new(0, "durable transaction missing receipt"))?
                    .0,
            )?;
            valid(
                r.kind == ReceiptKind::DurableWrite,
                "durable transaction receipt kind",
            )?;
        }
        if t.progress == CommitProgress::NoWriteValidated {
            if let Some(receipt) = t.receipt {
                valid(
                    lookup(&s.receipts, receipt.0)?.kind == ReceiptKind::NoWriteCompletion,
                    "no-write transaction receipt kind",
                )?;
            } else {
                valid(
                    t.delivery == crate::lifecycle::schema::Delivery::NotApplicable,
                    "identity-free no-write delivery",
                )?;
            }
        }
    }
    for (i, r) in s.receipts.as_slice().iter().enumerate() {
        let t = lookup(&s.transactions, r.transaction.0)?;
        scope(r.id, t.id)?;
        valid(r.engine.0 == r.id.engine, "receipt engine link")?;
        lookup(&s.engines, r.engine.0)?;
        valid(
            !s.receipts.as_slice()[..i]
                .iter()
                .any(|old| old.transaction == r.transaction),
            "duplicate transaction receipt",
        )?;
        valid(
            r.anchor.is_some() == (r.kind == ReceiptKind::DurableWrite),
            "receipt anchor shape",
        )?;
        if let Some(anchor) = r.anchor {
            engine_scope(r.id, anchor)?;
        }
        if let Some(f) = r.source_fact {
            let f = lookup(&s.facts, f.0)?;
            engine_scope(r.id, f.id)?;
            valid(
                f.transaction == Some(r.transaction),
                "receipt source transaction",
            )?;
        }
        if let Some(c) = r.delivery_context {
            context(s, r.id, c.0, false)?;
        }
    }
    Ok(())
}
type IdentityKey = (u8, u16, u16, u8, u8, u16, u16);
type ObligationKey = (IdentityKey, IdentityKey, IdentityKey);
fn obligation_key(s: &State, o: &ObligationRecord) -> Result<ObligationKey, CodecError> {
    Ok((
        lookup(&s.engines, o.engine.0)?.id.key(),
        lookup(&s.transactions, o.transaction.0)?.id.key(),
        lookup(&s.session_incarnations, o.origin_session.0)?
            .id
            .key(),
    ))
}
fn owners_obligations(s: &State) -> Result<(), CodecError> {
    for owner in s.owners.as_slice() {
        valid(owner.transfer_count <= 6, "owner transfer bound")?;
        same_holder_scope(s, owner.id, owner.original_holder)?;
        valid(
            owner.current_holder.is_some() != (owner.disposition == OwnerDisposition::Released),
            "owner disposition holder shape",
        )?;
        if let Some(h) = owner.current_holder {
            same_holder_scope(s, owner.id, h)?;
        }
        if let Some(h) = owner.previous_holder {
            same_holder_scope(s, owner.id, h)?;
        }
        if let Some(c) = owner.birth_context {
            context(
                s,
                owner.id,
                c.0,
                owner.resource_kind == ResourceKind::Recovery,
            )?;
        }
        if let Some(f) = owner.last_fact {
            engine_scope(owner.id, lookup(&s.facts, f.0)?.id)?;
        }
    }
    for pair in s.obligations.as_slice().windows(2) {
        valid(
            obligation_key(s, &pair[0])? < obligation_key(s, &pair[1])?,
            "obligation key duplicate or order",
        )?;
    }
    for o in s.obligations.as_slice() {
        let t = lookup(&s.transactions, o.transaction.0)?;
        let origin = lookup(&s.session_incarnations, o.origin_session.0)?;
        lookup(&s.engines, o.engine.0)?;
        scope(o.id, t.id)?;
        scope(o.id, origin.id)?;
        valid(
            o.id.engine == o.engine.0 && o.id.session == o.origin_session.0,
            "obligation identity link",
        )?;
        valid(
            o.reason_mask & !15 == 0
                && (o.reason_mask & (1 << (o.creation_reason as u8))) != 0
                && (o.reason_mask & (1 << (o.latest_reason as u8))) != 0,
            "obligation reason mask",
        )?;
        for f in [o.first_fact, o.latest_fact, o.recovery_fact]
            .into_iter()
            .flatten()
        {
            engine_scope(o.id, lookup(&s.facts, f.0)?.id)?;
        }
        if o.settlement == Settlement::RecoveryConfirmed {
            let f = lookup(
                &s.facts,
                o.recovery_fact
                    .ok_or(CodecError::new(0, "settled obligation missing evidence"))?
                    .0,
            )?;
            valid(
                f.kind == FactKind::RecoveryConfirmed
                    && f.api == Api::RecoveryConfirmedFreshEngine
                    && f.fresh_engine.is_some(),
                "recovery settlement evidence",
            )?;
        }
        if o.settlement == Settlement::DrainedConfirmed {
            let f = lookup(
                &s.facts,
                o.recovery_fact
                    .ok_or(CodecError::new(0, "drained obligation missing evidence"))?
                    .0,
            )?;
            valid(
                o.creation_reason == ObligationReason::PeerActiveTransactionAtFence
                    && f.kind == FactKind::CommitProgress
                    && f.api == Api::CommitFact
                    && f.outcome_class == 2
                    && f.transaction == Some(o.transaction)
                    && f.fresh_engine.is_none()
                    && t.progress == CommitProgress::Published,
                "drained settlement Published evidence",
            )?;
            valid(
                lookup(&s.engines, o.engine.0)?.admission != EngineAdmission::Admitting,
                "drained obligation lost fence",
            )?;
            for owner in s.owners.as_slice().iter().filter(|owner| {
                owner.original_holder
                    == HolderRef {
                        kind: IdentityKind::Transaction,
                        nonce: o.transaction.0,
                    }
                    && matches!(
                        owner.resource_kind,
                        ResourceKind::Undo
                            | ResourceKind::Locks
                            | ResourceKind::AbortReserve
                            | ResourceKind::Recovery
                    )
            }) {
                valid(
                    owner.disposition == OwnerDisposition::Released
                        && owner.current_holder.is_none(),
                    "drained owner not released",
                )?;
                valid(
                    s.fact_owners.as_slice().iter().any(|m| {
                        m.fact.0 == f.id.nonce
                            && m.resource.0 == owner.id.nonce
                            && m.obligation.is_some_and(|r| r.0 == o.id.nonce)
                            && m.disposition == OwnerDisposition::Released
                            && m.after_holder.is_none()
                    }),
                    "drained owner missing fact release",
                )?;
            }
        }
    }
    Ok(())
}
fn facts(s: &State) -> Result<(), CodecError> {
    for f in s.facts.as_slice() {
        valid(f.id.engine == f.engine.0, "fact engine link")?;
        lookup(&s.engines, f.engine.0)?;
        valid(
            outcome(f.api, f.outcome_class) && f.consumed_parts & !31 == 0,
            "fact outcome or consumption bits",
        )?;
        valid(
            f.obligation_count <= 6 && f.owner_count <= 54,
            "fact membership count cap",
        )?;
        let expected = match f.kind {
            FactKind::Execution => f.api == Api::Execute,
            FactKind::FetchWorkOrCleanup => matches!(f.api, Api::Fetch | Api::ResumeFetch),
            FactKind::CommitAtomic => f.api == Api::CommitAtomic,
            FactKind::CommitProgress => f.api == Api::CommitFact,
            FactKind::CommitFinish => f.api == Api::FinishCommit,
            FactKind::RollbackCleanup => f.api == Api::Rollback,
            FactKind::SessionCleanup => f.api == Api::CloseSession,
            FactKind::RecoveryConfirmed => f.api == Api::RecoveryConfirmedFreshEngine,
            FactKind::LocalCleanup => matches!(f.api, Api::CloseResult | Api::CloseStatement),
        };
        valid(expected, "fact kind API mismatch")?;
        if let Some(c) = f.context {
            context(s, f.id, c.0, false)?;
        }
        if let Some(t) = f.transaction {
            engine_scope(f.id, lookup(&s.transactions, t.0)?.id)?;
        }
        if let Some(r) = f.receipt {
            engine_scope(f.id, lookup(&s.receipts, r.0)?.id)?;
        }
        if let Some(e) = f.fresh_engine {
            lookup(&s.engines, e.0)?;
        }
        if let Some(a) = f.anchor {
            let anchor = s
                .receipts
                .as_slice()
                .iter()
                .filter_map(|r| r.anchor)
                .find(|id| id.nonce == a.0)
                .ok_or(CodecError::new(0, "dangling anchor reference"))?;
            engine_scope(f.id, anchor)?;
        }
        valid(
            s.fact_obligations
                .as_slice()
                .iter()
                .filter(|m| m.fact.0 == f.id.nonce)
                .count()
                == usize::from(f.obligation_count)
                && s.fact_owners
                    .as_slice()
                    .iter()
                    .filter(|m| m.fact.0 == f.id.nonce)
                    .count()
                    == usize::from(f.owner_count),
            "fact membership census",
        )?;
    }
    for m in s.fact_obligations.as_slice() {
        let f = lookup(&s.facts, m.fact.0)?;
        let o = lookup(&s.obligations, m.obligation.0)?;
        engine_scope(f.id, o.id)?;
        valid(
            m.owner_count <= 54
                && s.fact_owners
                    .as_slice()
                    .iter()
                    .filter(|owner| owner.fact == m.fact && owner.obligation == Some(m.obligation))
                    .count()
                    == usize::from(m.owner_count),
            "fact obligation owner census",
        )?;
    }
    for pair in s.fact_obligations.as_slice().windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let ak = (
            lookup(&s.facts, a.fact.0)?.id.key(),
            obligation_key(s, lookup(&s.obligations, a.obligation.0)?)?,
        );
        let bk = (
            lookup(&s.facts, b.fact.0)?.id.key(),
            obligation_key(s, lookup(&s.obligations, b.obligation.0)?)?,
        );
        valid(ak < bk, "fact obligation duplicate or order")?;
    }
    for m in s.fact_owners.as_slice() {
        let f = lookup(&s.facts, m.fact.0)?;
        let owner = lookup(&s.owners, m.resource.0)?;
        engine_scope(f.id, owner.id)?;
        valid(
            m.after_holder.is_some() != (m.disposition == OwnerDisposition::Released),
            "fact owner disposition holder shape",
        )?;
        for h in [m.before_holder, m.after_holder].into_iter().flatten() {
            same_holder_scope(s, owner.id, h)?;
        }
        if let Some(o) = m.obligation {
            scope(owner.id, lookup(&s.obligations, o.0)?.id)?;
            valid(
                s.fact_obligations
                    .as_slice()
                    .iter()
                    .any(|x| x.fact == m.fact && x.obligation == o),
                "fact owner missing obligation membership",
            )?;
        }
    }
    for pair in s.fact_owners.as_slice().windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let ak = (
            lookup(&s.facts, a.fact.0)?.id.key(),
            lookup(&s.owners, a.resource.0)?.id.key(),
        );
        let bk = (
            lookup(&s.facts, b.fact.0)?.id.key(),
            lookup(&s.owners, b.resource.0)?.id.key(),
        );
        valid(ak < bk, "fact owner duplicate or order")?;
    }
    Ok(())
}
