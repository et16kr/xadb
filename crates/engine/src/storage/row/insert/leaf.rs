//! The RowLeaf part of INSERT. All inputs are byte observations; this module
//! has no access to a writer, pool, pin registry or installation capability.

use super::{InsertError, Result};
use crate::{
    api::Namespace,
    storage::{
        overflow::INLINE_ROW_BYTES,
        page::{self, PAGE_BYTES, PageLsn},
        row_record::{
            self, ROW_HEADER_BYTES, RecordExpectation, RecordVersion, RowPayload, RowSchemaInput,
        },
        transaction_record::VersionStamp,
        tree_page::{
            self, EntryPayload, TREE_ENTRY_CAPACITY, TreeExpectation, TreePageView,
            edit::{LeafEdit, LeafEditPlan},
        },
    },
};
use foundation::identity::RowId;

const MIN_CANONICAL_BYTES: usize = 5;
const MIN_ENTRY_COST: usize = ROW_HEADER_BYTES + MIN_CANONICAL_BYTES + 16;
// A validated inline Row occupies at least189 bytes including its slot. The
// scratch also contains metadata and the newly inserted cell (87 references).
const ROW_EDIT_CELLS: usize = TREE_ENTRY_CAPACITY / MIN_ENTRY_COST + 2;
const MAX_RECORD_BYTES: usize = ROW_HEADER_BYTES + INLINE_ROW_BYTES;
const MAX_CELL_BYTES: usize = 12 + MAX_RECORD_BYTES;

pub(super) struct LeafSource<'a> {
    view: TreePageView<'a>,
    pub(super) free_bytes: usize,
}

/// Check every existing row, including entries outside the insertion path.
/// This typed validation establishes the bounded reference scratch below.
pub(super) fn inspect<'a>(
    namespace: Namespace,
    expected: TreeExpectation,
    schema: RowSchemaInput<'_>,
    bytes: &'a [u8; PAGE_BYTES],
) -> Result<LeafSource<'a>> {
    if expected.kind != crate::storage::page::PageKind::RowLeaf
        || expected.level != 0
        || expected.arity != 0
    {
        return Err(InsertError::Unsupported);
    }
    if expected.identity.database != namespace.database {
        return Err(InsertError::IdentityMismatch);
    }
    let view = tree_page::decode_tree_page(bytes, expected)?;
    let mut free_bytes = TREE_ENTRY_CAPACITY;
    for index in 0..view.len() {
        let entry = view.entry(index)?;
        let key: [u8; 8] = entry
            .key
            .try_into()
            .map_err(|_| InsertError::IdentityMismatch)?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| InsertError::IdentityMismatch)?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(InsertError::Unsupported);
        };
        let record = row_record::decode_row_record_with_schema(
            value,
            RecordExpectation { namespace, row },
            schema,
        )?;
        if !matches!(record.payload(), RowPayload::Inline(_)) {
            return Err(InsertError::Unsupported);
        }
        free_bytes = free_bytes
            .checked_sub(value.len() + 16)
            .ok_or(InsertError::Capacity)?;
    }
    Ok(LeafSource { view, free_bytes })
}

pub(super) struct LeafInput<'a> {
    pub(super) namespace: Namespace,
    pub(super) expected: TreeExpectation,
    pub(super) before: &'a [u8; PAGE_BYTES],
    pub(super) schema: RowSchemaInput<'a>,
    pub(super) canonical: &'a [u8],
    pub(super) version: RecordVersion,
    pub(super) lsn: PageLsn,
}

/// Sizes and key of the candidate only, never an installed-row observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafBytes {
    pub(crate) key: [u8; 8],
    pub(crate) record_bytes: usize,
    pub(crate) cell_bytes: usize,
}

/// Write a complete RowLeaf after image. Errors preserve `output`; temporary
/// row/cell scratch may change and must not be interpreted as a WAL frame.
pub(super) fn produce(
    input: LeafInput<'_>,
    scratch: &mut [u8],
    output: &mut [u8; PAGE_BYTES],
) -> Result<LeafBytes> {
    let source = inspect(input.namespace, input.expected, input.schema, input.before)?;
    if input.lsn.0 <= source.view.header().lsn.0
        || input.version.undo.is_none()
        || !matches!(input.version.stamp, VersionStamp::Transaction(_))
    {
        return Err(InsertError::IdentityMismatch);
    }
    if input.canonical.len() > INLINE_ROW_BYTES {
        return Err(InsertError::Unsupported);
    }
    let row = row_record::decode_canonical_row_with_schema(input.canonical, input.schema)?;
    if input.canonical.len() + ROW_HEADER_BYTES + 16 > source.free_bytes {
        return Err(InsertError::Capacity);
    }
    if scratch.len() < MAX_RECORD_BYTES + MAX_CELL_BYTES {
        return Err(InsertError::Capacity);
    }
    let (record_space, cell_space) = scratch.split_at_mut(MAX_RECORD_BYTES);
    let record_bytes = row_record::encode_row_record_with_schema(
        RecordExpectation {
            namespace: input.namespace,
            row: input.version.row,
        },
        input.schema,
        input.version,
        RowPayload::Inline(row),
        record_space,
    )?;
    let key = input.version.row.get().to_be_bytes();
    let cell = tree_page::encode_leaf_cell(
        input.expected.kind,
        0,
        &key,
        &record_space[..record_bytes],
        cell_space,
    )?;
    let mut cells: [&[u8]; ROW_EDIT_CELLS] = [&[]; ROW_EDIT_CELLS];
    let plan = source
        .view
        .prepare_leaf_edit(LeafEdit::Insert(cell), &mut cells)?;
    if plan.needs_split() {
        return Err(InsertError::Capacity);
    }
    plan.encode(input.lsn, output)?;
    Ok(LeafBytes {
        key,
        record_bytes,
        cell_bytes: cell.len(),
    })
}

/// The actual current row of a CLR: one inline record whose version is the
/// caller's U-owned Missing INSERT. `cell` is (slot, offset, length).
pub(super) struct Current<'a> {
    pub(super) record: &'a [u8],
    pub(super) cell: (u16, u16, u16),
}

pub(super) fn current<'a>(
    namespace: Namespace,
    expected: TreeExpectation,
    schema: RowSchemaInput<'_>,
    bytes: &'a [u8; PAGE_BYTES],
    version: RecordVersion,
) -> Result<Current<'a>> {
    let source = inspect(namespace, expected, schema, bytes)?;
    let position = source.view.lower_bound(&version.row.get().to_be_bytes())?;
    if !position.equal {
        return Err(InsertError::IdentityMismatch);
    }
    let EntryPayload::Value(record) = source.view.entry(position.index)?.payload else {
        return Err(InsertError::Unsupported);
    };
    let view = row_record::decode_row_record_with_schema(
        record,
        RecordExpectation {
            namespace,
            row: version.row,
        },
        schema,
    )?;
    if view.version() != version {
        return Err(InsertError::IdentityMismatch);
    }
    let slot = position.index + 1;
    let range = page::decode_page(bytes, expected.identity, expected.kind)?
        .cell_range(slot)
        .ok_or(InsertError::IdentityMismatch)?;
    Ok(Current {
        record,
        cell: (slot as u16, range.start as u16, range.len() as u16),
    })
}

/// Physically remove the U-owned Missing INSERT row: the CLR RowLeaf after.
/// Errors preserve `output`; the caller already proved the exact current row.
pub(super) fn remove(
    namespace: Namespace,
    expected: TreeExpectation,
    schema: RowSchemaInput<'_>,
    before: &[u8; PAGE_BYTES],
    row: RowId,
    lsn: PageLsn,
    output: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let source = inspect(namespace, expected, schema, before)?;
    if lsn.0 <= source.view.header().lsn.0 {
        return Err(InsertError::IdentityMismatch);
    }
    let key = row.get().to_be_bytes();
    let mut cells: [&[u8]; ROW_EDIT_CELLS] = [&[]; ROW_EDIT_CELLS];
    let plan = source
        .view
        .prepare_leaf_edit(LeafEdit::Remove(&key), &mut cells)?;
    plan.encode(lsn, output)?;
    Ok(())
}

pub(super) fn local_source_carrier_bytes() -> usize {
    use std::mem::size_of;
    size_of::<LeafInput<'_>>()
        + size_of::<LeafSource<'_>>()
        + size_of::<Result<LeafSource<'_>>>()
        + size_of::<LeafEditPlan<'_, '_>>()
        + size_of::<std::result::Result<LeafEditPlan<'_, '_>, tree_page::edit::LeafEditError>>()
        + size_of::<[&[u8]; ROW_EDIT_CELLS]>()
        + size_of::<row_record::CanonicalRowView<'_>>()
        + size_of::<row_record::RowRecordView<'_>>()
        + size_of::<Result<LeafBytes>>()
        + size_of::<Result<Current<'_>>>()
        + 2 * size_of::<LeafBytes>()
        + 2 * size_of::<RecordExpectation>()
        + 2 * size_of::<RecordVersion>()
        + 12 * size_of::<usize>()
        + 6 * size_of::<&[u8]>()
        + 16
}
