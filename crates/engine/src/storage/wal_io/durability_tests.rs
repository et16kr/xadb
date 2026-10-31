use super::{durability_test_support::*, test_support::*, *};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::Ordering,
};

#[test]
fn wal_requirement_syncs_once_and_reuses_confirmation() {
    for raw_winner in [false, true] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let (required, bytes) = issue(&session, 0, 1);
        let first = calls(&state);
        if raw_winner {
            assert_eq!(session.sync_pending().unwrap().frame, required.location());
        } else {
            let EnsureDurable::SyncedHere(proof) = session.ensure_durable(&required).unwrap()
            else {
                panic!("first exact claimant must perform the I/O");
            };
            assert!(proof.confirms(&required));
        }
        assert_eq!(calls(&state), first + 2);
        let EnsureDurable::AlreadySynced(proof) = session.ensure_durable(&required).unwrap() else {
            panic!("already confirmed requirement must not claim I/O");
        };
        let observed = proof.observation();
        assert_eq!(observed.frame, required.location());
        assert_eq!(
            (observed.prefix.synced_lsn, observed.prefix.synced_offset),
            (1, 4291)
        );
        assert!(session.wait_durable(&required).unwrap().confirms(&required));
        assert_eq!(calls(&state), first + 2);
        assert_eq!(&state.lock().unwrap().files[WAL_NAME].bytes[4096..], bytes);
        assert!(matches!(
            session.sync_pending().unwrap_err().cause,
            Cause::NoPending
        ));
        assert!(!session.is_fenced());
    }
    println!("WD01 cases=2");
}

#[test]
fn wal_requirement_awaits_existing_io_owner() {
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    let (required, _) = issue(&session, 0, 1);
    let first = calls(&state);
    let (arrival, resume) = pause_at(&state, first, false);
    std::thread::scope(|scope| {
        let winner = scope.spawn(|| session.ensure_durable(&required));
        arrival.recv_timeout(STEP).unwrap();
        retained(&session, required.location());
        assert!(matches!(
            session.ensure_durable(&required).unwrap(),
            EnsureDurable::AwaitWalOwner
        ));
        assert_eq!(calls(&state), first + 1);
        assert!(matches!(
            session.sync_pending().unwrap_err().cause,
            Cause::Busy
        ));
        resume.try_send(()).unwrap();
        assert!(matches!(
            winner.join().unwrap().unwrap(),
            EnsureDurable::SyncedHere(_)
        ));
    });
    assert!(matches!(
        session.ensure_durable(&required).unwrap(),
        EnsureDurable::AlreadySynced(_)
    ));
    assert_eq!(calls(&state), first + 2);
    println!("WD02 cases=1");
}

#[test]
fn wal_requirement_waiter_handles_publish_and_spurious_wakes() {
    for raw_winner in [false, true] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let (required, _) = issue(&session, 0, 1);
        let first = calls(&state);
        let (arrival, resume) = pause_at(&state, first, false);
        std::thread::scope(|scope| {
            let winner = scope.spawn(|| {
                if raw_winner {
                    session.sync_pending().unwrap();
                } else {
                    assert!(matches!(
                        session.ensure_durable(&required).unwrap(),
                        EnsureDurable::SyncedHere(_)
                    ));
                }
            });
            arrival.recv_timeout(STEP).unwrap();
            let waiter = scope.spawn(|| session.wait_durable(&required));
            waited(&session, 1);
            let previous = session.durability_waits.load(Ordering::SeqCst);
            session.completion.notify_all();
            waited(&session, previous + 1);
            retained(&session, required.location());
            assert_eq!(calls(&state), first + 1);
            resume.try_send(()).unwrap();
            winner.join().unwrap();
            let proof = waiter.join().unwrap().unwrap();
            assert!(proof.confirms(&required));
            assert_eq!(proof.observation().prefix.synced_offset, 4291);
        });
        assert_eq!(calls(&state), first + 2);
    }
    println!("WD03 cases=2");
}

#[test]
fn wal_requirement_waiter_preserves_failed_claim() {
    for at_sync in [false, true] {
        for action in [
            Action::BeforeError,
            Action::AfterError,
            Action::BeforePanic,
            Action::AfterPanic,
        ] {
            let mut memory = slot();
            let (session, state) = session(&mut memory);
            let (required, bytes) = issue(&session, 0, 1);
            let before_files = state.lock().unwrap().files.clone();
            let first = calls(&state);
            state.lock().unwrap().plan = Some(Plan {
                at: first + usize::from(at_sync),
                action,
            });
            let (arrival, resume) = pause_at(&state, first, false);
            std::thread::scope(|scope| {
                let winner = scope
                    .spawn(|| catch_unwind(AssertUnwindSafe(|| session.ensure_durable(&required))));
                arrival.recv_timeout(STEP).unwrap();
                let waiter = scope.spawn(|| session.wait_durable(&required));
                waited(&session, 1);
                resume.try_send(()).unwrap();
                let outcome = winner.join().unwrap();
                if matches!(action, Action::BeforePanic | Action::AfterPanic) {
                    assert!(outcome.is_err());
                } else {
                    let error = outcome.unwrap().unwrap_err();
                    assert_eq!(
                        error.phase,
                        if at_sync { Phase::Sync } else { Phase::Write }
                    );
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    let Cause::Io(original) = error.cause else {
                        panic!("original I/O error lost")
                    };
                    assert_eq!(original.code, foundation::error::ErrorCode::IoWriteUnknown);
                    assert_eq!(original.io_kind, Some(std::io::ErrorKind::Other));
                    assert_eq!(
                        original.context,
                        if at_sync {
                            "sync outcome unknown"
                        } else {
                            "write outcome requires caller resolution"
                        }
                    );
                    assert_eq!(original.certainty(), foundation::error::Certainty::Unknown);
                }
                fatal(&waiter.join().unwrap().unwrap_err());
            });
            assert!(session.is_fenced());
            retained(&session, required.location());
            assert_eq!(state.lock().unwrap().hit, 1);
            assert_eq!(calls(&state), first + if at_sync { 2 } else { 1 });
            let observed = state.lock().unwrap();
            let events = &observed.events[first..];
            assert_eq!(events[0].kind, "write");
            assert_eq!(events[0].offset, 4096);
            assert_eq!(events[0].length, 195);
            if at_sync {
                assert_eq!(events[1].kind, "sync");
                assert_eq!(events[1].name, events[0].name);
            }
            let changed = at_sync || matches!(action, Action::AfterError | Action::AfterPanic);
            let old = &before_files[&events[0].name].bytes;
            let actual = &observed.files[&events[0].name].bytes;
            let mut expected = old.clone();
            if changed {
                expected.resize(4291, 0);
                expected[4096..4291].copy_from_slice(&bytes);
            }
            assert_eq!(actual, &expected);
            if at_sync || matches!(action, Action::AfterError | Action::AfterPanic) {
                assert_eq!(events[0].data, bytes);
            } else {
                assert!(events[0].data.is_empty());
            }
        }
    }
    println!("WD04 cases=8");
}

#[test]
fn wal_requirement_old_interval_ignores_new_pending() {
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    let (old, _) = issue(&session, 0, 1);
    session.wait_durable(&old).unwrap();
    let (new, _) = issue(&session, 1, 7);
    let first = calls(&state);
    let EnsureDurable::AlreadySynced(proof) = session.ensure_durable(&old).unwrap() else {
        panic!()
    };
    assert!(proof.confirms(&old));
    assert!(!proof.confirms(&new));
    assert_eq!(calls(&state), first);
    assert_eq!(
        session.append.lock().unwrap().state,
        SlotState::Complete(new.location())
    );
    let (arrival, resume) = pause_at(&state, first, false);
    std::thread::scope(|scope| {
        let winner = scope.spawn(|| session.ensure_durable(&new));
        arrival.recv_timeout(STEP).unwrap();
        let EnsureDurable::AlreadySynced(proof) = session.ensure_durable(&old).unwrap() else {
            panic!()
        };
        assert_eq!(proof.observation().frame, old.location());
        assert_eq!(calls(&state), first + 1);
        assert_eq!(
            session.append.lock().unwrap().state,
            SlotState::IoOwned(new.location())
        );
        resume.try_send(()).unwrap();
        winner.join().unwrap().unwrap();
    });
    let EnsureDurable::AlreadySynced(proof) = session.ensure_durable(&old).unwrap() else {
        panic!()
    };
    assert_eq!(proof.observation().frame, old.location());
    assert_eq!(
        (
            proof.observation().prefix.synced_lsn,
            proof.observation().prefix.synced_offset
        ),
        (7, 4486)
    );
    assert_eq!(calls(&state), first + 2);
    println!("WD05 cases=3");
}

#[test]
fn wal_requirement_rejects_foreign_and_impossible_intervals() {
    let mut left_memory = slot();
    let mut right_memory = slot();
    let (left, _) = session(&mut left_memory);
    let (right, state) = session(&mut right_memory);
    let (left_req, _) = issue(&left, 0, 1);
    let (right_req, _) = issue(&right, 0, 1);
    assert_eq!(left_req.location(), right_req.location()); // same numbers, foreign owner
    let first = calls(&state);
    let error = right.ensure_durable(&left_req).unwrap_err();
    assert_eq!(error.phase, Phase::Observe);
    assert_eq!(error.effect, Effect::Unchanged);
    assert!(matches!(error.cause, Cause::IdentityMismatch));
    assert!(!left.is_fenced() && !right.is_fenced());
    assert_eq!(calls(&state), first);
    right.fenced.fence();
    fatal(&right.ensure_durable(&left_req).unwrap_err());
    assert!(!left.is_fenced());
    for damage in 0..8 {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let (mut required, _) = issue(&session, 0, 1);
        let first = calls(&state);
        // Private corruption fixtures, never public receipt constructors.
        match damage {
            0 => required.exact.file.inode += 1,
            1 => required.exact.stream.writer_epoch += 1,
            2 => required.exact.end += 1,
            3 => required.exact.length = 0,
            4 => required.exact.start = 4095,
            5 => required.exact.lsn = 2,
            6 => session.append.lock().unwrap().state = SlotState::Idle,
            7 => session.append.lock().unwrap().prefix.synced_lsn = 1,
            _ => unreachable!(),
        }
        let error = session.ensure_durable(&required).unwrap_err();
        assert_eq!(error.phase, Phase::Observe);
        assert!(matches!(error.cause, Cause::Poisoned));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(session.is_fenced());
        assert_eq!(calls(&state), first);
    }
    println!("WD06 cases=10");
}

#[test]
fn wal_requirement_waiter_observes_external_fence_and_poison() {
    for poison in [false, true] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let (required, _) = issue(&session, 0, 1);
        let first = calls(&state);
        let (arrival, resume) = pause_at(&state, first, false);
        std::thread::scope(|scope| {
            let winner = scope.spawn(|| session.ensure_durable(&required));
            arrival.recv_timeout(STEP).unwrap();
            let waiter = scope.spawn(|| session.wait_durable(&required));
            waited(&session, 1);
            if poison {
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _state = session.append.lock().unwrap();
                        panic!("fixed append poison while waiting");
                    }))
                    .is_err()
                );
            } else {
                session.fenced.fence(); // intentionally no condvar notification
            }
            fatal(&waiter.join().unwrap().unwrap_err());
            resume.try_send(()).unwrap();
            fatal(&winner.join().unwrap().unwrap_err());
        });
        retained(&session, required.location());
        // The in-flight write had entered before fencing; its completion is legal.
        assert_eq!(calls(&state), first + 2);
    }
    // Actual file sync has completed, but publication must still reject fencing.
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    let (required, _) = issue(&session, 0, 1);
    let first = calls(&state);
    let (arrival, resume) = pause_at(&state, first + 1, true);
    std::thread::scope(|scope| {
        let winner = scope.spawn(|| session.ensure_durable(&required));
        arrival.recv_timeout(STEP).unwrap();
        let waiter = scope.spawn(|| session.wait_durable(&required));
        waited(&session, 1);
        session.fenced.fence();
        resume.try_send(()).unwrap();
        let error = winner.join().unwrap().unwrap_err();
        assert_eq!(error.phase, Phase::Publish);
        fatal(&error);
        fatal(&waiter.join().unwrap().unwrap_err());
    });
    retained(&session, required.location());
    assert_eq!(calls(&state), first + 2);
    println!("WD07 cases=3");
}

#[test]
fn wal_requirement_layout_and_publication_before_wait() {
    use std::mem::size_of;
    assert!(size_of::<WalRequirement<'_>>() <= 128);
    assert!(size_of::<Option<WalRequirement<'_>>>() <= 128);
    assert!(size_of::<DurableWal<'_>>() <= 256);
    assert!(size_of::<EnsureDurable<'_>>() <= 512);
    assert!(size_of::<Result<EnsureDurable<'_>>>() <= 512);
    assert!(size_of::<Condvar>() <= 64);
    assert!(size_of::<WalFrameLocation>() <= 128);
    assert!(size_of::<WalPrefix>() <= 128);
    assert!(size_of::<WalIoError>() <= 128);
    assert!(size_of::<IoFence<'_>>() <= 64);
    assert!(size_of::<MutexGuard<'_, AppendState<'_>>>() <= 32);
    assert!(size_of::<std::sync::LockResult<MutexGuard<'_, AppendState<'_>>>>() <= 64);
    assert!(
        size_of::<
            std::sync::LockResult<(
                MutexGuard<'_, AppendState<'_>>,
                std::sync::WaitTimeoutResult
            )>,
        >() <= 64
    );
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    let (required, _) = issue(&session, 0, 1);
    let first = calls(&state);
    let (io_arrival, io_resume) = pause_at(&state, first, false);
    let (wait_arrival, wait_resume) = pause_before_wait(&session);
    std::thread::scope(|scope| {
        let winner = scope.spawn(|| session.sync_pending());
        io_arrival.recv_timeout(STEP).unwrap();
        let waiter = scope.spawn(|| session.wait_durable(&required));
        wait_arrival.recv_timeout(STEP).unwrap();
        // The waiter has seen Await but cannot yet reacquire append state.
        io_resume.try_send(()).unwrap();
        assert_eq!(winner.join().unwrap().unwrap().frame, required.location());
        wait_resume.try_send(()).unwrap();
        let proof = waiter.join().unwrap().unwrap();
        assert!(proof.confirms(&required));
    });
    assert_eq!(calls(&state), first + 2);
    assert_eq!(session.durability_waits.load(Ordering::SeqCst), 0);
    println!(
        "WD08 cases=1 requirement={} optional={} proof={} progress={} result={} condvar={}",
        size_of::<WalRequirement<'_>>(),
        size_of::<Option<WalRequirement<'_>>>(),
        size_of::<DurableWal<'_>>(),
        size_of::<EnsureDurable<'_>>(),
        size_of::<Result<EnsureDurable<'_>>>(),
        size_of::<Condvar>()
    );
}
