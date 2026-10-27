//! Current scope2 replay. Candidate native fields supply logical arguments only;
//! every successor is recomputed from the original accepted before arena.
use super::*;
use crate::storage::{
    allocation_page::ReservationOwner,
    catalog_record::{CatalogExpectation, CatalogTableInput},
    page::PageRef,
    retention_page::catalog as queue,
    semantic_catalog::{self as semantic, Change, Operation},
    status_home,
    transaction_record::{StatusSlotRef, TransactionState},
    wal_frame::WalFrameHeader,
};
type ObservationResult<T> = std::result::Result<T, RetentionCause>;
fn invalid() -> RetentionCause {
    RetentionCause::Invariant
}
fn sem(error: semantic::Error) -> RetentionCause {
    RetentionCause::CatalogSemantic(error)
}
fn job<'a>(
    cells: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    profile: Profile,
    ordinal: usize,
) -> ObservationResult<queue::JobView<'a>> {
    queue::decode_job(
        cells.get(ordinal).ok_or_else(invalid)?,
        profile.spec().namespace,
    )
    .map_err(RetentionCause::Retention)
}
fn actor_job(
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    profile: Profile,
    actor: TxnRef,
    terminal: bool,
) -> ObservationResult<Option<usize>> {
    let mut found = None;
    for i in 0..JOB_COUNT {
        let value = job(cells, profile, i)?;
        if value.source() == Some(actor) && value.state().is_terminal() == terminal {
            if found.is_some() {
                return Err(invalid());
            }
            found = Some(i);
        }
    }
    Ok(found)
}
fn row_actor(bank: &[u8], actor: TxnRef) -> ObservationResult<Option<usize>> {
    let mut found = None;
    for i in 0..32 {
        let row = control::row(bank, i);
        if row[PHASE] == STABLE
            && TxnRef::decode(&row[ACTOR]).map_err(RetentionCause::Page)? == actor
        {
            if found.is_some() {
                return Err(invalid());
            }
            found = Some(i);
        }
    }
    Ok(found)
}
fn image(arena: &[u8], page: PageRef) -> ObservationResult<&[u8; PAGE_BYTES]> {
    let i = usize::try_from(page.page_no).map_err(|_| invalid())?;
    if i >= 64 {
        return Err(invalid());
    }
    arena[i * PAGE_BYTES..(i + 1) * PAGE_BYTES]
        .try_into()
        .map_err(|_| invalid())
}
fn raw_descriptor(row: &[u8]) -> Option<&[u8; 1536]> {
    row[..1536]
        .iter()
        .any(|b| *b != 0)
        .then(|| row[..1536].try_into().expect("descriptor"))
}
fn native_job<'a>(
    view: semantic::LayoutView<'a>,
    profile: Profile,
    after: bool,
) -> ObservationResult<queue::JobView<'a>> {
    let mut found = None;
    for native in view.native_records() {
        if native.kind() == 9 {
            if found.is_some() {
                return Err(invalid());
            }
            let bytes = if after {
                native.after()
            } else {
                native.before()
            }
            .ok_or_else(invalid)?;
            found = Some(
                queue::decode_job(bytes, profile.spec().namespace)
                    .map_err(RetentionCause::Retention)?,
            );
        }
    }
    found.ok_or_else(invalid)
}
/// The source hash excludes temporary frame/after bytes but includes every
/// accepted historical INIT claim, including after its original Job is freed.
pub(in super::super) fn state_digest(replay: &[u8]) -> [u8; 32] {
    let mut hash = ContentHasher::new();
    hash.update(&replay[..BANK_END]);
    hash.update(&replay[AFTER_END..AFTER_END + PROVENANCE_BYTES]);
    hash.digest()
}
pub(in super::super) fn provenance(replay: &[u8]) -> &[[u8; 352]; 64] {
    replay[AFTER_END..AFTER_END + PROVENANCE_BYTES]
        .as_chunks::<352>()
        .0
        .try_into()
        .expect("provenance")
}
/// Last System identity is an observation from the accepted current replay,
/// never a reservation or a grant derived from arbitrary candidate bytes.
pub(in super::super) fn system_initial(
    replay: &[u8],
    high_water: u64,
) -> ObservationResult<Option<(PageRef, PageKind, u64)>> {
    system_initial_claims(provenance(replay), high_water)
}
pub(in super::super) fn system_initial_claims(
    claims: &[[u8; 352]; 64],
    high_water: u64,
) -> ObservationResult<Option<(PageRef, PageKind, u64)>> {
    let mut found = None;
    for bytes in claims {
        if bytes[0] == 0 || bytes[4] != 1 {
            continue;
        }
        let source = crate::storage::allocation_page::SystemWorkRef::decode(&bytes[128..176])
            .map_err(RetentionCause::Allocation)?;
        if source.id() == high_water {
            if found.is_some() {
                return Err(invalid());
            }
            found = Some((
                PageRef::decode(&bytes[8..32]).map_err(RetentionCause::Page)?,
                PageKind::from_byte(bytes[2]).map_err(RetentionCause::Page)?,
                u64_at(bytes, 48),
            ));
        }
    }
    Ok(found)
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("bounded scalar"))
}
fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
/// Caller supplies only a successfully recomputed INIT image and accepted frame.
/// These bytes remain an observation; the selected owner retains the source proof.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit frame provenance and caller-owned output are validated together without allocation"
)]
pub(in super::super) fn init_record(
    request: TargetRequest,
    frame: WalFrameHeader,
    frame_digest: [u8; 32],
    initial: &[u8; PAGE_BYTES],
    source: ReservationOwner,
    reserve: u64,
    root: Option<queue::JobView<'_>>,
    out: &mut [u8; 352],
) -> ObservationResult<()> {
    let decoded =
        page::decode_page(initial, request.identity, request.kind).map_err(RetentionCause::Page)?;
    if decoded.header().lsn.0 != frame.lsn
        || request.level != 0
        || reserve == 0
        || reserve >= frame.lsn
    {
        return Err(invalid());
    }
    out.fill(0);
    out[0] = 1;
    out[1] = match (request.kind, request.identity.owner.get()) {
        (PageKind::TransactionStatus, 2) => 1,
        (PageKind::Undo, 3) => 2,
        (PageKind::TreeAnchor, 6..) => 3,
        (PageKind::RowLeaf, 6..) => 4,
        _ => return Err(invalid()),
    };
    out[2] = request.kind as u8;
    request
        .identity
        .page
        .encode(&mut out[8..32])
        .map_err(RetentionCause::Page)?;
    put(out, 32, request.identity.owner.get());
    put(out, 40, request.identity.owner_generation.get());
    put(out, 48, frame.lsn);
    out[56..60].copy_from_slice(&frame.stream.writer_epoch.to_le_bytes());
    out[64..96].copy_from_slice(&frame_digest);
    out[96..128].copy_from_slice(&content_digest(initial));
    put(out, 176, reserve);
    match source {
        ReservationOwner::System(owner) if root.is_none() && out[1] <= 2 => {
            out[4] = 1;
            owner
                .encode(&mut out[128..176])
                .map_err(RetentionCause::Allocation)?;
        }
        ReservationOwner::Transaction(actor) if out[1] >= 3 => {
            let root = root.ok_or_else(invalid)?;
            if root.source() != Some(actor) || actor.id().writer_epoch != frame.stream.writer_epoch
            {
                return Err(invalid());
            }
            let object = root.object().ok_or_else(invalid)?;
            let pair = root.pair().ok_or_else(invalid)?;
            if root.lease_lsn() != reserve
                || pair[usize::from(out[1] == 4)] != request.identity.page
                || object.object != request.identity.owner
                || object.generation != request.identity.owner_generation
            {
                return Err(invalid());
            }
            out[4] = 2;
            actor
                .encode(&mut out[128..176])
                .map_err(RetentionCause::Page)?;
            put(out, 184, object.object.get());
            put(out, 192, object.generation.get());
            put(out, 200, object.schema_epoch.get());
            pair[0]
                .encode(&mut out[208..232])
                .map_err(RetentionCause::Page)?;
            pair[1]
                .encode(&mut out[232..256])
                .map_err(RetentionCause::Page)?;
        }
        _ => return Err(invalid()),
    }
    Ok(())
}

/// The accepted seed and exact semantic transition preserve the full graph.
/// This bounded closure checks cached queue/ATT/status agreement between frames;
/// full XOI/current graph validation is repeated by the v3 capture decoder.
pub(in super::super) fn validate_current(profile: Profile, replay: &[u8]) -> ObservationResult<()> {
    if profile.format() != 2
        || replay.len() < WORK_END
        || replay[AFTER_END + PROVENANCE_BYTES..WORK_END]
            .iter()
            .any(|b| *b != 0)
    {
        return Err(invalid());
    }
    let namespace = profile.spec().namespace;
    let (_, root) = queue::decode_root_page(arena_page(replay, 1), namespace)
        .map_err(RetentionCause::Retention)?;
    let cells: &[[u8; JOB_BYTES]; JOB_COUNT] = replay[ARENA_END..JOB_END]
        .as_chunks::<JOB_BYTES>()
        .0
        .try_into()
        .expect("jobs");
    let mut aggregate = [0; 640];
    queue::verify_census(root, namespace, cells, &mut aggregate)
        .map_err(RetentionCause::Retention)?;
    let bank = &replay[JOB_END..BANK_END];
    let aging = raw_descriptor(control::row(bank, AGING_SLOT));
    if root.aging().is_some() != aging.is_some() {
        return Err(invalid());
    }
    if let Some(aging) = aging {
        crate::storage::retention_obligation::catalog::decode(
            aging,
            profile,
            arena_page(replay, 1),
        )
        .map_err(|e| sem(semantic::Error::Aging(e)))?;
    }
    let allocation = allocation_page::decode_allocation_page(
        arena_page(replay, 0),
        recovery::allocator_identity(profile)?,
        namespace,
    )
    .map_err(RetentionCause::Allocation)?;
    let mut matched = 0u32;
    for ordinal in 0..JOB_COUNT {
        let terminal = job(cells, profile, ordinal)?;
        if !terminal.state().is_terminal() {
            continue;
        }
        let actor = terminal.source().ok_or_else(invalid)?;
        let location = terminal.status().ok_or_else(invalid)?;
        if allocation
            .entry(location.page().page_no as usize)
            .map_err(RetentionCause::Allocation)?
            != (AllocationEntry::Allocated {
                generation: location.page().incarnation,
                object: ObjectId::new(2).expect("Status"),
                object_generation: Generation::new(1).expect("one"),
                kind: PageKind::TransactionStatus,
            })
        {
            return Err(invalid());
        }
        let identity = PageIdentity {
            page: location.page(),
            owner: ObjectId::new(2).expect("Status"),
            ..recovery::allocator_identity(profile)?
        };
        let home = status_home::decode_status_home_page(
            image(replay, location.page())?,
            identity,
            namespace,
        )
        .map_err(RetentionCause::Page)?;
        let current = home.cell(0).map_err(RetentionCause::Page)?;
        let TransactionStatusCell::Occupied(status) = current.core() else {
            return Err(invalid());
        };
        if status.reference() != actor {
            return Err(invalid());
        }
        let att = row_actor(bank, actor)?;
        if terminal.state() == queue::JobState::TerminalLeased {
            let slot = att.ok_or_else(invalid)?;
            if matched & (1 << slot) != 0 {
                return Err(invalid());
            }
            matched |= 1 << slot;
            let root = actor_job(cells, profile, actor, false)?
                .map(|i| job(cells, profile, i))
                .transpose()?;
            semantic::ledger::validate_transaction(
                raw_descriptor(control::row(bank, slot)).ok_or_else(invalid)?,
                profile,
                current,
                root,
            )
            .map_err(sem)?;
            if StatusSlotRef::decode(&control::row(bank, slot)[LOCATION])
                .map_err(RetentionCause::Page)?
                != location
            {
                return Err(invalid());
            }
        } else if att.is_some()
            || !matches!(
                status.state(),
                TransactionState::Committed(_) | TransactionState::Aborted
            )
        {
            return Err(invalid());
        }
    }
    for i in 0..32 {
        let row = control::row(bank, i);
        if row.iter().any(|b| *b != 0)
            && (matched & (1 << i) == 0
                || row[PHASE] != STABLE
                || row[PHASE + 1..].iter().any(|b| *b != 0))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "disjoint original replay loans")]
pub(in super::super) fn apply(
    profile: Profile,
    expected: FrameExpectation,
    raw: &[u8],
    arena: &mut [u8],
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    bank: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 8],
    claims: &mut [[u8; 352]; 64],
    semantic_scratch: &mut [u8; REPLAY_SCRATCH_BYTES],
    adopted: &mut u64,
) -> ObservationResult<recovery::RetentionFrameObservation> {
    // Logical-argument/schema/Inputs carriers end before private publication.
    // The finishing phase retains only the selected borrowed identity packet.
    let (view, op, owner, location, root_before, destination, observed) = {
        let view = semantic::decode_layout(raw, expected, profile).map_err(sem)?;
        let op = view.operation();
        let candidate = if matches!(op, Operation::Begin | Operation::Reserve) {
            Some(native_job(view, profile, true)?)
        } else {
            None
        };
        let actor = match view.owner(0) {
            Some(ReservationOwner::Transaction(actor)) => Some(actor),
            _ => None,
        };
        let selected = if let Some(actor) = actor {
            if matches!(op, Operation::Begin | Operation::Reserve) {
                None
            } else {
                actor_job(
                    cells,
                    profile,
                    actor,
                    matches!(
                        op,
                        Operation::Commit | Operation::AbortEnd | Operation::RollbackStart
                    ),
                )?
            }
        } else if op == Operation::AgingStart {
            None
        } else {
            let old = native_job(view, profile, false)?;
            let source = old.source().ok_or_else(invalid)?;
            let selected = actor_job(cells, profile, source, op == Operation::TerminalFinish)?
                .ok_or_else(invalid)?;
            if job(cells, profile, selected)?.bytes() != old.bytes() {
                return Err(invalid());
            }
            Some(selected)
        };
        if !matches!(
            op,
            Operation::AgingStart | Operation::Begin | Operation::Reserve
        ) && selected.is_none()
        {
            return Err(invalid());
        }
        let selected_job = selected.map(|i| job(cells, profile, i)).transpose()?;
        let owner = actor.or_else(|| selected_job.and_then(|j| j.source()));
        let location = candidate.or(selected_job).and_then(|j| j.status());
        let root_before = owner
            .map(|actor| actor_job(cells, profile, actor, false))
            .transpose()?
            .flatten()
            .map(|i| job(cells, profile, i))
            .transpose()?;
        let att = owner
            .map(|actor| row_actor(bank, actor))
            .transpose()?
            .flatten();
        let destination = if op == Operation::Begin {
            let actor = actor.ok_or_else(invalid)?;
            if actor.id().writer_epoch != expected.stream.writer_epoch
                // A real pre-stage Unchanged failure may burn issued_local;
                // replay sees only adopted Begin records, so gaps are valid.
                || actor.id().local <= *adopted
                || att.is_some()
            {
                return Err(invalid());
            }
            Some(
                (0..32)
                    .find(|i| control::row(bank, *i).iter().all(|b| *b == 0))
                    .ok_or_else(invalid)?,
            )
        } else {
            att
        };
        let planned;
        let ordinal = selected.unwrap_or(0);
        let change = match op {
            Operation::AgingStart => Change::AgingStart,
            Operation::Begin => Change::Begin {
                actor: actor.ok_or_else(invalid)?,
                status: location.ok_or_else(invalid)?,
            },
            Operation::Reserve => {
                let job = candidate.ok_or_else(invalid)?;
                planned = semantic::queue::ReservePlan {
                    actor: actor.ok_or_else(invalid)?,
                    status: job.status().ok_or_else(invalid)?,
                    undo: job.undo().ok_or_else(invalid)?,
                    pair: job.pair().ok_or_else(invalid)?,
                    catalog_bytes: job.catalog_bytes(),
                    projection: *job.projection_digest(),
                };
                Change::Reserve(&planned)
            }
            Operation::Forward => {
                let object = root_before
                    .ok_or_else(invalid)?
                    .object()
                    .ok_or_else(invalid)?;
                Change::Forward {
                    ordinal,
                    definition: CatalogTableInput::View(
                        view.forward_table(CatalogExpectation {
                            namespace: profile.spec().namespace,
                            object: object.object,
                        })
                        .map_err(sem)?,
                    ),
                }
            }
            Operation::Cancel => Change::Cancel { ordinal },
            Operation::Clr => Change::Clr { ordinal },
            Operation::RollbackStart => Change::RollbackStart { terminal: ordinal },
            Operation::Commit => Change::Commit { terminal: ordinal },
            Operation::AbortEnd => Change::AbortEnd { terminal: ordinal },
            Operation::Prune => Change::Prune { ordinal },
            Operation::ReleaseRole => Change::ReleaseRole { ordinal },
            Operation::RootFree => Change::RootFree { ordinal },
            Operation::TerminalFinish => Change::TerminalFinish { ordinal },
        };
        let job_ordinal = if matches!(op, Operation::Begin | Operation::Reserve) {
            let (_, root) = queue::decode_root_page(arena_page(arena, 1), profile.spec().namespace)
                .map_err(RetentionCause::Retention)?;
            (0..JOB_COUNT)
                .map(|offset| (root.cursor() as usize + offset) % JOB_COUNT)
                .find(|i| job(cells, profile, *i).is_ok_and(|j| j.state() == queue::JobState::Free))
                .ok_or_else(invalid)?
        } else {
            ordinal
        };
        let root = if op == Operation::Reserve {
            candidate
        } else {
            root_before
        };
        let observed = semantic::observe_borrowed(
            &semantic::Inputs {
                profile,
                frame: expected,
                change,
                jobs: cells,
                root: arena_page(arena, 1),
                jobs_page: Some(arena_page(arena, 2 + job_ordinal / 15)),
                allocator: Some(arena_page(arena, 0)),
                catalog_anchor: Some(arena_page(arena, 7)),
                catalog: Some(arena_page(arena, 8)),
                status: location.map(|v| image(arena, v.page())).transpose()?,
                undo: root
                    .and_then(|j| j.undo())
                    .map(|u| image(arena, u.page()))
                    .transpose()?,
                transaction: att.and_then(|i| raw_descriptor(control::row(bank, i))),
                aging: raw_descriptor(control::row(bank, AGING_SLOT)),
            },
            raw,
            after,
            semantic_scratch,
        )
        .map_err(sem)?;
        (
            view,
            op,
            owner,
            location,
            root_before,
            destination,
            observed,
        )
    };
    let mut result = recovery::RetentionFrameObservation {
        header: observed.header,
        changed: [None; 8],
        initialized: [None; 2],
        retired: [None; 2],
    };
    let mut new_claims = [[0; 352]; 2];
    for (at, role) in observed.order[..usize::from(observed.count)]
        .iter()
        .copied()
        .enumerate()
    {
        let request = recovery::request_from_image(profile, &after[usize::from(role)])?;
        if request.identity.page.page_no >= 64
            || result.changed[..at].iter().flatten().any(|r| {
                r.identity.page.file == request.identity.page.file
                    && r.identity.page.page_no == request.identity.page.page_no
            })
        {
            return Err(invalid());
        }
        result.changed[at] = Some(request);
        if role >= 6 {
            let side = usize::from(role - 6);
            let index = request.identity.page.page_no as usize;
            if op != Operation::Forward
                || arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                    .iter()
                    .any(|b| *b != 0)
                || claims[index].iter().any(|b| *b != 0)
            {
                return Err(invalid());
            }
            let root = root_before.ok_or_else(invalid)?;
            init_record(
                request,
                observed.header,
                content_digest(raw),
                &after[usize::from(role)],
                ReservationOwner::Transaction(owner.ok_or_else(invalid)?),
                root.lease_lsn(),
                Some(root),
                &mut new_claims[side],
            )?;
            result.initialized[side] = Some(request);
        }
    }
    if op == Operation::Clr {
        let root = root_before.ok_or_else(invalid)?;
        let pair = root.pair().ok_or_else(invalid)?;
        let object = root.object().ok_or_else(invalid)?;
        for (side, page) in pair.iter().enumerate() {
            let req = recovery::request_from_image(profile, image(arena, *page)?)?;
            if req.identity.page != *page
                || req.identity.owner != object.object
                || req.identity.owner_generation != object.generation
                || req.level != 0
                || req.kind
                    != if side == 0 {
                        PageKind::TreeAnchor
                    } else {
                        PageKind::RowLeaf
                    }
                || claims[req.identity.page.page_no as usize]
                    .iter()
                    .all(|b| *b == 0)
            {
                return Err(invalid());
            }
            result.retired[side] = Some(req);
        }
    }
    let tx_destination = if op.handoff() || op == Operation::RollbackStart {
        Some(destination.ok_or_else(invalid)?)
    } else {
        None
    };
    // All fallible semantic/identity/claim preparation precedes publication into
    // this private canonical arena. No externally visible capability is issued.
    for (at, role) in observed.order[..usize::from(observed.count)]
        .iter()
        .copied()
        .enumerate()
    {
        let index = result.changed[at]
            .expect("checked target")
            .identity
            .page
            .page_no as usize;
        arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
            .copy_from_slice(&after[usize::from(role)]);
    }
    for (side, claim) in new_claims.iter().enumerate() {
        if let Some(request) = result.initialized[side] {
            claims[request.identity.page.page_no as usize] = *claim;
        }
        if let Some(request) = result.retired[side] {
            let index = request.identity.page.page_no as usize;
            arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES].fill(0);
            claims[index].fill(0);
        }
    }
    if op.handoff() || op == Operation::RollbackStart {
        let slot = tx_destination.expect("prechecked transaction destination");
        let row = control::row_mut(bank, slot);
        if matches!(op, Operation::Commit | Operation::AbortEnd) {
            row.fill(0);
        } else {
            row[..1536].copy_from_slice(view.descriptor(0).expect("validated descriptor"));
            owner
                .expect("checked actor")
                .encode(&mut row[ACTOR])
                .expect("fixed actor output");
            location
                .expect("checked Status")
                .encode(&mut row[LOCATION])
                .expect("fixed Status output");
            row[PHASE] = STABLE;
        }
    }
    if op != Operation::RollbackStart {
        let row = control::row_mut(bank, AGING_SLOT);
        row[..1536].copy_from_slice(
            view.descriptor(usize::from(op.handoff()))
                .expect("validated Aging"),
        );
        row[PHASE] = STABLE;
    }
    if op == Operation::Begin {
        *adopted = owner.expect("checked Begin").id().local;
    }
    Ok(result)
}

/// Caller-owned source carriers around the pure semantic observer. Backing
/// slices remain the original replay loan and are never counted as local arrays.
#[derive(Clone, Copy)]
pub(in super::super) struct SourceCensus {
    pub(in super::super) semantic: usize,
    pub(in super::super) finishing: usize,
    pub(in super::super) closure: usize,
}
pub(in super::super) fn source_bytes() -> SourceCensus {
    use crate::storage::obligation::DescriptorSpec;
    use std::mem::size_of;
    let hashing = 4 * size_of::<ContentHasher>() + 1536;
    let outer = 2 * size_of::<semantic::LayoutView<'_>>()
        + size_of::<semantic::Inputs<'_>>()
        // Inputs already embeds its copied Change/TableInput/TableView. The
        // separate local Change is the only additional live copy at observe.
        + size_of::<Change<'_>>()
        + size_of::<semantic::queue::ReservePlan>()
        + 8 * size_of::<Option<queue::JobView<'_>>>()
        + 4 * size_of::<Option<TxnRef>>()
        + 4 * size_of::<Option<StatusSlotRef>>()
        + 4 * size_of::<Option<usize>>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>()
        + 4 * size_of::<ObservationResult<()>>();
    // forward_table returns directly into Change::Forward; no separate schema
    // local or TableInput survives. Its construction returns are in the schema
    // closure, while observe retains the local Change and copied Inputs above.
    let semantic = outer
        + size_of::<&mut [u8; REPLAY_SCRATCH_BYTES]>()
        + semantic::borrowed_source_bytes().max(semantic::schema_source_bytes());
    let finishing = size_of::<semantic::LayoutView<'_>>()
        + size_of::<Operation>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Option<StatusSlotRef>>()
        + size_of::<Option<queue::JobView<'_>>>()
        + size_of::<Option<usize>>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>()
        + 4 * size_of::<ObservationResult<()>>()
        + 2 * size_of::<semantic::Observation>()
        + 2 * size_of::<std::result::Result<semantic::Observation, semantic::Error>>()
        + 2 * size_of::<recovery::RetentionFrameObservation>()
        + 2 * size_of::<ObservationResult<recovery::RetentionFrameObservation>>()
        + 2 * 352
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<ObservationResult<TargetRequest>>()
        + 4 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<crate::storage::page::PageHeader>()
        + hashing;
    let closure_outer = 640
        + 6 * size_of::<queue::JobView<'_>>()
        + 2 * size_of::<queue::RootView<'_>>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<status_home::StatusHomePage<'_>>()
        + 4 * size_of::<status_home::StatusHome>()
        + 4 * size_of::<ObservationResult<()>>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>();
    // The transaction verifier is a subset of the pure observer's ledger
    // closure. The actual Root2 Aging decoder and queue census stay explicit.
    let closure = closure_outer
        + queue::source_bytes()
            .max(crate::storage::retention_obligation::catalog::source_bytes())
            .max(semantic::transaction_source_bytes() + 2 * size_of::<DescriptorSpec>());
    SourceCensus {
        semantic,
        finishing,
        closure,
    }
}
