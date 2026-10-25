//! K1 fixtures exercise volatile serializable publication, not durable recovery.
use engine::{
    ErrorCode,
    api::{Column, DataType, DatabaseId, TableHandle, TableSpec},
    transaction::{
        Mutation, TransactionEffect, TransactionErrorCode, TransactionMode, TransactionOptions,
        TransactionRetry, VolatileDatabase,
    },
};
use foundation::value::Value;
use std::sync::{Arc, Barrier};

fn database(options: TransactionOptions) -> VolatileDatabase {
    VolatileDatabase::new(DatabaseId::new([51; 16]).unwrap(), 1, 1, options).unwrap()
}
fn table(database: &VolatileDatabase, data_type: DataType) -> TableHandle {
    database
        .create_table(TableSpec {
            name: "items".into(),
            columns: vec![Column {
                name: "value".into(),
                data_type,
                nullable: true,
            }],
        })
        .unwrap()
}

#[test]
fn failed_statement_preserves_previous_rows_and_exact_private_leases() {
    let database = database(TransactionOptions::default());
    let table = table(&database, DataType::Int64);
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    let first = writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(10)],
        }])
        .unwrap()
        .inserted[0];
    let before = writer.private_bytes();
    let error = writer
        .execute_statement(vec![
            Mutation::Insert {
                table: table.clone(),
                values: vec![Value::Int64(20)],
            },
            Mutation::Replace {
                table: table.clone(),
                row: first,
                values: vec![Value::Bool(true)],
            },
        ])
        .unwrap_err();
    assert_eq!(
        error.code,
        TransactionErrorCode::Product(ErrorCode::SchemaMismatch)
    );
    assert_eq!(error.effect, TransactionEffect::Active);
    assert_eq!(writer.private_bytes(), before);
    assert_eq!(
        writer.read(&table, first).unwrap().unwrap().value(0),
        Some(&Value::Int64(10))
    );
    let third = writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(30)],
        }])
        .unwrap()
        .inserted[0];
    assert_eq!(third.row_id().get(), first.row_id().get() + 2);
    assert!(writer.commit().unwrap().published);
    assert_eq!(database.metrics().private_bytes, 0);
    assert_eq!(database.metrics().active_transactions, 0);
    let reader = database.begin(TransactionMode::ReadOnly).unwrap();
    let mut cursor = reader.scan(&table).unwrap();
    let page = cursor.next_page(10).unwrap();
    assert!(page.eof());
    assert_eq!(page.rows().len(), 2);
    assert_eq!(page.rows()[0].value(0), Some(&Value::Int64(10)));
    assert_eq!(page.rows()[1].value(0), Some(&Value::Int64(30)));
}

#[test]
fn competing_real_writers_publish_one_root_and_old_reader_keeps_its_value() {
    let database = database(TransactionOptions::default());
    let table = table(&database, DataType::Int64);
    let mut initial = database.begin(TransactionMode::ReadWrite).unwrap();
    let row = initial
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(0)],
        }])
        .unwrap()
        .inserted[0];
    initial.commit().unwrap();
    let reader = database.begin(TransactionMode::ReadOnly).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let threads: Vec<_> = [1, 2]
            .into_iter()
            .map(|value| {
                let database = database.clone();
                let table = table.clone();
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
                    writer
                        .execute_statement(vec![Mutation::Replace {
                            table,
                            row,
                            values: vec![Value::Int64(value)],
                        }])
                        .unwrap();
                    barrier.wait();
                    (value, writer.commit())
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let loser = results
        .iter()
        .find_map(|(_, result)| result.as_ref().err())
        .unwrap();
    assert_eq!(loser.code, TransactionErrorCode::SerializationConflict);
    assert_eq!(loser.effect, TransactionEffect::Aborted);
    assert_eq!(loser.retry, TransactionRetry::Transaction);
    assert_eq!(
        reader.read(&table, row).unwrap().unwrap().value(0),
        Some(&Value::Int64(0))
    );
    let winner = results.iter().find(|(_, result)| result.is_ok()).unwrap().0;
    let current = database.begin(TransactionMode::ReadOnly).unwrap();
    assert_eq!(
        current.read(&table, row).unwrap().unwrap().value(0),
        Some(&Value::Int64(winner))
    );
    current.rollback();
    assert!(!reader.commit().unwrap().published);
    assert_eq!(database.metrics().private_bytes, 0);
    assert_eq!(database.metrics().substrate.snapshot_pins, 0);
}

#[test]
fn noop_writer_validates_and_net_zero_writer_does_not_advance_position() {
    let database = database(TransactionOptions::default());
    let table = table(&database, DataType::Int64);
    let stale = database.begin(TransactionMode::ReadWrite).unwrap();
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    let row = writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(4)],
        }])
        .unwrap()
        .inserted[0];
    let position = writer.commit().unwrap().position;
    assert_eq!(
        stale.commit().unwrap_err().code,
        TransactionErrorCode::SerializationConflict
    );
    let mut zero = database.begin(TransactionMode::ReadWrite).unwrap();
    zero.execute_statement(vec![
        Mutation::Replace {
            table: table.clone(),
            row,
            values: vec![Value::Int64(8)],
        },
        Mutation::Replace {
            table: table.clone(),
            row,
            values: vec![Value::Int64(4)],
        },
    ])
    .unwrap();
    let temporary = zero
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Null],
        }])
        .unwrap()
        .inserted[0];
    zero.execute_statement(vec![Mutation::Delete {
        table,
        row: temporary,
    }])
    .unwrap();
    let completed = zero.commit().unwrap();
    assert!(!completed.published);
    assert_eq!(completed.position, position);
    assert_eq!(database.metrics().private_bytes, 0);
}

#[test]
fn global_private_quota_rejection_returns_first_lease_and_retry_can_progress() {
    let options = TransactionOptions {
        private_bytes: 8192,
        transaction_bytes: 8192,
        ..TransactionOptions::default()
    };
    let database = database(options);
    let table = table(&database, DataType::Binary);
    let mut first = database.begin(TransactionMode::ReadWrite).unwrap();
    let mut second = database.begin(TransactionMode::ReadWrite).unwrap();
    first
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Bytes(vec![1; 5000])],
        }])
        .unwrap();
    let held = database.metrics().private_bytes;
    assert!(held > 5000 && held < 8192);
    let error = second
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Bytes(vec![2; 5000])],
        }])
        .unwrap_err();
    assert_eq!(
        error.code,
        TransactionErrorCode::Product(ErrorCode::QuotaExceeded)
    );
    assert_eq!(error.effect, TransactionEffect::Active);
    assert_eq!(database.metrics().private_bytes, held);
    assert_eq!(second.private_bytes(), 0);
    first.rollback();
    second
        .execute_statement(vec![Mutation::Insert {
            table,
            values: vec![Value::Bytes(vec![3; 5000])],
        }])
        .unwrap();
    second.rollback();
    assert_eq!(database.metrics().private_bytes, 0);
    assert_eq!(database.metrics().active_transactions, 0);
}

#[test]
fn cancellation_aborts_previous_statements_and_releases_all_admission() {
    let database = database(TransactionOptions::default());
    let table = table(&database, DataType::Int64);
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    let id = writer.id();
    writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(5)],
        }])
        .unwrap();
    writer.cancellation().cancel();
    let error = writer.execute_statement(Vec::new()).unwrap_err();
    assert_eq!(error.code, TransactionErrorCode::Cancelled);
    assert_eq!(error.effect, TransactionEffect::Aborted);
    assert_eq!(writer.private_bytes(), 0);
    assert_eq!(
        writer.lookup_table("items").unwrap_err().code,
        TransactionErrorCode::Closed
    );
    writer.rollback();
    let reader = database.begin(TransactionMode::ReadOnly).unwrap();
    assert!(reader.id().local > id.local);
    assert!(
        reader
            .scan(&table)
            .unwrap()
            .next_page(1)
            .unwrap()
            .rows()
            .is_empty()
    );
    reader.cancellation().cancel();
    assert_eq!(
        reader.commit().unwrap_err().code,
        TransactionErrorCode::Cancelled
    );
    assert_eq!(database.metrics().private_bytes, 0);
    assert_eq!(database.metrics().active_transactions, 0);
    assert_eq!(database.metrics().substrate.snapshot_pins, 0);
}

#[test]
fn old_catalog_reader_survives_drop_recreate_and_page_returns_its_private_charge() {
    let database = database(TransactionOptions::default());
    let table = table(&database, DataType::Int64);
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    let row = writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(7)],
        }])
        .unwrap()
        .inserted[0];
    writer.commit().unwrap();
    let reader = database.begin(TransactionMode::ReadOnly).unwrap();
    database.drop_table(&table).unwrap();
    let recreated = database
        .create_table(TableSpec {
            name: "items".into(),
            columns: vec![Column {
                name: "value".into(),
                data_type: DataType::Int64,
                nullable: true,
            }],
        })
        .unwrap();
    assert_ne!(table.identity(), recreated.identity());
    let before = reader.private_bytes();
    let mut cursor = reader.scan(&table).unwrap();
    let page = cursor.next_page(1).unwrap();
    let view = page.rows()[0];
    assert!(reader.private_bytes() > before);
    assert_eq!(view.uid(), row);
    drop(page);
    assert_eq!(reader.private_bytes(), before);
    assert_eq!(view.value(0), Some(&Value::Int64(7)));
    let current = database.begin(TransactionMode::ReadOnly).unwrap();
    assert_eq!(
        current.read(&table, row).err().unwrap().code,
        TransactionErrorCode::Product(ErrorCode::StaleHandle)
    );
    assert_eq!(current.lookup_table("items").unwrap(), recreated);
}

#[test]
fn only_successful_statement_inputs_consume_the_mutation_limit() {
    let database = database(TransactionOptions::default());
    let table = table(&database, DataType::Int64);
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    let row = writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(0)],
        }])
        .unwrap()
        .inserted[0];
    writer
        .execute_statement(vec![
            Mutation::Replace {
                table: table.clone(),
                row,
                values: vec![Value::Int64(1)],
            },
            Mutation::Replace {
                table: table.clone(),
                row,
                values: vec![Value::Bool(false)],
            },
        ])
        .unwrap_err();
    writer
        .execute_statement(
            (1..=255)
                .map(|value| Mutation::Replace {
                    table: table.clone(),
                    row,
                    values: vec![Value::Int64(value)],
                })
                .collect(),
        )
        .unwrap();
    let before = writer.private_bytes();
    assert_eq!(
        writer
            .execute_statement(vec![Mutation::Delete {
                table: table.clone(),
                row
            }])
            .unwrap_err()
            .code,
        TransactionErrorCode::Product(ErrorCode::LimitExceeded)
    );
    assert_eq!(writer.private_bytes(), before);
    assert_eq!(
        writer.read(&table, row).unwrap().unwrap().value(0),
        Some(&Value::Int64(255))
    );
    writer.commit().unwrap();
}
