//! Hook-boundary failure tests. The callback is a local fixture, not a WAL oracle.
use super::*;
use crate::api::{Column, DataType};
use foundation::{error::Certainty, value::Value};

fn fixture() -> (VolatileDatabase, TableHandle) {
    let database = VolatileDatabase::new(
        DatabaseId::new([62; 16]).unwrap(),
        1,
        1,
        TransactionOptions::default(),
    )
    .unwrap();
    let table = database
        .create_table(TableSpec {
            name: "values".into(),
            columns: vec![Column {
                name: "v".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        })
        .unwrap();
    (database, table)
}

#[test]
fn attempted_hook_failure_fences_mutations_but_keeps_existing_reads_and_rollback() {
    let (database, table) = fixture();
    let reader = database.begin(TransactionMode::ReadOnly).unwrap();
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(1)],
        }])
        .unwrap();
    let failure = writer.commit_with(|_| {
        Err(HookFailure {
            error: "attempted",
            fence: true,
        })
    });
    assert!(matches!(failure, Err(CommitWithError::Hook("attempted"))));
    assert!(database.is_fenced());
    assert_eq!(
        database
            .begin(TransactionMode::ReadWrite)
            .err()
            .unwrap()
            .code,
        TransactionErrorCode::Fenced
    );
    assert!(
        reader
            .scan(&table)
            .unwrap()
            .next_page(1)
            .unwrap()
            .rows()
            .is_empty()
    );
    reader.rollback();
    assert_eq!(database.metrics().private_bytes, 0);
    assert_eq!(database.metrics().substrate.snapshot_pins, 0);
}

#[test]
fn pre_attempt_failure_does_not_publish_or_fence_and_does_not_consume_position() {
    let (database, table) = fixture();
    let position = database.committed_root().unwrap().position;
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(2)],
        }])
        .unwrap();
    let failure = writer.commit_with(|prepared| {
        assert_eq!(prepared.position, position + 1);
        assert_eq!(prepared.before.tables[0].rows.len(), 0);
        assert_eq!(prepared.after.tables[0].rows.len(), 1);
        Err(HookFailure {
            error: "no effect",
            fence: false,
        })
    });
    assert!(matches!(failure, Err(CommitWithError::Hook("no effect"))));
    assert!(!database.is_fenced());
    assert_eq!(database.committed_root().unwrap().position, position);
    let mut retry = database.begin(TransactionMode::ReadWrite).unwrap();
    retry
        .execute_statement(vec![Mutation::Insert {
            table,
            values: vec![Value::Int64(3)],
        }])
        .unwrap();
    assert_eq!(retry.commit().unwrap().position, position + 1);
}

#[test]
fn hook_success_then_poisoned_publication_is_unknown_and_fences_the_core() {
    let (database, table) = fixture();
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    writer
        .execute_statement(vec![Mutation::Insert {
            table,
            values: vec![Value::Int64(4)],
        }])
        .unwrap();
    let publication = &database.shared.publication;
    let failure = writer.commit_with::<Infallible>(|_| {
        publication.poison_for_test();
        Ok(())
    });
    let Err(CommitWithError::Publication(error)) = failure else {
        panic!("publication failure expected");
    };
    assert_eq!(error.certainty, Certainty::Unknown);
    assert_eq!(error.effect, TransactionEffect::Unknown);
    assert_eq!(error.retry, TransactionRetry::AfterRecovery);
    assert!(database.is_fenced());
    assert_eq!(database.metrics().private_bytes, 0);
    assert_eq!(database.metrics().active_transactions, 0);
}

#[test]
fn cancellation_from_hook_does_not_undo_a_completed_decision() {
    let (database, table) = fixture();
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    writer
        .execute_statement(vec![Mutation::Insert {
            table,
            values: vec![Value::Int64(5)],
        }])
        .unwrap();
    let result = writer.commit_with::<Infallible>(|prepared| {
        prepared.cancellation.cancel();
        Ok(())
    });
    assert!(matches!(
        result,
        Ok(VolatileCommit {
            published: true,
            ..
        })
    ));
    assert!(!database.is_fenced());
}

#[test]
fn final_no_change_skips_hook_after_validation() {
    let (database, table) = fixture();
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    let row = writer
        .execute_statement(vec![Mutation::Insert {
            table: table.clone(),
            values: vec![Value::Int64(6)],
        }])
        .unwrap()
        .inserted[0];
    writer
        .execute_statement(vec![Mutation::Delete { table, row }])
        .unwrap();
    let result = writer.commit_with::<Infallible>(|_| panic!("no final changes must skip WAL"));
    assert!(matches!(
        result,
        Ok(VolatileCommit {
            published: false,
            ..
        })
    ));
}

#[test]
fn absent_and_injected_clock_metrics_distinguish_missing_from_observed_time() {
    use std::sync::atomic::AtomicU64;
    struct Clock(AtomicU64);
    impl foundation::runtime::Clock for Clock {
        fn now_nanos(&self) -> u64 {
            self.0.fetch_add(10, Ordering::Relaxed)
        }
    }
    let (without_clock, _) = fixture();
    assert_eq!(without_clock.metrics().coordinator_wait_ns, None);
    assert_eq!(without_clock.metrics().coordinator_hold_ns, None);
    let clock = Arc::new(Clock(AtomicU64::new(0)));
    let database = VolatileDatabase::new_with_clock(
        DatabaseId::new([63; 16]).unwrap(),
        1,
        1,
        TransactionOptions::default(),
        clock,
    )
    .unwrap();
    let reader = database.begin(TransactionMode::ReadOnly).unwrap();
    reader.commit().unwrap();
    let metrics = database.metrics();
    assert_eq!(metrics.coordinator_acquisitions, 2);
    assert_eq!(metrics.coordinator_wait_ns, Some(20));
    assert_eq!(metrics.coordinator_hold_ns, Some(20));
    assert!(!metrics.coordinator_clock_invalid);
    assert!(!metrics.counters_saturated);
}

#[test]
fn clock_panic_during_guard_release_invalidates_timing_without_poisoning_product_state() {
    use std::sync::atomic::AtomicU64;
    struct Clock(AtomicU64);
    impl foundation::runtime::Clock for Clock {
        fn now_nanos(&self) -> u64 {
            let call = self.0.fetch_add(1, Ordering::Relaxed);
            assert_ne!(call, 2, "diagnostic release clock fixture");
            call
        }
    }
    let database = VolatileDatabase::new_with_clock(
        DatabaseId::new([64; 16]).unwrap(),
        1,
        1,
        TransactionOptions::default(),
        Arc::new(Clock(AtomicU64::new(0))),
    )
    .unwrap();
    let mut writer = database.begin(TransactionMode::ReadWrite).unwrap();
    assert!(database.metrics().coordinator_clock_invalid);
    assert_eq!(database.metrics().coordinator_hold_ns, None);
    assert!(!database.is_fenced());
    writer.execute_statement(Vec::new()).unwrap();
    assert!(!writer.commit().unwrap().published);
    assert!(!database.is_fenced());
}
