//! RDL-C (dev3-row-delete.md#classification, #tests): the DELETE preflight's
//! classification of its target (`delete_class`) over the RowLeaf and Jobs
//! bank of history D (`transition_delete_tests.rs`, independent
//! `transition-delete/author.py` literals), by every row of the contract's
//! table, and the source contradictions it must never lower to a refusal or
//! zero rows. Literal observations confer no actual source, reservation,
//! pin, staging, installation or recovery authority.

use super::{DeleteClass, delete_class};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        page::{FileId, PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self as job3, JobState},
        },
        row_record::{self, RecordExpectation, RecordVersion, RowPayload, RowSchemaInput},
        semantic_row::transition::tests::delete::{D_TERMINAL, a_actor, d_leaves},
        transaction_record::{TxnRef, VersionStamp},
        tree_page::{self, EntryPayload, TreeExpectation, edit::LeafEdit},
    },
    transaction::TxnId,
};
use foundation::identity::{CommitSeq, Generation, ObjectId, RowId};

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
    let payload = if deleted {
        RowPayload::Deleted
    } else {
        old.payload()
    };
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
