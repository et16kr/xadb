//! Identity packing, construction and read-only lookups shared by the checker families.
//!
//! The 16-byte layout follows the frozen Identity16 contract
//! `(kind, actor, slot, 0, engine, session, nonce, generation, 0000)`; it is written here
//! independently of the codec and the selector.
use super::super::input_schema::RawIdentity;
use super::super::schema::*;

/// Exact Identity16 bytes for one defining identity.
pub(super) fn pack(id: Identity) -> RawIdentity {
    let mut b = [0u8; 16];
    b[0] = id.kind as u8;
    b[1] = id.actor;
    b[2] = id.slot;
    b[4..6].copy_from_slice(&id.engine.to_le_bytes());
    b[6..8].copy_from_slice(&id.session.to_le_bytes());
    b[8..10].copy_from_slice(&id.nonce.to_le_bytes());
    b[10..12].copy_from_slice(&id.generation.to_le_bytes());
    RawIdentity(b)
}

/// Optional packing for envelope comparisons.
pub(super) fn pack_opt(id: Option<Identity>) -> Option<RawIdentity> {
    id.map(pack)
}

/// Interpret raw bytes as a defining identity; reserved bytes and unknown kinds never match.
pub(super) fn unpack(raw: RawIdentity) -> Option<Identity> {
    let b = raw.0;
    if b[3] != 0 || b[12..].iter().any(|&x| x != 0) {
        return None;
    }
    let kind = IdentityKind::try_from(b[0]).ok()?;
    let word = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    Some(Identity {
        kind,
        actor: b[1],
        slot: b[2],
        engine: word(4),
        session: word(6),
        nonce: word(8),
        generation: word(10),
    })
}

/// Field-ordered constructor, matching the literal tuple order.
pub(super) fn ident(
    kind: IdentityKind,
    actor: u8,
    slot: u8,
    engine: u16,
    session: u16,
    nonce: u16,
    generation: u16,
) -> Identity {
    Identity {
        kind,
        actor,
        slot,
        engine,
        session,
        nonce,
        generation,
    }
}

/// A session-scoped non-handle identity (`slot255`, `generation0`) in the scope of `like`.
pub(super) fn scoped(kind: IdentityKind, like: Identity, nonce: u16) -> Identity {
    ident(kind, like.actor, 255, like.engine, like.session, nonce, 0)
}

/// An engine-wide identity such as a fact or anchor.
pub(super) fn engine_wide(kind: IdentityKind, engine: u16, nonce: u16) -> Identity {
    ident(kind, 255, 255, engine, 0, nonce, 0)
}

/// Stored next nonce of one kind; `None` has no counter.
pub(super) fn next(s: &State, kind: IdentityKind) -> u16 {
    match kind as usize {
        0 => 0,
        k => s.next_nonce[k - 1],
    }
}

/// Typed holder reference of a full identity.
pub(super) fn href(id: Identity) -> HolderRef {
    HolderRef {
        kind: id.kind,
        nonce: id.nonce,
    }
}

pub(super) fn actor(s: &State, actor: u8) -> Option<&ActorRecord> {
    s.actors.as_slice().iter().find(|a| a.actor == actor)
}
pub(super) fn engine(s: &State, nonce: u16) -> Option<&EngineRecord> {
    s.engines.as_slice().iter().find(|e| e.id.nonce == nonce)
}
pub(super) fn slot(s: &State, actor: u8, slot: u8) -> Option<&PreparedSlot> {
    s.prepared_slots
        .as_slice()
        .iter()
        .find(|p| p.actor == actor && p.slot == slot)
}
pub(super) fn statement(s: &State, nonce: u16) -> Option<&StatementRecord> {
    s.issued_statements
        .as_slice()
        .iter()
        .find(|r| r.id.nonce == nonce)
}
pub(super) fn result(s: &State, nonce: u16) -> Option<&ResultRecord> {
    s.issued_results
        .as_slice()
        .iter()
        .find(|r| r.id.nonce == nonce)
}
pub(super) fn descriptor(s: &State, result: u16) -> Option<&Descriptor> {
    s.private_descriptors
        .as_slice()
        .iter()
        .find(|d| d.result.0 == result)
}
pub(super) fn context(s: &State, nonce: u16) -> Option<&ContextRecord> {
    s.contexts.as_slice().iter().find(|r| r.id.nonce == nonce)
}
pub(super) fn txn(s: &State, nonce: u16) -> Option<&TransactionRecord> {
    s.transactions
        .as_slice()
        .iter()
        .find(|r| r.id.nonce == nonce)
}
pub(super) fn obligation(s: &State, nonce: u16) -> Option<&ObligationRecord> {
    s.obligations
        .as_slice()
        .iter()
        .find(|r| r.id.nonce == nonce)
}
pub(super) fn receipt(s: &State, nonce: u16) -> Option<&ReceiptRecord> {
    s.receipts.as_slice().iter().find(|r| r.id.nonce == nonce)
}

/// Full identity of a typed holder, resolved in the pre-state.
pub(super) fn holder(s: &State, h: HolderRef) -> Option<Identity> {
    match h.kind {
        IdentityKind::Engine => engine(s, h.nonce).map(|r| r.id),
        IdentityKind::Statement => statement(s, h.nonce).map(|r| r.id),
        IdentityKind::Result => result(s, h.nonce).map(|r| r.id),
        IdentityKind::Transaction => txn(s, h.nonce).map(|r| r.id),
        IdentityKind::Obligation => obligation(s, h.nonce).map(|r| r.id),
        _ => None,
    }
}

/// Cursor aggregate kinds owned by a result.
pub(super) fn is_cursor(kind: ResourceKind) -> bool {
    matches!(
        kind,
        ResourceKind::Snapshot | ResourceKind::Scratch | ResourceKind::Temp
    )
}

/// Transaction termination aggregates (`RW3`).
pub(super) fn is_rw(kind: ResourceKind) -> bool {
    matches!(
        kind,
        ResourceKind::Undo | ResourceKind::Locks | ResourceKind::AbortReserve
    )
}

/// Owners counted by an obligation's derived disposition: `RW3` plus `Recovery`.
pub(super) fn is_obligation_owner(kind: ResourceKind) -> bool {
    is_rw(kind) || kind == ResourceKind::Recovery
}

/// Aggregate of after-dispositions; zero rows is `None`.
pub(super) fn summary(held: usize, transferred: usize, released: usize) -> OwnershipSummary {
    match (held, transferred, released) {
        (0, 0, 0) => OwnershipSummary::None,
        (_, 0, 0) => OwnershipSummary::AllHeld,
        (0, _, 0) => OwnershipSummary::AllTransferred,
        (0, 0, _) => OwnershipSummary::AllReleased,
        _ => OwnershipSummary::Mixed,
    }
}
