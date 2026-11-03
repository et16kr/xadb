//! S2-b2 review A6: `check_candidate`'s RowLeaf shortage branch. No live
//! fixture reaches it without first filling the leaf (about eighty rows of
//! the producer's header), so the rule is checked at its boundary here: a
//! candidate that needs exactly the reported free bytes is accepted, one
//! byte more is Unchanged Capacity in Prepare.
use super::*;
use crate::storage::page::{FileId, PageIdentity, PageRef};
use foundation::identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch};

fn table() -> RowTableSource {
    let one = Generation::new(1).expect("generation");
    RowTableSource {
        handle: ObjectHandle {
            object: ObjectId::new(9).expect("object"),
            generation: one,
            schema_epoch: SchemaEpoch::new(1).expect("epoch"),
        },
        slot: 0,
        anchor: PageIdentity {
            database: crate::api::DatabaseId::new([7; 16]).expect("database"),
            page: PageRef {
                file: FileId::new((2 << 32) | 1).expect("file"),
                page_no: 11,
                incarnation: one,
            },
            owner: ObjectId::new(9).expect("owner"),
            owner_generation: one,
        },
    }
}

fn cut(leaf_free: u16) -> RowPreflightCut {
    RowPreflightCut {
        source: [0; 32],
        complete_lsn: 1,
        roles: 0,
        leaf_free,
        root_job: 0,
        delete: None,
    }
}

#[test]
fn s2b2_a6_check_candidate_leaf_shortage_is_capacity() {
    let length = 64;
    let mut candidate = [0u8; PAGE_BYTES];
    candidate[..length].fill(0x5a);
    let plan = RowInputPlan::new(
        length,
        SchemaEpoch::new(1).expect("epoch"),
        content_digest(&candidate[..length]),
    );
    // A's rule: four cell-header bytes, eight key bytes, the row header and
    // a slot over the canonical row.
    let need = u16::try_from(length + ROW_HEADER_BYTES + 16).expect("need");
    check_candidate(&plan, &candidate, table(), (&cut(need), 0)).expect("fits exactly");
    let error = check_candidate(&plan, &candidate, table(), (&cut(need - 1), 0))
        .expect_err("one byte short");
    assert_eq!(error.effect, Effect::Unchanged);
    assert_eq!(error.phase, RetentionPhase::Prepare);
    assert!(
        matches!(error.cause, RetentionCause::Capacity),
        "{:?}",
        error.cause
    );
    // The plan check runs first: a shortage never masks a forged candidate.
    candidate[0] ^= 1;
    let error = check_candidate(&plan, &candidate, table(), (&cut(need - 1), 0))
        .expect_err("forged candidate");
    assert!(
        matches!(
            error.cause,
            RetentionCause::RowSemantic(row_semantic::Error::InvalidInput)
        ),
        "{:?}",
        error.cause
    );
}

/// MCA01-V12-P (consumers §9.1, the variant's one C0 case): the pure
/// complete Catalog census (`table_index_count`, the source of
/// `RowPreflightContext::index_count`) over a Catalog leaf holding T's Table
/// record and one valid Index definition whose parent is T counts that Index
/// (1), so A's profile predicate (`index_count() != 0`, `row/insert.rs`)
/// would refuse the table Unsupported. The same leaf with the Index under
/// another parent, or with no Index, counts 0, and T's Table value is found
/// in all three. C0 only: no grant, no READY, no indexed database (CREATE
/// INDEX is DEV3); A's refusal needs a `RowPreflightContext` that only C's
/// preflight builds (consumers §8.4 forbids a context factory), so A's
/// predicate is not executed here: `census` hands the same census output to
/// A's test (S11, `row/insert/tests.rs`), which puts it through A's own
/// pure predicate. The leaf is built from the record and tree page codecs;
/// the expected counts are this test's own.
pub(in crate::storage) mod mca01_v12_p {
    use super::*;
    use crate::api::{Column, DataType, Namespace, TableSpec};
    use crate::storage::catalog_record::{
        CatalogDefinition, CatalogExpectation, CatalogHeader, IndexDefinition, IndexMode,
        encode_catalog_record,
    };
    use crate::storage::page::{PAGE_BYTES, PageHeader, PageKind, PageLsn};
    use crate::storage::transaction_record::{LogicalRowVersion, VersionStamp};
    use crate::storage::tree_page;
    use crate::transaction::TxnId;
    use foundation::identity::CommitSeq;

    const TABLE: u64 = 9;
    const INDEX: u64 = 10;

    fn namespace() -> Namespace {
        Namespace {
            database: table().anchor.database,
            timeline: 1,
        }
    }

    fn leaf_identity() -> PageIdentity {
        PageIdentity {
            database: namespace().database,
            page: PageRef {
                file: FileId::new(0x1_0000_0001).expect("file"),
                page_no: 8,
                incarnation: Generation::new(1).expect("generation"),
            },
            owner: ObjectId::new(5).expect("owner"),
            owner_generation: Generation::new(1).expect("generation"),
        }
    }

    /// T's Table record (object 9, anchor = the source's anchor) and, if
    /// `parent` is given, one Index record (object 10, Nonunique, columns
    /// [0]) whose parent is that object.
    fn leaf(parent: Option<u64>) -> [u8; PAGE_BYTES] {
        let source = table();
        let spec = TableSpec {
            name: "row_table".into(),
            columns: vec![
                Column {
                    name: "n".into(),
                    data_type: DataType::Int64,
                    nullable: false,
                },
                Column {
                    name: "t".into(),
                    data_type: DataType::Utf8,
                    nullable: true,
                },
            ],
        };
        let header = |object: u64| CatalogHeader {
            identity: ObjectHandle {
                object: ObjectId::new(object).expect("object"),
                ..source.handle
            },
            retired: false,
            stamp: VersionStamp::Committed(CommitSeq::new(1).expect("seq")),
            logical: LogicalRowVersion::new(
                TxnId {
                    database: namespace().database,
                    timeline: 1,
                    writer_epoch: 1,
                    local: 1,
                },
                1,
            )
            .expect("logical"),
            undo: None,
            anchor: source.anchor.page,
        };
        let mut definitions = vec![(TABLE, CatalogDefinition::Table(&spec))];
        if let Some(parent) = parent {
            definitions.push((
                INDEX,
                CatalogDefinition::Index(IndexDefinition {
                    parent: ObjectHandle {
                        object: ObjectId::new(parent).expect("parent"),
                        ..source.handle
                    },
                    name: "row_index",
                    mode: IndexMode::Nonunique,
                    columns: &[0],
                }),
            ));
        }
        let mut metadata = [0; 32];
        let metadata = tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut metadata)
            .expect("metadata");
        let mut cells = vec![metadata.to_vec()];
        for (object, definition) in definitions {
            let mut record = vec![0; 4096];
            let length = encode_catalog_record(
                CatalogExpectation {
                    namespace: namespace(),
                    object: ObjectId::new(object).expect("object"),
                },
                header(object),
                definition,
                &mut record,
            )
            .expect("record");
            let key = object.to_be_bytes();
            let mut entry = vec![0; 4 + key.len() + length];
            let entry = tree_page::encode_leaf_cell(
                PageKind::CatalogLeaf,
                0,
                &key,
                &record[..length],
                &mut entry,
            )
            .expect("cell");
            cells.push(entry.to_vec());
        }
        let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
        let mut page = [0; PAGE_BYTES];
        tree_page::encode_tree_page(
            PageHeader {
                identity: leaf_identity(),
                kind: PageKind::CatalogLeaf,
                level: 0,
                lsn: PageLsn(1),
            },
            0,
            &refs,
            &mut page,
        )
        .expect("page");
        page
    }

    /// The complete census over this module's leaf with T's Table record
    /// and, if `parent` is given, one valid Index (Nonunique, columns [0])
    /// whose parent is that object (T is object 9): the count C hands to
    /// `RowPreflightContext::index_count`. T's Table value must be found.
    pub(in crate::storage) fn census(parent: Option<u64>) -> u16 {
        let leaf = leaf(parent);
        assert!(
            table_value(&leaf, leaf_identity(), table(), namespace()).is_ok(),
            "T's Table value with Index parent {parent:?}"
        );
        table_index_count(&leaf, leaf_identity(), table(), namespace())
            .unwrap_or_else(|cause| panic!("census refused {cause:?}"))
    }

    /// T's object, the parent of the counted Index.
    pub(in crate::storage) const TABLE_OBJECT: u64 = TABLE;

    #[test]
    fn mca01_v12_p_pure_catalog_census_counts_a_valid_index() {
        assert_eq!(TABLE_OBJECT, table().handle.object.get());
        for (parent, expected) in [(Some(TABLE), 1), (Some(TABLE + 2), 0), (None, 0)] {
            let count = census(parent);
            assert_eq!(count, expected, "Index parent {parent:?}");
            println!(
                "MCA01_V12_P parent={parent:?} index_count={count} a_refuses={}",
                count != 0
            );
        }
    }
}
