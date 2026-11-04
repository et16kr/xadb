//! Original admission, cancellation and checked-error branches of E02/03/05/07/09.
use super::*;

#[test]
fn structural_effect_original_clean_victim_stamp_change_rejects_before_claim() {
    clean_victim(false);
}

#[test]
fn structural_effect_original_clean_victim_stamp_change_child() {
    clean_victim(true);
}

fn clean_victim(child: bool) {
    case_within(
        &format!("SMO-E02-clean-{child}"),
        Duration::from_secs(120),
        || {
            ready!(
                child,
                extra_undo = 14,
                |writer, reader, flush, owner, flusher, private, core, fence, state, target| {
                    let _ = flusher;
                    let pool = reader.catalog_test_pool();
                    let before = residents(pool);
                    assert_eq!(before.len(), 32, "actual INIT fills original pool");
                    let generations = init::retention_generations(pool);
                    let bank = writer.recovery_test_bank().to_vec();
                    let jobs = writer.catalog_test_jobs().to_vec();
                    let wal = files(state);
                    let fault = writer.test_structural_fault(6);
                    let error = writer
                        .split_row(owner, object(), target, private, flush)
                        .err()
                        .unwrap();
                    assert_eq!(fault.hits(), 1);
                    drop(fault);
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(
                        matches!(error.cause, RetentionCause::Frame(e) if e.cause == dirty_frame::Cause::Stale)
                    );
                    assert_eq!(files(state), wal);
                    assert_eq!(writer.recovery_test_bank(), bank);
                    assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                    assert_eq!(init::retention_generations(pool), generations);
                    let after = residents(pool);
                    let mut changes = 0;
                    for (old, new) in before.iter().zip(&after) {
                        assert_eq!(old.target, new.target);
                        assert_eq!(old.bytes, new.bytes);
                        assert_eq!(old.requirement, new.requirement);
                        if old.stamp != new.stamp {
                            assert_eq!(
                                new.stamp.modification_counter,
                                old.stamp.modification_counter + 1
                            );
                            let at =
                                probe::registered_index(pool, old.target.identity.page.page_no)
                                    .unwrap();
                            probe::seed_counter(pool, at, old.stamp.modification_counter);
                            changes += 1;
                        }
                    }
                    assert_eq!(changes, 1);
                    released(pool, core, false);
                    fatal_returned(core);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!fence.is_fenced() && !writer.test_structural_pending());
                    writer
                        .split_row(owner, object(), target, private, flush)
                        .unwrap();
                    assert!(!fence.is_fenced());
                    println!(
                        "SMO_E02 clean victim child={child} original stamp/unchanged generations/reentry"
                    );
                }
            );
        },
    );
}

#[test]
fn structural_effect_changed_selection_and_before_cancel_without_wal() {
    for child in [false, true] {
        for variant in 0..3 {
            case_within(
                &format!("SMO-E02-{child}-{variant}"),
                Duration::from_secs(90),
                || {
                    ready!(child, |writer,
                                   reader,
                                   flush,
                                   owner,
                                   flusher,
                                   private,
                                   core,
                                   fence,
                                   state,
                                   target| {
                        let _ = flusher;
                        let pool = reader.catalog_test_pool();
                        let at = probe::registered_index(pool, target.page_no).unwrap();
                        let mut bytes = boxed::<PAGE_BYTES>();
                        let before = probe::diagnose(pool, at, &mut bytes);
                        let bank = writer.recovery_test_bank().to_vec();
                        let jobs = writer.catalog_test_jobs().to_vec();
                        let wal = files(state);
                        let generations = init::retention_generations(pool);
                        let fault = (variant == 0).then(|| writer.test_structural_fault(3));
                        let result = if variant == 0 {
                            writer.split_row(owner, object(), target, private, flush)
                        } else {
                            paused(
                                pool,
                                init::InitPoint::StructuralClaimed,
                                || {
                                    if variant == 1 {
                                        probe::seed_counter(
                                            pool,
                                            at,
                                            before.frame.modification_counter + 1,
                                        );
                                        let mut entry = before.entry.unwrap();
                                        entry.modification_counter += 1;
                                        probe::seed_dpt(pool, at, Some(entry));
                                    } else {
                                        init::corrupt_retention_content(
                                            pool,
                                            at,
                                            PAGE_BYTES - 1,
                                            bytes[PAGE_BYTES - 1] ^ 1,
                                        );
                                    }
                                },
                                probe::Resume::Continue,
                                || writer.split_row(owner, object(), target, private, flush),
                            )
                        };
                        let error = result.err().unwrap();
                        assert_eq!(error.effect, Effect::Unchanged);
                        if variant == 0 {
                            assert!(
                                matches!(error.cause, RetentionCause::Frame(e) if e.cause == dirty_frame::Cause::Stale)
                            );
                            assert_eq!(fault.as_ref().unwrap().hits(), 1);
                        } else {
                            assert!(matches!(error.cause, RetentionCause::Busy));
                        }
                        drop(fault);
                        assert_eq!(files(state), wal);
                        assert_eq!(writer.recovery_test_bank(), bank);
                        assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                        let cancelled = init::retention_generations(pool);
                        let consumed: Vec<_> = (1..32)
                            .filter(|at| cancelled[*at] != generations[*at])
                            .collect();
                        assert_eq!(consumed.len(), if variant == 0 || child { 1 } else { 2 });
                        for at in &consumed {
                            assert_eq!(cancelled[*at], generations[*at].map(|g| g + 1));
                        }
                        if variant == 1 {
                            probe::seed_counter(pool, at, before.frame.modification_counter);
                            probe::seed_dpt(pool, at, before.entry);
                        } else if variant == 2 {
                            init::corrupt_retention_content(
                                pool,
                                at,
                                PAGE_BYTES - 1,
                                bytes[PAGE_BYTES - 1],
                            );
                        }
                        released(pool, core, false);
                        assert_eq!(reader.catalog_test_source_used(), 0);
                        assert!(!fence.is_fenced() && !writer.test_structural_pending());
                        owner.check_idle().unwrap();
                        writer
                            .split_row(owner, object(), target, private, flush)
                            .unwrap();
                        assert!(!fence.is_fenced());
                        println!(
                            "SMO_E02 child={child} variant={variant} exact before rejection and reentry"
                        );
                    });
                },
            );
        }
    }
}

#[test]
fn structural_effect_flush_admission_and_failed_cancel_lease() {
    for child in [false, true] {
        for variant in 0..4 {
            case_within(
                &format!("SMO-E03-{child}-{variant}"),
                Duration::from_secs(90),
                || {
                    ready!(child, |writer,
                                   reader,
                                   flush,
                                   owner,
                                   flusher,
                                   private,
                                   core,
                                   fence,
                                   state,
                                   target| {
                        let _ = flusher;
                        let pool = reader.catalog_test_pool();
                        let bank = writer.recovery_test_bank().to_vec();
                        let wal = files(state);
                        let fault = match variant {
                            0 => {
                                flush.row_test_flush_fault(2);
                                None
                            }
                            1 => {
                                flush.row_test_flush_fault(1);
                                None
                            }
                            2 => Some(writer.test_structural_fault(1)),
                            3 => Some(writer.test_structural_fault(2)),
                            _ => unreachable!(),
                        };
                        let capacity = (variant == 3).then(
                            crate::storage::wal_io::durability_test_support::stage_capacity_once,
                        );
                        let error = writer
                            .split_row(owner, object(), target, private, flush)
                            .err()
                            .unwrap();
                        if let Some(fault) = &fault {
                            assert_eq!(fault.hits(), 1);
                        }
                        if let Some(capacity) = &capacity {
                            assert_eq!(capacity.hits(), 1);
                        }
                        drop(capacity);
                        drop(fault);
                        assert_eq!(files(state), wal);
                        assert_eq!(writer.recovery_test_bank(), bank);
                        assert!(!writer.test_structural_pending());
                        released(pool, core, variant == 3);
                        if variant == 0 {
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert!(matches!(error.cause, RetentionCause::Busy));
                            assert!(!fence.is_fenced());
                            assert_eq!(reader.catalog_test_source_used(), 0);
                            flush.row_test_flush_fault(0);
                            writer
                                .split_row(owner, object(), target, private, flush)
                                .unwrap();
                        } else {
                            assert_eq!(error.effect, Effect::CallerMustResolve);
                            assert!(fence.is_fenced());
                            assert_eq!(reader.catalog_test_source_used(), 44_304);
                            fatal_returned(core);
                            assert!(matches!(
                                error.cause,
                                RetentionCause::BindingMismatch | RetentionCause::Busy
                            ));
                        }
                        println!("SMO_E03 child={child} variant={variant} {error:?}");
                    });
                },
            );
        }
    }
}

#[test]
fn structural_effect_actual_stage_history_and_replay_errors_are_fatal() {
    for child in [false, true] {
        for variant in 0..4 {
            case_within(
                &format!("SMO-E05-E07-E09-{child}-{variant}"),
                Duration::from_secs(90),
                || {
                    ready!(child, |writer,
                                   reader,
                                   flush,
                                   owner,
                                   flusher,
                                   private,
                                   core,
                                   fence,
                                   state,
                                   target| {
                        let _ = flusher;
                        let pool = reader.catalog_test_pool();
                        let wal = files(state);
                        let fault =
                            (variant >= 2).then(|| writer.test_structural_fault(variant + 2));
                        let result = if variant < 2 {
                            let point = if variant == 0 {
                                init::InitPoint::StructuralStageEntered
                            } else {
                                init::InitPoint::StructuralHistoryInstalling
                            };
                            paused(
                                pool,
                                point,
                                || fence.fence(),
                                probe::Resume::Continue,
                                || writer.split_row(owner, object(), target, private, flush),
                            )
                        } else {
                            writer.split_row(owner, object(), target, private, flush)
                        };
                        let error = result.err().unwrap();
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        if let Some(fault) = &fault {
                            assert_eq!(fault.hits(), 1);
                        }
                        drop(fault);
                        assert!(fence.is_fenced());
                        assert_eq!(writer.test_structural_pending(), variant != 0);
                        assert!(writer.test_structural_replay_returned());
                        assert_eq!(reader.catalog_test_source_used(), 44_304);
                        fatal_returned(core);
                        released(pool, core, true);
                        if variant < 2 {
                            assert_eq!(files(state), wal);
                        }
                        if variant == 2 {
                            assert!(matches!(error.cause, RetentionCause::Busy));
                        }
                        println!("SMO_ACTUAL_FAILURE child={child} variant={variant} {error:?}");
                    });
                },
            );
        }
    }
}
