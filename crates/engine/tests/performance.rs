use engine::{
    ErrorCode,
    api::{Column, DataType, DatabaseId, Metrics, StoreOptions, TableSpec},
    diagnostics::{
        DiagnosticsMode, DiagnosticsOptions, MAX_LOGICAL_BYTES, Operation, OperationCounts,
    },
    substrate::VolatileStore,
};
use foundation::value::Value;

fn spec() -> TableSpec {
    TableSpec {
        name: "t".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    }
}
fn store(mode: DiagnosticsMode, capacity: usize, options: StoreOptions) -> VolatileStore {
    VolatileStore::new_with_diagnostics(
        DatabaseId::new([2; 16]).unwrap(),
        7,
        3,
        options,
        DiagnosticsOptions {
            mode,
            event_capacity: capacity,
        },
    )
    .unwrap()
}

fn exercise(
    mode: DiagnosticsMode,
    capacity: usize,
) -> (Vec<u64>, Vec<i64>, Vec<ErrorCode>, Metrics) {
    let mut db = store(
        mode,
        capacity,
        StoreOptions {
            resident_bytes: 8192,
            snapshot_pins: 1,
        },
    );
    let mut errors = Vec::new();
    let table = db.create_table(spec()).unwrap();
    errors.push(db.create_table(spec()).unwrap_err().code);
    let row = db.insert(&table, vec![Value::Int64(7)]).unwrap();
    errors.push(
        db.insert(&table, vec![Value::Bool(false)])
            .unwrap_err()
            .code,
    );
    let old = db.snapshot().unwrap();
    errors.push(db.snapshot().err().unwrap().code);
    db.replace(&table, row, vec![Value::Int64(9)]).unwrap();
    errors.push(db.replace(&table, row, vec![Value::Null]).unwrap_err().code);
    let old_view = old.read(&table, row).unwrap().unwrap();
    let Value::Int64(old_value) = old_view.value(0).unwrap() else {
        panic!("int row")
    };
    let old_value = *old_value;
    drop(old_view);
    drop(old);
    let current = db.snapshot().unwrap();
    let current_view = current.read(&table, row).unwrap().unwrap();
    let Value::Int64(value) = current_view.value(0).unwrap() else {
        panic!("int row")
    };
    let value = *value;
    drop(current_view);
    drop(current);
    db.delete(&table, row).unwrap();
    errors.push(db.delete(&table, row).unwrap_err().code);
    db.drop_table(&table).unwrap();
    errors.push(db.drop_table(&table).unwrap_err().code);
    let fresh = db.create_table(spec()).unwrap();
    let next = db.insert(&fresh, vec![Value::Int64(11)]).unwrap();
    // A real resident-quota failure, independent of diagnostic queue capacity.
    let mut blob = spec();
    blob.name = "blob".into();
    blob.columns[0].data_type = DataType::Binary;
    let blob = db.create_table(blob).unwrap();
    errors.push(
        db.insert(&blob, vec![Value::Bytes(vec![0; 65536])])
            .unwrap_err()
            .code,
    );
    (
        vec![
            table.identity().object.get(),
            row.row_id().get(),
            fresh.identity().object.get(),
            next.row_id().get(),
        ],
        vec![old_value, value],
        errors,
        db.metrics(),
    )
}

#[test]
fn modes_and_queue_limits_preserve_rows_ids_errors_and_row_quota() {
    let baseline = exercise(DiagnosticsMode::Off, 256);
    assert_eq!(
        baseline.0,
        vec![0x300000001, 0x300000001, 0x300000002, 0x300000002]
    );
    assert_eq!(baseline.1, vec![7, 9]);
    assert_eq!(
        baseline.2,
        vec![
            ErrorCode::TableExists,
            ErrorCode::SchemaMismatch,
            ErrorCode::QuotaExceeded,
            ErrorCode::SchemaMismatch,
            ErrorCode::RowNotFound,
            ErrorCode::StaleHandle,
            ErrorCode::QuotaExceeded
        ]
    );
    for mode in [
        DiagnosticsMode::Off,
        DiagnosticsMode::Always,
        DiagnosticsMode::Detailed,
    ] {
        for capacity in [0, 1, 256] {
            assert_eq!(exercise(mode, capacity), baseline, "{mode:?}/{capacity}");
        }
    }
}

#[test]
fn every_operation_records_success_failure_and_actual_completion_error() {
    let mut db = store(
        DiagnosticsMode::Detailed,
        256,
        StoreOptions {
            snapshot_pins: 1,
            ..StoreOptions::default()
        },
    );
    let mut foreign = store(DiagnosticsMode::Off, 0, StoreOptions::default());
    let foreign_table = foreign.create_table(spec()).unwrap();
    let table = db.create_table(spec()).unwrap();
    assert!(db.create_table(spec()).is_err());
    let row = db.insert(&table, vec![Value::Int64(4)]).unwrap();
    assert!(db.insert(&table, vec![Value::Null]).is_err());
    db.replace(&table, row, vec![Value::Int64(5)]).unwrap();
    assert!(db.replace(&table, row, vec![Value::Null]).is_err());
    let snapshot = db.snapshot().unwrap();
    assert!(db.snapshot().is_err());
    assert!(snapshot.lookup_table("t").is_ok());
    assert!(snapshot.lookup_table("missing").is_err());
    assert!(snapshot.schema(&table).is_ok());
    assert!(snapshot.schema(&foreign_table).is_err());
    assert!(snapshot.read(&table, row).is_ok());
    assert!(snapshot.read(&foreign_table, row).is_err());
    let mut cursor = snapshot.scan(&table).unwrap();
    assert!(snapshot.scan(&foreign_table).is_err());
    assert!(cursor.next_page(1).is_ok());
    assert!(cursor.next_page(0).is_err());
    db.delete(&table, row).unwrap();
    assert!(db.delete(&table, row).is_err());
    db.drop_table(&table).unwrap();
    assert!(db.drop_table(&table).is_err());
    assert_eq!(
        db.diagnostics().operations,
        Some(
            [OperationCounts {
                started: 2,
                completed: 2,
                failed: 1
            }; 11]
        )
    );
    assert_eq!(db.diagnostics().dropped_events, Some(0));
    let events = db.drain_diagnostics().unwrap();
    assert!(events.available);
    assert_eq!(events.events.len(), 22);
    assert_eq!(
        events.events.iter().filter(|e| e.error.is_some()).count(),
        11
    );
    assert_eq!(events.events[3].operation, Operation::Insert);
    assert_eq!(events.events[3].error, Some(ErrorCode::SchemaMismatch));
    for (index, event) in events.events.iter().enumerate() {
        assert_eq!(event.sequence, index as u64 + 1);
    }
}

#[test]
fn bounded_collection_exposes_missing_and_dropped_observations_and_retains_capacity() {
    for mode in [
        DiagnosticsMode::Off,
        DiagnosticsMode::Always,
        DiagnosticsMode::Detailed,
    ] {
        for capacity in [0, 1, 256, 1024] {
            let db = store(mode, capacity, StoreOptions::default());
            let before = db.diagnostics();
            assert!(before.logical_bytes <= MAX_LOGICAL_BYTES);
            for _ in 0..1025 {
                drop(db.snapshot().unwrap());
            }
            let snapshot = db.diagnostics();
            let first = db.drain_diagnostics().unwrap();
            assert!(first.available);
            assert_eq!(db.diagnostics().logical_bytes, before.logical_bytes);
            match mode {
                DiagnosticsMode::Off => {
                    assert_eq!(snapshot.logical_bytes, 0);
                    assert_eq!(snapshot.operations, None);
                    assert_eq!(snapshot.dropped_events, None);
                    assert_eq!(snapshot.saturated, None);
                    assert!(first.events.is_empty());
                }
                DiagnosticsMode::Always => {
                    assert_eq!(
                        snapshot.operations.unwrap()[Operation::Snapshot as usize].completed,
                        1025
                    );
                    assert_eq!(snapshot.dropped_events, None);
                    assert_eq!(snapshot.saturated, Some(false));
                    assert!(first.events.is_empty());
                }
                DiagnosticsMode::Detailed => {
                    assert_eq!(first.events.len(), capacity);
                    assert!(first.events.capacity() <= capacity);
                    assert_eq!(snapshot.dropped_events, Some(1025 - capacity as u64));
                    drop(db.snapshot().unwrap());
                    let second = db.drain_diagnostics().unwrap();
                    assert_eq!(second.events.len(), usize::from(capacity > 0));
                    if capacity > 0 {
                        assert_eq!(second.events[0].sequence, 1026);
                    }
                    assert_eq!(db.diagnostics().logical_bytes, before.logical_bytes);
                }
            }
        }
    }
}

#[test]
fn invalid_capacity_is_rejected_before_a_store_is_returned() {
    for mode in [
        DiagnosticsMode::Off,
        DiagnosticsMode::Always,
        DiagnosticsMode::Detailed,
    ] {
        let error = VolatileStore::new_with_diagnostics(
            DatabaseId::new([2; 16]).unwrap(),
            7,
            3,
            StoreOptions::default(),
            DiagnosticsOptions {
                mode,
                event_capacity: 1025,
            },
        )
        .err()
        .unwrap();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
}

#[test]
fn concurrent_readers_keep_counts_and_loss_accounting_after_bounded_drains() {
    let mut db = store(DiagnosticsMode::Detailed, 1, StoreOptions::default());
    let table = db.create_table(spec()).unwrap();
    let row = db.insert(&table, vec![Value::Int64(7)]).unwrap();
    let snapshot = db.snapshot().unwrap();
    db.drain_diagnostics().unwrap();
    let dropped_before = db.diagnostics().dropped_events.unwrap();
    let mut events = Vec::new();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..128 {
                    let value = snapshot.read(&table, row).unwrap().unwrap();
                    assert_eq!(value.value(0), Some(&Value::Int64(7)));
                }
            });
        }
        for _ in 0..128 {
            let drain = db.drain_diagnostics().unwrap();
            events.extend(drain.events);
            std::thread::yield_now();
        }
    });
    events.extend(db.drain_diagnostics().unwrap().events);
    let diagnostics = db.diagnostics();
    assert_eq!(
        diagnostics.operations.unwrap()[Operation::Read as usize],
        OperationCounts {
            started: 512,
            completed: 512,
            failed: 0
        }
    );
    assert_eq!(
        diagnostics.dropped_events.unwrap() - dropped_before + events.len() as u64,
        512
    );
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    assert_eq!(db.metrics().rows_read, 512);
}
