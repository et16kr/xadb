//! Exact target selection with only the adopted local state checks.
use super::input_schema::RawIdentity;
use super::schema::{
    ActorRecord, ContextRecord, Identity, IdentityKind, ResultRecord, State, StatementRecord,
};

/// Namespace selected by a raw action that carries a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// Prepared statement identities.
    Statement,
    /// Public result identities, including retired results.
    Result,
    /// Original call context identities, including completed contexts.
    Context,
}
impl TargetKind {
    fn identity_kind(self) -> IdentityKind {
        match self {
            Self::Statement => IdentityKind::Statement,
            Self::Result => IdentityKind::Result,
            Self::Context => IdentityKind::Context,
        }
    }

    fn is_handle(self) -> bool {
        !matches!(self, Self::Context)
    }
}

/// One target class in the frozen sixteen-selector alphabet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSelector {
    /// Original issue ordinal minus one, in zero through five; absence resolves to zero16.
    Issued(u8),
    /// Exact U0 through U9 malformed or never-issued target class.
    FrozenUnissued(u8),
}

/// An input or local workspace structure error, not an API admission result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorError {
    /// Caller actor is outside zero through one.
    InvalidActor,
    /// Issued or frozen-unissued selector index is outside its class range.
    InvalidSelector,
    /// The requested caller actor has no record.
    MissingActor,
    /// More than one record carries the requested caller actor.
    DuplicateActor,
    /// The caller's attached engine or session scalar is outside its frozen range.
    InvalidCallerScope,
    /// The required caller-local slot for U6 or U7 has no record.
    MissingSlot,
    /// More than one record carries the required caller-local slot.
    DuplicateSlot,
    /// The requested handle kind's next generation is outside one through thirteen.
    InvalidGeneration,
    /// A selected-namespace identity envelope or issue ordinal is malformed.
    InvalidIssuedRecord,
    /// Two selected-namespace records share an issue ordinal.
    DuplicateOrdinal,
    /// Two selected-namespace records share an identity nonce.
    DuplicateNonce,
    /// The selected kind's next nonce is not one more than its retained maximum.
    InvalidNextNonce,
}

/// Resolve a target without allocation, state mutation, or full-state validation.
///
/// Issued selectors search original issue ordinals without filtering actor, current scope,
/// retirement, or context status. Frozen selectors preserve their exact invalid bytes.
/// Only the requested actor, selected issued namespace, or required U6/U7 slot is read;
/// engine/session tables and unrelated counters are not inspected.
///
/// # Errors
///
/// Checks actor then selector arguments, caller presence/uniqueness then scope, and finally
/// branch-local structure. Issued checks every primitive before ordinal duplicates, nonce
/// duplicates, and the selected next counter. Slot duplicates precede generation checks.
/// Errors leave the original state unchanged and never stand in for environment exclusion.
pub fn resolve_selector(
    state: &State,
    actor: u8,
    kind: TargetKind,
    selector: TargetSelector,
) -> Result<RawIdentity, SelectorError> {
    if actor > 1 {
        return Err(SelectorError::InvalidActor);
    }
    match selector {
        TargetSelector::Issued(index) if index < 6 => {}
        TargetSelector::FrozenUnissued(index) if index < 10 => {}
        _ => return Err(SelectorError::InvalidSelector),
    }
    let caller = caller_record(state, actor)?;
    match selector {
        TargetSelector::Issued(index) => {
            let expected = kind.identity_kind();
            let next_nonce = &state.next_nonce[expected as usize - 1];
            let ordinal = u16::from(index) + 1;
            match kind {
                TargetKind::Statement => resolve_issued(
                    state.issued_statements.as_slice(),
                    expected,
                    next_nonce,
                    ordinal,
                ),
                TargetKind::Result => resolve_issued(
                    state.issued_results.as_slice(),
                    expected,
                    next_nonce,
                    ordinal,
                ),
                TargetKind::Context => {
                    resolve_issued(state.contexts.as_slice(), expected, next_nonce, ordinal)
                }
            }
        }
        TargetSelector::FrozenUnissued(index) => resolve_unissued(state, caller, kind, index),
    }
}

fn caller_record(state: &State, actor: u8) -> Result<&ActorRecord, SelectorError> {
    let mut matching = state
        .actors
        .as_slice()
        .iter()
        .filter(|row| row.actor == actor);
    let caller = matching.next().ok_or(SelectorError::MissingActor)?;
    // Presence/uniqueness precedes scope even when the first matching record is malformed.
    if matching.next().is_some() {
        return Err(SelectorError::DuplicateActor);
    }
    if !(1..=7).contains(&caller.engine.0) || !(1..=8).contains(&caller.session.0) {
        return Err(SelectorError::InvalidCallerScope);
    }
    Ok(caller)
}

// These views deliberately expose no lifecycle, descriptor, or context payload fields.
trait IssuedRecord {
    fn id(&self) -> &Identity;
    fn ordinal(&self) -> u16;
}
impl IssuedRecord for StatementRecord {
    fn id(&self) -> &Identity {
        &self.id
    }
    fn ordinal(&self) -> u16 {
        self.issue_ordinal
    }
}
impl IssuedRecord for ResultRecord {
    fn id(&self) -> &Identity {
        &self.id
    }
    fn ordinal(&self) -> u16 {
        self.issue_ordinal
    }
}
impl IssuedRecord for ContextRecord {
    fn id(&self) -> &Identity {
        &self.id
    }
    fn ordinal(&self) -> u16 {
        self.issue_ordinal
    }
}

fn resolve_issued<T: IssuedRecord>(
    rows: &[T],
    expected: IdentityKind,
    next_nonce: &u16,
    requested_ordinal: u16,
) -> Result<RawIdentity, SelectorError> {
    let mut maximum_nonce = 0;
    // Separate full passes fix error precedence across different rows and pairs.
    for row in rows {
        let id = row.id();
        let location_valid = if expected == IdentityKind::Context {
            id.slot == 255 && id.generation == 0
        } else {
            id.slot <= 1 && (1..=12).contains(&id.generation)
        };
        if !(1..=6).contains(&row.ordinal())
            || id.kind != expected
            || !(1..=6).contains(&id.nonce)
            || id.actor > 1
            || !(1..=7).contains(&id.engine)
            || !(1..=8).contains(&id.session)
            || !location_valid
        {
            return Err(SelectorError::InvalidIssuedRecord);
        }
        maximum_nonce = maximum_nonce.max(id.nonce);
    }
    // At most fifteen ordinal pairs, then fifteen nonce pairs, for six retained records.
    for (index, row) in rows.iter().enumerate() {
        if rows[index + 1..]
            .iter()
            .any(|other| row.ordinal() == other.ordinal())
        {
            return Err(SelectorError::DuplicateOrdinal);
        }
    }
    for (index, row) in rows.iter().enumerate() {
        if rows[index + 1..]
            .iter()
            .any(|other| row.id().nonce == other.id().nonce)
        {
            return Err(SelectorError::DuplicateNonce);
        }
    }
    if *next_nonce != maximum_nonce + 1 {
        return Err(SelectorError::InvalidNextNonce);
    }
    // A missing ordinal, including a gap, remains the exact zero target.
    Ok(rows
        .iter()
        .find(|row| row.ordinal() == requested_ordinal)
        .map_or(RawIdentity([0; 16]), |row| pack_identity(row.id())))
}

fn resolve_unissued(
    state: &State,
    caller: &ActorRecord,
    kind: TargetKind,
    index: u8,
) -> Result<RawIdentity, SelectorError> {
    if index == 0 {
        return Ok(RawIdentity([0; 16]));
    }
    let mut id = Identity {
        kind: kind.identity_kind(),
        actor: caller.actor,
        slot: if kind.is_handle() { 0 } else { 255 },
        engine: caller.engine.0,
        session: caller.session.0,
        nonce: 255,
        generation: if kind.is_handle() { 1 } else { 0 },
    };
    match index {
        1 => {
            id.kind = match kind {
                TargetKind::Statement => IdentityKind::Result,
                TargetKind::Result | TargetKind::Context => IdentityKind::Statement,
            };
            id.slot = 0;
            id.generation = 1;
        }
        2 => id.engine = 255,
        3 => id.session = 255,
        4 => id.slot = 2,
        5 => id.generation = 255,
        6 | 7 => {
            if kind.is_handle() {
                id.slot = index - 6;
                id.generation = next_generation(state, caller.actor, id.slot, kind)?;
            } else {
                id.generation = u16::from(index - 5);
            }
        }
        8 => id.generation = 0,
        9 => {}
        _ => return Err(SelectorError::InvalidSelector),
    }
    let mut raw = pack_identity(&id);
    if index == 9 {
        // UnknownTag is a raw negative target and is never coerced to a valid IdentityKind.
        raw.0[0] = 254;
    }
    Ok(raw)
}

fn next_generation(
    state: &State,
    actor: u8,
    slot: u8,
    kind: TargetKind,
) -> Result<u16, SelectorError> {
    let mut matching = state
        .prepared_slots
        .as_slice()
        .iter()
        .filter(|row| row.actor == actor && row.slot == slot);
    let record = matching.next().ok_or(SelectorError::MissingSlot)?;
    if matching.next().is_some() {
        return Err(SelectorError::DuplicateSlot);
    }
    let generation = match kind {
        TargetKind::Statement => record.next_statement_generation,
        TargetKind::Result => record.next_result_generation,
        TargetKind::Context => return Err(SelectorError::InvalidSelector),
    };
    if !(1..=13).contains(&generation) {
        return Err(SelectorError::InvalidGeneration);
    }
    Ok(generation)
}

fn pack_identity(id: &Identity) -> RawIdentity {
    let mut bytes = [0; 16];
    bytes[0] = id.kind as u8;
    bytes[1] = id.actor;
    bytes[2] = id.slot;
    bytes[4..6].copy_from_slice(&id.engine.to_le_bytes());
    bytes[6..8].copy_from_slice(&id.session.to_le_bytes());
    bytes[8..10].copy_from_slice(&id.nonce.to_le_bytes());
    bytes[10..12].copy_from_slice(&id.generation.to_le_bytes());
    RawIdentity(bytes)
}
