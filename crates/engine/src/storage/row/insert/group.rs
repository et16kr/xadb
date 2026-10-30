//! Complete scope3 Row INSERT Forward and its CLR over plain observations.
//! C0 only: every input is a borrowed copy of an owner observation and grants
//! no reservation, pin, stage, install or recovery authority. The owner must
//! recheck the whole candidate. Every refusal over the inputs returns before
//! the first frame/after byte is written; see `produce`.

mod frame;
mod images;

use super::{InsertError, Result, leaf};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::ReservationOwner,
        anchor::{self, TreeKind},
        control::Profile,
        disk::{RowPageRole, RowSourceRole},
        obligation::{self, DescriptorSpec, InventoryClaim, OwnerState, RemainingBudget},
        page::{self, PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef, PageView},
        retention_genesis::{self, RetentionGenesisPage},
        retention_obligation::{profile_reference, row as aging_row},
        retention_page::{
            JOB_BYTES, JOB_COUNT, ROOT_BYTES,
            row::{self, JobScope, JobState, JobView},
        },
        row_record::RowSchemaInput,
        semantic_row::Operation,
        status_home::{STATUS_HOME_BYTES, StatusHome},
        transaction_record::{
            RollbackState, StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef,
        },
        tree_page::TreeExpectation,
        undo_record::{
            self, INITIAL_UNDO_CELL, UndoBeforeKind, UndoCell, UndoCreator, UndoEdge, UndoLink,
            UndoPayload, UndoRecord, UndoRecordExpectation, UndoRecordState, UndoRelease,
            UndoTargetKind, UndoTargetView,
        },
        undo_ref::UndoRef,
        wal_frame::{FrameExpectation, FrameKind, WalFrameHeader},
    },
};
use foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectHandle, ObjectId, RowId},
};

pub(super) const FRAME_BYTES: usize = 98_304;
/// The lane's A scratch part (`RowProducerLane::parts`, eighth element).
pub(super) const SCRATCH_BYTES: usize = 32_768;
/// RowSourceRole order of `GroupInput::sources`; the lane's own index is private.
pub(super) const SOURCE_ROLES: [RowSourceRole; 9] = [
    RowSourceRole::Root,
    RowSourceRole::RootJob,
    RowSourceRole::TerminalJob,
    RowSourceRole::Status,
    RowSourceRole::Undo,
    RowSourceRole::RowLeaf,
    RowSourceRole::Allocator,
    RowSourceRole::TableAnchor,
    RowSourceRole::PreviousUndo,
];
const ROOT_CELL: (u16, u16, u16) = (0, 15_840, 512);
const STATUS_CELL: (u16, u16, u16) = (0, 16_128, 224);
const UNDO_CELL: (u16, u16, u16) = (0, 15_920, 432);
const ANCHOR_CELL: (u16, u16, u16) = (0, 16_304, 48);
const FORWARD_FIXED: u64 = 91_304;
const CLR_FIXED: u64 = 91_096;

/// Plain value mirror of one `RowSourceBinding`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Source {
    pub(super) index: usize,
    pub(super) identity: PageIdentity,
    pub(super) kind: PageKind,
    pub(super) level: u8,
    pub(super) cell: Option<(u16, u16, u16)>,
}

/// The reserve-sealed INSERT value, or the CLR's actual current Row/RootJob
/// observation. `cell_bytes` is the RootJob tree-cell length C (`180+L`).
#[derive(Clone, Copy, Debug)]
pub(super) enum Change<'a> {
    Forward {
        canonical: &'a [u8],
    },
    Clr {
        cell_bytes: usize,
        projection: [u8; 32],
        target: Option<UndoRef>,
        next: Option<UndoLink>,
    },
}

/// Plain inputs mirroring the producer lane getters. The fields are copies of
/// owner observations; constructing this value creates no lane or authority.
pub(super) struct GroupInput<'a> {
    pub(super) profile: Profile,
    pub(super) table: ObjectHandle,
    pub(super) schema: RowSchemaInput<'a>,
    pub(super) leaf: TreeExpectation,
    pub(super) expected: FrameExpectation,
    pub(super) header: WalFrameHeader,
    pub(super) actor: TxnRef,
    pub(super) status: StatusSlotRef,
    pub(super) row: RowId,
    pub(super) undo: UndoLink,
    pub(super) statement: Option<u64>,
    pub(super) mutation: u64,
    pub(super) mark: Option<UndoRef>,
    pub(super) predecessor: Option<UndoLink>,
    pub(super) root_job: usize,
    pub(super) terminal_job: usize,
    pub(super) sources: [Option<Source>; 9],
    pub(super) before: &'a [[u8; PAGE_BYTES]; 9],
    pub(super) before_count: usize,
    pub(super) after_order: &'a [RowPageRole],
    pub(super) jobs: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    pub(super) transaction: Option<&'a [u8; 1536]>,
    pub(super) aging: Option<&'a [u8; 1536]>,
    pub(super) change: Change<'a>,
}

/// Declared candidate metadata; the owner revalidates bytes, order and count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Produced {
    pub(super) frame_len: usize,
    pub(super) after_count: usize,
    pub(super) roles: [RowPageRole; 5],
}

/// One changed physical page before image in physical order.
#[derive(Clone, Copy)]
struct Changed<'a> {
    role: RowPageRole,
    before: &'a [u8; PAGE_BYTES],
    identity: PageIdentity,
    kind: PageKind,
    lsn: u64,
}

/// M2: the producer's byte buffers, carved from the lane's A scratch part
/// instead of the stack (S2-0 Row write amendment §3 and M2). Bytes only:
/// every Rust reference to them stays on A's stack, and C never reads them.
/// `carve` zeroes the regions, as the stack arrays they replace were, so
/// nothing depends on what the scratch held before the lane was issued.
struct Scratch<'s> {
    /// `observe`'s Forward RowLeaf check (record and cell).
    leaf: &'s mut [u8; leaf::LEAF_SCRATCH_BYTES],
    /// Job census aggregate of `observe`, then of `images::root` and
    /// `images::root_page`, one after another.
    aggregate: &'s mut [u8; 640],
    /// `observe`'s expected CLR U before cell.
    check: &'s mut [u8; undo_record::UNDO_RECORD_BYTES],
    job: &'s mut [u8; JOB_BYTES],
    root: &'s mut [u8; ROOT_BYTES],
    undo: &'s mut [u8; undo_record::UNDO_RECORD_BYTES],
    status: &'s mut [u8; STATUS_HOME_BYTES],
    descriptors: &'s mut [u8; images::DESCRIPTORS_BYTES],
}

/// Bytes the regions of `Scratch` take, from offset0 in this order.
const SCRATCH_USED: usize = leaf::LEAF_SCRATCH_BYTES
    + 640
    + undo_record::UNDO_RECORD_BYTES
    + JOB_BYTES
    + ROOT_BYTES
    + undo_record::UNDO_RECORD_BYTES
    + STATUS_HOME_BYTES
    + images::DESCRIPTORS_BYTES;
const _: () = assert!(SCRATCH_USED <= SCRATCH_BYTES);

/// The next `N` bytes of `rest` as an array.
fn take<'s, const N: usize>(rest: &mut &'s mut [u8]) -> &'s mut [u8; N] {
    let (head, tail) = std::mem::take(rest)
        .split_first_chunk_mut::<N>()
        .expect("SCRATCH_USED fits SCRATCH_BYTES");
    *rest = tail;
    head
}

impl<'s> Scratch<'s> {
    fn carve(bytes: &'s mut [u8; SCRATCH_BYTES]) -> Self {
        let mut rest: &mut [u8] = &mut bytes[..SCRATCH_USED];
        rest.fill(0);
        Self {
            leaf: take(&mut rest),
            aggregate: take(&mut rest),
            check: take(&mut rest),
            job: take(&mut rest),
            root: take(&mut rest),
            undo: take(&mut rest),
            status: take(&mut rest),
            descriptors: take(&mut rest),
        }
    }
}

/// Facts checked from the inputs before any output byte is written.
struct Observed<'a> {
    forward: bool,
    namespace: Namespace,
    profile: Profile,
    header: WalFrameHeader,
    actor: TxnRef,
    table: ObjectHandle,
    row: RowId,
    leaf: TreeExpectation,
    undo: UndoLink,
    predecessor: Option<UndoLink>,
    target: Option<UndoRef>,
    ordinal: usize,
    job: JobView<'a>,
    cell: usize,
    frame_len: usize,
    status: StatusHome,
    transaction: DescriptorSpec,
    transaction_bytes: &'a [u8; 1536],
    aging: &'a [u8; 1536],
    jobs: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    pages: [Changed<'a>; 5],
}

/// Produce the whole Forward/CLR candidate. `observe` and `images::prepare`
/// run every check over the inputs before the first output byte: header and
/// page LSNs, the leaf insert/remove plan with duplicate RowId and capacity,
/// the Forward record projection, the Root after cell, the exact Tx residual
/// and the Aging before. A refusal therefore leaves frame/after untouched.
/// The write pass only places prepared values and re-encodes checked images:
/// first the Root after page, encoded straight into its `after` slot, and the
/// Aging after bound to it (M1: no Root page copy is kept), then the other
/// four pages and the frame. An error there is a broken internal invariant
/// and leaves frame/after unspecified.
pub(super) fn produce(
    input: &GroupInput<'_>,
    frame: &mut [u8; FRAME_BYTES],
    after: &mut [[u8; PAGE_BYTES]; 5],
    scratch: &mut [u8; SCRATCH_BYTES],
) -> Result<Produced> {
    let scratch = Scratch::carve(scratch);
    let observed = observe(input, scratch.leaf, scratch.aggregate, scratch.check)?;
    images::job(&observed, scratch.job)?;
    let mut prepared = images::Prepared {
        job: scratch.job,
        status: images::status(&observed)?,
        root: scratch.root,
        undo: scratch.undo,
        descriptors: scratch.descriptors,
        status_cell: scratch.status,
        aggregate: scratch.aggregate,
    };
    images::prepare(&observed, &mut prepared)?;

    // Write pass: every input refusal has already returned above.
    images::root_page(&observed, &mut prepared, after)?;
    images::pages(&observed, input, &mut prepared, frame, after)?;
    let output = &mut frame[..observed.frame_len];
    output.fill(0);
    output[416..416 + images::DESCRIPTORS_BYTES].copy_from_slice(&prepared.descriptors[..]);
    frame::write(&observed, after, output)?;
    let mut roles = [RowPageRole::Root; 5];
    for (role, page) in roles.iter_mut().zip(observed.pages) {
        *role = page.role;
    }
    Ok(Produced {
        frame_len: observed.frame_len,
        after_count: 5,
        roles,
    })
}

fn require(condition: bool, error: InsertError) -> Result<()> {
    if condition { Ok(()) } else { Err(error) }
}

fn source_index(role: RowSourceRole) -> usize {
    SOURCE_ROLES
        .iter()
        .position(|candidate| *candidate == role)
        .expect("closed role list")
}

/// Select one before image by role and its exact expected page identity.
/// `exact` cells must be supplied; otherwise an absent cell is also accepted.
fn bind<'a>(
    input: &GroupInput<'a>,
    role: RowSourceRole,
    identity: PageIdentity,
    kind: PageKind,
    cell: Option<(u16, u16, u16)>,
    exact: bool,
) -> Result<&'a [u8; PAGE_BYTES]> {
    let source = input.sources[source_index(role)].ok_or(InsertError::InvalidInput)?;
    require(
        source.index < input.before_count
            && source.identity == identity
            && source.kind == kind
            && source.level == 0
            && (source.cell == cell || !exact && source.cell.is_none()),
        InsertError::IdentityMismatch,
    )?;
    Ok(&input.before[source.index])
}

fn checked_page<'a>(
    bytes: &'a [u8; PAGE_BYTES],
    identity: PageIdentity,
    kind: PageKind,
    header: WalFrameHeader,
) -> Result<PageView<'a>> {
    let view = page::decode_page(bytes, identity, kind)?;
    // Like the Status/Undo/Leaf observers, a changed or proof page before is
    // never the LSN0 genesis image.
    require(
        view.header().level == 0
            && view.header().lsn.0 != 0
            && view.header().lsn.0 <= header.previous_lsn
            && view.header().lsn.0 < header.lsn,
        InsertError::OrderMismatch,
    )?;
    Ok(view)
}

fn fixed_identity(namespace: Namespace, page: PageRef, owner: u64) -> PageIdentity {
    PageIdentity {
        database: namespace.database,
        page,
        owner: ObjectId::new(owner).expect("fixed owner"),
        owner_generation: Generation::new(1).expect("one"),
    }
}

fn queue_identity(namespace: Namespace, page: RetentionGenesisPage) -> Result<PageIdentity> {
    Ok(retention_genesis::expected_header(namespace, page)
        .map_err(|_| InsertError::IdentityMismatch)?
        .identity)
}

fn job_cell(ordinal: usize) -> (u16, u16, u16) {
    let slot = ordinal % 15;
    (slot as u16, (16_352 - 1024 * (slot + 1)) as u16, 1024)
}

/// Bind the Jobs page that holds `ordinal` and require it to match the bank.
fn bind_jobs<'a>(
    input: &GroupInput<'a>,
    role: RowSourceRole,
    ordinal: usize,
    exact: bool,
    namespace: Namespace,
) -> Result<(&'a [u8; PAGE_BYTES], PageIdentity)> {
    let index = (ordinal / 15) as u8;
    let identity = queue_identity(namespace, RetentionGenesisPage::Jobs { index })?;
    let bytes = bind(
        input,
        role,
        identity,
        PageKind::RetentionJobs,
        Some(job_cell(ordinal)),
        exact,
    )?;
    let view = row::decode_jobs_page(bytes, namespace, index)?;
    require(
        view.header().lsn.0 <= input.header.previous_lsn,
        InsertError::OrderMismatch,
    )?;
    for slot in 0..view.len() {
        require(
            view.cell(slot) == Some(&input.jobs[usize::from(index) * 15 + slot][..]),
            InsertError::IdentityMismatch,
        )?;
    }
    Ok((bytes, identity))
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

/// XRP1 projection of one Row presence, as the Job and core commit to it.
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

/// Missing-only U material: Live(incoming2, prevTxn H/Never) before CLR,
/// Releasing(incoming0, cursor0, prevTxn Cut/Never) after it.
fn material(observed: &Observed<'_>, live: bool) -> UndoCell<'static> {
    let transaction = match (live, observed.predecessor) {
        (true, Some(link)) => UndoEdge::Linked(link),
        (false, Some(_)) => UndoEdge::Cut,
        (_, None) => UndoEdge::Never,
    };
    let record = UndoRecord {
        creator: UndoCreator {
            id: observed.actor.id(),
            status_generation: observed.actor.generation(),
        },
        sequence: observed.undo.sequence,
        mutation_sequence: observed.job.mutation(),
        incoming: if live { 2 } else { 0 },
        target: row_target(observed.table, observed.row),
        edges: [transaction, UndoEdge::Never],
        before_kind: UndoBeforeKind::Missing,
        before_stamp: None,
        before_logical: None,
        payload: UndoPayload::None,
        release: UndoRelease {
            cursor: 0,
            chunks_released: 0,
            next: None,
        },
    };
    if live {
        UndoCell::Live(record)
    } else {
        UndoCell::Releasing(record)
    }
}

fn encode_material(observed: &Observed<'_>, live: bool, output: &mut [u8]) -> Result<()> {
    let expected = UndoRecordExpectation {
        namespace: observed.namespace,
        reference: observed.undo.reference,
        target: row_target(observed.table, observed.row),
    };
    undo_record::encode_undo_record(material(observed, live), expected, &[], output)?;
    Ok(())
}

/// Exact Row debt rows of the RootJob: Forward (structural) and CLR.
fn debt(fixed: u64, cell: usize, h: bool, undo_slots: u64) -> RemainingBudget {
    let wal = fixed + cell as u64 + 128 * u64::from(h);
    RemainingBudget {
        steps: 1,
        wal,
        undo_slots,
        peak: row::ROW_PEAK,
        protected: wal,
        ..RemainingBudget::default()
    }
}

/// The one normalized UndoSlot Item96 consumed by Forward.
fn undo_item(job: JobView<'_>) -> Result<[u8; 96]> {
    let mut item = [0; 96];
    item[0] = 2; // UndoSlot
    item[1] = 2; // Structural
    job.undo()
        .ok_or(InsertError::IdentityMismatch)?
        .encode(&mut item[8..48])?;
    item[64..72].copy_from_slice(&1u64.to_le_bytes());
    item[72..80].copy_from_slice(&job.lease_lsn().to_le_bytes());
    Ok(item)
}

fn rank(page: PageRef) -> Result<(u64, u64)> {
    let mut bytes = [0; 24];
    page.encode(&mut bytes)?;
    Ok((
        u64::from_le_bytes(bytes[..8].try_into().expect("file")),
        page.page_no,
    ))
}

/// Every input check, before any output byte. `record`, `aggregate` and
/// `check` are A scratch regions (M2) used only as check buffers here.
fn observe<'a>(
    input: &GroupInput<'a>,
    record: &mut [u8; leaf::LEAF_SCRATCH_BYTES],
    aggregate: &mut [u8; 640],
    check: &mut [u8; undo_record::UNDO_RECORD_BYTES],
) -> Result<Observed<'a>> {
    let forward = matches!(input.change, Change::Forward { .. });
    let profile = input.profile;
    let namespace = profile.spec().namespace;
    require(
        profile.format() == 3
            && namespace.timeline == 1
            && input.leaf.kind == PageKind::RowLeaf
            && input.leaf.level == 0
            && input.leaf.arity == 0,
        InsertError::Unsupported,
    )?;
    let header = input.header;
    let actor = input.actor;
    require(
        header.kind
            == if forward {
                FrameKind::Update
            } else {
                FrameKind::Clr
            }
            && header.stream == input.expected.stream
            && header.previous_lsn == input.expected.previous_lsn
            && header.stream.database == namespace.database
            && header.stream.timeline == namespace.timeline
            && header.stream.writer_epoch != 0
            && header.txn == Some(actor.id())
            && header.previous_lsn < header.lsn
            && header.previous_txn_lsn != 0
            && header.previous_txn_lsn <= header.previous_lsn
            && actor.id().database == namespace.database
            && actor.id().timeline == namespace.timeline
            && actor.generation() == input.status.generation()
            && input.status.slot() == 0
            && input.leaf.identity.database == namespace.database
            && input.leaf.identity.owner == input.table.object
            && input.leaf.identity.owner_generation == input.table.generation
            && input.before_count <= 9
            && input.root_job < JOB_COUNT
            && input.terminal_job < JOB_COUNT
            && input.root_job != input.terminal_job,
        InsertError::IdentityMismatch,
    )?;

    // Selected RootJob, TerminalJob and the census-derived predecessor H.
    let ordinal = input.root_job;
    let job = row::decode_job(&input.jobs[ordinal], namespace)?;
    let h = input.predecessor.is_some();
    require(
        job.state()
            == if forward {
                JobState::RootReserved
            } else {
                JobState::RootRetained
            }
            && job.last_step() <= header.previous_lsn,
        InsertError::OrderMismatch,
    )?;
    let pair = job.pair().ok_or(InsertError::IdentityMismatch)?;
    require(
        job.scope() == Some(JobScope::Row)
            && job.source() == Some(actor)
            && job.status() == Some(input.status)
            && job.undo() == Some(input.undo.reference)
            && job.undo_sequence() == input.undo.sequence
            && job.row_id() == Some(input.row)
            && job.object() == Some(input.table)
            && job.mutation() == input.mutation
            && input.statement.is_none_or(|value| value == job.statement())
            && job.has_previous_transaction() == Some(h)
            && pair[1] == input.leaf.identity.page,
        InsertError::IdentityMismatch,
    )?;
    let terminal = row::decode_job(&input.jobs[input.terminal_job], namespace)?;
    require(
        terminal.state() == JobState::TerminalLeased
            && terminal.source() == Some(actor)
            && terminal.status() == Some(input.status),
        InsertError::IdentityMismatch,
    )?;
    // One pass also sums the exact CLR residual row of this actor: one CLR
    // debt per Reserved/Retained Row root, each with its own h recipe.
    let mut previous: Option<JobView<'_>> = None;
    let (mut clr_steps, mut clr_wal) = (0u64, 0u64);
    for other in 0..JOB_COUNT {
        let candidate = row::decode_job(&input.jobs[other], namespace)?;
        if candidate.source() != Some(actor) {
            continue;
        }
        if matches!(
            candidate.state(),
            JobState::RootReserved | JobState::RootRetained
        ) {
            let recipe = candidate
                .has_previous_transaction()
                .ok_or(InsertError::IdentityMismatch)?;
            let owed = debt(CLR_FIXED, candidate.cell_bytes() as usize, recipe, 0);
            clr_steps = clr_steps
                .checked_add(1)
                .ok_or(InsertError::BudgetMismatch)?;
            clr_wal = clr_wal
                .checked_add(owed.wal)
                .ok_or(InsertError::BudgetMismatch)?;
        }
        if other != ordinal
            && candidate.state() == JobState::RootRetained
            && candidate.undo_sequence() < input.undo.sequence
            && previous.is_none_or(|old| old.undo_sequence() < candidate.undo_sequence())
        {
            previous = Some(candidate);
        }
    }
    let census = previous
        .map(|job| -> Result<UndoLink> {
            Ok(UndoLink {
                reference: job.undo().ok_or(InsertError::IdentityMismatch)?,
                sequence: job.undo_sequence(),
            })
        })
        .transpose()?;
    require(census == input.predecessor, InsertError::OrderMismatch)?;
    if let Some(mark) = input.mark {
        let mut found = false;
        for other in 0..JOB_COUNT {
            let candidate = row::decode_job(&input.jobs[other], namespace)?;
            found |= candidate.source() == Some(actor)
                && candidate.state() == JobState::RootRetained
                && candidate.undo() == Some(mark)
                && candidate.undo_sequence() < input.undo.sequence;
        }
        require(found, InsertError::OrderMismatch)?;
    }

    // Root page and the complete stable Job census bound to it.
    let root_identity = queue_identity(namespace, RetentionGenesisPage::Root)?;
    let root_bytes = bind(
        input,
        RowSourceRole::Root,
        root_identity,
        PageKind::RetentionRoot,
        Some(ROOT_CELL),
        true,
    )?;
    let (root_header, root) = row::decode_root_page(root_bytes, namespace)?;
    require(
        root_header.lsn.0 <= header.previous_lsn,
        InsertError::OrderMismatch,
    )?;
    row::verify_census(root, namespace, input.jobs, aggregate)?;
    let (jobs_bytes, jobs_identity) =
        bind_jobs(input, RowSourceRole::RootJob, ordinal, true, namespace)?;
    bind_jobs(
        input,
        RowSourceRole::TerminalJob,
        input.terminal_job,
        false,
        namespace,
    )?;

    // Actual Status before and the rollback/next/target relation.
    let status_identity = fixed_identity(namespace, input.status.page(), 2);
    let status_bytes = bind(
        input,
        RowSourceRole::Status,
        status_identity,
        PageKind::TransactionStatus,
        Some(STATUS_CELL),
        true,
    )?;
    let status_page = checked_page(
        status_bytes,
        status_identity,
        PageKind::TransactionStatus,
        header,
    )?;
    require(status_page.len() == 1, InsertError::IdentityMismatch)?;
    let status = StatusHome::decode(status_page.cell(0).ok_or(InsertError::IdentityMismatch)?)?;
    let TransactionStatusCell::Occupied(core) = status.core() else {
        return Err(InsertError::OrderMismatch);
    };
    require(core.reference() == actor, InsertError::IdentityMismatch)?;
    let head = input.predecessor.map(|link| link.reference);
    let (cell, target) = match input.change {
        Change::Forward { canonical } => {
            require(
                core.state() == TransactionState::Active && core.undo_head() == head,
                InsertError::OrderMismatch,
            )?;
            require(input.statement.is_some(), InsertError::InvalidInput)?;
            (180 + canonical.len(), None)
        }
        Change::Clr {
            cell_bytes,
            projection: digest,
            target,
            next,
        } => {
            let rollback_target = match core.state() {
                TransactionState::Aborting(RollbackState::Statement { target, next })
                    if next == core.undo_head() =>
                {
                    require(
                        input.statement.is_some() && input.mark == target,
                        InsertError::InvalidInput,
                    )?;
                    target
                }
                TransactionState::Aborting(RollbackState::Transaction { next })
                    if next == core.undo_head() =>
                {
                    require(
                        input.statement.is_none() && input.mark.is_none(),
                        InsertError::InvalidInput,
                    )?;
                    None
                }
                _ => return Err(InsertError::OrderMismatch),
            };
            require(
                core.undo_head() == Some(input.undo.reference)
                    && status.incoming() != 0
                    && target == rollback_target
                    && next == input.predecessor
                    && target != Some(input.undo.reference),
                InsertError::OrderMismatch,
            )?;
            require(
                digest == *job.projection_digest(),
                InsertError::IdentityMismatch,
            )?;
            (cell_bytes, target)
        }
    };
    require(
        cell == job.cell_bytes() as usize,
        InsertError::IdentityMismatch,
    )?;

    // Transaction descriptor before: actor, chain and reserved inventory.
    let transaction_bytes = input.transaction.ok_or(InsertError::InvalidInput)?;
    let aging = input.aging.ok_or(InsertError::InvalidInput)?;
    let transaction = obligation::decode_descriptor(
        transaction_bytes,
        ReservationOwner::Transaction(actor),
        profile_reference(profile),
    )?;
    require(
        transaction.owner == OwnerState::Transaction(core)
            && transaction.chain.apply == header.previous_txn_lsn
            && transaction.consumed_control == 0
            && transaction.inventory.growth_extents == 0,
        InsertError::IdentityMismatch,
    )?;
    // The residual is exact, so the CLR after row (one step and this C/h
    // debt less) cannot underflow or leave WAL without a step.
    let residual = RemainingBudget {
        steps: clr_steps,
        wal: clr_wal,
        peak: row::ROW_PEAK,
        protected: clr_wal,
        ..RemainingBudget::default()
    };
    require(
        clr_steps >= 1 && transaction.remaining[0] == residual,
        InsertError::BudgetMismatch,
    )?;
    if forward {
        let item = undo_item(job)?;
        require(
            transaction.inventory
                == InventoryClaim {
                    change_lsn: job.lease_lsn(),
                    items: 1,
                    digest: content_digest(&item),
                    growth_extents: 0,
                }
                && transaction.chain.apply == job.lease_lsn()
                && transaction.last_mutation_sequence == job.mutation()
                && transaction.remaining[1] == debt(FORWARD_FIXED, cell, h, 1),
            InsertError::BudgetMismatch,
        )?;
    } else {
        require(
            transaction.inventory.items == 0 && transaction.remaining[1].steps == 0,
            InsertError::BudgetMismatch,
        )?;
    }
    // Aging before: bound to this Root before (sequence, work digest, chain).
    aging_row::decode(aging, profile, root_bytes)?;

    // U before: the reserved Free generation or the Live material.
    let undo_identity = fixed_identity(namespace, input.undo.reference.page(), 3);
    let undo_bytes = bind(
        input,
        RowSourceRole::Undo,
        undo_identity,
        PageKind::Undo,
        Some(UNDO_CELL),
        true,
    )?;
    let undo_page = checked_page(undo_bytes, undo_identity, PageKind::Undo, header)?;
    require(
        undo_page.len() == 1 && undo_page.cell_range(0) == Some(INITIAL_UNDO_CELL),
        InsertError::IdentityMismatch,
    )?;
    let undo_cell = undo_page.cell(0).ok_or(InsertError::IdentityMismatch)?;

    // H proof: unchanged Live predecessor with a removable incoming edge.
    if let Some(link) = input.predecessor {
        let identity = fixed_identity(namespace, link.reference.page(), 3);
        let bytes = bind(
            input,
            RowSourceRole::PreviousUndo,
            identity,
            PageKind::Undo,
            Some(UNDO_CELL),
            false,
        )?;
        let proof = checked_page(bytes, identity, PageKind::Undo, header)?;
        let previous = previous.ok_or(InsertError::OrderMismatch)?;
        require(
            proof.len() == 1
                && proof.cell_range(0) == Some(INITIAL_UNDO_CELL)
                && proof.header().lsn.0 >= previous.last_step(),
            InsertError::IdentityMismatch,
        )?;
        let view = undo_record::decode_undo_record(
            proof.cell(0).ok_or(InsertError::IdentityMismatch)?,
            UndoRecordExpectation {
                namespace,
                reference: link.reference,
                target: row_target(
                    previous.object().ok_or(InsertError::IdentityMismatch)?,
                    previous.row_id().ok_or(InsertError::IdentityMismatch)?,
                ),
            },
            &[],
        )?;
        let metadata = view.metadata()?;
        require(
            view.state() == UndoRecordState::Live
                && metadata.sequence == link.sequence
                && metadata.creator.id == actor.id()
                && metadata.creator.status_generation == actor.generation()
                && metadata.incoming >= 1,
            InsertError::OrderMismatch,
        )?;
    } else {
        require(
            input.sources[source_index(RowSourceRole::PreviousUndo)].is_none(),
            InsertError::InvalidInput,
        )?;
    }

    // Stable anchor proof selects the single level0 RowLeaf.
    let anchor_identity = PageIdentity {
        database: namespace.database,
        page: pair[0],
        owner: input.table.object,
        owner_generation: input.table.generation,
    };
    let anchor_bytes = bind(
        input,
        RowSourceRole::TableAnchor,
        anchor_identity,
        PageKind::TreeAnchor,
        Some(ANCHOR_CELL),
        false,
    )?;
    checked_page(anchor_bytes, anchor_identity, PageKind::TreeAnchor, header)?;
    let anchor = anchor::decode_anchor(anchor_bytes, anchor_identity)?;
    require(
        anchor.root().tree == TreeKind::Row
            && anchor.root().level == 0
            && anchor.root().page == input.leaf.identity.page,
        InsertError::IdentityMismatch,
    )?;

    // RowLeaf before; the CLR current row is checked here, before writes.
    let leaf_source =
        input.sources[source_index(RowSourceRole::RowLeaf)].ok_or(InsertError::InvalidInput)?;
    require(
        leaf_source.index < input.before_count,
        InsertError::IdentityMismatch,
    )?;
    let leaf_bytes = &input.before[leaf_source.index];
    checked_page(leaf_bytes, input.leaf.identity, PageKind::RowLeaf, header)?;
    let actual_cell = if forward {
        None
    } else {
        let current = leaf::current(
            namespace,
            input.leaf,
            input.schema,
            leaf_bytes,
            current_version(actor, input.row, job, input.undo.reference)?,
        )?;
        require(
            current.record.len() + 12 == cell
                && projection(input.table, input.row, Some(current.record))?
                    == *job.projection_digest(),
            InsertError::IdentityMismatch,
        )?;
        Some(current.cell)
    };
    bind(
        input,
        RowSourceRole::RowLeaf,
        input.leaf.identity,
        PageKind::RowLeaf,
        actual_cell,
        false,
    )?;
    // The whole leaf edit plan, before writes: duplicate RowId, inline limit
    // and capacity for Forward; the exact removal for CLR. The Forward record
    // must be the one the RootJob projection committed to at reserve.
    let version = current_version(actor, input.row, job, input.undo.reference)?;
    match input.change {
        Change::Forward { canonical } => {
            let candidate = leaf::check(
                leaf::LeafInput {
                    namespace,
                    expected: input.leaf,
                    before: leaf_bytes,
                    schema: input.schema,
                    canonical,
                    version,
                    lsn: PageLsn(header.lsn),
                },
                record,
            )?;
            let record = &record[..candidate.record_bytes];
            require(
                record.len() + 12 == cell
                    && projection(input.table, input.row, Some(record))?
                        == *job.projection_digest(),
                InsertError::IdentityMismatch,
            )?;
        }
        Change::Clr { .. } => leaf::remove(
            namespace,
            input.leaf,
            input.schema,
            leaf_bytes,
            input.row,
            PageLsn(header.lsn),
            None,
        )?,
    }

    if forward {
        require(
            undo_page.header().lsn.0 == job.lease_lsn(),
            InsertError::OrderMismatch,
        )?;
        undo_record::decode_free_undo_record(undo_cell, input.undo.reference.generation())?;
    }
    let mut changed = [
        (
            RowPageRole::Root,
            root_bytes,
            root_identity,
            PageKind::RetentionRoot,
        ),
        (
            RowPageRole::RootJob,
            jobs_bytes,
            jobs_identity,
            PageKind::RetentionJobs,
        ),
        (
            RowPageRole::Status,
            status_bytes,
            status_identity,
            PageKind::TransactionStatus,
        ),
        (RowPageRole::Undo, undo_bytes, undo_identity, PageKind::Undo),
        (
            RowPageRole::RowLeaf,
            leaf_bytes,
            input.leaf.identity,
            PageKind::RowLeaf,
        ),
    ];
    let mut ranks = [(0, 0); 5];
    for (rank_value, page) in ranks.iter_mut().zip(changed) {
        *rank_value = rank(page.2.page)?;
    }
    let mut order = [0, 1, 2, 3, 4];
    order.sort_unstable_by_key(|index| ranks[*index]);
    changed = order.map(|index| changed[index]);
    let mut pages = [Changed {
        role: RowPageRole::Root,
        before: root_bytes,
        identity: root_identity,
        kind: PageKind::RetentionRoot,
        lsn: 0,
    }; 5];
    for (index, (slot, page)) in pages.iter_mut().zip(changed).enumerate() {
        require(
            index == 0 || ranks[order[index - 1]] < ranks[order[index]],
            InsertError::IdentityMismatch,
        )?;
        *slot = Changed {
            role: page.0,
            before: page.1,
            identity: page.2,
            kind: page.3,
            lsn: page::decode_page(page.1, page.2, page.3)?.header().lsn.0,
        };
    }
    require(
        input.after_order.len() == 5
            && input
                .after_order
                .iter()
                .zip(pages)
                .all(|(role, page)| *role == page.role),
        InsertError::IdentityMismatch,
    )?;
    if !forward {
        let partial = Observed {
            forward,
            namespace,
            profile,
            header,
            actor,
            table: input.table,
            row: input.row,
            leaf: input.leaf,
            undo: input.undo,
            predecessor: input.predecessor,
            target,
            ordinal,
            job,
            cell,
            frame_len: 0,
            status,
            transaction,
            transaction_bytes,
            aging,
            jobs: input.jobs,
            pages,
        };
        encode_material(&partial, true, check)?;
        require(
            undo_cell == &check[..] && undo_page.header().lsn.0 >= job.last_step(),
            InsertError::IdentityMismatch,
        )?;
    }
    let operation = if forward {
        Operation::Forward
    } else {
        Operation::Clr
    };
    Ok(Observed {
        forward,
        namespace,
        profile,
        header,
        actor,
        table: input.table,
        row: input.row,
        leaf: input.leaf,
        undo: input.undo,
        predecessor: input.predecessor,
        target,
        ordinal,
        job,
        cell,
        frame_len: operation.frame_bytes(cell, h)?,
        status,
        transaction,
        transaction_bytes,
        aging,
        jobs: input.jobs,
        pages,
    })
}

fn current_version(
    actor: TxnRef,
    row: RowId,
    job: JobView<'_>,
    undo: UndoRef,
) -> Result<crate::storage::row_record::RecordVersion> {
    Ok(crate::storage::row_record::RecordVersion {
        row,
        stamp: crate::storage::transaction_record::VersionStamp::Transaction(actor),
        logical: crate::storage::transaction_record::LogicalRowVersion::new(
            actor.id(),
            job.mutation(),
        )?,
        undo: Some(undo),
    })
}

/// Rough nonrecursive carrier inventory of this module; caller loans excluded.
/// The byte buffers of `Scratch` are A scratch regions of the lane (M2), a
/// loan, so only their references count. The 256 B allowance stands for the
/// module's small byte arrays (projection prefix, Undo item, page rank);
/// no 256 B buffer exists to move to the A scratch.
pub(super) fn local_source_carrier_bytes() -> usize {
    use std::mem::size_of;
    size_of::<GroupInput<'_>>()
        + size_of::<Observed<'_>>()
        + size_of::<Result<Observed<'_>>>()
        + size_of::<Produced>()
        + size_of::<Scratch<'_>>()
        + size_of::<[u8; 256]>()
        + size_of::<[&[u8]; 15]>()
        + 2 * size_of::<DescriptorSpec>()
        + size_of::<images::Prepared<'_>>()
        + size_of::<leaf::LeafInput<'_>>()
        + size_of::<RemainingBudget>()
        + images::local_source_carrier_bytes()
        + frame::local_source_carrier_bytes()
}
