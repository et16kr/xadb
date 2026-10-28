//! Actual CLR progress waits for finite reads before its fixed source charge.
//! Hooks pause the real reader and retirement, never manufacture authority.
use super::*;
use crate::storage::disk::RetentionCause;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn catalog_mt04_reader_returns_before_clr_progress() {
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
                let charge = writer.catalog_test_retirement_charge() as u64;
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let core_ref = core;
                    let reading = scope.spawn(move || {
                        let mut pinned = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut pinned).unwrap();
                        assert_eq!(snapshot.q(), 0);
                        // Pauses with both captured references alive.
                        assert!(!lookup_resident!(
                            reader_ref,
                            &mut pinned,
                            &snapshot,
                            6,
                            reader_private
                        ));
                        // Once this read returns, retirement may already own
                        // its rollback reference. Check global zero after both.
                        snapshot.release_checked().unwrap();
                        pinned.check_idle().unwrap();
                        reader_private
                    });
                    arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(writer.catalog_test_references(), ((2, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    let prepared = writer
                        .prepare_catalog_retirement(owner, 1, Operation::Clr, private)
                        .unwrap();
                    let before = state.lock().unwrap().images();
                    let read_charge = reader.catalog_test_source_used();
                    let (entered, waiting) = sync_channel(1);
                    let (advance, release) = sync_channel(1);
                    hook::install(core, Point::ReadGateWaiting, entered, release);
                    let controlling = scope.spawn(move || {
                        waiting.recv_timeout(WAIT).unwrap();
                        // Keep the !Send Owner on this thread. The writer is
                        // paused before Apply, so snapshot creation is legal;
                        // the later lookup is what tests the closed read gate.
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut late).unwrap();
                        let observed = hook::observe(core);
                        assert!(observed.read_gate_closed);
                        assert_eq!((observed.active_read_operations, observed.active_content,
                            observed.active_apply), (1, 0, 0));
                        assert_eq!(reader_ref.catalog_test_source_used(), read_charge);
                        assert_eq!(state.lock().unwrap().images(), before, "wait has WAL0");
                        let (entered, staging) = sync_channel(1);
                        let (continue_stage, release) = sync_channel(1);
                        hook::install(core, Point::RetirementIntent, entered, release);
                        advance.send(Resume::Continue).unwrap();
                        resume.send(Resume::Continue).unwrap();
                        let reader_private = reading.join().unwrap();
                        staging.recv_timeout(WAIT).unwrap();
                        assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        let error = reader_ref.lookup_catalog_resident(&mut late, &snapshot,
                            ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(),
                            reader_private).err().unwrap();
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                            if error.cause == crate::transaction::concurrency::Cause::InstallationPending));
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        snapshot.release_checked().unwrap();
                        continue_stage.send(Resume::Continue).unwrap();
                        reader_private
                    });
                    writer
                        .apply_catalog_retirement(owner, prepared, &mut flush)
                        .unwrap();
                    assert!(!fence.is_fenced());
                    assert_eq!(writer.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    owner.check_idle().unwrap();
                    controlling.join().unwrap()
                })
            };
            assert_eq!(live.catalog_test_references(), ((0, 0, 0), false));
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

#[test]
fn catalog_retirement_capacity_returns_gate_before_effects() {
    case("MCRP01", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            live.catalog_rollback_start(owner, actor, private).unwrap();
            let before = state.lock().unwrap().images();
            let prepared = live
                .prepare_catalog_retirement(owner, 1, Operation::Clr, private)
                .unwrap();
            {
                let (mut writer, reader, mut flush) = live.split();
                let [shared, ..] = writer.catalog_test_commit_charges();
                let held = 65536 - shared - writer.catalog_test_retirement_charge() + 1;
                let error = reader.catalog_test_with_held_source(held, || {
                    let error = writer
                        .apply_catalog_retirement(owner, prepared, &mut flush)
                        .err()
                        .unwrap();
                    assert_eq!(writer.catalog_test_source_used(), held as u64);
                    error
                });
                assert_eq!(
                    (error.phase, error.effect),
                    (
                        crate::storage::disk::RetentionPhase::Census,
                        Effect::Unchanged
                    )
                );
                assert!(matches!(error.cause, RetentionCause::Capacity));
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
            }
            assert_eq!(state.lock().unwrap().images(), before);
            owner.check_idle().unwrap();
            assert!(!fence.is_fenced());
            live.catalog_retire(owner, 1, Operation::Clr, private)
                .unwrap();
            oracle.check(live, state, "R05");
        });
    });
}
