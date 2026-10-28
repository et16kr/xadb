//! Continuous original flush ownership and rejected forged lease identity.
use super::*;
use crate::storage::dirty_frame::init_test_support as init;
use crate::storage::disk::{RetentionCause, RetentionPhase};

#[test]
fn before_keeps_original_lease_for_successful_cancel() {
    case("MCL01", || {
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
            let actor = live
                .catalog_begin(owner, status, private)
                .unwrap()
                .actor
                .unwrap();
            let definition = table_spec("T");
            let mut reserved = live
                .reserve_catalog_create(owner, actor, undo, &definition, private)
                .unwrap();
            oracle.check(reserved.catalog_test_bound(), state, "R02");
            let identity = reserved.catalog_test_lease_identity();
            assert_ne!(identity.0, 0);
            assert_ne!(identity.1, 0);
            let wal = reserved.catalog_test_wal_state();
            let images = state.lock().unwrap().images();
            let claim = init::pair_close_state(reserved.catalog_test_pool());
            // Fault-only refusal: no competing maintenance operation can obtain
            // this mutable owner while the reservation is alive.
            reserved.catalog_test_lease_fault(0);
            let mut reserved = match reserved.cancel() {
                Err(RetentionReservationError::Before { error, reservation }) => {
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::Busy));
                    reservation
                }
                _ => panic!("original reservation must be returned"),
            };
            assert_eq!(reserved.catalog_test_lease_identity(), identity);
            assert_eq!(reserved.catalog_test_wal_state(), wal);
            assert_eq!(init::pair_close_state(reserved.catalog_test_pool()), claim);
            assert_eq!(state.lock().unwrap().images(), images);
            assert!(!fence.is_fenced());
            reserved.catalog_test_lease_fault(1);
            assert!(reserved.cancel().is_ok());
            oracle.check(live, state, "K03");
            owner.check_idle().unwrap();
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn lease_generation_pair_and_origin_corruption_fence_before_effect() {
    case("MCL02", || {
        for fault in [2, 3, 4] {
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
                let actor = live
                    .catalog_begin(owner, status, private)
                    .unwrap()
                    .actor
                    .unwrap();
                let definition = table_spec("T");
                let mut reserved = live
                    .reserve_catalog_create(owner, actor, undo, &definition, private)
                    .unwrap();
                oracle.check(reserved.catalog_test_bound(), state, "R02");
                let images = state.lock().unwrap().images();
                let wal = reserved.catalog_test_wal_state();
                let claim = init::pair_close_state(reserved.catalog_test_pool());
                reserved.catalog_test_lease_fault(fault);
                let error = if fault == 3 {
                    match reserved.initialize() {
                        Err(RetentionReservationError::Fatal(e)) => e,
                        _ => panic!("fatal expected"),
                    }
                } else {
                    match reserved.cancel() {
                        Err(RetentionReservationError::Fatal(e)) => e,
                        _ => panic!("fatal expected"),
                    }
                };
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(matches!(error.cause, RetentionCause::BindingMismatch));
                assert_eq!(live.catalog_test_wal_state(), wal);
                assert_eq!(state.lock().unwrap().images(), images);
                assert_eq!(init::pair_close_state(live.catalog_test_pool()), claim);
                assert!(fence.is_fenced());
                assert!(owner.check_idle().is_err());
            });
        }
    });
}

#[test]
fn original_flush_origin_and_ready_residual_are_checked() {
    case("MCL03", || {
        for fault in [0, 1] {
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
                let actor = live
                    .catalog_begin(owner, status, private)
                    .unwrap()
                    .actor
                    .unwrap();
                oracle.check(live, state, "R01");
                if fault == 0 {
                    let root = live.target(Plan4Page::Root).unwrap();
                    let root_index = resident_index(live, 1).unwrap();
                    {
                        let (_, reader, mut flush) = live.split();
                        let mut pinned = core.try_owner(OwnerRole::Foreground).unwrap();
                        let request = reader
                            .catalog_test_pool()
                            .target_request(root_index)
                            .unwrap();
                        assert_eq!(request.identity, root.identity());
                        let read = reader
                            .catalog_test_pool()
                            .reserve_target_set(&mut pinned, &[request])
                            .unwrap()
                            .try_read()
                            .unwrap();
                        assert!(matches!(
                            flush.try_flush(flusher, &root).unwrap(),
                            RetentionFlushStep::RetiredPendingFinalize
                        ));
                        read.release_checked().unwrap();
                        pinned.check_idle().unwrap();
                    }
                }
                live.catalog_test_create_flush_fault(fault);
                let images = state.lock().unwrap().images();
                let wal = live.catalog_test_wal_state();
                let claim = init::pair_close_state(live.catalog_test_pool());
                let definition = table_spec("T");
                let error = live
                    .reserve_catalog_create(owner, actor, undo, &definition, private)
                    .err()
                    .expect("invalid original flush must reject");
                assert_eq!(error.effect, Effect::CallerMustResolve);
                if fault == 0 {
                    assert!(matches!(error.cause, RetentionCause::Invariant));
                } else {
                    assert!(matches!(error.cause, RetentionCause::BindingMismatch));
                }
                assert_eq!(live.catalog_test_wal_state(), wal);
                assert_eq!(state.lock().unwrap().images(), images);
                assert_eq!(init::pair_close_state(live.catalog_test_pool()), claim);
                assert!(fence.is_fenced());
                assert!(owner.check_idle().is_err());
            });
        }
    });
}
