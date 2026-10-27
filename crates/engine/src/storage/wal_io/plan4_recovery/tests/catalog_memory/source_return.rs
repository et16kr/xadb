//! Original Drain must include the admitted lookup's final source cleanup,
//! after all reference/physical guards have already ended.
use super::*;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

fn drain_finished(fenced: bool) {
    case(if fenced { "C2_RETURN02" } else { "C2_RETURN01" }, || {
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
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::LookupBeforeSourceReturn, entered, release);
            {
                let (mut writer, reader, _) = live.split();
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                        let result = {
                            reader_ref.lookup_catalog(
                                &mut separate,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                        };
                        if fenced {
                            assert_eq!(result.err().unwrap().effect, Effect::CallerMustResolve);
                            assert!(snapshot.release_checked().is_err());
                        } else {
                            assert!(matches!(result.unwrap(), CatalogLookupStep::Missing));
                            snapshot.release_checked().unwrap();
                            separate.check_idle().unwrap();
                        }
                    });
                    arrived.recv_timeout(WAIT).unwrap();
                    let charge = reader.catalog_test_source_used();
                    assert!(charge > 0);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(hook::observe(core).active_read_operations, 1);
                    let (waiting, wait_arrived) = sync_channel(1);
                    let (drain_resume, drain_release) = sync_channel(1);
                    hook::install(core, Point::DrainWaiting, waiting, drain_release);
                    let controlling = scope.spawn(move || {
                        wait_arrived.recv_timeout(WAIT).unwrap();
                        let observation = hook::observe(core);
                        assert_eq!(
                            (
                                observation.installation,
                                observation.active_content,
                                observation.active_read_operations
                            ),
                            ("Draining", 0, 1)
                        );
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        assert_eq!(reader_ref.catalog_test_references(), ((0, 0, 0), false));
                        if fenced {
                            fence.fence();
                        }
                        resume.send(Resume::Continue).unwrap();
                        reading.join().unwrap();
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        assert_eq!(
                            reader_ref.catalog_test_source_used(),
                            if fenced { charge } else { 0 }
                        );
                        drain_resume.send(Resume::Continue).unwrap();
                    });
                    let result = writer.commit_catalog(owner, prepared);
                    if fenced {
                        assert_eq!(result.err().unwrap().effect, Effect::CallerMustResolve);
                    } else {
                        result.unwrap();
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
    });
}
#[test]
fn drain_waits_after_refs_until_source_return() {
    drain_finished(false);
}
#[test]
fn fenced_source_return_still_cleans_progress() {
    drain_finished(true);
}

fn found_return(unwind: bool) {
    case(if unwind { "C2_RETURN04" } else { "C2_RETURN03" }, || {
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
            {
                let (mut writer, _, _) = live.split();
                writer.commit_catalog(owner, prepared).unwrap();
            }
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::LookupBeforeSourceReturn, entered, release);
            let before = state.lock().unwrap().images();
            let (writer, reader, _) = live.split();
            let reader_ref = &reader;
            std::thread::scope(|scope| {
                let reading = scope.spawn(move || {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                        assert_eq!(snapshot.q(), 1);
                        let record = match reader_ref
                            .lookup_catalog(
                                &mut separate,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                            .unwrap()
                        {
                            CatalogLookupStep::Found(record) => record,
                            _ => panic!("committed table T"),
                        };
                        assert_eq!(record.header().identity.object.get(), 6);
                        snapshot.release_checked().unwrap();
                        separate.check_idle().unwrap();
                    }))
                });
                arrived.recv_timeout(WAIT).unwrap();
                // Full lookup may replay/read WAL before final decode. From
                // this hook onward only checked no-I/O cleanup is allowed.
                let return_events = state.lock().unwrap().events.len();
                let charge = reader.catalog_test_source_used();
                assert!(charge > 0);
                assert_eq!(hook::observe(core).active_read_operations, 1);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                resume
                    .send(if unwind {
                        Resume::Panic
                    } else {
                        Resume::Continue
                    })
                    .unwrap();
                assert_eq!(reading.join().unwrap().is_err(), unwind);
                let after_events = state.lock().unwrap().events.len();
                assert_eq!(after_events, return_events);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(
                    reader.catalog_test_source_used(),
                    if unwind { charge } else { 0 }
                );
            });
            let after = state.lock().unwrap().images();
            assert_eq!(after, before);
            assert_eq!(fence.is_fenced(), unwind);
        });
    });
}
#[test]
fn found_decode_keeps_progress_until_source_return() {
    found_return(false);
}
#[test]
fn unwinding_final_decode_preserves_source_and_cleans_progress() {
    found_return(true);
}
