//! Every input check and after value of one Row group, before the first output
//! byte (consumers.md row-states/row-shape; create-group-format.md §queue).
//! `write_pages` and `finish_aging` only place values whose inputs were
//! checked here; their errors are broken internal invariants.
use super::super::{Error, Operation, Result, ledger, status::StatementBoundary};
use super::{
    Change, Changed, Inputs, Issued, JOB, LEAF, LeafEdit, MAX_CELL, MAX_RECORD, Prepared, ROOT,
    STATUS, UNDO,
    cells::{self, RELEASE_BYTES, TERMINAL_BYTES, put, require, same, set_budget},
    frame,
    graph::{self, Aux, Core},
};
use crate::storage::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    anchor::{self, TreeKind},
    obligation::{self, DescriptorSpec, OwnerChain, OwnerState},
    page::{self, PAGE_BYTES, PageHeader, PageKind, PageLsn, PageRef},
    retention_genesis::RetentionGenesisPage,
    retention_obligation::{
        AGING_START_BYTES, profile_reference,
        row::{self as aging_row, AgingTransition},
    },
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        row::{self, JobCells, JobScope, JobState, JobView},
    },
    row_record::{self, RecordExpectation, RecordVersion, RowPayload, RowSchemaInput},
    status_home::{STATUS_HOME_BYTES, StatusHome},
    transaction_record::{
        LogicalRowVersion, RollbackState, StatusSlotRef, TransactionState, TransactionStatus,
        TransactionStatusCell, TxnRef, VersionStamp,
    },
    tree_page::{self, EntryPayload, TREE_ENTRY_CAPACITY, TreeExpectation, edit},
    undo_record::{self, UndoCell, UndoEdge, UndoLink},
    undo_ref::UndoRef,
    wal_frame::WalFrameHeader,
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, RowId},
};

/// Validated rows occupy at least 189 bytes including their slot, plus the
/// metadata cell and one inserted cell.
const ROW_EDIT_CELLS: usize = TREE_ENTRY_CAPACITY / (row_record::ROW_HEADER_BYTES + 5 + 16) + 2;

pub(super) fn new_prepared<'a>(inputs: &Inputs<'a>) -> Prepared<'a> {
    Prepared {
        op: inputs.change.operation(),
        header: WalFrameHeader {
            stream: inputs.frame.stream,
            kind: inputs.change.operation().outer(),
            lsn: 0,
            previous_lsn: inputs.frame.previous_lsn,
            txn: None,
            previous_txn_lsn: 0,
        },
        frame_bytes: 0,
        row_cell: 0,
        predecessor: false,
        ordinal: None,
        actor: None,
        location: None,
        issued: None,
        job: [0; JOB_BYTES],
        root: [0; crate::storage::retention_page::ROOT_BYTES],
        status: None,
        undo: [0; undo_record::UNDO_RECORD_BYTES],
        leaf: None,
        cell: [0; MAX_CELL],
        cell_len: 0,
        descriptors: [[0; 1536]; 2],
        core: [0; 256],
        aux: Aux::new(),
        pages: [None; 5],
        aging_before: None,
        transaction_before: None,
    }
}

fn needs_status(op: Operation) -> bool {
    !matches!(
        op,
        Operation::AgingStart | Operation::ReleaseRole | Operation::RootFree
    )
}
fn is_tx(op: Operation) -> bool {
    op.handoff() || op.status_patch()
}
fn has_root(op: Operation) -> bool {
    !op.status_patch()
}
fn occupied(home: StatusHome, actor: TxnRef) -> Result<TransactionStatus> {
    let TransactionStatusCell::Occupied(status) = home.core() else {
        return Err(Error::OrderMismatch);
    };
    same(status.reference() == actor)?;
    Ok(status)
}
fn home(
    actor: TxnRef,
    state: TransactionState,
    head: Option<UndoRef>,
    incoming: u64,
) -> Result<StatusHome> {
    require(head.is_some() == (incoming != 0))?;
    Ok(StatusHome::new(
        TransactionStatusCell::Occupied(TransactionStatus::new(actor, state, head)?),
        incoming,
    )?)
}
fn decode_job<'a>(inputs: &Inputs<'a>, ordinal: usize) -> Result<JobView<'a>> {
    if ordinal >= JOB_COUNT {
        return Err(Error::InvalidInput);
    }
    Ok(row::decode_job(
        &inputs.jobs[ordinal],
        inputs.profile.spec().namespace,
    )?)
}
/// The actor's live TerminalLeased Job names the actor and its Status.
fn terminal_actor(inputs: &Inputs<'_>, ordinal: usize) -> Result<(TxnRef, StatusSlotRef)> {
    let job = decode_job(inputs, ordinal)?;
    if job.scope() != Some(JobScope::Row) {
        return Err(if job.state() == JobState::Free {
            Error::OrderMismatch
        } else {
            Error::Unsupported
        });
    }
    require(job.state() == JobState::TerminalLeased)?;
    Ok((
        job.source().ok_or(Error::IdentityMismatch)?,
        job.status().ok_or(Error::IdentityMismatch)?,
    ))
}

/// Exact U material of a Live root and its H proof page, when H exists.
fn live_material(
    inputs: &Inputs<'_>,
    job: JobView<'_>,
    previous: Option<JobView<'_>>,
) -> Result<UndoEdge> {
    let namespace = inputs.profile.spec().namespace;
    let Some(previous) = previous else {
        same(inputs.previous.is_none())?;
        return Ok(UndoEdge::Never);
    };
    let link = cells::link(previous)?;
    same(!cells::same_address(
        link.reference.page(),
        job.undo().ok_or(Error::IdentityMismatch)?.page(),
    ))?;
    let bytes = inputs.previous.ok_or(Error::InvalidInput)?;
    let actor = job.source().ok_or(Error::IdentityMismatch)?;
    let cell = cells::undo_before(bytes, namespace, link.reference.page(), inputs.frame)?;
    require(cells::page_lsn(bytes) >= previous.last_step())?;
    let earlier = cells::predecessor(
        JobCells::original(inputs.jobs),
        namespace,
        actor,
        previous.undo_sequence(),
    )?;
    require(previous.has_previous_transaction() == Some(earlier.is_some()))?;
    let edge = match earlier {
        Some(job) => UndoEdge::Linked(cells::link(job)?),
        None => UndoEdge::Never,
    };
    // H is Live with its two persistent incoming edges: a removable one exists.
    cells::check_material(
        namespace,
        previous,
        UndoCell::Live(cells::material(previous, edge, 2, 0)?),
        cell,
    )?;
    Ok(UndoEdge::Linked(link))
}

/// Leaf observation of Forward/CLR/Prune: anchor re-search, all rows typed.
struct LeafSource<'a> {
    expected: TreeExpectation,
    schema: RowSchemaInput<'a>,
    before: &'a [u8; PAGE_BYTES],
}
fn leaf_source<'a>(inputs: &Inputs<'a>, job: JobView<'_>) -> Result<LeafSource<'a>> {
    let table = inputs.table.ok_or(Error::InvalidInput)?;
    let pair = job.pair().ok_or(Error::IdentityMismatch)?;
    same(job.object() == Some(table.handle) && pair[0] == table.anchor)?;
    let leaf = table_leaf(inputs, table)?;
    same(leaf == pair[1])?;
    typed_leaf(inputs, table, leaf)
}
/// The anchor's RowLeaf before image with every row typed by the schema.
fn typed_leaf<'a>(
    inputs: &Inputs<'a>,
    table: super::TableSource<'a>,
    leaf: PageRef,
) -> Result<LeafSource<'a>> {
    let namespace = inputs.profile.spec().namespace;
    let expected = TreeExpectation {
        identity: cells::table_identity(table.handle, namespace, leaf),
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    };
    let before = table.leaf_image.ok_or(Error::InvalidInput)?;
    cells::before_page(
        before,
        expected.identity,
        PageKind::RowLeaf,
        inputs.frame,
        false,
    )?;
    let view = tree_page::decode_tree_page(before, expected)?;
    for index in 0..view.len() {
        let entry = view.entry(index)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Corrupt)?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(Error::Unsupported);
        };
        let record = row_record::decode_row_record_with_schema(
            value,
            RecordExpectation { namespace, row },
            table.schema,
        )?;
        if !matches!(record.payload(), RowPayload::Inline(_)) {
            return Err(Error::Unsupported);
        }
    }
    Ok(LeafSource {
        expected,
        schema: table.schema,
        before,
    })
}
/// RowReserve's leaf proof (consumers.md:73, :282): the stable RowLeaf types
/// every row, lacks the issued key and takes the planned cell without a
/// split. `scratch` only holds the planned cell while it is checked.
fn reserve_leaf(
    inputs: &Inputs<'_>,
    table: super::TableSource<'_>,
    leaf: PageRef,
    row: RowId,
    record: &[u8],
    scratch: &mut [u8; MAX_CELL],
) -> Result<()> {
    let source = typed_leaf(inputs, table, leaf)?;
    let key = row.get().to_be_bytes();
    let view = tree_page::decode_tree_page(source.before, source.expected)?;
    let cell = tree_page::encode_leaf_cell(PageKind::RowLeaf, 0, &key, record, scratch)?;
    let mut refs: [&[u8]; ROW_EDIT_CELLS] = [&[]; ROW_EDIT_CELLS];
    let plan = view
        .prepare_leaf_edit(edit::LeafEdit::Insert(cell), &mut refs)
        .map_err(cells::edit_error)?;
    require(!plan.needs_split())
}
/// The stable anchor's single level0 RowLeaf (the Job leaf is only a hint).
fn table_leaf(
    inputs: &Inputs<'_>,
    table: super::TableSource<'_>,
) -> Result<crate::storage::page::PageRef> {
    let namespace = inputs.profile.spec().namespace;
    same(cells::dynamic(table.anchor))?;
    let identity = cells::table_identity(table.handle, namespace, table.anchor);
    cells::before_page(
        table.anchor_image,
        identity,
        PageKind::TreeAnchor,
        inputs.frame,
        false,
    )?;
    let anchor = anchor::decode_anchor(table.anchor_image, identity)?;
    if anchor.root().tree != TreeKind::Row || anchor.root().level != 0 {
        return Err(Error::Unsupported);
    }
    same(
        cells::dynamic(anchor.root().page)
            && !cells::same_address(anchor.root().page, table.anchor),
    )?;
    Ok(anchor.root().page)
}
fn current_version(job: JobView<'_>, actor: TxnRef) -> Result<RecordVersion> {
    Ok(RecordVersion {
        row: job.row_id().ok_or(Error::IdentityMismatch)?,
        stamp: VersionStamp::Transaction(actor),
        logical: LogicalRowVersion::new(actor.id(), job.mutation())?,
        undo: Some(job.undo().ok_or(Error::IdentityMismatch)?),
    })
}
/// One U-owned Missing INSERT record: version, C and the sealed projection.
fn check_record<'a>(
    source: &LeafSource<'_>,
    job: JobView<'_>,
    actor: TxnRef,
    record: &'a [u8],
) -> Result<row_record::RowRecordView<'a>> {
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    let view = row_record::decode_row_record_with_schema(
        record,
        RecordExpectation {
            namespace: crate::api::Namespace {
                database: source.expected.identity.database,
                timeline: 1,
            },
            row,
        },
        source.schema,
    )?;
    same(
        view.version() == current_version(job, actor)?
            && matches!(view.payload(), RowPayload::Inline(_))
            && record.len() + 12 == job.cell_bytes() as usize
            && cells::projection(
                job.object().ok_or(Error::IdentityMismatch)?,
                row,
                Some(record),
            )? == *job.projection_digest(),
    )?;
    Ok(view)
}

/// Row native slot and the after cell plan; also the core projections.
fn prepare_leaf<'a>(
    inputs: &Inputs<'a>,
    out: &mut Prepared<'a>,
    job: JobView<'_>,
    actor: TxnRef,
    inserted: Option<&'a [u8]>,
    q: u64,
) -> Result<(Option<u16>, Option<u16>, [u8; 32], [u8; 32])> {
    let op = out.op;
    let source = leaf_source(inputs, job)?;
    require(op == Operation::Forward || cells::page_lsn(source.before) >= job.last_step())?;
    let table = job.object().ok_or(Error::IdentityMismatch)?;
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    let key = row.get().to_be_bytes();
    let view = tree_page::decode_tree_page(source.before, source.expected)?;
    let position = view.lower_bound(&key)?;
    let slot = (position.index + 1) as u16;
    let missing = cells::projection(table, row, None)?;
    let mut refs: [&[u8]; ROW_EDIT_CELLS] = [&[]; ROW_EDIT_CELLS];
    let result = match op {
        Operation::Forward => {
            same(!position.equal)?;
            let cell = inserted.ok_or(Error::InvalidInput)?;
            same(cell.len() == job.cell_bytes() as usize && cell.len() >= 12)?;
            same(
                cell[..2] == 8u16.to_le_bytes()
                    && usize::from(u16::from_le_bytes([cell[2], cell[3]])) + 12 == cell.len()
                    && cell[4..12] == key,
            )?;
            check_record(&source, job, actor, &cell[12..])?;
            let plan = view
                .prepare_leaf_edit(edit::LeafEdit::Insert(cell), &mut refs)
                .map_err(cells::edit_error)?;
            require(!plan.needs_split())?;
            out.leaf = Some((LeafEdit::Insert(cell), source.expected, source.schema));
            (None, Some(slot), missing, *job.projection_digest())
        }
        Operation::Clr | Operation::Prune => {
            same(position.equal)?;
            let EntryPayload::Value(record) = view.entry(position.index)?.payload else {
                return Err(Error::Unsupported);
            };
            let old = check_record(&source, job, actor, record)?;
            if op == Operation::Clr {
                view.prepare_leaf_edit(edit::LeafEdit::Remove(&key), &mut refs)
                    .map_err(cells::edit_error)?;
                out.leaf = Some((LeafEdit::Remove(row), source.expected, source.schema));
                (Some(slot), None, *job.projection_digest(), missing)
            } else {
                let RowPayload::Inline(canonical) = old.payload() else {
                    return Err(Error::Unsupported);
                };
                let version = RecordVersion {
                    stamp: VersionStamp::Committed(
                        CommitSeq::new(q).map_err(|_| Error::OrderMismatch)?,
                    ),
                    undo: None,
                    ..old.version()
                };
                let mut record_bytes = [0; MAX_RECORD];
                let length = row_record::encode_row_record_with_schema(
                    RecordExpectation {
                        namespace: inputs.profile.spec().namespace,
                        row,
                    },
                    source.schema,
                    version,
                    RowPayload::Inline(canonical),
                    &mut record_bytes,
                )?;
                same(length == record.len())?;
                let after = cells::projection(table, row, Some(&record_bytes[..length]))?;
                let cell_len = tree_page::encode_leaf_cell(
                    PageKind::RowLeaf,
                    0,
                    &key,
                    &record_bytes[..length],
                    &mut out.cell,
                )?
                .len();
                out.cell_len = cell_len;
                let plan = view
                    .prepare_leaf_edit(edit::LeafEdit::Replace(&out.cell[..cell_len]), &mut refs)
                    .map_err(cells::edit_error)?;
                require(!plan.needs_split())?;
                out.leaf = Some((LeafEdit::Replace, source.expected, source.schema));
                (Some(slot), Some(slot), *job.projection_digest(), after)
            }
        }
        _ => return Err(Error::InvalidInput),
    };
    out.pages[LEAF] = Some(Changed {
        identity: source.expected.identity,
        kind: PageKind::RowLeaf,
        before: source.before,
        before_slot: result.0,
        after_slot: result.1,
    });
    Ok(result)
}

#[allow(
    clippy::too_many_lines,
    reason = "one closed transition table keeps every refusal before the first write"
)]
pub(super) fn prepare<'a>(
    inputs: &Inputs<'a>,
    inserted: Option<&'a [u8]>,
    out: &mut Prepared<'a>,
) -> Result<()> {
    let op = inputs.change.operation();
    let profile = inputs.profile;
    if profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    let namespace = profile.spec().namespace;
    let frame = inputs.frame;
    if namespace.timeline != 1
        || namespace.database != frame.stream.database
        || namespace.timeline != frame.stream.timeline
        || frame.stream.writer_epoch == 0
    {
        return Err(Error::IdentityMismatch);
    }
    let lsn = frame.previous_lsn.checked_add(1).ok_or(Error::Exhausted)?;
    out.op = op;
    out.header.lsn = lsn;

    // Root and the complete stable Job census committed by it.
    let (root_header, root) = row::decode_root_page(inputs.root, namespace)?;
    require(root_header.lsn.0 <= frame.previous_lsn)?;
    require(root.aging().is_none() == (op == Operation::AgingStart))?;
    let mut aggregate = [0; 640];
    row::verify_census(root, namespace, inputs.jobs, &mut aggregate)?;
    let original = JobCells::original(inputs.jobs);

    // Selected Job, actor and Status location.
    let selected = match inputs.change {
        Change::AgingStart | Change::RollbackStart { .. } | Change::StatementFinish { .. } => None,
        Change::Begin { scope, .. } => {
            if scope != JobScope::Row {
                return Err(Error::Unsupported);
            }
            Some(cells::first_free(root, inputs.jobs, namespace)?)
        }
        Change::Reserve(_) => Some(cells::first_free(root, inputs.jobs, namespace)?),
        Change::Forward { ordinal, .. }
        | Change::Cancel { ordinal }
        | Change::Clr { ordinal, .. }
        | Change::Prune { ordinal }
        | Change::ReleaseRole { ordinal }
        | Change::RootFree { ordinal }
        | Change::TerminalFinish { ordinal } => Some(ordinal),
        Change::Commit { terminal } | Change::AbortEnd { terminal } => Some(terminal),
    };
    let before = selected
        .map(|ordinal| decode_job(inputs, ordinal))
        .transpose()?;
    let (actor, location) = match inputs.change {
        Change::AgingStart => (None, None),
        Change::Begin { actor, status, .. } => (Some(actor), Some(status)),
        Change::Reserve(plan) => {
            let (actor, status) = terminal_actor(inputs, plan.terminal)?;
            (Some(actor), Some(status))
        }
        Change::RollbackStart { terminal, .. } | Change::StatementFinish { terminal, .. } => {
            let (actor, status) = terminal_actor(inputs, terminal)?;
            (Some(actor), Some(status))
        }
        _ => {
            let job = before.ok_or(Error::InvalidInput)?;
            if job.state() == JobState::Free {
                return Err(Error::OrderMismatch);
            }
            if job.scope() != Some(JobScope::Row) {
                return Err(Error::Unsupported);
            }
            (job.source(), job.status())
        }
    };
    out.ordinal = selected;
    out.actor = actor;
    out.location = location;
    if let (Some(actor), Some(location)) = (actor, location) {
        cells::validate_actor(namespace, actor, location)?;
    }
    let summary = actor
        .map(|actor| cells::actor_jobs(inputs.jobs, namespace, actor))
        .transpose()?;

    // Status before and the actor/descriptor closure of the original owner.
    let status_before = if needs_status(op) {
        Some(cells::status_before(
            inputs.status.ok_or(Error::InvalidInput)?,
            namespace,
            location.ok_or(Error::InvalidInput)?,
            frame,
        )?)
    } else {
        None
    };
    let tx_spec = if is_tx(op) && op != Operation::Begin {
        let raw = inputs.transaction.ok_or(Error::InvalidInput)?;
        ledger::validate_transaction(
            raw,
            profile,
            status_before.ok_or(Error::InvalidInput)?,
            location.ok_or(Error::InvalidInput)?,
            original,
        )?;
        let spec = obligation::decode_descriptor(
            raw,
            ReservationOwner::Transaction(actor.ok_or(Error::InvalidInput)?),
            profile_reference(profile),
        )?;
        require(spec.chain.apply <= frame.previous_lsn && spec.chain.apply < lsn)?;
        out.transaction_before = Some(raw);
        Some(spec)
    } else {
        if op == Operation::Begin && inputs.transaction.is_some() {
            return Err(Error::OrderMismatch);
        }
        if let (Some(home), Some(location)) = (status_before, location)
            && op != Operation::Begin
        {
            ledger::validate_actor(profile, home, location, original)?;
        }
        None
    };
    if has_root(op) {
        if op == Operation::AgingStart {
            require(inputs.aging.is_none())?;
        } else {
            let raw = inputs.aging.ok_or(Error::InvalidInput)?;
            aging_row::decode(raw, profile, inputs.root).map_err(cells::aging_error)?;
            out.aging_before = Some(raw);
        }
    }
    let sender = inputs.transaction.map(|raw| content_digest(raw));

    // Op-specific preconditions, selected Job after, Status/U/Leaf afters.
    let head = status_before.and_then(|home| match home.core() {
        TransactionStatusCell::Occupied(status) => status.undo_head(),
        TransactionStatusCell::Free(_) => None,
    });
    let mut q = 0;
    let mut core = Core::default();
    let mut credit = false;
    let mut item = None;
    let mut statement_target = None;
    let mut graph_links = None;
    if let Some(ordinal) = selected {
        out.job.copy_from_slice(&inputs.jobs[ordinal]);
    }
    match inputs.change {
        Change::AgingStart => {}
        Change::Begin { actor, status, .. } => {
            same(actor.id().writer_epoch == frame.stream.writer_epoch)?;
            let summary = summary.as_ref().expect("actor");
            require(summary.terminal.is_none() && summary.roots == 0)?;
            let home_before = status_before.expect("status");
            let TransactionStatusCell::Free(generation) = home_before.core() else {
                return Err(Error::OrderMismatch);
            };
            same(
                generation.next().map_err(|_| Error::Exhausted)? == actor.generation()
                    && home_before.incoming() == 0,
            )?;
            cells::lease(&mut out.job, before.expect("free"), actor, status, lsn, 1)?;
            set_budget(&mut out.job, 1, TERMINAL_BYTES);
            out.status = Some(home(actor, TransactionState::Active, None, 0)?);
        }
        Change::Reserve(plan) => {
            let actor = actor.expect("actor");
            let location = location.expect("location");
            let summary = summary.as_ref().expect("actor");
            let spec = tx_spec.as_ref().expect("descriptor");
            let status = occupied(status_before.expect("status"), actor)?;
            require(
                status.state() == TransactionState::Active
                    && summary.reserved.is_none()
                    && plan.statement != 0
                    && plan.statement >= summary.statement,
            )?;
            let table = inputs.table.ok_or(Error::InvalidInput)?;
            same(table.handle == plan.table)?;
            let leaf = table_leaf(inputs, table)?;
            let canonical =
                row_record::decode_canonical_row_with_schema(plan.canonical, table.schema)?;
            if !(5..=crate::storage::overflow::INLINE_ROW_BYTES).contains(&plan.canonical.len()) {
                return Err(Error::Unsupported);
            }
            let cell_bytes = 180 + plan.canonical.len();
            // The planned U is the next generation of an actual Free slot.
            same(
                !cells::same_address(plan.container, location.page())
                    && !cells::same_address(plan.container, table.anchor)
                    && !cells::same_address(plan.container, leaf),
            )?;
            let undo_bytes = inputs.undo.ok_or(Error::InvalidInput)?;
            let cell = cells::undo_before(undo_bytes, namespace, plan.container, frame)?;
            let old = undo_record::decode_free_undo_record(cell, cells::undo_generation(cell)?)?;
            let generation = old.next().map_err(|_| Error::Exhausted)?;
            let undo = UndoRef::new(plan.container, 0, generation)?;
            let row = RowId::new(root.row_high().checked_add(1).ok_or(Error::Exhausted)?)
                .map_err(|_| Error::Exhausted)?;
            let sequence = root.undo_high().checked_add(1).ok_or(Error::Exhausted)?;
            let mutation = spec
                .last_mutation_sequence
                .checked_add(1)
                .ok_or(Error::Exhausted)?;
            let h = head.is_some();
            let mut record = [0; MAX_RECORD];
            let length = row_record::encode_row_record_with_schema(
                RecordExpectation { namespace, row },
                table.schema,
                RecordVersion {
                    row,
                    stamp: VersionStamp::Transaction(actor),
                    logical: LogicalRowVersion::new(actor.id(), mutation)?,
                    undo: Some(undo),
                },
                RowPayload::Inline(canonical),
                &mut record,
            )?;
            same(length + 12 == cell_bytes)?;
            reserve_leaf(inputs, table, leaf, row, &record[..length], &mut out.cell)?;
            out.cell.fill(0);
            let projection = cells::projection(plan.table, row, Some(&record[..length]))?;
            let job = &mut out.job;
            cells::lease(job, before.expect("free"), actor, location, lsn, 3)?;
            undo.encode(&mut job[128..168])?;
            put(job, 168, sequence);
            put(job, 176, plan.table.object.get());
            put(job, 184, plan.table.generation.get());
            put(job, 192, plan.table.schema_epoch.get());
            table.anchor.encode(&mut job[200..224])?;
            leaf.encode(&mut job[224..248])?;
            put(job, 248, plan.statement);
            put(job, 256, mutation);
            cells::put32(job, 280, cell_bytes as u32);
            job[288..320].copy_from_slice(&projection);
            put(job, 320, row.get());
            set_budget(job, 5, 311_608 + 2 * cell_bytes as u64 + 128 * u64::from(h));
            undo_record::encode_free_undo_record(generation, &mut out.undo)?;
            out.issued = Some(Issued {
                row,
                undo,
                sequence,
                mutation,
            });
            out.pages[UNDO] = Some(Changed {
                identity: cells::fixed_identity(namespace, plan.container, 3),
                kind: PageKind::Undo,
                before: undo_bytes,
                before_slot: Some(0),
                after_slot: Some(0),
            });
            credit = true;
            core = Core {
                statement: plan.statement,
                mutation,
                sequence,
                current: Some(undo),
                ..Core::default()
            };
        }
        Change::Cancel { .. } => {
            let job = before.expect("selected");
            let actor = actor.expect("actor");
            require(job.state() == JobState::RootReserved)?;
            let status = occupied(status_before.expect("status"), actor)?;
            require(status.state() == TransactionState::Active)?;
            let undo = job.undo().ok_or(Error::IdentityMismatch)?;
            let bytes = inputs.undo.ok_or(Error::InvalidInput)?;
            let cell = cells::undo_before(bytes, namespace, undo.page(), frame)?;
            undo_record::decode_free_undo_record(cell, undo.generation())?;
            require(cells::page_lsn(bytes) == job.lease_lsn())?;
            item = Some(cells::undo_item(job)?);
            row::free_job(job.generation(), &mut out.job);
            credit = true;
            core = Core {
                statement: job.statement(),
                mutation: job.mutation(),
                sequence: job.undo_sequence(),
                current: Some(undo),
                ..Core::default()
            };
        }
        Change::RollbackStart { statement, .. } => {
            let actor = actor.expect("actor");
            let summary = summary.as_ref().expect("actor");
            let home_before = status_before.expect("status");
            let status = occupied(home_before, actor)?;
            require(summary.reserved.is_none())?;
            let rollback = match statement {
                None => {
                    require(
                        status.state() == TransactionState::Active
                            || matches!(
                                status.state(),
                                TransactionState::Aborting(RollbackState::Statement { .. })
                            ),
                    )?;
                    RollbackState::Transaction { next: head }
                }
                Some(boundary) => {
                    require(
                        status.state() == TransactionState::Active
                            && boundary.sequence != 0
                            && tx_spec.as_ref().expect("descriptor").remaining[3].steps == 3,
                    )?;
                    require(boundary.mark.is_none() || head.is_some())?;
                    RollbackState::Statement {
                        target: boundary.mark,
                        next: head,
                    }
                }
            };
            let sequence = head_sequence(inputs, actor, head)?;
            if let Some(undo) = head {
                // The retained head U is Live: proof only, no Undo native.
                let job = root_of(inputs, actor, undo)?;
                require(job.state() == JobState::RootRetained)?;
                let previous = cells::predecessor(original, namespace, actor, job.undo_sequence())?;
                let edge = match previous {
                    Some(previous) => UndoEdge::Linked(cells::link(previous)?),
                    None => UndoEdge::Never,
                };
                let bytes = inputs.undo.ok_or(Error::InvalidInput)?;
                let cell = cells::undo_before(bytes, namespace, undo.page(), frame)?;
                require(cells::page_lsn(bytes) >= job.last_step())?;
                cells::check_material(
                    namespace,
                    job,
                    UndoCell::Live(cells::material(job, edge, 2, 0)?),
                    cell,
                )?;
            } else {
                same(inputs.undo.is_none())?;
            }
            out.status = Some(home(
                actor,
                TransactionState::Aborting(rollback),
                head,
                home_before.incoming(),
            )?);
            core = Core {
                statement: statement.map_or(0, |boundary| boundary.sequence),
                sequence,
                current: head,
                next: head,
                target: statement.and_then(|boundary| boundary.mark),
                ..Core::default()
            };
        }
        Change::StatementFinish { statement, .. } => {
            let actor = actor.expect("actor");
            let home_before = status_before.expect("status");
            let status = occupied(home_before, actor)?;
            require(
                statement.sequence != 0
                    && status.state()
                        == TransactionState::Aborting(RollbackState::Statement {
                            target: head,
                            next: head,
                        })
                    && statement.mark == head,
            )?;
            out.status = Some(home(
                actor,
                TransactionState::Active,
                head,
                home_before.incoming(),
            )?);
            core = Core {
                statement: statement.sequence,
                sequence: head_sequence(inputs, actor, head)?,
                current: head,
                next: head,
                target: head,
                ..Core::default()
            };
        }
        Change::Commit { .. } | Change::AbortEnd { .. } => {
            let job = before.expect("selected");
            let actor = actor.expect("actor");
            let summary = summary.as_ref().expect("actor");
            let home_before = status_before.expect("status");
            let status = occupied(home_before, actor)?;
            require(job.state() == JobState::TerminalLeased && summary.reserved.is_none())?;
            let committed = op == Operation::Commit;
            let after = if committed {
                require(status.state() == TransactionState::Active)?;
                q = root.commit_c().checked_add(1).ok_or(Error::Exhausted)?;
                home(
                    actor,
                    TransactionState::Committed(CommitSeq::new(q).map_err(|_| Error::Exhausted)?),
                    head,
                    home_before.incoming(),
                )?
            } else {
                require(
                    status.state()
                        == TransactionState::Aborting(RollbackState::Transaction { next: None })
                        && head.is_none()
                        && home_before.incoming() == 0
                        && summary.retained == 0,
                )?;
                home(actor, TransactionState::Aborted, None, 0)?
            };
            let jobs = &mut out.job;
            jobs[6] = 2;
            jobs[120] = if committed { 1 } else { 2 };
            put(jobs, 72, lsn);
            if committed {
                put(jobs, 264, q);
            }
            jobs[328..360].copy_from_slice(&sender.ok_or(Error::InvalidInput)?);
            out.status = Some(after);
            core.q = q;
        }
        Change::Forward { .. } | Change::Clr { .. } | Change::Prune { .. } => {
            let job = before.expect("selected");
            let actor = actor.expect("actor");
            let home_before = status_before.expect("status");
            let status = occupied(home_before, actor)?;
            let undo = job.undo().ok_or(Error::IdentityMismatch)?;
            let previous = cells::predecessor(original, namespace, actor, job.undo_sequence())?;
            let h = previous.is_some();
            require(job.has_previous_transaction() == Some(h))?;
            let next = previous.map(|job| job.undo().expect("root"));
            let incoming = home_before.incoming();
            let edge = live_material(inputs, job, previous)?;
            let undo_bytes = inputs.undo.ok_or(Error::InvalidInput)?;
            let undo_cell = cells::undo_before(undo_bytes, namespace, undo.page(), frame)?;
            let (state, new_head, count) = match op {
                Operation::Forward => {
                    let Change::Forward { statement, .. } = inputs.change else {
                        unreachable!("matched Forward");
                    };
                    require(
                        job.state() == JobState::RootReserved
                            && status.state() == TransactionState::Active
                            && head == next
                            && statement.sequence == job.statement(),
                    )?;
                    require(cells::page_lsn(undo_bytes) == job.lease_lsn())?;
                    undo_record::decode_free_undo_record(undo_cell, undo.generation())?;
                    cells::encode_material(
                        namespace,
                        job,
                        UndoCell::Live(cells::material(job, edge, 2, 0)?),
                        &mut out.undo,
                    )?;
                    item = Some(cells::undo_item(job)?);
                    let jobs = &mut out.job;
                    jobs[6] = 2;
                    put(jobs, 72, lsn);
                    jobs[328..360].copy_from_slice(&sender.ok_or(Error::InvalidInput)?);
                    credit = true;
                    (
                        TransactionState::Active,
                        Some(undo),
                        incoming.checked_add(1).ok_or(Error::Exhausted)?,
                    )
                }
                _ => {
                    require(job.state() == JobState::RootRetained && head == Some(undo))?;
                    require(cells::page_lsn(undo_bytes) >= job.last_step())?;
                    cells::check_material(
                        namespace,
                        job,
                        UndoCell::Live(cells::material(job, edge, 2, 0)?),
                        undo_cell,
                    )?;
                    let cut = if h { UndoEdge::Cut } else { UndoEdge::Never };
                    cells::encode_material(
                        namespace,
                        job,
                        UndoCell::Releasing(cells::material(job, cut, 0, 0)?),
                        &mut out.undo,
                    )?;
                    let state = if op == Operation::Clr {
                        let Change::Clr { statement, .. } = inputs.change else {
                            unreachable!("matched CLR");
                        };
                        match status.state() {
                            TransactionState::Aborting(RollbackState::Statement {
                                target,
                                next: n,
                            }) => {
                                require(
                                    n == head
                                        && target != Some(undo)
                                        && statement.is_some_and(|boundary: StatementBoundary| {
                                            boundary.sequence == job.statement()
                                                && boundary.mark == target
                                        }),
                                )?;
                                statement_target = target;
                                TransactionState::Aborting(RollbackState::Statement {
                                    target,
                                    next,
                                })
                            }
                            TransactionState::Aborting(RollbackState::Transaction { next: n }) => {
                                require(n == head && statement.is_none())?;
                                TransactionState::Aborting(RollbackState::Transaction { next })
                            }
                            _ => return Err(Error::OrderMismatch),
                        }
                    } else {
                        // Committed Prune starts at the current head (consumers.md:212).
                        let terminal = summary
                            .as_ref()
                            .and_then(|summary| summary.terminal)
                            .ok_or(Error::IdentityMismatch)?;
                        let terminal = decode_job(inputs, terminal)?;
                        let TransactionState::Committed(commit) = status.state() else {
                            return Err(Error::OrderMismatch);
                        };
                        require(
                            terminal.state() == JobState::TerminalReady
                                && terminal.outcome() == 1
                                && terminal.commit_q() == commit.get(),
                        )?;
                        q = commit.get();
                        status.state()
                    };
                    let jobs = &mut out.job;
                    jobs[6] = if op == Operation::Prune { 3 } else { 4 };
                    jobs[200..248].fill(0);
                    jobs[280..320].fill(0);
                    put(jobs, 272, lsn);
                    if op == Operation::Prune {
                        put(jobs, 264, q);
                    }
                    set_budget(jobs, 4, 4 * RELEASE_BYTES);
                    (
                        state,
                        next,
                        incoming.checked_sub(1).ok_or(Error::OrderMismatch)?,
                    )
                }
            };
            out.status = Some(home(actor, state, new_head, count)?);
            out.pages[UNDO] = Some(Changed {
                identity: cells::fixed_identity(namespace, undo.page(), 3),
                kind: PageKind::Undo,
                before: undo_bytes,
                before_slot: Some(0),
                after_slot: Some(0),
            });
            let (_, _, before_digest, after_digest) =
                prepare_leaf(inputs, out, job, actor, inserted, q)?;
            out.row_cell = job.cell_bytes() as usize;
            out.predecessor = h;
            graph_links = Some((
                UndoLink {
                    reference: undo,
                    sequence: job.undo_sequence(),
                },
                previous.map(cells::link).transpose()?,
                incoming,
            ));
            core = Core {
                statement: if op == Operation::Prune {
                    0
                } else {
                    job.statement()
                },
                mutation: if op == Operation::Prune {
                    0
                } else {
                    job.mutation()
                },
                sequence: job.undo_sequence(),
                current: Some(undo),
                next,
                target: statement_target,
                digests: Some((before_digest, after_digest)),
                ..Core::default()
            };
        }
        Change::ReleaseRole { .. } | Change::RootFree { .. } => {
            let job = before.expect("selected");
            let summary = summary.as_ref().expect("actor");
            let terminal = decode_job(inputs, summary.terminal.ok_or(Error::IdentityMismatch)?)?;
            require(terminal.state() == JobState::TerminalReady && job.state().is_releasing())?;
            let cursor = job.release_cursor();
            require(if op == Operation::RootFree {
                cursor == 3
            } else {
                cursor < 3
            })?;
            let undo = job.undo().ok_or(Error::IdentityMismatch)?;
            let bytes = inputs.undo.ok_or(Error::InvalidInput)?;
            let cell = cells::undo_before(bytes, namespace, undo.page(), frame)?;
            require(cells::page_lsn(bytes) >= job.last_step())?;
            // Missing before has Never/Cut at cursor0/1 and Released from 2.
            let transaction = match (cursor, cell[326]) {
                (0 | 1, 0) => UndoEdge::Never,
                (0 | 1, 2) => UndoEdge::Cut,
                (2 | 3, 3) => UndoEdge::Released,
                _ => return Err(Error::OrderMismatch),
            };
            cells::check_material(
                namespace,
                job,
                UndoCell::Releasing(cells::material(job, transaction, 0, cursor)?),
                cell,
            )?;
            if op == Operation::RootFree {
                undo_record::encode_free_undo_record(undo.generation(), &mut out.undo)?;
                row::free_job(job.generation(), &mut out.job);
            } else {
                let next = cursor + 1;
                cells::encode_material(
                    namespace,
                    job,
                    UndoCell::Releasing(cells::material(
                        job,
                        if next >= 2 {
                            UndoEdge::Released
                        } else {
                            transaction
                        },
                        0,
                        next,
                    )?),
                    &mut out.undo,
                )?;
                out.job[121] = next;
                set_budget(
                    &mut out.job,
                    u64::from(3 - cursor),
                    u64::from(3 - cursor) * RELEASE_BYTES,
                );
            }
            out.pages[UNDO] = Some(Changed {
                identity: cells::fixed_identity(namespace, undo.page(), 3),
                kind: PageKind::Undo,
                before: bytes,
                before_slot: Some(0),
                after_slot: Some(0),
            });
            core = Core {
                sequence: job.undo_sequence(),
                current: Some(undo),
                ..Core::default()
            };
        }
        Change::TerminalFinish { .. } => {
            let job = before.expect("selected");
            let actor = actor.expect("actor");
            let summary = summary.as_ref().expect("actor");
            let home_before = status_before.expect("status");
            let status = occupied(home_before, actor)?;
            require(
                job.state() == JobState::TerminalReady
                    && summary.roots == 0
                    && head.is_none()
                    && home_before.incoming() == 0
                    && matches!(
                        status.state(),
                        TransactionState::Committed(_) | TransactionState::Aborted
                    ),
            )?;
            row::free_job(job.generation(), &mut out.job);
            out.status = Some(StatusHome::new(
                TransactionStatusCell::Free(actor.generation()),
                0,
            )?);
        }
    }
    if out.job[6] != 0 && selected.is_some() {
        put(&mut out.job, 376, lsn);
    }
    if selected.is_some() {
        row::decode_job(&out.job, namespace)?;
    }
    out.frame_bytes = op.frame_bytes(out.row_cell, out.predecessor)?;

    // Root after (not for the status-patch ops) and the after census.
    if has_root(op) {
        let cells_after = match selected {
            Some(ordinal) => original.replace(ordinal, &out.job)?,
            None => original,
        };
        let root_cell = &mut out.root;
        root_cell.copy_from_slice(root.bytes());
        put(
            root_cell,
            64,
            root.revision().checked_add(1).ok_or(Error::Exhausted)?,
        );
        put(root_cell, 80, lsn);
        let next = |value: u64| value.checked_add(1).ok_or(Error::Exhausted);
        match op {
            Operation::AgingStart => {
                root_cell[6] = 1;
                SystemWorkRef::new(namespace, SystemWorkKind::Aging, 1, cells::one())
                    .map_err(|_| Error::IdentityMismatch)?
                    .encode(&mut root_cell[16..64])
                    .map_err(|_| Error::IdentityMismatch)?;
            }
            Operation::Begin => put(root_cell, 88, next(root.issued())?),
            Operation::Reserve => {
                let issued = out.issued.expect("reserve");
                put(root_cell, 88, next(root.issued())?);
                put(root_cell, 392, issued.sequence);
                put(root_cell, 432, issued.row.get());
            }
            Operation::Cancel => put(root_cell, 384, next(root.canceled())?),
            Operation::Commit => put(root_cell, 408, q),
            Operation::RootFree | Operation::TerminalFinish => {
                put(root_cell, 376, next(root.completed())?)
            }
            _ => {}
        }
        if !op.handoff() {
            let cost = if op == Operation::AgingStart {
                AGING_START_BYTES as u64
            } else {
                out.frame_bytes as u64
            };
            put(
                root_cell,
                416,
                root.consumed_wal()
                    .checked_add(cost)
                    .ok_or(Error::Exhausted)?,
            );
            if let Some(ordinal) = selected {
                put(root_cell, 72, ((ordinal + 1) % JOB_COUNT) as u64);
            }
        }
        let (masks, digest) = row::summarize_jobs(cells_after, namespace, &mut aggregate)?;
        for (index, mask) in masks.into_iter().enumerate() {
            put(root_cell, 96 + index * 8, mask);
        }
        root_cell[128..160].copy_from_slice(&digest);
        root_cell[160..192].copy_from_slice(&content_digest(&aggregate));
        let after_root = row::decode_root(&out.root, namespace)?;
        row::verify_selected_census(after_root, namespace, cells_after, &mut aggregate)?;
        if matches!(
            op,
            Operation::Prune
                | Operation::ReleaseRole
                | Operation::RootFree
                | Operation::TerminalFinish
        ) {
            core.cursors = (root.cursor(), after_root.cursor());
        }
        out.pages[ROOT] = Some(Changed {
            identity: cells::genesis_identity(namespace, RetentionGenesisPage::Root)?,
            kind: PageKind::RetentionRoot,
            before: inputs.root,
            before_slot: Some(0),
            after_slot: Some(0),
        });
    }
    if let Some(ordinal) = selected {
        let index = (ordinal / 15) as u8;
        let bytes = inputs.jobs_page.ok_or(Error::InvalidInput)?;
        let view = row::decode_jobs_page(bytes, namespace, index)?;
        require(view.header().lsn.0 <= frame.previous_lsn)?;
        for slot in 0..view.len() {
            same(view.cell(slot) == Some(&inputs.jobs[usize::from(index) * 15 + slot][..]))?;
        }
        let slot = Some((ordinal % 15) as u16);
        out.pages[JOB] = Some(Changed {
            identity: cells::genesis_identity(namespace, RetentionGenesisPage::Jobs { index })?,
            kind: PageKind::RetentionJobs,
            before: bytes,
            before_slot: slot,
            after_slot: slot,
        });
    }
    if out.status.is_some() {
        let location = location.expect("location");
        out.pages[STATUS] = Some(Changed {
            identity: cells::fixed_identity(namespace, location.page(), 2),
            kind: PageKind::TransactionStatus,
            before: inputs.status.ok_or(Error::InvalidInput)?,
            before_slot: Some(0),
            after_slot: Some(0),
        });
    }

    // Transaction descriptor after (XHG ordinal0, Tx-owned XSG).
    if is_tx(op) {
        let actor = actor.expect("actor");
        let location = location.expect("location");
        // Reserve/Cancel keep the Status home: its before is the after.
        let status_after = out.status.or(status_before).ok_or(Error::InvalidInput)?;
        let TransactionStatusCell::Occupied(core_after) = status_after.core() else {
            return Err(Error::OrderMismatch);
        };
        let cells_after = match selected {
            Some(ordinal) => original.replace(ordinal, &out.job)?,
            None => original,
        };
        let frame_bytes = out.frame_bytes as u64;
        let mut spec = match tx_spec {
            None => DescriptorSpec {
                owner: OwnerState::Transaction(core_after),
                chain: OwnerChain {
                    apply: lsn,
                    previous: 0,
                    birth: lsn,
                    groups: 1,
                },
                profile: profile_reference(profile),
                inventory: cells::empty_claim(lsn),
                consumed_wal: frame_bytes,
                consumed_control: 0,
                remaining: Default::default(),
                minimum_wal_lsn: lsn,
                last_mutation_sequence: 0,
            },
            Some(mut spec) => {
                spec.chain = OwnerChain {
                    apply: lsn,
                    previous: spec.chain.apply,
                    birth: spec.chain.birth,
                    groups: spec.chain.groups.checked_add(1).ok_or(Error::Exhausted)?,
                };
                spec.consumed_wal = spec
                    .consumed_wal
                    .checked_add(frame_bytes)
                    .ok_or(Error::Exhausted)?;
                match op {
                    Operation::Reserve => {
                        let issued = out.issued.expect("reserve");
                        let reserved = row::decode_job(&out.job, namespace)?;
                        spec.inventory = cells::item_claim(&cells::undo_item(reserved)?, lsn);
                        spec.last_mutation_sequence = issued.mutation;
                    }
                    Operation::Forward | Operation::Cancel => {
                        let reserved = before.expect("selected");
                        same(
                            spec.inventory
                                == cells::item_claim(
                                    &cells::undo_item(reserved)?,
                                    reserved.lease_lsn(),
                                ),
                        )?;
                        spec.inventory = cells::empty_claim(lsn);
                    }
                    _ => {}
                }
                spec
            }
        };
        spec.owner = OwnerState::Transaction(core_after);
        spec.remaining =
            ledger::transaction_remaining(profile, status_after, location, cells_after, credit)?;
        obligation::encode_descriptor(&spec, &mut out.descriptors[0])?;
        ledger::validate_transaction(
            &out.descriptors[0],
            profile,
            status_after,
            location,
            cells_after,
        )?;
        out.header.txn = Some(actor.id());
        out.header.previous_txn_lsn = tx_spec.map_or(0, |spec| spec.chain.apply);
    }

    // Prune keeps its actor live without a Tx descriptor: the after Status
    // must close over the after Job census as the ledger observer reads it.
    if op == Operation::Prune {
        ledger::validate_actor(
            profile,
            out.status.ok_or(Error::InvalidInput)?,
            location.ok_or(Error::InvalidInput)?,
            original.replace(selected.ok_or(Error::InvalidInput)?, &out.job)?,
        )?;
    }

    // Graph and inventory in physical native order.
    let order = frame::physical_order(out)?;
    let index = |role: usize| -> u16 {
        order
            .iter()
            .position(|candidate| *candidate == Some(role))
            .map_or(u16::MAX, |at| at as u16)
    };
    if let Some((undo, previous, incoming)) = graph_links {
        graph::references(
            op,
            actor.expect("actor"),
            undo,
            previous,
            incoming,
            graph::Indices {
                status: index(STATUS),
                undo: index(UNDO),
                leaf: index(LEAF),
            },
            &mut out.aux,
        )?;
    }
    if matches!(
        op,
        Operation::Reserve | Operation::Forward | Operation::Cancel
    ) {
        let item = match item {
            Some(item) => item,
            None => cells::undo_item(row::decode_job(&out.job, namespace)?)?,
        };
        graph::inventory(op, &item, &mut out.aux)?;
    }
    require(out.aux.len == op.shape(out.row_cell, out.predecessor)?.auxiliary_bytes())?;
    graph::encode_core(op, &core, &mut out.core)?;
    Ok(())
}

/// The retained root whose U is `undo`.
fn root_of<'a>(inputs: &Inputs<'a>, actor: TxnRef, undo: UndoRef) -> Result<JobView<'a>> {
    let namespace = inputs.profile.spec().namespace;
    for cell in inputs.jobs {
        let job = row::decode_job(cell, namespace)?;
        if job.source() == Some(actor) && job.state().is_root() && job.undo() == Some(undo) {
            return Ok(job);
        }
    }
    Err(Error::IdentityMismatch)
}
fn head_sequence(inputs: &Inputs<'_>, actor: TxnRef, head: Option<UndoRef>) -> Result<u64> {
    head.map_or(Ok(0), |undo| {
        Ok(root_of(inputs, actor, undo)?.undo_sequence())
    })
}

/// Place every prepared after image into its fixed role.
pub(super) fn write_pages(
    inputs: &Inputs<'_>,
    prepared: &Prepared<'_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<()> {
    let namespace = inputs.profile.spec().namespace;
    let lsn = prepared.header.lsn;
    if prepared.pages[ROOT].is_some() {
        row::encode_root_page(&prepared.root, namespace, &mut after[ROOT])?;
    }
    if let (Some(_), Some(ordinal)) = (prepared.pages[JOB], prepared.ordinal) {
        let base = ordinal / 15 * 15;
        let count = if base == 60 { JOB_COUNT - 60 } else { 15 };
        let mut cells: [&[u8]; 15] = [&[]; 15];
        for (slot, cell) in cells[..count].iter_mut().enumerate() {
            *cell = if base + slot == ordinal {
                &prepared.job
            } else {
                &inputs.jobs[base + slot]
            };
        }
        row::encode_jobs_page(
            namespace,
            (base / 15) as u8,
            lsn,
            &cells[..count],
            &mut after[JOB],
        )?;
    }
    for role in [STATUS, UNDO] {
        let Some(page) = prepared.pages[role] else {
            continue;
        };
        let header = PageHeader {
            identity: page.identity,
            kind: page.kind,
            level: 0,
            lsn: PageLsn(lsn),
        };
        if role == STATUS {
            let mut cell = [0; STATUS_HOME_BYTES];
            prepared
                .status
                .ok_or(Error::InvalidInput)?
                .encode(&mut cell)?;
            page::encode_page(header, &[&cell], &mut after[STATUS])?;
        } else {
            page::encode_page(header, &[&prepared.undo], &mut after[UNDO])?;
        }
    }
    if let (Some(page), Some((leaf_edit, expected, _))) = (prepared.pages[LEAF], prepared.leaf) {
        let view = tree_page::decode_tree_page(page.before, expected)?;
        let mut refs: [&[u8]; ROW_EDIT_CELLS] = [&[]; ROW_EDIT_CELLS];
        let key;
        let change = match leaf_edit {
            LeafEdit::Insert(cell) => edit::LeafEdit::Insert(cell),
            LeafEdit::Remove(row) => {
                key = row.get().to_be_bytes();
                edit::LeafEdit::Remove(&key)
            }
            LeafEdit::Replace => edit::LeafEdit::Replace(&prepared.cell[..prepared.cell_len]),
        };
        view.prepare_leaf_edit(change, &mut refs)
            .map_err(cells::edit_error)?
            .encode(PageLsn(lsn), &mut after[LEAF])?;
    }
    Ok(())
}

/// Aging after over the placed Root after page (`retention_obligation::row`).
pub(super) fn finish_aging(
    inputs: &Inputs<'_>,
    prepared: &mut Prepared<'_>,
    after: &[[u8; PAGE_BYTES]; 5],
) -> Result<()> {
    let op = prepared.op;
    if !has_root(op) {
        return Ok(());
    }
    let original = JobCells::original(inputs.jobs);
    let jobs = match prepared.ordinal {
        Some(ordinal) => original.replace(ordinal, &prepared.job)?,
        None => original,
    };
    let mut aggregate = [0; 640];
    let slot = usize::from(op.handoff());
    let (left, right) = prepared.descriptors.split_at_mut(1);
    let output = if slot == 0 {
        &mut left[0]
    } else {
        &mut right[0]
    };
    aging_row::next(
        AgingTransition {
            profile: inputs.profile,
            before_descriptor: prepared.aging_before,
            before_root: inputs.root,
            after_root: &after[ROOT],
            jobs,
        },
        &mut aggregate,
        output,
    )
    .map_err(cells::aging_error)
}

/// prepare's own frame: decoded views, summaries, specs and scratch.
pub(super) fn locals_bytes() -> usize {
    use std::mem::size_of;
    640 // aggregate
        + MAX_RECORD // Reserve/Prune record scratch
        + size_of::<[&[u8]; ROW_EDIT_CELLS]>()
        + 2 * size_of::<DescriptorSpec>()
        + size_of::<Option<DescriptorSpec>>()
        + 8 * size_of::<JobView<'_>>()
        + 4 * size_of::<Option<JobView<'_>>>()
        + 2 * size_of::<JobCells<'_>>()
        + size_of::<row::RootView<'_>>()
        + size_of::<(PageHeader, row::RootView<'_>)>()
        + 4 * size_of::<StatusHome>()
        + 2 * size_of::<TransactionStatus>()
        + size_of::<Core>()
        + size_of::<cells::ActorJobs>()
        + size_of::<LeafSource<'_>>()
        + size_of::<tree_page::TreePageView<'_>>()
        + size_of::<row_record::RowRecordView<'_>>()
        + size_of::<row_record::CanonicalRowView<'_>>()
        + 2 * size_of::<RecordVersion>()
        + 4 * size_of::<UndoRef>()
        + 4 * size_of::<UndoLink>()
        + size_of::<Issued>()
        + 2 * size_of::<[u8; 96]>()
        + 3 * size_of::<[u8; 32]>()
        + size_of::<[Option<usize>; 5]>()
        + 32 * size_of::<usize>()
}
/// Deepest nested callee of prepare: Row census/summary, the Aging decode
/// closure, the descriptor codec under the ledger observer, Undo material
/// encode+compare (two 432B cells) or the leaf edit/record codec.
pub(super) fn callee_bytes() -> usize {
    use std::mem::size_of;
    let ledger = size_of::<[obligation::RemainingBudget; 5]>()
        + size_of::<DescriptorSpec>()
        + 2 * size_of::<JobView<'_>>()
        + 512;
    let material = 2 * undo_record::UNDO_RECORD_BYTES
        + size_of::<undo_record::UndoRecord<'_>>()
        + 2 * size_of::<undo_record::UndoRecordExpectation<'_>>()
        + 1024;
    row::source_bytes()
        .max(crate::storage::retention_obligation::row::source_bytes())
        .max(obligation::descriptor_source_bytes() + ledger)
        .max(material)
        .max(tree_page_source_bytes())
}
pub(super) fn source_bytes() -> usize {
    locals_bytes() + callee_bytes() + 1536 // SHA closure of the root digests
}
fn tree_page_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<tree_page::TreePageView<'_>>()
        + size_of::<edit::LeafEditPlan<'_, '_>>()
        + size_of::<[usize; 16]>()
        + 2048
}
pub(super) fn write_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<[&[u8]; ROW_EDIT_CELLS]>()
        + size_of::<[&[u8]; 15]>()
        + STATUS_HOME_BYTES
        + 640
        + size_of::<AgingTransition<'_>>()
        + size_of::<tree_page::TreePageView<'_>>()
        + size_of::<PageHeader>()
        + crate::storage::retention_obligation::row::source_bytes()
            .max(row::source_bytes())
            .max(tree_page_source_bytes())
        + 1536
}
