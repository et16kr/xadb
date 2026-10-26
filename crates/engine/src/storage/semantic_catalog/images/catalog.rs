use super::{Error, Operation, PAGE_BYTES, Result, RootContext};
use crate::storage::{
    anchor::{self, AnchorRoot, TreeKind},
    catalog_record::{
        self, CatalogDefinitionView, CatalogExpectation, CatalogHeader, CatalogTableInput,
    },
    page::PageKind,
    semantic_catalog::catalog_projection_digest,
    transaction_record::{LogicalRowVersion, VersionStamp},
    tree_page::{self, EntryPayload, TreeExpectation, edit::LeafEdit},
};
use foundation::identity::CommitSeq;

/// A decoded Catalog record owns at least its 232-byte fixed header. Every
/// leaf entry additionally occupies a 12-byte key/value prefix and four-byte
/// page slot. Two reference slots cover metadata and a possible insertion.
/// This is deliberately wider than the minimum table definition permits.
pub(crate) const CATALOG_EDIT_CELLS: usize =
    tree_page::TREE_ENTRY_CAPACITY / (catalog_record::CATALOG_HEADER_BYTES + 12 + 4) + 2;

/// Establish the semantic lower cell bound before using the smaller scratch.
/// A structurally valid tree page can still contain tiny opaque values; such
/// values must never turn into an unproved Catalog capacity assumption.
pub(crate) fn validate_catalog_leaf(
    leaf: &tree_page::TreePageView<'_>,
    namespace: crate::api::Namespace,
) -> Result<()> {
    if leaf.header().kind != PageKind::CatalogLeaf || leaf.header().level != 0 {
        return Err(Error::IdentityMismatch);
    }
    for index in 0..leaf.len() {
        let entry = leaf.entry(index)?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(Error::Corrupt);
        };
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let object = foundation::identity::ObjectId::new(u64::from_be_bytes(key))
            .map_err(|_| Error::IdentityMismatch)?;
        catalog_record::decode_catalog_record(value, CatalogExpectation { namespace, object })?;
    }
    if leaf.len() + 2 > CATALOG_EDIT_CELLS {
        return Err(Error::Corrupt);
    }
    Ok(())
}

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
    validate_catalog_leaf(&leaf, context.namespace)?;
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
    let mut cells = [&[][..]; CATALOG_EDIT_CELLS];
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

#[cfg(test)]
mod scratch_tests {
    use super::*;
    use crate::{
        api::DatabaseId,
        storage::page::{self, FileId, PageHeader, PageIdentity, PageLsn, PageRef},
    };
    use foundation::identity::{Generation, ObjectId};

    fn namespace() -> crate::api::Namespace {
        crate::api::Namespace {
            database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
            timeline: 1,
        }
    }
    fn header() -> PageHeader {
        PageHeader {
            identity: PageIdentity {
                database: namespace().database,
                page: PageRef {
                    file: FileId::new(0x100000001).unwrap(),
                    page_no: 8,
                    incarnation: Generation::new(1).unwrap(),
                },
                owner: ObjectId::new(5).unwrap(),
                owner_generation: Generation::new(1).unwrap(),
            },
            kind: PageKind::CatalogLeaf,
            level: 0,
            lsn: PageLsn(8),
        }
    }
    fn expectation() -> TreeExpectation {
        TreeExpectation {
            identity: header().identity,
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        }
    }
    fn literal_cell() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dev3-catalog-create/transition/raw/ca16fa2a2869199554b3638706d8b2269c757ba4ef510c69667979dfddd4fcad.bin");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            foundation::codec::content_digest(&bytes),
            [
                0xca, 0x16, 0xfa, 0x2a, 0x28, 0x69, 0x19, 0x95, 0x54, 0xb3, 0x63, 0x87, 0x06, 0xd8,
                0xb2, 0x26, 0x9c, 0x75, 0x7b, 0xa4, 0xef, 0x51, 0x0c, 0x69, 0x66, 0x79, 0x79, 0xdf,
                0xdd, 0xd4, 0xfc, 0xad
            ]
        );
        let page = page::decode_page(&bytes, header().identity, PageKind::CatalogLeaf).unwrap();
        page.cell(1).unwrap().to_vec()
    }
    fn candidate(cells: &[Vec<u8>]) -> [u8; PAGE_BYTES] {
        let mut metadata = [0; 32];
        let meta =
            tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut metadata).unwrap();
        let refs: Vec<_> = std::iter::once(meta)
            .chain(cells.iter().map(Vec::as_slice))
            .collect();
        let mut bytes = [0; PAGE_BYTES];
        tree_page::encode_tree_page(header(), 0, &refs, &mut bytes).unwrap();
        bytes
    }
    #[test]
    fn dense_catalog_scratch_has_room_for_metadata_and_insert() {
        let original = literal_cell();
        assert_eq!(original.len(), 254);
        let mut cells = Vec::new();
        for object in 6u64..68 {
            let mut cell = original.clone();
            cell[4..12].copy_from_slice(&object.to_be_bytes());
            cell[20..28].copy_from_slice(&object.to_le_bytes());
            cells.push(cell);
        }
        let bytes = candidate(&cells);
        let view = tree_page::decode_tree_page(&bytes, expectation()).unwrap();
        assert_eq!(view.len(), 62);
        validate_catalog_leaf(&view, namespace()).unwrap();
        let mut inserted = original;
        inserted[4..12].copy_from_slice(&68u64.to_be_bytes());
        inserted[20..28].copy_from_slice(&68u64.to_le_bytes());
        let mut scratch = [&[][..]; CATALOG_EDIT_CELLS];
        let plan = view
            .prepare_leaf_edit(LeafEdit::Insert(&inserted), &mut scratch)
            .unwrap();
        assert!(
            plan.needs_split(),
            "63 minimal entries exceed the fixed leaf bytes"
        );
        assert_eq!(bytes, candidate(&cells));
    }
    #[test]
    fn tiny_or_mismatched_prior_catalog_value_is_rejected() {
        let mut tiny = [0; 32];
        let raw = tree_page::encode_leaf_cell(
            PageKind::CatalogLeaf,
            0,
            &6u64.to_be_bytes(),
            b"x",
            &mut tiny,
        )
        .unwrap();
        let bytes = candidate(&[raw.to_vec()]);
        let view = tree_page::decode_tree_page(&bytes, expectation()).unwrap();
        assert!(validate_catalog_leaf(&view, namespace()).is_err());
        let mut wrong = literal_cell();
        wrong[4..12].copy_from_slice(&7u64.to_be_bytes());
        let bytes = candidate(&[wrong]);
        let view = tree_page::decode_tree_page(&bytes, expectation()).unwrap();
        assert!(validate_catalog_leaf(&view, namespace()).is_err());
    }
}
