//! First inline INSERT input, RowLeaf candidate and complete scope3 Forward/CLR
//! candidate production. C0 only: these functions neither reserve an identity
//! nor stage, append or install a group. Actual reserve, whole-group validation
//! and apply belong to the original disk owner; no candidate is a grant.
#![allow(
    dead_code,
    reason = "no product caller in S2: the driver's live mode and the producers are reached from tests only"
)]

pub(in crate::storage) mod driver;
mod group;
#[cfg(test)]
mod group_tests;
mod leaf;
#[cfg(test)]
mod tests;

use crate::storage::{
    catalog_record::{self, CatalogDefinitionView, CatalogError, TableDefinitionView},
    disk::{
        RowInputPlan, RowOperation, RowPageRole, RowPreflightContext, RowProducerInput,
        RowProducerLane, RowProposal, RowSourceRole, RowValidationLane,
    },
    obligation::ObligationError,
    overflow::INLINE_ROW_BYTES,
    page::{PAGE_BYTES, PageError, PageKind, PageLsn},
    retention_obligation::AgingObligationError,
    retention_page::RetentionError,
    row_record::{self, RecordVersion, RowError, RowSchemaInput, ValueRef},
    semantic_row,
    transaction_record::{LogicalRowVersion, VersionStamp},
    tree_page::{TreeError, edit::LeafEditError},
    undo_record::UndoError,
    wal_frame::WalError,
};
use foundation::codec::content_digest;

/// Contract section 6.6 names this `RowError`; `row_record::RowError` is the
/// existing codec error wrapped below, so the producer keeps `InsertError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertError {
    Unsupported,
    Capacity,
    IdentityMismatch,
    InvalidInput,
    OrderMismatch,
    BudgetMismatch,
    Row(RowError),
    Catalog(CatalogError),
    Page(PageError),
    Tree(TreeError),
    LeafEdit(LeafEditError),
    Retention(RetentionError),
    Obligation(ObligationError),
    Aging(AgingObligationError),
    Undo(UndoError),
    Wal(WalError),
    Layout(semantic_row::Error),
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
impl From<RetentionError> for InsertError {
    fn from(value: RetentionError) -> Self {
        Self::Retention(value)
    }
}
impl From<ObligationError> for InsertError {
    fn from(value: ObligationError) -> Self {
        Self::Obligation(value)
    }
}
impl From<AgingObligationError> for InsertError {
    fn from(value: AgingObligationError) -> Self {
        Self::Aging(value)
    }
}
impl From<UndoError> for InsertError {
    fn from(value: UndoError) -> Self {
        Self::Undo(value)
    }
}
impl From<WalError> for InsertError {
    fn from(value: WalError) -> Self {
        Self::Wal(value)
    }
}
impl From<semantic_row::Error> for InsertError {
    fn from(value: semantic_row::Error) -> Self {
        Self::Layout(value)
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
    let (context, _control, before, schema_bytes, frame, after, candidate, _scratch) = lane.parts();
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

/// Complete scope3 INSERT Forward candidate: the whole tag3 frame in `frame`
/// and five after images in the lane's physical order. The canonical value is
/// the reserve-sealed candidate prefix. Nothing here reserves, stages, installs
/// or appends; the original owner must recheck the proposal and every byte.
pub(in crate::storage) fn produce_insert(lane: RowProducerLane<'_>) -> Result<RowProposal> {
    propose(lane, RowOperation::Forward)
}

/// Complete CLR candidate for the U-owned Missing INSERT, using the owner's
/// actual current Row/RootJob observation and rollback target. Same limits.
pub(in crate::storage) fn produce_clr(lane: RowProducerLane<'_>) -> Result<RowProposal> {
    propose(lane, RowOperation::Clr)
}

/// Thin adapter: copy lane getters into plain inputs, then call the pure core.
fn propose(lane: RowProducerLane<'_>, operation: RowOperation) -> Result<RowProposal> {
    let (context, control, before, schema_bytes, frame, after, candidate, scratch) = lane.parts();
    if context.operation() != operation {
        return Err(InsertError::Unsupported);
    }
    let change = match (operation, context.input()) {
        (
            RowOperation::Forward,
            RowProducerInput::Insert {
                canonical_len,
                canonical_digest,
            },
        ) => {
            let canonical = candidate
                .get(..*canonical_len)
                .ok_or(InsertError::IdentityMismatch)?;
            if content_digest(canonical) != *canonical_digest {
                return Err(InsertError::IdentityMismatch);
            }
            group::Change::Forward { canonical }
        }
        (
            RowOperation::Clr,
            RowProducerInput::Clr {
                current_row_len,
                current_projection,
                rollback_target,
                next,
            },
        ) => group::Change::Clr {
            cell_bytes: *current_row_len,
            projection: *current_projection,
            target: *rollback_target,
            next: *next,
        },
        _ => return Err(InsertError::Unsupported),
    };
    let table = schema(context.schema(), schema_bytes)?;
    let mut sources = [None; 9];
    for (source, role) in sources.iter_mut().zip(group::SOURCE_ROLES) {
        *source = context.source(role).map(|binding| {
            let (identity, kind, level) = binding.expected();
            group::Source {
                index: usize::from(binding.before_index()),
                identity,
                kind,
                level,
                cell: binding.cell(),
            }
        });
    }
    let input = group::GroupInput {
        profile: context.schema().profile(),
        table: context.schema().table(),
        schema: RowSchemaInput::Catalog(table),
        leaf: context.schema().leaf_expected(),
        expected: context.frame_expected(),
        header: context.header(),
        actor: context.actor(),
        status: context.status(),
        row: context.row(),
        undo: context.undo(),
        statement: context.statement_sequence(),
        mutation: context.planned_mutation(),
        mark: context.statement_mark(),
        predecessor: context.predecessor(),
        root_job: usize::from(context.root_job_ordinal()),
        terminal_job: usize::from(context.terminal_job_ordinal()),
        sources,
        before,
        before_count: usize::from(context.before_count()),
        after_order: context.after_order(),
        jobs: control.jobs(),
        transaction: control.transaction(),
        aging: control.aging(),
        change,
    };
    let produced = group::produce(&input, frame, after, scratch)?;
    Ok(RowProposal::new(
        operation,
        produced.frame_len,
        produced.after_count,
        produced.roles,
    ))
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
        + leaf::local_source_carrier_bytes()
        + size_of::<[Option<group::Source>; 9]>()
        + size_of::<group::Change<'_>>()
        + size_of::<Result<RowProposal>>()
        + group::local_source_carrier_bytes();
    (validation, producer)
}
