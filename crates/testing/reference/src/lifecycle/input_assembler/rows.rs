//! Bounded owner/obligation planning with symbolic births and one exact row per resource.
//!
//! Rows follow only pre-state references. Nonces of prospective definitions are assigned
//! after every used namespace has been checked, in RW3 -> cursor3 -> Recovery phase order.
use super::{INVALID, Step, primitive, unique};
use crate::lifecycle::schema::{
    Access, CommitProgress, Decision, HolderRef, Identity, IdentityKind, ObligationRecord,
    OwnerDisposition, ResourceKind, ResultState, Settlement, State, TransactionLife,
    TransactionRecord,
};

/// A row identity that is either retained in the pre-state or chosen at materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sym {
    /// Original identity copied from the pre-state.
    Known(Identity),
    /// The one prospective transaction of an authenticated Execute.
    NewTransaction,
    /// The one prospective cursor holder of an authenticated Execute Success Select.
    NewResult,
    /// The k-th new obligation, numbered in transaction order.
    NewObligation(u8),
    /// The k-th resource birth, numbered in push (phase) order.
    NewResource(u8),
}

/// One supplied ownership row before identities are materialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnerRow {
    pub(super) resource: Sym,
    pub(super) kind: ResourceKind,
    pub(super) after: OwnerDisposition,
    pub(super) obligation: Option<Sym>,
    pub(super) before: Option<Identity>,
    pub(super) after_holder: Option<Sym>,
}

/// One supplied obligation row; count and summary come from linked envelope rows only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ObligationRow {
    pub(super) id: Sym,
    pub(super) decision: Decision,
}

/// The decision source of an obligation row, fixed by its role before any value is chosen.
#[derive(Clone, Copy, Debug)]
pub(super) enum Role {
    /// Handoff, recovery, or a direct release without a new decision fact.
    Retained,
    /// A newly supplied publication confirmation.
    Committed,
    /// The exact cause of a fence: raw class mapping.
    Cause(Decision),
}

/// The cause transaction of a fence census.
#[derive(Clone, Copy)]
pub(super) enum Cause<'a> {
    /// No exact cause transaction; every census member is a peer.
    None,
    /// An existing transaction selected by the family.
    Existing(&'a TransactionRecord),
    /// The prospective transaction of an authenticated Execute.
    Prospective { read_write: bool },
}

const OWNER_PLAN: usize = 62;
const OBLIGATION_PLAN: usize = 14;
const BIRTH_PLAN: usize = 16;
const RW: [ResourceKind; 3] = [
    ResourceKind::Undo,
    ResourceKind::Locks,
    ResourceKind::AbortReserve,
];
const CURSOR: [ResourceKind; 3] = [
    ResourceKind::Snapshot,
    ResourceKind::Scratch,
    ResourceKind::Temp,
];
const RECOVERY: [ResourceKind; 1] = [ResourceKind::Recovery];
const LOCAL: [ResourceKind; 2] = [ResourceKind::Plan, ResourceKind::Binding];

/// Live owners of one holder, each with its expanded current holder.
struct Selection {
    rows: [(usize, Identity); 4],
    len: usize,
}
impl Selection {
    fn iter(&self) -> impl Iterator<Item = (usize, Identity)> + '_ {
        self.rows[..self.len].iter().copied()
    }
    fn has(&self, state: &State, kind: ResourceKind) -> bool {
        self.iter()
            .any(|(i, _)| state.owners.as_slice()[i].resource_kind == kind)
    }
}

/// Bounded plan storage. Overflow is recorded and reported as Capacity only at the end.
pub(super) struct Rows {
    owners: [OwnerRow; OWNER_PLAN],
    owner_len: usize,
    obligations: [ObligationRow; OBLIGATION_PLAN],
    obligation_len: usize,
    births: [Sym; BIRTH_PLAN],
    birth_len: usize,
    new_obligations: [Sym; OBLIGATION_PLAN],
    new_obligation_len: usize,
    overflow: bool,
}

impl Rows {
    pub(super) fn new() -> Self {
        let empty = OwnerRow {
            resource: Sym::NewResource(0),
            kind: ResourceKind::Plan,
            after: OwnerDisposition::Held,
            obligation: None,
            before: None,
            after_holder: None,
        };
        let obligation = ObligationRow {
            id: Sym::NewObligation(0),
            decision: Decision::NotApplicable,
        };
        Self {
            owners: [empty; OWNER_PLAN],
            owner_len: 0,
            obligations: [obligation; OBLIGATION_PLAN],
            obligation_len: 0,
            births: [Sym::NewTransaction; BIRTH_PLAN],
            birth_len: 0,
            new_obligations: [Sym::NewTransaction; OBLIGATION_PLAN],
            new_obligation_len: 0,
            overflow: false,
        }
    }

    pub(super) fn owners(&self) -> &[OwnerRow] {
        &self.owners[..self.owner_len]
    }
    pub(super) fn obligations(&self) -> &[ObligationRow] {
        &self.obligations[..self.obligation_len]
    }
    /// Original holder of each birth, in nonce assignment order.
    pub(super) fn births(&self) -> &[Sym] {
        &self.births[..self.birth_len]
    }
    /// Transaction of each new obligation, in nonce assignment order.
    pub(super) fn new_obligations(&self) -> &[Sym] {
        &self.new_obligations[..self.new_obligation_len]
    }
    pub(super) fn overflowed(&self) -> bool {
        self.overflow
    }

    /// Union one exact row; a different requirement for the same resource is a state error.
    fn add(&mut self, row: OwnerRow) -> Step<()> {
        if let Some(old) = self.owners().iter().find(|o| o.resource == row.resource) {
            return if *old == row { Ok(()) } else { Err(INVALID) };
        }
        if self.owner_len == OWNER_PLAN {
            self.overflow = true;
            return Ok(());
        }
        self.owners[self.owner_len] = row;
        self.owner_len += 1;
        Ok(())
    }

    fn add_obligation(&mut self, id: Sym, decision: Decision) -> Step<()> {
        if let Some(old) = self.obligations().iter().find(|o| o.id == id) {
            return if old.decision == decision {
                Ok(())
            } else {
                Err(INVALID)
            };
        }
        if self.obligation_len == OBLIGATION_PLAN {
            self.overflow = true;
            return Ok(());
        }
        self.obligations[self.obligation_len] = ObligationRow { id, decision };
        self.obligation_len += 1;
        Ok(())
    }

    fn birth(&mut self, holder: Sym) -> Sym {
        if self.birth_len == BIRTH_PLAN {
            self.overflow = true;
            return Sym::NewResource(u8::MAX);
        }
        self.births[self.birth_len] = holder;
        self.birth_len += 1;
        Sym::NewResource((self.birth_len - 1) as u8)
    }

    fn new_obligation(&mut self, transaction: Sym) -> Sym {
        if self.new_obligation_len == OBLIGATION_PLAN {
            self.overflow = true;
            return Sym::NewObligation(u8::MAX);
        }
        self.new_obligations[self.new_obligation_len] = transaction;
        self.new_obligation_len += 1;
        Sym::NewObligation((self.new_obligation_len - 1) as u8)
    }

    /// Supply births for the given kinds; Released births are one final row without a holder.
    pub(super) fn births_of(
        &mut self,
        kinds: &[ResourceKind],
        holder: Sym,
        after: OwnerDisposition,
        obligation: Option<Sym>,
    ) -> Step<()> {
        for &kind in kinds {
            let resource = self.birth(holder);
            self.add(OwnerRow {
                resource,
                kind,
                after,
                obligation,
                before: None,
                after_holder: (after != OwnerDisposition::Released).then_some(holder),
            })?;
        }
        Ok(())
    }
    pub(super) fn rw_births(&mut self, after: OwnerDisposition) -> Step<()> {
        self.births_of(&RW, Sym::NewTransaction, after, None)
    }
    pub(super) fn cursor_births(&mut self) -> Step<()> {
        self.births_of(&CURSOR, Sym::NewResult, OwnerDisposition::Held, None)
    }

    fn release(
        &mut self,
        state: &State,
        selection: &Selection,
        obligation: Option<Sym>,
    ) -> Step<()> {
        for (index, before) in selection.iter() {
            let owner = &state.owners.as_slice()[index];
            self.add(OwnerRow {
                resource: Sym::Known(owner.id),
                kind: owner.resource_kind,
                after: OwnerDisposition::Released,
                obligation,
                before: Some(before),
                after_holder: None,
            })?;
        }
        Ok(())
    }

    fn keep(&mut self, state: &State, selection: &Selection, obligation: Sym) -> Step<()> {
        for (index, before) in selection.iter() {
            let owner = &state.owners.as_slice()[index];
            self.add(OwnerRow {
                resource: Sym::Known(owner.id),
                kind: owner.resource_kind,
                after: owner.disposition,
                obligation: Some(obligation),
                before: Some(before),
                after_holder: Some(Sym::Known(before)),
            })?;
        }
        Ok(())
    }

    fn handoff(&mut self, state: &State, selection: &Selection, obligation: Identity) -> Step<()> {
        for (index, before) in selection.iter() {
            let owner = &state.owners.as_slice()[index];
            self.add(OwnerRow {
                resource: Sym::Known(owner.id),
                kind: owner.resource_kind,
                after: OwnerDisposition::Transferred,
                obligation: Some(Sym::Known(obligation)),
                before: Some(before),
                after_holder: Some(Sym::Known(obligation)),
            })?;
        }
        Ok(())
    }

    /// C(R): the result's live Snapshot/Scratch/Temp, released.
    pub(super) fn release_cursor(&mut self, state: &State, result: u16) -> Step<()> {
        let selection = select(state, holder(IdentityKind::Result, result), &CURSOR, None)?;
        self.release(state, &selection, None)
    }

    /// L(P): live Plan/Binding of the statement and C of its slot's descriptor result.
    pub(super) fn release_local(
        &mut self,
        state: &State,
        statement: Option<u16>,
        descriptor_result: Option<u16>,
    ) -> Step<()> {
        if let Some(statement) = statement {
            let selection = select(
                state,
                holder(IdentityKind::Statement, statement),
                &LOCAL,
                None,
            )?;
            self.release(state, &selection, None)?;
        }
        if let Some(result) = descriptor_result {
            self.release_cursor(state, result)?;
        }
        Ok(())
    }

    /// RW(T), optionally with R(T), released; rows link the transaction's existing obligation.
    pub(super) fn release_transaction(
        &mut self,
        state: &State,
        transaction: &TransactionRecord,
        with_recovery: bool,
        role: Role,
    ) -> Step<()> {
        let obligation = transaction_obligation(state, transaction)?;
        let link = obligation.map(|o| o.id);
        let mut selection = select(state, transaction_holder(transaction), &RW, link)?;
        if with_recovery {
            let recovery = select(state, transaction_holder(transaction), &RECOVERY, link)?;
            selection.extend(&recovery)?;
        }
        self.release(state, &selection, link.map(Sym::Known))?;
        if let Some(o) = obligation
            && selection.len > 0
        {
            self.add_obligation(Sym::Known(o.id), decision(role, o))?;
        }
        Ok(())
    }

    /// Currently Open cursors whose private descriptor names the transaction.
    pub(super) fn release_open_cursors(&mut self, state: &State, transaction: u16) -> Step<()> {
        for descriptor in state.private_descriptors.as_slice() {
            if descriptor.transaction.is_some_and(|t| t.0 == transaction)
                && descriptor.state == ResultState::Open
            {
                self.release_cursor(state, descriptor.result.0)?;
            }
        }
        Ok(())
    }

    /// Hand the transaction's live RW and Recovery owners to its exact existing obligation.
    pub(super) fn handoff_transaction(
        &mut self,
        state: &State,
        transaction: &TransactionRecord,
    ) -> Step<()> {
        // Handoff requires the existing obligation; it is never invented here.
        let obligation = transaction_obligation(state, transaction)?.ok_or(INVALID)?;
        let mut selection = select(
            state,
            transaction_holder(transaction),
            &RW,
            Some(obligation.id),
        )?;
        let recovery = select(
            state,
            transaction_holder(transaction),
            &RECOVERY,
            Some(obligation.id),
        )?;
        selection.extend(&recovery)?;
        self.handoff(state, &selection, obligation.id)?;
        self.add_obligation(
            Sym::Known(obligation.id),
            decision(Role::Retained, obligation),
        )
    }

    /// Fenced CloseSession: every outstanding obligation born in the caller session.
    pub(super) fn handoff_session(&mut self, state: &State, engine: u16, session: u16) -> Step<()> {
        for obligation in state.obligations.as_slice() {
            if obligation.engine.0 == engine
                && obligation.origin_session.0 == session
                && obligation.settlement == Settlement::Outstanding
            {
                let transaction = obligation_transaction(state, obligation)?;
                self.handoff_transaction(state, transaction)?;
            }
        }
        Ok(())
    }

    /// Recovery: release RW and Recovery of every outstanding obligation of the engine.
    pub(super) fn recover(&mut self, state: &State, engine: u16) -> Step<()> {
        for obligation in state.obligations.as_slice() {
            if obligation.engine.0 == engine && obligation.settlement == Settlement::Outstanding {
                let transaction = obligation_transaction(state, obligation)?;
                let mut selection = select(
                    state,
                    transaction_holder(transaction),
                    &RW,
                    Some(obligation.id),
                )?;
                let recovery = select(
                    state,
                    transaction_holder(transaction),
                    &RECOVERY,
                    Some(obligation.id),
                )?;
                selection.extend(&recovery)?;
                self.release(state, &selection, Some(Sym::Known(obligation.id)))?;
                self.add_obligation(
                    Sym::Known(obligation.id),
                    decision(Role::Retained, obligation),
                )?;
            }
        }
        Ok(())
    }

    /// First-fence census of unfinished RW transactions plus a prospective RW transaction.
    ///
    /// Members follow the transaction's full identity key, so the prospective transaction is
    /// placed by its caller session and actor. Existing obligations are reused; peer obligations
    /// without any change are omitted, and a cause reason update keeps a count-zero row. Peer
    /// Plan/Binding/cursors are never read. `caller` is the prospective transaction's
    /// (actor, session) scope.
    pub(super) fn census(
        &mut self,
        state: &State,
        engine: u16,
        caller: (u8, u16),
        cause: Cause<'_>,
        raw: Decision,
    ) -> Step<()> {
        let mut members: [(Option<&TransactionRecord>, Order); 7] = [(None, ORDER_ZERO); 7];
        let mut count = 0;
        for transaction in state.transactions.as_slice() {
            if transaction.id.engine == engine
                && transaction.access == Access::ReadWrite
                && transaction.life != TransactionLife::Ended
                && matches!(
                    transaction.progress,
                    CommitProgress::None
                        | CommitProgress::BeforeAttempt
                        | CommitProgress::Attempted
                        | CommitProgress::DurableConfirmed
                )
            {
                members[count] = (
                    Some(transaction),
                    order(transaction.id, u32::from(transaction.id.nonce)),
                );
                count += 1;
            }
        }
        if matches!(cause, Cause::Prospective { read_write: true }) {
            let id = Identity {
                kind: IdentityKind::Transaction,
                actor: caller.0,
                slot: 255,
                engine,
                session: caller.1,
                nonce: 0,
                generation: 0,
            };
            // Its nonce is the stored next, above every retained nonce of a valid namespace.
            members[count] = (None, order(id, u32::MAX));
            count += 1;
        }
        let members = &mut members[..count];
        members.sort_unstable_by_key(|m| m.1);
        // Obligations are numbered in member order before any birth is placed.
        let mut links: [Option<(Sym, Option<&ObligationRecord>)>; 7] = [None; 7];
        for (slot, (member, _)) in members.iter().enumerate() {
            links[slot] = Some(match member {
                Some(transaction) => match transaction_obligation(state, transaction)? {
                    Some(o) => (Sym::Known(o.id), Some(o)),
                    None => (self.new_obligation(Sym::Known(transaction.id)), None),
                },
                None => (self.new_obligation(Sym::NewTransaction), None),
            });
        }
        // Resource phase order: the prospective RW3 precede every Recovery birth.
        for (slot, (member, _)) in members.iter().enumerate() {
            if member.is_none() {
                let (link, _) = links[slot].ok_or(INVALID)?;
                self.births_of(&RW, Sym::NewTransaction, OwnerDisposition::Held, Some(link))?;
            }
        }
        for (slot, (member, _)) in members.iter().enumerate() {
            let (link, existing) = links[slot].ok_or(INVALID)?;
            let Some(transaction) = member else {
                self.births_of(
                    &RECOVERY,
                    Sym::NewTransaction,
                    OwnerDisposition::Held,
                    Some(link),
                )?;
                self.add_obligation(link, raw)?;
                continue;
            };
            let is_cause =
                matches!(cause, Cause::Existing(t) if t.id.nonce == transaction.id.nonce);
            let existing_id = existing.map(|o| o.id);
            let rw = select(state, transaction_holder(transaction), &RW, existing_id)?;
            let recovery = select(
                state,
                transaction_holder(transaction),
                &RECOVERY,
                existing_id,
            )?;
            let needs_recovery = !recovery.has(state, ResourceKind::Recovery);
            match existing {
                Some(o) => {
                    // A repeated fence never issues a second Recovery for an existing obligation.
                    if needs_recovery {
                        return Err(INVALID);
                    }
                    if is_cause {
                        self.add_obligation(link, decision(Role::Cause(raw), o))?;
                    }
                }
                None => {
                    self.keep(state, &rw, link)?;
                    self.keep(state, &recovery, link)?;
                    if needs_recovery {
                        self.births_of(
                            &RECOVERY,
                            Sym::Known(transaction.id),
                            OwnerDisposition::Held,
                            Some(link),
                        )?;
                    }
                    let value = if is_cause { raw } else { transaction.decision };
                    self.add_obligation(link, value)?;
                }
            }
        }
        Ok(())
    }
}

/// Full identity key with a widened nonce, so a prospective nonce sorts after retained ones.
type Order = (u8, u16, u16, u8, u8, u32, u16);
const ORDER_ZERO: Order = (0, 0, 0, 0, 0, 0, 0);
fn order(id: Identity, nonce: u32) -> Order {
    (
        id.kind as u8,
        id.engine,
        id.session,
        id.actor,
        id.slot,
        nonce,
        id.generation,
    )
}

impl Selection {
    fn extend(&mut self, other: &Selection) -> Step<()> {
        for row in other.iter() {
            if self.len == self.rows.len() {
                return Err(INVALID);
            }
            self.rows[self.len] = row;
            self.len += 1;
        }
        Ok(())
    }
}

fn decision(role: Role, obligation: &ObligationRecord) -> Decision {
    match role {
        Role::Retained => obligation.known_decision,
        Role::Committed => Decision::Committed,
        Role::Cause(raw) => raw,
    }
}

fn holder(kind: IdentityKind, nonce: u16) -> HolderRef {
    HolderRef { kind, nonce }
}
fn transaction_holder(transaction: &TransactionRecord) -> HolderRef {
    holder(IdentityKind::Transaction, transaction.id.nonce)
}

/// Select live owners of the holder for the given kinds. Each kind has at most one live owner;
/// a transferred owner must name the transaction's exact obligation.
fn select(
    state: &State,
    holder: HolderRef,
    kinds: &[ResourceKind],
    obligation: Option<Identity>,
) -> Step<Selection> {
    let mut out = Selection {
        rows: [(0, Identity::default()); 4],
        len: 0,
    };
    for (index, owner) in state.owners.as_slice().iter().enumerate() {
        if owner.original_holder != holder
            || !kinds.contains(&owner.resource_kind)
            || owner.disposition == OwnerDisposition::Released
        {
            continue;
        }
        if !primitive(owner.id, IdentityKind::Resource)
            || out
                .iter()
                .any(|(i, _)| state.owners.as_slice()[i].resource_kind == owner.resource_kind)
            || out.len == out.rows.len()
        {
            return Err(INVALID);
        }
        let current = owner.current_holder.ok_or(INVALID)?;
        match owner.disposition {
            OwnerDisposition::Held if current != holder => return Err(INVALID),
            OwnerDisposition::Transferred
                if current.kind != IdentityKind::Obligation
                    || obligation.is_none_or(|o| o.nonce != current.nonce) =>
            {
                return Err(INVALID);
            }
            _ => {}
        }
        out.rows[out.len] = (index, holder_identity(state, current)?);
        out.len += 1;
    }
    Ok(out)
}

/// Expand a typed holder into its declared full identity.
fn holder_identity(state: &State, holder: HolderRef) -> Step<Identity> {
    let n = holder.nonce;
    let id = match holder.kind {
        IdentityKind::Engine => unique(state.engines.as_slice(), |r| r.id.nonce == n)?.id,
        IdentityKind::Statement => {
            unique(state.issued_statements.as_slice(), |r| r.id.nonce == n)?.id
        }
        IdentityKind::Result => unique(state.issued_results.as_slice(), |r| r.id.nonce == n)?.id,
        IdentityKind::Transaction => unique(state.transactions.as_slice(), |r| r.id.nonce == n)?.id,
        IdentityKind::Obligation => unique(state.obligations.as_slice(), |r| r.id.nonce == n)?.id,
        _ => return Err(INVALID),
    };
    if primitive(id, holder.kind) {
        Ok(id)
    } else {
        Err(INVALID)
    }
}

/// The transaction's obligation: the T backlink and the (engine, T, origin session) tuple agree.
pub(super) fn transaction_obligation<'a>(
    state: &'a State,
    transaction: &TransactionRecord,
) -> Step<Option<&'a ObligationRecord>> {
    let mut tuple = state.obligations.as_slice().iter().filter(|o| {
        o.engine.0 == transaction.id.engine
            && o.transaction.0 == transaction.id.nonce
            && o.origin_session == transaction.birth_session
    });
    let by_tuple = tuple.next();
    if tuple.next().is_some() {
        return Err(INVALID);
    }
    match (transaction.obligation, by_tuple) {
        (None, None) => Ok(None),
        (Some(reference), Some(o))
            if o.id.nonce == reference.0 && primitive(o.id, IdentityKind::Obligation) =>
        {
            let duplicates = state
                .obligations
                .as_slice()
                .iter()
                .filter(|other| other.id.nonce == reference.0)
                .count();
            if duplicates == 1 {
                Ok(Some(o))
            } else {
                Err(INVALID)
            }
        }
        _ => Err(INVALID),
    }
}

/// The transaction of a directly handled obligation, with the exact backlink checked.
fn obligation_transaction<'a>(
    state: &'a State,
    obligation: &ObligationRecord,
) -> Step<&'a TransactionRecord> {
    let transaction = unique(state.transactions.as_slice(), |t| {
        t.id.nonce == obligation.transaction.0
    })?;
    match transaction_obligation(state, transaction)? {
        Some(o) if o.id == obligation.id => Ok(transaction),
        _ => Err(INVALID),
    }
}
