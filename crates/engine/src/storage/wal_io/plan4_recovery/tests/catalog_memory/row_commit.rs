//! S3 actual consumer: independent Commit bytes after the frozen S2 input,
//! original durable/installation receipts, logical epochs and capsule cleanup.
use super::super::support::namespace;
use super::row_forward::{VALUE, files, wal_since};
use super::row_preflight::create_table;
use super::*;
use crate::storage::{
    disk::{IsolationLevel, RowMutationObservation, RowPreflightStep, TableStep},
    retention_page::row::{self as job3, JobScope, JobState},
    row::insert::driver::live::{LiveCall, commit_live, insert_live},
    row_record::ValueRef,
};

const COMMIT_FRAME: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-module-c3/row-commit/commit.bin"
));

#[test]
fn s3_r01_serializable_insert_commit_matches_independent_frame() {
    insert_commit(IsolationLevel::Serializable);
}
#[test]
fn s3_r01_read_committed_insert_commit_matches_independent_frame() {
    insert_commit(IsolationLevel::ReadCommitted);
}

fn insert_commit(mode: IsolationLevel) {
    case("S3-R01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let initial_epochs = live.catalog_test_epochs();
                let initial_isolation = live.catalog_test_isolation();
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let transaction = writer
                        .begin_row_transaction(owner, mode, &row_status, private, &mut flush)
                        .unwrap();
                    let view = writer.begin_statement(owner, &transaction).unwrap();
                    let actor = view.own_actor().unwrap();
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    let _ = settle!(writer, owner, view, table, private, flush, flusher);
                    let values = [ValueRef::Int64(VALUE)];
                    let rows: [&[ValueRef<'_>]; 1] = [&values];
                    let mut slots: [Option<RowMutationObservation>; 1] = [None];
                    {
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        match insert_live(&mut call, view, &table, private, &rows, &mut slots) {
                            Ok(1) => {}
                            Ok(n) => panic!("inserted {n}"),
                            Err(error) => panic!("insert failed {:?}", error.failure),
                        }
                    }
                    assert_eq!(transaction.test_statement_slot().0, 0);
                    let (before, _) = files(state);
                    let result = {
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        commit_live(&mut call, transaction, private)
                    };
                    let committed = match result {
                        Ok(committed) => committed,
                        Err(error) => panic!("commit failed {:?}", error.failure),
                    };
                    assert_eq!((committed.actor(), committed.q()), (actor, 2));
                    assert_eq!(wal_since(state, before), COMMIT_FRAME);
                    let jobs = writer.catalog_test_jobs();
                    let mut terminals = 0;
                    let mut roots = 0;
                    for bytes in jobs {
                        let job = job3::decode_job(bytes, namespace()).unwrap();
                        if job.source() != Some(actor) {
                            continue;
                        }
                        assert_eq!(job.scope(), Some(JobScope::Row));
                        match job.state() {
                            JobState::TerminalReady => {
                                terminals += 1;
                                assert_eq!(
                                    (job.outcome(), job.commit_q(), job.adoption_lsn()),
                                    (1, 2, 17)
                                );
                            }
                            JobState::RootRetained => roots += 1,
                            _ => panic!("unexpected root {:?}", job.state()),
                        }
                    }
                    assert_eq!((terminals, roots), (1, 1));
                    assert_eq!(
                        hex(writer.catalog_test_jobs().as_flattened()),
                        "da215cbf9b7a7f05bce7697ca8572b2fc4a3fc0532318c49818fb63b3ea638d5"
                    );
                    let expected = include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../tests/fixtures/dev3-module-c3/row-commit/expected.json"
                    ));
                    for (page, digest) in [
                        (
                            1,
                            "4d07641d079310eebc47e843101cc2fa53916ba582d7a2d4e26668a273c9a0b8",
                        ),
                        (
                            2,
                            "de95366ad7b38d6e0e2ed3644788e8464880d8f729aaf77457b40f499478c2a0",
                        ),
                        (
                            11,
                            "ea1022d5c40ad700adfc20e27561a9f2cc88d769e56deef6a02738aeb58dfdd1",
                        ),
                    ] {
                        assert!(expected.contains(digest));
                        let index = resident_index(&writer, page).expect("Commit target resident");
                        let mut bytes = boxed::<PAGE_BYTES>();
                        writer.catalog_test_with_pool(|pool| {
                            probe::diagnose(pool, index, &mut bytes)
                        });
                        assert_eq!(hex(&bytes[..]), digest, "installed page {page}");
                    }
                    assert!(
                        writer.recovery_test_bank()[..32 * 1664]
                            .chunks_exact(1664)
                            .all(|slot| slot[..1536].iter().all(|b| *b == 0)),
                        "Commit removed its ATT descriptor"
                    );
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!fence.is_fenced());
                    assert!(
                        !crate::transaction::concurrency::catalog_test_support::observe(core)
                            .read_gate_closed
                    );
                    owner.check_idle().unwrap();
                }
                assert_eq!(
                    live.catalog_test_epochs(),
                    (true, initial_epochs.1, vec![(2, 6, 2)])
                );
                assert_eq!(
                    (
                        live.catalog_test_isolation().1,
                        live.catalog_test_isolation().2
                    ),
                    (initial_isolation.1, initial_isolation.2)
                );
                assert_eq!(live.catalog_test_q(), 2);
                assert_state(live, 17, false);
                println!(
                    "S3_R01 mode={mode:?} frame={} q=2 catalog_epoch={} rows=1 ACK_AFTER_CLEANUP",
                    hex(COMMIT_FRAME),
                    initial_epochs.1
                );
            }
        );
    });
}

use crate::storage::disk::{RetentionCause, RetentionPhase, RowOwnedError, TransactionFinishError};
use crate::storage::row::insert::{produce_insert, validate_insert};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::{sync::mpsc::sync_channel, time::Duration};
const WAIT: Duration = Duration::from_secs(10);

macro_rules! ready {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident) => {
        match $writer
            .preflight_row($owner, &$view, &$table, $private, &mut $flush)
            .unwrap()
        {
            RowPreflightStep::Ready(preflight) => preflight,
            RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
        }
    };
}
macro_rules! accepted {
    ($call:expr) => {
        match $call {
            Ok(value) => value,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("Row operation {error:?}")
            }
        }
    };
}
macro_rules! inserted {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident, $flusher:ident) => {{
        let _ = settle!($writer, $owner, $view, $table, $private, $flush, $flusher);
        let mut preflight = ready!($writer, $owner, $view, $table, $private, $flush);
        let plan = validate_insert(
            &[ValueRef::Int64(VALUE)],
            preflight.validation_lane().unwrap(),
        )
        .unwrap();
        let mut reserved = accepted!(preflight.reserve(plan));
        let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
        let prepared = accepted!(reserved.prepare(proposal));
        accepted!(prepared.apply());
    }};
}
macro_rules! committed {
    ($call:expr) => {
        match $call {
            Ok(value) => value,
            Err(TransactionFinishError::Before { error, .. })
            | Err(TransactionFinishError::RollbackRequired { error, .. })
            | Err(TransactionFinishError::Rejected(error))
            | Err(TransactionFinishError::Fatal(error)) => panic!("Commit {error:?}"),
        }
    };
}

#[test]
fn s3_r02_empty_cancel_and_duplicate_table_roots() {
    case("S3-R02", || {
        // Frozen variants: empty, Cancel-only, two retained roots in one table.
        for variant in 0..3 {
            let oracle = Oracle::new();
            with_fresh!(
                oracle,
                frames = 16,
                |live,
                 owner,
                 flusher,
                 status,
                 undo,
                 private,
                 reader_private,
                 core,
                 fence,
                 state| {
                    let (row_status, _) =
                        create_table(live, owner, flusher, status, undo, private, true);
                    if variant == 2 {
                        row_preflight::system_init(live, owner, flusher, true);
                    }
                    let before_epochs = live.catalog_test_epochs();
                    {
                        let (mut writer, reader, mut flush) = live.split();
                        let tx = writer
                            .begin_row_transaction(
                                owner,
                                IsolationLevel::Serializable,
                                &row_status,
                                private,
                                &mut flush,
                            )
                            .unwrap();
                        if variant != 0 {
                            let view = writer.begin_statement(owner, &tx).unwrap();
                            let table =
                                issue_table!(reader, owner, view, reader_private, flush, flusher);
                            if variant == 1 {
                                let _ =
                                    settle!(writer, owner, view, table, private, flush, flusher);
                                let mut preflight =
                                    ready!(writer, owner, view, table, private, flush);
                                let plan = validate_insert(
                                    &[ValueRef::Int64(VALUE)],
                                    preflight.validation_lane().unwrap(),
                                )
                                .unwrap();
                                accepted!(accepted!(preflight.reserve(plan)).cancel());
                                assert_eq!(
                                    view.test_table_masks(),
                                    (1, 1),
                                    "Cancel retains conservative mask"
                                );
                            } else {
                                inserted!(writer, owner, view, table, private, flush, flusher);
                                inserted!(writer, owner, view, table, private, flush, flusher);
                            }
                            let wal = files(state).0;
                            assert!(writer.finish_statement(owner, view).is_ok());
                            assert_eq!(files(state).0, wal, "statement end has no WAL");
                        }
                        assert_eq!(committed!(writer.commit_row(owner, tx, private)).q(), 2);
                        assert_eq!(reader.catalog_test_source_used(), 0);
                    }
                    assert_eq!(
                        live.catalog_test_epochs(),
                        (
                            true,
                            before_epochs.1,
                            vec![(2, 6, if variant == 2 { 2 } else { 1 })]
                        )
                    );
                    assert_eq!(
                        (
                            live.catalog_test_isolation().1,
                            live.catalog_test_isolation().2
                        ),
                        (0, 0)
                    );
                    assert!(!hook::observe(core).read_gate_closed && !fence.is_fenced());
                    println!("S3_R02 variant={variant} q=2");
                }
            );
        }
    });
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "the unchanged capsule remains in its 512-byte lease"
)]
fn s3_r03_scope_foreign_capacity_reserved_and_aborting_refusals() {
    case("S3-R03", || {
        for variant in 0..5 {
            let oracle = Oracle::new();
            with_fresh!(
                oracle,
                frames = 12,
                |live,
                 owner,
                 flusher,
                 status,
                 undo,
                 private,
                 reader_private,
                 core,
                 fence,
                 state,
                 foreign| {
                    let _ = reader_private;
                    let (row_status, _) =
                        create_table(live, owner, flusher, status, undo, private, false);
                    let (mut writer, reader, mut flush) = live.split();
                    let tx = if variant == 0 {
                        writer
                            .begin_catalog_transaction(
                                owner,
                                IsolationLevel::Serializable,
                                &row_status,
                                private,
                                &mut flush,
                            )
                            .unwrap()
                    } else {
                        writer
                            .begin_row_transaction(
                                owner,
                                IsolationLevel::Serializable,
                                &row_status,
                                private,
                                &mut flush,
                            )
                            .unwrap()
                    };
                    if variant == 4 {
                        writer
                            .row_rollback_start(
                                owner,
                                tx.test_write_identity().0.unwrap(),
                                private,
                                &mut flush,
                            )
                            .unwrap();
                    }
                    let identity = tx.test_write_identity();
                    let before = files(state);
                    let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                    let injected = (variant == 3).then(|| tx.test_commit_reserved_once());
                    let result = if variant == 2 {
                        let [shared, _, _, _, _] = writer.catalog_test_commit_charges();
                        let held = 65536 - shared - writer.catalog_test_retirement_charge() + 1;
                        reader.catalog_test_with_held_source(held, || {
                            writer.commit_row(owner, tx, private)
                        })
                    } else {
                        writer.commit_row(
                            if variant == 1 { &mut alien } else { owner },
                            tx,
                            private,
                        )
                    };
                    drop(injected);
                    let tx = match result {
                        Err(TransactionFinishError::Before { error, transaction }) => {
                            assert_eq!(error.effect, Effect::Unchanged);
                            match variant {
                                0 => assert!(matches!(error.cause, RetentionCause::Unsupported)),
                                1 => assert!(matches!(error.cause, RetentionCause::Concurrency(_))),
                                2 => assert!(matches!(error.cause, RetentionCause::Capacity)),
                                3 | 4 => assert!(matches!(error.cause, RetentionCause::WrongState)),
                                _ => unreachable!(),
                            }
                            transaction
                        }
                        _ => panic!("expected Before variant {variant}"),
                    };
                    assert_eq!(tx.test_write_identity(), identity);
                    assert_eq!(files(state), before);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                    if matches!(variant, 1..=3) {
                        assert_eq!(committed!(writer.commit_row(owner, tx, private)).q(), 2);
                    } else {
                        drop(tx);
                    }
                }
            );
        }
    });
}

fn io_refusal(action: port::Action, sync: bool) {
    case("S3-R06-IO", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (reader_private, core);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, false);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let at = files(state).0;
                {
                    let mut state = state.lock().unwrap();
                    state.events.clear();
                    state.fault_hits = 0;
                    state.fault = Some(port::Fault {
                        key: port::Key::new(
                            if sync { "sync" } else { "write" },
                            "d3-wal-00000001",
                            if sync { 0 } else { at as u64 },
                            1,
                        ),
                        action,
                    });
                }
                assert!(matches!(
                    writer.commit_row(owner, tx, private),
                    Err(TransactionFinishError::Fatal(_))
                ));
                assert!(fence.is_fenced());
                assert_eq!(reader.catalog_test_horizon().0, 1);
                assert_eq!(state.lock().unwrap().fault_hits, 1);
                assert!(
                    reader.catalog_test_source_used() > 0,
                    "fatal retains its accounting"
                );
                println!("S3_R06 action={action:?} sync={sync} ACK=0");
            }
        );
    });
}
#[test]
fn s3_r06_partial_write_fences_without_ack() {
    io_refusal(port::Action::PrefixErrorBytes(96), false);
}
#[test]
fn s3_r06_sync_after_error_fences_without_ack() {
    io_refusal(port::Action::AfterError, true);
}

macro_rules! issue_object {
    ($reader:ident,$owner:ident,$view:ident,$reader_private:ident,$flush:ident,$flusher:ident,$object:expr) => {{
        let mut issued = None;
        for _ in 0..8 {
            match $reader
                .catalog_statement_call($owner, &$view)
                .unwrap()
                .finish_table(
                    ObjectId::new($object).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    $reader_private,
                )
                .unwrap()
            {
                TableStep::Table(table) => {
                    issued = Some(table);
                    break;
                }
                TableStep::FlushRequired { target } => flush_target!($flush, $flusher, target),
                TableStep::Missing => panic!("committed table missing"),
            }
        }
        issued.expect("bounded table issue")
    }};
}

fn conflict(mode: IsolationLevel, same: bool) {
    case("S3-R04", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 20,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let second_undo = row_preflight::system_init(live, owner, flusher, true);
                let catalog_status = row_preflight::system_init(live, owner, flusher, false);
                let concurrent_status = row_preflight::system_init(live, owner, flusher, false);
                prepare_create(live, owner, flusher, &catalog_status, &second_undo);
                {
                    let (mut writer, _, mut flush) = live.split();
                    let tx = writer
                        .begin_catalog_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &catalog_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    writer
                        .catalog_test_forward_in_statement(
                            owner,
                            &view,
                            &second_undo,
                            &table_spec("U"),
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    assert!(writer.finish_statement(owner, view).is_ok());
                    assert_eq!(
                        committed!(writer.commit_catalog_transaction(owner, tx, private)).q(),
                        2
                    );
                }
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let first = writer
                        .begin_row_transaction(owner, mode, &row_status, private, &mut flush)
                        .unwrap();
                    let view = writer.begin_statement(owner, &first).unwrap();
                    let _table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    assert_eq!(view.test_table_masks().0, 1);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let second = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::ReadCommitted,
                            &concurrent_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &second).unwrap();
                    let table = issue_object!(
                        reader,
                        owner,
                        view,
                        reader_private,
                        flush,
                        flusher,
                        if same { 6 } else { 7 }
                    );
                    inserted!(writer, owner, view, table, private, flush, flusher);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    assert_eq!(committed!(writer.commit_row(owner, second, private)).q(), 3);
                    let before = files(state);
                    if mode == IsolationLevel::Serializable && same {
                        let first = match writer.commit_row(owner, first, private) {
                            Err(TransactionFinishError::RollbackRequired {
                                error,
                                transaction,
                            }) => {
                                assert!(matches!(
                                    error.cause,
                                    RetentionCause::SerializationConflict
                                ));
                                transaction
                            }
                            _ => panic!("same-table conflict must require rollback"),
                        };
                        assert_eq!(files(state), before);
                        let first = match writer.commit_row(owner, first, private) {
                            Err(TransactionFinishError::RollbackRequired {
                                transaction, ..
                            }) => transaction,
                            _ => panic!("MustAbort cannot commit"),
                        };
                        assert_eq!(files(state), before);
                        assert!(
                            writer
                                .abort_empty_transaction(owner, first, private, &mut flush)
                                .is_ok()
                        );
                    } else {
                        assert_eq!(committed!(writer.commit_row(owner, first, private)).q(), 4);
                    }
                    assert_eq!(reader.catalog_test_source_used(), 0);
                }
                let epochs = live.catalog_test_epochs();
                assert_eq!(epochs.1, 2, "Row never publishes a namespace epoch");
                assert_eq!(
                    epochs.2,
                    vec![
                        (2, 6, if same { 3 } else { 1 }),
                        (2, 7, if same { 2 } else { 3 })
                    ]
                );
                assert_eq!(
                    (
                        live.catalog_test_isolation().1,
                        live.catalog_test_isolation().2
                    ),
                    (0, 0)
                );
                assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                println!("S3_R04 mode={mode:?} same={same} logical_epochs={epochs:?}");
            }
        );
    });
}
#[test]
fn s3_r04_serializable_same_table_requires_rollback() {
    conflict(IsolationLevel::Serializable, true);
}
#[test]
fn s3_r04_serializable_other_table_commits() {
    conflict(IsolationLevel::Serializable, false);
}
#[test]
fn s3_r04_read_committed_same_table_commits() {
    conflict(IsolationLevel::ReadCommitted, true);
}

#[test]
fn s3_r02_cancel_statement_can_empty_abort() {
    case("S3-R02-abort", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let plan = validate_insert(
                    &[ValueRef::Int64(VALUE)],
                    preflight.validation_lane().unwrap(),
                )
                .unwrap();
                accepted!(accepted!(preflight.reserve(plan)).cancel());
                let wal = files(state).0;
                assert!(writer.finish_statement(owner, view).is_ok());
                assert_eq!(files(state).0, wal);
                assert!(
                    writer
                        .abort_empty_transaction(owner, tx, private, &mut flush)
                        .is_ok()
                );
                assert_eq!(reader.catalog_test_horizon().0, 1);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
            }
        );
    });
}

#[test]
fn s3_r03_active_statement_is_before_busy() {
    case("S3-R03-active", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = reader_private;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, false);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                // Safe Rust can forget a borrow-bearing view. Its real slot stays
                // active; this deliberately stranded fixture cannot manufacture a close.
                std::mem::forget(writer.begin_statement(owner, &tx).unwrap());
                let before = files(state);
                let identity = tx.test_write_identity();
                match writer.commit_row(owner, tx, private) {
                    Err(TransactionFinishError::Before { error, transaction }) => {
                        assert!(matches!(error.cause, RetentionCause::Busy));
                        assert_eq!(transaction.test_write_identity(), identity);
                        assert_eq!(files(state), before);
                        assert_eq!(reader.catalog_test_source_used(), 0);
                        assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                        drop(transaction);
                    }
                    _ => panic!("active statement must refuse Commit"),
                }
            }
        );
    });
}

#[test]
fn s3_r05_commit_replays_divergent_forward_before_stage() {
    case("S3-R05", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let plan = validate_insert(
                    &[ValueRef::Int64(VALUE)],
                    preflight.validation_lane().unwrap(),
                )
                .unwrap();
                let mut reserved = accepted!(preflight.reserve(plan));
                let pool = reader.catalog_test_pool();
                row_forward::move_root_cursor(pool);
                reserved.test_mirror_aging(&row_forward::page_of(pool, 1));
                reserved.test_refold_draft().unwrap();
                let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                accepted!(accepted!(reserved.prepare(proposal)).apply());
                assert!(writer.finish_statement(owner, view).is_ok());
                let before = files(state);
                let error = match writer.commit_row(owner, tx, private) {
                    Err(TransactionFinishError::Fatal(error)) => error,
                    _ => panic!("canonical replay must reject divergent Forward"),
                };
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Source, Effect::CallerMustResolve)
                );
                assert_eq!(files(state), before, "no new Commit WAL");
                assert!(fence.is_fenced());
                assert_eq!(reader.catalog_test_horizon().0, 1);
                assert!(!hook::observe(core).read_gate_closed);
                println!("S3_R05 {error:?} ACK=0");
            }
        );
    });
}

#[test]
fn s3_r06_stale_preparation_returns_same_capsule_then_retry() {
    case("S3-R06-stale", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = reader_private;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, false);
                let root = live.target(Plan4Page::Root).unwrap();
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let identity = tx.test_write_identity();
                let wal = files(state).0;
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, Point::CommitPrepared, entered, release);
                std::thread::scope(|scope| {
                    let (rejected, rejection) = sync_channel(1);
                    let (retry, retry_start) = sync_channel(1);
                    let worker=scope.spawn(move || {
                    let mut other=core.try_owner(OwnerRole::Foreground).unwrap();
                    let tx=match writer.commit_row(&mut other,tx,private) {
                        Err(TransactionFinishError::Before{error,transaction})=> {
                            assert!(matches!(error.cause,RetentionCause::Frame(error) if error.cause==crate::storage::dirty_frame::Cause::Stale));
                            assert_eq!(transaction.test_write_identity(),identity); transaction
                        }, _=>panic!("prepared flush must be stale"),
                    };
                    rejected.send(()).unwrap(); retry_start.recv_timeout(WAIT).unwrap();
                    assert_eq!(committed!(writer.commit_row(&mut other,tx,private)).q(),2);
                });
                    arrived.recv_timeout(WAIT).unwrap();
                    flush_target!(flush, flusher, root);
                    let after_flush = files(state);
                    assert_eq!(after_flush.0, wal);
                    resume.send(Resume::Continue).unwrap();
                    rejection.recv_timeout(WAIT).unwrap();
                    assert_eq!(files(state), after_flush);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                    retry.send(()).unwrap();
                    worker.join().unwrap();
                });
            }
        );
    });
}

fn fatal_boundary(point: Point) {
    case("S3-R06-boundary", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (reader_private, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, false);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, point, entered, release);
                std::thread::scope(|scope| {
                    let controlling = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        fence.fence();
                        resume.send(Resume::Continue).unwrap();
                    });
                    assert!(matches!(
                        writer.commit_row(owner, tx, private),
                        Err(TransactionFinishError::Fatal(_))
                    ));
                    controlling.join().unwrap();
                });
                assert!(fence.is_fenced());
                assert_eq!(
                    reader.catalog_test_horizon().0,
                    if point == Point::CommitCompleted {
                        2
                    } else {
                        1
                    }
                );
                println!("S3_R06 boundary={point:?} ACK=0");
            }
        );
    });
}
#[test]
fn s3_r06_partial_install_is_fatal() {
    fatal_boundary(Point::InstalledSecondPage);
}
#[test]
fn s3_r06_before_publication_is_fatal() {
    fatal_boundary(Point::BeforePublish);
}
#[test]
fn s3_r06_cleanup_failure_is_fatal() {
    fatal_boundary(Point::CommitCompleted);
}

#[test]
fn s3_r07_pending_full_read_drain_flusher_and_old_q() {
    case("S3-R07", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let root = live.target(Plan4Page::Root).unwrap();
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                inserted!(writer, owner, view, table, private, flush, flusher);
                assert!(writer.finish_statement(owner, view).is_ok());
                let pending_charge = writer.catalog_test_commit_charges()[2] as u64;
                let full_charge = reader.catalog_test_fixed_charges()[1] as u64;
                let (pending_entered, pending_arrived) = sync_channel(1);
                let (pending_resume, pending_release) = sync_channel(1);
                hook::install(core, Point::Pending, pending_entered, pending_release);
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let (registered, ready) = sync_channel(1);
                    let (start, started) = sync_channel(1);
                    let reading = scope.spawn(move || {
                        let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                        assert_eq!(snapshot.q(), 1);
                        registered.send(()).unwrap();
                        started.recv_timeout(WAIT).unwrap();
                        let error = match reader_ref.lookup_catalog(
                            &mut separate,
                            &snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private,
                        ) {
                            Err(error) => error,
                            Ok(_) => panic!("captured lookup must meet Drain"),
                        };
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(format!("{error:?}").contains("InstallationPending"));
                        snapshot.release_checked().unwrap();
                        separate.check_idle().unwrap();
                    });
                    ready.recv_timeout(WAIT).unwrap();
                    let (acked, ack) = sync_channel(1);
                    let committing = scope.spawn(move || {
                        let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                        let commit = committed!(writer.commit_row(&mut other, tx, private));
                        assert_eq!(commit.q(), 2);
                        other.check_idle().unwrap();
                        acked.send(()).unwrap();
                    });
                    pending_arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(
                        (
                            hook::observe(core).installation,
                            hook::observe(core).read_gate_closed
                        ),
                        ("Pending", false)
                    );
                    assert_eq!(reader_ref.catalog_test_source_used(), pending_charge);
                    assert!(ack.try_recv().is_err());
                    flush_target!(flush, flusher, root);
                    let (captured, capture_arrived) = sync_channel(1);
                    let (read_resume, read_release) = sync_channel(1);
                    hook::install(core, Point::LookupCaptured, captured, read_release);
                    start.send(()).unwrap();
                    capture_arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(reader_ref.catalog_test_references(), ((2, 0, 0), false));
                    assert_eq!(
                        reader_ref.catalog_test_source_used(),
                        pending_charge + full_charge
                    );
                    let (waiting, wait_arrived) = sync_channel(1);
                    let (drain_resume, drain_release) = sync_channel(1);
                    hook::install(core, Point::DrainWaiting, waiting, drain_release);
                    pending_resume.send(Resume::Continue).unwrap();
                    wait_arrived.recv_timeout(WAIT).unwrap();
                    let obs = hook::observe(core);
                    assert_eq!(
                        (
                            obs.installation,
                            obs.active_apply,
                            obs.active_content,
                            obs.active_read_operations
                        ),
                        ("Draining", 0, 0, 1)
                    );
                    assert_eq!(reader_ref.catalog_test_horizon().0, 1);
                    assert!(ack.try_recv().is_err());
                    read_resume.send(Resume::Continue).unwrap();
                    reading.join().unwrap();
                    assert_eq!(reader_ref.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(reader_ref.catalog_test_source_used(), pending_charge);
                    let (publishing, publish_arrived) = sync_channel(1);
                    let (publish_resume, publish_release) = sync_channel(1);
                    hook::install(core, Point::BeforePublish, publishing, publish_release);
                    drain_resume.send(Resume::Continue).unwrap();
                    publish_arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(hook::observe(core).installation, "Open");
                    assert_eq!(reader_ref.catalog_test_horizon().0, 1);
                    assert!(ack.try_recv().is_err());
                    let snapshot = reader_ref.catalog_snapshot(owner).unwrap();
                    assert_eq!(snapshot.q(), 1);
                    snapshot.release_checked().unwrap();
                    publish_resume.send(Resume::Continue).unwrap();
                    ack.recv_timeout(WAIT).unwrap();
                    committing.join().unwrap();
                });
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_horizon().0, 2);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert!(!fence.is_fenced());
                println!(
                    "S3_R07 Pending full lookup + original flusher; Drain waited; Q=1 before publication; ACK after cleanup WAL={}",
                    files(state).0
                );
            }
        );
    });
}

#[cfg(target_os = "linux")]
#[path = "row_commit/native.rs"]
mod native;
