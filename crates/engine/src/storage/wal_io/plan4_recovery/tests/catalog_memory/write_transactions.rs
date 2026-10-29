//! Actual Begin ownership, no-effect rejection and consuming empty abort.
use super::*;
use crate::storage::disk::{
    IsolationLevel, RetentionCause, TransactionFinishError, TransactionOptions,
};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn issued_capsule_statement_and_empty_abort_return_original_loans() {
    case("WT01", || {
        for isolation in [IsolationLevel::Serializable, IsolationLevel::ReadCommitted] {
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
                prepare_create(live, owner, flusher, status, undo);
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let tx = writer
                        .begin_catalog_transaction(owner, isolation, status, private, &mut flush)
                        .unwrap();
                    oracle.check(&writer, state, "R01");
                    let identity = tx.test_write_identity();
                    assert_eq!(identity, (Some(oracle.actor("R01")), 0, 1, 1, 512));
                    let tx = match writer.finish_read_only(owner, tx) {
                        Err(TransactionFinishError::Before { error, transaction }) => {
                            assert!(matches!(error.cause, RetentionCause::WrongState));
                            assert_eq!(error.effect, Effect::Unchanged);
                            transaction
                        }
                        _ => panic!("writable must not finish as read-only"),
                    };
                    assert_eq!(tx.test_write_identity(), identity);
                    for sequence in 1..=2 {
                        let view = writer.begin_statement(owner, &tx).unwrap();
                        assert_eq!(view.q().unwrap(), 0);
                        assert_eq!(view.own_actor(), identity.0);
                        assert_eq!(view.test_sequence(), sequence);
                        assert_eq!(tx.test_write_identity().4, 1024);
                        let mut call = reader.catalog_statement_call(owner, &view).unwrap();
                        assert!(matches!(
                            call.lookup(
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private
                            )
                            .unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        call.finish_checked().unwrap();
                        assert_eq!(view.test_catalog_reads(), 1);
                        assert!(writer.finish_statement(owner, view).is_ok());
                    }
                    assert_eq!(tx.test_write_identity(), identity);
                    assert!(
                        writer
                            .abort_empty_transaction(owner, tx, private, &mut flush)
                            .is_ok()
                    );
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                    assert!(
                        writer.recovery_test_bank()[..32 * 1664]
                            .iter()
                            .all(|&b| b == 0)
                    );
                    let job = crate::storage::retention_page::catalog3::decode_job(
                        &writer.catalog_test_jobs()[0],
                        crate::api::Namespace {
                            database: identity.0.unwrap().id().database,
                            timeline: identity.0.unwrap().id().timeline,
                        },
                    )
                    .unwrap();
                    assert_eq!(job.source(), identity.0);
                    assert_eq!(
                        job.state(),
                        crate::storage::retention_page::catalog3::JobState::TerminalReady
                    );
                    assert_eq!(job.outcome(), 2);
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                    assert_eq!(writer.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                }
                assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                assert_eq!(live.catalog_test_q(), 0);
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
            });
        }
    });
}

#[test]
fn rejected_begin_returns_original_slot_snapshot_and_wal() {
    case("WT02", || {
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
            let before = state.lock().unwrap().images();
            let wrong = live.target(Plan4Page::Root).unwrap();
            {
                let (mut writer, reader, mut flush) = live.split();
                for chosen in [&wrong, status] {
                    let selected_owner = if std::ptr::eq(chosen, status) {
                        &mut *flusher
                    } else {
                        &mut *owner
                    };
                    let error = writer
                        .begin_catalog_transaction(
                            selected_owner,
                            IsolationLevel::Serializable,
                            chosen,
                            private,
                            &mut flush,
                        )
                        .err()
                        .unwrap();
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                }
                let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                assert_eq!(
                    writer
                        .begin_catalog_transaction(
                            &mut alien,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush
                        )
                        .err()
                        .unwrap()
                        .effect,
                    Effect::Unchanged
                );
                let held = 65536
                    - writer.catalog_test_commit_charges()[0]
                    - writer.catalog_test_retirement_charge()
                    + 1;
                reader.catalog_test_with_held_source(held, || {
                    let error = writer
                        .begin_catalog_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        )
                        .err()
                        .unwrap();
                    assert!(matches!(error.cause, RetentionCause::Capacity));
                    assert_eq!(error.effect, Effect::Unchanged);
                });
                let mut occupied = Vec::new();
                for _ in 0..32 {
                    occupied.push(
                        writer
                            .begin_transaction(
                                owner,
                                TransactionOptions {
                                    read_only: true,
                                    isolation: IsolationLevel::ReadCommitted,
                                },
                                private,
                            )
                            .unwrap(),
                    );
                }
                assert!(matches!(
                    writer
                        .begin_catalog_transaction(
                            owner,
                            IsolationLevel::ReadCommitted,
                            status,
                            private,
                            &mut flush
                        )
                        .err()
                        .unwrap()
                        .cause,
                    RetentionCause::Capacity
                ));
                drop(occupied);
                assert_eq!(writer.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
            }
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(live.catalog_test_isolation().1, 0);
            assert_eq!(live.catalog_test_isolation().2, 0);
            assert_eq!(live.catalog_test_locals(), (0, 0));
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn abort_capacity_after_rollback_returns_same_must_abort_capsule() {
    case("WT03", || {
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
            let _ = (reader_private, state);
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
            let identity = tx.test_write_identity();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            let (done, finished) = sync_channel(1);
            hook::install(core, Point::EmptyRollbackCompleted, entered, release);
            let held = 65536
                - writer.catalog_test_commit_charges()[0]
                - writer.catalog_test_retirement_charge()
                + 1;
            let reader_ref = &reader;
            let tx = std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(reader_ref.catalog_test_source_used(), 0);
                    reader_ref.catalog_test_with_held_source(held, || {
                        resume.send(Resume::Continue).unwrap();
                        finished.recv_timeout(WAIT).unwrap();
                    });
                });
                let tx = match writer.abort_empty_transaction(owner, tx, private, &mut flush) {
                    Err(TransactionFinishError::RollbackRequired { error, transaction }) => {
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        assert_eq!(error.effect, Effect::Unchanged);
                        transaction
                    }
                    _ => panic!("must return original unfinished abort"),
                };
                done.send(()).unwrap();
                controller.join().unwrap();
                tx
            });
            assert_eq!(
                tx.test_write_identity(),
                (identity.0, identity.1, identity.2, 2, 512)
            );
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 7);
            assert!(writer.begin_statement(owner, &tx).is_err());
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
            assert_eq!(reader.catalog_test_references().0.0, 0);
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn writable_drop_and_identity_faults_fence_without_io() {
    case("WT04", || {
        for fault in [0, 1, 2, 3, 4] {
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
                if fault == 0 {
                    drop(tx);
                } else if fault == 1 {
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    drop(view);
                    drop(tx);
                } else {
                    tx.test_corrupt_write_identity([0, 16, 32][fault - 2]);
                    assert_eq!(
                        writer.begin_statement(owner, &tx).err().unwrap().effect,
                        Effect::CallerMustResolve
                    );
                    drop(tx);
                }
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(
                    writer.free_undo_test_state().unwrap_err().effect,
                    Effect::CallerMustResolve
                );
            });
        }
    });
}

#[test]
fn actual_nonempty_actor_rejected_at_statement_begin_finish_and_empty_abort() {
    case("WT05", || {
        for active_view in [true, false] {
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
                let actor = tx.test_write_identity().0.unwrap();
                if !active_view {
                    writer.catalog_test_forward_beside_capsule(
                        owner,
                        actor,
                        undo,
                        &table_spec("T"),
                        private,
                        &mut flush,
                    );
                    oracle.check(&writer, state, "R03");
                    let before = state.lock().unwrap().images();
                    let identity = tx.test_write_identity();
                    assert!(matches!(
                        writer.begin_statement(owner, &tx).err().unwrap().cause,
                        RetentionCause::Unsupported
                    ));
                    let tx = match writer.abort_empty_transaction(owner, tx, private, &mut flush) {
                        Err(TransactionFinishError::Before { error, transaction }) => {
                            assert!(matches!(error.cause, RetentionCause::Unsupported));
                            assert_eq!(error.effect, Effect::Unchanged);
                            transaction
                        }
                        _ => panic!("nonempty actor must return unchanged capsule"),
                    };
                    assert_eq!(tx.test_write_identity(), identity);
                    assert_eq!(state.lock().unwrap().images(), before);
                    assert!(!fence.is_fenced());
                    drop(tx);
                    assert!(fence.is_fenced());
                    return;
                }
                let view = writer.begin_statement(owner, &tx).unwrap();
                writer.catalog_test_forward_beside_capsule(
                    owner,
                    actor,
                    undo,
                    &table_spec("T"),
                    private,
                    &mut flush,
                );
                oracle.check(&writer, state, "R03");
                let before = state.lock().unwrap().images();
                let view = match writer.finish_statement(owner, view) {
                    Err(crate::storage::disk::StatementFinishError::Before { error, view }) => {
                        assert!(matches!(error.cause, RetentionCause::Unsupported));
                        view
                    }
                    _ => panic!("changed actor cannot finish an empty statement"),
                };
                // The unclosed statement prevents any second begin. Its Drop must
                // preserve the actual root/ATT under a fence, never fake rollback.
                assert!(writer.begin_statement(owner, &tx).is_err());
                drop(view);
                drop(tx);
                assert_eq!(state.lock().unwrap().images(), before);
                assert!(fence.is_fenced());
            });
        }
    });
}

#[test]
fn new_capsule_begin_and_abort_images_recover_in_current_build() {
    case("WT06", || {
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
                    if aborted {
                        assert!(
                            writer
                                .abort_empty_transaction(owner, tx, private, &mut flush)
                                .is_ok()
                        );
                    } else {
                        drop(tx);
                    }
                    assert_eq!(fence.is_fenced(), !aborted);
                    state.lock().unwrap().images()
                });
            let directory = clone_directory(&images);
            let state = directory.state();
            {
                let mut maintenance = boxed::<12_582_912>();
                let mut dwb = boxed::<4_194_304>();
                let mut slot = boxed::<1_048_576>();
                let runtime = actual_runtime(&directory, &mut maintenance, &mut dwb, &mut slot);
                let capture = if aborted { 8 } else { 6 };
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
                assert_eq!(last, 9);
                assert_eq!(lsns, if aborted { vec![9] } else { vec![7, 8, 9] });
                let (mut live, private) = closed.into_live_with_private().ok().unwrap();
                assert_eq!(live.catalog_test_isolation(), (1, 0, 0));
                assert!(
                    live.recovery_test_bank()[..32 * 1664]
                        .iter()
                        .all(|&b| b == 0)
                );
                assert_eq!(live.catalog_test_q(), 0);
                query(&mut live, &mut owner, &mut flusher, private, 0, 6, false);
                assert!(!runtime.session_fence().is_fenced());
                println!(
                    "WRITABLE_CURRENT_BUILD capture={capture} cleanup={lsns:?} q=0 old_capsules=0"
                );
            }
            assert_closed(&state);
        }
    });
}

#[test]
fn readonly_abort_phase_corruption_cannot_drop_as_healthy() {
    case("WT07", || {
        for phase in [2, 3, 4] {
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
                let _ = (flusher, status, undo, reader_private, core);
                let before = state.lock().unwrap().images();
                let (mut writer, _, _) = live.split();
                let good = writer
                    .begin_transaction(
                        owner,
                        TransactionOptions {
                            read_only: true,
                            ..TransactionOptions::default()
                        },
                        private,
                    )
                    .unwrap();
                drop(good);
                assert!(!fence.is_fenced());
                let tx = writer
                    .begin_transaction(
                        owner,
                        TransactionOptions {
                            read_only: true,
                            ..TransactionOptions::default()
                        },
                        private,
                    )
                    .unwrap();
                tx.test_corrupt_readonly_phase(phase);
                drop(tx);
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(live.catalog_test_isolation(), (3, 1, 512));
            });
        }
    });
}
