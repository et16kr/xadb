//! First inline INSERT input and RowLeaf candidate production.
//! C0 only: these functions neither reserve an identity nor emit a complete
//! Forward/CLR group. Actual reserve, whole-group validation and apply belong
//! to the original disk owner; no leaf candidate is an installation grant.
#![allow(
    dead_code,
    reason = "actual Row reservation/group adapter is not supplied yet"
)]

mod leaf;
#[cfg(test)]
mod tests;

use crate::storage::{
    catalog_record::{self, CatalogDefinitionView, CatalogError, TableDefinitionView},
    disk::{
        RowInputPlan, RowOperation, RowPageRole, RowPreflightContext, RowProducerInput,
        RowProducerLane, RowSourceRole, RowValidationLane,
    },
    overflow::INLINE_ROW_BYTES,
    page::{PAGE_BYTES, PageError, PageKind, PageLsn},
    row_record::{self, RecordVersion, RowError, RowSchemaInput, ValueRef},
    transaction_record::{LogicalRowVersion, VersionStamp},
    tree_page::{TreeError, edit::LeafEditError},
};
use foundation::codec::content_digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertError {
    Unsupported,
    Capacity,
    IdentityMismatch,
    Row(RowError),
    Catalog(CatalogError),
    Page(PageError),
    Tree(TreeError),
    LeafEdit(LeafEditError),
}

impl From<RowError> for InsertError {
    fn from(value: RowError) -> Self {
        Self::Row(value)
    }
}
impl From<CatalogError> for InsertError {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}
impl From<PageError> for InsertError {
    fn from(value: PageError) -> Self {
        Self::Page(value)
    }
}
impl From<TreeError> for InsertError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}
impl From<LeafEditError> for InsertError {
    fn from(value: LeafEditError) -> Self {
        Self::LeafEdit(value)
    }
}
type Result<T> = std::result::Result<T, InsertError>;

fn schema<'a>(
    context: &RowPreflightContext,
    bytes: &'a [u8; PAGE_BYTES],
) -> Result<TableDefinitionView<'a>> {
    let (offset, length) = context.schema_cell();
    let start = usize::from(offset);
    let end = start + usize::from(length);
    let record = catalog_record::decode_catalog_record(
        bytes.get(start..end).ok_or(InsertError::IdentityMismatch)?,
        context.catalog_expected(),
    )?;
    record.validate_handle(context.table())?;
    let expected = context.leaf_expected();
    if record.header().retired
        || context.catalog_expected().object != context.table().object
        || context.profile().spec().namespace != context.namespace()
        || expected.identity.database != context.namespace().database
        || expected.identity.owner != context.table().object
        || expected.identity.owner_generation != context.table().generation
    {
        return Err(InsertError::IdentityMismatch);
    }
    let CatalogDefinitionView::Table(table) = record.definition() else {
        return Err(InsertError::Unsupported);
    };
    if context.profile().format() != 3
        || context.index_count() != 0
        || context.canonical_capacity() != INLINE_ROW_BYTES
        || expected.kind != PageKind::RowLeaf
        || expected.level != 0
        || expected.arity != 0
    {
        return Err(InsertError::Unsupported);
    }
    Ok(table)
}

/// Validate all input and capacity before changing the candidate prefix.
/// This helper is also the pure C0 test entry; its scalars grant no authority.
fn encode_input(
    schema: RowSchemaInput<'_>,
    values: &[ValueRef<'_>],
    leaf_free_bytes: usize,
    candidate: &mut [u8],
) -> Result<(usize, [u8; 32])> {
    let length = row_record::canonical_row_len_borrowed(schema, values)?;
    if length > INLINE_ROW_BYTES {
        return Err(InsertError::Unsupported);
    }
    // Four cell-header bytes, eight key bytes, the row header and a slot.
    if length + row_record::ROW_HEADER_BYTES + 16 > leaf_free_bytes {
        return Err(InsertError::Capacity);
    }
    let length = row_record::encode_canonical_row_borrowed(schema, values, candidate)?;
    Ok((length, content_digest(&candidate[..length])))
}

/// Produce the untrusted input plan from the original preflight's copies.
/// Source/identity failures precede value validation. Every failure preserves
/// candidate bytes; the owner still must recheck the plan before RowReserve.
pub(in crate::storage) fn validate_insert(
    values: &[ValueRef<'_>],
    lane: RowValidationLane<'_>,
) -> Result<RowInputPlan> {
    let (context, schema_bytes, leaf_bytes, candidate) = lane.parts();
    let schema = RowSchemaInput::Catalog(schema(&context, schema_bytes)?);
    let source = leaf::inspect(
        context.namespace(),
        context.leaf_expected(),
        schema,
        leaf_bytes,
    )?;
    if source.free_bytes != context.leaf_free_bytes() {
        return Err(InsertError::IdentityMismatch);
    }
    let (length, digest) = encode_input(schema, values, source.free_bytes, candidate)?;
    Ok(RowInputPlan::new(length, context.schema_epoch(), digest))
}

/// One incomplete component of a future Forward. There is deliberately no
/// conversion to RowProposal or a WAL/apply receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InsertLeafCandidate {
    pub(crate) after_index: u8,
    pub(crate) bytes: leaf::LeafBytes,
}

/// Reuse the reserve-sealed canonical bytes; no user values are accepted here.
/// The frame lane is temporary row/cell scratch, not a complete WAL frame.
/// Only the selected RowLeaf after image is written, and only on success.
pub(in crate::storage) fn produce_insert_leaf(
    lane: RowProducerLane<'_>,
) -> Result<InsertLeafCandidate> {
    let (context, _control, before, schema_bytes, frame, after, candidate) = lane.parts();
    if context.operation() != RowOperation::Forward {
        return Err(InsertError::Unsupported);
    }
    let RowProducerInput::Insert {
        canonical_len,
        canonical_digest,
    } = context.input()
    else {
        return Err(InsertError::Unsupported);
    };
    let canonical = candidate
        .get(..*canonical_len)
        .ok_or(InsertError::IdentityMismatch)?;
    if content_digest(canonical) != *canonical_digest {
        return Err(InsertError::IdentityMismatch);
    }
    let schema = RowSchemaInput::Catalog(schema(context.schema(), schema_bytes)?);
    let source = context
        .source(RowSourceRole::RowLeaf)
        .ok_or(InsertError::IdentityMismatch)?;
    let before_index = usize::from(source.before_index());
    if context.before_count() > 9 || before_index >= usize::from(context.before_count()) {
        return Err(InsertError::IdentityMismatch);
    }
    let expected = context.schema().leaf_expected();
    if source.expected() != (expected.identity, expected.kind, expected.level) {
        return Err(InsertError::IdentityMismatch);
    }
    let mut matches = context
        .after_order()
        .iter()
        .enumerate()
        .filter(|(_, role)| **role == RowPageRole::RowLeaf);
    let after_index = matches.next().ok_or(InsertError::IdentityMismatch)?.0;
    if matches.next().is_some() {
        return Err(InsertError::IdentityMismatch);
    }
    let output = after
        .get_mut(after_index)
        .ok_or(InsertError::IdentityMismatch)?;
    let version = RecordVersion {
        row: context.row(),
        stamp: VersionStamp::Transaction(context.actor()),
        logical: LogicalRowVersion::new(context.actor().id(), context.planned_mutation())?,
        undo: Some(context.undo().reference),
    };
    let bytes = leaf::produce(
        leaf::LeafInput {
            namespace: context.schema().namespace(),
            expected,
            before: &before[before_index],
            schema,
            canonical,
            version,
            lsn: PageLsn(context.header().lsn),
        },
        frame,
        output,
    )?;
    Ok(InsertLeafCandidate {
        after_index: after_index as u8,
        bytes,
    })
}

/// Nonrecursive inventory of A's local carriers, excluding caller-owned page
/// and frame loans. Shared codec locals and overlapping caller frames still
/// require the root's source census; this is not an aggregate peak proof.
pub(crate) fn local_source_carrier_bytes() -> (usize, usize) {
    use std::mem::size_of;
    let validation = size_of::<RowPreflightContext>()
        + size_of::<TableDefinitionView<'_>>()
        + size_of::<RowSchemaInput<'_>>()
        + size_of::<leaf::LeafSource<'_>>()
        + size_of::<Result<leaf::LeafSource<'_>>>()
        + size_of::<RowInputPlan>()
        + size_of::<Result<RowInputPlan>>()
        + size_of::<Result<(usize, [u8; 32])>>()
        + size_of::<[usize; 8]>()
        + 4 * size_of::<&[u8]>();
    let producer = size_of::<crate::storage::disk::RowProducerContext>()
        + size_of::<crate::storage::disk::RowControlObservation<'_>>()
        + size_of::<leaf::LeafInput<'_>>()
        + size_of::<Result<InsertLeafCandidate>>()
        + leaf::local_source_carrier_bytes();
    (validation, producer)
}
