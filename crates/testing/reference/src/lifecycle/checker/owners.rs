//! Expected Env1 recipe (wire-v2 §3–§4, assembler contract) and its application.
//!
//! The recipe is recomputed from the pre-state and the admitted path; the supplied envelope
//! must equal it (rows compared as sets). A mismatch justifies `Excluded(Envelope)` (or
//! `RecoverySet` for recovery rows). Owner/obligation updates of the expected post-state are
//! derived from this plan, never from the supplied rows or the observed post-state.
use super::super::input_schema::{Environment, InputObligation, InputOwner};
use super::super::schema::*;
use super::CheckFailure;
use super::expect::Expect;
use super::ident::{self, pack, pack_opt};

const CAPACITY: CheckFailure = CheckFailure::CheckerCapacity("recipe table full");
const DANGLING: CheckFailure = CheckFailure::PreState("dangling owner holder");

/// One expected owner row and the data needed to apply it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OwnerPlan {
    pub id: Identity,
    pub kind: ResourceKind,
    pub birth: bool,
    /// Original holder of a birth; unused for an existing owner.
    pub original: Identity,
    pub before: Option<Identity>,
    pub after: Option<Identity>,
    pub disp: OwnerDisposition,
    pub obligation: Option<Identity>,
}

/// One expected obligation row.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ObligationPlan {
    pub id: Identity,
    pub txn: Identity,
    pub new: bool,
    pub known: Decision,
    /// Creation reason of a new obligation, or the reason of a cause update.
    pub reason: Option<ObligationReason>,
    /// Settlement applied by a drain or recovery.
    pub settle: Option<Settlement>,
}

/// Expected aggregate environment envelope of one admitted path.
pub(super) struct EnvPlan {
    pub kind: FactKind,
    pub fact: Identity,
    pub work_kind: u8,
    pub work_class: u8,
    pub cleanup: u8,
    pub commit_class: u8,
    pub transaction: Option<Identity>,
    pub receipt: Option<Identity>,
    pub anchor: Option<Identity>,
    pub fresh_engine: Option<Identity>,
    /// Receipt and anchor issued by this edge (counter advance).
    pub new_receipt: bool,
    pub new_anchor: bool,
    pub owners: Records<OwnerPlan, 64>,
    pub obligations: Records<ObligationPlan, 8>,
    next_resource: u16,
    next_obligation: u16,
}

/// Fact fields that come from the call rather than from the recipe.
#[derive(Clone, Copy)]
pub(super) struct FactSpec {
    pub api: Api,
    pub outcome: u8,
    pub context: Option<ContextRef>,
    pub cleanup: Cleanup,
    pub next_work: NextWork,
}

/// Outcome of comparing the supplied envelope with the recipe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EnvMatch {
    Exact,
    /// Kind, fact, scalar class, transaction, receipt/anchor or fresh engine differ.
    Header,
    /// Header agrees but obligation or owner rows differ.
    Rows,
}

impl EnvPlan {
    pub(super) fn new(pre: &State, kind: FactKind, fact_engine: u16) -> Self {
        Self {
            kind,
            fact: ident::engine_wide(
                IdentityKind::Fact,
                fact_engine,
                ident::next(pre, IdentityKind::Fact),
            ),
            work_kind: 0,
            work_class: 255,
            cleanup: 255,
            commit_class: 255,
            transaction: None,
            receipt: None,
            anchor: None,
            fresh_engine: None,
            new_receipt: false,
            new_anchor: false,
            owners: Records::default(),
            obligations: Records::default(),
            next_resource: ident::next(pre, IdentityKind::Resource),
            next_obligation: ident::next(pre, IdentityKind::Obligation),
        }
    }

    fn has_owner(&self, nonce: u16) -> bool {
        self.owners.as_slice().iter().any(|o| o.id.nonce == nonce)
    }

    fn push(&mut self, row: OwnerPlan) -> Result<(), CheckFailure> {
        if self.has_owner(row.id.nonce) {
            return Ok(());
        }
        self.owners.try_push(row).map_err(|_| CAPACITY)
    }

    /// A new retained resource held by `holder`, or born and released in this action.
    pub(super) fn birth(
        &mut self,
        kind: ResourceKind,
        holder: Identity,
        released: bool,
    ) -> Result<Identity, CheckFailure> {
        let id = ident::scoped(IdentityKind::Resource, holder, self.next_resource);
        self.next_resource = self.next_resource.wrapping_add(1);
        self.owners
            .try_push(OwnerPlan {
                id,
                kind,
                birth: true,
                original: holder,
                before: None,
                after: (!released).then_some(holder),
                disp: if released {
                    OwnerDisposition::Released
                } else {
                    OwnerDisposition::Held
                },
                obligation: None,
            })
            .map_err(|_| CAPACITY)?;
        Ok(id)
    }

    /// `RW3` births in Undo, Locks, AbortReserve order.
    pub(super) fn birth_rw(&mut self, txn: Identity, released: bool) -> Result<(), CheckFailure> {
        for kind in [
            ResourceKind::Undo,
            ResourceKind::Locks,
            ResourceKind::AbortReserve,
        ] {
            self.birth(kind, txn, released)?;
        }
        Ok(())
    }

    /// Cursor births in Snapshot, Scratch, Temp order.
    pub(super) fn birth_cursor(&mut self, result: Identity) -> Result<(), CheckFailure> {
        for kind in [
            ResourceKind::Snapshot,
            ResourceKind::Scratch,
            ResourceKind::Temp,
        ] {
            self.birth(kind, result, false)?;
        }
        Ok(())
    }

    /// Obligation linked to the original transaction holder of an RW/Recovery owner.
    fn linked(pre: &State, o: &OwnerRecord) -> Option<Identity> {
        if o.original_holder.kind != IdentityKind::Transaction
            || !ident::is_obligation_owner(o.resource_kind)
        {
            return None;
        }
        let t = ident::txn(pre, o.original_holder.nonce)?;
        t.obligation
            .and_then(|n| ident::obligation(pre, n.0))
            .map(|r| r.id)
    }

    fn current(pre: &State, o: &OwnerRecord) -> Result<Option<Identity>, CheckFailure> {
        match o.current_holder {
            None => Ok(None),
            Some(h) => ident::holder(pre, h).map(Some).ok_or(DANGLING),
        }
    }

    /// Release one live owner, linking an RW/Recovery row to its transaction's obligation.
    pub(super) fn release(&mut self, pre: &State, o: &OwnerRecord) -> Result<(), CheckFailure> {
        let link = Self::linked(pre, o);
        self.push(OwnerPlan {
            id: o.id,
            kind: o.resource_kind,
            birth: false,
            original: Identity::default(),
            before: Self::current(pre, o)?,
            after: None,
            disp: OwnerDisposition::Released,
            obligation: link,
        })
    }

    /// Report one owner unchanged under obligation `link`.
    pub(super) fn keep(
        &mut self,
        pre: &State,
        o: &OwnerRecord,
        link: Option<Identity>,
    ) -> Result<(), CheckFailure> {
        let current = Self::current(pre, o)?;
        self.push(OwnerPlan {
            id: o.id,
            kind: o.resource_kind,
            birth: false,
            original: Identity::default(),
            before: current,
            after: current,
            disp: o.disposition,
            obligation: link,
        })
    }

    /// Exact handoff of one live owner to obligation `to`; an existing transfer is kept.
    pub(super) fn handoff(
        &mut self,
        pre: &State,
        o: &OwnerRecord,
        to: Identity,
    ) -> Result<(), CheckFailure> {
        if o.disposition == OwnerDisposition::Transferred
            && o.current_holder == Some(ident::href(to))
        {
            return self.keep(pre, o, Some(to));
        }
        self.push(OwnerPlan {
            id: o.id,
            kind: o.resource_kind,
            birth: false,
            original: Identity::default(),
            before: Self::current(pre, o)?,
            after: Some(to),
            disp: OwnerDisposition::Transferred,
            obligation: Some(to),
        })
    }

    /// Include an obligation row once; later calls keep the first role.
    pub(super) fn obligation(&mut self, plan: ObligationPlan) -> Result<(), CheckFailure> {
        if self.obligations.as_slice().iter().any(|o| o.id == plan.id) {
            return Ok(());
        }
        self.obligations.try_push(plan).map_err(|_| CAPACITY)
    }

    /// Include existing obligations referenced by owner rows, keeping their known decision.
    pub(super) fn link_existing(
        &mut self,
        pre: &State,
        known: Option<Decision>,
    ) -> Result<(), CheckFailure> {
        let mut ids: Records<Identity, 8> = Records::default();
        for row in self.owners.as_slice() {
            if let Some(o) = row.obligation
                && !ids.as_slice().contains(&o)
            {
                ids.try_push(o).map_err(|_| CAPACITY)?;
            }
        }
        for o in ids.as_slice() {
            if let Some(rec) = ident::obligation(pre, o.nonce).filter(|r| r.id == *o) {
                let txn = ident::txn(pre, rec.transaction.0).ok_or(DANGLING)?.id;
                self.obligation(ObligationPlan {
                    id: rec.id,
                    txn,
                    new: false,
                    known: known.unwrap_or(rec.known_decision),
                    reason: None,
                    settle: None,
                })?;
            }
        }
        Ok(())
    }

    /// Allocate the next new obligation identity in the scope of `txn`.
    pub(super) fn new_obligation_id(&mut self, txn: Identity) -> Identity {
        let id = ident::scoped(IdentityKind::Obligation, txn, self.next_obligation);
        self.next_obligation = self.next_obligation.wrapping_add(1);
        id
    }

    fn linked_rows(&self, o: Identity) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for row in self.owners.as_slice() {
            if row.obligation == Some(o) {
                match row.disp {
                    OwnerDisposition::Held => counts.0 += 1,
                    OwnerDisposition::Transferred => counts.1 += 1,
                    OwnerDisposition::Released => counts.2 += 1,
                }
            }
        }
        counts
    }

    fn input_obligation(&self, o: &ObligationPlan) -> InputObligation {
        let (h, t, r) = self.linked_rows(o.id);
        InputObligation {
            obligation: pack(o.id),
            known_decision: o.known,
            after_ownership_summary: ident::summary(h, t, r),
            associated_owner_count: (h + t + r) as u16,
        }
    }

    fn input_owner(row: &OwnerPlan) -> InputOwner {
        InputOwner {
            resource: pack(row.id),
            resource_kind: row.kind,
            after_disposition: row.disp,
            obligation: pack_opt(row.obligation),
            before_holder: pack_opt(row.before),
            after_holder: pack_opt(row.after),
        }
    }

    /// Compare the supplied envelope with this recipe.
    pub(super) fn matches(&self, env: Option<&Environment>) -> EnvMatch {
        let Some(env) = env else {
            return EnvMatch::Header;
        };
        if !self.identities_match(env) || !self.header_matches(env) {
            return EnvMatch::Header;
        }
        if !self.rows_match(env) {
            return EnvMatch::Rows;
        }
        EnvMatch::Exact
    }

    /// Supplied fact and fresh-engine identities.
    pub(super) fn identities_match(&self, env: &Environment) -> bool {
        env.fact == pack(self.fact) && env.fresh_engine == pack_opt(self.fresh_engine)
    }

    /// Every other header byte: kind, scalar classes, transaction, receipt and anchor.
    pub(super) fn header_matches(&self, env: &Environment) -> bool {
        env.kind == self.kind
            && env.work_kind == self.work_kind
            && env.work_class == self.work_class
            && env.cleanup == self.cleanup
            && env.commit_class == self.commit_class
            && env.transaction == pack_opt(self.transaction)
            && env.receipt == pack_opt(self.receipt)
            && env.anchor == pack_opt(self.anchor)
    }

    /// Obligation and owner rows, compared as sets.
    pub(super) fn rows_match(&self, env: &Environment) -> bool {
        env.obligations.len() == self.obligations.len()
            && env.owners.len() == self.owners.len()
            && self.obligations.as_slice().iter().all(|o| {
                env.obligations
                    .as_slice()
                    .contains(&self.input_obligation(o))
            })
            && self
                .owners
                .as_slice()
                .iter()
                .all(|o| env.owners.as_slice().contains(&Self::input_owner(o)))
    }

    pub(super) fn births(&self) -> u16 {
        self.owners.as_slice().iter().filter(|o| o.birth).count() as u16
    }

    /// Owners, obligations, fact, membership rows and counters of this consumed envelope.
    /// The fact's decision/delivery are filled from the expected post transaction later.
    pub(super) fn apply(
        &self,
        pre: &State,
        exp: &mut Expect,
        spec: FactSpec,
    ) -> Result<(), CheckFailure> {
        let FactSpec {
            api,
            outcome,
            context,
            cleanup,
            next_work,
        } = spec;
        let fact = FactRef(self.fact.nonce);
        let d = &mut exp.delta;
        for row in self.owners.as_slice() {
            if row.birth {
                d.add_owner(OwnerRecord {
                    id: row.id,
                    resource_kind: row.kind,
                    disposition: row.disp,
                    transfer_count: 0,
                    original_holder: ident::href(row.original),
                    current_holder: row.after.map(ident::href),
                    previous_holder: (row.disp == OwnerDisposition::Released)
                        .then(|| ident::href(row.original)),
                    birth_context: context,
                    last_fact: Some(fact),
                })?;
                continue;
            }
            let pre_row = *ident_owner(pre, row.id.nonce)?;
            let changed = row.disp != pre_row.disposition
                || row.after.map(ident::href) != pre_row.current_holder;
            if !changed {
                continue;
            }
            let o = d.owner(pre, row.id.nonce)?;
            o.previous_holder = pre_row.current_holder;
            o.current_holder = row.after.map(ident::href);
            o.last_fact = Some(fact);
            if row.disp == OwnerDisposition::Transferred
                && (pre_row.disposition != OwnerDisposition::Transferred
                    || pre_row.current_holder != row.after.map(ident::href))
            {
                o.transfer_count = o.transfer_count.wrapping_add(1);
            }
            o.disposition = row.disp;
        }
        let mut new_obligations = 0;
        for plan in self.obligations.as_slice() {
            if plan.new {
                new_obligations += 1;
                let reason = plan
                    .reason
                    .unwrap_or(ObligationReason::PeerActiveTransactionAtFence);
                d.add_obligation(ObligationRecord {
                    id: plan.id,
                    engine: EngineRef(plan.txn.engine),
                    transaction: TransactionRef(plan.txn.nonce),
                    origin_session: SessionRef(plan.txn.session),
                    creation_reason: reason,
                    latest_reason: reason,
                    reason_mask: 1 << (reason as u8),
                    disposition: OwnershipSummary::None,
                    known_decision: plan.known,
                    settlement: Settlement::Outstanding,
                    first_fact: Some(fact),
                    latest_fact: Some(fact),
                    recovery_fact: None,
                })?;
                d.txn(pre, plan.txn.nonce)?.obligation = Some(ObligationRef(plan.id.nonce));
            } else {
                let o = d.obligation(pre, plan.id.nonce)?;
                o.latest_fact = Some(fact);
                o.known_decision = plan.known;
                if let Some(reason) = plan.reason {
                    o.latest_reason = reason;
                    o.reason_mask |= 1 << (reason as u8);
                }
                if let Some(settle) = plan.settle {
                    o.settlement = settle;
                    o.recovery_fact = Some(fact);
                }
            }
        }
        // Derived aggregate over all expected RW/Recovery owners of each touched obligation.
        for plan in self.obligations.as_slice() {
            let holder = HolderRef {
                kind: IdentityKind::Transaction,
                nonce: plan.txn.nonce,
            };
            let mut counts = (0, 0, 0);
            d.for_each_owner(pre, |o| {
                if o.original_holder == holder && ident::is_obligation_owner(o.resource_kind) {
                    match o.disposition {
                        OwnerDisposition::Held => counts.0 += 1,
                        OwnerDisposition::Transferred => counts.1 += 1,
                        OwnerDisposition::Released => counts.2 += 1,
                    }
                }
            });
            d.obligation(pre, plan.id.nonce)?.disposition =
                ident::summary(counts.0, counts.1, counts.2);
        }
        for plan in self.obligations.as_slice() {
            let row = self.input_obligation(plan);
            d.add_fact_obligation(FactObligation {
                fact,
                obligation: ObligationRef(plan.id.nonce),
                known_decision: plan.known,
                disposition: row.after_ownership_summary,
                owner_count: row.associated_owner_count,
            })?;
        }
        for row in self.owners.as_slice() {
            d.add_fact_owner(FactOwner {
                fact,
                resource: ResourceRef(row.id.nonce),
                disposition: row.disp,
                obligation: row.obligation.map(|o| ObligationRef(o.nonce)),
                before_holder: row.before.map(ident::href),
                after_holder: row.after.map(ident::href),
            })?;
        }
        d.bump(IdentityKind::Fact, 1);
        d.bump(IdentityKind::Resource, self.births());
        d.bump(IdentityKind::Obligation, new_obligations);
        if self.new_receipt {
            d.bump(IdentityKind::Receipt, 1);
        }
        if self.new_anchor {
            d.bump(IdentityKind::Anchor, 1);
        }
        if self.fresh_engine.is_some() {
            d.bump(IdentityKind::Engine, 1);
        }
        let receipt_part = exp.consumed & 4 != 0;
        exp.fact = Some(FactRecord {
            id: self.fact,
            kind: self.kind,
            api,
            outcome_class: outcome,
            cleanup,
            decision: Decision::NotApplicable,
            delivery: Delivery::NotApplicable,
            next_work,
            consumed_parts: exp.consumed,
            engine: EngineRef(self.fact.engine),
            obligation_count: self.obligations.len() as u16,
            owner_count: self.owners.len() as u16,
            context,
            transaction: self.transaction.map(|t| TransactionRef(t.nonce)),
            receipt: self
                .receipt
                .filter(|_| receipt_part)
                .map(|r| ReceiptRef(r.nonce)),
            fresh_engine: self.fresh_engine.map(|e| EngineRef(e.nonce)),
            anchor: self
                .anchor
                .filter(|_| receipt_part)
                .map(|a| AnchorRef(a.nonce)),
        });
        Ok(())
    }
}

fn ident_owner(pre: &State, nonce: u16) -> Result<&OwnerRecord, CheckFailure> {
    pre.owners
        .as_slice()
        .iter()
        .find(|o| o.id.nonce == nonce)
        .ok_or(DANGLING)
}

/// Live owners whose original holder is `holder` and whose kind satisfies `kinds`.
pub(super) fn live_of<'a>(
    pre: &'a State,
    holder: HolderRef,
    kinds: impl Fn(ResourceKind) -> bool + 'a,
) -> impl Iterator<Item = &'a OwnerRecord> + 'a {
    pre.owners.as_slice().iter().filter(move |o| {
        o.original_holder == holder
            && kinds(o.resource_kind)
            && o.disposition != OwnerDisposition::Released
    })
}

pub(super) fn txn_holder(nonce: u16) -> HolderRef {
    HolderRef {
        kind: IdentityKind::Transaction,
        nonce,
    }
}

/// `C(R)` release rows.
pub(super) fn release_cursor(
    plan: &mut EnvPlan,
    pre: &State,
    result: u16,
) -> Result<(), CheckFailure> {
    let holder = HolderRef {
        kind: IdentityKind::Result,
        nonce: result,
    };
    for o in live_of(pre, holder, ident::is_cursor) {
        plan.release(pre, o)?;
    }
    Ok(())
}

/// `RW(T)` release rows.
pub(super) fn release_rw(plan: &mut EnvPlan, pre: &State, txn: u16) -> Result<(), CheckFailure> {
    for o in live_of(pre, txn_holder(txn), ident::is_rw) {
        plan.release(pre, o)?;
    }
    Ok(())
}

/// `R(T)` release rows.
pub(super) fn release_recovery(
    plan: &mut EnvPlan,
    pre: &State,
    txn: u16,
) -> Result<(), CheckFailure> {
    for o in live_of(pre, txn_holder(txn), |k| k == ResourceKind::Recovery) {
        plan.release(pre, o)?;
    }
    Ok(())
}

/// `L(P)` release rows: current plan, current binding and the slot descriptor's cursors.
pub(super) fn release_local(
    plan: &mut EnvPlan,
    pre: &State,
    statement: u16,
    descriptor_result: Option<u16>,
) -> Result<(), CheckFailure> {
    let holder = HolderRef {
        kind: IdentityKind::Statement,
        nonce: statement,
    };
    for o in live_of(pre, holder, |k| {
        matches!(k, ResourceKind::Plan | ResourceKind::Binding)
    }) {
        plan.release(pre, o)?;
    }
    if let Some(r) = descriptor_result {
        release_cursor(plan, pre, r)?;
    }
    Ok(())
}

/// Fence class of a first fence or a cause update.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FenceClass {
    UndoIncomplete,
    Unknown,
    PublicationPending,
}

impl FenceClass {
    pub(super) fn reason(self) -> ObligationReason {
        match self {
            Self::UndoIncomplete => ObligationReason::KnownNotCommittedUndoPending,
            Self::Unknown => ObligationReason::CommitDecisionUnknown,
            Self::PublicationPending => ObligationReason::KnownCommittedPublicationPending,
        }
    }
    pub(super) fn decision(self) -> Decision {
        match self {
            Self::UndoIncomplete => Decision::NotCommittedRecoveryRequired,
            Self::Unknown => Decision::Unknown,
            Self::PublicationPending => Decision::Committed,
        }
    }
    pub(super) fn knowledge(self) -> ClientKnowledge {
        match self {
            Self::UndoIncomplete => ClientKnowledge::KnownNotCommittedRecoveryRequired,
            Self::Unknown | Self::PublicationPending => ClientKnowledge::Unknown,
        }
    }
    pub(super) fn engine_cause(self) -> EngineCause {
        match self {
            Self::UndoIncomplete => EngineCause::UndoIncomplete,
            Self::Unknown => EngineCause::CommitUnknown,
            Self::PublicationPending => EngineCause::CommittedPublicationPending,
        }
    }
    pub(super) fn origin_cause(self) -> OriginCause {
        match self {
            Self::UndoIncomplete => OriginCause::UndoIncomplete,
            Self::Unknown => OriginCause::CommitUnknown,
            Self::PublicationPending => OriginCause::PublicationPending,
        }
    }
}

/// Fence census of engine `engine`: outstanding RW transactions plus a prospective RW cause,
/// in transaction identity order. New obligations reuse an existing exact key; Recovery births
/// follow all other births in census order. A cause with an existing obligation receives a
/// count-0 cause update; an unchanged peer with one is omitted. A missing cause (no caller
/// transaction) still supplies the peer census, as the assembler contract requires.
pub(super) fn census(
    plan: &mut EnvPlan,
    pre: &State,
    engine: u16,
    cause: Option<Identity>,
    prospective: bool,
    class: FenceClass,
) -> Result<(), CheckFailure> {
    let mut txns: Records<Identity, 8> = Records::default();
    for t in pre.transactions.as_slice() {
        if t.id.engine == engine
            && t.life != TransactionLife::Ended
            && t.access == Access::ReadWrite
            && matches!(
                t.progress,
                CommitProgress::None
                    | CommitProgress::BeforeAttempt
                    | CommitProgress::Attempted
                    | CommitProgress::DurableConfirmed
            )
        {
            txns.try_push(t.id).map_err(|_| CAPACITY)?;
        }
    }
    if let Some(cause) = cause.filter(|c| prospective && !txns.as_slice().contains(c)) {
        txns.try_push(cause).map_err(|_| CAPACITY)?;
    }
    txns.as_mut_slice().sort_unstable_by_key(|t| t.key());
    let mut recovery: Records<(Identity, Identity), 8> = Records::default();
    for &t in txns.as_slice() {
        let is_cause = Some(t) == cause;
        let existing = ident::txn(pre, t.nonce)
            .and_then(|r| r.obligation)
            .and_then(|o| ident::obligation(pre, o.0))
            .map(|o| o.id);
        if let Some(o) = existing {
            if is_cause {
                plan.obligation(ObligationPlan {
                    id: o,
                    txn: t,
                    new: false,
                    known: class.decision(),
                    reason: Some(class.reason()),
                    settle: None,
                })?;
            }
            continue;
        }
        let o = plan.new_obligation_id(t);
        let known = if is_cause {
            class.decision()
        } else {
            ident::txn(pre, t.nonce).map_or(Decision::NotAttempted, |r| r.decision)
        };
        plan.obligation(ObligationPlan {
            id: o,
            txn: t,
            new: true,
            known,
            reason: Some(if is_cause {
                class.reason()
            } else {
                ObligationReason::PeerActiveTransactionAtFence
            }),
            settle: None,
        })?;
        for o_row in live_of(pre, txn_holder(t.nonce), ident::is_obligation_owner) {
            plan.keep(pre, o_row, Some(o))?;
        }
        // RW3 born by this edge for a prospective cause join the new obligation.
        for row in plan.owners.as_mut_slice() {
            if row.birth && row.original == t {
                row.obligation = Some(o);
            }
        }
        recovery.try_push((t, o)).map_err(|_| CAPACITY)?;
    }
    for &(t, o) in recovery.as_slice() {
        let before = plan.owners.len();
        plan.birth(ResourceKind::Recovery, t, false)?;
        plan.owners.as_mut_slice()[before].obligation = Some(o);
    }
    Ok(())
}
