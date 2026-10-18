//! Literal state and typed outcome checks against an explicit two-cache device
//! model. These are not real-device qualification or the independent ACK oracle.
#[path = "support/durable_disk.rs"]
mod durable_disk;
use durable_disk::{Disk, Failure, Fault, Operation, Writer};
use xadb_engine::{
    api::{Column, DataType, RowUid, TableSpec},
    database::{
        CommitReceipt, DurableErrorCode, Engine, EngineError, EngineOptions, TransactionOutcome,
    },
    transaction::{Mutation, TransactionMode},
};
use xadb_foundation::value::Value;

type Database = Engine<Writer>;
fn open(disk: &Disk) -> Database {
    Engine::open(disk, EngineOptions::default()).unwrap()
}
fn fixture() -> (Disk, [RowUid; 2]) {
    let disk = Disk::new();
    let db = Engine::create(&disk, EngineOptions::default()).unwrap();
    let (table, ddl) = db
        .create_table_with_receipt(TableSpec {
            name: "items".into(),
            columns: vec![Column {
                name: "value".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        })
        .unwrap();
    assert_eq!(ddl.position, 1);
    let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
    let rows = txn
        .execute_statement(vec![
            Mutation::Insert {
                table: table.clone(),
                values: vec![Value::Int64(10)],
            },
            Mutation::Insert {
                table,
                values: vec![Value::Int64(11)],
            },
        ])
        .unwrap()
        .inserted;
    assert_eq!(txn.commit().unwrap().position, 2);
    drop(db);
    (disk, [rows[0], rows[1]])
}
fn change(
    db: &Database,
    rows: [RowUid; 2],
    first: i64,
) -> xadb_engine::database::Result<CommitReceipt> {
    let mut txn = db.begin(TransactionMode::ReadWrite)?;
    let table = txn.lookup_table("items")?;
    txn.execute_statement(vec![
        Mutation::Replace {
            table: table.clone(),
            row: rows[0],
            values: vec![Value::Int64(first)],
        },
        Mutation::Replace {
            table,
            row: rows[1],
            values: vec![Value::Int64(first + 1)],
        },
    ])?;
    txn.commit()
}
fn values(db: &Database, rows: [RowUid; 2]) -> [i64; 2] {
    let txn = db.begin(TransactionMode::ReadOnly).unwrap();
    let table = txn.lookup_table("items").unwrap();
    rows.map(
        |row| match txn.read(&table, row).unwrap().unwrap().value(0).unwrap() {
            Value::Int64(value) => *value,
            other => panic!("wrong scalar: {other:?}"),
        },
    )
}
fn unknown(error: EngineError) {
    match error {
        EngineError::Storage {
            error,
            transaction: Some(_),
        } => {
            assert_eq!(error.outcome, TransactionOutcome::Unknown);
            assert!(error.fenced);
        }
        other => panic!("expected an issued UNKNOWN transaction: {other:?}"),
    }
}
fn storage_code(error: EngineError, expected: DurableErrorCode) {
    match error {
        EngineError::Storage { error, .. } => assert_eq!(error.code, expected, "{error:?}"),
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}
fn rejected_open(disk: &Disk, expected: DurableErrorCode) {
    match Engine::open(disk, EngineOptions::default()) {
        Err(error) => storage_code(error, expected),
        Ok(_) => panic!("an invalid image exposed READY"),
    }
}
fn image(disk: &Disk) -> Vec<(String, Vec<u8>)> {
    disk.names()
        .into_iter()
        .map(|name| {
            let bytes = disk.bytes(&name);
            (name, bytes)
        })
        .collect()
}
fn restored_rows(db: &Database, name: &str, original: &[RowUid]) -> Vec<RowUid> {
    let txn = db.begin(TransactionMode::ReadOnly).unwrap();
    let table = txn.lookup_table(name).unwrap();
    for row in original {
        match txn.read(&table, *row) {
            Err(error) => assert_eq!(
                error.code,
                xadb_engine::transaction::TransactionErrorCode::Product(
                    xadb_engine::ErrorCode::StaleHandle
                )
            ),
            Ok(_) => panic!("restore admitted a source-timeline row identity"),
        }
    }
    let mut scan = txn.scan(&table).unwrap();
    let mut rows = Vec::new();
    loop {
        let page = scan.next_page(16).unwrap();
        rows.extend(page.rows().iter().map(|row| row.uid()));
        assert!(rows.len() <= original.len());
        if page.eof() {
            break;
        }
        assert!(!page.rows().is_empty());
    }
    assert_eq!(rows.len(), original.len());
    for (row, source) in rows.iter().zip(original) {
        assert_eq!(row.row_id(), source.row_id());
        assert_eq!(row.table_identity(), source.table_identity());
        assert_eq!(row.database_id(), source.database_id());
        assert_ne!(row.timeline(), source.timeline());
    }
    rows
}
fn export(disk: &Disk, name: &str) {
    if let Some(root) = std::env::var_os("XADBTEST_CACHE_IMAGES") {
        disk.export(&std::path::PathBuf::from(root).join(name));
    }
}

#[test]
fn commit_write_sync_cuts_preserve_atomicity_and_known_ack_witnesses() {
    let (base, rows) = fixture();
    let baseline = base.fork();
    let db = open(&baseline);
    baseline.reset_trace();
    change(&db, rows, 20).unwrap();
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].event.operation, Operation::Write);
    assert_eq!(plan[1].event.operation, Operation::Sync);
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let disk = base.fork();
            let db = open(&disk);
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            unknown(change(&db, rows, 20).unwrap_err());
            assert!(db.is_fenced());
            disk.assert_reached();
            drop(db);
            disk.power_loss();
            export(&disk, &format!("commit-{index}-{failure:?}"));
            let recovered = open(&disk);
            let expected = if index == 1 && failure == Failure::After {
                [20, 21]
            } else {
                [10, 11]
            };
            assert_eq!(values(&recovered, rows), expected);
            // The initial two acknowledged values survive every lost-tail case;
            // the atomic replacement is whole when sync actually took effect.
            assert_eq!(
                recovered
                    .durability_metrics()
                    .unwrap()
                    .gauges
                    .durable_position,
                if expected[0] == 20 { 3 } else { 2 }
            );
        }
    }
}

#[test]
fn every_partial_commit_prefix_is_ignored_without_truncating_original_bytes() {
    let (base, rows) = fixture();
    let baseline = base.fork();
    let db = open(&baseline);
    baseline.reset_trace();
    change(&db, rows, 20).unwrap();
    let event = baseline.receipts()[0].event.clone();
    drop(db);
    drop(baseline);
    assert!(event.bytes > 42);
    for length in 1..event.bytes {
        let disk = base.fork();
        let db = open(&disk);
        disk.inject(Fault {
            ordinal: 1,
            expected: event.clone(),
            failure: Failure::PrefixError(length),
        });
        unknown(change(&db, rows, 20).unwrap_err());
        disk.assert_reached();
        let original = disk.bytes(&event.name);
        drop(db);
        disk.clear_fault();
        let recovered = open(&disk);
        assert_eq!(values(&recovered, rows), [10, 11]);
        assert_eq!(
            disk.bytes(&event.name),
            original,
            "recovery modified a partial old WAL"
        );
    }
    let disk = base.fork();
    let db = open(&disk);
    disk.inject(Fault {
        ordinal: 1,
        expected: event,
        failure: Failure::Short(1),
    });
    assert_eq!(change(&db, rows, 20).unwrap().position, 3);
    disk.assert_reached();
    drop(db);
    disk.power_loss();
    assert_eq!(values(&open(&disk), rows), [20, 21]);
}

#[test]
fn restart_stabilizes_adopted_unsynced_t1_before_acknowledging_t2_and_cache_loss() {
    let (base, rows) = fixture();
    let baseline = base.fork();
    let db = open(&baseline);
    baseline.reset_trace();
    change(&db, rows, 20).unwrap();
    let sync = baseline.receipts()[1].event.clone();
    drop(db);
    drop(baseline);
    let disk = base.fork();
    let db = open(&disk);
    disk.inject(Fault {
        ordinal: 2,
        expected: sync.clone(),
        failure: Failure::Before,
    });
    unknown(change(&db, rows, 20).unwrap_err());
    disk.assert_reached();
    drop(db);
    // Process crash preserves host cache. Reopen adopts the complete T1 frame.
    disk.reset_trace();
    let recovered = open(&disk);
    assert_eq!(values(&recovered, rows), [20, 21]);
    let barrier = disk.receipts();
    let old_sync = barrier
        .iter()
        .position(|r| r.event.operation == Operation::Sync && r.event.name == sync.name)
        .unwrap();
    let new_writer = barrier
        .iter()
        .position(|r| r.event.operation == Operation::Rename && r.event.name == "WRITER")
        .unwrap();
    assert!(old_sync < new_writer);
    let mut t2 = recovered.begin(TransactionMode::ReadWrite).unwrap();
    let table = t2.lookup_table("items").unwrap();
    t2.execute_statement(vec![Mutation::Replace {
        table,
        row: rows[0],
        values: vec![Value::Int64(30)],
    }])
    .unwrap();
    let ack = t2.commit().unwrap();
    assert_eq!(ack.position, 4);
    drop(recovered);
    disk.power_loss();
    export(&disk, "stabilized-t1-t2");
    let reopened = open(&disk);
    assert_eq!(values(&reopened, rows), [30, 21]);
    assert_eq!(
        reopened
            .durability_metrics()
            .unwrap()
            .gauges
            .durable_position,
        ack.position
    );
}

#[test]
fn checkpoint_every_mutating_boundary_preserves_the_selected_ack_closure() {
    let (base, rows) = fixture();
    let baseline = base.fork();
    let db = open(&baseline);
    baseline.reset_trace();
    db.checkpoint().unwrap();
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    assert!(
        plan.iter()
            .any(|r| r.event.operation == Operation::Rename && r.event.name == "CURRENT")
    );
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let disk = base.fork();
            let db = open(&disk);
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            storage_code(db.checkpoint().unwrap_err(), DurableErrorCode::Io);
            disk.assert_reached();
            // All selector calls, including errors before their first byte,
            // conservatively fence and preserve both possible closures.
            let selector_start = plan
                .iter()
                .position(|r| {
                    r.event.operation == Operation::Create && r.event.name.starts_with("select-")
                })
                .unwrap();
            if index >= selector_start {
                assert!(db.is_fenced());
                match db.begin(TransactionMode::ReadWrite) {
                    Err(error) => storage_code(error, DurableErrorCode::Fenced),
                    Ok(_) => panic!("uncertain CURRENT admitted a new writer"),
                }
                let before_gc = disk.receipts().len();
                storage_code(db.collect_garbage().unwrap_err(), DurableErrorCode::Fenced);
                assert_eq!(
                    disk.receipts().len(),
                    before_gc,
                    "fenced engine issued a delete"
                );
            }
            drop(db);
            disk.power_loss();
            export(&disk, &format!("checkpoint-{index}-{failure:?}"));
            let recovered = open(&disk);
            assert_eq!(values(&recovered, rows), [10, 11]);
            assert_eq!(
                recovered
                    .durability_metrics()
                    .unwrap()
                    .gauges
                    .durable_position,
                2
            );
        }
    }
}

#[test]
fn gc_keeps_fallback_chain_and_stops_before_anchor_predecessor_after_reclaim() {
    let (disk, rows) = fixture();
    let db = open(&disk);
    assert_eq!(db.checkpoint().unwrap().generation, 2);
    change(&db, rows, 20).unwrap();
    assert_eq!(db.checkpoint().unwrap().generation, 3);
    let gc = db.collect_garbage().unwrap();
    assert!(gc.files > 0);
    assert!(
        !disk
            .names()
            .iter()
            .any(|name| name.starts_with("wal-00000001-"))
    );
    drop(db);
    disk.power_loss();
    disk.damage("manifest-0000000000000003.axf", 24);
    let recovered = open(&disk);
    assert_eq!(values(&recovered, rows), [20, 21]);
    assert_eq!(
        recovered
            .durability_metrics()
            .unwrap()
            .gauges
            .checkpoint_position,
        2
    );
    // Literal Current payload: common43 + generation8 + digest32, then option.
    assert_eq!(disk.bytes("CURRENT")[10 + 43 + 8 + 32], 0);
    drop(recovered);
    disk.power_loss();
    assert_eq!(values(&open(&disk), rows), [20, 21]);
}

#[test]
fn full_frame_corruption_and_missing_anchor_are_explicit_errors() {
    let (base, rows) = fixture();
    let corrupt = base.fork();
    let db = open(&corrupt);
    corrupt.reset_trace();
    change(&db, rows, 20).unwrap();
    let write = corrupt.receipts()[0].event.clone();
    drop(db);
    corrupt.damage(&write.name, write.offset as usize + write.bytes - 1);
    match Engine::open(&corrupt, EngineOptions::default()) {
        Err(EngineError::Storage { error, .. }) => {
            assert_eq!(error.code, DurableErrorCode::Corruption)
        }
        _ => panic!("full checksum corruption was silently accepted"),
    }
    let missing = base.fork();
    let db = open(&missing);
    db.checkpoint().unwrap();
    db.checkpoint().unwrap();
    drop(db);
    missing.erase("wal-00000002-0000000000000001.axf");
    rejected_open(&missing, DurableErrorCode::Corruption);
}

#[test]
fn every_stabilize_resume_and_writer_cut_blocks_ready_and_preserves_the_fixed_prefix() {
    let (base, rows) = fixture();
    let unsynced = base.fork();
    let db = open(&unsynced);
    unsynced.reset_trace();
    change(&db, rows, 20).unwrap();
    let sync = unsynced.receipts()[1].event.clone();
    drop(db);
    drop(unsynced);
    let seed = base.fork();
    let db = open(&seed);
    seed.inject(Fault {
        ordinal: 2,
        expected: sync.clone(),
        failure: Failure::Before,
    });
    unknown(change(&db, rows, 20).unwrap_err());
    seed.assert_reached();
    drop(db);
    seed.clear_fault();
    let baseline = seed.fork();
    let db = open(&baseline);
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    let stabilize = plan
        .iter()
        .position(|r| r.event.operation == Operation::Sync && r.event.name == sync.name)
        .unwrap();
    assert!(plan[..=stabilize].iter().all(|r| matches!(
        r.event.operation,
        Operation::Sync | Operation::DirectorySync
    )));
    assert!(
        plan[stabilize + 1..]
            .iter()
            .any(|r| r.event.operation == Operation::Rename && r.event.name == "WRITER")
    );
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let disk = seed.fork();
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            rejected_open(&disk, DurableErrorCode::Io);
            disk.assert_reached();
            if index <= stabilize {
                assert!(!disk.receipts().iter().any(|r| r.event.operation == Operation::Rename && r.event.name == "WRITER"));
            }
            disk.power_loss();
            export(&disk, &format!("reopen-{index}-{failure:?}"));
            // Frozen before execution: only the old-file sync's completed effect
            // determines whether the cached T1 survives this device model.
            let retained = index > stabilize || index == stabilize && failure == Failure::After;
            let db = open(&disk);
            assert_eq!(
                values(&db, rows),
                if retained { [20, 21] } else { [10, 11] }
            );
            let ack = change(&db, rows, 30).unwrap();
            drop(db);
            disk.power_loss();
            let db = open(&disk);
            assert_eq!(values(&db, rows), [30, 31]);
            assert_eq!(
                db.durability_metrics().unwrap().gauges.durable_position,
                ack.position
            );
        }
    }
}

#[test]
fn fallback_reselection_every_cut_and_following_checkpoint_never_reselects_damage() {
    let (seed, rows) = fixture();
    let db = open(&seed);
    db.checkpoint().unwrap();
    change(&db, rows, 20).unwrap();
    db.checkpoint().unwrap();
    drop(db);
    seed.damage("manifest-0000000000000003.axf", 24);
    let baseline = seed.fork();
    let db = open(&baseline);
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    assert!(
        plan.iter()
            .any(|r| r.event.operation == Operation::Rename && r.event.name == "CURRENT")
    );
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let disk = seed.fork();
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            rejected_open(&disk, DurableErrorCode::Io);
            disk.assert_reached();
            disk.power_loss();
            let db = open(&disk);
            assert_eq!(values(&db, rows), [20, 21]);
            assert_eq!(
                db.durability_metrics().unwrap().gauges.checkpoint_position,
                2
            );
            assert_eq!(disk.bytes("CURRENT")[10 + 43 + 8 + 32], 0);
            let checkpoint = db.checkpoint().unwrap();
            assert!(checkpoint.generation > 3);
            db.collect_garbage().unwrap();
            drop(db);
            disk.power_loss();
            let db = open(&disk);
            assert_eq!(values(&db, rows), [20, 21]);
            assert_eq!(
                db.durability_metrics().unwrap().gauges.checkpoint_position,
                3
            );
        }
    }
}

#[test]
fn gc_each_unlink_and_directory_sync_cut_preserves_both_selected_closures() {
    let (seed, rows) = fixture();
    let db = open(&seed);
    db.checkpoint().unwrap();
    change(&db, rows, 20).unwrap();
    db.checkpoint().unwrap();
    drop(db);
    let baseline = seed.fork();
    let db = open(&baseline);
    baseline.reset_trace();
    assert!(db.collect_garbage().unwrap().files > 0);
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    assert!(plan.iter().all(|r| matches!(
        r.event.operation,
        Operation::Unlink | Operation::DirectorySync
    )));
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let disk = seed.fork();
            let db = open(&disk);
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            storage_code(db.collect_garbage().unwrap_err(), DurableErrorCode::Io);
            disk.assert_reached();
            assert!(db.is_fenced());
            drop(db);
            disk.power_loss();
            let fallback = disk.fork();
            fallback.damage("manifest-0000000000000003.axf", 24);
            let db = open(&disk);
            assert_eq!(values(&db, rows), [20, 21]);
            drop(db);
            let db = open(&fallback);
            assert_eq!(values(&db, rows), [20, 21]);
            assert_eq!(
                db.durability_metrics().unwrap().gauges.checkpoint_position,
                2
            );
        }
    }
}

#[test]
fn corrupt_current_and_writer_never_fall_back_or_modify_the_source_image() {
    let (seed, _) = fixture();
    for name in ["CURRENT", "WRITER"] {
        let disk = seed.fork();
        disk.damage(name, 24);
        let original = image(&disk);
        rejected_open(&disk, DurableErrorCode::Corruption);
        assert_eq!(image(&disk), original);
    }
}

#[test]
fn bootstrap_each_boundary_is_incomplete_until_the_final_selector_is_durable() {
    let seed = Disk::new();
    let baseline = seed.fork();
    let db = Engine::create(&baseline, EngineOptions::default()).unwrap();
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    assert_eq!(
        plan.last().unwrap().event.operation,
        Operation::DirectorySync
    );
    assert_eq!(plan[plan.len() - 2].event.name, "CURRENT");
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let disk = seed.fork();
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            match Engine::create(&disk, EngineOptions::default()) {
                Err(error) => storage_code(error, DurableErrorCode::Io),
                Ok(_) => panic!("create ignored a required I/O failure"),
            }
            disk.assert_reached();
            disk.power_loss();
            if index == plan.len() - 1 && failure == Failure::After {
                let db = open(&disk);
                assert_eq!(db.durability_metrics().unwrap().gauges.durable_position, 0);
                let cp = db.checkpoint().unwrap();
                assert_eq!(cp.position, 0);
                db.collect_garbage().unwrap();
                drop(db);
                disk.power_loss();
                assert_eq!(
                    open(&disk)
                        .durability_metrics()
                        .unwrap()
                        .gauges
                        .durable_position,
                    0
                );
            } else {
                let original = image(&disk);
                rejected_open(&disk, DurableErrorCode::IncompleteCreate);
                assert_eq!(
                    image(&disk),
                    original,
                    "incomplete create was modified by open"
                );
            }
        }
    }
}

#[test]
fn backup_each_target_cut_preserves_source_and_only_complete_archives_restore() {
    let (source, rows) = fixture();
    let db = open(&source);
    db.checkpoint().unwrap();
    change(&db, rows, 20).unwrap(); // The archive must still contain cut q=2.
    let source_before = image(&source);
    let seed = Disk::new();
    let baseline = seed.fork();
    assert_eq!(db.backup(&baseline).unwrap().position, 2);
    let plan = baseline.receipts();
    drop(baseline);
    assert_eq!(
        plan.last().unwrap().event.operation,
        Operation::DirectorySync
    );
    assert_eq!(plan[plan.len() - 2].event.name, "CURRENT");
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let archive = seed.fork();
            archive.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            storage_code(db.backup(&archive).unwrap_err(), DurableErrorCode::Io);
            archive.assert_reached();
            archive.power_loss();
            assert!(!db.is_fenced());
            assert_eq!(values(&db, rows), [20, 21]);
            assert_eq!(image(&source), source_before);
            let target = Disk::new();
            let result = Engine::restore_new(&archive, &target, EngineOptions::default());
            if index == plan.len() - 1 && failure == Failure::After {
                let restored = result.unwrap();
                let rows: [RowUid; 2] =
                    restored_rows(&restored, "items", &rows).try_into().unwrap();
                assert_eq!(values(&restored, rows), [10, 11]);
                assert_eq!(
                    restored
                        .durability_metrics()
                        .unwrap()
                        .gauges
                        .durable_position,
                    2
                );
                let ack = change(&restored, rows, 30).unwrap();
                drop(restored);
                target.power_loss();
                let restored = open(&target);
                assert_eq!(values(&restored, rows), [30, 31]);
                assert_eq!(
                    restored
                        .durability_metrics()
                        .unwrap()
                        .gauges
                        .durable_position,
                    ack.position
                );
            } else {
                match result {
                    Err(error) => storage_code(error, DurableErrorCode::Corruption),
                    Ok(_) => panic!("partial archive restored"),
                }
                assert_eq!(target.names(), ["LOCK"]);
            }
        }
    }
}

#[test]
fn restore_each_target_cut_rejects_incomplete_selection_and_preserves_exact_cut() {
    let (source, rows) = fixture();
    let db = open(&source);
    db.checkpoint().unwrap();
    let archive = Disk::new();
    db.backup(&archive).unwrap();
    drop(db);
    drop(source);
    assert!(!archive.names().iter().any(|name| name.starts_with("wal-")));
    let archive_before = image(&archive);
    let seed = Disk::new();
    let baseline = seed.fork();
    let restored = Engine::restore_new(&archive, &baseline, EngineOptions::default()).unwrap();
    let plan = baseline.receipts();
    drop(restored);
    drop(baseline);
    assert_eq!(
        plan.last().unwrap().event.operation,
        Operation::DirectorySync
    );
    assert_eq!(plan[plan.len() - 2].event.name, "CURRENT");
    for (index, receipt) in plan.iter().enumerate() {
        for failure in [Failure::Before, Failure::After] {
            let target = seed.fork();
            target.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            match Engine::restore_new(&archive, &target, EngineOptions::default()) {
                Err(error) => storage_code(error, DurableErrorCode::Io),
                Ok(_) => panic!("restore ignored required I/O failure"),
            }
            target.assert_reached();
            target.power_loss();
            assert_eq!(image(&archive), archive_before);
            if index == plan.len() - 1 && failure == Failure::After {
                let db = open(&target);
                let rows: [RowUid; 2] = restored_rows(&db, "items", &rows).try_into().unwrap();
                assert_eq!(values(&db, rows), [10, 11]);
                assert_eq!(db.durability_metrics().unwrap().gauges.durable_position, 2);
                assert_ne!(
                    target.bytes("WRITER")[10 + 31..10 + 39],
                    archive.bytes("CURRENT")[10 + 31..10 + 39]
                );
                let ack = change(&db, rows, 40).unwrap();
                drop(db);
                target.power_loss();
                let db = open(&target);
                assert_eq!(values(&db, rows), [40, 41]);
                assert_eq!(
                    db.durability_metrics().unwrap().gauges.durable_position,
                    ack.position
                );
            } else {
                let before = image(&target);
                rejected_open(&target, DurableErrorCode::IncompleteCreate);
                assert_eq!(image(&target), before);
            }
        }
    }
}

fn blob_change(
    db: &Database,
    rows: &[RowUid],
    byte: u8,
) -> xadb_engine::database::Result<CommitReceipt> {
    let mut txn = db.begin(TransactionMode::ReadWrite)?;
    let table = txn.lookup_table("blobs")?;
    txn.execute_statement(
        rows.iter()
            .map(|row| Mutation::Replace {
                table: table.clone(),
                row: *row,
                values: vec![Value::Bytes(vec![byte; 65536])],
            })
            .collect(),
    )?;
    txn.commit()
}
fn blob_values(db: &Database, rows: &[RowUid], byte: u8) {
    let txn = db.begin(TransactionMode::ReadOnly).unwrap();
    let table = txn.lookup_table("blobs").unwrap();
    for row in rows {
        let value = txn.read(&table, *row).unwrap().unwrap();
        match value.value(0).unwrap() {
            Value::Bytes(bytes) => {
                assert_eq!(bytes.len(), 65536);
                assert!(bytes.iter().all(|value| *value == byte));
            }
            other => panic!("unexpected blob {other:?}"),
        }
    }
}
/// One same-process writer epoch with 31 half-MiB batches. The next batch must
/// rotate the real 16-MiB segment; no reduced product limit or fabricated WAL.
fn prepare_rotation(db: &Database, disk: &Disk) -> Vec<RowUid> {
    let table = db
        .create_table(TableSpec {
            name: "blobs".into(),
            columns: vec![Column {
                name: "value".into(),
                data_type: DataType::Binary,
                nullable: false,
            }],
        })
        .unwrap();
    let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
    let rows = txn
        .execute_statement(
            (0..8)
                .map(|_| Mutation::Insert {
                    table: table.clone(),
                    values: vec![Value::Bytes(vec![0; 65536])],
                })
                .collect(),
        )
        .unwrap()
        .inserted;
    txn.commit().unwrap();
    for byte in 1..=30 {
        blob_change(db, &rows, byte).unwrap();
    }
    assert_eq!(
        disk.names()
            .iter()
            .filter(|name| name.starts_with("wal-") && name.ends_with("-0000000000000002.axf"))
            .count(),
        0
    );
    rows
}

#[test]
fn checkpoint_pins_exact_root_while_commit_rotation_and_gc_progress() {
    let (disk, rows) = fixture();
    let db = open(&disk);
    let blobs = prepare_rotation(&db, &disk);
    let cut = db.durability_metrics().unwrap().gauges.durable_position;
    let old = db.begin(TransactionMode::ReadOnly).unwrap();
    let table = old.lookup_table("items").unwrap();
    std::thread::scope(|scope| {
        let pause = disk.pause_write("chunk-");
        let building = scope.spawn(|| db.checkpoint());
        let reached = pause.reached();
        assert!(reached.bytes > 0);
        assert_eq!(change(&db, rows, 20).unwrap().position, cut + 1);
        assert_eq!(blob_change(&db, &blobs, 31).unwrap().position, cut + 2);
        assert!(
            disk.names()
                .iter()
                .any(|name| name.ends_with("-0000000000000002.axf") && name.starts_with("wal-"))
        );
        let selected_writer = disk.bytes("WRITER");
        db.collect_garbage().unwrap();
        assert!(
            disk.names().contains(&reached.name),
            "GC retired a pinned checkpoint build file"
        );
        pause.release();
        let checkpoint = building.join().unwrap().unwrap();
        assert_eq!(checkpoint.position, cut);
        assert_eq!(
            disk.bytes("WRITER"),
            selected_writer,
            "CURRENT selection rewound the WRITER tip"
        );
    });
    assert_eq!(
        old.read(&table, rows[0]).unwrap().unwrap().value(0),
        Some(&Value::Int64(10))
    );
    drop(old);
    let archive = Disk::new();
    assert_eq!(db.backup(&archive).unwrap().position, cut);
    let target = Disk::new();
    let restored = Engine::restore_new(&archive, &target, EngineOptions::default()).unwrap();
    let restored_items = restored_rows(&restored, "items", &rows).try_into().unwrap();
    let restored_blobs = restored_rows(&restored, "blobs", &blobs);
    assert_eq!(values(&restored, restored_items), [10, 11]);
    blob_values(&restored, &restored_blobs, 30);
    drop(restored);
    drop(db);
    disk.power_loss();
    let db = open(&disk);
    assert_eq!(values(&db, rows), [20, 21]);
    blob_values(&db, &blobs, 31);
    assert_eq!(
        db.durability_metrics().unwrap().gauges.durable_position,
        cut + 2
    );
}

#[test]
fn rotation_seal_header_writer_and_commit_cuts_keep_the_predeclared_atomic_prefix() {
    let seed = Disk::new();
    let baseline = seed.fork();
    let db = Engine::create(&baseline, EngineOptions::default()).unwrap();
    let rows = prepare_rotation(&db, &baseline);
    baseline.reset_trace();
    let ack = blob_change(&db, &rows, 31).unwrap();
    let plan = baseline.receipts();
    drop(db);
    drop(baseline);
    assert_eq!(plan[0].event.operation, Operation::Write);
    assert!(plan[0].event.offset > 15 * 1024 * 1024);
    assert!(
        plan.iter()
            .any(|r| r.event.operation == Operation::Rename && r.event.name == "WRITER")
    );
    let batch = plan.len() - 2;
    assert_eq!(plan[batch].event.operation, Operation::Write);
    assert_eq!(plan[batch + 1].event.operation, Operation::Sync);
    for (index, receipt) in plan.iter().enumerate() {
        let mut failures = vec![Failure::Before, Failure::After];
        if receipt.event.operation == Operation::Write {
            failures.push(Failure::PrefixError(receipt.event.bytes / 2));
        }
        for failure in failures {
            let disk = seed.fork();
            let db = Engine::create(&disk, EngineOptions::default()).unwrap();
            let rows = prepare_rotation(&db, &disk);
            disk.inject(Fault {
                ordinal: index + 1,
                expected: receipt.event.clone(),
                failure,
            });
            match blob_change(&db, &rows, 31).unwrap_err() {
                EngineError::Storage {
                    error,
                    transaction: Some(_),
                } => {
                    assert_eq!(error.code, DurableErrorCode::Io);
                    assert!(error.fenced);
                    assert_eq!(
                        error.outcome,
                        if index < batch {
                            TransactionOutcome::Aborted
                        } else {
                            TransactionOutcome::Unknown
                        }
                    );
                }
                other => panic!("rotation failure lost its decision boundary: {other:?}"),
            }
            assert!(db.is_fenced());
            disk.assert_reached();
            drop(db);
            disk.power_loss();
            let retained = index == plan.len() - 1 && failure == Failure::After;
            let db = open(&disk);
            blob_values(&db, &rows, if retained { 31 } else { 30 });
            assert_eq!(
                db.durability_metrics().unwrap().gauges.durable_position,
                if retained {
                    ack.position
                } else {
                    ack.position - 1
                }
            );
        }
    }
}
