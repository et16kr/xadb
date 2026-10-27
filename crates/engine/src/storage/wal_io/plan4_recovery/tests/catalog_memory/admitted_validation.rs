//! Real fixed source reservation precedes deep validation; malformed metadata
//! is only a diagnostic input, never an invented owner or installation token.
use super::*;
use crate::storage::disk::{RetentionCause, RetentionError, RetentionPhase};
use crate::transaction::concurrency::catalog_test_support::observe;

#[test]
fn capacity_and_original_admission_precede_unobserved_metadata_faults() {
    case("C2_ADMIT01", || {
        for resident in [false, true] {
            for snapshot_fault in [false, true] {
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
                    let _ = (status, undo, private, reader_private);
                    prepare_catalog(live, owner, flusher);
                    let before = state.lock().unwrap().images();
                    let before_events = state.lock().unwrap().events.len();
                    let (writer, reader, _) = live.split();
                    let mut second = core.try_owner(OwnerRole::Foreground).unwrap();
                    let mut wrong_core = foreign.try_owner(OwnerRole::Foreground).unwrap();
                    let original = reader.catalog_snapshot(owner).unwrap();
                    let mut checked_later = reader.catalog_snapshot(&mut second).unwrap();
                    let charges = reader.catalog_test_fixed_charges();
                    let held = reader.catalog_call(owner, &original).unwrap();
                    assert_eq!(reader.catalog_test_source_used(), charges[1] as u64);
                    let pool = writer.catalog_test_pool();
                    if snapshot_fault {
                        checked_later.test_corrupt_observed_q();
                    } else {
                        probe::seed_pins(pool, 0, 0, 1); // bad sum, no actual pin
                    }
                    // Both failures retain their original projections even
                    // when source is full and deeper metadata is malformed.
                    let error: RetentionError = reader
                        .catalog_call(&mut wrong_core, &checked_later)
                        .err()
                        .unwrap()
                        .into();
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Census, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::Frame(error)
                        if error.cause == crate::storage::dirty_frame::Cause::BindingMismatch));
                    let error: RetentionError = reader
                        .catalog_call(flusher, &checked_later)
                        .err()
                        .unwrap()
                        .into();
                    assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                        if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                    assert_eq!(error.effect, Effect::Unchanged);
                    let error: RetentionError = if resident {
                        reader.catalog_resident_call(&mut second, &checked_later)
                    } else {
                        reader.catalog_call(&mut second, &checked_later)
                    }
                    .err()
                    .unwrap()
                    .into();
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Census, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::Capacity));
                    assert!(!fence.is_fenced());
                    assert_eq!(reader.catalog_test_source_used(), charges[1] as u64);
                    assert_eq!(observe(core).active_read_operations, 1);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(
                        probe::diagnose(pool, 0, &mut boxed::<PAGE_BYTES>()).pins,
                        [0; 64]
                    );
                    second.check_idle().unwrap();
                    wrong_core.check_idle().unwrap();
                    held.finish_checked().unwrap();
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert_eq!(observe(core).active_read_operations, 0);
                    // The same malformed input now reaches the original deep
                    // validator under its source charge and must fail Fatal.
                    let error: RetentionError = if resident {
                        reader.catalog_resident_call(&mut second, &checked_later)
                    } else {
                        reader.catalog_call(&mut second, &checked_later)
                    }
                    .err()
                    .unwrap()
                    .into();
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    if snapshot_fault {
                        assert_eq!(error.phase, RetentionPhase::Census);
                        assert!(matches!(error.cause, RetentionCause::Visibility(error)
                            if error.cause == super::super::super::super::plan4_pages::VisibilityCause::Invariant));
                    } else {
                        assert_eq!(error.phase, RetentionPhase::Prepare);
                        assert!(matches!(error.cause, RetentionCause::Frame(error)
                            if error.cause == crate::storage::dirty_frame::Cause::Invariant));
                    }
                    assert!(fence.is_fenced());
                    assert_eq!(
                        reader.catalog_test_source_used(),
                        charges[if resident { 2 } else { 1 }] as u64
                    );
                    assert_eq!(observe(core).active_read_operations, 0);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(
                        probe::diagnose(pool, 0, &mut boxed::<PAGE_BYTES>()).pins,
                        [0; 64]
                    );
                    assert_eq!(state.lock().unwrap().images(), before);
                    assert_eq!(state.lock().unwrap().events.len(), before_events);
                });
            }
        }
    });
}

#[test]
fn already_observed_pool_poison_precedes_capacity() {
    case("C2_ADMIT02", || {
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
            let _ = (status, undo, private, reader_private);
            prepare_catalog(live, owner, flusher);
            let before = state.lock().unwrap().images();
            let (writer, reader, _) = live.split();
            let mut second = core.try_owner(OwnerRole::Foreground).unwrap();
            let original = reader.catalog_snapshot(owner).unwrap();
            let second_snapshot = reader.catalog_snapshot(&mut second).unwrap();
            let held = reader.catalog_call(owner, &original).unwrap();
            let charge = reader.catalog_test_source_used();
            probe::poison_pins(writer.catalog_test_pool());
            assert!(!fence.is_fenced());
            let error: RetentionError = reader
                .catalog_call(&mut second, &second_snapshot)
                .err()
                .unwrap()
                .into();
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Prepare, Effect::CallerMustResolve)
            );
            assert!(matches!(error.cause, RetentionCause::Frame(error)
                if error.cause == crate::storage::dirty_frame::Cause::Poisoned));
            assert_eq!(reader.catalog_test_source_used(), charge);
            assert_eq!(observe(core).active_read_operations, 1);
            assert!(held.finish_checked().is_err());
            assert_eq!(observe(core).active_read_operations, 0);
            assert_eq!(reader.catalog_test_source_used(), charge);
            assert!(fence.is_fenced());
            assert_eq!(state.lock().unwrap().images(), before);
        });
    });
}
