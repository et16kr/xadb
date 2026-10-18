use xadb_engine::{
    ErrorCode,
    api::{Column, DataType, DatabaseId, StoreOptions, TableSpec},
    substrate::VolatileStore,
};
use xadb_foundation::value::Value;

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
fn store(options: StoreOptions) -> VolatileStore {
    VolatileStore::new(DatabaseId::new([1; 16]).unwrap(), 1, 1, options).unwrap()
}
fn int(value: i64) -> Vec<Value> {
    vec![Value::Int64(value)]
}

#[test]
fn actual_rows_crud_and_hand_calculated_epoch_id_vectors() {
    let mut db = store(StoreOptions::default());
    let table = db.create_table(spec("t", DataType::Int64)).unwrap();
    assert_eq!(table.identity().object.get(), 0x0000_0001_0000_0001);
    assert_eq!(table.identity().generation.get(), 1);
    assert_eq!(table.identity().schema_epoch.get(), 1);
    let a = db.insert(&table, int(7)).unwrap();
    let b = db.insert(&table, int(9)).unwrap();
    assert_eq!(a.row_id().get(), 0x0000_0001_0000_0001);
    assert_eq!(b.row_id().get(), 0x0000_0001_0000_0002);
    assert_eq!(a.table_id(), table.identity().object);
    let old = db.snapshot().unwrap();
    let view = old.read(&table, a).unwrap().unwrap();
    db.replace(&table, a, int(11)).unwrap();
    db.delete(&table, b).unwrap();
    let current = db.snapshot().unwrap();
    assert_eq!(view.value(0), Some(&Value::Int64(7)));
    assert_eq!(
        current.read(&table, a).unwrap().unwrap().value(0),
        Some(&Value::Int64(11))
    );
    assert!(current.read(&table, b).unwrap().is_none());
    assert_eq!(
        db.delete(&table, b).unwrap_err().code,
        ErrorCode::RowNotFound
    );
    assert_eq!(
        db.replace(&table, b, int(3)).unwrap_err().code,
        ErrorCode::RowNotFound
    );
    assert_eq!(db.metrics().row_references_copied, 5);
}

#[test]
fn schema_rejects_invalid_names_duplicates_types_and_null_without_effects() {
    let mut db = store(StoreOptions::default());
    for name in [
        "".to_string(),
        "a\0b".to_string(),
        "가".repeat(43),
        "a".repeat(129),
    ] {
        assert_eq!(
            db.create_table(spec(&name, DataType::Int64))
                .unwrap_err()
                .code,
            ErrorCode::InvalidInput
        );
    }
    let mut schema = spec("t", DataType::Int64);
    schema.columns.push(schema.columns[0].clone());
    assert_eq!(
        db.create_table(schema).unwrap_err().code,
        ErrorCode::InvalidInput
    );
    let table = db
        .create_table(spec(&"a".repeat(128), DataType::Int64))
        .unwrap();
    assert_eq!(table.identity().object.get(), 0x0000_0001_0000_0001);
    assert_eq!(
        db.create_table(spec(&"a".repeat(128), DataType::Int64))
            .unwrap_err()
            .code,
        ErrorCode::TableExists
    );
    let other = db.create_table(spec("A", DataType::Int64)).unwrap();
    assert_ne!(table, other);
    let before = db.metrics().resident_bytes;
    for values in [
        vec![],
        vec![Value::Null],
        vec![Value::Bool(true)],
        vec![Value::Int64(1); 2],
    ] {
        assert_eq!(
            db.insert(&table, values).unwrap_err().code,
            ErrorCode::SchemaMismatch
        );
        assert_eq!(db.metrics().resident_bytes, before);
    }
    let uid = db.insert(&table, int(i64::MIN)).unwrap();
    assert_eq!(uid.row_id().get(), 0x0000_0001_0000_0001);
    let mut nullable = spec("nullable", DataType::Bool);
    nullable.columns[0].nullable = true;
    let nullable = db.create_table(nullable).unwrap();
    let null = db.insert(&nullable, vec![Value::Null]).unwrap();
    assert_eq!(
        db.snapshot()
            .unwrap()
            .read(&nullable, null)
            .unwrap()
            .unwrap()
            .value(0),
        Some(&Value::Null)
    );
}

#[test]
fn fixed_schema_and_table_count_boundaries_are_enforced() {
    let mut db = store(StoreOptions::default());
    for count in [0, 33] {
        let mut schema = spec("bad", DataType::Bool);
        schema.columns = (0..count)
            .map(|i| Column {
                name: format!("c{i}"),
                data_type: DataType::Bool,
                nullable: false,
            })
            .collect();
        assert_eq!(
            db.create_table(schema).unwrap_err().code,
            ErrorCode::InvalidInput
        );
    }
    let columns: Vec<_> = (0..32)
        .map(|i| Column {
            name: format!("c{i}"),
            data_type: DataType::Bool,
            nullable: false,
        })
        .collect();
    let table = db
        .create_table(TableSpec {
            name: "wide".into(),
            columns,
        })
        .unwrap();
    db.insert(&table, vec![Value::Bool(false); 32]).unwrap();
    for i in 1..64 {
        db.create_table(spec(&format!("t{i}"), DataType::Bool))
            .unwrap();
    }
    assert_eq!(
        db.create_table(spec("overflow", DataType::Bool))
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    db.drop_table(&table).unwrap();
    let fresh = db.create_table(spec("wide", DataType::Bool)).unwrap();
    assert_eq!(fresh.identity().object.get(), 0x0000_0001_0000_0041);
}

#[test]
fn value_and_canonical_row_exact_byte_boundaries() {
    let mut db = store(StoreOptions::default());
    let mut schema = spec("blob", DataType::Binary);
    schema.columns.push(Column {
        name: "b".into(),
        data_type: DataType::Binary,
        nullable: false,
    });
    let table = db.create_table(schema).unwrap();
    // u32 count + two(tag + u32 length) + payloads = 131072 exactly.
    let row = db
        .insert(
            &table,
            vec![Value::Bytes(vec![0; 65_536]), Value::Bytes(vec![1; 65_522])],
        )
        .unwrap();
    let before = db.metrics().resident_bytes;
    assert_eq!(
        db.replace(
            &table,
            row,
            vec![Value::Bytes(vec![0; 65_536]), Value::Bytes(vec![1; 65_523])]
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        db.replace(
            &table,
            row,
            vec![Value::Bytes(vec![0; 65_537]), Value::Bytes(vec![])]
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(db.metrics().resident_bytes, before);
    let text = db.create_table(spec("text", DataType::Utf8)).unwrap();
    db.insert(&text, vec![Value::Utf8("é".repeat(32_768))])
        .unwrap();
    assert_eq!(
        db.insert(&text, vec![Value::Utf8("é".repeat(32_769))])
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn foreign_instance_row_namespace_and_wrong_table_are_rejected() {
    let mut first = store(StoreOptions::default());
    let mut second = store(StoreOptions::default());
    let a = first.create_table(spec("t", DataType::Int64)).unwrap();
    let b = second.create_table(spec("t", DataType::Int64)).unwrap();
    assert_eq!(a.identity(), b.identity());
    assert_ne!(a, b);
    let row = first.insert(&a, int(1)).unwrap();
    assert_eq!(
        second.insert(&a, int(2)).unwrap_err().code,
        ErrorCode::StaleHandle
    );
    let other = first.create_table(spec("other", DataType::Int64)).unwrap();
    assert_eq!(
        first
            .snapshot()
            .unwrap()
            .read(&other, row)
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleHandle
    );
    let mut restored = VolatileStore::new(
        DatabaseId::new([1; 16]).unwrap(),
        2,
        1,
        StoreOptions::default(),
    )
    .unwrap();
    let restored_table = restored.create_table(spec("t", DataType::Int64)).unwrap();
    let restored_row = restored.insert(&restored_table, int(1)).unwrap();
    assert_ne!(row, restored_row);
    assert_eq!(
        restored
            .snapshot()
            .unwrap()
            .read(&restored_table, row)
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleHandle
    );
}

#[test]
fn old_reader_survives_drop_recreate_and_views_share_one_pin() {
    let mut db = store(StoreOptions {
        snapshot_pins: 1,
        ..StoreOptions::default()
    });
    let table = db.create_table(spec("t", DataType::Int64)).unwrap();
    let id = db.insert(&table, int(4)).unwrap();
    let snapshot = db.snapshot().unwrap();
    let view = snapshot.read(&table, id).unwrap().unwrap();
    let mut cursor = snapshot.scan(&table).unwrap();
    let page = cursor.next_page(1).unwrap();
    db.drop_table(&table).unwrap();
    let new = db.create_table(spec("t", DataType::Int64)).unwrap();
    assert_ne!(new.identity(), table.identity());
    assert_eq!(snapshot.lookup_table("t").unwrap(), table);
    assert_eq!(
        snapshot.read(&table, id).unwrap().unwrap().value(0),
        Some(&Value::Int64(4))
    );
    drop(snapshot);
    drop(cursor);
    assert_eq!(db.metrics().snapshot_pins, 1);
    assert_eq!(db.snapshot().err().unwrap().code, ErrorCode::QuotaExceeded);
    drop(page);
    assert_eq!(db.metrics().snapshot_pins, 1);
    assert_eq!(view.value(0), Some(&Value::Int64(4)));
    drop(view);
    assert_eq!(db.metrics().snapshot_pins, 0);
    let current = db.snapshot().unwrap();
    assert_eq!(current.lookup_table("t").unwrap(), new);
    assert_eq!(
        current.read(&table, id).err().unwrap().code,
        ErrorCode::StaleHandle
    );
    assert_eq!(
        current.read(&new, id).err().unwrap().code,
        ErrorCode::StaleHandle
    );
}

#[test]
fn fixed_cursor_orders_pages_reports_eof_and_charges_result_arrays() {
    let mut db = store(StoreOptions::default());
    let table = db.create_table(spec("t", DataType::Int64)).unwrap();
    let ids: Vec<_> = (0..5).map(|i| db.insert(&table, int(i)).unwrap()).collect();
    let snapshot = db.snapshot().unwrap();
    let mut cursor = snapshot.scan(&table).unwrap();
    assert_eq!(
        cursor.next_page(0).err().unwrap().code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        cursor.next_page(1025).err().unwrap().code,
        ErrorCode::LimitExceeded
    );
    let baseline = db.metrics().resident_bytes;
    let first = cursor.next_page(2).unwrap();
    assert_eq!(
        first.rows().iter().map(|r| r.uid()).collect::<Vec<_>>(),
        ids[..2]
    );
    assert_eq!(first.canonical_bytes(), 2 * (4 + 1 + 8 + 56));
    assert!(!first.eof());
    assert!(db.metrics().resident_bytes > baseline);
    drop(first);
    assert_eq!(db.metrics().resident_bytes, baseline);
    db.delete(&table, ids[2]).unwrap();
    let second = cursor.next_page(10).unwrap();
    assert_eq!(
        second.rows().iter().map(|r| r.uid()).collect::<Vec<_>>(),
        ids[2..]
    );
    assert!(second.eof());
    assert!(cursor.next_page(1).unwrap().rows().is_empty());
    assert!(cursor.next_page(1).unwrap().eof());
}

#[test]
fn scan_byte_limit_and_owning_cursor_survive_store_drop() {
    let mut db = store(StoreOptions::default());
    let table = db.create_table(spec("b", DataType::Binary)).unwrap();
    for _ in 0..5 {
        db.insert(&table, vec![Value::Bytes(vec![0; 65_536])])
            .unwrap();
    }
    let snapshot = db.snapshot().unwrap();
    let mut cursor = snapshot.scan(&table).unwrap();
    drop(snapshot);
    drop(db);
    let page = cursor.next_page(1024).unwrap();
    assert_eq!(page.rows().len(), 3);
    assert!(page.canonical_bytes() <= 256 * 1024);
    assert!(!page.eof());
    let tail = cursor.next_page(1024).unwrap();
    assert_eq!(tail.rows().len(), 2);
    assert!(tail.eof());
}

#[test]
fn failed_preparation_releases_budget_but_never_reuses_issued_ids() {
    let mut db = store(StoreOptions {
        resident_bytes: 8192,
        ..StoreOptions::default()
    });
    let table = db.create_table(spec("t", DataType::Binary)).unwrap();
    let one = db.insert(&table, vec![Value::Bytes(vec![1])]).unwrap();
    let baseline = db.metrics().resident_bytes;
    assert_eq!(
        db.insert(&table, vec![Value::Bytes(vec![0; 9000])])
            .unwrap_err()
            .code,
        ErrorCode::QuotaExceeded
    );
    assert_eq!(db.metrics().resident_bytes, baseline);
    let three = db.insert(&table, vec![Value::Bytes(vec![3])]).unwrap();
    assert_eq!(three.row_id().get(), one.row_id().get() + 2);
    let mut schema = spec("fails", DataType::Int64);
    schema.name.reserve_exact(9000);
    let baseline = db.metrics().resident_bytes;
    assert_eq!(
        db.create_table(schema).unwrap_err().code,
        ErrorCode::QuotaExceeded
    );
    assert_eq!(db.metrics().resident_bytes, baseline);
    let next = db.create_table(spec("next", DataType::Int64)).unwrap();
    assert_eq!(
        next.identity().object.get(),
        table.identity().object.get() + 2
    );
}

#[test]
fn quota_failures_across_mutations_preserve_root_and_ledger() {
    let mut failures = [0; 5];
    let mut successes = [0; 5];
    for operation in 0..5 {
        for budget in (256..4096).step_by(8) {
            let Ok(mut db) = VolatileStore::new(
                DatabaseId::new([1; 16]).unwrap(),
                1,
                1,
                StoreOptions {
                    resident_bytes: budget,
                    ..StoreOptions::default()
                },
            ) else {
                continue;
            };
            let Ok(table) = db.create_table(spec("t", DataType::Int64)) else {
                continue;
            };
            let Ok(id) = db.insert(&table, int(1)) else {
                continue;
            };
            let Ok(old) = db.snapshot() else {
                continue;
            };
            // Occupy remaining headroom on alternate fixtures, so even a small
            // catalog-only preparation reaches its own quota rejection boundary.
            let mut held = Vec::new();
            if budget % 16 == 0 {
                while let Ok(snapshot) = db.snapshot() {
                    held.push(snapshot);
                }
            }
            let baseline = db.metrics().resident_bytes;
            let result = match operation {
                0 => db.create_table(spec("new", DataType::Int64)).map(|_| ()),
                1 => db.drop_table(&table),
                2 => db.insert(&table, int(2)).map(|_| ()),
                3 => db.replace(&table, id, int(2)),
                _ => db.delete(&table, id),
            };
            if let Err(error) = result {
                assert_eq!(error.code, ErrorCode::QuotaExceeded);
                failures[operation] += 1;
                assert_eq!(db.metrics().resident_bytes, baseline);
                drop(old);
                let current = db.snapshot().unwrap();
                assert_eq!(
                    current.read(&table, id).unwrap().unwrap().value(0),
                    Some(&Value::Int64(1))
                );
                assert_eq!(
                    current.lookup_table("new").unwrap_err().code,
                    ErrorCode::TableNotFound
                );
            } else {
                successes[operation] += 1;
                assert_eq!(
                    old.read(&table, id).unwrap().unwrap().value(0),
                    Some(&Value::Int64(1))
                );
            }
        }
    }
    assert!(failures.iter().all(|count| *count > 0), "{failures:?}");
    assert!(successes.iter().all(|count| *count > 0), "{successes:?}");
}

#[test]
fn invalid_namespace_and_resource_configuration_is_rejected() {
    assert_eq!(
        DatabaseId::new([0; 16]).unwrap_err().code,
        ErrorCode::InvalidInput
    );
    for (timeline, epoch, pins) in [(0, 1, 32), (1, 0, 32), (1, 1, 0), (1, 1, 33)] {
        assert_eq!(
            VolatileStore::new(
                DatabaseId::new([1; 16]).unwrap(),
                timeline,
                epoch,
                StoreOptions {
                    snapshot_pins: pins,
                    ..StoreOptions::default()
                }
            )
            .err()
            .unwrap()
            .code,
            ErrorCode::InvalidInput
        );
    }
    assert_eq!(
        VolatileStore::new(
            DatabaseId::new([1; 16]).unwrap(),
            1,
            1,
            StoreOptions {
                resident_bytes: 0,
                ..StoreOptions::default()
            }
        )
        .err()
        .unwrap()
        .code,
        ErrorCode::QuotaExceeded
    );
}

#[test]
fn repeated_scan_results_are_charged_and_failure_does_not_advance_cursor() {
    let mut db = store(StoreOptions {
        resident_bytes: 8192,
        ..StoreOptions::default()
    });
    let table = db.create_table(spec("t", DataType::Int64)).unwrap();
    let ids: Vec<_> = (0..5).map(|i| db.insert(&table, int(i)).unwrap()).collect();
    let snapshot = db.snapshot().unwrap();
    let baseline = db.metrics().resident_bytes;
    let mut pages = Vec::new();
    let mut blocked = None;
    for _ in 0..128 {
        let mut cursor = snapshot.scan(&table).unwrap();
        match cursor.next_page(5) {
            Ok(page) => pages.push(page),
            Err(error) => {
                assert_eq!(error.code, ErrorCode::QuotaExceeded);
                blocked = Some(cursor);
                break;
            }
        }
    }
    let mut blocked = blocked.expect("bounded output must exhaust this fixture's quota");
    assert!(db.metrics().resident_bytes > baseline);
    drop(pages.pop().unwrap());
    let retry = blocked.next_page(5).unwrap();
    assert_eq!(
        retry.rows().iter().map(|row| row.uid()).collect::<Vec<_>>(),
        ids
    );
    drop(retry);
    drop(pages);
    assert_eq!(db.metrics().resident_bytes, baseline);
    assert_eq!(db.metrics().snapshot_pins, 1);
}
