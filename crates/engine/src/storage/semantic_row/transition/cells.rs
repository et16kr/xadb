//! Small exact recipes shared by every Row transition: identities, before
//! page checks, Undo material, projection, predecessor and budget rows.
use super::super::{Error, Result};
use crate::{
    api::Namespace,
    storage::{
        obligation::{self, RemainingBudget},
        page::{self, FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef, PageView},
        retention_genesis::{self, RetentionGenesisPage},
        retention_obligation::AgingObligationError,
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self, JobCells, JobScope, JobState, JobView, RootView},
        },
        status_home::StatusHome,
        transaction_record::{StatusSlotRef, TxnRef},
        tree_page::edit::LeafEditError,
        undo_record::{
            self, UndoBeforeKind, UndoCell, UndoCreator, UndoEdge, UndoLink, UndoPayload,
            UndoRecord, UndoRecordExpectation, UndoRelease, UndoTargetKind, UndoTargetView,
        },
        wal_frame::FrameExpectation,
    },
};
use foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectHandle, ObjectId, RowId},
};

pub(super) const FIRST_FILE: u64 = 0x1_0000_0001;
pub(super) const TERMINAL_BYTES: u64 = 55_112;
pub(super) const RELEASE_BYTES: u64 = 55_528;

pub(super) fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::OrderMismatch)
    }
}
pub(super) fn same(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::IdentityMismatch)
    }
}
pub(super) fn one() -> Generation {
    Generation::new(1).expect("one")
}

pub(super) fn aging_error(error: AgingObligationError) -> Error {
    match error {
        AgingObligationError::Retention(error) => Error::Queue(error),
        AgingObligationError::Descriptor(error) => Error::Descriptor(error),
        AgingObligationError::IdentityMismatch => Error::IdentityMismatch,
        AgingObligationError::OrderMismatch => Error::OrderMismatch,
        AgingObligationError::BudgetMismatch => Error::BudgetMismatch,
    }
}
pub(super) fn edit_error(error: LeafEditError) -> Error {
    match error {
        LeafEditError::Tree(error) => Error::Tree(error),
        LeafEditError::KeyExists | LeafEditError::KeyMissing => Error::IdentityMismatch,
        LeafEditError::ScratchTooSmall => Error::Exhausted,
    }
}

pub(super) fn dynamic(page: PageRef) -> bool {
    page.file == FileId::new(FIRST_FILE).expect("first file") && (9..64).contains(&page.page_no)
}
pub(super) fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}
pub(super) fn fixed_identity(namespace: Namespace, page: PageRef, owner: u64) -> PageIdentity {
    PageIdentity {
        database: namespace.database,
        page,
        owner: ObjectId::new(owner).expect("fixed owner"),
        owner_generation: one(),
    }
}
pub(super) fn genesis_identity(
    namespace: Namespace,
    page: RetentionGenesisPage,
) -> Result<PageIdentity> {
    Ok(retention_genesis::expected_header(namespace, page)
        .map_err(|_| Error::IdentityMismatch)?
        .identity)
}
pub(super) fn table_identity(
    table: ObjectHandle,
    namespace: Namespace,
    page: PageRef,
) -> PageIdentity {
    PageIdentity {
        database: namespace.database,
        page,
        owner: table.object,
        owner_generation: table.generation,
    }
}

/// A stable before image of one changed or proof page. Except for the Root
/// and Jobs genesis images, a before page is never the LSN0 image.
pub(super) fn before_page<'a>(
    bytes: &'a [u8; PAGE_BYTES],
    identity: PageIdentity,
    kind: PageKind,
    frame: FrameExpectation,
    genesis: bool,
) -> Result<PageView<'a>> {
    let view = page::decode_page(bytes, identity, kind)?;
    require(
        view.header().level == 0
            && (genesis || view.header().lsn.0 != 0)
            && view.header().lsn.0 <= frame.previous_lsn,
    )?;
    Ok(view)
}

/// One-home Status page before image of `location`.
pub(super) fn status_before(
    bytes: &[u8; PAGE_BYTES],
    namespace: Namespace,
    location: StatusSlotRef,
    frame: FrameExpectation,
) -> Result<StatusHome> {
    let identity = fixed_identity(namespace, location.page(), 2);
    let view = before_page(bytes, identity, PageKind::TransactionStatus, frame, false)?;
    same(view.len() == 1 && location.slot() == 0)?;
    Ok(StatusHome::decode(view.cell(0).ok_or(Error::Corrupt)?)?)
}

/// One-slot Undo page before image at the exact `INITIAL_UNDO_CELL`.
pub(super) fn undo_before<'a>(
    bytes: &'a [u8; PAGE_BYTES],
    namespace: Namespace,
    page: PageRef,
    frame: FrameExpectation,
) -> Result<&'a [u8]> {
    same(dynamic(page))?;
    let view = before_page(
        bytes,
        fixed_identity(namespace, page, 3),
        PageKind::Undo,
        frame,
        false,
    )?;
    same(view.len() == 1 && view.cell_range(0) == Some(undo_record::INITIAL_UNDO_CELL))?;
    Ok(&bytes[undo_record::INITIAL_UNDO_CELL])
}
pub(super) fn page_lsn(bytes: &[u8; PAGE_BYTES]) -> u64 {
    u64::from_le_bytes(bytes[64..72].try_into().expect("page LSN"))
}

pub(super) fn validate_actor(
    namespace: Namespace,
    actor: TxnRef,
    location: StatusSlotRef,
) -> Result<()> {
    same(
        actor.id().database == namespace.database
            && actor.id().timeline == namespace.timeline
            && actor.generation() == location.generation()
            && actor.generation().get() >= 2
            && location.slot() == 0
            && dynamic(location.page()),
    )
}

/// The actor's Row jobs. One actor never owns Jobs of both scopes.
pub(super) struct ActorJobs {
    pub(super) terminal: Option<usize>,
    pub(super) reserved: Option<usize>,
    pub(super) retained: usize,
    pub(super) roots: usize,
    pub(super) statement: u64,
}
pub(super) fn actor_jobs(
    jobs: &[[u8; JOB_BYTES]; JOB_COUNT],
    namespace: Namespace,
    actor: TxnRef,
) -> Result<ActorJobs> {
    let mut summary = ActorJobs {
        terminal: None,
        reserved: None,
        retained: 0,
        roots: 0,
        statement: 0,
    };
    for (ordinal, cell) in jobs.iter().enumerate() {
        let job = row::decode_job(cell, namespace)?;
        let Some(source) = job.source() else {
            continue;
        };
        if source.id() != actor.id() {
            continue;
        }
        same(source == actor)?;
        if job.scope() != Some(JobScope::Row) {
            return Err(Error::Unsupported);
        }
        if job.state().is_terminal() {
            same(summary.terminal.replace(ordinal).is_none())?;
            continue;
        }
        summary.roots += 1;
        summary.statement = summary.statement.max(job.statement());
        match job.state() {
            JobState::RootReserved => same(summary.reserved.replace(ordinal).is_none())?,
            JobState::RootRetained => summary.retained += 1,
            _ => {}
        }
    }
    Ok(summary)
}

/// Begin/Reserve lease the first Free cell from the Root cursor.
pub(super) fn first_free(
    root: RootView<'_>,
    jobs: &[[u8; JOB_BYTES]; JOB_COUNT],
    namespace: Namespace,
) -> Result<usize> {
    for offset in 0..JOB_COUNT {
        let ordinal = (root.cursor() as usize + offset) % JOB_COUNT;
        if row::decode_job(&jobs[ordinal], namespace)?.state() == JobState::Free {
            return Ok(ordinal);
        }
    }
    Err(Error::BudgetMismatch)
}

/// Nearest older Retained root of the same actor (the census H).
pub(super) fn predecessor<'a>(
    jobs: JobCells<'a>,
    namespace: Namespace,
    actor: TxnRef,
    sequence: u64,
) -> Result<Option<JobView<'a>>> {
    let mut previous: Option<JobView<'a>> = None;
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(jobs.cell(ordinal), namespace)?;
        if job.source() == Some(actor)
            && job.state() == JobState::RootRetained
            && job.undo_sequence() < sequence
            && previous.is_none_or(|old| old.undo_sequence() < job.undo_sequence())
        {
            previous = Some(job);
        }
    }
    Ok(previous)
}
pub(super) fn link(job: JobView<'_>) -> Result<UndoLink> {
    Ok(UndoLink {
        reference: job.undo().ok_or(Error::IdentityMismatch)?,
        sequence: job.undo_sequence(),
    })
}

pub(super) fn row_target(table: ObjectHandle, row: RowId) -> UndoTargetView<'static> {
    UndoTargetView {
        kind: UndoTargetKind::Row,
        owner: table.object,
        generation: table.generation,
        entity: row.get(),
        arity: 0,
        key: &[],
    }
}

/// XRP1 projection of one Row presence (consumers.md row logical projection).
pub(super) fn projection(
    table: ObjectHandle,
    row: RowId,
    value: Option<&[u8]>,
) -> Result<[u8; 32]> {
    let mut prefix = [0; 48];
    prefix[..8].copy_from_slice(b"XRP1\x01\0\x01\0");
    let target = row_target(table, row);
    undo_record::encode_undo_target(target, target, &mut prefix[8..40])?;
    prefix[40] = u8::from(value.is_some());
    let mut hash = ContentHasher::new();
    hash.update(&prefix);
    if let Some(value) = value {
        hash.update(value);
    }
    Ok(hash.digest())
}

/// Missing-only U material of a Row root Job (Live, Releasing or released).
pub(super) fn material(
    job: JobView<'_>,
    transaction: UndoEdge,
    incoming: u64,
    cursor: u8,
) -> Result<UndoRecord<'static>> {
    let actor = job.source().ok_or(Error::IdentityMismatch)?;
    let object = job.object().ok_or(Error::IdentityMismatch)?;
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    Ok(UndoRecord {
        creator: UndoCreator {
            id: actor.id(),
            status_generation: actor.generation(),
        },
        sequence: job.undo_sequence(),
        mutation_sequence: job.mutation(),
        incoming,
        target: row_target(object, row),
        edges: [
            transaction,
            if cursor >= 1 {
                UndoEdge::Released
            } else {
                UndoEdge::Never
            },
        ],
        before_kind: UndoBeforeKind::Missing,
        before_stamp: None,
        before_logical: None,
        payload: UndoPayload::None,
        release: UndoRelease {
            cursor,
            chunks_released: 0,
            next: None,
        },
    })
}
pub(super) fn encode_material(
    namespace: Namespace,
    job: JobView<'_>,
    cell: UndoCell<'_>,
    output: &mut [u8; undo_record::UNDO_RECORD_BYTES],
) -> Result<()> {
    let reference = job.undo().ok_or(Error::IdentityMismatch)?;
    let object = job.object().ok_or(Error::IdentityMismatch)?;
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    let expected = UndoRecordExpectation {
        namespace,
        reference,
        target: row_target(object, row),
    };
    undo_record::encode_undo_record(cell, expected, &[], output)?;
    undo_record::decode_undo_record(output, expected, &[])?;
    Ok(())
}
/// Exact equality of an actual U cell with its canonical material.
pub(super) fn check_material(
    namespace: Namespace,
    job: JobView<'_>,
    cell: UndoCell<'_>,
    actual: &[u8],
) -> Result<()> {
    let mut canonical = [0; undo_record::UNDO_RECORD_BYTES];
    encode_material(namespace, job, cell, &mut canonical)?;
    same(actual == canonical)
}

/// The normalized UndoSlot Item96 of a Reserved root.
pub(super) fn undo_item(job: JobView<'_>) -> Result<[u8; 96]> {
    let mut item = [0; 96];
    item[0] = 2; // UndoSlot
    item[1] = 2; // Structural
    job.undo()
        .ok_or(Error::IdentityMismatch)?
        .encode(&mut item[8..48])?;
    put(&mut item, 64, 1);
    put(&mut item, 72, job.lease_lsn());
    Ok(item)
}
pub(super) fn item_claim(item: &[u8; 96], lsn: u64) -> obligation::InventoryClaim {
    obligation::InventoryClaim {
        change_lsn: lsn,
        items: 1,
        digest: content_digest(item),
        growth_extents: 0,
    }
}
pub(super) fn empty_claim(lsn: u64) -> obligation::InventoryClaim {
    obligation::InventoryClaim {
        change_lsn: lsn,
        items: 0,
        digest: content_digest(&[]),
        growth_extents: 0,
    }
}

/// Row cleanup recipe in budget row 3 (Terminal, Root or Releasing).
pub(super) fn set_budget(job: &mut [u8; JOB_BYTES], steps: u64, wal: u64) {
    let mut rows = [RemainingBudget::default(); 5];
    rows[3] = RemainingBudget {
        steps,
        wal,
        protected: wal,
        peak: row::ROW_PEAK,
        ..RemainingBudget::default()
    };
    obligation::encode_budget_rows(
        &rows,
        (&mut job[384..1024]).try_into().expect("budget rows"),
    );
}

/// Free-generation lease of a Job cell (Begin kind1, Reserve kind3).
pub(super) fn lease(
    job: &mut [u8; JOB_BYTES],
    before: JobView<'_>,
    actor: TxnRef,
    location: StatusSlotRef,
    lsn: u64,
    kind: u8,
) -> Result<()> {
    require(before.state() == JobState::Free)?;
    let generation = before.generation().next().map_err(|_| Error::Exhausted)?;
    row::free_job(generation, job);
    job[6] = 1;
    job[7] = kind;
    actor.encode(&mut job[16..64])?;
    location.encode(&mut job[80..120])?;
    put(job, 64, lsn);
    put(job, 376, lsn);
    Ok(())
}

pub(super) fn encode_target_undo(link: UndoLink, output: &mut [u8]) -> Result<()> {
    output[0] = 1;
    link.reference.encode(&mut output[8..48])?;
    put(output, 48, link.sequence);
    Ok(())
}
pub(super) fn encode_target_txn(actor: TxnRef, output: &mut [u8]) -> Result<()> {
    output[0] = 2;
    actor.encode(&mut output[8..56])?;
    Ok(())
}
pub(super) fn undo_generation(cell: &[u8]) -> Result<Generation> {
    Generation::new(u64::from_le_bytes(
        cell[8..16].try_into().expect("generation"),
    ))
    .map_err(|_| Error::Corrupt)
}
