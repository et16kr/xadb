//! Actual CREATE errors across the temporary original-resource view.
use super::*;
use crate::storage::dirty_frame::init_test_support as init;
use crate::storage::disk::{RetentionCause, RetentionPhase};
use crate::transaction::concurrency::catalog_test_support as hook;
use std::sync::mpsc::sync_channel;

#[test]
fn create_health_pending_precedes_busy_flush() {
    case("MCP01", || {
        for pending in [false, true] {
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
                if pending {
                    live.catalog_test_create_pending_fault();
                }
                let before = state.lock().unwrap().images();
                let bank = live.recovery_test_bank().to_vec();
                let jobs = live.catalog_test_jobs().to_vec();
                let wal = live.catalog_test_wal_state();
                let definition = table_spec("T");
                let error = live
                    .reserve_catalog_create(owner, actor, undo, &definition, private)
                    .err()
                    .expect("preparation must reject");
                assert_eq!(error.phase, RetentionPhase::Prepare);
                if pending {
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    assert!(matches!(error.cause, RetentionCause::Invariant));
                    assert!(fence.is_fenced());
                } else {
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(matches!(error.cause, RetentionCause::Busy));
                    assert!(!fence.is_fenced());
                }
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(live.recovery_test_bank(), bank);
                assert_eq!(live.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(live.catalog_test_wal_state(), wal);
                assert_eq!(
                    init::pair_close_state(live.catalog_test_pool()),
                    (None, None, false, 0)
                );
                if !pending {
                    let (_, _, mut flush) = live.split();
                    assert!(matches!(
                        flush.continue_flush(flusher).unwrap(),
                        RetentionFlushStep::Complete { .. }
                    ));
                    flusher.check_idle().unwrap();
                    owner.check_idle().unwrap();
                } else {
                    assert_eq!(
                        owner.check_idle().unwrap_err().cause,
                        crate::transaction::concurrency::Cause::Fenced
                    );
                }
            });
        }
    });
}

#[test]
fn cancel_before_returns_original_active_reservation() {
    case("MCP02", || {
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
            let competitor = live.catalog_test_stage_competitor();
            let definition = table_spec("T");
            let reserved = live
                .reserve_catalog_create(owner, actor, undo, &definition, private)
                .unwrap();
            oracle.check(reserved.catalog_test_bound(), state, "R02");
            let before = state.lock().unwrap().images();
            let bank = reserved.catalog_test_bound().recovery_test_bank().to_vec();
            let jobs = reserved.catalog_test_bound().catalog_test_jobs().to_vec();
            let epochs = reserved.catalog_test_bound().catalog_test_epochs();
            let wal = reserved.catalog_test_bound().catalog_test_wal_state();
            let q = reserved.catalog_test_bound().catalog_test_q();
            let claim = init::pair_close_state(reserved.catalog_test_bound().catalog_test_pool());
            let bytes = oracle.frame("K03");
            let staged_bytes = bytes.len() as u64;
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(
                core,
                hook::Point::ExistingStageCallStarted,
                entered,
                release,
            );
            let reserved = std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived.recv_timeout(WAIT).unwrap();
                    competitor(&bytes);
                    resume.send(hook::Resume::Continue).unwrap();
                });
                let returned = match reserved.cancel() {
                    Err(RetentionReservationError::Before { error, reservation }) => {
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Stage, Effect::Unchanged)
                        );
                        assert!(matches!(error.cause, RetentionCause::Wal(error)
                            if matches!(error.cause, crate::storage::wal_io::Cause::Busy)));
                        reservation
                    }
                    _ => panic!("Cancel must return its original reservation"),
                };
                controller.join().unwrap();
                returned
            });
            assert!(!fence.is_fenced());
            assert_eq!(reserved.catalog_test_bound().recovery_test_bank(), bank);
            assert_eq!(
                reserved.catalog_test_bound().catalog_test_jobs().as_slice(),
                jobs
            );
            assert_eq!(reserved.catalog_test_bound().catalog_test_epochs(), epochs);
            assert_eq!(reserved.catalog_test_bound().catalog_test_q(), q);
            let (prefix, first_write) = reserved.catalog_test_bound().catalog_test_wal_state();
            assert_eq!(first_write, wal.1);
            assert_eq!(prefix.complete_lsn, wal.0.complete_lsn + 1);
            assert_eq!(prefix.complete_offset, wal.0.complete_offset + staged_bytes);
            assert_eq!(
                (prefix.synced_lsn, prefix.synced_offset),
                (wal.0.synced_lsn, wal.0.synced_offset)
            );
            assert_eq!(
                init::pair_close_state(reserved.catalog_test_bound().catalog_test_pool()),
                claim
            );
            assert_eq!(state.lock().unwrap().images(), before);
            // A competing staged frame is not a healthy retry. Dropping the
            // still-active token must retain its claim and fence the session.
            drop(reserved);
            assert!(fence.is_fenced());
            assert_eq!(init::pair_close_state(live.catalog_test_pool()), claim);
            assert_eq!(
                owner.check_idle().unwrap_err().cause,
                crate::transaction::concurrency::Cause::Fenced
            );
        });
    });
}
