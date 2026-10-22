//! Actual writer/flusher/reader ACK schedules for two registered targets.
use super::super::super::StatusInitError;
use super::*;
fn io_pause(state: &State, at: usize, after: bool) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (arrived, ack) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    state.lock().unwrap().pause = Some(port::Pause {
        at,
        after,
        entered: arrived,
        release,
        unwind: false,
    });
    (ack, resume)
}
fn terminal<W: WriterNamespace>(
    reservation: super::super::super::ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> std::result::Result<InitializedStatusObservation, StatusInitError> {
    match reservation.initialize() {
        Ok(v) => Ok(v),
        Err(InitAttemptError::Fatal(e)) => Err(e),
        Err(InitAttemptError::Before(e)) => panic!("unexpected before {:?}", e.error),
    }
}
// action0 normal,1 fence while retained,2 view drop+continue,3 owner abandon.
fn waiting(action: u8) {
    fixture!(1, |bound, core, state| {
        {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let (ready, reserved) = mpsc::sync_channel(1);
            let (start, proceed) = mpsc::sync_channel(1);
            std::thread::scope(|scope| {
                let core_ref = &core;
                let worker = scope.spawn(move || {
                    let mut owner = ok(core_ref.try_owner(OwnerRole::Maintenance));
                    let reservation = ok(writer.reserve_one(&mut owner));
                    ready.send(()).unwrap();
                    proceed.recv_timeout(STEP).unwrap();
                    terminal(reservation)
                });
                reserved.recv_timeout(STEP).unwrap();
                reset(&state);
                let (ack, resume) = io_pause(&state, 1, false);
                start.send(()).unwrap();
                ack.recv_timeout(STEP).unwrap();
                let mut owner = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                assert_eq!(
                    ok(flusher.try_flush_status(&mut owner)),
                    FlushProgress::WaitingWal
                );
                let snapshot = flusher.snapshot();
                let registry = frame_test::flush_diagnose(flusher.pool);
                let prefix = flusher.storage.wal().append.lock().unwrap().prefix;
                assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (2, 1));
                let counts = {
                    let s = state.lock().unwrap();
                    (s.events.len(), s.reads)
                };
                if action == 1 {
                    flusher.storage.session_fence().fence();
                    assert_eq!(
                        err(flusher.continue_flush(&mut owner)).effect,
                        Effect::CallerMustResolve
                    );
                    assert_eq!(
                        err(flusher.try_flush_allocator(&mut owner)).effect,
                        Effect::CallerMustResolve
                    );
                } else {
                    let error = err(flusher.try_flush_allocator(&mut owner));
                    assert!(matches!(error.cause, Cause::Busy));
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(
                        ok(flusher.continue_flush(&mut owner)),
                        FlushProgress::WaitingWal
                    );
                }
                assert_eq!(flusher.snapshot().phase, snapshot.phase);
                assert_eq!(flusher.snapshot().io, snapshot.io);
                assert_eq!(
                    frame_test::flush_diagnose(flusher.pool).claim,
                    registry.claim
                );
                {
                    let s = state.lock().unwrap();
                    assert_eq!((s.events.len(), s.reads), counts);
                }
                resume.send(()).unwrap();
                let observed = worker.join().unwrap();
                if action == 1 {
                    assert_eq!(err(observed).effect, Effect::CallerMustResolve);
                } else {
                    assert_eq!(ok(observed).lsn, PageLsn(2));
                }
            });
        }
        if action != 1 {
            reset(&state);
            let (_, _, mut flusher) = bound.split_with_flush();
            assert_eq!(flusher.snapshot().phase, Phase::WaitingWal);
            assert_eq!(
                flusher
                    .storage
                    .wal()
                    .append
                    .lock()
                    .unwrap()
                    .prefix
                    .synced_lsn,
                2
            );
            if action != 3 {
                let before = inventory(&state);
                let mut owner = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                let result = done(ok(flusher.continue_flush(&mut owner)));
                assert_eq!((result.io.attempted, result.io.completed), (31, 31));
                full_state(&state, "p1-c0-SA-b1/after", &before);
                assert_eq!(state.lock().unwrap().events.len(), 31);
            } else {
                no_io(&state);
            }
        }
    });
}
fn flusher_first() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let storage = flusher.storage;
        let (ready, reserved) = mpsc::sync_channel(1);
        let (start, proceed) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let core_ref = &core;
            let writer_thread = scope.spawn(move || {
                let mut owner = ok(core_ref.try_owner(OwnerRole::Maintenance));
                let reservation = ok(writer.reserve_one(&mut owner));
                ready.send(()).unwrap();
                proceed.recv_timeout(STEP).unwrap();
                terminal(reservation)
            });
            reserved.recv_timeout(STEP).unwrap();
            reset(&state);
            let (force_arrived, force_ack) = mpsc::sync_channel(1);
            let (force_resume, force_release) = mpsc::sync_channel(1);
            *FORCE_PAUSE.lock().unwrap() = Some(AdapterPause {
                binding: std::ptr::from_ref(&storage.binding) as usize,
                arrived: force_arrived,
                release: force_release,
            });
            let (wait_ack, wait_resume) =
                wal::durability_test_support::pause_before_wait(storage.wal());
            let (io_ack, io_resume) = io_pause(&state, 1, false);
            start.send(()).unwrap();
            force_ack.recv_timeout(STEP).unwrap();
            let before = inventory(&state);
            let flush_thread = scope.spawn(|| {
                let mut owner = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                done(ok(flusher.try_flush_status(&mut owner)))
            });
            io_ack.recv_timeout(STEP).unwrap();
            force_resume.send(()).unwrap();
            wait_ack.recv_timeout(STEP).unwrap();
            assert_eq!(storage.wal().append.lock().unwrap().prefix.synced_lsn, 1);
            io_resume.send(()).unwrap();
            let flushed = flush_thread.join().unwrap();
            assert!(flushed.finalized.cleaned);
            assert_eq!(flushed.captured.page_lsn, PageLsn(2));
            wait_resume.send(()).unwrap();
            let initialized = ok(writer_thread.join().unwrap());
            assert_eq!(initialized.lsn, PageLsn(2));
            assert_eq!(initialized.wal.synced_lsn, 2);
            assert_eq!(state.lock().unwrap().events.len(), 33);
            let mut after_wal = before;
            after_wal.insert(wal::WAL_NAME.into(), inventory(&state)[wal::WAL_NAME]);
            full_state(&state, "p1-c0-SA-b1/after", &after_wal);
        });
    });
}
fn redirty(clean_before: bool) {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        let reservation = ok(writer.reserve_one(&mut owner));
        let before = inventory(&state);
        reset(&state);
        if clean_before {
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            assert!(
                done(ok(flusher.try_flush_allocator(&mut o)))
                    .finalized
                    .cleaned
            );
            full_state(&state, "BR04-b1/after", &before);
            assert!(!diagnose(flusher.pool).2.dirty[0]);
            ok(terminal(reservation));
        } else {
            let (ack, resume) = io_pause(&state, 9, false);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                    done(ok(flusher.try_flush_allocator(&mut o)))
                });
                ack.recv_timeout(STEP).unwrap();
                assert_eq!(
                    frame_test::flush_diagnose(reservation.writer.pool)
                        .claim
                        .unwrap()
                        .captured
                        .unwrap()
                        .page_lsn,
                    PageLsn(1)
                );
                ok(terminal(reservation));
                resume.send(()).unwrap();
                assert!(!worker.join().unwrap().finalized.cleaned);
            });
            let mut updated = before.clone();
            updated.insert(wal::WAL_NAME.into(), inventory(&state)[wal::WAL_NAME]);
            full_state(&state, "BR03-b1/after", &updated);
        }
        let snapshot = diagnose(flusher.pool).2;
        assert_eq!(snapshot.page_lsns, [PageLsn(2), PageLsn(2)]);
        assert_eq!(snapshot.counters, [2, 1]);
        assert_eq!(
            snapshot.rec_lsns,
            [
                Some(PageLsn(if clean_before { 2 } else { 1 })),
                Some(PageLsn(2))
            ]
        );
        assert_eq!(snapshot.requirements[0].unwrap().lsn, 2);
        assert_eq!(snapshot.requirements[1].unwrap().lsn, 2);
        let before = inventory(&state);
        let order = if clean_before { "SA" } else { "AS" };
        for (index, target) in order.bytes().enumerate() {
            reset(&state);
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            let result = done(ok(if target == b'A' {
                flusher.try_flush_allocator(&mut o)
            } else {
                flusher.try_flush_status(&mut o)
            }));
            assert!(result.finalized.cleaned);
            assert_eq!(result.io.last_issued, index as u64 + 2);
            full_state(
                &state,
                &format!(
                    "BR0{}-b{}/after",
                    if clean_before { 4 } else { 3 },
                    index + 2
                ),
                &before,
            );
        }
        assert_eq!(flusher.snapshot().io.known.generation, 10);
        assert_eq!(diagnose(flusher.pool).2.dirty, [false, false]);
    });
}
fn retired(action: u8) {
    fixture!(1, |bound, core, state| {
        {
            let (mut writer, reader, mut flusher) = bound.split_with_flush();
            let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
            initialize(&mut writer, &mut owner, 0);
            reset(&state);
            let before = inventory(&state);
            let (ack, resume) = io_pause(&state, 30, true);
            let mut reader_owner = ok(core.try_owner(OwnerRole::Foreground));
            let held = std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                    ok(flusher.try_flush_status(&mut o))
                });
                ack.recv_timeout(STEP).unwrap();
                let held = ok(reader.read(&mut reader_owner));
                resume.send(()).unwrap();
                assert_eq!(
                    worker.join().unwrap(),
                    FlushProgress::RetiredPendingFinalize
                );
                held
            });
            let original = flusher.snapshot();
            assert_eq!(original.phase, Phase::RetiredPendingFinalize);
            assert!(original.io.retirement_selected);
            full_state(&state, "p1-c0-SA-b1/after", &before);
            reset(&state);
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            if action == 1 {
                flusher.storage.session_fence().fence();
                assert_eq!(
                    err(flusher.continue_flush(&mut o)).effect,
                    Effect::CallerMustResolve
                );
                assert_eq!(
                    err(flusher.try_flush_allocator(&mut o)).effect,
                    Effect::CallerMustResolve
                );
                let _ = held.release_checked();
            } else {
                let error = err(flusher.try_flush_allocator(&mut o));
                assert!(matches!(error.cause, Cause::Busy));
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(
                    ok(flusher.continue_flush(&mut o)),
                    FlushProgress::RetiredPendingFinalize
                );
                no_io(&state);
                ok(held.release_checked());
            }
            assert_eq!(flusher.snapshot().phase, original.phase);
            assert_eq!(flusher.snapshot().io, original.io);
            no_io(&state);
        }
        if action != 1 {
            let (_, _, mut flusher) = bound.split_with_flush();
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            assert_eq!(flusher.snapshot().phase, Phase::RetiredPendingFinalize);
            if action != 3 {
                assert!(done(ok(flusher.continue_flush(&mut o))).finalized.cleaned);
                assert!(frame_test::flush_diagnose(flusher.pool).claim.is_none());
            }
            no_io(&state);
        }
    });
}
pub(super) fn fenced_progress() {
    fixture!(1, |bound, core, state| {
        let (_, _, mut flusher) = bound.split_with_flush();
        let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        let before = flusher.snapshot();
        flusher.storage.session_fence().fence();
        assert_eq!(
            err(flusher.try_flush_status(&mut o)).effect,
            Effect::CallerMustResolve
        );
        assert_eq!(
            err(flusher.continue_flush(&mut o)).effect,
            Effect::CallerMustResolve
        );
        assert_eq!(flusher.snapshot().phase, before.phase);
        assert_eq!(flusher.snapshot().io, before.io);
        no_io(&state);
    });
    variant("BA06", "Ready");
    waiting(1);
    variant("BA06", "WaitingWal");
    retired(1);
    variant("BA06", "RetiredPendingFinalize");
}
pub(super) fn run_br() {
    let mut count = 0;
    if selected("BR01") {
        waiting(0);
        variant("BR01", "switch-to-allocator-while-waiting");
        passed("BR01", &mut count);
    }
    if selected("BR02") {
        flusher_first();
        passed("BR02", &mut count);
    }
    if selected("BR03") {
        redirty(false);
        passed("BR03", &mut count);
    }
    if selected("BR04") {
        redirty(true);
        passed("BR04", &mut count);
    }
    if selected("BR05") {
        retired(0);
        variant("BR05", "switch-to-allocator-while-retired");
        passed("BR05", &mut count);
    }
    if selected("BR06") {
        waiting(2);
        waiting(3);
        variant("BR06", "WaitingWal");
        retired(2);
        retired(3);
        variant("BR06", "RetiredPendingFinalize");
        passed("BR06", &mut count);
    }
    complete("BR", count);
}
