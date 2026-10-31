//! C0_ONLY. Literal byte observations confer no actual source/owner rights.
//! Leaf cases have separate IDs; MCA00-V06 Forward/CLR is in `group_tests`.

use super::*;
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        page::{FileId, PageIdentity, PageRef},
        transaction_record::TxnRef,
        tree_page::{TREE_ENTRY_CAPACITY, TreeExpectation, edit::LeafEditError},
        undo_ref::UndoRef,
    },
    transaction::TxnId,
};
use foundation::identity::{Generation, ObjectId, RowId};

const EMPTY: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/empty-before.bin"
));
const ROW7: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/row7-after.bin"
));
const ROWS3_9: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/rows3-9-before.bin"
));
const ROWS3_7_9: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/rows3-7-9-after.bin"
));
const BAD_ROW: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/bad-row-before.bin"
));
const ROWS84: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/rows84-before.bin"
));
const ROWS85: &[u8; PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/rows85-after.bin"
));
const RECORD7: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-insert/row7.bin"
));

fn column(data_type: DataType, nullable: bool) -> Column {
    Column {
        name: "v".into(),
        data_type,
        nullable,
    }
}

fn canonical(data_type: DataType, nullable: bool, value: ValueRef<'_>, expected: &[u8]) {
    let columns = [column(data_type, nullable)];
    let mut output = [0xa5; 64];
    let (length, _) = encode_input(
        RowSchemaInput::Columns(&columns),
        &[value],
        TREE_ENTRY_CAPACITY,
        &mut output,
    )
    .unwrap();
    assert_eq!(&output[..length], expected);
    assert!(output[length..].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn mca00_v01_nullable_bool_literals() {
    canonical(DataType::Bool, true, ValueRef::Null, &[1, 0, 0, 0, 0]);
    canonical(
        DataType::Bool,
        true,
        ValueRef::Bool(false),
        &[1, 0, 0, 0, 1, 0],
    );
    canonical(
        DataType::Bool,
        true,
        ValueRef::Bool(true),
        &[1, 0, 0, 0, 1, 1],
    );
}

#[test]
fn mca00_v02_int64_extrema_literals() {
    canonical(
        DataType::Int64,
        false,
        ValueRef::Int64(i64::MIN),
        &[1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 128],
    );
    canonical(
        DataType::Int64,
        false,
        ValueRef::Int64(i64::MAX),
        &[1, 0, 0, 0, 2, 255, 255, 255, 255, 255, 255, 255, 127],
    );
}

#[test]
fn mca00_v03_multibyte_utf8_literal() {
    canonical(
        DataType::Utf8,
        false,
        ValueRef::Utf8("A\0한"),
        &[1, 0, 0, 0, 3, 5, 0, 0, 0, 65, 0, 237, 149, 156],
    );
}

#[test]
fn mca00_v04_binary_zero_literal() {
    canonical(
        DataType::Binary,
        false,
        ValueRef::Binary(&[0, 255, 0, 128]),
        &[1, 0, 0, 0, 4, 4, 0, 0, 0, 0, 255, 0, 128],
    );
}

#[test]
fn mca00_v05_invalid_input_preserves_candidate() {
    let bool_columns = [column(DataType::Bool, false)];
    let binary_columns = [column(DataType::Binary, false)];
    let oversize = [0x3c; 2040]; // canonical prefix9 + payload2040 = 2049
    for (columns, values, error) in [
        (
            &bool_columns,
            vec![ValueRef::Int64(1)],
            InsertError::Row(RowError::SchemaMismatch),
        ),
        (
            &bool_columns,
            vec![ValueRef::Null],
            InsertError::Row(RowError::SchemaMismatch),
        ),
        (
            &bool_columns,
            vec![],
            InsertError::Row(RowError::SchemaMismatch),
        ),
        (
            &binary_columns,
            vec![ValueRef::Binary(&oversize)],
            InsertError::Unsupported,
        ),
    ] {
        let mut output = [0xa5; PAGE_BYTES];
        assert_eq!(
            encode_input(
                RowSchemaInput::Columns(columns),
                &values,
                TREE_ENTRY_CAPACITY,
                &mut output
            ),
            Err(error)
        );
        assert!(output.iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn mca_leaf_v01_inline_and_capacity_boundaries() {
    let columns = [column(DataType::Binary, false)];
    let value = [0x3c; 2039];
    let mut output = [0xa5; PAGE_BYTES];
    let cost = 2048 + 168 + 16;
    assert_eq!(
        encode_input(
            RowSchemaInput::Columns(&columns),
            &[ValueRef::Binary(&value)],
            cost - 1,
            &mut output
        ),
        Err(InsertError::Capacity)
    );
    assert!(output.iter().all(|byte| *byte == 0xa5));
    let (length, _) = encode_input(
        RowSchemaInput::Columns(&columns),
        &[ValueRef::Binary(&value)],
        cost,
        &mut output,
    )
    .unwrap();
    assert_eq!(length, 2048);
    assert_eq!(&output[..9], &[1, 0, 0, 0, 4, 247, 7, 0, 0]);
    assert_eq!(&output[9..2048], &value);
    assert!(output[2048..].iter().all(|byte| *byte == 0xa5));
    let mut short = [0xa5; 2047];
    assert_eq!(
        encode_input(
            RowSchemaInput::Columns(&columns),
            &[ValueRef::Binary(&value)],
            cost,
            &mut short
        ),
        Err(InsertError::Row(RowError::NoSpace))
    );
    assert!(short.iter().all(|byte| *byte == 0xa5));
}

fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new([1; 16]).unwrap(),
        timeline: 2,
    }
}
fn generation(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn page(page_no: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x100000001).unwrap(),
        page_no,
        incarnation: generation(1),
    }
}
fn expected() -> TreeExpectation {
    TreeExpectation {
        identity: PageIdentity {
            database: namespace().database,
            page: page(10),
            owner: ObjectId::new(20).unwrap(),
            owner_generation: generation(1),
        },
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    }
}
fn version(row: u64) -> RecordVersion {
    let id = TxnId {
        database: namespace().database,
        timeline: 2,
        writer_epoch: 3,
        local: 4,
    };
    RecordVersion {
        row: RowId::new(row).unwrap(),
        stamp: VersionStamp::Transaction(TxnRef::new(id, generation(5)).unwrap()),
        logical: LogicalRowVersion::new(id, row - 1).unwrap(),
        undo: Some(UndoRef::new(page((row + 20).min(63)), 0, generation(2)).unwrap()),
    }
}
fn input<'a>(
    before: &'a [u8; PAGE_BYTES],
    columns: &'a [Column],
    row: u64,
    canonical: &'a [u8],
    lsn: u64,
) -> leaf::LeafInput<'a> {
    leaf::LeafInput {
        namespace: namespace(),
        expected: expected(),
        before,
        schema: RowSchemaInput::Columns(columns),
        canonical,
        version: version(row),
        lsn: PageLsn(lsn),
    }
}

#[test]
fn mca_leaf_v02_row_key_and_page_match_independent_literals() {
    let columns = [column(DataType::Bool, false)];
    let mut scratch = [0xa5; 4444];
    for (before, expected_after) in [(EMPTY, ROW7), (ROWS3_9, ROWS3_7_9)] {
        let mut output = [0xa5; PAGE_BYTES];
        let result = leaf::produce(
            input(before, &columns, 7, &[1, 0, 0, 0, 1, 1], 12),
            &mut scratch,
            &mut output,
        )
        .unwrap();
        assert_eq!(&output, expected_after);
        assert_eq!(&scratch[..174], RECORD7);
        assert_eq!(
            result,
            leaf::LeafBytes {
                key: [0, 0, 0, 0, 0, 0, 0, 7],
                record_bytes: 174,
                cell_bytes: 186
            }
        );
    }
}

#[test]
fn mca_leaf_v03_maximum_minimal_rows_use_bounded_scratch() {
    let columns = [column(DataType::Bool, true)];
    let mut scratch = [0xa5; 4444];
    let mut output = [0xa5; PAGE_BYTES];
    let result = leaf::produce(
        input(ROWS84, &columns, 86, &[1, 0, 0, 0, 0], 12),
        &mut scratch,
        &mut output,
    )
    .unwrap();
    assert_eq!(&output, ROWS85);
    assert_eq!(result.cell_bytes, 185);
    let mut rejected = [0xa5; PAGE_BYTES];
    assert_eq!(
        leaf::produce(
            input(ROWS85, &columns, 87, &[1, 0, 0, 0, 0], 13),
            &mut scratch,
            &mut rejected
        ),
        Err(InsertError::Capacity)
    );
    assert!(rejected.iter().all(|byte| *byte == 0xa5));
}

#[test]
fn mca_leaf_v04_duplicate_and_corrupt_existing_row_preserve_output() {
    let columns = [column(DataType::Bool, false)];
    for (before, row, lsn, error) in [
        (ROW7, 7, 13, InsertError::LeafEdit(LeafEditError::KeyExists)),
        (BAD_ROW, 9, 12, InsertError::Row(RowError::Corrupt)),
    ] {
        let mut scratch = [0xa5; 4444];
        let mut output = [0xa5; PAGE_BYTES];
        assert_eq!(
            leaf::produce(
                input(before, &columns, row, &[1, 0, 0, 0, 1, 1], lsn),
                &mut scratch,
                &mut output
            ),
            Err(error)
        );
        assert!(output.iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn mca_leaf_v05_stale_lsn_wrong_owner_and_small_scratch_preserve_output() {
    let columns = [column(DataType::Bool, false)];
    let mut output = [0xa5; PAGE_BYTES];
    let mut scratch = [0xa5; 4444];
    assert_eq!(
        leaf::produce(
            input(EMPTY, &columns, 7, &[1, 0, 0, 0, 1, 1], 11),
            &mut scratch,
            &mut output
        ),
        Err(InsertError::IdentityMismatch)
    );
    assert!(output.iter().all(|byte| *byte == 0xa5));
    let mut wrong = input(EMPTY, &columns, 7, &[1, 0, 0, 0, 1, 1], 12);
    wrong.expected.identity.owner = ObjectId::new(21).unwrap();
    assert!(matches!(
        leaf::produce(wrong, &mut scratch, &mut output),
        Err(InsertError::Tree(TreeError::Page(_)))
    ));
    assert!(output.iter().all(|byte| *byte == 0xa5));
    assert_eq!(
        leaf::produce(
            input(EMPTY, &columns, 7, &[1, 0, 0, 0, 1, 1], 12),
            &mut scratch[..4443],
            &mut output
        ),
        Err(InsertError::Capacity)
    );
    assert!(output.iter().all(|byte| *byte == 0xa5));
}
