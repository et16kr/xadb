//! Actual result consumption remains inside original source/progress admission.
use super::*;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

#[test]
fn direct_error_finish_keeps_priority_and_original_accounting() {
    use crate::storage::disk::{RetentionCause, RetentionError, RetentionPhase};
    case("C2_TAIL01", || {
        let oracle = Oracle::new();
        for original_fatal in [false, true] {
            for cleanup_fenced in [false, true] {
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
                    let (writer, reader, _) = live.split();
                    let snapshot = reader.catalog_snapshot(owner).unwrap();
                    let call = reader.catalog_resident_call(owner, &snapshot).unwrap();
                    let charge = reader.catalog_test_source_used();
                    assert!(charge > 0);
                    let before = state.lock().unwrap().events.len();
                    // Caller-supplied error packets exercise all priority
                    // branches; they grant no product/source authority.
                    let original = RetentionError {
                        phase: RetentionPhase::Load,
                        cause: if original_fatal {
                            RetentionCause::Invariant
                        } else {
                            RetentionCause::Capacity
                        },
                        effect: if original_fatal {
                            Effect::CallerMustResolve
                        } else {
                            Effect::Unchanged
                        },
                    };
                    if cleanup_fenced {
                        fence.fence();
                    }
                    let error = call.finish_error(original);
                    if original_fatal {
                        assert_eq!(error.phase, RetentionPhase::Load);
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        assert!(matches!(error.cause, RetentionCause::Invariant));
                    } else if cleanup_fenced {
                        assert_eq!(error.phase, RetentionPhase::Census);
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        assert!(matches!(error.cause, RetentionCause::Fenced));
                    } else {
                        assert_eq!(error.phase, RetentionPhase::Load);
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                    }
                    assert_eq!(hook::observe(core).active_read_operations, 0);
                    assert_eq!(
                        reader.catalog_test_source_used(),
                        if cleanup_fenced { charge } else { 0 }
                    );
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(state.lock().unwrap().events.len(), before);
                    assert_eq!(fence.is_fenced(), cleanup_fenced);
                    if cleanup_fenced {
                        assert!(snapshot.release_checked().is_err());
                    } else {
                        snapshot.release_checked().unwrap();
                        owner.check_idle().unwrap();
                    }
                });
            }
        }
    });
}

#[test]
fn query_block_consumes_missing_and_found_before_close() {
    case("C2_TAIL02", || {
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
            query(live, owner, flusher, reader_private, 0, 6, false);
            {
                let (writer, reader, _) = live.split();
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
            }
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            {
                let (mut writer, _, _) = live.split();
                writer.commit_catalog(owner, prepared).unwrap();
            }
            query(live, owner, flusher, reader_private, 1, 6, true);
            let (writer, reader, _) = live.split();
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            assert_eq!(hook::observe(core).active_read_operations, 0);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

fn lookup_error_cleanup(io_error: bool) {
    case(if io_error { "C2_SCOPE06" } else { "C2_SCOPE05" }, || {
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
            let _actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            super::read_boundary::evict_undo(live, flusher, undo);
            let (_, reader, _) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
            let mut call = if io_error {
                reader.catalog_call(owner, &snapshot).unwrap()
            } else {
                reader.catalog_resident_call(owner, &snapshot).unwrap()
            };
            let charge = reader.catalog_test_source_used();
            let before = state.lock().unwrap().events.len();
            if io_error {
                let mut io = state.lock().unwrap();
                io.events.clear();
                io.fault_hits = 0;
                io.fault = Some(port::Fault {
                    key: port::Key::new("read", HOME_NAME, 10 * PAGE_BYTES as u64, 1),
                    action: port::Action::BeforeError,
                });
            }
            let original = call
                .lookup(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                )
                .err()
                .unwrap();
            assert!(charge > 0);
            assert_eq!(reader.catalog_test_source_used(), charge);
            assert_eq!(hook::observe(core).active_read_operations, 1);
            if io_error {
                assert_eq!(state.lock().unwrap().fault_hits, 1);
                assert_eq!(original.effect, Effect::CallerMustResolve);
                assert_eq!(original.phase, crate::storage::disk::RetentionPhase::Load);
                assert!(matches!(
                    original.cause,
                    crate::storage::disk::RetentionCause::PageIo(_)
                ));
            } else {
                assert_eq!(original.effect, Effect::Unchanged);
                assert!(matches!(
                    original.cause,
                    crate::storage::disk::RetentionCause::Busy
                ));
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(state.lock().unwrap().events.len(), before);
                fence.fence();
            }
            let error = call.finish_error(original);
            assert_eq!(error.effect, Effect::CallerMustResolve);
            if io_error {
                assert_eq!(error.phase, crate::storage::disk::RetentionPhase::Load);
                assert!(matches!(
                    error.cause,
                    crate::storage::disk::RetentionCause::PageIo(_)
                ));
            } else {
                assert!(matches!(
                    error.cause,
                    crate::storage::disk::RetentionCause::Fenced
                ));
            }
            assert_eq!(hook::observe(core).active_read_operations, 0);
            assert_eq!(reader.catalog_test_source_used(), charge);
            assert!(fence.is_fenced());
            assert!(snapshot.release_checked().is_err());
        });
    });
}

#[test]
fn actual_resident_busy_is_upgraded_by_fatal_scope_cleanup() {
    lookup_error_cleanup(false);
}

#[test]
fn actual_undo_io_fatal_survives_scope_cleanup() {
    lookup_error_cleanup(true);
}

#[test]
fn found_view_consumption_keeps_source_and_progress() {
    case("C2_SCOPE01", || {
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
            let before = state.lock().unwrap().events.len();
            let (writer, reader, _) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
            let mut call = reader.catalog_resident_call(owner, &snapshot).unwrap();
            let charge = reader.catalog_test_source_used();
            assert!(charge > 0);
            // Sequential lookups share one reservation; every copied/nested
            // view is last used before the next reborrow of this same scope.
            for _ in 0..2 {
                let record = match call
                    .lookup(
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .unwrap()
                {
                    CatalogLookupStep::Found(record) => record,
                    _ => panic!("committed table T"),
                };
                let copied = record;
                let crate::storage::catalog_record::CatalogDefinitionView::Table(table) =
                    copied.definition()
                else {
                    panic!("table definition")
                };
                let column = table.column(0).unwrap();
                assert_eq!(reader.catalog_test_source_used(), charge);
                assert_eq!(hook::observe(core).active_read_operations, 1);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                let other_snapshot = reader.catalog_snapshot(&mut other).unwrap();
                let rejected: crate::storage::disk::RetentionError = reader
                    .catalog_call(&mut other, &other_snapshot)
                    .err()
                    .unwrap()
                    .into();
                assert_eq!(rejected.effect, Effect::Unchanged);
                assert!(matches!(
                    rejected.cause,
                    crate::storage::disk::RetentionCause::Capacity
                ));
                other_snapshot.release_checked().unwrap();
                other.check_idle().unwrap();
                assert_eq!(
                    (
                        table.name(),
                        column.name,
                        copied.header().identity.object.get()
                    ),
                    ("T", "i", 6)
                );
            }
            call.finish_checked().unwrap();
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(hook::observe(core).active_read_operations, 0);
            snapshot.release_checked().unwrap();
            owner.check_idle().unwrap();
            assert_eq!(state.lock().unwrap().events.len(), before);
            assert!(!fence.is_fenced());
        });
    });
}

fn retained_missing(fenced: bool) {
    case(if fenced { "C2_SCOPE03" } else { "C2_SCOPE02" }, || {
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
            {
                let (mut writer, reader, _) = live.split();
                let reader_ref = &reader;
                std::thread::scope(|threads| {
                    let reading = threads.spawn(move || {
                        let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                        let mut call = reader_ref.catalog_call(&mut separate, &snapshot).unwrap();
                        assert!(matches!(
                            call.lookup(
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private
                            )
                            .unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        // Product lookup and all refs already returned; this
                        // is the real selected caller retaining its scope.
                        entered.send(()).unwrap();
                        release.recv_timeout(WAIT).unwrap();
                        if fenced {
                            let error = call.finish_error(crate::storage::disk::RetentionError {
                                phase: crate::storage::disk::RetentionPhase::Census,
                                effect: Effect::Unchanged,
                                cause: crate::storage::disk::RetentionCause::Capacity,
                            });
                            assert_eq!(error.effect, Effect::CallerMustResolve);
                            assert!(matches!(
                                error.cause,
                                crate::storage::disk::RetentionCause::Fenced
                            ));
                            assert!(snapshot.release_checked().is_err());
                        } else {
                            call.finish_checked().unwrap();
                            snapshot.release_checked().unwrap();
                            separate.check_idle().unwrap();
                        }
                    });
                    arrived.recv_timeout(WAIT).unwrap();
                    let charge = reader.catalog_test_source_used();
                    assert!(charge > 0);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(hook::observe(core).active_read_operations, 1);
                    let (waiting, wait_arrived) = sync_channel(1);
                    let (drain_resume, drain_release) = sync_channel(1);
                    hook::install(core, Point::DrainWaiting, waiting, drain_release);
                    let controlling = threads.spawn(move || {
                        wait_arrived.recv_timeout(WAIT).unwrap();
                        let observed = hook::observe(core);
                        assert_eq!(
                            (
                                observed.installation,
                                observed.active_content,
                                observed.active_read_operations
                            ),
                            ("Draining", 0, 1)
                        );
                        assert_eq!(reader_ref.catalog_test_source_used(), charge);
                        if fenced {
                            fence.fence();
                        }
                        resume.send(()).unwrap();
                        reading.join().unwrap();
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        assert_eq!(
                            reader_ref.catalog_test_source_used(),
                            if fenced { charge } else { 0 }
                        );
                        drain_resume.send(Resume::Continue).unwrap();
                    });
                    let result = writer.commit_catalog(owner, prepared);
                    assert_eq!(result.is_err(), fenced);
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
fn returned_missing_scope_blocks_original_drain() {
    retained_missing(false);
}

#[test]
fn scope_cleanup_fatal_overrides_unchanged_error_and_returns_progress() {
    retained_missing(true);
}

#[test]
fn scope_normal_drop_and_unwind_preserve_original_cleanup_policy() {
    case("C2_SCOPE04", || {
        for unwind in [false, true] {
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
                let _ = (status, undo, private);
                prepare_catalog(live, owner, flusher);
                let (_, reader, _) = live.split();
                let snapshot = reader.catalog_snapshot(owner).unwrap();
                let mut charge = 0;
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut call = reader.catalog_resident_call(owner, &snapshot).unwrap();
                    assert!(matches!(
                        call.lookup(
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private
                        )
                        .unwrap(),
                        CatalogLookupStep::Missing
                    ));
                    charge = reader.catalog_test_source_used();
                    assert!(charge > 0);
                    assert_eq!(hook::observe(core).active_read_operations, 1);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    if unwind {
                        panic!("scope caller unwind after result consumption");
                    }
                    drop(call);
                }));
                assert_eq!(outcome.is_err(), unwind);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(
                    reader.catalog_test_source_used(),
                    if unwind { charge } else { 0 }
                );
                assert_eq!(fence.is_fenced(), unwind);
                assert_eq!(snapshot.release_checked().is_err(), unwind);
                if !unwind {
                    owner.check_idle().unwrap();
                }
                let _ = state;
            });
        }
    });
}
