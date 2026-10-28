//! Actual shared CREATE with the original statement, admission and private loan.
use super::*;
use crate::storage::disk::{CatalogCreateError, IsolationLevel, TransactionOptions};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn actual_shared_create_completes_beside_long_statement() {
    case("CCA01", || {
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
                    prepare_create(live, owner, flusher, status, undo);
                    let (mut writer, reader, mut flush) = live.split();
                    let first = writer
                        .begin_transaction(
                            owner,
                            TransactionOptions {
                                isolation: mode,
                                read_only: true,
                            },
                            reader_private,
                        )
                        .unwrap();
                    let first_view = writer.begin_statement(owner, &first).unwrap();
                    let mut call = reader.catalog_statement_call(owner, &first_view).unwrap();
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
                    assert_eq!(first_view.test_catalog_reads(), 1);
                    let tx = writer
                        .begin_catalog_transaction(owner, mode, status, private, &mut flush)
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    let fixed = writer.catalog_test_retirement_charge();
                    let address = private.as_ptr();
                    let definition = table_spec("T");
                    let reserved = writer
                        .reserve_catalog_create(
                            owner,
                            &view,
                            undo,
                            &definition,
                            private,
                            &mut flush,
                        )
                        .ok()
                        .expect("shared Reserve");
                    oracle.check(&reserved, state, "R02");
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), fixed as u64);
                    assert_eq!(first_view.q().unwrap(), 0);
                    assert_eq!(view.test_catalog_reads(), 3);
                    let (object, private) = reserved
                        .initialize()
                        .ok()
                        .expect("shared Forward")
                        .into_parts();
                    assert_eq!((object.object.get(), private.as_ptr()), (6, address));
                    oracle.check(&writer, state, "R03");
                    assert_eq!(view.revision(), 1);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    if commit {
                        assert_eq!(
                            writer
                                .commit_catalog_transaction(owner, tx, private)
                                .ok()
                                .unwrap()
                                .q(),
                            1
                        );
                        oracle.check(&writer, state, "C04");
                    } else {
                        assert!(
                            writer
                                .abort_catalog_transaction(owner, tx, private, &mut flush)
                                .is_ok()
                        );
                        oracle.check(&writer, state, "R06");
                    }
                    assert_eq!(first_view.q().unwrap(), 0);
                    assert!(writer.finish_statement(owner, first_view).is_ok());
                    let ended = writer.finish_read_only(owner, first);
                    if commit && mode == IsolationLevel::Serializable {
                        assert!(
                            matches!(ended, Err(crate::storage::disk::TransactionFinishError::Rejected(error)) if matches!(error.cause, crate::storage::disk::RetentionCause::SerializationConflict))
                        );
                    } else {
                        assert!(ended.is_ok());
                    }
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                    assert!(!fence.is_fenced());
                });
            }
        }
    });
}

#[test]
fn before_cancel_and_rereserve_preserve_original_loans() {
    case("CCA02", || {
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
            let definition = table_spec("T");
            let address = private.as_ptr();
            let mut reserved = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap();
            let identity = reserved.catalog_test_lease_identity();
            let wal = reserved.catalog_test_wal_state();
            reserved.catalog_test_lease_fault(0);
            let mut reserved = match reserved.initialize() {
                Err(RetentionReservationError::Before { error, reservation }) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    reservation
                }
                _ => panic!("same active reservation required"),
            };
            assert_eq!(reserved.catalog_test_lease_identity(), identity);
            assert_eq!(reserved.catalog_test_wal_state(), wal);
            assert!(hook::observe(core).read_gate_closed);
            assert_ne!(reader.catalog_test_source_used(), 0);
            reserved.catalog_test_lease_fault(1);
            let private = reserved.cancel().ok().unwrap().into_private();
            assert_eq!(private.as_ptr(), address);
            oracle.check(&writer, state, "K03");
            assert_eq!((view.revision(), view.test_catalog_reads()), (0, 3));
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert!(!hook::observe(core).read_gate_closed);
            let reserved = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap();
            assert_eq!(reserved.object().object.get(), 7);
            let (object, private) = reserved.initialize().ok().unwrap().into_parts();
            assert_eq!((object.object.get(), private.as_ptr()), (7, address));
            assert!(writer.finish_statement(owner, view).is_ok());
            assert_eq!(
                writer
                    .commit_catalog_transaction(owner, tx, private)
                    .ok()
                    .unwrap()
                    .q(),
                1
            );
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 11);
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn initial_capacity_and_foreign_owner_return_private_without_wal() {
    case("CCA03", || {
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
            let definition = table_spec("T");
            let address = private.as_ptr();
            let before = state.lock().unwrap().images();
            let held = 65536
                - writer.catalog_test_commit_charges()[0]
                - writer.catalog_test_retirement_charge()
                + 1;
            let private = reader.catalog_test_with_held_source(held, || {
                match writer.reserve_catalog_create(
                    owner,
                    &view,
                    undo,
                    &definition,
                    private,
                    &mut flush,
                ) {
                    Err(CatalogCreateError::Before { error, private }) => {
                        assert!(matches!(
                            error.cause,
                            crate::storage::disk::RetentionCause::Capacity
                        ));
                        private
                    }
                    _ => panic!("source Capacity must return private"),
                }
            });
            assert_eq!(view.test_catalog_reads(), 0);
            let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
            let private = match writer.reserve_catalog_create(
                &mut alien,
                &view,
                undo,
                &definition,
                private,
                &mut flush,
            ) {
                Err(CatalogCreateError::Before { error, private }) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    private
                }
                _ => panic!("foreign owner Before required"),
            };
            assert_eq!(private.as_ptr(), address);
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert!(!hook::observe(core).read_gate_closed);
            let private = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap()
                .cancel()
                .ok()
                .unwrap()
                .into_private();
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(
                writer
                    .abort_catalog_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn active_drop_fences_without_cancel_or_claim_erasure() {
    case("CCA04", || {
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
            let definition = table_spec("T");
            let fixed = writer.catalog_test_retirement_charge();
            let reserved = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap();
            let before = state.lock().unwrap().images();
            let claim = crate::storage::dirty_frame::init_test_support::pair_close_state(
                reserved.catalog_test_pool(),
            );
            drop(reserved);
            assert!(fence.is_fenced());
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(
                crate::storage::dirty_frame::init_test_support::pair_close_state(
                    writer.catalog_test_pool()
                ),
                claim
            );
            assert_eq!(reader.catalog_test_source_used(), fixed as u64);
            assert!(owner.check_idle().is_err());
            assert!(!hook::observe(core).read_gate_closed);
        });
    });
}

#[test]
fn completed_forward_and_cancel_cleanup_failure_are_fatal() {
    case("CCA05", || {
        for cancel in [false, true] {
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
                let definition = table_spec("T");
                let reserved = writer
                    .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                    .ok()
                    .unwrap();
                let (entered, events) = sync_channel(1);
                let (proceed, release) = sync_channel(1);
                hook::install(core, Point::CatalogCreateCompleted, entered, release);
                std::thread::scope(|scope| {
                    let reader = &reader;
                    let controller = scope.spawn(move || {
                        events
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .unwrap();
                        reader.catalog_test_corrupt_short_source();
                        proceed.send(Resume::Continue).unwrap();
                    });
                    let error = if cancel {
                        reserved.cancel().err()
                    } else {
                        reserved.initialize().err()
                    };
                    assert!(matches!(error, Some(RetentionReservationError::Fatal(_))));
                    controller.join().unwrap();
                });
                assert!(fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                let mut frames: Vec<_> = (1..=5).map(|n| oracle.native_frame(n)).collect();
                frames.extend(
                    (1..=3).map(|n| {
                        oracle.frame(&format!("{}{n:02}", if cancel { "K" } else { "R" }))
                    }),
                );
                assert_wal(state, 1, &frames);
                assert_eq!(view.revision(), u64::from(!cancel));
            });
        }
    });
}

#[test]
fn shared_create_outputs_recover_in_current_build() {
    case("CCA06", || {
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
                    let definition = table_spec("T");
                    let private = writer
                        .reserve_catalog_create(
                            owner,
                            &view,
                            undo,
                            &definition,
                            private,
                            &mut flush,
                        )
                        .ok()
                        .unwrap()
                        .initialize()
                        .ok()
                        .unwrap()
                        .into_parts()
                        .1;
                    assert!(writer.finish_statement(owner, view).is_ok());
                    if aborted {
                        assert!(
                            writer
                                .abort_catalog_transaction(owner, tx, private, &mut flush)
                                .is_ok()
                        );
                    } else {
                        assert_eq!(
                            writer
                                .commit_catalog_transaction(owner, tx, private)
                                .ok()
                                .unwrap()
                                .q(),
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
fn corrupt_mode_or_mark_rejected_before_each_create_effect() {
    case("CCA07", || {
        for boundary in 0..3 {
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
                    let _ = reader_private;
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
                    let definition = table_spec("T");
                    if boundary == 0 {
                        let before = state.lock().unwrap().images();
                        tx.test_corrupt_catalog_completion(mark);
                        assert!(matches!(
                            writer.reserve_catalog_create(
                                owner,
                                &view,
                                undo,
                                &definition,
                                private,
                                &mut flush
                            ),
                            Err(CatalogCreateError::Fatal(_))
                        ));
                        assert_eq!(state.lock().unwrap().images(), before);
                    } else {
                        let reserved = writer
                            .reserve_catalog_create(
                                owner,
                                &view,
                                undo,
                                &definition,
                                private,
                                &mut flush,
                            )
                            .ok()
                            .unwrap();
                        let before = state.lock().unwrap().images();
                        tx.test_corrupt_catalog_completion(mark);
                        let error = if boundary == 1 {
                            reserved.initialize().err()
                        } else {
                            reserved.cancel().err()
                        };
                        assert!(matches!(error, Some(RetentionReservationError::Fatal(_))));
                        assert_eq!(state.lock().unwrap().images(), before);
                    }
                    assert!(fence.is_fenced());
                    assert!(!hook::observe(core).read_gate_closed);
                });
            }
        }
    });
}

#[test]
fn actual_read_operation_drains_before_create_admission() {
    case("CCA08", || {
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
            let definition = table_spec("T");
            let mut earlier = core.try_owner(OwnerRole::Foreground).unwrap();
            let operation = earlier.try_read_operation(core).unwrap();
            let before = state.lock().unwrap().images();
            let (entered, waiting) = sync_channel(1);
            let (proceed, release) = sync_channel(1);
            hook::install(core, Point::ReadGateWaiting, entered, release);
            let private = std::thread::scope(|scope| {
                let reader_ref = &reader;
                let controller = scope.spawn(move || {
                    waiting
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(hook::observe(core).active_read_operations, 1);
                    assert_eq!(reader_ref.catalog_test_source_used(), 0);
                    assert_eq!(state.lock().unwrap().images(), before);
                    operation.finish_checked().unwrap();
                    proceed.send(Resume::Continue).unwrap();
                });
                let reserved = writer
                    .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                    .ok()
                    .unwrap();
                controller.join().unwrap();
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert!(hook::observe(core).read_gate_closed);
                let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                let error = late.try_read_operation(core).err().unwrap();
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(
                    error.cause,
                    crate::transaction::concurrency::Cause::InstallationPending
                );
                reserved.cancel().ok().unwrap().into_private()
            });
            assert!(!hook::observe(core).read_gate_closed);
            earlier
                .try_read_operation(core)
                .unwrap()
                .finish_checked()
                .unwrap();
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(
                writer
                    .abort_catalog_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn planner_before_preserves_read_dependency_and_private() {
    case("CCA09", || {
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
            let invalid = table_spec("");
            let definition = table_spec("T");
            let address = private.as_ptr();
            let before = state.lock().unwrap().images();
            let private = match writer
                .reserve_catalog_create(owner, &view, undo, &invalid, private, &mut flush)
            {
                Err(CatalogCreateError::Before { error, private }) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(matches!(
                        error.cause,
                        crate::storage::disk::RetentionCause::Catalog(_)
                    ));
                    private
                }
                _ => panic!("invalid schema planner Before required"),
            };
            assert_eq!(private.as_ptr(), address);
            assert_eq!(view.test_catalog_reads(), 1);
            assert_eq!(view.revision(), 0);
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert!(!hook::observe(core).read_gate_closed);
            let private = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap()
                .cancel()
                .ok()
                .unwrap()
                .into_private();
            assert_eq!((view.revision(), view.test_catalog_reads()), (0, 3));
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(
                writer
                    .abort_catalog_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert!(!fence.is_fenced());
        });
    });
}
