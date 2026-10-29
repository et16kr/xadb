//! Canonical empty-transaction continuation of the original recovery prefix.
//! All page and descriptor storage is borrowed from the maintenance arena.
use super::*;
use crate::storage::{
    obligation::ResourcePeak,
    semantic_group::{self, RollbackStartExpected, begin, empty_abort},
    status_home::decode_status_home_page,
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef,
    },
    wal_frame::RedoTarget,
};

pub(super) const PAGE_AREA: usize = TARGET_COUNT * PAGE_BYTES;
const ROW: usize = 1664;
const ROWS: usize = 32;
const LOCATION: std::ops::Range<usize> = 1536..1576;
const ACTOR: std::ops::Range<usize> = 1576..1624;
const PRESENT: usize = 1624;
pub(super) const ARENA_BYTES: usize = PAGE_AREA + ROWS * ROW;

#[derive(Default, PartialEq, Eq)]
pub(super) struct Transactions {
    pub(super) high_water: u64,
    pub(super) active: u32,
}
fn invalid() -> RecoveryPrefixError {
    semantic(SemanticError::StateMismatch)
}
fn transaction_error(cause: semantic_group::SemanticError) -> RecoveryPrefixError {
    error(Phase::Semantic, Cause::Transaction(cause))
}
pub(super) fn bank_digest(arena: &[u8; ARENA_BYTES]) -> [u8; 32] {
    content_digest(&arena[PAGE_AREA..])
}
pub(super) fn descriptor_at(arena: &[u8; ARENA_BYTES], index: usize) -> Option<&[u8; 1536]> {
    if index >= ROWS {
        return None;
    }
    let row = &arena[PAGE_AREA + index * ROW..PAGE_AREA + (index + 1) * ROW];
    (row[PRESENT] == 1).then(|| row[..1536].try_into().expect("transaction descriptor"))
}

/// Consumables accumulate; one closed executor lends progress space to one
/// step at a time. No row or owner contributes another simultaneous FD/frame.
fn merge_budget(total: &mut RemainingBudget, value: RemainingBudget) -> Result<()> {
    macro_rules! add {
        ($($field:ident),*) => { $(total.$field = total.$field.checked_add(value.$field).ok_or_else(arithmetic)?;)* };
    }
    add!(
        steps,
        wal,
        control,
        growth,
        pages,
        undo_slots,
        status_slots,
        protected
    );
    total.peak = ResourcePeak {
        frames: total.peak.frames.max(value.peak.frames),
        memory: total.peak.memory.max(value.peak.memory),
        locks: total.peak.locks.max(value.peak.locks),
        registry: total.peak.registry.max(value.peak.registry),
        files: total.peak.files.max(value.peak.files),
        doublewrite: total.peak.doublewrite.max(value.peak.doublewrite),
    };
    Ok(())
}
fn aggregate(arena: &[u8; ARENA_BYTES], profile: Profile) -> Result<(RemainingBudget, u32)> {
    let mut remaining = RemainingBudget::default();
    let mut count = 0u32;
    for row in arena[PAGE_AREA..].chunks_exact(ROW) {
        if row[PRESENT] == 0 {
            if row.iter().any(|b| *b != 0) {
                return Err(invalid());
            }
            continue;
        }
        if row[PRESENT] != 1 || row[PRESENT + 1..].iter().any(|b| *b != 0) {
            return Err(invalid());
        }
        let actor = TxnRef::decode(&row[ACTOR]).map_err(|_| invalid())?;
        let descriptor = obligation::decode_descriptor(
            &row[..1536],
            ReservationOwner::Transaction(actor),
            profile_ref(profile),
        )
        .map_err(|_| invalid())?;
        for budget in descriptor.remaining {
            merge_budget(&mut remaining, budget)?;
        }
        count += 1;
    }
    Ok((remaining, count))
}

pub(super) fn adopt(
    observation: &mut RecoveryPrefixObservation,
    allocator: PageIdentity,
    images: CanonicalImages<'_>,
    arena: &mut [u8; ARENA_BYTES],
    header: WalFrameHeader,
    bytes: &[u8],
) -> Result<()> {
    if observation.scope != RecoveryScope::Effects
        || observation.plan != 2
        || observation.active.is_some()
        || observation.status_mask == 0
        || bytes.len() != 3040
        || header.previous_lsn != observation.complete_lsn
        || header.lsn
            != observation
                .complete_lsn
                .checked_add(1)
                .ok_or_else(arithmetic)?
    {
        return Err(invalid());
    }
    if observation.transactions.high_water == 0 {
        validate_operational_before(observation, allocator, images.before, images.descriptor)?;
        if observation.remaining != RemainingBudget::default() {
            return Err(invalid());
        }
    }
    let opcode = get16(bytes, 166);
    if !matches!(
        (opcode, header.kind),
        (1, FrameKind::Begin) | (3, FrameKind::Update) | (7, FrameKind::AbortEnd)
    ) {
        return Err(semantic(SemanticError::UnsupportedOperation));
    }
    let redo = wal_frame::decode_redo_record(header, &bytes[2720..3008])
        .map_err(|e| error(Phase::Semantic, Cause::Wal(e)))?;
    let index = usize::try_from(redo.target.page.page_no).map_err(|_| invalid())?;
    if !(1..TARGET_COUNT).contains(&index) || observation.status_mask & (1u64 << index) == 0 {
        return Err(invalid());
    }
    let table = &images.targets[index * TARGET_BYTES..(index + 1) * TARGET_BYTES];
    let identity = PageIdentity {
        page: PageRef {
            page_no: index as u64,
            incarnation: Generation::new(u64::from_le_bytes(
                table[8..16].try_into().expect("incarnation"),
            ))
            .map_err(|_| invalid())?,
            ..allocator.page
        },
        owner: ObjectId::new(2).expect("Status owner"),
        owner_generation: Generation::new(1).expect("Status generation"),
        ..allocator
    };
    if redo.target
        != (RedoTarget {
            page: identity.page,
            owner: identity.owner,
            owner_generation: identity.owner_generation,
            kind: PageKind::TransactionStatus,
            level: 0,
        })
    {
        return Err(invalid());
    }
    let actor = TxnRef::decode(&bytes[200..248]).map_err(|_| invalid())?;
    if actor.generation().get() != 2 || actor.id().writer_epoch != header.stream.writer_epoch {
        return Err(invalid());
    }
    let location =
        StatusSlotRef::new(identity.page, 0, actor.generation()).map_err(|_| invalid())?;
    let (pages, bank) = arena.split_at_mut(PAGE_AREA);
    images
        .status
        .copy_from_slice(&pages[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]);
    let row_index = if opcode == 1 {
        if actor.id().local <= observation.transactions.high_water {
            return Err(invalid());
        }
        bank.chunks_exact(ROW)
            .position(|r| r[PRESENT] == 0)
            .ok_or_else(|| error(Phase::Ledger, Cause::Capacity))?
    } else {
        bank.chunks_exact(ROW)
            .position(|r| {
                r[PRESENT] == 1
                    && TxnRef::decode(&r[ACTOR]).ok() == Some(actor)
                    && StatusSlotRef::decode(&r[LOCATION]).ok() == Some(location)
            })
            .ok_or_else(invalid)?
    };
    let row = &mut bank[row_index * ROW..(row_index + 1) * ROW];
    let expected = RollbackStartExpected {
        frame: FrameExpectation {
            stream: header.stream,
            previous_lsn: observation.complete_lsn,
        },
        profile: observation.profile,
        actor,
        location,
        page: identity,
    };
    match opcode {
        1 => begin::materialize_begin(
            bytes,
            begin::BeginExpected {
                frame: expected.frame,
                profile: expected.profile,
                actor,
                location,
                page: identity,
                previous_local: actor.id().local.checked_sub(1).ok_or_else(invalid)?,
            },
            images.status,
            images.next_status,
        )
        .map(|_| ())
        .map_err(transaction_error)?,
        3 => semantic_group::materialize_rollback_start(
            bytes,
            expected,
            &row[..1536],
            images.status,
            images.next_status,
        )
        .map(|_| ())
        .map_err(transaction_error)?,
        7 => empty_abort::materialize_empty_abort(
            bytes,
            expected,
            &row[..1536],
            images.status,
            images.next_status,
        )
        .map(|_| ())
        .map_err(transaction_error)?,
        _ => return Err(invalid()),
    }
    // Full materializers bind the native before/after, exact descriptor and
    // page bytes. This restricted owner also requires the empty lifecycle.
    let page = decode_status_home_page(images.next_status, identity, observation.namespace())
        .map_err(|_| invalid())?;
    let home = page.cell(0).map_err(|_| invalid())?;
    let TransactionStatusCell::Occupied(status) = home.core() else {
        return Err(invalid());
    };
    if status.reference() != actor || status.undo_head().is_some() || home.incoming() != 0 {
        return Err(invalid());
    }
    if !matches!(
        (opcode, status.state()),
        (1, TransactionState::Active)
            | (
                3,
                TransactionState::Aborting(RollbackState::Transaction { next: None })
            )
            | (7, TransactionState::Aborted)
    ) {
        return Err(invalid());
    }
    match_pending(
        observation,
        header.lsn,
        identity,
        PageKind::TransactionStatus,
        images.next_status,
    )?;
    if opcode == 7 {
        row.fill(0);
    } else {
        row[..1536].copy_from_slice(&bytes[352..1888]);
        location.encode(&mut row[LOCATION]).map_err(|_| invalid())?;
        actor.encode(&mut row[ACTOR]).map_err(|_| invalid())?;
        row[PRESENT] = 1;
    }
    pages[index * PAGE_BYTES..(index + 1) * PAGE_BYTES].copy_from_slice(images.next_status);
    let table = &mut images.targets[index * TARGET_BYTES..(index + 1) * TARGET_BYTES];
    table[..8].copy_from_slice(&header.lsn.to_le_bytes());
    table[16..].copy_from_slice(&content_digest(images.next_status));
    if opcode == 1 {
        observation.transactions.high_water = actor.id().local;
    }
    let (remaining, count) = aggregate(arena, observation.profile)?;
    observation.remaining = remaining;
    observation.transactions.active = count;
    Ok(())
}

pub(super) fn component_bytes() -> usize {
    begin::component_bytes().max(empty_abort::component_bytes())
        + 2 * size_of::<CanonicalImages<'static>>()
        + 4 * size_of::<RollbackStartExpected>()
        + 4 * size_of::<RemainingBudget>()
        + 2 * size_of::<TxnRef>()
        + 2 * size_of::<StatusSlotRef>()
        // RollbackStart shares the common status-patch observer/materializer;
        // empty Abort retains seven descriptors and eight result carriers,
        // covering RollbackStart's before/after decode and observation returns.
        + 4 * size_of::<crate::storage::wal_frame::RedoRecordView<'static>>()
        + 4 * size_of::<WalFrameHeader>()
        + 4 * size_of::<crate::storage::status_home::StatusHome>()
        + 2 * size_of::<crate::storage::status_home::StatusHomePage<'static>>()
        + 2 * size_of::<ContentHasher>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<Result<(RemainingBudget, u32)>>()
        + 4 * size_of::<(&[u8], &mut [u8], PageIdentity, usize, u64, u32, bool)>()
}

/// Typed empty-home inspection for recovery only. Provisioning readers retain
/// their Free1-only contract. Physical bytes never become semantic authority.
pub(super) fn empty_home(
    bytes: &[u8; PAGE_BYTES],
    identity: PageIdentity,
    namespace: Namespace,
) -> Result<crate::storage::status_home::StatusHome> {
    init::status_envelope(bytes, &identity).map_err(semantic)?;
    let page = decode_status_home_page(bytes, identity, namespace).map_err(|_| invalid())?;
    let home = page.cell(0).map_err(|_| invalid())?;
    if home.incoming() != 0 {
        return Err(invalid());
    }
    match home.core() {
        TransactionStatusCell::Free(g) if g.get() == 1 => {}
        TransactionStatusCell::Occupied(status)
            if status.reference().generation().get() == 2
                && status.reference().id().writer_epoch == 1
                && status.undo_head().is_none()
                && matches!(
                    status.state(),
                    TransactionState::Active
                        | TransactionState::Aborting(RollbackState::Transaction { next: None })
                        | TransactionState::Aborted
                ) => {}
        _ => return Err(invalid()),
    }
    Ok(home)
}

/// Bidirectional final canonical census: every nonterminal home has exactly
/// one row and every retained row is encountered at its exact adopted home.
pub(super) fn census(
    observation: &RecoveryPrefixObservation,
    allocator: PageIdentity,
    arena: &[u8; ARENA_BYTES],
    targets: &[u8],
) -> Result<()> {
    let mut seen = 0u32;
    for index in 1..TARGET_COUNT {
        if observation.status_mask & (1u64 << index) == 0 {
            continue;
        }
        let table = &targets[index * TARGET_BYTES..(index + 1) * TARGET_BYTES];
        let identity = PageIdentity {
            page: PageRef {
                page_no: index as u64,
                incarnation: Generation::new(u64::from_le_bytes(
                    table[8..16].try_into().expect("incarnation"),
                ))
                .map_err(|_| invalid())?,
                ..allocator.page
            },
            owner: ObjectId::new(2).expect("Status owner"),
            owner_generation: Generation::new(1).expect("Status generation"),
            ..allocator
        };
        let image: &[u8; PAGE_BYTES] = arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
            .try_into()
            .expect("canonical Status");
        if image[64..72] != table[..8] || content_digest(image) != table[16..] {
            return Err(invalid());
        }
        let home = empty_home(image, identity, observation.namespace())?;
        let active = match home.core() {
            TransactionStatusCell::Occupied(status)
                if status.state() != TransactionState::Aborted =>
            {
                Some(status)
            }
            _ => None,
        };
        let mut found = false;
        for (slot, row) in arena[PAGE_AREA..].chunks_exact(ROW).enumerate() {
            if row[PRESENT] == 0 {
                continue;
            }
            let location = StatusSlotRef::decode(&row[LOCATION]).map_err(|_| invalid())?;
            if location.page() != identity.page {
                continue;
            }
            let status = active.ok_or_else(invalid)?;
            if found
                || location.slot() != 0
                || location.generation() != status.reference().generation()
                || status.reference().id().local > observation.transactions.high_water
                || TxnRef::decode(&row[ACTOR]).map_err(|_| invalid())? != status.reference()
            {
                return Err(invalid());
            }
            let d = obligation::decode_descriptor(
                &row[..1536],
                ReservationOwner::Transaction(status.reference()),
                profile_ref(observation.profile),
            )
            .map_err(|_| invalid())?;
            if d.owner != obligation::OwnerState::Transaction(status) {
                return Err(invalid());
            }
            found = true;
            seen |= 1u32 << slot;
        }
        if found != active.is_some() {
            return Err(invalid());
        }
    }
    let (remaining, count) = aggregate(arena, observation.profile)?;
    if seen.count_ones() != count
        || count != observation.transactions.active
        || remaining != observation.remaining
    {
        return Err(invalid());
    }
    Ok(())
}
