//! Counterexamples for the volatile-only exclusive replacement candidate.
use engine::{
    ErrorCode,
    api::{Column, DataType, DatabaseId, StoreOptions, TableSpec},
    diagnostics::{DiagnosticsMode, DiagnosticsOptions},
    substrate::VolatileStore,
};
use foundation::value::Value;

fn spec(name: &str, data_type: DataType) -> TableSpec {
    TableSpec {
        name: name.into(),
        columns: vec![Column {
            name: "v".into(),
            data_type,
            nullable: false,
        }],
    }
}

fn store() -> VolatileStore {
    VolatileStore::new(
        DatabaseId::new([41; 16]).unwrap(),
        1,
        1,
        StoreOptions::default(),
    )
    .unwrap()
}

#[test]
fn exclusive_replace_preserves_ids_and_avoids_reference_array_copies_in_all_modes() {
    for mode in [
        DiagnosticsMode::Off,
        DiagnosticsMode::Always,
        DiagnosticsMode::Detailed,
    ] {
        let mut db = VolatileStore::new_with_diagnostics(
            DatabaseId::new([41; 16]).unwrap(),
            1,
            1,
            StoreOptions::default(),
            DiagnosticsOptions {
                mode,
                event_capacity: 1,
            },
        )
        .unwrap();
        let table = db.create_table(spec("t", DataType::Int64)).unwrap();
        let row = db.insert(&table, vec![Value::Int64(0)]).unwrap();
        db.insert(&table, vec![Value::Int64(10)]).unwrap();
        let before = db.metrics();
        for value in 1..=8 {
            db.replace(&table, row, vec![Value::Int64(value)]).unwrap();
            let snapshot = db.snapshot().unwrap();
            let observed = snapshot.read(&table, row).unwrap().unwrap();
            assert_eq!(observed.uid(), row);
            assert_eq!(observed.value(0), Some(&Value::Int64(value)));
        }
        let after = db.metrics();
        assert_eq!(after.row_references_copied, before.row_references_copied);
        assert_eq!(after.resident_bytes, before.resident_bytes);
        assert_eq!(after.operations - before.operations, 8);
        assert_eq!(after.snapshot_pins, 0);
    }
}

#[test]
fn current_root_snapshot_forces_full_copy_and_keeps_old_values() {
    let mut db = store();
    let table = db.create_table(spec("t", DataType::Int64)).unwrap();
    let row = db.insert(&table, vec![Value::Int64(1)]).unwrap();
    db.insert(&table, vec![Value::Int64(2)]).unwrap();
    let old = db.snapshot().unwrap();
    let copied = db.metrics().row_references_copied;
    db.replace(&table, row, vec![Value::Int64(3)]).unwrap();
    assert_eq!(db.metrics().row_references_copied - copied, 2);
    assert_eq!(
        old.read(&table, row).unwrap().unwrap().value(0),
        Some(&Value::Int64(1))
    );
    // The old root remains pinned, but it no longer aliases the current table.
    db.replace(&table, row, vec![Value::Int64(4)]).unwrap();
    assert_eq!(db.metrics().row_references_copied - copied, 2);
    assert_eq!(
        old.read(&table, row).unwrap().unwrap().value(0),
        Some(&Value::Int64(1))
    );
}

#[test]
fn table_only_cursor_forces_copy_after_source_snapshot_is_dropped() {
    let mut db = store();
    let table = db.create_table(spec("t", DataType::Int64)).unwrap();
    let row = db.insert(&table, vec![Value::Int64(1)]).unwrap();
    let mut cursor = db.snapshot().unwrap().scan(&table).unwrap();
    let copied = db.metrics().row_references_copied;
    db.replace(&table, row, vec![Value::Int64(2)]).unwrap();
    assert_eq!(db.metrics().row_references_copied - copied, 1);
    assert_eq!(
        cursor.next_page(1).unwrap().rows()[0].value(0),
        Some(&Value::Int64(1))
    );
}

#[test]
fn view_and_page_own_old_payload_without_pinning_current_reference_array() {
    let mut db = store();
    let table = db.create_table(spec("t", DataType::Binary)).unwrap();
    let row = db.insert(&table, vec![Value::Bytes(vec![1; 8])]).unwrap();
    let snapshot = db.snapshot().unwrap();
    let view = snapshot.read(&table, row).unwrap().unwrap();
    let page = snapshot.scan(&table).unwrap().next_page(1).unwrap();
    drop(snapshot);
    let copied = db.metrics().row_references_copied;
    db.replace(&table, row, vec![Value::Bytes(vec![2; 64])])
        .unwrap();
    db.replace(&table, row, vec![Value::Bytes(vec![3; 1])])
        .unwrap();
    assert_eq!(db.metrics().row_references_copied, copied);
    assert_eq!(view.value(0), Some(&Value::Bytes(vec![1; 8])));
    assert_eq!(page.rows()[0].value(0), Some(&Value::Bytes(vec![1; 8])));
    assert_eq!(db.metrics().snapshot_pins, 1);
    drop(view);
    drop(page);
    assert_eq!(db.metrics().snapshot_pins, 0);
}

#[test]
fn old_root_can_share_target_table_even_when_current_root_is_unique() {
    let mut db = store();
    let first = db.create_table(spec("a", DataType::Int64)).unwrap();
    let second = db.create_table(spec("b", DataType::Int64)).unwrap();
    let a = db.insert(&first, vec![Value::Int64(1)]).unwrap();
    let b = db.insert(&second, vec![Value::Int64(2)]).unwrap();
    let old = db.snapshot().unwrap();
    db.replace(&second, b, vec![Value::Int64(3)]).unwrap();
    let copied = db.metrics().row_references_copied;
    db.replace(&first, a, vec![Value::Int64(4)]).unwrap();
    assert_eq!(db.metrics().row_references_copied - copied, 1);
    assert_eq!(
        old.read(&first, a).unwrap().unwrap().value(0),
        Some(&Value::Int64(1))
    );
}

#[test]
fn preparation_error_preserves_unique_and_shared_root_and_ledger() {
    for shared in [false, true] {
        let mut db = VolatileStore::new(
            DatabaseId::new([41; 16]).unwrap(),
            1,
            1,
            StoreOptions {
                resident_bytes: 8192,
                ..StoreOptions::default()
            },
        )
        .unwrap();
        let table = db.create_table(spec("t", DataType::Binary)).unwrap();
        let row = db.insert(&table, vec![Value::Bytes(vec![1])]).unwrap();
        let old = shared.then(|| db.snapshot().unwrap());
        let before = db.metrics();
        assert_eq!(
            db.replace(&table, row, vec![Value::Int64(2)])
                .unwrap_err()
                .code,
            ErrorCode::SchemaMismatch
        );
        assert_eq!(
            db.replace(&table, row, vec![Value::Bytes(vec![2; 64 * 1024])])
                .unwrap_err()
                .code,
            ErrorCode::QuotaExceeded
        );
        assert_eq!(db.metrics().resident_bytes, before.resident_bytes);
        assert_eq!(
            db.metrics().row_references_copied,
            before.row_references_copied
        );
        assert_eq!(
            db.snapshot()
                .unwrap()
                .read(&table, row)
                .unwrap()
                .unwrap()
                .value(0),
            Some(&Value::Bytes(vec![1]))
        );
        if let Some(old) = old {
            assert_eq!(
                old.read(&table, row).unwrap().unwrap().value(0),
                Some(&Value::Bytes(vec![1]))
            );
        }
    }
}
