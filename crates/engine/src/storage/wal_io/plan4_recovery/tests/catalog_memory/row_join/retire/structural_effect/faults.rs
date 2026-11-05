//! E02–E09 exercise the real split caller and original pool, without a test
//! authority factory. Each fixture's four rows have finished all cleanup.
use super::*;
use crate::storage::dirty_frame::{self, init_test_support as init};
use crate::storage::disk::RetentionPhase;
use crate::transaction::concurrency::catalog_test_support as hook;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::sync_channel,
    time::Duration,
};

macro_rules! ready {
    ($child:expr, |$writer:ident, $reader:ident, $flush:ident, $owner:ident,
        $flusher:ident, $private:ident, $core:ident, $fence:ident, $state:ident, $target:ident| $body:block) => {
        ready!(
            $child,
            extra_undo = 0,
            |$writer,
             $reader,
             $flush,
             $owner,
             $flusher,
             $private,
             $core,
             $fence,
             $state,
             $target| $body
        )
    };
    ($child:expr, extra_undo = $extra:expr, |$writer:ident, $reader:ident, $flush:ident, $owner:ident,
        $flusher:ident, $private:ident, $core:ident, $fence:ident, $state:ident, $target:ident| $body:block) => {{
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                live.catalog_retire(owner, 1, CatalogOperation::Prune, private)
                    .unwrap();
                for _ in 0..3 {
                    live.catalog_release_role(owner, 1, private).unwrap();
                }
                live.catalog_retire(owner, 1, CatalogOperation::RootFree, private)
                    .unwrap();
                live.catalog_retire(owner, 0, CatalogOperation::TerminalFinish, private)
                    .unwrap();
                for _ in 0..3 {
                    system_init(live, owner, flusher, true);
                }
                let extra = $extra;
                for _ in 0..extra {
                    let target = system_init(live, owner, flusher, true);
                    drive_flush(live, flusher, Some(&target));
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let _rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    view,
                    &table,
                    private,
                    [7, 8, 9, 10]
                );
                assert_eq!(
                    a_commit!(&mut writer, owner, flush, flusher, tx, private),
                    2
                );
                assert_eq!(
                    drained!(&mut writer, owner, flush, flusher, private),
                    done(21, 0)
                );
                let target = writer.catalog_test_with_pool(source);
                if $extra != 0 {
                    for at in [1, 2] {
                        let target = writer.test_structural_target_at(at);
                        flush_target!(flush, flusher, target);
                    }
                }
                if $child {
                    writer
                        .split_row(owner, object(), target, private, &mut flush)
                        .unwrap();
                    if $extra != 0 {
                        for at in [1, 2] {
                            let target = writer.test_structural_target_at(at);
                            let mut bytes = boxed::<PAGE_BYTES>();
                            let before =
                                probe::diagnose(reader.catalog_test_pool(), at, &mut bytes);
                            assert!(before.dirty && before.frame.page_lsn.0 > 0);
                            flush_target!(flush, flusher, target);
                            let after = probe::diagnose(reader.catalog_test_pool(), at, &mut bytes);
                            assert!(!after.dirty && after.entry.is_none());
                            assert_eq!(after.frame.page_lsn, before.frame.page_lsn);
                            assert_eq!(
                                after.frame.modification_counter,
                                before.frame.modification_counter
                            );
                            assert!(after.frame.rec_lsn.is_none());
                            let request = reader.catalog_test_pool().target_request(at).unwrap();
                            println!(
                                "SMO_E02 original flush {:?} level={} clean",
                                request.kind, request.level
                            );
                        }
                    }
                }
                let (
                    $writer,
                    $reader,
                    $flush,
                    $owner,
                    $flusher,
                    $private,
                    $core,
                    $fence,
                    $state,
                    $target,
                ) = (
                    &mut writer,
                    &reader,
                    &mut flush,
                    owner,
                    flusher,
                    private,
                    core,
                    fence,
                    state,
                    target,
                );
                $body
            }
        );
    }};
}

#[path = "faults/refusals.rs"]
mod refusals;

fn paused<T>(
    pool: &dirty_frame::SharedPool<'_, '_, '_, '_>,
    point: init::InitPoint,
    action: impl FnOnce() + Send,
    resume: probe::Resume,
    run: impl FnOnce() -> T,
) -> T {
    let (entered, arrival) = sync_channel(1);
    let (proceed, release) = sync_channel(1);
    init::install_init_pause(pool, point, entered, release);
    std::thread::scope(|scope| {
        let observer = scope.spawn(move || {
            arrival
                .recv_timeout(Duration::from_secs(30))
                .expect("split reached frozen cut");
            action();
            proceed.send(resume).unwrap();
        });
        let result = run();
        observer.join().unwrap();
        result
    })
}

fn released(
    pool: &dirty_frame::SharedPool<'_, '_, '_, '_>,
    core: &PageConcurrency<'_>,
    claim: bool,
) {
    assert_eq!(init::claim_state(pool), (claim, 0));
    for at in 0..32 {
        assert!(!probe::content_busy(pool, at), "content {at}");
    }
    let state = hook::observe(core);
    assert_eq!((state.active_apply, state.active_content), (0, 0));
}

fn fatal_returned(core: &PageConcurrency<'_>) {
    assert!(!core.diagnostic_version_busy());
    assert!(!hook::observe(core).read_gate_closed);
}

#[test]
fn structural_effect_stage_capacity_cancels_and_reenters_root_child() {
    for child in [false, true] {
        case_within(&format!("SMO-E04-{child}"), Duration::from_secs(90), || {
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
                let before = residents(pool);
                let generations = init::retention_generations(pool);
                let bank = writer.recovery_test_bank().to_vec();
                let jobs = writer.catalog_test_jobs().to_vec();
                let inventory = writer.test_structural_inventory_before().to_vec();
                let wal = files(state);
                let once = crate::storage::wal_io::durability_test_support::stage_capacity_once();
                let error = writer
                    .split_row(owner, object(), target, private, flush)
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Wal(e)
                    if e.effect == Effect::Unchanged && matches!(e.cause, crate::storage::wal_io::Cause::Capacity)));
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(once.hits(), 1);
                drop(once);
                assert_eq!(files(state), wal);
                assert_eq!(writer.recovery_test_bank(), bank);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(
                    writer.test_structural_inventory_before().as_slice(),
                    inventory
                );
                let after = residents(pool);
                assert_eq!(before.len(), after.len());
                for (old, new) in before.iter().zip(&after) {
                    assert_eq!(old.target, new.target);
                    assert_eq!(old.stamp, new.stamp);
                    assert_eq!(old.bytes, new.bytes);
                    assert_eq!(old.requirement, new.requirement);
                }
                let cancelled = init::retention_generations(pool);
                let consumed: Vec<_> = (1..32)
                    .filter(|at| cancelled[*at] != generations[*at])
                    .collect();
                assert_eq!(consumed.len(), if child { 1 } else { 2 });
                for at in &consumed {
                    assert_eq!(cancelled[*at], generations[*at].map(|g| g + 1));
                }
                released(pool, core, false);
                assert!(!fence.is_fenced() && !writer.test_structural_pending());
                assert_eq!(reader.catalog_test_source_used(), 0);
                owner.check_idle().unwrap();
                let split = writer
                    .split_row(owner, object(), target, private, flush)
                    .unwrap();
                assert_eq!(
                    split.operation,
                    if child {
                        Split::ChildSplit
                    } else {
                        Split::RootSplit
                    }
                );
                let reused = init::retention_generations(pool);
                for at in consumed {
                    assert_eq!(reused[at], cancelled[at].map(|g| g + 1));
                }
                released(pool, core, false);
                assert!(!fence.is_fenced() && writer.test_structural_replay_returned());
                println!(
                    "SMO_E04 child={child} original rejection/cancel; tombstones, stale-key refusal and reentry"
                );
            });
        });
    }
}

#[test]
fn structural_effect_unwind_at_install_publication_and_finish_boundaries() {
    use init::InitPoint::*;
    // E05–E09. The first and last points also cover unresolved claim/key Drop.
    const POINTS: [init::InitPoint; 14] = [
        StructuralClaimed,
        StructuralStageEntered,
        StructuralStaged,
        StructuralFirstImage,
        StructuralImages,
        StructuralControl,
        StructuralDpt,
        StructuralPublished,
        StructuralHistoryInstalled,
        StructuralReleased,
        StructuralDurable,
        StructuralReplayReturning,
        StructuralReplayChecked,
        StructuralFinishChecked,
    ];
    for child in [false, true] {
        let points: Vec<_> = POINTS
            .into_iter()
            .chain(
                (!child)
                    .then_some([PairFirstRegistered, PairBothRegisteredBeforeGate])
                    .into_iter()
                    .flatten(),
            )
            .collect();
        for point in points {
            case_within(
                &format!("SMO-E05-E09-{child}-{point:?}"),
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
                        let _ = (flusher, state);
                        let pool = reader.catalog_test_pool();
                        let result = paused(
                            pool,
                            point,
                            || {},
                            probe::Resume::Panic,
                            || {
                                catch_unwind(AssertUnwindSafe(|| {
                                    writer.split_row(owner, object(), target, private, flush)
                                }))
                            },
                        );
                        assert!(result.is_err(), "{point:?}");
                        assert!(fence.is_fenced());
                        released(pool, core, true);
                        // Fatal accounting remains reserved under the original fence (§6.9).
                        assert_eq!(reader.catalog_test_source_used(), 44_304);
                        fatal_returned(core);
                        assert!(writer.test_structural_replay_returned());
                        assert_eq!(
                            writer.test_structural_pending(),
                            !matches!(point, StructuralClaimed | StructuralStageEntered)
                        );
                        println!(
                            "SMO_FAULT child={child} point={point:?} fenced; claim/source charge retained, guards/pins returned"
                        );
                    });
                },
            );
        }
    }
}

#[test]
fn structural_effect_replay_checked_return_failure_keeps_pending() {
    for child in [false, true] {
        case_within(
            &format!("SMO-E09-return-{child}"),
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
                    let _ = (flusher, state);
                    let pool = reader.catalog_test_pool();
                    let result = paused(
                        pool,
                        init::InitPoint::StructuralReplayReturning,
                        || fence.fence(),
                        probe::Resume::Continue,
                        || writer.split_row(owner, object(), target, private, flush),
                    );
                    assert_eq!(result.err().unwrap().effect, Effect::CallerMustResolve);
                    assert!(fence.is_fenced() && writer.test_structural_pending());
                    assert!(writer.test_structural_replay_returned());
                    // Fatal accounting remains reserved under the original fence (§6.9).
                    assert_eq!(reader.catalog_test_source_used(), 44_304);
                    fatal_returned(core);
                    released(pool, core, true);
                });
            },
        );
    }
}

#[test]
fn structural_effect_guardless_durable_wait_blocks_ordinary_paths_and_io_failure() {
    for child in [false, true] {
        for action in [port::Action::BeforeError, port::Action::AfterError] {
            case_within(
                &format!("SMO-E08-{child}-{action:?}"),
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
                        // Return the idle dedicated owner so the observer can
                        // acquire that same original role on its own thread.
                        drop(std::mem::replace(
                            flusher,
                            core.try_owner(OwnerRole::Maintenance).unwrap(),
                        ));
                        let pool = reader.catalog_test_pool();
                        let request = pool
                            .target_request(probe::registered_index(pool, target.page_no).unwrap())
                            .unwrap();
                        let result = paused(
                            pool,
                            init::InitPoint::StructuralReleased,
                            || {
                                released(pool, core, true);
                                let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                                assert!(matches!(pool.reserve_target_set(&mut other, &[request]),
                            Err(e) if e.effect == Effect::Unchanged && e.cause == dirty_frame::Cause::Busy));
                                assert!(
                                    matches!(pool.claim_retention_load(&mut other, request, 1, &[]),
                            Err(e) if e.effect == Effect::Unchanged && e.cause == dirty_frame::Cause::Busy)
                                );
                                let mut output = [0; PAGE_BYTES];
                                let mut flush_owner =
                                    core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                                assert!(
                                    matches!(pool.capture_flush(&mut flush_owner, request, &mut output),
                            Err(e) if e.effect == Effect::Unchanged && e.cause == dirty_frame::Cause::Busy)
                                );
                                let mut state = state.lock().unwrap();
                                state.events.clear();
                                state.fault_hits = 0;
                                state.fault = Some(port::Fault {
                                    key: port::Key::new("sync", "d3-wal-00000001", 0, 1),
                                    action,
                                });
                            },
                            probe::Resume::Continue,
                            || writer.split_row(owner, object(), target, private, flush),
                        );
                        let _ = flusher;
                        let error = result.err().unwrap();
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Durable, Effect::CallerMustResolve)
                        );
                        assert!(matches!(error.cause, RetentionCause::Wal(_)));
                        assert_eq!(state.lock().unwrap().fault_hits, 1);
                        assert!(fence.is_fenced() && writer.test_structural_pending());
                        // Fatal accounting remains reserved under the original fence (§6.9).
                        assert_eq!(reader.catalog_test_source_used(), 44_304);
                        fatal_returned(core);
                        released(pool, core, true);
                    });
                },
            );
        }
    }
}

#[test]
fn structural_effect_durable_write_failure_keeps_pending_and_fences() {
    for child in [false, true] {
        for action in [
            port::Action::BeforeError,
            port::Action::PrefixErrorBytes(96),
        ] {
            case_within(
                &format!("SMO-E08-write-{child}-{action:?}"),
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
                        let offset = writer.test_structural_prefix().complete_offset;
                        let wal_before = files(state);
                        let pool = reader.catalog_test_pool();
                        let result = paused(
                            pool,
                            init::InitPoint::StructuralReleased,
                            || {
                                released(pool, core, true);
                                let mut state = state.lock().unwrap();
                                state.events.clear();
                                state.fault_hits = 0;
                                state.fault = Some(port::Fault {
                                    key: port::Key::new("write", "d3-wal-00000001", offset, 1),
                                    action,
                                });
                            },
                            probe::Resume::Continue,
                            || writer.split_row(owner, object(), target, private, flush),
                        );
                        let error = result.err().unwrap();
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Durable, Effect::CallerMustResolve)
                        );
                        assert!(matches!(error.cause, RetentionCause::Wal(_)));
                        assert_eq!(state.lock().unwrap().fault_hits, 1);
                        let wal_after = files(state);
                        let expected_extra = if action == port::Action::BeforeError {
                            0
                        } else {
                            96
                        };
                        assert_eq!(wal_after.0, wal_before.0 + expected_extra);
                        for (name, bytes) in &wal_before.1 {
                            let after = &wal_after.1[name];
                            assert_eq!(
                                &after[..bytes.len()],
                                bytes,
                                "original durable prefix {name}"
                            );
                            if name != "d3-wal-00000001" {
                                assert_eq!(after, bytes);
                            }
                        }
                        assert!(fence.is_fenced() && writer.test_structural_pending());
                        assert_eq!(reader.catalog_test_source_used(), 44_304);
                        fatal_returned(core);
                        released(pool, core, true);
                    });
                },
            );
        }
    }
}
