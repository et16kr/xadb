//! Expected change set of one edge and its exact comparison with the observed post-state.
//!
//! The delta holds full replacement or new records only; every record it does not name must
//! appear in the post-state exactly as in the pre-state. No state is cloned: the comparison
//! walks the observed post tables against `pre ⊕ delta` record by record.
use super::super::schema::*;
use super::CheckFailure;

/// Fixed-capacity change set; capacities bound one action of the frozen profile.
pub(super) struct Delta {
    pub current_engine: EngineRef,
    pub next_nonce: [u16; 12],
    pub engines: Records<EngineRecord, 2>,
    pub sessions: Records<SessionRecord, 2>,
    pub actors: Records<ActorRecord, 2>,
    pub slots: Records<PreparedSlot, 4>,
    pub statements: Records<StatementRecord, 8>,
    pub results: Records<ResultRecord, 8>,
    pub descriptors: Records<Descriptor, 5>,
    pub removed_descriptors: Records<ResultRef, 4>,
    pub contexts: Records<ContextRecord, 2>,
    pub transactions: Records<TransactionRecord, 8>,
    pub obligations: Records<ObligationRecord, 8>,
    pub bindings: Records<BindingRecord, 8>,
    pub receipts: Records<ReceiptRecord, 2>,
    pub owners: Records<OwnerRecord, 64>,
    pub facts: Records<FactRecord, 1>,
    pub fact_obligations: Records<FactObligation, 8>,
    pub fact_owners: Records<FactOwner, 64>,
}

const CAPACITY: CheckFailure = CheckFailure::CheckerCapacity("delta table full");
const DANGLING: CheckFailure = CheckFailure::PreState("dangling pre-state reference");

/// Patch row for `hit`, inserting a copy of the pre-state row on first use.
fn upsert<'a, T: Copy + Default, const N: usize, const M: usize>(
    patch: &'a mut Records<T, N>,
    pre: &Records<T, M>,
    hit: impl Fn(&T) -> bool,
) -> Result<&'a mut T, CheckFailure> {
    let index = match patch.as_slice().iter().position(&hit) {
        Some(i) => i,
        None => {
            let row = *pre.as_slice().iter().find(|r| hit(r)).ok_or(DANGLING)?;
            patch.try_push(row).map_err(|_| CAPACITY)?;
            patch.len() - 1
        }
    };
    Ok(&mut patch.as_mut_slice()[index])
}

fn insert<T: Copy + Default, const N: usize>(
    patch: &mut Records<T, N>,
    row: T,
) -> Result<(), CheckFailure> {
    patch.try_push(row).map_err(|_| CAPACITY)
}

fn view<T: Copy, const N: usize, const M: usize>(
    patch: &Records<T, N>,
    pre: &Records<T, M>,
    hit: impl Fn(&T) -> bool,
) -> Option<T> {
    patch
        .as_slice()
        .iter()
        .find(|r| hit(r))
        .or_else(|| pre.as_slice().iter().find(|r| hit(r)))
        .copied()
}

impl Delta {
    pub(super) fn new(pre: &State) -> Self {
        Self {
            current_engine: pre.current_engine,
            next_nonce: pre.next_nonce,
            engines: Records::default(),
            sessions: Records::default(),
            actors: Records::default(),
            slots: Records::default(),
            statements: Records::default(),
            results: Records::default(),
            descriptors: Records::default(),
            removed_descriptors: Records::default(),
            contexts: Records::default(),
            transactions: Records::default(),
            obligations: Records::default(),
            bindings: Records::default(),
            receipts: Records::default(),
            owners: Records::default(),
            facts: Records::default(),
            fact_obligations: Records::default(),
            fact_owners: Records::default(),
        }
    }

    /// Advance one kind's stored next nonce by `by`.
    pub(super) fn bump(&mut self, kind: IdentityKind, by: u16) {
        let k = kind as usize;
        if k > 0 {
            self.next_nonce[k - 1] = self.next_nonce[k - 1].wrapping_add(by);
        }
    }

    pub(super) fn engine(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut EngineRecord, CheckFailure> {
        upsert(&mut self.engines, &pre.engines, |r| r.id.nonce == n)
    }
    pub(super) fn session(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut SessionRecord, CheckFailure> {
        upsert(&mut self.sessions, &pre.session_incarnations, |r| {
            r.id.nonce == n
        })
    }
    pub(super) fn actor(&mut self, pre: &State, a: u8) -> Result<&mut ActorRecord, CheckFailure> {
        upsert(&mut self.actors, &pre.actors, |r| r.actor == a)
    }
    pub(super) fn slot(
        &mut self,
        pre: &State,
        a: u8,
        s: u8,
    ) -> Result<&mut PreparedSlot, CheckFailure> {
        upsert(&mut self.slots, &pre.prepared_slots, |r| {
            r.actor == a && r.slot == s
        })
    }
    pub(super) fn statement(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut StatementRecord, CheckFailure> {
        upsert(&mut self.statements, &pre.issued_statements, |r| {
            r.id.nonce == n
        })
    }
    pub(super) fn result(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut ResultRecord, CheckFailure> {
        upsert(&mut self.results, &pre.issued_results, |r| r.id.nonce == n)
    }
    pub(super) fn descriptor(
        &mut self,
        pre: &State,
        result: u16,
    ) -> Result<&mut Descriptor, CheckFailure> {
        upsert(&mut self.descriptors, &pre.private_descriptors, |r| {
            r.result.0 == result
        })
    }
    pub(super) fn context(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut ContextRecord, CheckFailure> {
        upsert(&mut self.contexts, &pre.contexts, |r| r.id.nonce == n)
    }
    pub(super) fn txn(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut TransactionRecord, CheckFailure> {
        upsert(&mut self.transactions, &pre.transactions, |r| {
            r.id.nonce == n
        })
    }
    pub(super) fn obligation(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut ObligationRecord, CheckFailure> {
        upsert(&mut self.obligations, &pre.obligations, |r| r.id.nonce == n)
    }
    pub(super) fn binding(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut BindingRecord, CheckFailure> {
        upsert(&mut self.bindings, &pre.bindings, |r| r.id.nonce == n)
    }
    pub(super) fn receipt(
        &mut self,
        pre: &State,
        n: u16,
    ) -> Result<&mut ReceiptRecord, CheckFailure> {
        upsert(&mut self.receipts, &pre.receipts, |r| r.id.nonce == n)
    }
    pub(super) fn owner(&mut self, pre: &State, n: u16) -> Result<&mut OwnerRecord, CheckFailure> {
        upsert(&mut self.owners, &pre.owners, |r| r.id.nonce == n)
    }

    pub(super) fn add_engine(&mut self, r: EngineRecord) -> Result<(), CheckFailure> {
        insert(&mut self.engines, r)
    }
    pub(super) fn add_session(&mut self, r: SessionRecord) -> Result<(), CheckFailure> {
        insert(&mut self.sessions, r)
    }
    pub(super) fn add_statement(&mut self, r: StatementRecord) -> Result<(), CheckFailure> {
        insert(&mut self.statements, r)
    }
    pub(super) fn add_result(&mut self, r: ResultRecord) -> Result<(), CheckFailure> {
        insert(&mut self.results, r)
    }
    pub(super) fn add_descriptor(&mut self, r: Descriptor) -> Result<(), CheckFailure> {
        insert(&mut self.descriptors, r)
    }
    pub(super) fn remove_descriptor(&mut self, result: ResultRef) -> Result<(), CheckFailure> {
        if let Some(i) = self
            .descriptors
            .as_slice()
            .iter()
            .position(|d| d.result == result)
        {
            self.descriptors.remove(i);
        }
        if !self.removed_descriptors.as_slice().contains(&result) {
            insert(&mut self.removed_descriptors, result)?;
        }
        Ok(())
    }
    pub(super) fn add_context(&mut self, r: ContextRecord) -> Result<(), CheckFailure> {
        insert(&mut self.contexts, r)
    }
    pub(super) fn add_txn(&mut self, r: TransactionRecord) -> Result<(), CheckFailure> {
        insert(&mut self.transactions, r)
    }
    pub(super) fn add_obligation(&mut self, r: ObligationRecord) -> Result<(), CheckFailure> {
        insert(&mut self.obligations, r)
    }
    pub(super) fn add_binding(&mut self, r: BindingRecord) -> Result<(), CheckFailure> {
        insert(&mut self.bindings, r)
    }
    pub(super) fn add_receipt(&mut self, r: ReceiptRecord) -> Result<(), CheckFailure> {
        insert(&mut self.receipts, r)
    }
    pub(super) fn add_owner(&mut self, r: OwnerRecord) -> Result<(), CheckFailure> {
        insert(&mut self.owners, r)
    }
    pub(super) fn add_fact(&mut self, r: FactRecord) -> Result<(), CheckFailure> {
        insert(&mut self.facts, r)
    }
    pub(super) fn add_fact_obligation(&mut self, r: FactObligation) -> Result<(), CheckFailure> {
        insert(&mut self.fact_obligations, r)
    }
    pub(super) fn add_fact_owner(&mut self, r: FactOwner) -> Result<(), CheckFailure> {
        insert(&mut self.fact_owners, r)
    }

    // Expected post-state views (patched row, else the unchanged pre-state row).
    pub(super) fn view_actor(&self, pre: &State, a: u8) -> Option<ActorRecord> {
        view(&self.actors, &pre.actors, |r| r.actor == a)
    }
    pub(super) fn view_txn(&self, pre: &State, n: u16) -> Option<TransactionRecord> {
        view(&self.transactions, &pre.transactions, |r| r.id.nonce == n)
    }
    pub(super) fn view_receipt(&self, pre: &State, n: u16) -> Option<ReceiptRecord> {
        view(&self.receipts, &pre.receipts, |r| r.id.nonce == n)
    }
    pub(super) fn view_result(&self, pre: &State, n: u16) -> Option<ResultRecord> {
        view(&self.results, &pre.issued_results, |r| r.id.nonce == n)
    }
    pub(super) fn view_descriptor(&self, pre: &State, n: u16) -> Option<Descriptor> {
        if self.removed_descriptors.as_slice().contains(&ResultRef(n)) {
            return None;
        }
        view(&self.descriptors, &pre.private_descriptors, |r| {
            r.result.0 == n
        })
    }

    /// Every expected owner, patched or unchanged, for derived aggregates.
    pub(super) fn for_each_owner(&self, pre: &State, mut f: impl FnMut(&OwnerRecord)) {
        for o in self.owners.as_slice() {
            f(o);
        }
        for o in pre.owners.as_slice() {
            if !self
                .owners
                .as_slice()
                .iter()
                .any(|p| p.id.nonce == o.id.nonce)
            {
                f(o);
            }
        }
    }
}

fn post_failure(table: &'static str, reason: &'static str) -> CheckFailure {
    CheckFailure::PostState { table, reason }
}

/// Compare one observed table with `pre ⊕ patch` minus `removed` rows.
fn compare<T: Copy + PartialEq, const N: usize, const M: usize>(
    table: &'static str,
    pre: &Records<T, M>,
    post: &Records<T, M>,
    patch: &Records<T, N>,
    removed: impl Fn(&T) -> bool,
    same: impl Fn(&T, &T) -> bool,
) -> Result<(), CheckFailure> {
    let rows = post.as_slice();
    for (i, p) in rows.iter().enumerate() {
        if rows[..i].iter().any(|q| same(q, p)) {
            return Err(post_failure(table, "duplicate record"));
        }
        if let Some(e) = patch.as_slice().iter().find(|e| same(e, p)) {
            if e != p {
                return Err(post_failure(
                    table,
                    "changed record differs from expectation",
                ));
            }
        } else if let Some(e) = pre.as_slice().iter().find(|e| same(e, p)) {
            if removed(e) {
                return Err(post_failure(table, "record retained although removed"));
            }
            if e != p {
                return Err(post_failure(table, "record changed outside the transition"));
            }
        } else {
            return Err(post_failure(table, "unexpected record"));
        }
    }
    let kept = pre.as_slice().iter().filter(|e| !removed(e)).count();
    let new = patch
        .as_slice()
        .iter()
        .filter(|e| !pre.as_slice().iter().any(|p| same(p, e)))
        .count();
    if rows.len() != kept + new {
        return Err(post_failure(table, "expected record missing"));
    }
    Ok(())
}

fn no_removal<T>(_: &T) -> bool {
    false
}

/// Exact `post == pre ⊕ delta`, including canonical record order.
pub(super) fn compare_post(pre: &State, post: &State, d: &Delta) -> Result<(), CheckFailure> {
    if post.current_engine != d.current_engine {
        return Err(post_failure("header", "current engine"));
    }
    if post.next_nonce != d.next_nonce {
        return Err(post_failure("header", "next nonce counters"));
    }
    let by_id = |a: &Identity, b: &Identity| a == b;
    compare(
        "engines",
        &pre.engines,
        &post.engines,
        &d.engines,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "session_incarnations",
        &pre.session_incarnations,
        &post.session_incarnations,
        &d.sessions,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "actors",
        &pre.actors,
        &post.actors,
        &d.actors,
        no_removal,
        |a, b| a.actor == b.actor,
    )?;
    compare(
        "prepared_slots",
        &pre.prepared_slots,
        &post.prepared_slots,
        &d.slots,
        no_removal,
        |a, b| a.actor == b.actor && a.slot == b.slot,
    )?;
    compare(
        "issued_statements",
        &pre.issued_statements,
        &post.issued_statements,
        &d.statements,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "issued_results",
        &pre.issued_results,
        &post.issued_results,
        &d.results,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "private_descriptors",
        &pre.private_descriptors,
        &post.private_descriptors,
        &d.descriptors,
        |r| d.removed_descriptors.as_slice().contains(&r.result),
        |a, b| a.result == b.result,
    )?;
    compare(
        "contexts",
        &pre.contexts,
        &post.contexts,
        &d.contexts,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "transactions",
        &pre.transactions,
        &post.transactions,
        &d.transactions,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "obligations",
        &pre.obligations,
        &post.obligations,
        &d.obligations,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "bindings",
        &pre.bindings,
        &post.bindings,
        &d.bindings,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "receipts",
        &pre.receipts,
        &post.receipts,
        &d.receipts,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "owners",
        &pre.owners,
        &post.owners,
        &d.owners,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "facts",
        &pre.facts,
        &post.facts,
        &d.facts,
        no_removal,
        |a, b| by_id(&a.id, &b.id),
    )?;
    compare(
        "fact_obligations",
        &pre.fact_obligations,
        &post.fact_obligations,
        &d.fact_obligations,
        no_removal,
        |a, b| a.fact == b.fact && a.obligation == b.obligation,
    )?;
    compare(
        "fact_owners",
        &pre.fact_owners,
        &post.fact_owners,
        &d.fact_owners,
        no_removal,
        |a, b| a.fact == b.fact && a.resource == b.resource,
    )?;
    order(post)
}

type Key = (u8, u16, u16, u8, u8, u16, u16);

fn strictly<T, K: Ord>(
    table: &'static str,
    rows: &[T],
    key: impl Fn(&T) -> Option<K>,
) -> Result<(), CheckFailure> {
    let mut previous: Option<K> = None;
    for row in rows {
        let k = key(row).ok_or(post_failure(table, "dangling reference"))?;
        if previous.as_ref().is_some_and(|p| *p >= k) {
            return Err(post_failure(table, "noncanonical record order"));
        }
        previous = Some(k);
    }
    Ok(())
}

fn key_of<T>(rows: &[T], id: impl Fn(&T) -> Identity, nonce: u16) -> Option<Key> {
    rows.iter()
        .map(id)
        .find(|i| i.nonce == nonce)
        .map(Identity::key)
}

/// Codec canonical order: identity tuples, actor/slot numbers and resolved membership keys.
fn order(s: &State) -> Result<(), CheckFailure> {
    let fact_key = |n: u16| key_of(s.facts.as_slice(), |r| r.id, n);
    let obligation_key = |n: u16| {
        let o = s.obligations.as_slice().iter().find(|o| o.id.nonce == n)?;
        Some((
            key_of(s.engines.as_slice(), |r| r.id, o.engine.0)?,
            key_of(s.transactions.as_slice(), |r| r.id, o.transaction.0)?,
            key_of(
                s.session_incarnations.as_slice(),
                |r| r.id,
                o.origin_session.0,
            )?,
        ))
    };
    strictly("engines", s.engines.as_slice(), |r| Some(r.id.key()))?;
    strictly(
        "session_incarnations",
        s.session_incarnations.as_slice(),
        |r| Some(r.id.key()),
    )?;
    strictly("actors", s.actors.as_slice(), |r| Some(r.actor))?;
    strictly("prepared_slots", s.prepared_slots.as_slice(), |r| {
        Some((r.actor, r.slot))
    })?;
    strictly("issued_statements", s.issued_statements.as_slice(), |r| {
        Some(r.id.key())
    })?;
    strictly("issued_results", s.issued_results.as_slice(), |r| {
        Some(r.id.key())
    })?;
    strictly(
        "private_descriptors",
        s.private_descriptors.as_slice(),
        |r| key_of(s.issued_results.as_slice(), |x| x.id, r.result.0),
    )?;
    strictly("contexts", s.contexts.as_slice(), |r| Some(r.id.key()))?;
    strictly("transactions", s.transactions.as_slice(), |r| {
        Some(r.id.key())
    })?;
    strictly("obligations", s.obligations.as_slice(), |r| {
        obligation_key(r.id.nonce)
    })?;
    strictly("bindings", s.bindings.as_slice(), |r| Some(r.id.key()))?;
    strictly("receipts", s.receipts.as_slice(), |r| Some(r.id.key()))?;
    strictly("owners", s.owners.as_slice(), |r| Some(r.id.key()))?;
    strictly("facts", s.facts.as_slice(), |r| Some(r.id.key()))?;
    strictly("fact_obligations", s.fact_obligations.as_slice(), |r| {
        Some((fact_key(r.fact.0)?, obligation_key(r.obligation.0)?))
    })?;
    strictly("fact_owners", s.fact_owners.as_slice(), |r| {
        Some((
            fact_key(r.fact.0)?,
            key_of(s.owners.as_slice(), |x| x.id, r.resource.0)?,
        ))
    })
}

/// Exact equality of every table, for excluded edges whose successor is the pre-state.
pub(super) fn unchanged(pre: &State, post: &State) -> Result<(), CheckFailure> {
    if pre != post {
        return Err(post_failure("state", "excluded edge changed the state"));
    }
    Ok(())
}
