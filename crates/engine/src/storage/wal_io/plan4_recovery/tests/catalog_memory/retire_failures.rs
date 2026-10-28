//! Actual CLR retirement unwind after its complete stage. The staged frame is
//! still only in the original WAL slot; a fresh runtime completes rollback from
//! the durable prefix and expected bytes come from the frozen MF14 corpus.
use super::*;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn catalog_mf14_clr_first_page_unwind() {
    case("MF14", || {
        let oracle = Oracle::new().with_retire();
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
            live.catalog_rollback_start(owner, actor, private).unwrap();
            oracle.check(live, state, "R04");
            assert_eq!(live.catalog_test_references(), ((0, 0, 0), false));
            let prior = state.lock().unwrap().images();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::RetirementFirstPage, entered, release);
            std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived.recv_timeout(WAIT).unwrap();
                    resume.send(Resume::Panic).unwrap();
                });
                assert!(
                    catch_unwind(AssertUnwindSafe(|| live.catalog_retire(
                        owner,
                        1,
                        Operation::Clr,
                        private
                    )))
                    .is_err()
                );
                controller.join().unwrap();
            });
            assert!(fence.is_fenced());
            // The retiring marker and the transferred own rollback pin remain;
            // no normal cancel restores the pin or clears the marker.
            assert_eq!(live.catalog_test_references(), ((0, 0, 1), true));
            let (prefix, first_write) = live.catalog_test_wal_state();
            assert_eq!(
                (prefix.complete_lsn, prefix.synced_lsn, first_write),
                (10, 9, 9),
                "complete CLR10 stage stays in the original slot"
            );
            assert_eq!(
                state.lock().unwrap().images(),
                prior,
                "staged CLR wrote no file bytes"
            );
            // The physical pair reservation is not retired by the unwind.
            for number in [11, 12] {
                assert!(resident_index(live, number).is_some(), "pair page {number}");
            }
            assert_eq!(live.catalog_test_q(), 0);
            let (writer, _, _) = live.split();
            assert_eq!(
                writer.catalog_test_source_used(),
                writer.catalog_test_retirement_charge() as u64,
                "fatal retirement keeps its original charge"
            );
            let observation = hook::observe(core);
            assert!(
                !observation.read_gate_closed,
                "unwind reopens the original gate"
            );
            assert_eq!(
                (observation.active_apply, observation.active_content),
                (0, 0)
            );
            prior
        });
        recover_check(&images, &oracle, "MF14");
    });
}
