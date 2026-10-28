//! Original Drain covers the read entry before source reservation, including
//! reservation failure and panic. Hooks observe; they create no authority.
use super::*;
use crate::{
    storage::dirty_frame::init_test_support as raw,
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;

fn drain_before_source(fenced: bool, resident: bool) {
    case(
        match (resident, fenced) {
            (true, false) => "C2_RESIDENT_ENTRY01",
            (true, true) => "C2_RESIDENT_ENTRY02",
            (false, true) => "C2_ENTRY02",
            (false, false) => "C2_ENTRY01",
        },
        || {
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
                let actor = create_forward(
                    live, owner, flusher, status, undo, private, &oracle, state, 1,
                );
                let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
                let (pending_entered, pending_arrived) = sync_channel(1);
                let (pending_resume, pending_release) = sync_channel(1);
                // The read is admitted during Pending. A read admitted before
                // Commit would instead be waited for by the terminal read gate.
                hook::install(core, Point::Pending, pending_entered, pending_release);
                {
                    let (mut writer, reader, maintenance) = live.split();
                    let pending = writer.catalog_test_commit_charges()[2] as u64;
                    let pool = maintenance.catalog_test_pool();
                    let reader_ref = &reader;
                    let (registered, ready) = sync_channel(1);
                    let (start, started) = sync_channel(1);
                    std::thread::scope(|scope| {
                        let reading = scope.spawn(move || {
                            let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                            // Registration needs apply admission, closed in Pending.
                            let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                            registered.send(()).unwrap();
                            started.recv_timeout(WAIT).unwrap();
                            let result = {
                                if resident {
                                    reader_ref.lookup_catalog_resident(
                                        &mut separate,
                                        &snapshot,
                                        ObjectId::new(6).unwrap(),
                                        &mut Plan4ResidencyCursor::new(),
                                        reader_private,
                                    )
                                } else {
                                    reader_ref.lookup_catalog(
                                        &mut separate,
                                        &snapshot,
                                        ObjectId::new(6).unwrap(),
                                        &mut Plan4ResidencyCursor::new(),
                                        reader_private,
                                    )
                                }
                            };
                            let error = result.err().expect("entry resumed inside Drain");
                            assert_eq!(error.effect, if fenced { Effect::CallerMustResolve } else { Effect::Unchanged });
                            if !fenced {
                                assert!(matches!(&error.cause, crate::storage::disk::RetentionCause::Frame(error)
                                    if error.cause == crate::storage::dirty_frame::Cause::Concurrency(
                                        crate::transaction::concurrency::Cause::InstallationPending)), "{error:?}");
                                separate.check_idle().unwrap();
                            }
                            assert_eq!(snapshot.release_checked().is_err(), fenced);
                        });
                        ready.recv_timeout(WAIT).unwrap();
                        let controlling = scope.spawn(move || {
                            pending_arrived.recv_timeout(WAIT).unwrap();
                            assert_eq!(reader_ref.catalog_test_source_used(), pending);
                            let (entered, arrived) = sync_channel(1);
                            let (resume, release) = sync_channel(1);
                            hook::install(core, Point::LookupBeforeSourceReserve, entered, release);
                            start.send(()).unwrap();
                            arrived.recv_timeout(WAIT).unwrap();
                            // Only the Pending writer is charged before this read reserves.
                            assert_eq!(reader_ref.catalog_test_source_used(), pending);
                            assert_eq!(reader_ref.catalog_test_references(), ((0, 0, 0), false));
                            assert_eq!(hook::observe(core).active_read_operations, 1);
                            // Only the Pending writer's four pins; the paused read holds none.
                            assert_eq!(raw::claim_state(pool).1, 4);
                            let (waiting, wait_arrived) = sync_channel(1);
                            let (drain_resume, drain_release) = sync_channel(1);
                            hook::install(core, Point::DrainWaiting, waiting, drain_release);
                            pending_resume.send(Resume::Continue).unwrap();
                            wait_arrived.recv_timeout(WAIT).unwrap();
                            let observed = hook::observe(core);
                            assert_eq!(
                                (
                                    observed.installation,
                                    observed.active_content,
                                    observed.active_read_operations
                                ),
                                ("Draining", 0, 1)
                            );
                            assert_eq!(reader_ref.catalog_test_source_used(), pending);
                            if fenced {
                                fence.fence();
                            }
                            resume.send(Resume::Continue).unwrap();
                            reading.join().unwrap();
                            assert_eq!(hook::observe(core).active_read_operations, 0);
                            assert_eq!(reader_ref.catalog_test_source_used(), pending);
                            assert_eq!(reader_ref.catalog_test_references(), ((0, 0, 0), false));
                            drain_resume.send(Resume::Continue).unwrap();
                        });
                        let result = writer.commit_catalog(owner, prepared);
                        if fenced {
                            assert_eq!(result.err().unwrap().effect, Effect::CallerMustResolve);
                        } else {
                            result.unwrap();
                            assert_eq!(writer.catalog_test_source_used(), 0);
                        }
                        controlling.join().unwrap();
                    });
                }
                assert_eq!(live.catalog_test_q(), u64::from(!fenced));
                assert_eq!(fence.is_fenced(), fenced);
                if !fenced {
                    oracle.check(live, state, "C04");
                }
            });
        },
    );
}

#[test]
fn drain_waits_for_entry_before_source_charge() {
    drain_before_source(false, false);
}
#[test]
fn failed_source_reservation_returns_progress_under_fence() {
    drain_before_source(true, false);
}

#[test]
fn entry_unwind_returns_progress_without_source_charge() {
    case("C2_ENTRY03", || {
        for resident in [false, true] {
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
                let _ = (status, undo, private);
                prepare_catalog(live, owner, flusher);
                let before = state.lock().unwrap().events.len();
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, Point::LookupBeforeSourceReserve, entered, release);
                let (writer, reader, _) = live.split();
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                            let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                            if resident {
                                reader_ref
                                    .lookup_catalog_resident(
                                        &mut separate,
                                        &snapshot,
                                        ObjectId::new(6).unwrap(),
                                        &mut Plan4ResidencyCursor::new(),
                                        reader_private,
                                    )
                                    .unwrap();
                            } else {
                                reader_ref
                                    .lookup_catalog(
                                        &mut separate,
                                        &snapshot,
                                        ObjectId::new(6).unwrap(),
                                        &mut Plan4ResidencyCursor::new(),
                                        reader_private,
                                    )
                                    .unwrap();
                            }
                        }))
                    });
                    arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert_eq!(hook::observe(core).active_read_operations, 1);
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    resume.send(Resume::Panic).unwrap();
                    assert!(reading.join().unwrap().is_err());
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(hook::observe(core).active_read_operations, 0);
                });
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().events.len(), before);
            });
        }
    });
}

#[test]
fn resident_drain_waits_before_source_charge() {
    drain_before_source(false, true);
}
#[test]
fn resident_failed_source_returns_progress() {
    drain_before_source(true, true);
}
