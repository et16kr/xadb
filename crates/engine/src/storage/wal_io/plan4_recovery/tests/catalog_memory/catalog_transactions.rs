//! Actual first CREATE and capsule completion. The CREATE bridge is a fixture,
//! not evidence of the still-pending shared product reservation admission.
use super::*;
use crate::storage::disk::{
    IsolationLevel, RetentionCause, StatementFinishError, TransactionFinishError,
};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

fn completed<T, U>(result: std::result::Result<T, TransactionFinishError<U>>) -> T {
    match result {
        Ok(value) => value,
        Err(
            TransactionFinishError::Before { error, .. }
            | TransactionFinishError::RollbackRequired { error, .. }
            | TransactionFinishError::Rejected(error)
            | TransactionFinishError::Fatal(error),
        ) => panic!("capsule completion: {error:?}"),
    }
}

#[test]
fn registered_forward_finishes_and_consumes_commit_or_abort() {
    case("CT01", || {
        for mode in [IsolationLevel::Serializable, IsolationLevel::ReadCommitted] {
            for commit in [true, false] {
                let oracle = Oracle::new();
                with_fresh!(oracle, |live,
                                     owner,
                                     flusher,
                                     status,
                                     undo,
                                     private,
                                     reader_private,
                                     core,
                                     fence,
                                     state| {
                    let _ = reader_private;
                    prepare_create(live, owner, flusher, status, undo);
                    {
                        let (mut writer, reader, mut flush) = live.split();
                        let tx = writer
                            .begin_catalog_transaction(owner, mode, status, private, &mut flush)
                            .unwrap();
                        let view = writer.begin_statement(owner, &tx).unwrap();
                        assert_eq!(view.revision(), 0);
                        assert_eq!(
                            writer
                                .catalog_test_forward_in_statement(
                                    owner,
                                    &view,
                                    undo,
                                    &table_spec("T"),
                                    private,
                                    &mut flush
                                )
                                .unwrap()
                                .object
                                .get(),
                            6
                        );
                        oracle.check(&writer, state, "R03");
                        assert_eq!(view.revision(), 1);
                        assert_eq!(view.test_catalog_reads(), 3);
                        assert!(writer.finish_statement(owner, view).is_ok());
                        assert_eq!(tx.test_write_identity().4, 512);
                        if commit {
                            assert_eq!(
                                completed(writer.commit_catalog_transaction(owner, tx, private))
                                    .q(),
                                1
                            );
                            oracle.check(&writer, state, "C04");
                        } else {
                            completed(
                                writer.abort_catalog_transaction(owner, tx, private, &mut flush),
                            );
                            oracle.check(&writer, state, "R06");
                        }
                        assert_eq!(reader.catalog_test_references().0.0, 0);
                        assert_eq!(writer.catalog_test_source_used(), 0);
                        assert!(!hook::observe(core).read_gate_closed);
                    }
                    assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                    assert_eq!(live.catalog_test_q(), u64::from(commit));
                    query(live, owner, flusher, private, u64::from(commit), 6, commit);
                    assert!(!fence.is_fenced());
                });
            }
        }
    });
}

#[test]
fn statement_and_commit_capacity_return_same_original_loans() {
    case("CT02", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
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
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_catalog_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            writer
                .catalog_test_forward_in_statement(
                    owner,
                    &view,
                    undo,
                    &table_spec("T"),
                    private,
                    &mut flush,
                )
                .unwrap();
            let before = state.lock().unwrap().images();
            let held = 65536
                - writer.catalog_test_commit_charges()[0]
                - writer.catalog_test_retirement_charge()
                + 1;
            let view = reader.catalog_test_with_held_source(held, || {
                match writer.finish_statement(owner, view) {
                    Err(StatementFinishError::Before { error, view }) => {
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        view
                    }
                    _ => panic!("same statement required"),
                }
            });
            assert_eq!(view.revision(), 1);
            assert_eq!(tx.test_write_identity().4, 1024);
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(writer.finish_statement(owner, view).is_ok());
            let identity = tx.test_write_identity();
            let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
            let tx = match writer.commit_catalog_transaction(&mut alien, tx, private) {
                Err(TransactionFinishError::Before { error, transaction }) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    transaction
                }
                _ => panic!("foreign owner must preserve capsule"),
            };
            let tx = reader.catalog_test_with_held_source(held, || {
                match writer.commit_catalog_transaction(owner, tx, private) {
                    Err(TransactionFinishError::Before { error, transaction }) => {
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        transaction
                    }
                    _ => panic!("same transaction required"),
                }
            });
            assert_eq!(tx.test_write_identity(), identity);
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(writer.catalog_test_source_used(), 0);
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(
                completed(writer.commit_catalog_transaction(owner, tx, private)).q(),
                1
            );
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn full_abort_retries_each_actual_completed_step_once() {
    case("CT03", || {
        for point in [Point::CatalogRollbackCompleted, Point::CatalogClrCompleted] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = reader_private;
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_catalog_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                writer
                    .catalog_test_forward_in_statement(
                        owner,
                        &view,
                        undo,
                        &table_spec("T"),
                        private,
                        &mut flush,
                    )
                    .unwrap();
                assert!(writer.finish_statement(owner, view).is_ok());
                let identity = tx.test_write_identity();
                let held = 65536
                    - writer.catalog_test_commit_charges()[0]
                    - writer.catalog_test_retirement_charge()
                    + 1;
                let (entered, events) = sync_channel(1);
                let (continue_at, release) = sync_channel(1);
                hook::install(core, point, entered, release);
                let (done, finished) = sync_channel(0);
                let tx = std::thread::scope(|scope| {
                    let reader_ref = &reader;
                    let controller = scope.spawn(move || {
                        events
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .unwrap();
                        reader_ref.catalog_test_with_held_source(held, || {
                            continue_at.send(Resume::Continue).unwrap();
                            finished
                                .recv_timeout(std::time::Duration::from_secs(10))
                                .unwrap();
                        });
                    });
                    let tx = match writer.abort_catalog_transaction(owner, tx, private, &mut flush)
                    {
                        Err(TransactionFinishError::RollbackRequired { error, transaction }) => {
                            assert!(matches!(error.cause, RetentionCause::Capacity));
                            transaction
                        }
                        _ => panic!("unfinished actual abort must preserve MustAbort"),
                    };
                    done.send(()).unwrap();
                    controller.join().unwrap();
                    tx
                });
                assert_eq!(
                    tx.test_write_identity(),
                    (identity.0, identity.1, identity.2, 2, 512)
                );
                oracle.check(
                    &writer,
                    state,
                    if point == Point::CatalogRollbackCompleted {
                        "R04"
                    } else {
                        "R05"
                    },
                );
                completed(writer.abort_catalog_transaction(owner, tx, private, &mut flush));
                oracle.check(&writer, state, "R06");
                assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 11);
                assert_eq!(reader.catalog_test_references().0.0, 0);
                assert!(!fence.is_fenced());
            });
        }
    });
}

fn additional<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    undo: bool,
) -> Plan4Target {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        let step = if undo {
            live.prepare_undo_slot(owner, &mut cursor)
        } else {
            live.prepare_status_slot(owner, &mut cursor)
        }
        .unwrap();
        match step {
            None => {
                let reserved = if undo {
                    live.reserve_undo(owner)
                } else {
                    live.reserve_status(owner)
                }
                .unwrap();
                return reserved.initialize().ok().expect("actual additional INIT");
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected preparation"),
        }
    }
    panic!("bounded additional INIT exhausted");
}

#[test]
fn actual_intervening_commit_conflicts_only_with_serializable_writer() {
    case("CT04", || {
        for mode in [IsolationLevel::Serializable, IsolationLevel::ReadCommitted] {
            let oracle = Oracle::new();
            with_fresh!(
                oracle,
                frames = 13,
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
                    let _ = reader_private;
                    let second_status = additional(live, owner, flusher, false);
                    let second_undo = additional(live, owner, flusher, true);
                    prepare_create(live, owner, flusher, status, undo);
                    prepare_create(live, owner, flusher, &second_status, &second_undo);
                    let (mut writer, reader, mut flush) = live.split();
                    let first = writer
                        .begin_catalog_transaction(owner, mode, status, private, &mut flush)
                        .unwrap();
                    let other = writer
                        .catalog_begin(owner, &second_status, private, &mut flush)
                        .unwrap()
                        .actor
                        .unwrap();
                    assert_eq!(
                        writer
                            .catalog_test_forward_beside_capsule(
                                owner,
                                other,
                                &second_undo,
                                &table_spec("OTHER"),
                                private,
                                &mut flush
                            )
                            .object
                            .get(),
                        6
                    );
                    writer.catalog_commit(owner, other, private).unwrap();
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 14);
                    let view = writer.begin_statement(owner, &first).unwrap();
                    assert_eq!(
                        view.q().unwrap(),
                        if mode == IsolationLevel::Serializable {
                            0
                        } else {
                            1
                        }
                    );
                    assert_eq!(
                        writer
                            .catalog_test_forward_in_statement(
                                owner,
                                &view,
                                undo,
                                &table_spec("T"),
                                private,
                                &mut flush
                            )
                            .unwrap()
                            .object
                            .get(),
                        7
                    );
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let before = state.lock().unwrap().images();
                    if mode == IsolationLevel::Serializable {
                        let first = match writer.commit_catalog_transaction(owner, first, private) {
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
                            _ => panic!("actual later catalog epoch must conflict"),
                        };
                        assert_eq!(first.test_write_identity().3, 2);
                        assert_eq!(state.lock().unwrap().images(), before);
                        completed(
                            writer.abort_catalog_transaction(owner, first, private, &mut flush),
                        );
                        assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 19);
                    } else {
                        assert_eq!(
                            completed(writer.commit_catalog_transaction(owner, first, private)).q(),
                            2
                        );
                        assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 17);
                    }
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert!(!fence.is_fenced());
                    assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                }
            );
        }
    });
}

#[test]
fn completed_commit_postcheck_capacity_is_fatal_not_before() {
    case("CT05", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = reader_private;
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_catalog_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            writer
                .catalog_test_forward_in_statement(
                    owner,
                    &view,
                    undo,
                    &table_spec("T"),
                    private,
                    &mut flush,
                )
                .unwrap();
            assert!(writer.finish_statement(owner, view).is_ok());
            let held = 65536
                - writer.catalog_test_commit_charges()[0]
                - writer.catalog_test_retirement_charge()
                + 1;
            let (entered, events) = sync_channel(1);
            let (continue_at, release) = sync_channel(1);
            hook::install(core, Point::CommitCompleted, entered, release);
            let (done, finished) = sync_channel(0);
            std::thread::scope(|scope| {
                let reader_ref = &reader;
                let controller = scope.spawn(move || {
                    events
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                    reader_ref.catalog_test_with_held_source(held, || {
                        continue_at.send(Resume::Continue).unwrap();
                        finished
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .unwrap();
                    });
                });
                assert!(matches!(
                    writer.commit_catalog_transaction(owner, tx, private),
                    Err(TransactionFinishError::Fatal(_))
                ));
                done.send(()).unwrap();
                controller.join().unwrap();
            });
            assert!(fence.is_fenced());
            // The actual Commit happened even though the caller received Fatal.
            let mut frames: Vec<_> = (1..=5).map(|n| oracle.native_frame(n)).collect();
            frames.extend((1..=4).map(|n| oracle.frame(&format!("C{n:02}"))));
            assert_wal(state, 1, &frames);
            assert!(
                writer.recovery_test_bank()[..32 * 1664]
                    .iter()
                    .all(|&b| b == 0)
            );
            // row17 is the selected before-bank; apply the independently
            // recorded Job-page after-image just as Oracle::check does.
            let row = oracle.row("C04");
            let mut jobs = oracle.transition.raw(&row[17]);
            let page = oracle.transition.raw(&row[31]);
            let page_index = (u64_at(&page, 32) - 2) as usize;
            for slot in 0..15 {
                let ordinal = page_index * 15 + slot;
                if ordinal < 64 {
                    let at = 16_352 - 1024 * (slot + 1);
                    jobs[ordinal * 1024..(ordinal + 1) * 1024]
                        .copy_from_slice(&page[at..at + 1024]);
                }
            }
            assert!(
                writer.catalog_test_jobs().as_flattened() == jobs,
                "C04 post-Commit Job bytes"
            );
        });
    });
}

#[test]
fn first_statement_and_empty_commit_limits_preserve_unmodified_capsule() {
    case("CT06", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = (reader_private, core);
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, _, mut flush) = live.split();
            let tx = writer
                .begin_catalog_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let before = state.lock().unwrap().images();
            let tx = match writer.commit_catalog_transaction(owner, tx, private) {
                Err(TransactionFinishError::Before { error, transaction }) => {
                    assert!(matches!(error.cause, RetentionCause::Unsupported));
                    transaction
                }
                _ => panic!("empty Commit remains unsupported"),
            };
            let view = writer.begin_statement(owner, &tx).unwrap();
            assert!(writer.finish_statement(owner, view).is_ok());
            let view = writer.begin_statement(owner, &tx).unwrap();
            assert!(matches!(
                writer
                    .catalog_test_forward_in_statement(
                        owner,
                        &view,
                        undo,
                        &table_spec("T"),
                        private,
                        &mut flush
                    )
                    .err()
                    .unwrap()
                    .cause,
                RetentionCause::Unsupported
            ));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(writer.finish_statement(owner, view).is_ok());
            completed(writer.abort_catalog_transaction(owner, tx, private, &mut flush));
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn completed_catalog_capsules_recover_in_current_build() {
    case("CT07", || {
        for aborted in [false, true] {
            let oracle = Oracle::new();
            let images =
                with_fresh!(oracle, |live,
                                     owner,
                                     flusher,
                                     status,
                                     undo,
                                     private,
                                     reader_private,
                                     core,
                                     fence,
                                     state| {
                    let _ = (reader_private, core);
                    prepare_create(live, owner, flusher, status, undo);
                    let (mut writer, _, mut flush) = live.split();
                    let tx = writer
                        .begin_catalog_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    writer
                        .catalog_test_forward_in_statement(
                            owner,
                            &view,
                            undo,
                            &table_spec("T"),
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    assert!(writer.finish_statement(owner, view).is_ok());
                    if aborted {
                        completed(writer.abort_catalog_transaction(owner, tx, private, &mut flush));
                    } else {
                        assert_eq!(
                            completed(writer.commit_catalog_transaction(owner, tx, private)).q(),
                            1
                        );
                    }
                    assert!(!fence.is_fenced());
                    state.lock().unwrap().images()
                });
            let directory = clone_directory(&images);
            let state = directory.state();
            {
                let mut maintenance = boxed::<12_582_912>();
                let mut dwb = boxed::<4_194_304>();
                let mut slot = boxed::<1_048_576>();
                let runtime = actual_runtime(&directory, &mut maintenance, &mut dwb, &mut slot);
                let capture = if aborted { 11 } else { 9 };
                assert_eq!(runtime.observation().source_lsn, capture);
                let core = PageConcurrency::new(runtime.session_fence()).unwrap();
                let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let mut backing = vec![[0xa7; PAGE_BYTES]; 9];
                let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
                let mut operation = boxed::<114_688>();
                let mut closed = runtime
                    .bind(&owner, &core, &mut frames, &mut operation)
                    .unwrap();
                let mut lsns = Vec::new();
                let mut ready = false;
                let mut last = capture;
                for _ in 0..256 {
                    let progress = closed.step(&mut owner, &mut flusher).unwrap();
                    let current = closed.inner.free_undo_test_state().unwrap().0.complete_lsn;
                    if current != last {
                        assert_eq!(current, last + 1);
                        lsns.push(current);
                        last = current;
                    }
                    if progress.ready {
                        ready = true;
                        break;
                    }
                }
                assert!(ready);
                assert_eq!(last, if aborted { 16 } else { 15 });
                assert_eq!(
                    lsns,
                    if aborted {
                        (12..=16).collect::<Vec<_>>()
                    } else {
                        (10..=15).collect::<Vec<_>>()
                    }
                );
                let (mut live, private) = closed.into_live_with_private().ok().unwrap();
                assert_eq!(live.catalog_test_isolation(), (1, 0, 0));
                assert!(
                    live.recovery_test_bank()[..32 * 1664]
                        .iter()
                        .all(|&b| b == 0)
                );
                assert_eq!(live.catalog_test_q(), u64::from(!aborted));
                query(
                    &mut live,
                    &mut owner,
                    &mut flusher,
                    private,
                    u64::from(!aborted),
                    6,
                    !aborted,
                );
                assert!(!runtime.session_fence().is_fenced());
                println!(
                    "CATALOG_CAPSULE_CURRENT_BUILD capture={capture} cleanup={lsns:?} aborted={aborted} old_capsules=0"
                );
            }
            assert_closed(&state);
        }
    });
}

#[test]
fn corrupted_completion_mode_or_mark_fences_before_wal() {
    case("CT08", || {
        for mark in [false, true] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = (reader_private, core);
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, _, mut flush) = live.split();
                let tx = writer
                    .begin_catalog_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                writer
                    .catalog_test_forward_in_statement(
                        owner,
                        &view,
                        undo,
                        &table_spec("T"),
                        private,
                        &mut flush,
                    )
                    .unwrap();
                assert!(writer.finish_statement(owner, view).is_ok());
                let before = state.lock().unwrap().images();
                tx.test_corrupt_catalog_completion(mark);
                assert!(matches!(
                    writer.commit_catalog_transaction(owner, tx, private),
                    Err(TransactionFinishError::Fatal(_))
                ));
                assert_eq!(state.lock().unwrap().images(), before);
                assert!(fence.is_fenced());
            });
        }
    });
}
