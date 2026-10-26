//! Original physical WAL observation only; no Commit meaning or fake grant.
use super::{durability_test_support::*, test_support::*, *};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

fn issue_empty<'s>(
    session: &'s WalRuntime<'_, Writer>,
    previous: u64,
    lsn: u64,
) -> WalRequirement<'s> {
    let bytes = raw_frame(previous, lsn, &[]);
    assert_eq!(bytes.len(), 192);
    let prefix = session.prefix().unwrap();
    let header = wal_frame::decode_frame(
        &bytes,
        FrameExpectation {
            stream: prefix.stream,
            previous_lsn: previous,
        },
    )
    .unwrap()
    .header();
    session
        .stage_verified_required(prefix, header, &bytes)
        .unwrap()
}

fn run_case(case: usize) {
    println!("B1_WAL_FIRST_WRITE BEGIN case={case:02} actual_commit=NOT_RUN");
    // Fixed independent markers: before write0, any actual first write1,
    // recovery/prefix observations never synthesize a marker.
    if case < 10 {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let required = issue_empty(&session, 0, 1);
        let first = calls(&state);
        assert!(!session.first_write_started(&required).unwrap());
        assert_eq!(session.first_write_lsn.load(Ordering::Acquire), 0);
        match case {
            3..=7 => {
                state.lock().unwrap().plan = Some(Plan {
                    at: first + usize::from(case == 7),
                    action: match case {
                        3 => Action::PrefixError,
                        4 => Action::Zero,
                        5 => Action::Interrupted,
                        6 => Action::Short(7),
                        7 => Action::AfterError,
                        _ => unreachable!(),
                    },
                });
            }
            8 => {
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _file = session.file.lock().unwrap();
                        panic!("new marker file poison input");
                    }))
                    .is_err()
                );
            }
            9 => session.fenced.fence(),
            _ => {}
        }
        let success = match case {
            0 => session.sync_pending().is_ok(),
            2 => session.wait_durable(&required).is_ok(),
            _ => session.ensure_durable(&required).is_ok(),
        };
        let expected_success = matches!(case, 0 | 1 | 2 | 5 | 6);
        assert_eq!(success, expected_success);
        let expected_marker = u64::from(case < 8);
        assert_eq!(
            session.first_write_lsn.load(Ordering::Acquire),
            expected_marker
        );
        assert_eq!(session.first_write_started(&required).unwrap(), case < 8);
        let expected_calls = match case {
            0..=2 => 2,
            3 => 2,
            4 => 1,
            5 | 6 => 3,
            7 => 2,
            8 | 9 => 0,
            _ => unreachable!(),
        };
        assert_eq!(calls(&state), first + expected_calls, "case={case:02}");
        {
            let io = state.lock().unwrap();
            let writes = io.events[first..]
                .iter()
                .filter(|event| event.kind == "write")
                .count();
            assert_eq!(
                writes,
                if case >= 8 {
                    0
                } else if matches!(case, 3 | 5 | 6) {
                    2
                } else {
                    1
                }
            );
            if case < 8 {
                assert_eq!(
                    (io.events[first].offset, io.events[first].length),
                    (4096, 192)
                );
            }
            if case == 3 {
                // The original port returns a successful half-write, then a
                // second BeforeError call. Preserve this actual input meaning.
                assert_eq!(
                    (io.events[first + 1].offset, io.events[first + 1].length),
                    (4192, 96)
                );
                assert!(io.events[first + 1].data.is_empty());
                let raw = raw_frame(0, 1, &[]);
                assert_eq!(&io.files[WAL_NAME].bytes[4096..], &raw[..96]);
                assert_eq!(io.files[WAL_NAME].length, 4192);
            }
        }
        if expected_success {
            assert!(matches!(
                session.ensure_durable(&required).unwrap(),
                EnsureDurable::AlreadySynced(_)
            ));
            assert_eq!(calls(&state), first + expected_calls, "case={case:02}");
            assert_eq!(session.first_write_lsn.load(Ordering::Acquire), 1);
        } else {
            assert!(session.is_fenced());
        }
        drop(session);
        let io = state.lock().unwrap();
        assert_eq!((io.writers, io.handles), (0, 0));
    }

    // Actual claim has already moved to IoOwned but the original file lock is
    // unavailable. A fence before its return must not be called a write attempt.
    if case == 10 {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let required = issue_empty(&session, 0, 1);
        let first = calls(&state);
        let file = session.file.lock().unwrap();
        std::thread::scope(|scope| {
            let drainer = scope.spawn(|| session.ensure_durable(&required));
            let deadline = Instant::now() + STEP;
            loop {
                if matches!(session.append.lock().unwrap().state, SlotState::IoOwned(_)) {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "actual I/O ownership cut timed out"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!session.first_write_started(&required).unwrap());
            session.fenced.fence();
            drop(file);
            assert!(drainer.join().unwrap().is_err());
        });
        assert!(!session.first_write_started(&required).unwrap());
        assert_eq!(calls(&state), first);
    }

    // Origin is authentic but foreign; same LSN/bytes cannot supply this fact.
    if case == 11 {
        let mut left = slot();
        let mut right = slot();
        let (left_session, _) = session(&mut left);
        let (right_session, _) = session(&mut right);
        let foreign = issue_empty(&right_session, 0, 1);
        let error = left_session.first_write_started(&foreign).unwrap_err();
        assert!(matches!(error.cause, Cause::IdentityMismatch));
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(left_session.first_write_lsn.load(Ordering::Acquire), 0);
        assert!(!left_session.is_fenced());
        assert!(!right_session.is_fenced());
    }

    // LSNs need not be adjacent. Later authentic attempts imply the earlier
    // sequential physical attempt, without changing an already synced claim.
    if case == 12 {
        let mut memory = slot();
        let (session, _) = session(&mut memory);
        let old = issue_empty(&session, 0, 1);
        session.wait_durable(&old).unwrap();
        let new = issue_empty(&session, 1, 7);
        assert!(session.first_write_started(&old).unwrap());
        assert!(!session.first_write_started(&new).unwrap());
        session.sync_pending().unwrap();
        assert_eq!(session.first_write_lsn.load(Ordering::Acquire), 7);
        assert!(session.first_write_started(&old).unwrap());
        assert!(session.first_write_started(&new).unwrap());
    }

    // A second drainer observes the original live I/O owner; it cannot mark or
    // perform the same first-write boundary again.
    if case == 13 {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let required = issue_empty(&session, 0, 1);
        let first = calls(&state);
        let (arrival, resume) = pause_at(&state, first, false);
        std::thread::scope(|scope| {
            let drainer = scope.spawn(|| session.ensure_durable(&required));
            arrival.recv_timeout(STEP).unwrap();
            assert!(session.first_write_started(&required).unwrap());
            assert!(matches!(
                session.ensure_durable(&required).unwrap(),
                EnsureDurable::AwaitWalOwner
            ));
            assert_eq!(session.first_write_lsn.load(Ordering::Acquire), 1);
            assert_eq!(calls(&state), first + 1);
            resume.try_send(()).unwrap();
            assert!(matches!(
                drainer.join().unwrap().unwrap(),
                EnsureDurable::SyncedHere(_)
            ));
        });
        assert_eq!(calls(&state), first + 2);
    }
    println!("B1_WAL_FIRST_WRITE PASS case={case:02} actual_commit=NOT_RUN");
}

macro_rules! physical_case {
    ($name:ident, $case:literal) => {
        #[test]
        fn $name() {
            run_case($case);
        }
    };
}
physical_case!(wal_first_write_c00_sync_pending, 0);
physical_case!(wal_first_write_c01_ensure_durable, 1);
physical_case!(wal_first_write_c02_wait_durable, 2);
physical_case!(wal_first_write_c03_prefix_error, 3);
physical_case!(wal_first_write_c04_zero_write, 4);
physical_case!(wal_first_write_c05_interrupted, 5);
physical_case!(wal_first_write_c06_short_write, 6);
physical_case!(wal_first_write_c07_sync_error, 7);
physical_case!(wal_first_write_c08_file_poison, 8);
physical_case!(wal_first_write_c09_prewrite_fence, 9);
physical_case!(wal_first_write_c10_claim_wait_fence, 10);
physical_case!(wal_first_write_c11_foreign_origin, 11);
physical_case!(wal_first_write_c12_lsn_gap, 12);
physical_case!(wal_first_write_c13_existing_io_owner, 13);
