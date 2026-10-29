//! Complete scope3 Row INSERT Forward over plain observations.
//! C0 only: every input is a borrowed copy of an owner observation and grants
//! no reservation, pin, stage, install or recovery authority. The owner must
//! recheck the whole candidate; an error leaves frame/after unspecified.

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
        page::{self, PAGE_BYTES, PageIdentity, PageKind, PageRef, PageView},
        retention_genesis::{self, RetentionGenesisPage},
        retention_obligation::profile_reference,
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self, JobScope, JobState, JobView},
        },
        row_record::RowSchemaInput,
        semantic_row::Operation,
        status_home::StatusHome,
        transaction_record::{StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef},
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

/// The reserve-sealed INSERT value.
#[derive(Clone, Copy, Debug)]
pub(super) enum Change<'a> {
    Forward { canonical: &'a [u8] },
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

/// Facts checked from the inputs before any output byte is written.
struct Observed<'a> {
    namespace: Namespace,
    profile: Profile,
    header: WalFrameHeader,
    actor: TxnRef,
    table: ObjectHandle,
    row: RowId,
    leaf: TreeExpectation,
    undo: UndoLink,
    predecessor: Option<UndoLink>,
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

/// Produce the whole Forward candidate. All input checks precede the
/// first output write; later codec failures may leave partial output bytes.
pub(super) fn produce(
    input: &GroupInput<'_>,
    frame: &mut [u8; FRAME_BYTES],
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<Produced> {
    let observed = observe(input)?;
    let job = images::job(&observed)?;
    let status = images::status(&observed)?;
    images::pages(&observed, input, &job, status, frame, after)?;
    let output = &mut frame[..observed.frame_len];
    output.fill(0);
    images::descriptors(&observed, &job, status, after, output)?;
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
    require(
        view.header().level == 0
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

/// Missing-only U material after Forward: Live(incoming2, prevTxn H/Never).
fn material(observed: &Observed<'_>) -> UndoCell<'static> {
    let transaction = match observed.predecessor {
        Some(link) => UndoEdge::Linked(link),
        None => UndoEdge::Never,
    };
    UndoCell::Live(UndoRecord {
        creator: UndoCreator {
            id: observed.actor.id(),
            status_generation: observed.actor.generation(),
        },
        sequence: observed.undo.sequence,
        mutation_sequence: observed.job.mutation(),
        incoming: 2,
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
    })
}

fn encode_material(observed: &Observed<'_>, output: &mut [u8]) -> Result<()> {
    let expected = UndoRecordExpectation {
        namespace: observed.namespace,
        reference: observed.undo.reference,
        target: row_target(observed.table, observed.row),
    };
    undo_record::encode_undo_record(material(observed), expected, &[], output)?;
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

fn observe<'a>(input: &GroupInput<'a>) -> Result<Observed<'a>> {
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
        header.kind == FrameKind::Update
            && header.stream == input.expected.stream
            && header.previous_lsn == input.expected.previous_lsn
            && header.stream.database == namespace.database
            && header.stream.timeline == namespace.timeline
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
        job.state() == JobState::RootReserved && job.last_step() <= header.previous_lsn,
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
    let mut previous: Option<JobView<'_>> = None;
    for other in 0..JOB_COUNT {
        let candidate = row::decode_job(&input.jobs[other], namespace)?;
        if other != ordinal
            && candidate.source() == Some(actor)
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
    let mut aggregate = [0; 640];
    row::verify_census(root, namespace, input.jobs, &mut aggregate)?;
    let (jobs_bytes, jobs_identity) =
        bind_jobs(input, RowSourceRole::RootJob, ordinal, true, namespace)?;
    bind_jobs(
        input,
        RowSourceRole::TerminalJob,
        input.terminal_job,
        false,
        namespace,
    )?;

    // Actual Status before: Active with head H (or empty).
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
    let Change::Forward { canonical } = input.change;
    require(
        core.state() == TransactionState::Active && core.undo_head() == head,
        InsertError::OrderMismatch,
    )?;
    require(input.statement.is_some(), InsertError::InvalidInput)?;
    let cell = 180 + canonical.len();
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
    let clr_debt = debt(CLR_FIXED, cell, h, 0);
    require(
        transaction.owner == OwnerState::Transaction(core)
            && transaction.chain.apply == header.previous_txn_lsn
            && transaction.consumed_control == 0
            && transaction.inventory.growth_extents == 0,
        InsertError::IdentityMismatch,
    )?;
    require(
        transaction.remaining[0].steps >= 1
            && transaction.remaining[0].wal >= clr_debt.wal
            && transaction.remaining[0].protected >= clr_debt.wal,
        InsertError::BudgetMismatch,
    )?;
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

    // U before: the reserved Free generation.
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

    // RowLeaf before; the inserted row is absent, so no cell is bound.
    let leaf_source =
        input.sources[source_index(RowSourceRole::RowLeaf)].ok_or(InsertError::InvalidInput)?;
    require(
        leaf_source.index < input.before_count,
        InsertError::IdentityMismatch,
    )?;
    let leaf_bytes = &input.before[leaf_source.index];
    checked_page(leaf_bytes, input.leaf.identity, PageKind::RowLeaf, header)?;
    bind(
        input,
        RowSourceRole::RowLeaf,
        input.leaf.identity,
        PageKind::RowLeaf,
        None,
        false,
    )?;
    require(
        undo_page.header().lsn.0 == job.lease_lsn(),
        InsertError::OrderMismatch,
    )?;
    undo_record::decode_free_undo_record(undo_cell, input.undo.reference.generation())?;
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
    Ok(Observed {
        namespace,
        profile,
        header,
        actor,
        table: input.table,
        row: input.row,
        leaf: input.leaf,
        undo: input.undo,
        predecessor: input.predecessor,
        ordinal,
        job,
        cell,
        frame_len: Operation::Forward.frame_bytes(cell, h)?,
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
pub(super) fn local_source_carrier_bytes() -> usize {
    use std::mem::size_of;
    size_of::<GroupInput<'_>>()
        + size_of::<Observed<'_>>()
        + size_of::<Result<Observed<'_>>>()
        + size_of::<Produced>()
        + size_of::<[u8; 640]>()
        + size_of::<[u8; JOB_BYTES]>()
        + size_of::<[u8; 512]>()
        + size_of::<[u8; 224]>()
        + size_of::<[u8; undo_record::UNDO_RECORD_BYTES]>()
        + size_of::<[u8; 256]>()
        + size_of::<[&[u8]; 15]>()
        + 2 * size_of::<DescriptorSpec>()
        + images::local_source_carrier_bytes()
        + frame::local_source_carrier_bytes()
}
