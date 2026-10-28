//! Actual Commit fault consumers. Expected bytes remain frozen independent
//! C04/N03 inputs; hooks only pause or unwind the original producer.
use super::*;
use crate::{
    storage::dirty_frame::init_test_support as raw,
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::{sync::mpsc::sync_channel, time::Duration};

fn io_failure(id: &str, action: port::Action, sync: bool) {
    case(id, || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let expected = oracle.frame("C04");
            let prior = state.lock().unwrap().files["d3-wal-00000001"].bytes.clone();
            let start = prior.len();
            {
                let mut adapter = state.lock().unwrap();
                adapter.events.clear();
                adapter.fault_hits = 0;
                adapter.fault = Some(port::Fault {
                    key: port::Key::new(
                        if sync { "sync" } else { "write" },
                        "d3-wal-00000001",
                        if sync { 0 } else { start as u64 },
                        1,
                    ),
                    action,
                });
            }
            let result = {
                let (mut writer, _, _) = live.split();
                writer.catalog_commit(owner, actor, private)
            };
            let interrupted = action == port::Action::Interrupted;
            if interrupted {
                assert!(result.is_ok(), "Interrupted must finish the actual Commit");
                oracle.check(live, state, "C04");
                assert_eq!(live.catalog_test_q(), 1);
                assert!(!fence.is_fenced());
            } else {
                let error = match result {
                    Err(error) => error,
                    Ok(_) => panic!("injected I/O failure must reject Commit"),
                };
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(fence.is_fenced());
                assert_eq!(live.catalog_test_q(), 0);
                let (prefix, first_write) = live.catalog_test_wal_state();
                assert_eq!(
                    (prefix.complete_lsn, prefix.synced_lsn, first_write),
                    (9, 8, 9)
                );
            }
            let adapter = state.lock().unwrap();
            assert_eq!(adapter.fault_hits, 1);
            assert!(adapter.fault.is_none());
            let calls: Vec<_> = adapter
                .events
                .iter()
                .filter(|e| e.op == "write" && e.name == "d3-wal-00000001")
                .collect();
            assert_eq!(calls.len(), if interrupted { 2 } else { 1 });
            assert_eq!(
                (calls[0].offset, calls[0].requested),
                (start as u64, expected.len())
            );
            if interrupted {
                assert_eq!(calls[0].returned, None);
                assert_eq!(
                    (calls[1].offset, calls[1].requested, calls[1].returned),
                    (start as u64, expected.len(), Some(expected.len()))
                );
            }
            let kept = if sync || interrupted {
                expected.len()
            } else if action == port::Action::PrefixErrorBytes(96) {
                96
            } else {
                0
            };
            let actual = &adapter.files["d3-wal-00000001"].bytes;
            assert_eq!(actual.len(), start + kept);
            assert_eq!(&actual[..start], prior);
            assert_eq!(&actual[start..], &expected[..kept]);
            if action == port::Action::PrefixErrorBytes(96) {
                assert_eq!((calls[0].returned, calls[0].error_prefix), (None, 96));
            }
            if action == port::Action::Zero {
                assert_eq!(calls[0].returned, Some(0));
            }
            let syncs: Vec<_> = adapter
                .events
                .iter()
                .filter(|e| e.op == "sync" && e.name == "d3-wal-00000001")
                .collect();
            assert_eq!(syncs.len(), usize::from(sync || interrupted));
            if sync {
                assert_eq!(syncs[0].completed, action == port::Action::AfterError);
            }
        });
    });
}

#[test]
fn catalog_mf03_commit_write_before_error() {
    io_failure("MF03", port::Action::BeforeError, false);
}
#[test]
fn catalog_mf04_commit_write_zero() {
    io_failure("MF04", port::Action::Zero, false);
}
#[test]
fn catalog_mf05_commit_write_prefix96_error() {
    io_failure("MF05", port::Action::PrefixErrorBytes(96), false);
}
#[test]
fn catalog_mf06_commit_interrupted_retry() {
    io_failure("MF06", port::Action::Interrupted, false);
}
#[test]
fn catalog_mf07_commit_sync_before_error() {
    io_failure("MF07", port::Action::BeforeError, true);
}
#[test]
fn catalog_mf08_commit_sync_after_error() {
    io_failure("MF08", port::Action::AfterError, true);
}

fn unwind_boundary(id: &str, point: Point) {
    case(id, || {
        let oracle = Oracle::new();
        let images = with_fresh!(oracle, |live,
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let mut originals = Vec::new();
            for (number, role) in [(1, 1), (2, 2), (9, 4)] {
                let index = resident_index(live, number).unwrap();
                let mut before = boxed::<PAGE_BYTES>();
                let observed = raw::snapshot(live.catalog_test_pool(), index, &mut before);
                let golden_before = oracle.transition.raw(&oracle.row("C04")[23 + role]);
                let golden_after = oracle.transition.raw(&oracle.row("C04")[29 + role]);
                assert_eq!(before.as_slice(), golden_before);
                originals.push((index, observed, before, golden_after));
            }
            let before_bank = live.recovery_test_bank().to_vec();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, point, entered, release);
            std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived.recv_timeout(Duration::from_secs(5)).unwrap();
                    resume.send(Resume::Panic).unwrap();
                });
                let (mut writer, _, _) = live.split();
                let charges = writer.catalog_test_commit_charges();
                assert!(
                    catch_unwind(AssertUnwindSafe(
                        || writer.catalog_commit(owner, actor, private)
                    ))
                    .is_err()
                );
                controller.join().unwrap();
                // Unwind fences: the current window charge stays reserved and
                // the read gate is cleared by its Drop.
                let window = match point {
                    Point::StageCallStarted => 1,
                    Point::Durable => 2,
                    Point::BeforePublish => 4,
                    _ => 3,
                };
                assert_eq!(writer.catalog_test_source_used(), charges[window] as u64);
                assert!(!hook::observe(core).read_gate_closed);
            });
            assert!(fence.is_fenced());
            assert_eq!(live.catalog_test_q(), 0);
            let (prefix, first_write) = live.catalog_test_wal_state();
            let before_stage = point == Point::StageCallStarted;
            assert_eq!(
                (prefix.complete_lsn, prefix.synced_lsn, first_write),
                if before_stage { (8, 8, 8) } else { (9, 9, 9) }
            );
            let installed_all =
                matches!(point, Point::InstalledBeforeFinish | Point::BeforePublish);
            let second_page = point == Point::InstalledSecondPage;
            for (ordinal, (index, original, before, after)) in originals.iter().enumerate() {
                let mut actual = boxed::<PAGE_BYTES>();
                let observed = raw::snapshot(live.catalog_test_pool(), *index, &mut actual);
                let bytes_installed = installed_all || second_page && ordinal <= 1;
                let metadata_installed = installed_all || second_page && ordinal == 0;
                assert_eq!(
                    actual.as_slice(),
                    if bytes_installed {
                        after.as_slice()
                    } else {
                        before.as_slice()
                    },
                    "separate physical bytes at role {ordinal}"
                );
                if metadata_installed {
                    assert_eq!(observed.page_lsn.0, 9);
                    assert_eq!(observed.counter, original.counter + 1);
                    assert_eq!(
                        observed.rec_lsn,
                        original.rec_lsn.or(Some(crate::storage::page::PageLsn(9)))
                    );
                    assert_eq!(observed.requirement.unwrap().lsn, 9);
                    assert!(observed.dirty);
                } else {
                    assert_eq!(
                        (
                            observed.page_lsn,
                            observed.counter,
                            observed.rec_lsn,
                            observed.dirty,
                            observed.entry,
                            observed.requirement
                        ),
                        (
                            // install_page publishes bytes and pageLSN together;
                            // the hook precedes counter/DPT/requirement adoption.
                            if bytes_installed {
                                crate::storage::page::PageLsn(9)
                            } else {
                                original.page_lsn
                            },
                            original.counter,
                            original.rec_lsn,
                            original.dirty,
                            original.entry,
                            original.requirement
                        )
                    );
                }
                assert_eq!(
                    observed.total, 0,
                    "unwind must return original physical pins"
                );
            }
            if installed_all {
                let frame = oracle.frame("C04");
                let bank = live.recovery_test_bank();
                assert!(bank[..1664].iter().all(|b| *b == 0));
                assert_eq!(&bank[32 * 1664..32 * 1664 + 1536], &frame[1952..3488]);
            } else {
                // STAGED/pending metadata may change; authoritative descriptor
                // prefixes remain the accepted old cut until whole install.
                let bank = live.recovery_test_bank();
                for slot in [0, 32, 33] {
                    assert_eq!(
                        &bank[slot * 1664..slot * 1664 + 1536],
                        &before_bank[slot * 1664..slot * 1664 + 1536]
                    );
                }
            }
            let observation = hook::observe(core);
            assert_eq!(
                (observation.active_apply, observation.active_content),
                (0, 0)
            );
            assert_eq!(
                observation.installation,
                if point == Point::BeforePublish {
                    "Open"
                } else {
                    "Abandoned"
                }
            );
            state.lock().unwrap().images()
        });
        if point != Point::StageCallStarted {
            recover_check(&images, &oracle, "N03");
        }
    });
}

#[test]
fn catalog_mf02_commit_stage_call_unwind() {
    unwind_boundary("MF02", Point::StageCallStarted);
}
#[test]
fn catalog_mf09_commit_durable_unwind() {
    unwind_boundary("MF09", Point::Durable);
}
#[test]
fn catalog_mf10_commit_second_page_unwind() {
    unwind_boundary("MF10", Point::InstalledSecondPage);
}
#[test]
fn catalog_mf11_commit_installed_before_finish_unwind() {
    unwind_boundary("MF11", Point::InstalledBeforeFinish);
}
#[test]
fn catalog_mf12_commit_before_publish_unwind() {
    unwind_boundary("MF12", Point::BeforePublish);
}

#[test]
fn catalog_drained_before_resume_unwind() {
    unwind_boundary("C2_DRAIN04", Point::DrainedBeforeResume);
}
