//! Actual Commit against the original flusher and an admitted content guard.
//! Expected pages are frozen C04 images; hooks only pause original producers.
use super::super::super::super::plan4_pages::install_after_capture;
use super::*;
use crate::{
    storage::{
        dirty_frame::{init_test_support as raw, test_support as flush_probe},
        disk::RetentionCause,
        page::PageLsn,
    },
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::{
    sync::{Mutex, mpsc::sync_channel},
    time::Instant,
};

/// Completes one original flush step sequence without retrying a failure.
macro_rules! finish {
    ($flush:expr, $owner:expr, $step:expr) => {{
        let mut step = $step;
        for _ in 0..8 {
            if !matches!(
                step,
                RetentionFlushStep::WaitingWal
                    | RetentionFlushStep::WaitingSource
                    | RetentionFlushStep::RetiredPendingFinalize
            ) {
                break;
            }
            step = $flush.continue_flush($owner).unwrap();
        }
        step
    }};
}

#[test]
fn catalog_mt05_pending_lower_flush() {
    case("MT05", || {
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
            // (page, C04 image role, original target)
            let targets = [
                (1usize, 1usize, live.target(Plan4Page::Root).unwrap()),
                (2, 2, live.target(Plan4Page::Jobs { index: 0 }).unwrap()),
                (9, 4, *status),
            ];
            let mut originals = Vec::new();
            for (number, _, _) in &targets {
                let index = resident_index(live, *number as u64).unwrap();
                let mut bytes = boxed::<PAGE_BYTES>();
                let observed = raw::snapshot(live.catalog_test_pool(), index, &mut bytes);
                assert!(
                    observed.dirty && observed.rec_lsn.is_some(),
                    "page {number} lower dirty image"
                );
                originals.push((index, observed));
            }
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::Pending, entered, release);
            // The sole dedicated flusher Owner stays on this thread. Commit
            // prepares and installs with its own Owner on a separate thread.
            std::thread::scope(|scope| {
                let (lend, lent) = sync_channel(1);
                let (core_ref, live_ref, private_ref) = (core, &mut *live, &mut *private);
                let committing = scope.spawn(move || {
                    let mut committer = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                    let prepared = live_ref
                        .prepare_catalog_commit(&mut committer, actor, private_ref)
                        .unwrap();
                    let (mut writer, _, flush) = live_ref.split();
                    lend.send(flush).unwrap();
                    writer.commit_catalog(&mut committer, prepared).unwrap();
                    committer.check_idle().unwrap();
                });
                let mut flush = lent.recv_timeout(WAIT).unwrap();
                arrived.recv_timeout(WAIT).unwrap();
                let observation = hook::observe(core);
                assert_eq!(
                    (
                        observation.installation,
                        observation.active_apply,
                        observation.active_content
                    ),
                    ("Pending", 0, 0)
                );
                for (number, _, target) in &targets {
                    let step = flush.try_flush(flusher, target).unwrap();
                    match finish!(flush, flusher, step) {
                        RetentionFlushStep::Complete { finalized, .. } => {
                            assert!(finalized.cleaned, "page {number} cleaned")
                        }
                        other => panic!("page {number} lower flush {other:?}"),
                    }
                }
                let pool = flush.catalog_test_pool();
                let home = state.lock().unwrap().files[HOME_NAME].bytes.clone();
                for ((number, role, _), (index, original)) in targets.iter().zip(&originals) {
                    let mut bytes = boxed::<PAGE_BYTES>();
                    let observed = raw::snapshot(pool, *index, &mut bytes);
                    assert_eq!(
                        (observed.dirty, observed.rec_lsn, observed.entry),
                        (false, None, None),
                        "page {number} old recLSN Some -> None"
                    );
                    assert_eq!(observed.page_lsn, original.page_lsn);
                    assert_eq!(observed.pins[*index], 1, "Pending keeps its pin");
                    assert_eq!(observed.total, 4, "Pending keeps exactly four pins");
                    let before = oracle.transition.raw(&oracle.row("C04")[23 + role]);
                    assert_eq!(bytes.as_slice(), before);
                    let at = number * PAGE_BYTES;
                    assert_eq!(&home[at..at + PAGE_BYTES], before.as_slice());
                }
                flusher.check_idle().unwrap();
                resume.send(Resume::Continue).unwrap();
                committing.join().unwrap();
            });
            oracle.check(live, state, "C04");
            assert_eq!(live.catalog_test_q(), 1);
            let home = state.lock().unwrap().files[HOME_NAME].bytes.clone();
            for ((number, role, _), (index, original)) in targets.iter().zip(&originals) {
                let mut bytes = boxed::<PAGE_BYTES>();
                let observed = raw::snapshot(live.catalog_test_pool(), *index, &mut bytes);
                assert_eq!(observed.page_lsn, PageLsn(9));
                assert!(observed.dirty);
                assert_eq!(
                    observed.rec_lsn,
                    Some(PageLsn(9)),
                    "page {number} re-resolved recLSN, not the pre-Pending stamp"
                );
                assert_ne!(observed.rec_lsn, original.rec_lsn);
                assert_eq!(observed.entry.map(|entry| entry.rec_lsn), Some(PageLsn(9)));
                assert_eq!(observed.requirement.unwrap().lsn, 9);
                // Physical home keeps the flushed old image; RAM is the Commit after.
                let before = oracle.transition.raw(&oracle.row("C04")[23 + role]);
                let at = number * PAGE_BYTES;
                assert_eq!(&home[at..at + PAGE_BYTES], before.as_slice());
            }
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn catalog_mt06_flush_capture_then_commit() {
    case("MT06", || {
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
            let index = resident_index(live, 1).unwrap();
            let mut bytes = boxed::<PAGE_BYTES>();
            let original = raw::snapshot(live.catalog_test_pool(), index, &mut bytes);
            assert!(original.dirty);
            let (flush_entered, flush_arrived) = sync_channel(1);
            let (flush_resume, flush_release) = sync_channel(1);
            install_after_capture(live.catalog_test_pool(), flush_entered, flush_release);
            let (commit_entered, commit_arrived) = sync_channel(1);
            let (commit_resume, commit_release) = sync_channel(1);
            hook::install(core, Point::Durable, commit_entered, commit_release);
            let events = Mutex::new(Vec::new());
            let (go, start) = sync_channel(1);
            let installed = std::thread::scope(|scope| {
                let (lend, lent) = sync_channel(1);
                let (core_ref, live_ref, private_ref, events_ref) =
                    (core, &mut *live, &mut *private, &events);
                let committing = scope.spawn(move || {
                    let mut committer = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                    let prepared = live_ref
                        .prepare_catalog_commit(&mut committer, actor, private_ref)
                        .unwrap();
                    let (mut writer, _, flush) = live_ref.split();
                    lend.send(flush).unwrap();
                    start.recv_timeout(WAIT).unwrap();
                    let pool = writer.catalog_test_pool();
                    assert!(flush_probe::flush_diagnose(pool).claim.is_some());
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    // Drain must not wait for the flusher's I/O claim.
                    writer.commit_catalog(&mut committer, prepared).unwrap();
                    events_ref.lock().unwrap().push("commit_returned");
                    let pool = writer.catalog_test_pool();
                    assert!(flush_probe::flush_diagnose(pool).claim.is_some());
                    let mut bytes = boxed::<PAGE_BYTES>();
                    let installed = raw::snapshot(pool, index, &mut bytes);
                    assert_eq!(installed.page_lsn, PageLsn(9));
                    assert_eq!(installed.requirement.unwrap().lsn, 9);
                    committer.check_idle().unwrap();
                    flush_resume.send(flush_probe::Resume::Continue).unwrap();
                    installed
                });
                let controlling = scope.spawn(move || {
                    flush_arrived.recv_timeout(WAIT).unwrap();
                    events_ref.lock().unwrap().push("flush_captured");
                    go.send(()).unwrap();
                    commit_arrived.recv_timeout(WAIT).unwrap();
                    events_ref.lock().unwrap().push("commit_durable");
                    commit_resume.send(Resume::Continue).unwrap();
                });
                // This thread keeps the sole dedicated flusher Owner.
                let mut flush = lent.recv_timeout(WAIT).unwrap();
                let step = flush.try_flush(flusher, &root).unwrap();
                let step = finish!(flush, flusher, step);
                events.lock().unwrap().push("flush_finished");
                assert!(
                    flush_probe::flush_diagnose(flush.catalog_test_pool())
                        .claim
                        .is_none(),
                    "old flush returns only its own claim"
                );
                flusher.check_idle().unwrap();
                controlling.join().unwrap();
                let installed = committing.join().unwrap();
                match step {
                    RetentionFlushStep::Complete {
                        captured,
                        finalized,
                    } => {
                        assert_eq!(captured.page_lsn, original.page_lsn);
                        assert!(!finalized.cleaned, "counter changed by actual Commit");
                        assert_eq!(finalized.frame.page_lsn, PageLsn(9));
                        assert_eq!(finalized.frame.rec_lsn, installed.rec_lsn);
                    }
                    other => panic!("old flush after Commit install {other:?}"),
                }
                installed
            });
            assert_eq!(
                events.into_inner().unwrap(),
                [
                    "flush_captured",
                    "commit_durable",
                    "commit_returned",
                    "flush_finished"
                ]
            );
            oracle.check(live, state, "C04");
            assert_eq!(live.catalog_test_q(), 1);
            let mut bytes = boxed::<PAGE_BYTES>();
            let observed = raw::snapshot(live.catalog_test_pool(), index, &mut bytes);
            assert!(observed.dirty, "new dirty preserved");
            assert_eq!(
                (observed.page_lsn, observed.rec_lsn, observed.entry),
                (installed.page_lsn, installed.rec_lsn, installed.entry)
            );
            assert_eq!(observed.requirement.unwrap().lsn, 9);
            let home = state.lock().unwrap().files[HOME_NAME].bytes.clone();
            let before = oracle.transition.raw(&oracle.row("C04")[24]);
            assert_eq!(&home[PAGE_BYTES..2 * PAGE_BYTES], before.as_slice());
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn catalog_mt07_drain_content_guard_fence() {
    case("MT07", || {
        let oracle = Oracle::new();
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
            // Status is an actual Commit target whose read guard decodes the
            // current home format; read() rejects the v2 Root page.
            let target = *status;
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            let (done, finished) = sync_channel(1);
            hook::install(core, Point::Pending, entered, release);
            {
                let (mut writer, reader, _) = live.split();
                std::thread::scope(|scope| {
                    let (core_ref, target) = (core, &target);
                    let holding = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        let mut holder = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                        // An admitted content guard taken while Commit is Pending.
                        let guard = reader.read(&mut holder, target).unwrap();
                        assert_eq!(hook::observe(core_ref).active_content, 1);
                        resume.send(Resume::Continue).unwrap();
                        let deadline = Instant::now() + WAIT;
                        while hook::observe(core_ref).installation != "Draining" {
                            assert!(Instant::now() < deadline, "bounded Drain entry");
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        let mut late = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                        let error = match reader.read(&mut late, target) {
                            Err(error) => error,
                            Ok(_) => panic!("new reader admitted during Drain"),
                        };
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(
                            format!("{error:?}").contains("InstallationPending"),
                            "{error:?}"
                        );
                        late.check_idle().unwrap();
                        let observation = hook::observe(core_ref);
                        assert_eq!(
                            (
                                observation.installation,
                                observation.active_apply,
                                observation.active_content
                            ),
                            ("Draining", 0, 1)
                        );
                        fence.fence();
                        finished.recv_timeout(WAIT).unwrap();
                        let _ = guard.release_checked();
                    });
                    let error = match writer.commit_catalog(owner, prepared) {
                        Err(error) => error,
                        Ok(_) => panic!("externally fenced Drain must not ACK"),
                    };
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    done.send(()).unwrap();
                    holding.join().unwrap();
                });
            }
            assert!(fence.is_fenced());
            assert_eq!(live.catalog_test_q(), 0);
            let (prefix, first_write) = live.catalog_test_wal_state();
            assert_eq!(
                (prefix.complete_lsn, prefix.synced_lsn, first_write),
                (9, 9, 9)
            );
            let mut frames: Vec<_> = (1..=5).map(|n| oracle.native_frame(n)).collect();
            for at in 1..=4 {
                frames.push(oracle.frame(&format!("C{at:02}")));
            }
            assert_wal(state, 1, &frames);
            let observation = hook::observe(core);
            assert_eq!(
                (
                    observation.installation,
                    observation.active_apply,
                    observation.active_content
                ),
                ("Abandoned", 0, 0)
            );
            state.lock().unwrap().images()
        });
        recover_check(&images, &oracle, "N03");
    });
}

#[test]
fn catalog_mt03_pending_reader_loads_undo() {
    case("MT03", || {
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
            // Actual clean victim replacement: flush U, then load a real Jobs
            // page into that same original frame. No directory is rewritten.
            drive_flush(live, flusher, Some(undo));
            let index = match live
                .residency_step(undo, &mut Plan4ResidencyCursor::new())
                .unwrap()
            {
                RetentionResidencyStep::Resident {
                    index,
                    loaded: false,
                    frame,
                } => {
                    assert!(frame.rec_lsn.is_none());
                    index
                }
                _ => panic!("flushed Undo must keep its clean registration"),
            };
            let jobs = live.target(Plan4Page::Jobs { index: 1 }).unwrap();
            let evicted = live.load_into(index, &jobs).unwrap();
            assert!(evicted.before.is_some(), "a real clean Undo is evicted");
            assert_eq!(resident_index(live, 10), None);
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::Pending, entered, release);
            {
                let (mut writer, reader, flush) = live.split();
                std::thread::scope(|scope| {
                    let core_ref = core;
                    let (registered, ready) = sync_channel(1);
                    let reading = scope.spawn(move || {
                        // The old snapshot registers before Commit; a new
                        // registration needs apply admission, closed in Pending.
                        let mut separate = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader.catalog_snapshot(&mut separate).unwrap();
                        assert_eq!(snapshot.q(), 0);
                        registered.send(()).unwrap();
                        arrived.recv_timeout(WAIT).unwrap();
                        let observation = hook::observe(core_ref);
                        assert_eq!(
                            (
                                observation.installation,
                                observation.active_apply,
                                observation.active_content
                            ),
                            ("Pending", 0, 0)
                        );
                        let pool = flush.catalog_test_pool();
                        assert_eq!(raw::claim_state(pool).1, 4, "Pending keeps four pins");
                        assert_eq!(flush_probe::registered_index(pool, 10), None);
                        state.lock().unwrap().events.clear();
                        // One actual lookup performs the LOAD into the clean
                        // victim; a dirty victim would be a failed precondition.
                        match reader
                            .lookup_catalog(
                                &mut separate,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                            .unwrap()
                        {
                            CatalogLookupStep::Missing => {}
                            CatalogLookupStep::Found(_) => {
                                panic!("old snapshot must not see uncommitted T")
                            }
                            CatalogLookupStep::FlushRequired { target } => {
                                panic!("prepared clean victim, got FlushRequired {target:?}")
                            }
                        }
                        let pool = flush.catalog_test_pool();
                        assert!(
                            flush_probe::registered_index(pool, 10).is_some(),
                            "actual LOAD made U resident"
                        );
                        assert!(
                            state.lock().unwrap().events.iter().any(|event| {
                                event.op == "read"
                                    && event.name == HOME_NAME
                                    && event.offset == 10 * PAGE_BYTES as u64
                            }),
                            "actual home read I/O for U"
                        );
                        assert_eq!(raw::claim_state(pool).1, 4, "reader returned its pins");
                        snapshot.release_checked().unwrap();
                        separate.check_idle().unwrap();
                        resume.send(Resume::Continue).unwrap();
                    });
                    ready.recv_timeout(WAIT).unwrap();
                    writer.commit_catalog(owner, prepared).unwrap();
                    reading.join().unwrap();
                });
            }
            oracle.check(live, state, "C04");
            assert_eq!(live.catalog_test_q(), 1);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

/// Exercise the gap after Catalog capture: no physical content admission is
/// alive, but the real operation still owns its Txn/Undo references. The read
/// is admitted while Commit is Pending; a read already admitted when Commit
/// starts is instead waited for by the terminal read gate before stage.
fn drain_captured_lookup(fence_during_wait: bool) {
    case(
        if fence_during_wait {
            "MCRD02"
        } else {
            "MCRD01"
        },
        || {
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
                let (pending_entered, pending_arrived) = sync_channel(1);
                let (pending_resume, pending_release) = sync_channel(1);
                hook::install(core, Point::Pending, pending_entered, pending_release);
                {
                    let (mut writer, reader, maintenance) = live.split();
                    let pending_charge = writer.catalog_test_commit_charges()[2] as u64;
                    let pool = maintenance.catalog_test_pool();
                    let full_charge = reader.catalog_test_fixed_charges()[1] as u64;
                    let late_snapshot = reader.catalog_snapshot(owner).unwrap();
                    let reader_ref = &reader;
                    let (registered, ready) = sync_channel(1);
                    let (start, started) = sync_channel(1);
                    std::thread::scope(|scope| {
                        let reading = scope.spawn(move || {
                            let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                            // Registration needs apply admission, closed in Pending.
                            let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                            registered.send(()).unwrap();
                            started.recv_timeout(WAIT).unwrap();
                            let error = match reader_ref.lookup_catalog(
                                &mut separate,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            ) {
                                Err(error) => error,
                                Ok(_) => panic!("captured operation must stop at Drain"),
                            };
                            assert_eq!(
                                error.effect,
                                if fence_during_wait {
                                    Effect::CallerMustResolve
                                } else {
                                    Effect::Unchanged
                                }
                            );
                            if !fence_during_wait {
                                assert!(
                                    format!("{error:?}").contains("InstallationPending"),
                                    "{error:?}"
                                );
                            }
                            assert_eq!(hook::observe(core).active_read_operations, 0);
                            assert_eq!(
                                reader_ref.catalog_test_references(),
                                ((if fence_during_wait { 2 } else { 0 }, 0, 0), false)
                            );
                            let cleanup = snapshot.release_checked();
                            assert_eq!(cleanup.is_err(), fence_during_wait);
                        });
                        ready.recv_timeout(WAIT).unwrap();
                        let controlling = scope.spawn(move || {
                        pending_arrived.recv_timeout(WAIT).unwrap();
                        let pending = hook::observe(core);
                        assert_eq!((pending.installation, pending.read_gate_closed),
                            ("Pending", false));
                        assert_eq!(reader_ref.catalog_test_source_used(), pending_charge);
                        let (captured, capture_arrived) = sync_channel(1);
                        let (read_resume, read_release) = sync_channel(1);
                        hook::install(core, Point::LookupCaptured, captured, read_release);
                        start.send(()).unwrap();
                        capture_arrived.recv_timeout(WAIT).unwrap();
                        let captured = hook::observe(core);
                        assert_eq!((captured.active_apply, captured.active_content,
                            captured.active_read_operations), (0, 0, 1));
                        assert_eq!(reader_ref.catalog_test_references(), ((2, 0, 0), false));
                        // Only the Pending writer's four pins; the captured read holds none.
                        assert_eq!(raw::claim_state(pool).1, 4);
                        // The Pending writer and the admitted Full read share one total.
                        assert_eq!(reader_ref.catalog_test_source_used(),
                            pending_charge + full_charge);
                        let (waiting, wait_arrived) = sync_channel(1);
                        let (drain_resume, drain_release) = sync_channel(1);
                        hook::install(core, Point::DrainWaiting, waiting, drain_release);
                        pending_resume.send(Resume::Continue).unwrap();
                        wait_arrived.recv_timeout(WAIT).unwrap();
                        let waiting = hook::observe(core);
                        assert_eq!((waiting.installation, waiting.active_apply,
                            waiting.active_content, waiting.active_read_operations),
                            ("Draining", 0, 0, 1));
                        // The old snapshot can outlive the finite operation. A
                        // fresh lookup must fail before reserving source refs.
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let mut late_private = boxed::<PRIVATE_BYTES>();
                        let error = match reader_ref.lookup_catalog(&mut late, &late_snapshot,
                            ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(), &mut late_private) {
                            Err(error) => error,
                            Ok(_) => panic!("late operation admitted during Drain"),
                        };
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                            if error.cause == crate::transaction::concurrency::Cause::InstallationPending));
                        assert_eq!(hook::observe(core).active_read_operations, 1);
                        assert_eq!(reader_ref.catalog_test_references(), ((2, 0, 0), false));
                        if fence_during_wait { fence.fence(); }
                        // Keep Commit paused while cleanup is independently
                        // observed, then release its original wait loop.
                        read_resume.send(Resume::Continue).unwrap();
                        reading.join().unwrap();
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        assert_eq!(reader_ref.catalog_test_source_used(),
                            pending_charge + if fence_during_wait { full_charge } else { 0 });
                        let cleanup = late_snapshot.release_checked();
                        assert_eq!(cleanup.is_err(), fence_during_wait);
                        drain_resume.send(Resume::Continue).unwrap();
                    });
                        let committed = writer.commit_catalog(owner, prepared);
                        if fence_during_wait {
                            assert_eq!(committed.err().unwrap().effect, Effect::CallerMustResolve);
                        } else {
                            committed.unwrap();
                            assert_eq!(writer.catalog_test_source_used(), 0);
                        }
                        controlling.join().unwrap();
                    });
                }
                let complete = hook::observe(core);
                assert_eq!(
                    (
                        complete.active_apply,
                        complete.active_content,
                        complete.active_read_operations
                    ),
                    (0, 0, 0)
                );
                assert!(!complete.read_gate_closed);
                assert_eq!(
                    complete.installation,
                    if fence_during_wait {
                        "Abandoned"
                    } else {
                        "Open"
                    }
                );
                assert_eq!(
                    live.catalog_test_references(),
                    ((if fence_during_wait { 2 } else { 0 }, 0, 0), false)
                );
                assert_eq!(live.catalog_test_q(), u64::from(!fence_during_wait));
                assert_eq!(fence.is_fenced(), fence_during_wait);
                if !fence_during_wait {
                    oracle.check(live, state, "C04");
                }
            });
        },
    );
}

#[test]
fn catalog_read_operation_drain_waits_for_captured_refs() {
    drain_captured_lookup(false);
}

#[test]
fn catalog_read_operation_fenced_drain_preserves_fatal_refs() {
    drain_captured_lookup(true);
}

/// A read admitted before Commit keeps its loan until it returns; the terminal
/// read gate rejects new reads, waits, and only then reserves its stage window.
/// The writer charge shrinks for Pending, where reads overlap, and grows into
/// the install window only after the original Drain.
#[test]
fn catalog_commit_read_gate_and_writer_windows() {
    case("MCW01", || {
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
            let (captured, capture_arrived) = sync_channel(1);
            let (read_resume, read_release) = sync_channel(1);
            hook::install(core, Point::LookupCaptured, captured, read_release);
            {
                let (mut writer, reader, _) = live.split();
                let [_, stage, pending, install, publish] = writer
                    .catalog_test_commit_charges()
                    .map(|charge| charge as u64);
                let full = reader.catalog_test_fixed_charges()[1] as u64;
                let late_snapshot = reader.catalog_snapshot(owner).unwrap();
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        let mut separate = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut separate).unwrap();
                        // Admitted before Commit: it completes normally.
                        assert!(matches!(
                            reader_ref
                                .lookup_catalog(
                                    &mut separate,
                                    &snapshot,
                                    ObjectId::new(6).unwrap(),
                                    &mut Plan4ResidencyCursor::new(),
                                    reader_private,
                                )
                                .unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        snapshot.release_checked().unwrap();
                        separate.check_idle().unwrap();
                    });
                    capture_arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(hook::observe(core).active_read_operations, 1);
                    assert_eq!(reader.catalog_test_source_used(), full);
                    let (gate_entered, gate_arrived) = sync_channel(1);
                    let (gate_resume, gate_release) = sync_channel(1);
                    hook::install(core, Point::ReadGateWaiting, gate_entered, gate_release);
                    let controlling = scope.spawn(move || {
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let mut late_private = boxed::<PRIVATE_BYTES>();
                        let rejected = |error: crate::storage::disk::RetentionError| {
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                                if error.cause
                                    == crate::transaction::concurrency::Cause::InstallationPending));
                        };
                        gate_arrived.recv_timeout(WAIT).unwrap();
                        let waiting = hook::observe(core);
                        assert_eq!(
                            (waiting.installation, waiting.read_gate_closed,
                                waiting.active_read_operations),
                            ("Open", true, 1)
                        );
                        // No writer charge exists before admitted reads return.
                        assert_eq!(reader_ref.catalog_test_source_used(), full);
                        rejected(reader_ref.lookup_catalog(
                            &mut late,
                            &late_snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            &mut late_private,
                        ).err().unwrap());
                        assert_eq!(reader_ref.catalog_test_source_used(), full);
                        let (stage_entered, stage_arrived) = sync_channel(1);
                        let (stage_resume, stage_release) = sync_channel(1);
                        hook::install(core, Point::StageCallStarted, stage_entered, stage_release);
                        gate_resume.send(Resume::Continue).unwrap();
                        read_resume.send(Resume::Continue).unwrap();
                        reading.join().unwrap();
                        stage_arrived.recv_timeout(WAIT).unwrap();
                        let staging = hook::observe(core);
                        assert_eq!(
                            (staging.installation, staging.read_gate_closed,
                                staging.active_read_operations),
                            ("Intent", true, 0)
                        );
                        assert_eq!(reader_ref.catalog_test_source_used(), stage);
                        rejected(reader_ref.lookup_catalog(
                            &mut late,
                            &late_snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            &mut late_private,
                        ).err().unwrap());
                        let (pending_entered, pending_arrived) = sync_channel(1);
                        let (pending_resume, pending_release) = sync_channel(1);
                        hook::install(core, Point::Pending, pending_entered, pending_release);
                        stage_resume.send(Resume::Continue).unwrap();
                        pending_arrived.recv_timeout(WAIT).unwrap();
                        let pending_state = hook::observe(core);
                        assert_eq!(
                            (pending_state.installation, pending_state.read_gate_closed),
                            ("Pending", false)
                        );
                        assert_eq!(reader_ref.catalog_test_source_used(), pending);
                        // A Full read is admitted beside the Pending writer.
                        assert!(matches!(
                            reader_ref.lookup_catalog(
                            &mut late,
                            &late_snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            &mut late_private,
                        ).unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        assert_eq!(reader_ref.catalog_test_source_used(), pending);
                        let (drained_entered, drained_arrived) = sync_channel(1);
                        let (drained_resume, drained_release) = sync_channel(1);
                        hook::install(
                            core,
                            Point::DrainedBeforeResume,
                            drained_entered,
                            drained_release,
                        );
                        pending_resume.send(Resume::Continue).unwrap();
                        drained_arrived.recv_timeout(WAIT).unwrap();
                        assert_eq!(hook::observe(core).installation, "Installing");
                        assert_eq!(reader_ref.catalog_test_source_used(), install);
                        let (publish_entered, publish_arrived) = sync_channel(1);
                        let (publish_resume, publish_release) = sync_channel(1);
                        hook::install(core, Point::BeforePublish, publish_entered, publish_release);
                        drained_resume.send(Resume::Continue).unwrap();
                        publish_arrived.recv_timeout(WAIT).unwrap();
                        assert_eq!(hook::observe(core).installation, "Open");
                        assert_eq!(reader_ref.catalog_test_source_used(), publish);
                        publish_resume.send(Resume::Continue).unwrap();
                        late.check_idle().unwrap();
                        late_snapshot.release_checked().unwrap();
                    });
                    writer.catalog_commit(owner, actor, private).unwrap();
                    controlling.join().unwrap();
                    assert_eq!(writer.catalog_test_source_used(), 0);
                });
                let complete = hook::observe(core);
                assert_eq!(
                    (
                        complete.installation,
                        complete.read_gate_closed,
                        complete.active_read_operations
                    ),
                    ("Open", false, 0)
                );
            }
            assert_eq!(live.catalog_test_q(), 1);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            oracle.check(live, state, "C04");
        });
    });
}

/// A failed stage reservation is an Unchanged Capacity before any effect; the
/// read gate is reopened by a checked cleanup and a fresh Commit succeeds.
#[test]
fn catalog_commit_reservation_capacity_reopens_reads() {
    case("MCW02", || {
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
            let before = state.lock().unwrap().images();
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            {
                let (mut writer, reader, _) = live.split();
                let [shared, stage, ..] = writer.catalog_test_commit_charges();
                let held = 65536 - shared - stage + 1;
                let error = reader.catalog_test_with_held_source(held, || {
                    let error = writer.commit_catalog(owner, prepared).err().unwrap();
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
                let observed = hook::observe(core);
                assert_eq!(
                    (
                        observed.installation,
                        observed.read_gate_closed,
                        observed.active_apply,
                        observed.active_read_operations
                    ),
                    ("Open", false, 0, 0)
                );
            }
            owner.check_idle().unwrap();
            assert!(!fence.is_fenced());
            assert_eq!(live.catalog_test_q(), 0);
            assert_eq!(state.lock().unwrap().images(), before);
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            live.split().0.commit_catalog(owner, prepared).unwrap();
            assert_eq!(live.catalog_test_q(), 1);
            oracle.check(live, state, "C04");
        });
    });
}

#[test]
fn catalog_full_source_capacity_precedes_second_operation_effects() {
    case("C2_SHORT01", || {
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
            let before = state.lock().unwrap().images();
            let (captured, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::LookupCaptured, captured, release);
            let (writer, reader, _) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
            let reader_ref = &reader;
            std::thread::scope(|scope| {
                let reading = scope.spawn(move || {
                    let mut first = core.try_owner(OwnerRole::Foreground).unwrap();
                    let snapshot = reader_ref.catalog_snapshot(&mut first).unwrap();
                    assert!(matches!(
                        reader_ref
                            .lookup_catalog(
                                &mut first,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private
                            )
                            .unwrap(),
                        CatalogLookupStep::Missing
                    ));
                    snapshot.release_checked().unwrap();
                });
                arrived.recv_timeout(WAIT).unwrap();
                let charged = reader.catalog_test_source_used();
                assert!(charged > 0);
                let refs = reader.catalog_test_references();
                assert_eq!(hook::observe(core).active_read_operations, 1);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                let mut second_private = boxed::<PRIVATE_BYTES>();
                let error = reader
                    .lookup_catalog(
                        owner,
                        &snapshot,
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        &mut second_private,
                    )
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Capacity));
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(reader.catalog_test_source_used(), charged);
                assert_eq!(reader.catalog_test_references(), refs);
                assert_eq!(hook::observe(core).active_read_operations, 1);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(state.lock().unwrap().images(), before);
                resume.send(Resume::Continue).unwrap();
                reading.join().unwrap();
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert!(matches!(
                    reader
                        .lookup_catalog(
                            owner,
                            &snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            &mut second_private
                        )
                        .unwrap(),
                    CatalogLookupStep::Missing
                ));
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            });
            snapshot.release_checked().unwrap();
            assert_eq!(state.lock().unwrap().images(), before);
        });
    });
}
