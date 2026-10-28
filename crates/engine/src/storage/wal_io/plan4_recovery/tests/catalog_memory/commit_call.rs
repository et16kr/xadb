//! Commit preparation and its consuming continuation share one source loan.
use super::*;
use crate::storage::disk::{RetentionCause, RetentionPhase};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::sync_channel,
};

#[test]
fn commit_call_capacity_roles_phases_and_prepared_flush() {
    case("MCC01", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let root = live.target(Plan4Page::Root).unwrap();
            let before = state.lock().unwrap().images();
            let private_before = hex(private);
            let missing = TxnRef::new(
                actor.id(),
                Generation::new(actor.generation().get() + 1).unwrap(),
            )
            .unwrap();
            {
                let (mut writer, reader, mut flush) = live.split();
                let [shared, stage, pending, _, publish] = writer.catalog_test_commit_charges();
                let held = 65536 - shared - stage + 1;
                reader.catalog_test_with_held_source(held, || {
                    let error = writer.catalog_commit(owner, actor, private).err().unwrap();
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Census, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::Capacity));
                    assert_eq!(reader.catalog_test_source_used(), held as u64);
                    assert!(!hook::observe(core).read_gate_closed);
                });
                assert_eq!(hex(private), private_before);
                let error = writer
                    .catalog_commit(owner, missing, private)
                    .err()
                    .unwrap();
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::MissingTransaction));
                for foreign_role in [false, true] {
                    let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                    let invalid = if foreign_role {
                        &mut alien
                    } else {
                        &mut *flusher
                    };
                    let error = writer
                        .catalog_commit(invalid, actor, private)
                        .err()
                        .unwrap();
                    assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                        if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                    alien.check_idle().unwrap();
                }
                assert_eq!(hex(private), private_before);
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, Point::CommitPreparing, entered, release);
                std::thread::scope(|scope| {
                    let (rejected, rejection) = sync_channel(1);
                    let (retry, retry_started) = sync_channel(1);
                    let committing = scope.spawn(move || {
                        let mut committer = core.try_owner(OwnerRole::Foreground).unwrap();
                        let original = writer.catalog_test_commit_state();
                        let error = writer
                            .catalog_commit(&mut committer, actor, private)
                            .err()
                            .unwrap();
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Prepare, Effect::Unchanged)
                        );
                        assert!(matches!(error.cause, RetentionCause::Frame(error)
                            if error.cause == crate::storage::dirty_frame::Cause::Stale));
                        assert_eq!(writer.catalog_test_commit_state(), original);
                        assert_eq!(writer.catalog_test_source_used(), 0);
                        assert_eq!(writer.catalog_test_physical_pins(), 0);
                        committer.check_idle().unwrap();
                        rejected.send(()).unwrap();
                        // Exactly one planned fresh preparation after the authentic
                        // stale rejection; no retry-until-success loop.
                        retry_started.recv_timeout(WAIT).unwrap();
                        let result = writer
                            .catalog_commit(&mut committer, actor, private)
                            .unwrap();
                        assert_eq!(result.operation, Operation::Commit);
                        assert_eq!(result.header.lsn, 9);
                        committer.check_idle().unwrap();
                        assert_eq!(writer.catalog_test_source_used(), 0);
                        assert_eq!(writer.catalog_test_physical_pins(), 0);
                    });
                    arrived.recv_timeout(WAIT).unwrap();
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), stage as u64);
                    assert_eq!(state.lock().unwrap().images(), before);
                    let (entered, arrived) = sync_channel(1);
                    let (prepared_resume, release) = sync_channel(1);
                    hook::install(core, Point::CommitPrepared, entered, release);
                    resume.send(Resume::Continue).unwrap();
                    arrived.recv_timeout(WAIT).unwrap();
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), stage as u64);
                    assert_eq!(state.lock().unwrap().images(), before);
                    // The disjoint original flusher may clean a prepared target.
                    // Its recLSN changes, so the original pre-stage full-stamp
                    // validation must reject this preparation without WAL effects.
                    let mut step = flush.try_flush(flusher, &root).unwrap();
                    for _ in 0..8 {
                        if !matches!(
                            step,
                            RetentionFlushStep::WaitingWal
                                | RetentionFlushStep::WaitingSource
                                | RetentionFlushStep::RetiredPendingFinalize
                        ) {
                            break;
                        }
                        step = flush.continue_flush(flusher).unwrap();
                    }
                    assert!(
                        matches!(step, RetentionFlushStep::Complete { finalized, .. } if finalized.cleaned)
                    );
                    let after_flush = state.lock().unwrap().images();
                    prepared_resume.send(Resume::Continue).unwrap();
                    rejection.recv_timeout(WAIT).unwrap();
                    assert_eq!(state.lock().unwrap().images(), after_flush);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_horizon().0, 0);
                    assert!(!fence.is_fenced());
                    let (entered, arrived) = sync_channel(1);
                    let (retry_prepare_resume, release) = sync_channel(1);
                    hook::install(core, Point::CommitPreparing, entered, release);
                    retry.send(()).unwrap();
                    arrived.recv_timeout(WAIT).unwrap();
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), stage as u64);
                    let (entered, arrived) = sync_channel(1);
                    let (retry_prepared_resume, release) = sync_channel(1);
                    hook::install(core, Point::CommitPrepared, entered, release);
                    retry_prepare_resume.send(Resume::Continue).unwrap();
                    arrived.recv_timeout(WAIT).unwrap();
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), stage as u64);
                    assert_eq!(state.lock().unwrap().images(), after_flush);
                    let (entered, arrived) = sync_channel(1);
                    let (pending_resume, release) = sync_channel(1);
                    hook::install(core, Point::Pending, entered, release);
                    retry_prepared_resume.send(Resume::Continue).unwrap();
                    arrived.recv_timeout(WAIT).unwrap();
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), pending as u64);
                    let (entered, arrived) = sync_channel(1);
                    let (completed_resume, release) = sync_channel(1);
                    hook::install(core, Point::CommitCompleted, entered, release);
                    pending_resume.send(Resume::Continue).unwrap();
                    arrived.recv_timeout(WAIT).unwrap();
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), publish as u64);
                    completed_resume.send(Resume::Continue).unwrap();
                    committing.join().unwrap();
                });
                assert_eq!(reader.catalog_test_source_used(), 0);
            }
            oracle.check(live, state, "C04");
            assert_eq!(live.catalog_test_q(), 1);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn commit_preparation_unwind_recovers_original_prefix() {
    case("MCC02", || {
        let oracle = Oracle::new();
        for point in [Point::CommitPreparing, Point::CommitPrepared] {
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
                    let _ = reader_private;
                    let actor = create_forward(
                        live, owner, flusher, status, undo, private, &oracle, state, 1,
                    );
                    let before = state.lock().unwrap().images();
                    let (entered, arrived) = sync_channel(1);
                    let (resume, release) = sync_channel(1);
                    hook::install(core, point, entered, release);
                    let (mut writer, reader, _) = live.split();
                    std::thread::scope(|scope| {
                        let controlling = scope.spawn(move || {
                            arrived.recv_timeout(WAIT).unwrap();
                            resume.send(Resume::Panic).unwrap();
                        });
                        assert!(
                            catch_unwind(AssertUnwindSafe(|| {
                                let _ = writer.catalog_commit(owner, actor, private);
                            }))
                            .is_err()
                        );
                        controlling.join().unwrap();
                    });
                    assert!(fence.is_fenced());
                    assert_eq!(
                        reader.catalog_test_source_used(),
                        writer.catalog_test_commit_charges()[1] as u64
                    );
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(state.lock().unwrap().images(), before);
                    before
                });
            // Both cuts have the original R03/LSN8 durable prefix. The native
            // N02 corpus independently describes the required rollback.
            let after_clr = super::lifecycle_progress::recover_to_n02_clr_cut(&images, &oracle);
            recover_check(&after_clr, &oracle, "N02");
        }
    });
}

#[test]
fn commit_preparation_and_cleanup_fatal_preserve_charge() {
    case("MCC03", || {
        let oracle = Oracle::new();
        for point in [
            Point::CommitPreparing,
            Point::CommitPrepared,
            Point::CommitCompleted,
        ] {
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
                let before = state.lock().unwrap().images();
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, point, entered, release);
                let (mut writer, reader, _) = live.split();
                let error = std::thread::scope(|scope| {
                    let controlling = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        fence.fence();
                        resume.send(Resume::Continue).unwrap();
                    });
                    let error = writer.catalog_commit(owner, actor, private).err().unwrap();
                    controlling.join().unwrap();
                    error
                });
                assert_eq!(error.effect, Effect::CallerMustResolve);
                if point == Point::CommitPreparing {
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert!(
                        matches!(&error.cause, RetentionCause::Storage(error)
                            if matches!(error.cause, super::super::super::super::Cause::Fenced)),
                        "original deep storage health Fatal wins over cleanup: {error:?}"
                    );
                } else if point == Point::CommitPrepared {
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert!(
                        matches!(&error.cause, RetentionCause::Wal(error)
                            if error.phase == crate::storage::wal_io::Phase::Observe
                                && matches!(error.cause, crate::storage::wal_io::Cause::Fenced)),
                        "original prefix Fatal wins over cleanup: {error:?}"
                    );
                } else {
                    assert_eq!(error.phase, RetentionPhase::Census);
                    assert!(
                        matches!(error.cause, RetentionCause::Fenced),
                        "cleanup Fatal replaces successful applied observation"
                    );
                }
                assert_eq!(
                    reader.catalog_test_source_used(),
                    writer.catalog_test_commit_charges()[if point == Point::CommitCompleted {
                        4
                    } else {
                        1
                    }] as u64
                );
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                if point == Point::CommitCompleted {
                    let mut frames: Vec<_> = (1..=5).map(|n| oracle.native_frame(n)).collect();
                    frames.extend((1..=3).map(|n| oracle.frame(&format!("R{n:02}"))));
                    frames.push(oracle.frame("C04"));
                    assert_wal(state, 1, &frames);
                    assert_ne!(state.lock().unwrap().images(), before);
                } else {
                    assert_eq!(state.lock().unwrap().images(), before);
                }
            });
        }
    });
}
