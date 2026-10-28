//! Synchronous lifecycle progress: original gate/source and ordinary application,
//! checked result cleanup and unwind, against the original Catalog corpus.
use super::*;
use crate::storage::disk::{RetentionCause, RetentionPhase};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn lifecycle_waits_and_keeps_one_charge() {
    case("MCLP01", || {
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
            let before = state.lock().unwrap().images();
            let (entered, captured) = sync_channel(1);
            let (resume_read, release) = sync_channel(1);
            hook::install(core, Point::LookupCaptured, entered, release);
            {
                let (mut writer, reader, mut flush) = live.split();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        let mut active = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut active).unwrap();
                        assert!(!lookup_resident!(
                            reader_ref,
                            &mut active,
                            &snapshot,
                            6,
                            reader_private
                        ));
                        snapshot.release_checked().unwrap();
                        active.check_idle().unwrap();
                        reader_private
                    });
                    captured.recv_timeout(WAIT).unwrap();
                    let read_charge = reader.catalog_test_source_used();
                    let (entered, waiting) = sync_channel(1);
                    let (resume_wait, release) = sync_channel(1);
                    hook::install(core, Point::ReadGateWaiting, entered, release);
                    let controlling = scope.spawn(move || {
                        waiting.recv_timeout(WAIT).unwrap();
                        assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(hook::observe(core).active_read_operations, 1);
                        assert_eq!(reader_ref.catalog_test_source_used(), read_charge);
                        assert_eq!(state.lock().unwrap().images(), before);
                        let (entered, preparing) = sync_channel(1);
                        let (resume_prepare, release) = sync_channel(1);
                        hook::install(core, Point::LifecyclePreparing, entered, release);
                        resume_wait.send(Resume::Continue).unwrap();
                        resume_read.send(Resume::Continue).unwrap();
                        let reader_private = reading.join().unwrap();
                        preparing.recv_timeout(WAIT).unwrap();
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut late).unwrap();
                        let error = reader_ref.lookup_catalog_resident(&mut late, &snapshot,
                            ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(),
                            reader_private).err().unwrap();
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                            if error.cause == crate::transaction::concurrency::Cause::InstallationPending));
                        snapshot.release_checked().unwrap();
                        let (entered, prepared) = sync_channel(1);
                        let (resume_return, release) = sync_channel(1);
                        hook::install(core, Point::LifecyclePrepared, entered, release);
                        resume_prepare.send(Resume::Continue).unwrap();
                        prepared.recv_timeout(WAIT).unwrap();
                        assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        assert_eq!(state.lock().unwrap().images(), before, "preparation has WAL0");
                        let snapshot = reader_ref.catalog_snapshot(&mut late).unwrap();
                        let error = reader_ref.lookup_catalog_resident(&mut late, &snapshot,
                            ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(), reader_private).err().unwrap();
                        snapshot.release_checked().unwrap();
                        assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                            if error.cause == crate::transaction::concurrency::Cause::InstallationPending));
                        let (entered, completed) = sync_channel(1);
                        let (resume_complete, release) = sync_channel(1);
                        hook::install(core, Point::LifecycleCompleted, entered, release);
                        resume_return.send(Resume::Continue).unwrap();
                        completed.recv_timeout(WAIT).unwrap();
                        assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        assert_ne!(state.lock().unwrap().images(), before, "application completed before cleanup");
                        resume_complete.send(Resume::Continue).unwrap();
                    });
                    writer
                        .catalog_rollback_start(owner, actor, private, &mut flush)
                        .unwrap();
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    controlling.join().unwrap();
                });
            }
            oracle.check(live, state, "R04");
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn lifecycle_capacity_roles_errors_and_retry() {
    case("MCLP02", || {
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
            let before = state.lock().unwrap().images();
            let private_before = hex(private);
            {
                let (mut writer, reader, mut flush) = live.split();
                let [shared, ..] = writer.catalog_test_commit_charges();
                let held = 65536 - shared - writer.catalog_test_retirement_charge() + 1;
                let error = reader.catalog_test_with_held_source(held, || {
                    let mut last = None;
                    for action in 0..4 {
                        let result = match action {
                            0 => writer.catalog_rollback_start(owner, actor, private, &mut flush),
                            1 => writer.catalog_abort_end(owner, actor, private, &mut flush),
                            2 => writer.catalog_cancel_reserved(owner, 1, private, &mut flush),
                            _ => writer.catalog_release_role(owner, 1, private, &mut flush),
                        };
                        let error = result.err().unwrap();
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Census, Effect::Unchanged)
                        );
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        assert_eq!(reader.catalog_test_source_used(), held as u64);
                        assert!(!hook::observe(core).read_gate_closed);
                        last = Some(error);
                    }
                    let error = last.unwrap();
                    assert_eq!(reader.catalog_test_source_used(), held as u64);
                    assert!(!hook::observe(core).read_gate_closed);
                    error
                });
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Census, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Capacity));
                assert_eq!(hex(private), private_before);
                // Malformed selection fails inside the reserved preparation.
                let error = writer
                    .catalog_release_role(owner, usize::MAX, private, &mut flush)
                    .err()
                    .unwrap();
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                let error = writer
                    .catalog_abort_end(owner, super::actor(1, 99, 1), private, &mut flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::MissingTransaction));
                let error = writer
                    .catalog_rollback_start(flusher, actor, private, &mut flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                    if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                let error = writer
                    .catalog_rollback_start(&mut alien, actor, private, &mut flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                    if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(hex(private), private_before);
                let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
                writer
                    .catalog_rollback_start(&mut maintenance, actor, private, &mut flush)
                    .unwrap();
                maintenance.check_idle().unwrap();
            }
            oracle.check(live, state, "R04");
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn lifecycle_unwind_recovers_original_prefix() {
    case("MCLP03", || {
        let oracle = Oracle::new();
        for point in [Point::LifecyclePreparing, Point::LifecyclePrepared] {
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
                    let (mut writer, reader, mut flush) = live.split();
                    std::thread::scope(|scope| {
                        let controlling = scope.spawn(move || {
                            arrived.recv_timeout(WAIT).unwrap();
                            resume.send(Resume::Panic).unwrap();
                        });
                        assert!(
                            catch_unwind(AssertUnwindSafe(|| {
                                let _ = writer
                                    .catalog_rollback_start(owner, actor, private, &mut flush);
                            }))
                            .is_err()
                        );
                        controlling.join().unwrap();
                    });
                    assert!(fence.is_fenced());
                    assert_eq!(
                        reader.catalog_test_source_used(),
                        writer.catalog_test_retirement_charge() as u64
                    );
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(state.lock().unwrap().images(), before);
                    before
                });
            // Both cuts have the original R03/LSN8 durable prefix. The native
            // N02 corpus independently describes the required rollback.
            let after_clr = recover_to_n02_clr_cut(&images, &oracle);
            recover_check(&after_clr, &oracle, "N02");
        }
    });
}

#[test]
fn lifecycle_fatal_preserves_error_and_charge() {
    case("MCLP04", || {
        let oracle = Oracle::new();
        for point in [
            Point::LifecyclePreparing,
            Point::LifecyclePrepared,
            Point::LifecycleCompleted,
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
                let (mut writer, reader, mut flush) = live.split();
                let error = std::thread::scope(|scope| {
                    let controlling = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        fence.fence();
                        resume.send(Resume::Continue).unwrap();
                    });
                    let error = writer
                        .catalog_rollback_start(owner, actor, private, &mut flush)
                        .err()
                        .unwrap();
                    controlling.join().unwrap();
                    error
                });
                assert_eq!(error.effect, Effect::CallerMustResolve);
                if point == Point::LifecyclePreparing {
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert!(
                        matches!(error.cause, RetentionCause::Frame(error)
                            if error.cause == crate::storage::dirty_frame::Cause::Fenced),
                        "original body Fatal wins over cleanup Fatal"
                    );
                } else if point == Point::LifecyclePrepared {
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert!(
                        matches!(error.cause, RetentionCause::Wal(_)),
                        "original apply-prefix Fatal wins over cleanup Fatal"
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
                    writer.catalog_test_retirement_charge() as u64
                );
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                if point == Point::LifecycleCompleted {
                    let mut frames: Vec<_> = (1..=5).map(|n| oracle.native_frame(n)).collect();
                    frames.extend((1..=4).map(|n| oracle.frame(&format!("R{n:02}"))));
                    assert_wal(state, 1, &frames);
                    assert_ne!(state.lock().unwrap().images(), before);
                } else {
                    assert_eq!(state.lock().unwrap().images(), before);
                }
            });
        }
    });
}

#[test]
fn lifecycle_cancel_keeps_original_flush_claim_exclusion() {
    case("MCLP05", || {
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
            let root = live.target(Plan4Page::Root).unwrap();
            {
                let (mut writer, reader, mut flush) = live.split();
                let mut pinned = core.try_owner(OwnerRole::Foreground).unwrap();
                let read = reader
                    .catalog_test_pool()
                    .reserve_target_set(
                        &mut pinned,
                        &[crate::storage::dirty_frame::TargetRequest {
                            identity: root.identity(),
                            kind: root.kind(),
                            level: root.level(),
                        }],
                    )
                    .unwrap()
                    .try_read()
                    .unwrap();
                // Original checked flush retires its physical I/O while this
                // content guard prevents finalization; its actual claim survives.
                let step = flush.try_flush(flusher, &root).unwrap();
                assert!(matches!(step, RetentionFlushStep::RetiredPendingFinalize));
                read.release_checked().unwrap();
                pinned.check_idle().unwrap();
                let before = state.lock().unwrap().images();
                let error = writer
                    .catalog_cancel_reserved(owner, usize::MAX, private, &mut flush)
                    .err()
                    .unwrap();
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Busy));
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                // The other lifecycle actions had no Ready-phase restriction
                // in the original exclusive API. Preserve their checked path.
                writer
                    .catalog_rollback_start(owner, actor, private, &mut flush)
                    .unwrap();
                assert!(matches!(
                    flush.continue_flush(flusher).unwrap(),
                    RetentionFlushStep::Complete { .. }
                ));
                let error = writer
                    .catalog_cancel_reserved(owner, usize::MAX, private, &mut flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
            }
            oracle.check(live, state, "R04");
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

/// Same durable CLR/owner-idle cut as the original native N02 controller.
/// A fresh runtime is required afterward: N02 L11–16 belong to epoch3.
pub(super) fn recover_to_n02_clr_cut(images: &Images, oracle: &Oracle) -> Images {
    let directory = clone_directory(images);
    let state = directory.state();
    let after_clr = {
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let runtime = actual_runtime(&directory, &mut maintenance, &mut dwb, &mut slot);
        assert_eq!(runtime.observation().source_lsn, 8);
        assert!(!state.lock().unwrap().files.contains_key("d3-wal-00000001"));
        let core = PageConcurrency::new(runtime.session_fence()).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 9];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut closed = runtime
            .bind(&owner, &core, &mut frames, &mut operation)
            .unwrap();
        let mut observed_lsn = 8;
        let mut cut = None;
        for _ in 0..256 {
            let progress = closed.step(&mut owner, &mut flusher).unwrap();
            let (prefix, durable, _, _, pending, claim, pins) =
                closed.inner.free_undo_test_state().unwrap();
            assert!(!progress.ready);
            if prefix.complete_lsn > observed_lsn {
                assert_eq!(prefix.complete_lsn, observed_lsn + 1);
                oracle.check_scenario(&closed.inner, &state, "N02", prefix.complete_lsn);
                observed_lsn = prefix.complete_lsn;
            }
            if prefix.complete_lsn == 10 {
                assert_eq!((prefix.synced_lsn, durable), (10, Some(10)));
                assert!(!pending && !claim && pins == 0);
                owner.check_idle().unwrap();
                flusher.check_idle().unwrap();
                cut = Some(state.lock().unwrap().images());
                break;
            }
            assert!(prefix.complete_lsn < 10, "CLR cut skipped before AbortEnd");
        }
        cut.expect("bounded actual recovery did not reach N02 CLR cut")
    };
    assert_closed(&state);
    assert!(
        state
            .lock()
            .unwrap()
            .files
            .values()
            .map(|file| file.length)
            .sum::<u64>()
            <= 32 * 1_048_576
    );
    after_clr
}
