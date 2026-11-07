//! Actual Row leaf effects connected to the same-frame Undo/Status observation.
//! Borrows original schema/Job/pages; creates no candidate or live authority.
//! A DELETE root's U before (stamp, old logical and value) is read here
//! through entry (a) with the table schema (dev3-row-delete.md#delete-undo);
//! `undo.rs` sees the rest of that U through entry (b).
use super::{Error, LayoutView, Operation, Result, operation::TOMBSTONE_CELL, u32_at, undo};
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation, TableDefinitionView},
    page::{self, FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef, PageView},
    retention_page::{
        JOB_COUNT,
        row::{self, JobCells, JobState, JobView, RowEffect},
    },
    row_record::{
        self, RecordExpectation, RecordVersion, RowPayload, RowRecordView, RowSchemaInput,
    },
    transaction_record::{LogicalRowVersion, VersionStamp},
    tree_page::{self, EntryPayload, TreeExpectation, TreePageView},
    undo_record::{
        self, UNDO_RECORD_BYTES, UndoCell, UndoEdge, UndoPayload, UndoTargetKind, UndoTargetView,
    },
    wal_frame,
};
use foundation::{
    codec::{ContentHasher, content_digest},
    identity::{ObjectHandle, RowId},
};

/// Original owner's stabilized Catalog observation. Bytes must not be obtained
/// from candidate after images. This carrier is not a CheckedTable/epoch token.
#[derive(Clone, Copy)]
pub(crate) struct TableInput<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) anchor: PageRef,
    pub(crate) catalog: &'a [u8],
    pub(crate) anchor_image: &'a [u8; PAGE_BYTES],
}
/// Original reservation's sealed canonical input, not a hash minted from after.
#[derive(Clone, Copy)]
pub(crate) struct SealedValue {
    pub(crate) length: usize,
    pub(crate) digest: [u8; 32],
}
#[derive(Clone, Copy)]
pub(crate) struct LeafImages<'a> {
    pub(crate) before: &'a [u8; PAGE_BYTES],
    pub(crate) after: &'a [u8; PAGE_BYTES],
}
#[derive(Clone, Copy)]
pub(crate) struct Inputs<'a> {
    pub(crate) context: undo::UndoContext,
    pub(crate) jobs: JobCells<'a>,
    pub(crate) ordinal: usize,
    pub(crate) undo: undo::UndoImages<'a>,
    pub(crate) status: undo::StatusProof<'a>,
    pub(crate) table: TableInput<'a>,
    pub(crate) leaf: LeafImages<'a>,
    pub(crate) sealed: Option<SealedValue>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafObservation {
    row: RowId,
    page: PageRef,
    present: (bool, bool),
    undo: undo::UndoObservation,
}
impl LeafObservation {
    pub(crate) fn row(self) -> RowId {
        self.row
    }
    pub(crate) fn page(self) -> PageRef {
        self.page
    }
    pub(crate) fn present(self) -> (bool, bool) {
        self.present
    }
    pub(crate) fn undo(self) -> undo::UndoObservation {
        self.undo
    }
}

/// Checks Forward/CLR/Prune's exact local Row change, schema, every actual row,
/// unchanged neighbors and native/redo bindings, DeleteForward/DeleteClr/
/// DeletePrune's tombstone, old or restored row and U before
/// (dev3-row-delete.md#delete-states), and UpdateForward/UpdateClr/
/// UpdatePrune's new, old, restored or restamped row and U before
/// (dev3-row-update.md#update-states). Undo/Status are checked against this
/// same frame, and the leaf reservation against the original Job bank
/// (#leaf-reservation). Original Catalog selection/index census, stable
/// epoch, after Job/descriptors/global census, source admission and
/// live/recovery authority remain the caller's obligations. No
/// allocation/I/O/page production.
pub(crate) fn observe(layout: LayoutView<'_>, input: Inputs<'_>) -> Result<LeafObservation> {
    let op = layout.operation();
    let role = op.role();
    if !matches!(role, Operation::Forward | Operation::Clr | Operation::Prune) {
        return Err(Error::Unsupported);
    }
    let effect = undo::opcode_effect(op);
    // INSERT's and UPDATE's Forward carry the sealed new value; a DELETE's
    // old value is the row it replaces.
    require(input.sealed.is_some() == (role == Operation::Forward && effect != RowEffect::Delete))?;
    let undo = undo::observe(
        layout,
        input.context,
        input.jobs,
        undo::UndoSource::Existing {
            ordinal: input.ordinal,
        },
        input.undo,
        Some(input.status),
    )?;
    let namespace = input.context.profile.spec().namespace;
    let job = row::decode_job(input.jobs.cell(input.ordinal), namespace)?;
    let table = input.table;
    require(job.object() == Some(table.handle))?;
    require(job.pair().is_some_and(|p| p[0] == table.anchor))?;
    let catalog = catalog_record::decode_catalog_record(
        table.catalog,
        CatalogExpectation {
            namespace,
            object: table.handle.object,
        },
    )?;
    catalog.validate_handle(table.handle)?;
    require(!catalog.header().retired && catalog.header().anchor == table.anchor)?;
    let CatalogDefinitionView::Table(schema) = catalog.definition() else {
        return Err(Error::Unsupported);
    };
    let identity = PageIdentity {
        database: namespace.database,
        page: table.anchor,
        owner: table.handle.object,
        owner_generation: table.handle.generation,
    };
    dynamic_page(table.anchor)?;
    let anchor_page = page::decode_page(table.anchor_image, identity, PageKind::TreeAnchor)?;
    valid_before_lsn(anchor_page.header().lsn.0, layout)?;
    require(anchor_page.header().level == 0)?;
    let anchor = anchor::decode_anchor(table.anchor_image, identity)?;
    if anchor.root().tree != TreeKind::Row || anchor.root().level != 0 {
        return Err(Error::Unsupported);
    }
    let leaf_ref = anchor.root().page;
    dynamic_page(leaf_ref)?;
    require(!same_address(leaf_ref, table.anchor))?;
    let protected = [
        Some(input.context.location.page()),
        Some(undo.reference().page()),
        undo.predecessor().map(|link| link.reference.page()),
    ];
    for reference in protected.into_iter().flatten() {
        require(!same_address(leaf_ref, reference))?;
        require(!same_address(table.anchor, reference))?;
    }
    let before_page = anchor.decode_root(input.leaf.before)?;
    let after_page = anchor.decode_root(input.leaf.after)?;
    let expected = TreeExpectation {
        identity: PageIdentity {
            page: leaf_ref,
            ..identity
        },
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    };
    let before = tree_page::decode_tree_page(input.leaf.before, expected)?;
    let after = tree_page::decode_tree_page(input.leaf.after, expected)?;
    valid_before_lsn(before.header().lsn.0, layout)?;
    require(after.header().lsn.0 == layout.header().lsn)?;
    if role != Operation::Forward {
        require(before.header().lsn.0 >= job.last_step())?;
    }
    let used = (
        validate_rows(&before, schema, namespace)?,
        validate_rows(&after, schema, namespace)?,
    );
    // The leaf reservation holds before the group and after it, where the
    // selected root's D changes by its restore bytes: DELETE's `C-180`,
    // UPDATE's `max(0, C-C')`, INSERT's 0 (#leaf-reservation, widened by
    // dev3-row-update.md#leaf-reservation). Forward adds it, Clr and Prune
    // take it back.
    let reserved = reservation(input.jobs, namespace, table.handle)?;
    if used.0 + reserved > tree_page::TREE_ENTRY_CAPACITY {
        return Err(Error::Corrupt);
    }
    let reserved = if role == Operation::Forward {
        reserved + job.restore_bytes()
    } else {
        reserved
            .checked_sub(job.restore_bytes())
            .ok_or(Error::Corrupt)?
    };
    if used.1 + reserved > tree_page::TREE_ENTRY_CAPACITY {
        return Err(Error::OrderMismatch);
    }
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    let key = row.get().to_be_bytes();
    let bpos = before.lower_bound(&key)?;
    let apos = after.lower_bound(&key)?;
    // DELETE replaces the row with its tombstone and back, and DeletePrune
    // removes the tombstone; UPDATE replaces a row with another (and back,
    // or restamped), so its key stays.
    let present = match effect {
        RowEffect::Insert => (role != Operation::Forward, role != Operation::Clr),
        RowEffect::Delete => (true, role != Operation::Prune),
        RowEffect::Update => (true, true),
    };
    require((bpos.equal, apos.equal) == present && bpos.index == apos.index)?;
    require(before.len() - usize::from(bpos.equal) == after.len() - usize::from(apos.equal))?;
    require(before_page.cell(0) == after_page.cell(0))?;
    // Ordered logical neighbors may move slots/offsets after insertion/removal.
    let mut a = 0;
    for b in 0..before.len() {
        if bpos.equal && b == bpos.index {
            continue;
        }
        if apos.equal && a == apos.index {
            a += 1;
        }
        require(before_page.cell(b + 1) == after_page.cell(a + 1))?;
        a += 1;
    }
    let b = if bpos.equal {
        Some(value(&before, bpos.index)?)
    } else {
        None
    };
    let a = if apos.equal {
        Some(value(&after, apos.index)?)
    } else {
        None
    };
    // The actor's record: INSERT's inline row, DELETE's tombstone, or
    // UPDATE's new inline row.
    let original = if role == Operation::Forward { a } else { b }.ok_or(Error::IdentityMismatch)?;
    let record = row_record::decode_row_record_with_schema(
        original,
        RecordExpectation { namespace, row },
        RowSchemaInput::Catalog(schema),
    )?;
    // The Job holds the planned tombstone's projection, INSERT's cell C and
    // row projection, or UPDATE's new cell C' and new row projection
    // (#job-effect, dev3-row-update.md#job-effect).
    require(
        record.version()
            == (RecordVersion {
                row,
                stamp: VersionStamp::Transaction(input.context.actor),
                logical: LogicalRowVersion::new(input.context.actor.id(), job.mutation())?,
                undo: Some(undo.reference()),
            })
            && original.len() + 12
                == match effect {
                    RowEffect::Insert => job.cell_bytes() as usize,
                    RowEffect::Delete => TOMBSTONE_CELL,
                    RowEffect::Update => job.new_cell() as usize,
                }
            && match (record.payload(), job.overflow()) {
                // An overflow root seals its head record with the root
                // zero, and only an overflow root has a head record
                // (dev3-row-overflow.md#overflow-row).
                (RowPayload::Overflow(_), true) => {
                    root_free_projection(table.handle, row, original)?
                }
                (RowPayload::Overflow(_), false) | (_, true) => {
                    return Err(Error::IdentityMismatch);
                }
                _ => projection(table.handle, row, Some(original))?,
            } == *job.projection_digest(),
    )?;
    let expected = RecordExpectation { namespace, row };
    let schema = RowSchemaInput::Catalog(schema);
    match effect {
        RowEffect::Insert => insert_rows(role, input, job, record, a, expected, schema)?,
        RowEffect::Delete => {
            require(record.payload() == RowPayload::Deleted)?;
            delete_rows(
                role,
                input,
                job,
                undo,
                expected,
                schema,
                if role == Operation::Forward { b } else { a },
            )?;
        }
        // The new row is sealed at Forward and restamped at Prune like an
        // INSERT's; the committed row U's before holds is the old row before
        // Forward and the restored row after Clr (dev3-row-update.md
        // #update-states).
        RowEffect::Update => {
            insert_rows(role, input, job, record, a, expected, schema)?;
            delete_rows(
                role,
                input,
                job,
                undo,
                expected,
                schema,
                match role {
                    Operation::Forward => b,
                    Operation::Clr => a,
                    _ => None,
                },
            )?;
        }
    }
    require(
        layout.core().before_digest() == Some(&projection(table.handle, row, b)?)
            && layout.core().after_digest() == Some(&projection(table.handle, row, a)?),
    )?;
    bind(
        layout,
        input.leaf,
        &before_page,
        &after_page,
        bpos.equal.then_some(bpos.index + 1),
        apos.equal.then_some(apos.index + 1),
    )?;
    Ok(LeafObservation {
        row,
        page: leaf_ref,
        present,
        undo,
    })
}
/// INSERT's or UPDATE's new inline row: its sealed value at Forward, and at
/// Prune the same value restamped `Committed(q)`. `op` is the INSERT role.
fn insert_rows(
    op: Operation,
    input: Inputs<'_>,
    job: JobView<'_>,
    record: RowRecordView<'_>,
    a: Option<&[u8]>,
    expected: RecordExpectation,
    schema: RowSchemaInput<'_>,
) -> Result<()> {
    let version = record.version();
    let (length, digest) = sealed_value(record, job.overflow())?;
    if let Some(sealed) = input.sealed {
        require(sealed.length == length && sealed.digest == digest)?;
    }
    if op == Operation::Prune {
        let new =
            row_record::decode_row_record_with_schema(a.ok_or(Error::Corrupt)?, expected, schema)?;
        let q = input.status.commit.ok_or(Error::InvalidInput)?;
        sealed_value(new, job.overflow())?;
        require(
            new.version().stamp == VersionStamp::Committed(q)
                && new.version().logical == version.logical
                && new.version().undo.is_none()
                && new.payload() == record.payload(),
        )?;
    }
    Ok(())
}
/// The value an INSERT row seals: an inline row's canonical length and
/// SHA-256, or an overflow row's head total and digest, which is the
/// canonical row's SHA-256 (dev3-row-overflow.md#overflow-row). The row's
/// payload kind is the Job's: a head record only for an overflow root.
fn sealed_value(row: RowRecordView<'_>, overflow: bool) -> Result<(usize, [u8; 32])> {
    match (row.payload(), overflow) {
        (RowPayload::Overflow(head), true) => Ok((head.total(), head.digest())),
        (RowPayload::Overflow(_), false) | (_, true) => Err(Error::IdentityMismatch),
        _ => {
            let canonical = inline(row)?;
            Ok((canonical.len(), content_digest(canonical)))
        }
    }
}
/// The XRP1 projection of an overflow head record with its root `[168,192)`
/// as zero (dev3-row-overflow.md#overflow-row), from a copy.
fn root_free_projection(table: ObjectHandle, row: RowId, record: &[u8]) -> Result<[u8; 32]> {
    let mut copy = [0; 232];
    require(record.len() == copy.len())?;
    copy.copy_from_slice(record);
    copy[168..192].fill(0);
    projection(table, row, Some(&copy))
}
/// DELETE's and UPDATE's U and committed row (dev3-row-delete.md
/// #delete-states, #delete-undo; dev3-row-update.md#update-states). U's Live
/// record (after Forward, before Clr/Prune), read through entry (a) with the
/// table schema, is the Job's with a RowValue before: `Committed(q_old)`, the
/// old logical and the inline old value of `C-180` bytes. `committed` (the
/// old row Forward replaces, the row Clr restores) is that before's row of
/// cell C: `Committed(q_old)`, the old logical, undo None and the same value.
/// Clr and Prune leave U Releasing with the before's stamp and logical and
/// without its value. `role` is the INSERT role.
fn delete_rows(
    role: Operation,
    input: Inputs<'_>,
    job: JobView<'_>,
    undo: undo::UndoObservation,
    expected: RecordExpectation,
    schema: RowSchemaInput<'_>,
    committed: Option<&[u8]>,
) -> Result<()> {
    let context = input.context;
    let expectation = undo::expectation(job, context)?;
    let live = undo::record_cell(
        context,
        undo.reference(),
        if role == Operation::Forward {
            input.undo.after
        } else {
            input.undo.before
        },
    )?;
    let live = undo_record::decode_undo_record(live, expectation, &schema)?.live_before()?;
    let (Some(stamp @ VersionStamp::Committed(_)), Some(logical), UndoPayload::Inline(value)) =
        (live.before_stamp, live.before_logical, live.payload)
    else {
        return Err(Error::IdentityMismatch);
    };
    let edge = undo.predecessor().map_or(UndoEdge::Never, UndoEdge::Linked);
    require(
        live == undo::delete_record(job, edge, 2, stamp, logical, live.payload)
            && value.len() + TOMBSTONE_CELL == job.cell_bytes() as usize,
    )?;
    if role != Operation::Prune {
        let bytes = committed.ok_or(Error::IdentityMismatch)?;
        let record = row_record::decode_row_record_with_schema(bytes, expected, schema)?;
        require(
            record.version()
                == (RecordVersion {
                    row: expected.row,
                    stamp,
                    logical,
                    undo: None,
                })
                && inline(record)? == value
                && bytes.len() + 12 == job.cell_bytes() as usize,
        )?;
    }
    if role != Operation::Forward {
        let cut = if undo.predecessor().is_some() {
            UndoEdge::Cut
        } else {
            UndoEdge::Never
        };
        let mut released = [0; UNDO_RECORD_BYTES];
        let length = undo_record::encode_undo_record(
            UndoCell::Releasing(undo::delete_record(
                job,
                cut,
                0,
                stamp,
                logical,
                UndoPayload::None,
            )),
            expectation,
            &schema,
            &mut released,
        )?;
        require(
            undo::record_cell(context, undo.reference(), input.undo.after)? == &released[..length],
        )?;
    }
    Ok(())
}
fn value<'a>(leaf: &TreePageView<'a>, at: usize) -> Result<&'a [u8]> {
    match leaf.entry(at)?.payload {
        EntryPayload::Value(bytes) => Ok(bytes),
        _ => Err(Error::Unsupported),
    }
}
fn inline(row: RowRecordView<'_>) -> Result<&[u8]> {
    match row.payload() {
        RowPayload::Inline(value) if (5..=2048).contains(&value.as_bytes().len()) => {
            Ok(value.as_bytes())
        }
        _ => Err(Error::Unsupported),
    }
}
/// Types every row: inline, or a DELETE's tombstone (`Transaction` stamp
/// with its U, dev3-row-delete.md#tombstone); a `Committed` or undo-less
/// tombstone is corrupt. Returns the rows' entry bytes (12-byte cell head,
/// record and 4-byte slot), the leaf reservation's measure.
fn validate_rows(
    leaf: &TreePageView<'_>,
    schema: TableDefinitionView<'_>,
    namespace: crate::api::Namespace,
) -> Result<usize> {
    let mut used = 0;
    for at in 0..leaf.len() {
        let entry = leaf.entry(at)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Corrupt)?;
        let bytes = value(leaf, at)?;
        let decoded = row_record::decode_row_record_with_schema(
            bytes,
            RecordExpectation { namespace, row },
            RowSchemaInput::Catalog(schema),
        )?;
        // An overflow row is an INSERT row with its head
        // (dev3-row-overflow.md#inline-gates).
        match decoded.payload() {
            RowPayload::Deleted => {
                let version = decoded.version();
                if !matches!(version.stamp, VersionStamp::Transaction(_)) || version.undo.is_none()
                {
                    return Err(Error::Corrupt);
                }
            }
            RowPayload::Overflow(_) => {}
            RowPayload::Inline(_) => {
                inline(decoded)?;
            }
        }
        used += bytes.len() + 16;
    }
    Ok(used)
}
/// D of the leaf reservation (dev3-row-delete.md#leaf-reservation, widened
/// by dev3-row-update.md#leaf-reservation): each Retained root of `table` in
/// the original bank adds its `JobView::restore_bytes` (DELETE `C-180`,
/// UPDATE `max(0, C-C')`).
fn reservation(
    jobs: JobCells<'_>,
    namespace: crate::api::Namespace,
    table: ObjectHandle,
) -> Result<usize> {
    let mut reserved = 0usize;
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(jobs.cell(ordinal), namespace)?;
        if job.state() == JobState::RootRetained
            && job.object().is_some_and(|object| {
                object.object == table.object && object.generation == table.generation
            })
        {
            reserved = reserved
                .checked_add(job.restore_bytes())
                .ok_or(Error::Corrupt)?;
        }
    }
    Ok(reserved)
}
fn projection(handle: ObjectHandle, row: RowId, value: Option<&[u8]>) -> Result<[u8; 32]> {
    let mut prefix = [0; 48];
    prefix[..8].copy_from_slice(b"XRP1\x01\0\x01\0");
    let target = UndoTargetView {
        kind: UndoTargetKind::Row,
        owner: handle.object,
        generation: handle.generation,
        entity: row.get(),
        arity: 0,
        key: &[],
    };
    undo_record::encode_undo_target(target, target, &mut prefix[8..40])?;
    prefix[40] = u8::from(value.is_some());
    let mut hash = ContentHasher::new();
    hash.update(&prefix);
    if let Some(value) = value {
        hash.update(value);
    }
    Ok(hash.digest())
}
fn bind(
    layout: LayoutView<'_>,
    images: LeafImages<'_>,
    before: &PageView<'_>,
    after: &PageView<'_>,
    bslot: Option<usize>,
    aslot: Option<usize>,
) -> Result<()> {
    let native = layout
        .native_records()
        .find(|n| n.kind() == 1)
        .ok_or(Error::Unsupported)?;
    for (page, slot, given_slot, given_offset, bytes) in [
        (
            before,
            bslot,
            native.before_slot(),
            native.before_offset(),
            native.before(),
        ),
        (
            after,
            aslot,
            native.after_slot(),
            native.after_offset(),
            native.after(),
        ),
    ] {
        require(
            given_slot.map(usize::from) == slot
                && given_offset.map(usize::from)
                    == slot.and_then(|s| page.cell_range(s)).map(|r| r.start)
                && bytes == slot.and_then(|s| page.cell(s)),
        )?;
    }
    let mut bytes = layout.redo();
    for at in 0..layout.shape().redo_count {
        let length = u32_at(bytes, 4) as usize;
        if at == native.redo_index() {
            let redo = wal_frame::decode_redo_record(layout.header(), &bytes[..length])?;
            let identity = before.header().identity;
            require(
                redo.target.page == identity.page
                    && redo.target.owner == identity.owner
                    && redo.target.owner_generation == identity.owner_generation
                    && redo.prior_lsn == before.header().lsn.0
                    && images.before[..64] == images.after[..64]
                    && images.after[72..16352] == *redo.data,
            )?;
            return Ok(());
        }
        bytes = &bytes[length..];
    }
    Err(Error::Corrupt)
}
fn valid_before_lsn(lsn: u64, layout: LayoutView<'_>) -> Result<()> {
    require(lsn != 0 && lsn <= layout.header().previous_lsn && lsn < layout.header().lsn)
}
fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}
fn dynamic_page(reference: PageRef) -> Result<()> {
    require(
        reference.file == FileId::new(0x100000001).expect("file")
            && (9..64).contains(&reference.page_no),
    )
}
fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::IdentityMismatch)
    }
}
