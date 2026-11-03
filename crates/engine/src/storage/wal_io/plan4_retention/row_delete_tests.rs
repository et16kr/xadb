//! RDL-C (dev3-row-delete.md#classification, #tests): the DELETE preflight's
//! classification of its target (`delete_class`) over the RowLeaf and Jobs
//! bank of history D (`transition_delete_tests.rs`, independent
//! `transition-delete/author.py` literals), by every row of the contract's
//! table, and the source contradictions it must never lower to a refusal or
//! zero rows. Literal observations confer no actual source, reservation,
//! pin, staging, installation or recovery authority.

use super::{DeleteClass, RowTableSource, delete_class, target_class};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace, TableSpec},
    storage::{
        catalog_record::{
            CatalogDefinition, CatalogExpectation, CatalogHeader, IndexDefinition, IndexMode,
            encode_catalog_record,
        },
        overflow::{INLINE_ROW_BYTES, OverflowHead},
        page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self as job3, JobState},
        },
        row_record::{self, RecordExpectation, RecordVersion, RowPayload, RowSchemaInput},
        semantic_row::transition::tests::delete::{D_TERMINAL, a_actor, d_leaves},
        transaction_record::{LogicalRowVersion, TxnRef, VersionStamp},
        tree_page::{self, EntryPayload, TreeExpectation, edit::LeafEdit},
    },
    transaction::TxnId,
};
use foundation::identity::{CommitSeq, Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch};

type Page = [u8; PAGE_BYTES];
type Bank = [[u8; JOB_BYTES]; JOB_COUNT];

fn generation(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new([1; 16]).unwrap(),
        timeline: 1,
    }
}
/// History D's RowLeaf: page 13 incarnation 2 of table 6.
fn expected() -> TreeExpectation {
    TreeExpectation {
        identity: PageIdentity {
            database: namespace().database,
            page: PageRef {
                file: FileId::new(0x1_0000_0001).unwrap(),
                page_no: 13,
                incarnation: generation(2),
            },
            owner: ObjectId::new(6).unwrap(),
            owner_generation: generation(1),
        },
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    }
}
fn columns() -> [Column; 2] {
    [
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
    ]
}
/// A transaction with no Job in history D's bank.
fn stranger() -> TxnRef {
    let id = TxnId {
        database: namespace().database,
        timeline: 1,
        writer_epoch: 1,
        local: 77,
    };
    TxnRef::new(id, generation(1)).unwrap()
}
fn t2() -> TxnRef {
    a_actor().0
}

fn classify(
    (leaf, bank): (&Page, &Bank),
    actor: TxnRef,
    row: u64,
    (snapshot, serializable): (u64, bool),
) -> Option<DeleteClass> {
    let columns = columns();
    delete_class(
        leaf,
        expected(),
        RowSchemaInput::Columns(&columns),
        (bank, namespace()),
        (actor, RowId::new(row).unwrap()),
        (snapshot, serializable),
    )
    .ok()
}

/// The record of `row` in a RowLeaf image.
fn version(leaf: &Page, row: u64) -> (RecordVersion, bool) {
    let view = tree_page::decode_tree_page(leaf, expected()).unwrap();
    let position = view.lower_bound(&row.to_be_bytes()).unwrap();
    assert!(position.equal, "row {row}");
    let EntryPayload::Value(value) = view.entry(position.index).unwrap().payload else {
        panic!("value");
    };
    let columns = columns();
    let record = row_record::decode_row_record_with_schema(
        value,
        RecordExpectation {
            namespace: namespace(),
            row: RowId::new(row).unwrap(),
        },
        RowSchemaInput::Columns(&columns),
    )
    .unwrap();
    (
        record.version(),
        matches!(record.payload(), RowPayload::Deleted),
    )
}

fn committed_q(leaf: &Page, row: u64) -> u64 {
    match version(leaf, row).0.stamp {
        VersionStamp::Committed(q) => q.get(),
        VersionStamp::Transaction(_) => panic!("row {row} is not committed"),
    }
}

/// `row`'s record re-encoded with `version` and `payload` (a Deleted payload
/// or the row's own value), over the same RowLeaf image.
fn restamp(leaf: &Page, row: u64, version: RecordVersion, deleted: bool) -> Box<Page> {
    rewrite(leaf, row, version, deleted.then_some(RowPayload::Deleted))
}

/// `row`'s record re-encoded with `version` and `payload` (None keeps the
/// row's own payload), over the same RowLeaf image.
fn rewrite(
    leaf: &Page,
    row: u64,
    version: RecordVersion,
    payload: Option<RowPayload<'_>>,
) -> Box<Page> {
    let view = tree_page::decode_tree_page(leaf, expected()).unwrap();
    let position = view.lower_bound(&row.to_be_bytes()).unwrap();
    let EntryPayload::Value(value) = view.entry(position.index).unwrap().payload else {
        panic!("value");
    };
    let columns = columns();
    let expectation = RecordExpectation {
        namespace: namespace(),
        row: RowId::new(row).unwrap(),
    };
    let schema = RowSchemaInput::Columns(&columns);
    let old = row_record::decode_row_record_with_schema(value, expectation, schema).unwrap();
    let payload = payload.unwrap_or(old.payload());
    let mut record = vec![0; PAGE_BYTES];
    let length = row_record::encode_row_record_with_schema(
        expectation,
        schema,
        version,
        payload,
        &mut record,
    )
    .unwrap();
    let mut cell = vec![0; PAGE_BYTES];
    let cell = tree_page::encode_leaf_cell(
        PageKind::RowLeaf,
        0,
        &row.to_be_bytes(),
        &record[..length],
        &mut cell,
    )
    .unwrap();
    let mut refs: Vec<&[u8]> = vec![&[]; 128];
    let plan = view
        .prepare_leaf_edit(LeafEdit::Replace(cell), &mut refs)
        .unwrap();
    let mut out = Box::new([0; PAGE_BYTES]);
    let lsn = u64::from_le_bytes(leaf[16..24].try_into().unwrap());
    plan.encode(PageLsn(lsn), &mut out[..]).unwrap();
    out
}

#[test]
fn rdl_c_delete_class_follows_the_contract_table() {
    let states = d_leaves(&["D-523", "D-524", "D-532", "D-534"]);
    let state = |name: &str| {
        let (_, leaf, bank) = states.iter().find(|(at, ..)| at == name).unwrap();
        (&**leaf, &**bank)
    };
    let all = (u64::MAX, true);
    let rc = |q| (q, false);
    let sr = |q| (q, true);

    // D-523: T2's delete Reserve of row 2 holds it (Reserved root 16), and
    // T's committed, pruned INSERT root 17 still holds row 11.
    let d523 = state("D-523");
    let q9 = committed_q(d523.0, 9);
    assert_eq!(
        classify(d523, t2(), 1000, all),
        Some(DeleteClass::NoRow),
        "absent key"
    );
    assert_eq!(
        classify(d523, t2(), 9, sr(q9)),
        Some(DeleteClass::Delete),
        "q = S"
    );
    assert_eq!(
        classify(d523, t2(), 9, sr(q9 - 1)),
        Some(DeleteClass::NoRow),
        "q > S"
    );
    assert_eq!(
        classify(d523, t2(), 9, rc(q9 - 1)),
        Some(DeleteClass::NoRow),
        "q > S, RC"
    );
    assert_eq!(
        classify(d523, t2(), 2, all),
        Some(DeleteClass::Busy),
        "own Reserved root"
    );
    assert_eq!(
        classify(d523, stranger(), 2, all),
        Some(DeleteClass::Busy),
        "T2's root"
    );
    assert_eq!(
        classify(d523, t2(), 11, all),
        Some(DeleteClass::Busy),
        "T's pruned root"
    );

    // D-524: row 2 is T2's tombstone; T2 is Active (terminal Leased).
    let d524 = state("D-524");
    assert!(version(d524.0, 2).1, "tombstone");
    assert_eq!(
        classify(d524, t2(), 2, all),
        Some(DeleteClass::NoRow),
        "own tombstone"
    );
    assert_eq!(
        classify(d524, stranger(), 2, all),
        Some(DeleteClass::Busy),
        "T' Leased"
    );

    // D-532: row 12 is T2's own INSERT row; T2's delete Reserve holds row 4.
    let d532 = state("D-532");
    // T2's INSERT at D-530 issued row_high + 1 = 12.
    let row12 = 12;
    assert!(matches!(
        version(d532.0, row12).0.stamp,
        VersionStamp::Transaction(actor) if actor == t2()
    ));
    assert_eq!(
        classify(d532, t2(), row12, all),
        Some(DeleteClass::Unsupported),
        "own INSERT row"
    );
    assert_eq!(
        classify(d532, stranger(), row12, all),
        Some(DeleteClass::NoRow),
        "T' Leased INSERT row"
    );
    assert_eq!(
        classify(d532, t2(), 4, all),
        Some(DeleteClass::Busy),
        "own Reserved root"
    );

    // D-534: T2 committed at q2 with its tombstones of rows 2 and 4, its
    // INSERT row 12 and its compensated root holding the restored row 9, none
    // pruned yet.
    let d534 = state("D-534");
    let terminal = job3::decode_job(&d534.1[D_TERMINAL], namespace()).unwrap();
    assert_eq!(terminal.state(), JobState::TerminalReady);
    assert_eq!(terminal.source(), Some(t2()));
    let q2 = terminal.commit_q();
    for row in [2, 4] {
        assert_eq!(
            classify(d534, stranger(), row, sr(q2)),
            Some(DeleteClass::NoRow)
        );
        assert_eq!(
            classify(d534, stranger(), row, sr(q2 - 1)),
            Some(DeleteClass::SerializationConflict),
            "row {row}"
        );
        assert_eq!(
            classify(d534, stranger(), row, rc(q2 - 1)),
            Some(DeleteClass::NoRow)
        );
    }
    assert_eq!(
        classify(d534, stranger(), row12, sr(q2)),
        Some(DeleteClass::Busy),
        "unpruned"
    );
    assert_eq!(
        classify(d534, stranger(), row12, sr(q2 - 1)),
        Some(DeleteClass::NoRow)
    );
    assert_eq!(
        classify(d534, stranger(), 9, all),
        Some(DeleteClass::Busy),
        "compensated root"
    );
}

#[test]
fn rdl_c_delete_class_never_lowers_a_contradiction() {
    let states = d_leaves(&["D-523", "D-524", "D-534"]);
    let all = (u64::MAX, true);
    let (_, d523_leaf, d523_bank) = &states[0];
    let (_, d524_leaf, d524_bank) = &states[1];
    let (_, d534_leaf, d534_bank) = &states[2];

    // T2's tombstone with no terminal for T2 (its Job freed in the bank).
    let free = d524_bank
        .iter()
        .position(|cell| {
            job3::decode_job(cell, namespace()).is_ok_and(|job| job.state() == JobState::Free)
        })
        .expect("a Free Job");
    let mut orphan = d524_bank.clone();
    orphan[D_TERMINAL] = orphan[free];
    assert!(
        classify((d524_leaf, &orphan), stranger(), 2, all).is_none(),
        "no terminal"
    );

    // T2 aborted: AbortEnd needs no Retained root, so its tombstone and its
    // INSERT row cannot remain.
    let mut aborted = d534_bank.clone();
    aborted[D_TERMINAL][120] = 2;
    aborted[D_TERMINAL][264..272].fill(0);
    for row in [2, 12] {
        assert!(
            classify((d534_leaf, &aborted), stranger(), row, all).is_none(),
            "aborted terminal, row {row}"
        );
    }

    // A Committed tombstone, and a Committed row with an undo.
    let (tombstone, _) = version(d524_leaf, 2);
    let q = CommitSeq::new(committed_q(d523_leaf, 9)).unwrap();
    let committed = restamp(
        d524_leaf,
        2,
        RecordVersion {
            stamp: VersionStamp::Committed(q),
            undo: None,
            ..tombstone
        },
        true,
    );
    let (row9, _) = version(d523_leaf, 9);
    let undone = restamp(
        d523_leaf,
        9,
        RecordVersion {
            undo: tombstone.undo,
            ..row9
        },
        false,
    );
    for (label, leaf, bank, row) in [
        ("Committed tombstone", &committed, d524_bank, 2),
        ("Committed row with an undo", &undone, d523_bank, 9),
    ] {
        for actor in [t2(), stranger()] {
            assert!(classify((leaf, bank), actor, row, all).is_none(), "{label}");
        }
    }
}

/// History D's table (object 6, anchor page 12) as an issued source.
fn table_source() -> RowTableSource {
    let table = ObjectId::new(6).unwrap();
    RowTableSource {
        handle: ObjectHandle {
            object: table,
            generation: generation(1),
            schema_epoch: SchemaEpoch::new(1).unwrap(),
        },
        slot: 0,
        anchor: PageIdentity {
            database: namespace().database,
            page: PageRef {
                file: FileId::new(0x1_0000_0001).unwrap(),
                page_no: 12,
                incarnation: generation(2),
            },
            owner: table,
            owner_generation: generation(1),
        },
    }
}
fn catalog_identity() -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: PageRef {
            file: FileId::new(0x1_0000_0001).unwrap(),
            page_no: 8,
            incarnation: generation(1),
        },
        owner: ObjectId::new(5).unwrap(),
        owner_generation: generation(1),
    }
}
/// A Catalog leaf with history D's Table record (`columns`) and, when
/// `parent` is given, one Index record (object 7) whose parent is that
/// object, as the MCA01-V12-P census leaf (`row_write_tests.rs`).
fn catalog(parent: Option<u64>) -> Box<Page> {
    let source = table_source();
    let spec = TableSpec {
        name: "row_table".into(),
        columns: columns().to_vec(),
    };
    let header = |object: u64| CatalogHeader {
        identity: ObjectHandle {
            object: ObjectId::new(object).unwrap(),
            ..source.handle
        },
        retired: false,
        stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
        logical: LogicalRowVersion::new(stranger().id(), 1).unwrap(),
        undo: None,
        anchor: source.anchor.page,
    };
    let mut definitions = vec![(6, CatalogDefinition::Table(&spec))];
    if let Some(parent) = parent {
        definitions.push((
            7,
            CatalogDefinition::Index(IndexDefinition {
                parent: ObjectHandle {
                    object: ObjectId::new(parent).unwrap(),
                    ..source.handle
                },
                name: "row_index",
                mode: IndexMode::Nonunique,
                columns: &[0],
            }),
        ));
    }
    let mut metadata = [0; 32];
    let metadata =
        tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut metadata).unwrap();
    let mut cells = vec![metadata.to_vec()];
    for (object, definition) in definitions {
        let mut record = vec![0; 4096];
        let length = encode_catalog_record(
            CatalogExpectation {
                namespace: namespace(),
                object: ObjectId::new(object).unwrap(),
            },
            header(object),
            definition,
            &mut record,
        )
        .unwrap();
        let key = u64::to_be_bytes(object);
        let mut entry = vec![0; 4 + key.len() + length];
        let entry = tree_page::encode_leaf_cell(
            PageKind::CatalogLeaf,
            0,
            &key,
            &record[..length],
            &mut entry,
        )
        .unwrap();
        cells.push(entry.to_vec());
    }
    let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
    let mut page = Box::new([0; PAGE_BYTES]);
    tree_page::encode_tree_page(
        PageHeader {
            identity: catalog_identity(),
            kind: PageKind::CatalogLeaf,
            level: 0,
            lsn: PageLsn(1),
        },
        0,
        &refs,
        &mut page[..],
    )
    .unwrap();
    page
}

/// RDL-C (dev3-row-delete.md#scope, #classification; 2b-2 review R1): an
/// indexed table is Unsupported ahead of the target's class, by the same
/// complete census that gives INSERT's `index_count`: for the row a delete
/// would take, an absent key and a row a root holds alike. An Index of
/// another table changes nothing. (No product path creates an Index yet:
/// the Catalog wire refuses Index definitions.)
#[test]
fn rdl_c_indexed_table_is_unsupported_before_the_class() {
    let states = d_leaves(&["D-523"]);
    let (_, leaf, bank) = &states[0];
    let q9 = committed_q(leaf, 9);
    let class = |parent: Option<u64>, row: u64| {
        target_class(
            (&catalog(parent), catalog_identity()),
            table_source(),
            (leaf, expected().identity),
            (bank, namespace()),
            (t2(), RowId::new(row).unwrap()),
            (q9, true),
        )
        .ok()
    };
    assert_eq!(class(None, 9), Some(DeleteClass::Delete), "no Index");
    assert_eq!(class(None, 1000), Some(DeleteClass::NoRow), "absent key");
    assert_eq!(class(None, 2), Some(DeleteClass::Busy), "held row");
    assert_eq!(
        class(Some(99), 9),
        Some(DeleteClass::Delete),
        "another table's Index"
    );
    for (row, label) in [(9, "the row"), (1000, "absent key"), (2, "held row")] {
        assert_eq!(
            class(Some(6), row),
            Some(DeleteClass::Unsupported),
            "indexed table, {label}"
        );
    }
}

/// RDL-C (dev3-row-delete.md#classification; 2b-2 review R7): an overflow
/// target is Unsupported, committed or another transaction's, and a
/// `Transaction` row with no undo is a contradiction (Corrupt), never a
/// refusal or zero rows.
#[test]
fn rdl_c_delete_class_overflow_and_undo_less_rows() {
    let states = d_leaves(&["D-523", "D-524"]);
    let all = (u64::MAX, true);
    let (_, d523_leaf, d523_bank) = &states[0];
    let (_, d524_leaf, d524_bank) = &states[1];
    let root = PageRef {
        file: FileId::new(0x1_0000_0001).unwrap(),
        page_no: 40,
        incarnation: generation(1),
    };
    let head = OverflowHead::new(root, INLINE_ROW_BYTES + 1, [0; 32]).unwrap();

    let (row9, _) = version(d523_leaf, 9);
    let committed = rewrite(d523_leaf, 9, row9, Some(RowPayload::Overflow(head)));
    assert_eq!(
        classify((&committed, d523_bank), t2(), 9, all),
        Some(DeleteClass::Unsupported),
        "committed overflow row"
    );

    // T2's tombstone at row 2 carried as T2's overflow row (T2 Leased).
    let (tombstone, _) = version(d524_leaf, 2);
    let foreign = rewrite(d524_leaf, 2, tombstone, Some(RowPayload::Overflow(head)));
    assert_eq!(
        classify((&foreign, d524_bank), stranger(), 2, all),
        Some(DeleteClass::Unsupported),
        "another transaction's overflow row"
    );

    let undo_less = rewrite(
        d524_leaf,
        2,
        RecordVersion {
            undo: None,
            ..tombstone
        },
        None,
    );
    for actor in [t2(), stranger()] {
        assert!(
            classify((&undo_less, d524_bank), actor, 2, all).is_none(),
            "Transaction row with no undo"
        );
    }
}
