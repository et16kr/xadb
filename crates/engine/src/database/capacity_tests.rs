//! Synthetic accounting boundaries; the real 256 MiB fixture lives in the
//! engine worker's durable_capacity native test. No production limits are shrunk.
use super::*;
use crate::api::{Column, DataType};
use xadb_foundation::value::Value;

#[path = "../../tests/support/durable_disk.rs"]
#[allow(dead_code)]
mod disk;

fn fixture() -> (disk::Disk, Engine<disk::Writer>, TableHandle, RowUid) {
    let disk = disk::Disk::new();
    let db = Engine::create(&disk, EngineOptions::default()).unwrap();
    let table = db
        .create_table(TableSpec {
            name: "capacity".into(),
            columns: vec![Column {
                name: "v".into(),
                data_type: DataType::Binary,
                nullable: false,
            }],
        })
        .unwrap();
    let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
    let row = txn
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Bytes(vec![0; 8])],
        }])
        .unwrap()
        .inserted[0];
    txn.commit().unwrap();
    disk.reset_trace();
    (disk, db, table, row)
}

fn replace(db: &Engine<disk::Writer>, table: &TableHandle, row: RowUid) -> Result<CommitReceipt> {
    let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
    txn.execute_statement(vec![Mutation::Replace {
        table: table.clone(),
        row,
        values: vec![Value::Bytes(vec![1; 8])],
    }])
    .unwrap();
    txn.commit()
}

fn rejected(result: Result<CommitReceipt>) {
    assert!(matches!(result, Err(EngineError::Storage { error, .. })
        if error.code == DurableErrorCode::LimitExceeded
            && error.outcome == TransactionOutcome::Aborted && !error.fenced));
}

#[test]
fn commit_reserves_one_resume_header_at_the_exact_wal_boundary() {
    for headroom in [202, 203] {
        let (disk, db, table, row) = fixture();
        db.state().unwrap().wal.retained_wal_bytes = codec::WAL_BYTES - 179 - headroom;
        let result = replace(&db, &table, row);
        if headroom == 202 {
            rejected(result);
            assert!(disk.receipts().is_empty());
            assert_eq!(db.state().unwrap().wal.position, 2);
        } else {
            assert_eq!(result.unwrap().position, 3);
            assert_eq!(
                db.state().unwrap().wal.retained_wal_bytes,
                codec::WAL_BYTES - 203
            );
        }
        assert!(!db.is_fenced());
        assert_eq!(db.metrics().private_bytes, 0);
        assert_eq!(db.metrics().active_transactions, 0);
    }
}

#[test]
fn rotation_capacity_is_rejected_before_sealing_or_selecting_a_writer() {
    let (disk, db, table, row) = fixture();
    {
        let mut state = db.state().unwrap();
        state.wal.offset = codec::SEGMENT_BYTES - 133;
        // The frame alone fits, but its rotation metadata would consume the
        // recovery reserve. The old implementation wrote metadata before failing.
        state.wal.retained_wal_bytes = codec::WAL_BYTES - 179 - 203;
    }
    rejected(replace(&db, &table, row));
    assert!(disk.receipts().is_empty());
    assert!(!db.is_fenced());
    let state = db.state().unwrap();
    assert_eq!(state.wal.header.segment, 1);
    assert_eq!(state.wal.position, 2);
}

#[test]
fn all_writes_and_promised_checkpoint_files_preserve_reopen_slots() {
    let (disk, db, table, row) = fixture();
    {
        let mut state = db.state().unwrap();
        state.wal.inventory_count = MAX_FILES - 4;
        state.wal.check_files(0).unwrap();
        assert!(state.wal.check_files(1).is_err());
        state.wal.reserved_files = 1;
        assert!(state.wal.check_files(0).is_err());
        state.wal.reserved_files = 0;
        state.wal.inventory_count += 1;
    }
    rejected(replace(&db, &table, row));
    assert!(disk.receipts().is_empty());
    assert!(!db.is_fenced());
}
