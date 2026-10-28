//! Actual Row leaf effects connected to the same-frame Undo/Status observation.
//! Borrows original schema/Job/pages; creates no candidate or live authority.
use super::{Error, LayoutView, Operation, Result, u32_at, undo};
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation, TableDefinitionView},
    page::{self, FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef, PageView},
    retention_page::row::{self, JobCells},
    row_record::{self, RecordExpectation, RowPayload, RowRecordView, RowSchemaInput},
    transaction_record::VersionStamp,
    tree_page::{self, EntryPayload, TreeExpectation, TreePageView},
    undo_record::{self, UndoTargetKind, UndoTargetView},
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
/// unchanged neighbors and native/redo bindings. Undo/Status are checked against
/// this same frame. Original Catalog selection/index census, stable epoch,
/// after Job/descriptors/global census, source admission and live/recovery
/// authority remain the caller's obligations. No allocation/I/O/page production.
pub(crate) fn observe(layout: LayoutView<'_>, input: Inputs<'_>) -> Result<LeafObservation> {
    let op = layout.operation();
    if !matches!(op, Operation::Forward | Operation::Clr | Operation::Prune) {
        return Err(Error::Unsupported);
    }
    require(input.sealed.is_some() == (op == Operation::Forward))?;
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
    if op != Operation::Forward {
        require(before.header().lsn.0 >= job.last_step())?;
    }
    validate_rows(&before, schema, namespace)?;
    validate_rows(&after, schema, namespace)?;
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    let key = row.get().to_be_bytes();
    let bpos = before.lower_bound(&key)?;
    let apos = after.lower_bound(&key)?;
    let present = (op != Operation::Forward, op != Operation::Clr);
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
    let original = if op == Operation::Forward { a } else { b }.ok_or(Error::IdentityMismatch)?;
    let record = row_record::decode_row_record_with_schema(
        original,
        RecordExpectation { namespace, row },
        RowSchemaInput::Catalog(schema),
    )?;
    let version = record.version();
    require(
        version.stamp == VersionStamp::Transaction(input.context.actor)
            && version.logical.transaction() == input.context.actor.id()
            && version.logical.mutation_sequence() == job.mutation()
            && version.undo == Some(undo.reference()),
    )?;
    require(
        original.len() + 12 == job.cell_bytes() as usize
            && projection(table.handle, row, Some(original))? == *job.projection_digest(),
    )?;
    let canonical = inline(record)?;
    if let Some(sealed) = input.sealed {
        require(sealed.length == canonical.len() && sealed.digest == content_digest(canonical))?;
    }
    if op == Operation::Prune {
        let new = row_record::decode_row_record_with_schema(
            a.ok_or(Error::Corrupt)?,
            RecordExpectation { namespace, row },
            RowSchemaInput::Catalog(schema),
        )?;
        let q = input.status.commit.ok_or(Error::InvalidInput)?;
        require(
            new.version().stamp == VersionStamp::Committed(q)
                && new.version().logical == version.logical
                && new.version().undo.is_none()
                && inline(new)? == canonical,
        )?;
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
fn validate_rows(
    leaf: &TreePageView<'_>,
    schema: TableDefinitionView<'_>,
    namespace: crate::api::Namespace,
) -> Result<()> {
    for at in 0..leaf.len() {
        let entry = leaf.entry(at)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Corrupt)?;
        let decoded = row_record::decode_row_record_with_schema(
            value(leaf, at)?,
            RecordExpectation { namespace, row },
            RowSchemaInput::Catalog(schema),
        )?;
        inline(decoded)?;
    }
    Ok(())
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
