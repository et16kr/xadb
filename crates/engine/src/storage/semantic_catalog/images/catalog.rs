use super::{Error, Operation, PAGE_BYTES, Result, RootContext};
use crate::storage::{
    anchor::{self, AnchorRoot, TreeKind},
    catalog_record::{
        self, CatalogDefinitionView, CatalogExpectation, CatalogHeader, CatalogTableInput,
    },
    page::PageKind,
    semantic_catalog::catalog_projection_digest,
    transaction_record::{LogicalRowVersion, VersionStamp},
    tree_page::{
        self, EntryPayload, TreeExpectation,
        edit::{LeafEdit, MAX_LEAF_EDIT_CELLS},
    },
};
use foundation::identity::CommitSeq;

pub(crate) enum CatalogChange<'a> {
    Forward(CatalogTableInput<'a>),
    Clr,
    Prune(CommitSeq),
}
impl CatalogChange<'_> {
    fn operation(&self) -> Operation {
        match self {
            Self::Forward(_) => Operation::Forward,
            Self::Clr => Operation::Clr,
            Self::Prune(_) => Operation::Prune,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct CatalogChangeObservation {
    pub(crate) cell_bytes: usize,
    pub(crate) before_projection: [u8; 32],
    pub(crate) after_projection: [u8; 32],
}

/// The Catalog anchor remains an immutable ninth target. Its actual root is
/// checked here before candidate insertion/removal in the current leaf.
pub(crate) fn change_catalog(
    change: CatalogChange<'_>,
    context: RootContext<'_>,
    anchor_before: &[u8; PAGE_BYTES],
    before: &[u8; PAGE_BYTES],
    after: &mut [u8; PAGE_BYTES],
) -> Result<CatalogChangeObservation> {
    let operation = change.operation();
    context.validate(operation)?;
    let leaf_header = context.fixed_header(8, 5, PageKind::CatalogLeaf);
    let catalog_anchor = anchor::decode_anchor(
        anchor_before,
        context.fixed_header(7, 5, PageKind::TreeAnchor).identity,
    )?;
    if catalog_anchor.root()
        != (AnchorRoot {
            tree: TreeKind::Catalog,
            page: leaf_header.identity.page,
            level: 0,
            revision: 1,
        })
    {
        return Err(Error::IdentityMismatch);
    }
    let leaf = tree_page::decode_tree_page(
        before,
        TreeExpectation {
            identity: leaf_header.identity,
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )?;
    if leaf.header().lsn.0 >= context.lsn.0 {
        return Err(Error::OrderMismatch);
    }
    let object = context.job.object().ok_or(Error::IdentityMismatch)?;
    let actor = context.job.source().ok_or(Error::IdentityMismatch)?;
    let pair = context.job.pair().ok_or(Error::IdentityMismatch)?;
    let reference = context.job.undo().ok_or(Error::IdentityMismatch)?;
    let expected = CatalogExpectation {
        namespace: context.namespace,
        object: object.object,
    };
    let key = object.object.get().to_be_bytes();
    let position = leaf.lower_bound(&key)?;
    let missing = catalog_projection_digest(context.namespace, object.object, None)?;
    let mut value = [0; catalog_record::MAX_CATALOG_CELL_BYTES];
    let mut cell = [0; super::super::MAX_CELL];
    let (edit, result) = match change {
        CatalogChange::Forward(definition) => {
            if position.equal {
                return Err(Error::IdentityMismatch);
            }
            let header = CatalogHeader {
                identity: object,
                retired: false,
                stamp: VersionStamp::Transaction(actor),
                logical: LogicalRowVersion::new(actor.id(), 1)?,
                undo: Some(reference),
                anchor: pair[0],
            };
            let length = catalog_record::encode_catalog_table_record(
                expected, header, definition, &mut value,
            )?;
            let projection = catalog_projection_digest(
                context.namespace,
                object.object,
                Some(&value[..length]),
            )?;
            if projection != *context.job.projection_digest()
                || length + 12 != context.job.catalog_bytes() as usize
            {
                return Err(Error::IdentityMismatch);
            }
            let encoded = tree_page::encode_leaf_cell(
                PageKind::CatalogLeaf,
                0,
                &key,
                &value[..length],
                &mut cell,
            )?;
            (
                LeafEdit::Insert(encoded),
                CatalogChangeObservation {
                    cell_bytes: encoded.len(),
                    before_projection: missing,
                    after_projection: projection,
                },
            )
        }
        CatalogChange::Clr | CatalogChange::Prune(_) => {
            if !position.equal {
                return Err(Error::IdentityMismatch);
            }
            let entry = leaf.entry(position.index)?;
            let EntryPayload::Value(old) = entry.payload else {
                return Err(Error::Corrupt);
            };
            let record = catalog_record::decode_catalog_record(old, expected)?;
            let header = record.header();
            if header.identity != object
                || header.retired
                || header.stamp != VersionStamp::Transaction(actor)
                || header.logical != LogicalRowVersion::new(actor.id(), 1)?
                || header.undo != Some(reference)
                || header.anchor != pair[0]
                || !matches!(record.definition(), CatalogDefinitionView::Table(_))
            {
                return Err(Error::IdentityMismatch);
            }
            let projection =
                catalog_projection_digest(context.namespace, object.object, Some(old))?;
            if projection != *context.job.projection_digest()
                || old.len() + 12 != context.job.catalog_bytes() as usize
            {
                return Err(Error::IdentityMismatch);
            }
            if let CatalogChange::Prune(q) = change {
                value[..old.len()].copy_from_slice(old);
                value[7] &= !2;
                VersionStamp::Committed(q).encode(&mut value[32..88])?;
                value[136..176].fill(0);
                let updated = &value[..old.len()];
                catalog_record::decode_catalog_record(updated, expected)?;
                let after_projection =
                    catalog_projection_digest(context.namespace, object.object, Some(updated))?;
                let encoded = tree_page::encode_leaf_cell(
                    PageKind::CatalogLeaf,
                    0,
                    &key,
                    updated,
                    &mut cell,
                )?;
                (
                    LeafEdit::Replace(encoded),
                    CatalogChangeObservation {
                        cell_bytes: encoded.len(),
                        before_projection: projection,
                        after_projection,
                    },
                )
            } else {
                (
                    LeafEdit::Remove(&key),
                    CatalogChangeObservation {
                        cell_bytes: old.len() + 12,
                        before_projection: projection,
                        after_projection: missing,
                    },
                )
            }
        }
    };
    let mut cells = [&[][..]; MAX_LEAF_EDIT_CELLS];
    let plan = leaf.prepare_leaf_edit(edit, &mut cells)?;
    if plan.needs_split() {
        return Err(Error::BudgetMismatch);
    }
    plan.encode(context.lsn, after)?;
    Ok(result)
}

pub(crate) fn initialize_pair(
    context: RootContext<'_>,
    anchor_after: &mut [u8; PAGE_BYTES],
    row_after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    context.validate(Operation::Forward)?;
    let pair = context.job.pair().ok_or(Error::IdentityMismatch)?;
    let object = context.job.object().ok_or(Error::IdentityMismatch)?;
    anchor::encode_anchor(
        context.dynamic_header(
            pair[0],
            object.object,
            object.generation,
            PageKind::TreeAnchor,
        ),
        AnchorRoot {
            tree: TreeKind::Row,
            page: pair[1],
            level: 0,
            revision: 1,
        },
        anchor_after,
    )?;
    let mut metadata = [0; 32];
    let cell = tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut metadata)?;
    tree_page::encode_tree_page(
        context.dynamic_header(pair[1], object.object, object.generation, PageKind::RowLeaf),
        0,
        &[cell],
        row_after,
    )?;
    Ok(())
}
