//! Test-only: turn the checker's own expectation into a full edge and canonical post-state.
//!
//! Used to walk deeper states and to validate expected post-states with the codec loader.
//! This is not part of `verify_edge`, may allocate, and is compiled only for tests.
use super::super::schema::*;
use super::super::{Edge, EdgeClass, Exclusion};
use super::CheckFailure;
use super::delta::Delta;
use super::expect::{Expect, bit};
use super::gates::Cx;

const RULES: [Exclusion; 14] = [
    Exclusion::ContextFresh,
    Exclusion::SignalNoDeadline,
    Exclusion::BindN0,
    Exclusion::ExecuteCommit,
    Exclusion::ReadOnlyCleanup,
    Exclusion::CommitNoWrite,
    Exclusion::CommitProgress,
    Exclusion::CommitPreattemptGate,
    Exclusion::RecoveryPre,
    Exclusion::RecoverySet,
    Exclusion::Envelope,
    Exclusion::ResumeWork,
    Exclusion::FencedClose,
    Exclusion::PeerFence,
];

/// Native bytes of the checker's two largest stack frames (`Expect`, `EnvPlan`).
pub(in super::super) fn frame_bytes() -> (usize, usize) {
    (
        std::mem::size_of::<Expect>(),
        std::mem::size_of::<super::owners::EnvPlan>(),
    )
}

/// The checker's expected edge and post-state; returns the applicable exclusion rules.
pub(in super::super) fn expected(
    pre: &State,
    input: &super::super::input_schema::ResolvedInput,
    edge: &mut Edge,
    post: &mut State,
) -> Result<Vec<Exclusion>, CheckFailure> {
    let mut exp = Expect::new(pre);
    super::build(&Cx { pre, input }, &mut exp)?;
    if exp.excluded != 0 {
        let rules: Vec<Exclusion> = RULES
            .iter()
            .copied()
            .filter(|r| exp.excluded & bit(*r) != 0)
            .collect();
        *edge = Edge {
            class: EdgeClass::Excluded(rules[0]),
            ..Edge::default()
        };
        post.copy_from(pre);
        return Ok(rules);
    }
    exp.finish(pre);
    *edge = Edge {
        class: exp.class,
        response: exp.response,
        context_event: exp.context_event,
        controls: exp.controls,
        consumed_parts: exp.consumed,
        authenticated: exp.authenticated,
    };
    apply(pre, &exp.delta, post);
    Ok(Vec::new())
}

fn patch<T: Copy + Default, const N: usize, const M: usize>(
    table: &mut Records<T, M>,
    rows: &Records<T, N>,
    same: impl Fn(&T, &T) -> bool,
) {
    for p in rows.as_slice() {
        match table.as_mut_slice().iter_mut().find(|r| same(r, p)) {
            Some(row) => *row = *p,
            None => table.try_push(*p).expect("materialized table capacity"),
        }
    }
}

type Key = (u8, u16, u16, u8, u8, u16, u16);

fn key_of<T>(rows: &[T], id: impl Fn(&T) -> Identity, nonce: u16) -> Key {
    rows.iter()
        .map(id)
        .find(|i| i.nonce == nonce)
        .map_or((0, 0, 0, 0, 0, 0, 0), Identity::key)
}

/// `pre ⊕ delta` in codec canonical order.
fn apply(pre: &State, d: &Delta, out: &mut State) {
    out.copy_from(pre);
    out.current_engine = d.current_engine;
    out.next_nonce = d.next_nonce;
    patch(&mut out.engines, &d.engines, |a, b| a.id == b.id);
    patch(&mut out.session_incarnations, &d.sessions, |a, b| {
        a.id == b.id
    });
    patch(&mut out.actors, &d.actors, |a, b| a.actor == b.actor);
    patch(&mut out.prepared_slots, &d.slots, |a, b| {
        a.actor == b.actor && a.slot == b.slot
    });
    patch(&mut out.issued_statements, &d.statements, |a, b| {
        a.id == b.id
    });
    patch(&mut out.issued_results, &d.results, |a, b| a.id == b.id);
    patch(&mut out.private_descriptors, &d.descriptors, |a, b| {
        a.result == b.result
    });
    for r in d.removed_descriptors.as_slice() {
        if let Some(i) = out
            .private_descriptors
            .as_slice()
            .iter()
            .position(|x| x.result == *r)
        {
            out.private_descriptors.remove(i);
        }
    }
    patch(&mut out.contexts, &d.contexts, |a, b| a.id == b.id);
    patch(&mut out.transactions, &d.transactions, |a, b| a.id == b.id);
    patch(&mut out.obligations, &d.obligations, |a, b| a.id == b.id);
    patch(&mut out.bindings, &d.bindings, |a, b| a.id == b.id);
    patch(&mut out.receipts, &d.receipts, |a, b| a.id == b.id);
    patch(&mut out.owners, &d.owners, |a, b| a.id == b.id);
    patch(&mut out.facts, &d.facts, |a, b| a.id == b.id);
    patch(&mut out.fact_obligations, &d.fact_obligations, |a, b| {
        a.fact == b.fact && a.obligation == b.obligation
    });
    patch(&mut out.fact_owners, &d.fact_owners, |a, b| {
        a.fact == b.fact && a.resource == b.resource
    });
    out.engines.as_mut_slice().sort_by_key(|r| r.id.key());
    out.session_incarnations
        .as_mut_slice()
        .sort_by_key(|r| r.id.key());
    out.actors.as_mut_slice().sort_by_key(|r| r.actor);
    out.prepared_slots
        .as_mut_slice()
        .sort_by_key(|r| (r.actor, r.slot));
    out.issued_statements
        .as_mut_slice()
        .sort_by_key(|r| r.id.key());
    out.issued_results
        .as_mut_slice()
        .sort_by_key(|r| r.id.key());
    let results = out.issued_results.as_slice().to_vec();
    out.private_descriptors
        .as_mut_slice()
        .sort_by_key(|r| key_of(&results, |x| x.id, r.result.0));
    out.contexts.as_mut_slice().sort_by_key(|r| r.id.key());
    out.transactions.as_mut_slice().sort_by_key(|r| r.id.key());
    out.bindings.as_mut_slice().sort_by_key(|r| r.id.key());
    out.receipts.as_mut_slice().sort_by_key(|r| r.id.key());
    out.owners.as_mut_slice().sort_by_key(|r| r.id.key());
    out.facts.as_mut_slice().sort_by_key(|r| r.id.key());
    let engines = out.engines.as_slice().to_vec();
    let txns = out.transactions.as_slice().to_vec();
    let sessions = out.session_incarnations.as_slice().to_vec();
    let obligation_key = |o: &ObligationRecord| {
        (
            key_of(&engines, |x| x.id, o.engine.0),
            key_of(&txns, |x| x.id, o.transaction.0),
            key_of(&sessions, |x| x.id, o.origin_session.0),
        )
    };
    out.obligations.as_mut_slice().sort_by_key(obligation_key);
    let obligations = out.obligations.as_slice().to_vec();
    let facts = out.facts.as_slice().to_vec();
    let owners = out.owners.as_slice().to_vec();
    out.fact_obligations.as_mut_slice().sort_by_key(|m| {
        let o = obligations
            .iter()
            .find(|o| o.id.nonce == m.obligation.0)
            .map(obligation_key);
        (key_of(&facts, |x| x.id, m.fact.0), o)
    });
    out.fact_owners.as_mut_slice().sort_by_key(|m| {
        (
            key_of(&facts, |x| x.id, m.fact.0),
            key_of(&owners, |x| x.id, m.resource.0),
        )
    });
}
