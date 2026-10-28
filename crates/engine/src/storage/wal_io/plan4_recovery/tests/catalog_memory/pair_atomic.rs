//! Original CREATE paths through atomic pool close/return. The bound is still
//! exclusive; these cuts do not certify shared admission or reader competition.
use super::*;
use crate::storage::dirty_frame::{init_test_support as init, test_support as frames};
use crate::storage::disk::{RetentionCause, RetentionPhase};
use std::sync::mpsc::sync_channel;

#[test]
fn rejected_reserve_returns_original_unstaged_pair() {
    case("MPA01", || {
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
            let begin = live.catalog_begin(owner, status, private).unwrap();
            let actor = begin.actor.unwrap();
            oracle.check(live, state, "R01");
            let before_files = state.lock().unwrap().images();
            let before_bank = live.recovery_test_bank().to_vec();
            let before_jobs = live.catalog_test_jobs().to_vec();
            let before_wal = live.catalog_test_wal_state();
            let before_epochs = live.catalog_test_epochs();
            let competitor = live.catalog_test_stage_competitor();
            let bytes = oracle.frame("R02");
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            use crate::transaction::concurrency::catalog_test_support as hook;
            hook::install(
                core,
                hook::Point::ExistingStageCallStarted,
                entered,
                release,
            );
            state.lock().unwrap().events.clear();
            let definition = table_spec("T");
            let error = std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived.recv_timeout(WAIT).unwrap();
                    competitor(&bytes);
                    resume.send(hook::Resume::Continue).unwrap();
                });
                let error = live
                    .reserve_catalog_create(owner, actor, undo, &definition, private)
                    .err()
                    .expect("competing physical stage rejects Reserve");
                controller.join().unwrap();
                error
            });
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Stage, Effect::Unchanged)
            );
            assert!(matches!(error.cause, RetentionCause::Wal(error)
                if matches!(error.cause, crate::storage::wal_io::Cause::Busy)));
            assert_eq!(
                init::pair_close_state(live.catalog_test_pool()),
                (None, None, false, 0)
            );
            assert_eq!(live.recovery_test_bank(), before_bank);
            assert_eq!(live.catalog_test_jobs().as_slice(), before_jobs);
            let (prefix, first_write) = live.catalog_test_wal_state();
            assert_eq!(first_write, before_wal.1);
            assert_eq!(prefix.complete_lsn, before_wal.0.complete_lsn + 1);
            assert_eq!(
                prefix.complete_offset,
                before_wal.0.complete_offset + oracle.frame("R02").len() as u64
            );
            assert_eq!(
                (prefix.synced_lsn, prefix.synced_offset),
                (before_wal.0.synced_lsn, before_wal.0.synced_offset)
            );
            assert_eq!(live.catalog_test_epochs(), before_epochs);
            assert_eq!(live.catalog_test_q(), 0);
            assert_eq!(state.lock().unwrap().mutations(), 0);
            assert_eq!(state.lock().unwrap().images(), before_files);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            // The synthetic competing stage is retained; this is not a healthy
            // retry or second-writer support fixture.
            fence.fence();
            assert!(fence.is_fenced());
        });
    });
}

#[test]
fn final_pair_retirement_unwind_preserves_claim_under_fence() {
    case("MPA02", || {
        for point in [
            init::InitPoint::PairFinishChecked,
            init::InitPoint::PairCancelChecked,
        ] {
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
                let begin = live.catalog_begin(owner, status, private).unwrap();
                oracle.check(live, state, "R01");
                let definition = table_spec("T");
                let reserved = live
                    .reserve_catalog_create(owner, begin.actor.unwrap(), undo, &definition, private)
                    .unwrap();
                let before =
                    init::pair_close_state(reserved.catalog_test_bound().catalog_test_pool());
                assert!(before.0.is_some() && before.1.is_some() && before.2 && before.3 == 0);
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                init::install_init_pause(
                    reserved.catalog_test_bound().catalog_test_pool(),
                    point,
                    entered,
                    release,
                );
                std::thread::scope(|scope| {
                    let controller = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        resume.send(frames::Resume::Panic).unwrap();
                    });
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            if point == init::InitPoint::PairFinishChecked {
                                let _ = reserved.initialize();
                            } else {
                                let _ = reserved.cancel();
                            }
                        }))
                        .is_err()
                    );
                    controller.join().unwrap();
                });
                assert!(fence.is_fenced());
                assert_eq!(init::pair_close_state(live.catalog_test_pool()), before);
                assert_eq!(live.catalog_test_q(), 0);
                assert_eq!(live.catalog_test_wal_state().0.complete_lsn, 8);
                let observed = crate::transaction::concurrency::catalog_test_support::observe(core);
                assert_eq!((observed.active_apply, observed.active_content), (0, 0));
            });
        }
    });
}
