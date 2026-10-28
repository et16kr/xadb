//! Synchronous retirement: one source reservation across preparation and apply,
//! checked result cleanup and unwind, against the original Catalog corpus.
use super::*;
use crate::storage::disk::{RetentionCause, RetentionPhase};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

pub(super) fn retire<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    ordinal: usize,
    operation: Operation,
    private: &mut [u8; PRIVATE_BYTES],
) {
    let (mut writer, reader, mut flush) = live.split();
    writer
        .retire_catalog(owner, ordinal, operation, private, &mut flush)
        .unwrap();
    assert_eq!(reader.catalog_test_source_used(), 0);
}

#[test]
fn preparation_waits_for_reader_and_returns_its_source() {
    case("MCRP02", || {
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
                        hook::install(core, Point::RetirementPreparing, entered, release);
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
                        hook::install(core, Point::RetirementPrepared, entered, release);
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
                        hook::install(core, Point::RetirementCompleted, entered, release);
                        resume_return.send(Resume::Continue).unwrap();
                        completed.recv_timeout(WAIT).unwrap();
                        assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        assert_ne!(state.lock().unwrap().images(), before, "application completed before cleanup");
                        resume_complete.send(Resume::Continue).unwrap();
                    });
                    writer
                        .retire_catalog(owner, 1, Operation::Clr, private, &mut flush)
                        .unwrap();
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    controlling.join().unwrap();
                });
            }
            oracle.check(live, state, "R05");
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn preparation_capacity_errors_roles_and_retry() {
    case("MCRP03", || {
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
            live.catalog_rollback_start(owner, actor, private).unwrap();
            let before = state.lock().unwrap().images();
            let private_before = hex(private);
            {
                let (mut writer, reader, mut flush) = live.split();
                let [shared, ..] = writer.catalog_test_commit_charges();
                let held = 65536 - shared - writer.catalog_test_retirement_charge() + 1;
                let error = reader.catalog_test_with_held_source(held, || {
                    let error = writer
                        .retire_catalog(owner, 1, Operation::Clr, private, &mut flush)
                        .err()
                        .unwrap();
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
                    .retire_catalog(owner, usize::MAX, Operation::Clr, private, &mut flush)
                    .err()
                    .unwrap();
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                let error = writer
                    .retire_catalog(owner, 1, Operation::Cancel, private, &mut flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Unsupported));
                let error = writer
                    .retire_catalog(flusher, 1, Operation::Clr, private, &mut flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                    if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                let error = writer
                    .retire_catalog(&mut alien, 1, Operation::Clr, private, &mut flush)
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
                    .retire_catalog(&mut maintenance, 1, Operation::Clr, private, &mut flush)
                    .unwrap();
                maintenance.check_idle().unwrap();
            }
            oracle.check(live, state, "R05");
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn preparation_unwind_keeps_charge_and_fresh_recovery() {
    case("MCRP04", || {
        let oracle = Oracle::new().with_retire();
        for point in [Point::RetirementPreparing, Point::RetirementPrepared] {
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
                    live.catalog_rollback_start(owner, actor, private).unwrap();
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
                                let _ = writer.retire_catalog(
                                    owner,
                                    1,
                                    Operation::Clr,
                                    private,
                                    &mut flush,
                                );
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
            // Both cuts have exactly the R04 durable prefix. The independent
            // MF14 corpus describes its mandatory rollback in a fresh owner.
            recover_check(&images, &oracle, "MF14");
        }
    });
}

#[test]
fn preparation_fatal_preserves_original_error_and_charge() {
    case("MCRP05", || {
        let oracle = Oracle::new();
        for point in [
            Point::RetirementPreparing,
            Point::RetirementPrepared,
            Point::RetirementCompleted,
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
                live.catalog_rollback_start(owner, actor, private).unwrap();
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
                        .retire_catalog(owner, 1, Operation::Clr, private, &mut flush)
                        .err()
                        .unwrap();
                    controlling.join().unwrap();
                    error
                });
                assert_eq!(error.effect, Effect::CallerMustResolve);
                if point == Point::RetirementPreparing {
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert!(
                        matches!(error.cause, RetentionCause::Frame(error)
                            if error.cause == crate::storage::dirty_frame::Cause::Fenced),
                        "original body Fatal wins over cleanup Fatal"
                    );
                } else if point == Point::RetirementPrepared {
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert!(
                        matches!(error.cause, RetentionCause::Storage(_)),
                        "original apply-entry Fatal wins over cleanup Fatal"
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
                if point == Point::RetirementCompleted {
                    let mut frames: Vec<_> = (1..=5).map(|n| oracle.native_frame(n)).collect();
                    frames.extend((1..=5).map(|n| oracle.frame(&format!("R{n:02}"))));
                    assert_wal(state, 1, &frames);
                    assert_ne!(state.lock().unwrap().images(), before);
                } else {
                    assert_eq!(state.lock().unwrap().images(), before);
                }
            });
        }
    });
}
