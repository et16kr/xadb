//! Native capacity boundaries with fixed literal byte accounting. These tests
//! use real files and reopen; they do not model power loss or qualify a device.
use std::{fs, path::Path};
use xadb_engine::{
    api::{Column, DataType, TableSpec},
    database::{DurableErrorCode, Engine, EngineError, EngineOptions, TransactionOutcome},
    transaction::{Mutation, TransactionMode},
};
use xadb_foundation::value::Value;
use xadb_platform::namespace::LinuxDirectory;

fn wal_bytes(path: &Path) -> (u64, u64) {
    let mut files: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_name().to_str().unwrap().starts_with("wal-"))
        .collect();
    files.sort_by_key(|entry| entry.file_name());
    let total = files
        .iter()
        .map(|entry| entry.metadata().unwrap().len())
        .sum();
    (total, files.last().unwrap().metadata().unwrap().len())
}

#[test]
#[ignore = "writes the real 256 MiB WAL capacity; run explicitly in release mode"]
fn wal_capacity_rejection_preserves_the_next_native_reopen() {
    // Literal current-format sizes, independent of the production codec:
    // AXF1 42 + common 43 + transaction 26 + Put/row 42 + trailer 18.
    const FRAME_FIXED: u64 = 171;
    const RESUME_HEADER: u64 = 203;
    const SEAL: u64 = 133;
    const SEGMENT: u64 = 16 * 1024 * 1024;
    const WAL: u64 = 256 * 1024 * 1024;
    let temporary = tempfile::tempdir().unwrap();
    let path = std::env::var_os("XADBTEST_CAPACITY_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("database"));
    fs::create_dir(&path).unwrap();
    let directory = LinuxDirectory::open_existing(&path).unwrap();
    let db = Engine::create(&directory, EngineOptions::default()).unwrap();
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
            values: vec![Value::Bytes(vec![])],
        }])
        .unwrap()
        .inserted[0];
    let mut acknowledged = txn.commit().unwrap().position;
    let mut expected = Vec::new();
    let mut counter = 0_u64;
    loop {
        let (used, offset) = wal_bytes(&path);
        assert_eq!(
            used,
            db.durability_metrics().unwrap().gauges.retained_wal_bytes
        );
        let remaining = WAL - used;
        let mut final_length = remaining - FRAME_FIXED - (RESUME_HEADER - 1);
        if offset + FRAME_FIXED + final_length + SEAL > SEGMENT {
            final_length -= SEAL + RESUME_HEADER;
        }
        let last = final_length <= 65536;
        let length = if last { final_length } else { 65536 };
        assert!(length <= 65536);
        counter += 1;
        let payload = vec![(counter % 251) as u8; length as usize];
        let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
        txn.execute_statement(vec![Mutation::Replace {
            table: table.clone(),
            row,
            values: vec![Value::Bytes(payload.clone())],
        }])
        .unwrap();
        let result = txn.commit();
        if last {
            println!("capacity attempt: used={used}, payload={length}, result={result:?}");
            let unchanged = wal_bytes(&path).0 == used;
            let unfenced = !db.is_fenced();
            drop(db);
            let reopened = Engine::open(&directory, EngineOptions::default());
            println!(
                "native reopen: {:?}",
                reopened
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            );
            assert!(matches!(result, Err(EngineError::Storage { error, .. })
                if error.code == DurableErrorCode::LimitExceeded
                    && error.outcome == TransactionOutcome::Aborted && !error.fenced));
            assert!(unchanged && unfenced);
            let reopened = reopened.unwrap();
            let read = reopened.begin(TransactionMode::ReadOnly).unwrap();
            let table = read.lookup_table("capacity").unwrap();
            assert_eq!(
                read.read(&table, row).unwrap().unwrap().value(0),
                Some(&Value::Bytes(expected))
            );
            assert_eq!(read.commit().unwrap().position, acknowledged);
            // The reserved reopen permits maintenance to recover write capacity.
            reopened.checkpoint().unwrap();
            reopened.checkpoint().unwrap();
            assert!(reopened.collect_garbage().unwrap().files > 0);
            let mut txn = reopened.begin(TransactionMode::ReadWrite).unwrap();
            txn.execute_statement(vec![Mutation::Replace {
                table,
                row,
                values: vec![Value::Bytes(vec![99])],
            }])
            .unwrap();
            assert_eq!(txn.commit().unwrap().position, acknowledged + 1);
            break;
        }
        acknowledged = result.unwrap().position;
        expected = payload;
        assert!(counter < 5000, "fixed workload bound");
    }
}

#[test]
#[ignore = "creates the real 4096-name inventory boundary; run explicitly"]
fn file_capacity_rejection_preserves_native_reopen_after_recovery() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = LinuxDirectory::open_existing(temporary.path()).unwrap();
    drop(Engine::create(&directory, EngineOptions::default()).unwrap());
    // Valid orphan selector names model abandoned metadata attempts. They do
    // not belong to either selected recovery closure and carry no WAL bytes.
    let count = fs::read_dir(temporary.path()).unwrap().count();
    for serial in 1..=4090 - count {
        fs::write(
            temporary
                .path()
                .join(format!("select-00000001-{serial:016x}.tmp")),
            [],
        )
        .unwrap();
    }
    fs::File::open(temporary.path())
        .unwrap()
        .sync_all()
        .unwrap();
    let spec = |name: &str| TableSpec {
        name: name.into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Bool,
            nullable: false,
        }],
    };
    let db = Engine::open(&directory, EngineOptions::default()).unwrap();
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 4092);
    db.create_table(spec("acknowledged")).unwrap();
    drop(db);
    let reopened = Engine::open(&directory, EngineOptions::default()).unwrap();
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 4094);
    let read = reopened.begin(TransactionMode::ReadOnly).unwrap();
    read.lookup_table("acknowledged").unwrap();
    assert_eq!(read.commit().unwrap().position, 1);
    // Recovery consumed the reserve. Even a nonrotating new batch must now
    // refuse ACK until maintenance frees metadata capacity.
    let before = wal_bytes(temporary.path());
    let result = reopened.create_table(spec("rejected"));
    assert!(matches!(result, Err(EngineError::Storage { error, .. })
        if error.code == DurableErrorCode::LimitExceeded
            && error.outcome == TransactionOutcome::Aborted && !error.fenced));
    assert_eq!(wal_bytes(temporary.path()), before);
    assert!(!reopened.is_fenced());
}
