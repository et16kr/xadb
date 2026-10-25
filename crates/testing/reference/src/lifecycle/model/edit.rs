//! Successor editing: exact lookups, identity issue, fact recording and canonical order.
use super::{CAPACITY, ENVELOPE, INVALID, Step, unpack};
use crate::lifecycle::input_schema::{Environment, InputObligation, InputOwner, RawIdentity};
use crate::lifecycle::schema::{
    ActorRecord, AnchorRef, Api, Cleanup, ContextRef, EngineRef, FactRef, ObligationRef,
    ReceiptRef, ResourceRef, TransactionRef,
};
use crate::lifecycle::schema::{
    BindingRecord, ContextData, ContextRecord, Decision, Delivery, Descriptor, EngineRecord,
    FactKind, FactObligation, FactOwner, FactRecord, HolderRef, Identity, IdentityKind, NextWork,
    ObligationReason, ObligationRecord, OwnerDisposition, OwnerRecord, OwnershipSummary,
    PreparedSlot, ReceiptRecord, Records, ResourceKind, ResultRecord, Settlement, State,
    StatementRecord, TransactionRecord,
};

/// Exactly one matching row index.
fn position<T>(rows: &[T], matches: impl Fn(&T) -> bool) -> Step<usize> {
    let mut found = rows.iter().enumerate().filter(|(_, r)| matches(r));
    let (index, _) = found.next().ok_or(INVALID)?;
    if found.next().is_some() {
        return Err(INVALID);
    }
    Ok(index)
}

fn push<T: Copy, const N: usize>(table: &mut Records<T, N>, row: T) -> Step<()> {
    table.try_push(row).map_err(|_| CAPACITY)
}

/// Insertion sort of the initialized prefix by a computed key; tables are small and bounded.
fn sort_by<T: Copy, K: Ord>(rows: &mut [T], key: impl Fn(&T) -> K) {
    for i in 1..rows.len() {
        let mut j = i;
        while j > 0 && key(&rows[j - 1]) > key(&rows[j]) {
            rows.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// How a fact-applied owner row changed its owner record.
pub(super) struct Fact {
    pub(super) kind: FactKind,
    pub(super) api: Api,
    pub(super) outcome: u8,
    pub(super) cleanup: Cleanup,
    pub(super) next_work: NextWork,
    pub(super) consumed: u8,
    pub(super) context: Option<Identity>,
    pub(super) transaction: Option<u16>,
    pub(super) receipt: bool,
    pub(super) fresh_engine: bool,
    pub(super) anchor: bool,
    pub(super) reason: Option<ObligationReason>,
}

pub(super) struct Editor<'a> {
    state: &'a mut State,
}

impl<'a> Editor<'a> {
    pub(super) fn new(state: &'a mut State) -> Self {
        Self { state }
    }
    pub(super) fn state(&self) -> &State {
        self.state
    }
    pub(super) fn state_mut(&mut self) -> &mut State {
        self.state
    }

    pub(super) fn next(&self, kind: IdentityKind) -> u16 {
        self.state.next_nonce[kind as usize - 1]
    }
    fn bump(&mut self, kind: IdentityKind) {
        self.state.next_nonce[kind as usize - 1] += 1;
    }

    pub(super) fn actor_mut(&mut self, actor: u8) -> Step<&mut ActorRecord> {
        let i = position(self.state.actors.as_slice(), |a| a.actor == actor)?;
        Ok(&mut self.state.actors.as_mut_slice()[i])
    }
    pub(super) fn engine(&self, nonce: u16) -> Step<EngineRecord> {
        let i = position(self.state.engines.as_slice(), |e| e.id.nonce == nonce)?;
        Ok(self.state.engines.as_slice()[i])
    }
    pub(super) fn engine_mut(&mut self, nonce: u16) -> Step<&mut EngineRecord> {
        let i = position(self.state.engines.as_slice(), |e| e.id.nonce == nonce)?;
        Ok(&mut self.state.engines.as_mut_slice()[i])
    }
    pub(super) fn transaction(&self, nonce: u16) -> Step<TransactionRecord> {
        let i = position(self.state.transactions.as_slice(), |t| t.id.nonce == nonce)?;
        Ok(self.state.transactions.as_slice()[i])
    }
    pub(super) fn transaction_mut(&mut self, nonce: u16) -> Step<&mut TransactionRecord> {
        let i = position(self.state.transactions.as_slice(), |t| t.id.nonce == nonce)?;
        Ok(&mut self.state.transactions.as_mut_slice()[i])
    }
    pub(super) fn receipt(&self, nonce: u16) -> Step<ReceiptRecord> {
        let i = position(self.state.receipts.as_slice(), |r| r.id.nonce == nonce)?;
        Ok(self.state.receipts.as_slice()[i])
    }
    pub(super) fn receipt_mut(&mut self, nonce: u16) -> Step<&mut ReceiptRecord> {
        let i = position(self.state.receipts.as_slice(), |r| r.id.nonce == nonce)?;
        Ok(&mut self.state.receipts.as_mut_slice()[i])
    }
    pub(super) fn statement(&self, nonce: u16) -> Step<StatementRecord> {
        let i = position(self.state.issued_statements.as_slice(), |s| {
            s.id.nonce == nonce
        })?;
        Ok(self.state.issued_statements.as_slice()[i])
    }
    pub(super) fn statement_mut(&mut self, nonce: u16) -> Step<&mut StatementRecord> {
        let i = position(self.state.issued_statements.as_slice(), |s| {
            s.id.nonce == nonce
        })?;
        Ok(&mut self.state.issued_statements.as_mut_slice()[i])
    }
    pub(super) fn result_mut(&mut self, nonce: u16) -> Step<&mut ResultRecord> {
        let i = position(self.state.issued_results.as_slice(), |r| {
            r.id.nonce == nonce
        })?;
        Ok(&mut self.state.issued_results.as_mut_slice()[i])
    }
    pub(super) fn descriptor(&self, result: u16) -> Step<Descriptor> {
        let i = position(self.state.private_descriptors.as_slice(), |d| {
            d.result.0 == result
        })?;
        Ok(self.state.private_descriptors.as_slice()[i])
    }
    pub(super) fn descriptor_mut(&mut self, result: u16) -> Step<&mut Descriptor> {
        let i = position(self.state.private_descriptors.as_slice(), |d| {
            d.result.0 == result
        })?;
        Ok(&mut self.state.private_descriptors.as_mut_slice()[i])
    }
    pub(super) fn remove_descriptor(&mut self, result: u16) -> Step<()> {
        let i = position(self.state.private_descriptors.as_slice(), |d| {
            d.result.0 == result
        })?;
        self.state.private_descriptors.remove(i);
        Ok(())
    }
    pub(super) fn slot(&self, actor: u8, slot: u8) -> Step<PreparedSlot> {
        let i = position(self.state.prepared_slots.as_slice(), |p| {
            p.actor == actor && p.slot == slot
        })?;
        Ok(self.state.prepared_slots.as_slice()[i])
    }
    pub(super) fn slot_mut(&mut self, actor: u8, slot: u8) -> Step<&mut PreparedSlot> {
        let i = position(self.state.prepared_slots.as_slice(), |p| {
            p.actor == actor && p.slot == slot
        })?;
        Ok(&mut self.state.prepared_slots.as_mut_slice()[i])
    }
    pub(super) fn binding_mut(&mut self, nonce: u16) -> Step<&mut BindingRecord> {
        let i = position(self.state.bindings.as_slice(), |b| b.id.nonce == nonce)?;
        Ok(&mut self.state.bindings.as_mut_slice()[i])
    }
    pub(super) fn obligation(&self, nonce: u16) -> Step<ObligationRecord> {
        let i = position(self.state.obligations.as_slice(), |o| o.id.nonce == nonce)?;
        Ok(self.state.obligations.as_slice()[i])
    }
    pub(super) fn obligation_mut(&mut self, nonce: u16) -> Step<&mut ObligationRecord> {
        let i = position(self.state.obligations.as_slice(), |o| o.id.nonce == nonce)?;
        Ok(&mut self.state.obligations.as_mut_slice()[i])
    }
    pub(super) fn context(&self, nonce: u16) -> Step<ContextRecord> {
        let i = position(self.state.contexts.as_slice(), |c| c.id.nonce == nonce)?;
        Ok(self.state.contexts.as_slice()[i])
    }
    pub(super) fn context_mut(&mut self, nonce: u16) -> Step<&mut ContextRecord> {
        let i = position(self.state.contexts.as_slice(), |c| c.id.nonce == nonce)?;
        Ok(&mut self.state.contexts.as_mut_slice()[i])
    }
    pub(super) fn owner_mut(&mut self, nonce: u16) -> Step<&mut OwnerRecord> {
        let i = position(self.state.owners.as_slice(), |o| o.id.nonce == nonce)?;
        Ok(&mut self.state.owners.as_mut_slice()[i])
    }

    /// Register a fresh context; None data means Completed.
    pub(super) fn add_context(&mut self, id: Identity, data: Option<ContextData>) -> Step<()> {
        push(
            &mut self.state.contexts,
            ContextRecord {
                id,
                issue_ordinal: id.nonce,
                data: data.unwrap_or(ContextData::Completed),
            },
        )?;
        self.bump(IdentityKind::Context);
        Ok(())
    }

    /// Issue an identity of a session-scoped kind for the given scope.
    pub(super) fn issue(
        &mut self,
        kind: IdentityKind,
        scope: Identity,
        slot: u8,
        generation: u16,
    ) -> Identity {
        let id = Identity {
            kind,
            actor: scope.actor,
            slot,
            engine: scope.engine,
            session: scope.session,
            nonce: self.next(kind),
            generation,
        };
        self.bump(kind);
        id
    }

    pub(super) fn add_statement(&mut self, row: StatementRecord) -> Step<()> {
        push(&mut self.state.issued_statements, row)
    }
    pub(super) fn add_result(&mut self, row: ResultRecord) -> Step<()> {
        push(&mut self.state.issued_results, row)
    }
    pub(super) fn add_descriptor(&mut self, row: Descriptor) -> Step<()> {
        push(&mut self.state.private_descriptors, row)
    }
    pub(super) fn add_transaction(&mut self, row: TransactionRecord) -> Step<()> {
        push(&mut self.state.transactions, row)
    }
    pub(super) fn add_binding(&mut self, row: BindingRecord) -> Step<()> {
        push(&mut self.state.bindings, row)
    }
    pub(super) fn add_engine(&mut self, row: EngineRecord) -> Step<()> {
        push(&mut self.state.engines, row)?;
        self.bump(IdentityKind::Engine);
        Ok(())
    }
    pub(super) fn add_session(&mut self, row: crate::lifecycle::schema::SessionRecord) -> Step<()> {
        push(&mut self.state.session_incarnations, row)
    }

    /// A model-local retained owner birth without an external fact (Prepare/Bind/Begin).
    pub(super) fn birth(
        &mut self,
        kind: ResourceKind,
        holder: Identity,
        context: Identity,
    ) -> Step<Identity> {
        let id = self.issue(IdentityKind::Resource, holder, 255, 0);
        let holder = holder_ref(holder);
        push(
            &mut self.state.owners,
            OwnerRecord {
                id,
                resource_kind: kind,
                disposition: OwnerDisposition::Held,
                transfer_count: 0,
                original_holder: holder,
                current_holder: Some(holder),
                previous_holder: None,
                birth_context: Some(ContextRef(context.nonce)),
                last_fact: None,
            },
        )?;
        Ok(id)
    }

    /// Model-local release without an external fact.
    pub(super) fn release_local(&mut self, owner: u16) -> Step<()> {
        let row = self.owner_mut(owner)?;
        if row.disposition != OwnerDisposition::Released {
            row.previous_holder = row.current_holder;
            row.current_holder = None;
            row.disposition = OwnerDisposition::Released;
        }
        Ok(())
    }

    /// Record the consumed fact, its memberships, owner changes and obligation records.
    ///
    /// Owner and obligation rows are taken from the supplied envelope; nothing is derived from the
    /// successor. `birth_holder` resolves the original holder of born-and-released resources.
    pub(super) fn record_fact(
        &mut self,
        env: &Environment,
        fact: &Fact,
        decision: Decision,
        delivery: Delivery,
        birth_holder: &dyn Fn(ResourceKind) -> Option<Identity>,
    ) -> Step<Identity> {
        let id = unpack(env.fact).ok_or(ENVELOPE)?;
        if id.nonce != self.next(IdentityKind::Fact) {
            return Err(ENVELOPE);
        }
        self.bump(IdentityKind::Fact);
        let fact_ref = FactRef(id.nonce);
        let context = fact.context.map(|c| ContextRef(c.nonce));
        // Owners first: new obligations and owner rows reference each other by identity.
        for row in env.obligations.as_slice() {
            self.apply_obligation(row, env, fact, fact_ref)?;
        }
        // Births form one contiguous nonce block from the stored next; rows are in key order.
        let first = self.next(IdentityKind::Resource);
        let mut births = 0u16;
        let mut seen = [false; 64];
        for row in env.owners.as_slice() {
            let id = unpack(row.resource).ok_or(ENVELOPE)?;
            if !self
                .state
                .owners
                .as_slice()
                .iter()
                .any(|o| o.id.nonce == id.nonce)
            {
                let offset = id.nonce.checked_sub(first).ok_or(ENVELOPE)? as usize;
                if offset >= seen.len() || seen[offset] {
                    return Err(ENVELOPE);
                }
                seen[offset] = true;
                births += 1;
            }
        }
        if seen[..usize::from(births)].iter().any(|s| !s) {
            return Err(ENVELOPE);
        }
        for row in env.owners.as_slice() {
            self.apply_owner(row, fact_ref, context, birth_holder)?;
        }
        self.state.next_nonce[IdentityKind::Resource as usize - 1] = first + births;
        for row in env.obligations.as_slice() {
            let o = unpack(row.obligation).ok_or(ENVELOPE)?;
            let summary = self.aggregate(o.nonce)?;
            let record = self.obligation_mut(o.nonce)?;
            record.disposition = summary;
            push(
                &mut self.state.fact_obligations,
                FactObligation {
                    fact: fact_ref,
                    obligation: ObligationRef(o.nonce),
                    known_decision: row.known_decision,
                    disposition: row.after_ownership_summary,
                    owner_count: row.associated_owner_count,
                },
            )?;
        }
        let receipt = if fact.receipt {
            env.receipt.and_then(unpack).map(|r| ReceiptRef(r.nonce))
        } else {
            None
        };
        push(
            &mut self.state.facts,
            FactRecord {
                id,
                kind: fact.kind,
                api: fact.api,
                outcome_class: fact.outcome,
                cleanup: fact.cleanup,
                decision,
                delivery,
                next_work: fact.next_work,
                consumed_parts: fact.consumed,
                engine: EngineRef(id.engine),
                obligation_count: env.obligations.len() as u16,
                owner_count: env.owners.len() as u16,
                context,
                transaction: fact.transaction.map(TransactionRef),
                receipt,
                fresh_engine: if fact.fresh_engine {
                    env.fresh_engine
                        .and_then(unpack)
                        .map(|e| EngineRef(e.nonce))
                } else {
                    None
                },
                anchor: if fact.anchor {
                    env.anchor.and_then(unpack).map(|a| AnchorRef(a.nonce))
                } else {
                    None
                },
            },
        )?;
        Ok(id)
    }

    fn apply_owner(
        &mut self,
        row: &InputOwner,
        fact: FactRef,
        context: Option<ContextRef>,
        birth_holder: &dyn Fn(ResourceKind) -> Option<Identity>,
    ) -> Step<()> {
        let id = unpack(row.resource).ok_or(ENVELOPE)?;
        let obligation = match row.obligation {
            Some(raw) => Some(ObligationRef(unpack(raw).ok_or(ENVELOPE)?.nonce)),
            None => None,
        };
        let before = row.before_holder.map(raw_holder).transpose()?;
        let after = row.after_holder.map(raw_holder).transpose()?;
        let exists = self
            .state
            .owners
            .as_slice()
            .iter()
            .any(|o| o.id.nonce == id.nonce);
        if exists {
            let record = self.owner_mut(id.nonce)?;
            let changed =
                record.disposition != row.after_disposition || record.current_holder != after;
            if record.current_holder != before {
                return Err(ENVELOPE);
            }
            if changed {
                if row.after_disposition == OwnerDisposition::Transferred {
                    record.transfer_count += 1;
                }
                record.previous_holder = record.current_holder;
                record.current_holder = after;
                record.disposition = row.after_disposition;
                record.last_fact = Some(fact);
            }
        } else {
            if before.is_some() {
                return Err(ENVELOPE);
            }
            let original = match after {
                Some(h) => h,
                None => holder_ref(birth_holder(row.resource_kind).ok_or(ENVELOPE)?),
            };
            push(
                &mut self.state.owners,
                OwnerRecord {
                    id,
                    resource_kind: row.resource_kind,
                    disposition: row.after_disposition,
                    transfer_count: 0,
                    original_holder: original,
                    current_holder: after,
                    previous_holder: if after.is_none() {
                        Some(original)
                    } else {
                        None
                    },
                    birth_context: context,
                    last_fact: Some(fact),
                },
            )?;
        }
        push(
            &mut self.state.fact_owners,
            FactOwner {
                fact,
                resource: ResourceRef(id.nonce),
                disposition: row.after_disposition,
                obligation,
                before_holder: before,
                after_holder: after,
            },
        )
    }

    fn apply_obligation(
        &mut self,
        row: &InputObligation,
        env: &Environment,
        fact: &Fact,
        fact_ref: FactRef,
    ) -> Step<()> {
        let id = unpack(row.obligation).ok_or(ENVELOPE)?;
        let exists = self
            .state
            .obligations
            .as_slice()
            .iter()
            .any(|o| o.id.nonce == id.nonce);
        if exists {
            let reason = fact.reason;
            let record = self.obligation_mut(id.nonce)?;
            record.known_decision = row.known_decision;
            record.latest_fact = Some(fact_ref);
            if let Some(reason) = reason
                && record.settlement == Settlement::Outstanding
            {
                record.latest_reason = reason;
                record.reason_mask |= 1 << (reason as u8);
            }
            return Ok(());
        }
        if id.nonce != self.next(IdentityKind::Obligation) {
            return Err(ENVELOPE);
        }
        self.bump(IdentityKind::Obligation);
        // The obligation's transaction is the transaction holder of its linked rows.
        let transaction = env
            .owners
            .as_slice()
            .iter()
            .filter(|o| o.obligation == Some(row.obligation))
            .flat_map(|o| [o.before_holder, o.after_holder])
            .flatten()
            .filter_map(unpack)
            .find(|h| h.kind == IdentityKind::Transaction)
            .ok_or(ENVELOPE)?;
        let t = self.transaction(transaction.nonce)?;
        let reason = fact
            .reason
            .unwrap_or(ObligationReason::PeerActiveTransactionAtFence);
        let peer = fact.transaction != Some(transaction.nonce);
        let reason = if peer {
            ObligationReason::PeerActiveTransactionAtFence
        } else {
            reason
        };
        push(
            &mut self.state.obligations,
            ObligationRecord {
                id,
                engine: EngineRef(id.engine),
                transaction: TransactionRef(transaction.nonce),
                origin_session: t.birth_session,
                creation_reason: reason,
                latest_reason: reason,
                reason_mask: 1 << (reason as u8),
                disposition: row.after_ownership_summary,
                known_decision: row.known_decision,
                settlement: Settlement::Outstanding,
                first_fact: Some(fact_ref),
                latest_fact: Some(fact_ref),
                recovery_fact: None,
            },
        )?;
        let record = self.transaction_mut(transaction.nonce)?;
        record.obligation = Some(ObligationRef(id.nonce));
        // A fenced RW transaction keeps its progress and decision but can only be recovered.
        if record.life == crate::lifecycle::schema::TransactionLife::Active {
            record.life = crate::lifecycle::schema::TransactionLife::RecoveryPending;
        }
        Ok(())
    }

    /// Aggregate disposition over every owner currently linked to the obligation's transaction.
    fn aggregate(&self, obligation: u16) -> Step<OwnershipSummary> {
        let o = self.obligation(obligation)?;
        let mut summary = OwnershipSummary::None;
        for owner in self.state.owners.as_slice() {
            let linked = owner.original_holder
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
                );
            if !linked {
                continue;
            }
            let this = match owner.disposition {
                OwnerDisposition::Held => OwnershipSummary::AllHeld,
                OwnerDisposition::Transferred => OwnershipSummary::AllTransferred,
                OwnerDisposition::Released => OwnershipSummary::AllReleased,
            };
            summary = match summary {
                OwnershipSummary::None => this,
                same if same == this => same,
                _ => OwnershipSummary::Mixed,
            };
        }
        Ok(summary)
    }

    /// Insert a receipt record supplied by the consumed envelope.
    pub(super) fn add_receipt(&mut self, row: ReceiptRecord) -> Step<()> {
        if row.id.nonce != self.next(IdentityKind::Receipt) {
            return Err(ENVELOPE);
        }
        self.bump(IdentityKind::Receipt);
        if let Some(anchor) = row.anchor {
            if anchor.nonce != self.next(IdentityKind::Anchor) {
                return Err(ENVELOPE);
            }
            self.bump(IdentityKind::Anchor);
        }
        push(&mut self.state.receipts, row)
    }

    /// Restore canonical table order after every edit.
    pub(super) fn canonicalize(&mut self) -> Step<()> {
        let s = &mut *self.state;
        sort_by(s.issued_statements.as_mut_slice(), |r| r.id.key());
        sort_by(s.issued_results.as_mut_slice(), |r| r.id.key());
        sort_by(s.contexts.as_mut_slice(), |r| r.id.key());
        sort_by(s.transactions.as_mut_slice(), |r| r.id.key());
        sort_by(s.bindings.as_mut_slice(), |r| r.id.key());
        sort_by(s.receipts.as_mut_slice(), |r| r.id.key());
        sort_by(s.owners.as_mut_slice(), |r| r.id.key());
        sort_by(s.facts.as_mut_slice(), |r| r.id.key());
        sort_by(s.engines.as_mut_slice(), |r| r.id.key());
        sort_by(s.session_incarnations.as_mut_slice(), |r| r.id.key());
        let results = s.issued_results;
        let key_of_result = |n: u16| {
            results
                .as_slice()
                .iter()
                .find(|r| r.id.nonce == n)
                .map(|r| r.id.key())
        };
        sort_by(s.private_descriptors.as_mut_slice(), |d| {
            key_of_result(d.result.0)
        });
        let engines = s.engines;
        let transactions = s.transactions;
        let sessions = s.session_incarnations;
        let ident = |kind: IdentityKind, n: u16| match kind {
            IdentityKind::Engine => engines
                .as_slice()
                .iter()
                .find(|e| e.id.nonce == n)
                .map(|e| e.id.key()),
            IdentityKind::Transaction => transactions
                .as_slice()
                .iter()
                .find(|t| t.id.nonce == n)
                .map(|t| t.id.key()),
            _ => sessions
                .as_slice()
                .iter()
                .find(|x| x.id.nonce == n)
                .map(|x| x.id.key()),
        };
        let obligation_key = |o: &ObligationRecord| {
            (
                ident(IdentityKind::Engine, o.engine.0),
                ident(IdentityKind::Transaction, o.transaction.0),
                ident(IdentityKind::Session, o.origin_session.0),
            )
        };
        sort_by(s.obligations.as_mut_slice(), obligation_key);
        let obligations = s.obligations;
        let facts = s.facts;
        let owners = s.owners;
        let fact_key = |n: u16| {
            facts
                .as_slice()
                .iter()
                .find(|f| f.id.nonce == n)
                .map(|f| f.id.key())
        };
        let o_key = |n: u16| {
            obligations
                .as_slice()
                .iter()
                .find(|o| o.id.nonce == n)
                .map(obligation_key)
        };
        sort_by(s.fact_obligations.as_mut_slice(), |m| {
            (fact_key(m.fact.0), o_key(m.obligation.0))
        });
        let r_key = |n: u16| {
            owners
                .as_slice()
                .iter()
                .find(|o| o.id.nonce == n)
                .map(|o| o.id.key())
        };
        sort_by(s.fact_owners.as_mut_slice(), |m| {
            (fact_key(m.fact.0), r_key(m.resource.0))
        });
        Ok(())
    }
}

/// Typed holder for a full identity of a holder kind.
pub(super) fn holder_ref(id: Identity) -> HolderRef {
    HolderRef {
        kind: id.kind,
        nonce: id.nonce,
    }
}

fn raw_holder(raw: RawIdentity) -> Step<HolderRef> {
    let id = unpack(raw).ok_or(ENVELOPE)?;
    Ok(holder_ref(id))
}
