//! An actual reader's Catalog TxnPin+UndoPin against an actual CLR. The hook
//! only pauses the reader after its source latch is returned.
use super::*;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn catalog_mt04_reader_pins_block_clr() {
    case("MT04", || {
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
            live.catalog_rollback_start(owner, actor, private).unwrap();
            oracle.check(live, state, "R04");
            assert_eq!(live.catalog_test_references(), ((0, 0, 0), false));
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::LookupCaptured, entered, release);
            let reader_private = {
                let (mut writer, reader, mut flush) = live.split();
                std::thread::scope(|scope| {
                    let core_ref = core;
                    let reading = scope.spawn(move || {
                        let mut pinned = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader.catalog_snapshot(&mut pinned).unwrap();
                        assert_eq!(snapshot.q(), 0);
                        // Pauses with both captured references alive.
                        assert!(!lookup_resident!(
                            reader,
                            &mut pinned,
                            &snapshot,
                            6,
                            reader_private
                        ));
                        assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                        snapshot.release_checked().unwrap();
                        pinned.check_idle().unwrap();
                        reader_private
                    });
                    arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(writer.catalog_test_references(), ((2, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    let before = state.lock().unwrap().images();
                    let prepared = writer
                        .prepare_catalog_retirement(owner, 1, Operation::Clr, private)
                        .unwrap();
                    let error = match writer.apply_catalog_retirement(owner, prepared, &mut flush) {
                        Err(error) => error,
                        Ok(_) => panic!("another Undo pin must keep CLR Busy"),
                    };
                    assert!(
                        busy(error.effect, &error.cause),
                        "CLR with reader pins: {error:?}"
                    );
                    assert!(!fence.is_fenced());
                    assert_eq!(state.lock().unwrap().images(), before, "Busy CLR WAL0");
                    // The own rollback pin is returned; the reader's pair stays.
                    assert_eq!(writer.catalog_test_references(), ((2, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    owner.check_idle().unwrap();
                    resume.send(Resume::Continue).unwrap();
                    reading.join().unwrap()
                })
            };
            assert_eq!(live.catalog_test_references(), ((0, 0, 0), false));
            oracle.check(live, state, "R04");
            live.catalog_retire(owner, 1, Operation::Clr, private)
                .unwrap();
            oracle.check(live, state, "R05");
            live.catalog_abort_end(owner, actor, private).unwrap();
            oracle.check(live, state, "R06");
            for id in ["R07", "R08", "R09"] {
                live.catalog_release_role(owner, 1, private).unwrap();
                oracle.check(live, state, id);
            }
            live.catalog_retire(owner, 1, Operation::RootFree, private)
                .unwrap();
            oracle.check(live, state, "R10");
            live.catalog_retire(owner, 0, Operation::TerminalFinish, private)
                .unwrap();
            oracle.check(live, state, "R11");
            query(live, owner, flusher, reader_private, 0, 6, false);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}
