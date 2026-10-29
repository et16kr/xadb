//! Current scope3 Row replay (XADB-MODULE-ROW-BEGIN-001, "replay와 분석
//! gate"). Live recording and recovery replay share `record`, the one body
//! that writes the ATT/Aging rows and Job cells of an observed group. Every
//! successor is recomputed by the whole-group Row observer
//! (`semantic_row::transition::observe_borrowed`) from the original accepted
//! before arena; candidate natives and the core only select logical
//! arguments. This module never calls `semantic_catalog`'s observer.
//!
//! RowReserve is refused here before any effect: its frame does not carry the
//! canonical row value the observer recomputes the Job projection and leaf
//! fit from, so no Reserve successor can be derived from the arena and bank.
use super::catalog::{image, raw_descriptor, row_actor};
use super::*;
use crate::storage::{
    allocation_page::ReservationOwner,
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation},
    obligation,
    page::{FileId, PageRef},
    retention_obligation,
    retention_page::row::{self as job3, JobCells, JobScope},
    row_record::RowSchemaInput,
    semantic_row::{
        self as semantic, LayoutView, Operation,
        status::StatementBoundary,
        transition::{self, Change, Inputs, Observation, ROW_REPLAY_SCRATCH_BYTES, TableSource},
    },
    status_home::{self, StatusHome},
    transaction_record::{RollbackState, StatusSlotRef, TransactionState},
    tree_page::{self, EntryPayload, TreeExpectation},
};

type ObservationResult<T> = std::result::Result<T, RetentionCause>;
fn invalid() -> RetentionCause {
    RetentionCause::Invariant
}
fn sem(error: semantic::Error) -> RetentionCause {
    RetentionCause::RowSemantic(error)
}
fn job<'a>(
    cells: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    profile: Profile,
    ordinal: usize,
) -> ObservationResult<job3::JobView<'a>> {
    job3::decode_job(
        cells.get(ordinal).ok_or_else(invalid)?,
        profile.spec().namespace,
    )
    .map_err(RetentionCause::Retention)
}
/// The actor's unique Row-scope Job in `state`.
fn actor_job(
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    profile: Profile,
    actor: TxnRef,
    state: job3::JobState,
) -> ObservationResult<usize> {
    let mut found = None;
    for i in 0..JOB_COUNT {
        let value = job(cells, profile, i)?;
        if value.scope() == Some(JobScope::Row)
            && value.source() == Some(actor)
            && value.state() == state
            && found.replace(i).is_some()
        {
            return Err(invalid());
        }
    }
    found.ok_or_else(invalid)
}
/// The candidate's single Job native (kind 9): its before, after and slot.
type NativeJob<'a> = (Option<&'a [u8]>, Option<&'a [u8]>, Option<u16>);
fn native_job(view: LayoutView<'_>) -> ObservationResult<NativeJob<'_>> {
    let mut found = None;
    for native in view.native_records() {
        if native.kind() == 9
            && found
                .replace((native.before(), native.after(), native.before_slot()))
                .is_some()
        {
            return Err(invalid());
        }
    }
    found.ok_or_else(invalid)
}
/// The one canonical non-Free cell at the native's slot whose bytes equal
/// the candidate's Job before. The observer compares that before again.
fn native_before(
    view: LayoutView<'_>,
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
) -> ObservationResult<usize> {
    let (before, _, slot) = native_job(view)?;
    let before = before.ok_or_else(invalid)?;
    let slot = usize::from(slot.ok_or_else(invalid)?);
    let mut found = None;
    for (ordinal, cell) in cells.iter().enumerate() {
        if ordinal % 15 == slot
            && cell[6] != 0
            && cell.as_slice() == before
            && found.replace(ordinal).is_some()
        {
            return Err(invalid());
        }
    }
    found.ok_or_else(invalid)
}
/// Begin's Jobs page: the first Free Job from the Root cursor.
fn first_free(
    arena: &[u8],
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    profile: Profile,
) -> ObservationResult<usize> {
    let (_, root) = job3::decode_root_page(arena_page(arena, 1), profile.spec().namespace)
        .map_err(RetentionCause::Retention)?;
    (0..JOB_COUNT)
        .map(|offset| (root.cursor() as usize + offset) % JOB_COUNT)
        .find(|i| matches!(job(cells, profile, *i), Ok(j) if j.state() == job3::JobState::Free))
        .ok_or_else(invalid)
}
fn status_identity(profile: Profile, location: StatusSlotRef) -> ObservationResult<PageIdentity> {
    Ok(PageIdentity {
        page: location.page(),
        owner: ObjectId::new(2).expect("Status"),
        ..recovery::allocator_identity(profile)?
    })
}
/// The current cell of an actor's one-home Status page.
fn status_home(
    arena: &[u8],
    profile: Profile,
    location: StatusSlotRef,
) -> ObservationResult<StatusHome> {
    status_home::decode_status_home_page(
        image(arena, location.page())?,
        status_identity(profile, location)?,
        profile.spec().namespace,
    )
    .map_err(RetentionCause::Page)?
    .cell(0)
    .map_err(RetentionCause::Page)
}
fn head(home: StatusHome) -> Option<crate::storage::undo_ref::UndoRef> {
    match home.core() {
        TransactionStatusCell::Occupied(status) => status.undo_head(),
        TransactionStatusCell::Free(_) => None,
    }
}
/// The Job's table: its anchor/leaf images and the canonical Catalog schema
/// from the fixed CatalogLeaf (page 8). The record must be the Job's live
/// table handle with the Job's anchor; a retired table has no Row replay.
fn table_source<'a>(
    arena: &'a [u8],
    profile: Profile,
    job: job3::JobView<'_>,
) -> ObservationResult<TableSource<'a>> {
    let namespace = profile.spec().namespace;
    let handle = job.object().ok_or_else(invalid)?;
    let pair = job.pair().ok_or_else(invalid)?;
    let catalog = tree_page::decode_tree_page(
        arena_page(arena, 8),
        TreeExpectation {
            identity: PageIdentity {
                database: namespace.database,
                page: PageRef {
                    file: FileId::new(0x0000_0001_0000_0001).expect("first file"),
                    page_no: 8,
                    incarnation: Generation::new(1).expect("one"),
                },
                owner: ObjectId::new(5).expect("Catalog"),
                owner_generation: Generation::new(1).expect("one"),
            },
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )
    .map_err(RetentionCause::Tree)?;
    let found = catalog
        .lower_bound(&handle.object.get().to_be_bytes())
        .map_err(RetentionCause::Tree)?;
    if !found.equal {
        return Err(invalid());
    }
    let EntryPayload::Value(value) = catalog
        .entry(found.index)
        .map_err(RetentionCause::Tree)?
        .payload
    else {
        return Err(invalid());
    };
    let record = catalog_record::decode_catalog_record(
        value,
        CatalogExpectation {
            namespace,
            object: handle.object,
        },
    )
    .map_err(RetentionCause::Catalog)?;
    let header = record.header();
    let CatalogDefinitionView::Table(schema) = record.definition() else {
        return Err(RetentionCause::Unsupported);
    };
    if header.retired {
        return Err(RetentionCause::Unsupported);
    }
    if header.identity != handle || header.anchor != pair[0] {
        return Err(invalid());
    }
    Ok(TableSource {
        handle,
        anchor: pair[0],
        anchor_image: image(arena, pair[0])?,
        leaf_image: Some(image(arena, pair[1])?),
        schema: RowSchemaInput::Catalog(schema),
    })
}

/// The scope3 arm of `recovery::apply_frame`: Profile3 XHG/XSG version2
/// flags3 only (`semantic_row::decode_layout`). Logical arguments come from
/// the canonical arena/bank; the candidate selects among them and is then
/// compared byte for byte. All fallible work precedes the first write.
#[allow(clippy::too_many_arguments, reason = "disjoint original replay loans")]
pub(in super::super) fn apply(
    profile: Profile,
    expected: FrameExpectation,
    raw: &[u8],
    arena: &mut [u8],
    cells: &mut [[u8; JOB_BYTES]; JOB_COUNT],
    bank: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 8],
    scratch: &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
    adopted: &mut u64,
) -> ObservationResult<recovery::RetentionFrameObservation> {
    let pages: &mut [[u8; PAGE_BYTES]; 5] = (&mut after[..5]).try_into().expect("Row after prefix");
    let (view, observed, destination) = {
        let view = semantic::decode_layout(raw, expected, profile).map_err(sem)?;
        let op = view.operation();
        let jobs: &[[u8; JOB_BYTES]; JOB_COUNT] = cells;
        let actor = match view.owner(0) {
            Some(ReservationOwner::Transaction(actor)) => Some(actor),
            _ => None,
        };
        let terminal = |actor: Option<TxnRef>| {
            actor_job(
                jobs,
                profile,
                actor.ok_or_else(invalid)?,
                job3::JobState::TerminalLeased,
            )
        };
        let ordinal = match op {
            Operation::AgingStart => None,
            Operation::Begin => Some(first_free(arena, jobs, profile)?),
            Operation::Reserve => return Err(RetentionCause::Unsupported),
            Operation::Forward | Operation::Cancel => Some(actor_job(
                jobs,
                profile,
                actor.ok_or_else(invalid)?,
                job3::JobState::RootReserved,
            )?),
            Operation::RollbackStart
            | Operation::StatementFinish
            | Operation::Commit
            | Operation::AbortEnd => Some(terminal(actor)?),
            Operation::Clr
            | Operation::Prune
            | Operation::ReleaseRole
            | Operation::RootFree
            | Operation::TerminalFinish => Some(native_before(view, jobs)?),
        };
        let selected = match op {
            Operation::Begin => None,
            _ => ordinal.map(|i| job(jobs, profile, i)).transpose()?,
        };
        let location = match op {
            Operation::Begin => job3::decode_job(
                native_job(view)?.1.ok_or_else(invalid)?,
                profile.spec().namespace,
            )
            .map_err(RetentionCause::Retention)?
            .status(),
            Operation::AgingStart | Operation::ReleaseRole | Operation::RootFree => None,
            _ => Some(selected.and_then(|j| j.status()).ok_or_else(invalid)?),
        };
        let home = match op {
            Operation::Clr | Operation::RollbackStart => {
                Some(status_home(arena, profile, location.ok_or_else(invalid)?)?)
            }
            _ => None,
        };
        let core = view.core();
        let ordinal_of = || ordinal.ok_or_else(invalid);
        let change = match op {
            Operation::AgingStart => Change::AgingStart,
            Operation::Begin => Change::Begin {
                actor: actor.ok_or_else(invalid)?,
                status: location.ok_or_else(invalid)?,
                scope: JobScope::Row,
            },
            Operation::Reserve => return Err(RetentionCause::Unsupported),
            Operation::Forward => Change::Forward {
                ordinal: ordinal_of()?,
                statement: StatementBoundary {
                    sequence: core.statement(),
                    mark: None,
                },
            },
            Operation::Cancel => Change::Cancel {
                ordinal: ordinal_of()?,
            },
            Operation::RollbackStart => Change::RollbackStart {
                terminal: ordinal_of()?,
                statement: (core.statement() != 0).then(|| StatementBoundary {
                    sequence: core.statement(),
                    mark: core.target(),
                }),
            },
            Operation::Clr => Change::Clr {
                ordinal: ordinal_of()?,
                statement: match home.ok_or_else(invalid)?.core() {
                    TransactionStatusCell::Occupied(status) => match status.state() {
                        TransactionState::Aborting(RollbackState::Statement { target, .. }) => {
                            Some(StatementBoundary {
                                sequence: core.statement(),
                                mark: target,
                            })
                        }
                        _ => None,
                    },
                    TransactionStatusCell::Free(_) => None,
                },
            },
            Operation::StatementFinish => Change::StatementFinish {
                terminal: ordinal_of()?,
                statement: StatementBoundary {
                    sequence: core.statement(),
                    mark: core.target(),
                },
            },
            Operation::Commit => Change::Commit {
                terminal: ordinal_of()?,
            },
            Operation::AbortEnd => Change::AbortEnd {
                terminal: ordinal_of()?,
            },
            Operation::Prune => Change::Prune {
                ordinal: ordinal_of()?,
            },
            Operation::ReleaseRole => Change::ReleaseRole {
                ordinal: ordinal_of()?,
            },
            Operation::RootFree => Change::RootFree {
                ordinal: ordinal_of()?,
            },
            Operation::TerminalFinish => Change::TerminalFinish {
                ordinal: ordinal_of()?,
            },
        };
        let undo = match op {
            Operation::Cancel
            | Operation::Forward
            | Operation::Clr
            | Operation::Prune
            | Operation::ReleaseRole
            | Operation::RootFree => {
                Some(selected.and_then(|j| j.undo()).ok_or_else(invalid)?.page())
            }
            Operation::RollbackStart => home.and_then(head).map(|u| u.page()),
            _ => None,
        };
        let row = matches!(op, Operation::Forward | Operation::Clr | Operation::Prune);
        let previous = if row {
            core.next().map(|u| u.page())
        } else {
            None
        };
        let table = if row {
            Some(table_source(arena, profile, selected.ok_or_else(invalid)?)?)
        } else {
            None
        };
        let att = actor
            .map(|actor| row_actor(bank, actor))
            .transpose()?
            .flatten();
        let tx = op.handoff() || op.status_patch();
        let destination = if op == Operation::Begin {
            let actor = actor.ok_or_else(invalid)?;
            if actor.id().writer_epoch != expected.stream.writer_epoch
                // As in the Catalog arm: replay sees only adopted Begins, so
                // an issued-only local gap is valid; a reused local is not.
                || actor.id().local <= *adopted
                || att.is_some()
            {
                return Err(invalid());
            }
            Some(
                (0..control::ATT_SLOTS)
                    .find(|i| control::row(bank, *i).iter().all(|b| *b == 0))
                    .ok_or_else(invalid)?,
            )
        } else if tx {
            Some(att.ok_or_else(invalid)?)
        } else {
            None
        };
        let observed = transition::observe_borrowed(
            &Inputs {
                profile,
                frame: expected,
                change,
                jobs,
                root: arena_page(arena, 1),
                jobs_page: ordinal.map(|i| arena_page(arena, 2 + i / 15)),
                status: location.map(|v| image(arena, v.page())).transpose()?,
                undo: undo.map(|page| image(arena, page)).transpose()?,
                previous: previous.map(|page| image(arena, page)).transpose()?,
                table,
                transaction: if tx && op != Operation::Begin {
                    att.and_then(|i| raw_descriptor(control::row(bank, i)))
                } else {
                    None
                },
                aging: raw_descriptor(control::row(bank, AGING_SLOT)),
            },
            raw,
            pages,
            scratch,
        )
        .map_err(sem)?;
        (view, observed, destination)
    };
    let mut result = recovery::RetentionFrameObservation {
        header: observed.header,
        changed: [None; 8],
        initialized: [None; 2],
        retired: [None; 2],
    };
    for (at, role) in observed.order[..usize::from(observed.count)]
        .iter()
        .copied()
        .enumerate()
    {
        let request = recovery::request_from_image(profile, &pages[usize::from(role)])?;
        if request.identity.page.page_no >= 64
            || result.changed[..at].iter().flatten().any(|r| {
                r.identity.page.file == request.identity.page.file
                    && r.identity.page.page_no == request.identity.page.page_no
            })
        {
            return Err(invalid());
        }
        result.changed[at] = Some(request);
    }
    // `record` checks everything it needs before its first write; after it
    // succeeds only infallible page copies remain.
    record(
        bank,
        cells,
        view,
        &observed,
        observed.job_ordinal.map(|_| &pages[transition::JOB]),
        destination,
        profile,
    )?;
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
            .copy_from_slice(&pages[usize::from(role)]);
    }
    if observed.operation == Operation::Begin {
        *adopted = observed.actor.expect("checked Begin").id().local;
    }
    Ok(result)
}

/// The single body that applies an observed scope3 group's control part: the
/// transaction ATT row (XHG and the Tx-owned XSG), the Aging row (every op
/// with a Root) and the changed Jobs page's cells. Replay calls it on the
/// replay bank and cells; the live install calls it on its control loan, so
/// `history::validate_live` compares bytes produced by this one body. Every
/// fallible check precedes the first write: on error nothing changes.
pub(in super::super) fn record(
    bank: &mut [u8],
    jobs: &mut [[u8; JOB_BYTES]; JOB_COUNT],
    layout: LayoutView<'_>,
    observation: &Observation,
    jobs_after: Option<&[u8; PAGE_BYTES]>,
    slot: Option<usize>,
    profile: Profile,
) -> ObservationResult<()> {
    let op = layout.operation();
    let tx = op.handoff() || op.status_patch();
    if op != observation.operation
        || layout.header() != observation.header
        || bank.len() < 36 * SLOT_BYTES
        || tx != slot.is_some()
        || slot.is_some_and(|slot| slot >= control::ATT_SLOTS)
        || tx && layout.descriptor(0).is_none()
        || !op.status_patch() && layout.descriptor(usize::from(op.handoff())).is_none()
    {
        return Err(invalid());
    }
    let mut actor = [0; 48];
    let mut status = [0; 40];
    if tx {
        observation
            .actor
            .ok_or_else(invalid)?
            .encode(&mut actor)
            .map_err(RetentionCause::Page)?;
        observation
            .status
            .ok_or_else(invalid)?
            .encode(&mut status)
            .map_err(RetentionCause::Page)?;
    }
    let page = match (observation.job_ordinal, jobs_after) {
        (Some(ordinal), Some(after)) => Some((
            ordinal / 15 * 15,
            job3::decode_jobs_page(after, profile.spec().namespace, (ordinal / 15) as u8)
                .map_err(RetentionCause::Retention)?,
        )),
        (None, None) => None,
        _ => return Err(invalid()),
    };
    // The slot's own precondition, one guard for replay and the live
    // install: Begin takes an all-zero row; every other Tx op finds this
    // actor's STABLE row at this Status, with the unused tail clear. A wrong
    // slot is refused here instead of overwriting another actor's row.
    if let Some(slot) = slot {
        let row = control::row(bank, slot);
        let own = if op == Operation::Begin {
            row.iter().all(|byte| *byte == 0)
        } else {
            row[PHASE] == STABLE
                && row[ACTOR] == actor
                && row[LOCATION] == status
                && row[PHASE + 1..].iter().all(|byte| *byte == 0)
        };
        if !own {
            return Err(invalid());
        }
    }
    // Every fallible check is above.
    if let Some(slot) = slot {
        let row = control::row_mut(bank, slot);
        if matches!(op, Operation::Commit | Operation::AbortEnd) {
            row.fill(0);
        } else {
            row[..1536].copy_from_slice(layout.descriptor(0).expect("checked descriptor"));
            row[ACTOR].copy_from_slice(&actor);
            row[LOCATION].copy_from_slice(&status);
            row[PHASE] = STABLE;
        }
    }
    if !op.status_patch() {
        let row = control::row_mut(bank, AGING_SLOT);
        row[..1536].copy_from_slice(
            layout
                .descriptor(usize::from(op.handoff()))
                .expect("checked Aging"),
        );
        row[PHASE] = STABLE;
    }
    if let Some((base, view)) = page {
        for i in 0..view.len() {
            jobs[base + i].copy_from_slice(view.cell(i).expect("checked cell"));
        }
    }
    Ok(())
}

/// Row part of the combined closure. Every Row Terminal names an allocated
/// Status whose current occupant is its actor. A TerminalLeased actor has
/// exactly one stable ATT row whose descriptor the Row ledger accepts against
/// the whole Job bank; a TerminalReady actor has none and closes over the
/// bank without a descriptor. The Catalog part's census already covered
/// every Row cell; the caller checks the ATT union.
pub(in super::super) fn validate_rows(
    profile: Profile,
    replay: &[u8],
    matched: &mut u32,
) -> ObservationResult<()> {
    let namespace = profile.spec().namespace;
    let cells: &[[u8; JOB_BYTES]; JOB_COUNT] = replay[ARENA_END..JOB_END]
        .as_chunks::<JOB_BYTES>()
        .0
        .try_into()
        .expect("jobs");
    let bank = &replay[JOB_END..BANK_END];
    let allocation = allocation_page::decode_allocation_page(
        arena_page(replay, 0),
        recovery::allocator_identity(profile)?,
        namespace,
    )
    .map_err(RetentionCause::Allocation)?;
    for ordinal in 0..JOB_COUNT {
        let terminal = job(cells, profile, ordinal)?;
        if terminal.scope() != Some(JobScope::Row) || !terminal.state().is_terminal() {
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
        let current = status_home(replay, profile, location)?;
        // The Status occupant is this Terminal's actor, as on the Catalog
        // side; the census pair rule and the ledger are not relied on here.
        if !matches!(current.core(), TransactionStatusCell::Occupied(status)
            if status.reference() == actor)
        {
            return Err(invalid());
        }
        let att = row_actor(bank, actor)?;
        if terminal.state() == job3::JobState::TerminalLeased {
            let slot = att.ok_or_else(invalid)?;
            if *matched & (1 << slot) != 0 {
                return Err(invalid());
            }
            *matched |= 1 << slot;
            semantic::ledger::validate_transaction(
                raw_descriptor(control::row(bank, slot)).ok_or_else(invalid)?,
                profile,
                current,
                location,
                JobCells::original(cells),
            )
            .map_err(sem)?;
            if StatusSlotRef::decode(&control::row(bank, slot)[LOCATION])
                .map_err(RetentionCause::Page)?
                != location
            {
                return Err(invalid());
            }
        } else {
            if att.is_some() {
                return Err(invalid());
            }
            semantic::ledger::validate_actor(profile, current, location, JobCells::original(cells))
                .map_err(sem)?;
        }
    }
    Ok(())
}

/// The interim analysis gate's observation (XADB-MODULE-ROW-BEGIN-001): a
/// non-Free Row-scope Job, or an occupied ATT row whose descriptor carries
/// the Row peak recipe (registry 16). Before C4 supplies the Row recovery
/// handler such a state has no READY. Corrupt cells and descriptors are
/// errors, never "no Row state".
pub(in super::super) fn scope_present(
    profile: Profile,
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    bank: &[u8],
) -> ObservationResult<bool> {
    let namespace = profile.spec().namespace;
    for cell in cells {
        if job3::decode_job(cell, namespace)
            .map_err(RetentionCause::Retention)?
            .scope()
            == Some(JobScope::Row)
        {
            return Ok(true);
        }
    }
    for slot in 0..control::ATT_SLOTS {
        let row = control::row(bank, slot);
        let Some(raw) = raw_descriptor(row) else {
            continue;
        };
        let actor = TxnRef::decode(&row[ACTOR]).map_err(RetentionCause::Page)?;
        let descriptor = obligation::decode_descriptor(
            raw,
            ReservationOwner::Transaction(actor),
            retention_obligation::profile_reference(profile),
        )
        .map_err(|_| invalid())?;
        if descriptor
            .remaining
            .iter()
            .any(|row| row.peak == job3::ROW_PEAK)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Caller-owned source carriers around the pure Row observer, as
/// `catalog::SourceCensus`. Backing slices remain the original replay loan.
#[derive(Clone, Copy)]
pub(in super::super) struct SourceCensus {
    pub(in super::super) semantic: usize,
    pub(in super::super) finishing: usize,
    pub(in super::super) closure: usize,
}
pub(in super::super) fn source_bytes() -> SourceCensus {
    use crate::storage::obligation::DescriptorSpec;
    use std::mem::size_of;
    let job = job3::source_bytes();
    let carriers =
        24 * size_of::<&[u8]>() + 24 * size_of::<u64>() + 4 * size_of::<ObservationResult<()>>();
    let outer = 2 * size_of::<LayoutView<'_>>()
        + size_of::<Inputs<'_>>()
        + size_of::<Change<'_>>()
        + size_of::<StatementBoundary>()
        + 4 * size_of::<Option<job3::JobView<'_>>>()
        + 4 * size_of::<Option<TxnRef>>()
        + 4 * size_of::<Option<StatusSlotRef>>()
        + 2 * size_of::<Option<StatusHome>>()
        + 4 * size_of::<Option<PageRef>>()
        + 6 * size_of::<Option<usize>>()
        + size_of::<Option<TableSource<'_>>>()
        + size_of::<semantic::CoreView<'_>>()
        + carriers;
    // Selection and lookups complete before the observer: Job/Root decode
    // (Row codec), the native scan, the Status home decode and the table
    // lookup (CatalogLeaf decode/search and the XCT1 record), each with its
    // returned carrier. Their results are already inside Inputs.
    let lookup = job
        .max(size_of::<NativeJob<'_>>() + 2 * size_of::<ObservationResult<NativeJob<'_>>>())
        .max(
            2 * size_of::<status_home::StatusHomePage<'_>>()
                + 2 * size_of::<ObservationResult<StatusHome>>()
                + semantic_catalog_status(),
        )
        .max(
            size_of::<tree_page::TreePageView<'_>>()
                + size_of::<tree_page::TreeEntry<'_>>()
                + size_of::<catalog_record::CatalogRecordView<'_>>()
                + size_of::<catalog_record::CatalogHeader>()
                + 2 * size_of::<ObservationResult<TableSource<'_>>>()
                + transition::tree_source_bytes()
                    .max(crate::storage::semantic_catalog::catalog_read_codec_bytes()),
        );
    let semantic = outer
        + size_of::<&mut [u8; ROW_REPLAY_SCRATCH_BYTES]>()
        + transition::observe_borrowed_source_bytes().max(lookup);
    let hashing = 4 * size_of::<foundation::codec::ContentHasher>() + 1536;
    let finishing = size_of::<LayoutView<'_>>()
        + size_of::<Option<usize>>()
        + carriers
        + 2 * size_of::<Observation>()
        + 2 * size_of::<std::result::Result<Observation, semantic::Error>>()
        + 2 * size_of::<recovery::RetentionFrameObservation>()
        + 2 * size_of::<ObservationResult<recovery::RetentionFrameObservation>>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<ObservationResult<TargetRequest>>()
        // record: actor/status encodings and the decoded after Jobs page.
        + size_of::<([u8; 48], [u8; 40])>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<crate::storage::page::PageHeader>()
        + job.max(hashing);
    let closure_outer = 2 * size_of::<job3::JobView<'_>>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<status_home::StatusHomePage<'_>>()
        + 4 * size_of::<StatusHome>()
        + size_of::<DescriptorSpec>()
        + carriers;
    // Gate: the same Row codec decode, or one descriptor decode.
    let closure = closure_outer
        + job
            .max(semantic_catalog_status())
            .max(transition::ledger_source_bytes())
            .max(obligation::descriptor_source_bytes() + size_of::<DescriptorSpec>());
    SourceCensus {
        semantic,
        finishing,
        closure,
    }
}
fn semantic_catalog_status() -> usize {
    crate::storage::semantic_catalog::status_read_codec_bytes()
}
