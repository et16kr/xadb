//! Independent scalar bookkeeping against the actual volatile API; no product codec/oracle reuse.
use std::collections::BTreeMap;
use xadb_engine::{
    api::{Column, DataType, DatabaseId, RowUid, StoreOptions, TableSpec},
    substrate::VolatileStore,
};
use xadb_foundation::value::Value;

fn fixture() -> (VolatileStore, xadb_engine::api::TableHandle) {
    let mut db = VolatileStore::new(
        DatabaseId::new([17; 16]).unwrap(),
        1,
        7,
        StoreOptions::default(),
    )
    .unwrap();
    let table = db
        .create_table(TableSpec {
            name: "t".into(),
            columns: vec![Column {
                name: "n".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        })
        .unwrap();
    assert_eq!(table.identity().object.get(), 0x0000_0007_0000_0001);
    (db, table)
}

// Generator v1: exactly `steps` mutation attempts. Every step is followed by an
// independent full scan assertion; observation calls are not mutation positions.
fn generated(seed: u64, steps: usize) {
    let (mut db, table) = fixture();
    let mut expected: BTreeMap<u64, (RowUid, i64)> = BTreeMap::new();
    let mut next_row = 1u64;
    let mut random = seed;
    for _ in 0..steps {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = (random >> 1) as i64;
        match random % 3 {
            0 if !expected.is_empty() => {
                let index = (random as usize) % expected.len();
                let key = *expected.keys().nth(index).unwrap();
                let (uid, _) = expected.remove(&key).unwrap();
                db.delete(&table, uid).unwrap();
            }
            1 if !expected.is_empty() => {
                let index = (random as usize) % expected.len();
                let key = *expected.keys().nth(index).unwrap();
                let (uid, old) = expected.get_mut(&key).unwrap();
                db.replace(&table, *uid, vec![Value::Int64(value)]).unwrap();
                *old = value;
            }
            _ => {
                let uid = db.insert(&table, vec![Value::Int64(value)]).unwrap();
                let independent_id = (7u64 << 32) + next_row;
                assert_eq!(uid.row_id().get(), independent_id);
                expected.insert(independent_id, (uid, value));
                next_row += 1;
            }
        }
        let snapshot = db.snapshot().unwrap();
        let mut cursor = snapshot.scan(&table).unwrap();
        let mut actual = Vec::new();
        loop {
            let page = cursor.next_page(3).unwrap();
            for row in page.rows() {
                let Some(Value::Int64(value)) = row.value(0) else {
                    panic!("scalar type changed");
                };
                actual.push((row.uid().row_id().get(), *value));
            }
            if page.eof() {
                break;
            }
        }
        let expected_rows: Vec<_> = expected
            .iter()
            .map(|(id, (_, value))| (*id, *value))
            .collect();
        assert_eq!(actual, expected_rows, "seed={seed}");
    }
    assert_eq!(db.metrics().operations, steps as u64 + 1);
    assert_eq!(db.metrics().snapshot_pins, 0);
}

#[test]
fn v2_64_fixed_seeds_cover_128_mutation_positions() {
    for seed in 0..64 {
        generated(seed, 128);
    }
}

#[test]
fn maximum_1024_mutation_positions_has_independent_full_scan_expectations() {
    generated(u64::MAX, 1024);
}

#[test]
#[ignore = "explicit V3 extended substrate profile"]
fn v3_256_fixed_seeds_cover_1024_mutation_positions() {
    for seed in 0..256 {
        generated(seed, 1024);
    }
}

#[test]
fn real_reader_threads_keep_one_snapshot_across_publications() {
    let (mut db, table) = fixture();
    let row = db.insert(&table, vec![Value::Int64(0)]).unwrap();
    let snapshot = db.snapshot().unwrap();
    let ready = std::sync::Barrier::new(3);
    let published = std::sync::Barrier::new(3);
    std::thread::scope(|scope| {
        let mut readers = Vec::new();
        for _ in 0..2 {
            let snapshot = &snapshot;
            let table = &table;
            let ready = &ready;
            let published = &published;
            readers.push(scope.spawn(move || {
                assert_eq!(
                    snapshot.read(table, row).unwrap().unwrap().value(0),
                    Some(&Value::Int64(0))
                );
                ready.wait();
                published.wait();
                for _ in 0..1000 {
                    assert_eq!(
                        snapshot.read(table, row).unwrap().unwrap().value(0),
                        Some(&Value::Int64(0))
                    );
                }
            }));
        }
        ready.wait();
        db.replace(&table, row, vec![Value::Int64(1)]).unwrap();
        published.wait();
        for value in 2..=1000 {
            db.replace(&table, row, vec![Value::Int64(value)]).unwrap();
        }
        for reader in readers {
            reader.join().unwrap();
        }
    });
    assert_eq!(
        db.snapshot()
            .unwrap()
            .read(&table, row)
            .unwrap()
            .unwrap()
            .value(0),
        Some(&Value::Int64(1000))
    );
    assert_eq!(db.metrics().snapshot_pins, 1);
}
