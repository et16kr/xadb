//! Exact mandatory effect cases for the new recovered consumer. Each selected
//! ID owns its setup and receipt; no completed wrapper is called.
use super::super::super::plan4_pages::Plan4Page;
use super::literals::{Oracle, SecondCycle};
use super::observations;
use super::support::*;
use super::*;
use crate::{
    storage::dirty_frame::DirtyFrame,
    transaction::concurrency::{OwnerRole, PageConcurrency},
};

fn wal_name(epoch: u32) -> String {
    format!("d3-wal-{epoch:08x}")
}
fn assert_homes(read: &impl Fn(&str) -> Vec<u8>, oracle: &Oracle) {
    let home = read(HOME_NAME);
    for (number, name) in [
        (1, "root-finish.page"),
        (2, "jobs-finish.page"),
        (9, "status-finish.page"),
    ] {
        assert_eq!(
            &home[number * PAGE_BYTES..(number + 1) * PAGE_BYTES],
            oracle.page(name)
        );
    }
}
pub(super) fn assert_cleanup(read: &impl Fn(&str) -> Vec<u8>, oracle: &Oracle, stop: u64) {
    let actual = read(&wal_name(2));
    let mut expected = Vec::new();
    if stop == 1 {
        let (mut cancel, _, _) = oracle.allocation(1);
        literals::put(&mut cancel, 48, 2, 4);
        let digest = content_digest(&cancel[..128]);
        cancel[128..160].copy_from_slice(&digest);
        literals::seal(&mut cancel);
        expected.extend_from_slice(&cancel);
    } else {
        for (lsn, name) in [(5, "rollback.wal"), (6, "abort.wal"), (7, "finish.wal")] {
            if lsn > stop {
                expected.extend_from_slice(&oracle.cleanup(name, 2));
            }
        }
    }
    assert_eq!(&actual[4096..], expected);
    assert_eq!(&actual[..4], b"XWE2");
}

pub(super) fn third_cycle<D: DirectoryPort>(directory: &D, read: impl Fn(&str) -> Vec<u8>) {
    let mut original = snapshot_without_old_wal(&read);
    original.insert("d3-wal-00000002".into(), read("d3-wal-00000002"));
    for path in ["CURRENT", "d3-rebase-0", "d3-rebase-1"] {
        original.insert(path.into(), read(path));
    }
    let expected_base = observations::second_base(&original);
    recovered!(directory, |runtime,
                           closed,
                           owner,
                           flusher,
                           _core,
                           _backing| {
        assert_eq!(runtime.observation().source_lsn, 11);
        observations::controls(
            &read,
            &expected_base,
            3,
            2,
            14,
            &original["d3-wal-00000002"][..4096],
        );
        observations::bank(
            closed.inner.recovery_test_bank(),
            &Oracle::new(),
            11,
            Some(&SecondCycle::new(&Oracle::new())),
        );
        let credits = closed.inner.recovery_test_credit().unwrap();
        assert_eq!((credits.0, credits.1), (0, 0));
        finish_closed(&mut closed, &mut owner, &mut flusher);
        assert_eq!(read("d3-wal-00000003").len(), 4096);
        assert_eq!(read(HOME_NAME), original[HOME_NAME]);
        let mut live = match closed.into_live() {
            Ok(value) => value,
            Err(_) => panic!("epoch3 did not verify"),
        };
        catalog(&mut live, &mut owner, &mut flusher);
    });
}

fn snapshot_without_old_wal(read: &impl Fn(&str) -> Vec<u8>) -> Images {
    [
        "LOCK",
        "d3-profile",
        "d3-bootstrap",
        "FILEMAP",
        "d3-filemap-0",
        "d3-filemap-1",
        HOME_NAME,
        "d3-dwb-stage",
        "DWB",
        "d3-dwb-0",
        "d3-dwb-1",
        "d3-dwb-images",
        "d3-dwb-reserved",
        "WRITER",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), read(name)))
    .collect()
}

pub(super) fn second_cycle<D: DirectoryPort>(directory: &D, read: impl Fn(&str) -> Vec<u8>) {
    let oracle = Oracle::new();
    let expected = SecondCycle::new(&oracle);
    let original = snapshot(&read);
    let expected_base = observations::first_base(&original, 4);
    recovered!(directory, |runtime,
                           closed,
                           owner,
                           flusher,
                           core,
                           backing| {
        assert_eq!(runtime.observation().source_lsn, 4);
        assert_eq!(
            runtime.observation().calls_attempted,
            runtime.observation().calls_completed
        );
        assert_eq!(
            runtime.observation().mutations_attempted,
            runtime.observation().mutations_completed
        );
        assert!(runtime.observation().mutations_attempted > 0);
        observations::controls(
            &read,
            &expected_base,
            2,
            1,
            7,
            &original["d3-wal-00000001"][..4096],
        );
        observations::bank(closed.inner.recovery_test_bank(), &oracle, 4, None);
        let credits = closed.inner.recovery_test_credit().unwrap();
        assert_eq!((credits.0, credits.1), observations::remaining(&oracle, 4));
        finish_closed(&mut closed, &mut owner, &mut flusher);
        observations::bank(closed.inner.recovery_test_bank(), &oracle, 7, None);
        assert_eq!(closed.inner.recovery_test_credit(), Some(credits));
        assert_cleanup(&read, &oracle, 4);
        assert_homes(&read, &oracle);
        let mut live = match closed.into_live() {
            Ok(value) => value,
            Err(_) => panic!("verified facade denied handoff"),
        };
        let status = live.status_target(page_ref(9, 2)).unwrap();
        prepare(&mut live, &owner, &mut flusher, &status, 3);
        let root = live.target(Plan4Page::Root).unwrap();
        let jobs = live.target(Plan4Page::Jobs { index: 0 }).unwrap();
        let mut offset = 4096 + 3040 + 56712 + 55112;
        for i in 0..4 {
            let observation = {
                let (mut writer, _, _) = live.split();
                match i {
                    0 => writer.begin_empty(&mut owner, &status, job(3)),
                    1 => writer.rollback_start(&mut owner, actor(2, 3)),
                    2 => writer.abort_end_empty(&mut owner, actor(2, 3)),
                    3 => writer.finish_empty_terminal(&mut owner, job(3)),
                    _ => unreachable!(),
                }
                .unwrap()
            };
            assert_eq!(
                (
                    observation.lsn.0,
                    observation.issued_local,
                    observation.adopted_local
                ),
                (8 + i as u64, 1, 1)
            );
            if i == 0 {
                assert_eq!(observation.actor, Some(actor(2, 3)));
            }
            let actual = read(&wal_name(2));
            assert_eq!(&actual[offset..], expected.frames[i]);
            offset += expected.frames[i].len();
            assert_eq!(actual.len(), offset);
            observations::bank(
                live.recovery_test_bank(),
                &oracle,
                8 + i as u64,
                Some(&expected),
            );
            assert_eq!(live.recovery_test_credit(), Some(credits));
            let at = if i == 0 || i == 1 { 0 } else { i - 1 };
            assert_resident(&mut live, &mut owner, &root, &expected.roots[at], &backing);
            assert_resident(&mut live, &mut owner, &jobs, &expected.jobs[at], &backing);
            assert_resident(
                &mut live,
                &mut owner,
                &status,
                &expected.statuses[i],
                &backing,
            );
        }
        drive_flush(&mut live, &mut flusher, None);
        for target in [&root, &jobs, &status] {
            drive_flush(&mut live, &mut flusher, Some(target));
        }
        live.recovery_verify(&mut owner).unwrap();
        catalog(&mut live, &mut owner, &mut flusher);
        let home = read(HOME_NAME);
        for (number, bytes) in [
            (1, &expected.roots[2]),
            (2, &expected.jobs[2]),
            (9, &expected.statuses[3]),
        ] {
            assert_eq!(&home[number * PAGE_BYTES..(number + 1) * PAGE_BYTES], bytes);
        }
        assert_eq!(
            runtime
                .storage
                .binding
                .load(std::sync::atomic::Ordering::Acquire),
            super::super::super::BOUND
        );
        drop(live);
    });
}

#[test]
fn recovery_lifecycle() {
    let ids: Vec<_> = (1..=6).map(|i| format!("P4-REC-L{i:02}")).collect();
    let names: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&names);
    for case in 1..=6 {
        h.case(&format!("P4-REC-L{case:02}"), || {
            let stop = match case {
                1 | 6 => 4,
                2 => 1,
                3 => 5,
                4 => 6,
                5 => 7,
                _ => unreachable!(),
            };
            let original = fresh_memory(stop);
            let d = clone_directory(&original);
            let state = d.state();
            clear(&state);
            if case == 1 || case == 6 {
                second_cycle(&d, |name| file(&state, name));
                assert_closed(&state);
                assert!(!images(&state).contains_key(&wal_name(1)));
                if case == 6 {
                    let accepted = images(&state);
                    let next = clone_directory(&accepted);
                    let next_state = next.state();
                    clear(&next_state);
                    third_cycle(&next, |name| file(&next_state, name));
                    assert!(!images(&next_state).contains_key(&wal_name(1)));
                    assert!(!images(&next_state).contains_key(&wal_name(2)));
                    let evidence = {
                        let s = next_state.lock().unwrap();
                        s.events.clone()
                    };
                    assert!(
                        !evidence
                            .iter()
                            .any(|e| e.op == "open" && e.name == wal_name(1))
                    );
                    assert!(
                        !evidence
                            .iter()
                            .any(|e| e.op == "write" && e.name == HOME_NAME),
                        "duplicate home repair after exact finished suffix"
                    );
                    assert_closed(&next_state);
                }
            } else {
                let oracle = Oracle::new();
                let expected_base = observations::first_base(&original, stop);
                recovered!(&d, |runtime, closed, owner, flusher, _core, _backing| {
                    assert_eq!(runtime.observation().source_lsn, stop);
                    assert_eq!(
                        runtime.observation().calls_attempted,
                        runtime.observation().calls_completed
                    );
                    assert_eq!(
                        runtime.observation().mutations_attempted,
                        runtime.observation().mutations_completed
                    );
                    assert!(runtime.observation().mutations_attempted > 0);
                    observations::controls(
                        &|name| file(&state, name),
                        &expected_base,
                        2,
                        1,
                        7,
                        &original["d3-wal-00000001"][..4096],
                    );
                    observations::bank(closed.inner.recovery_test_bank(), &oracle, stop, None);
                    let credits = closed.inner.recovery_test_credit().unwrap();
                    assert_eq!(
                        (credits.0, credits.1),
                        observations::remaining(&oracle, stop)
                    );
                    if case == 2 {
                        assert_eq!(
                            closed.progress.phase,
                            Plan4RecoveryPhase::CancelReservedFirst
                        );
                    }
                    finish_closed(&mut closed, &mut owner, &mut flusher);
                    observations::bank(
                        closed.inner.recovery_test_bank(),
                        &oracle,
                        if stop == 1 { 2 } else { 7 },
                        None,
                    );
                    assert_eq!(closed.inner.recovery_test_credit(), Some(credits));
                    assert_cleanup(&|name| file(&state, name), &oracle, stop);
                    if case == 2 {
                        let (_, allocator, _) = oracle.allocation(1);
                        assert_eq!(&file(&state, HOME_NAME)[..PAGE_BYTES], allocator);
                    } else {
                        assert_homes(&|name| file(&state, name), &oracle);
                    }
                    let mut live = match closed.into_live() {
                        Ok(value) => value,
                        Err(_) => panic!("cleanup did not verify"),
                    };
                    catalog(&mut live, &mut owner, &mut flusher);
                    if case == 5 {
                        let events = state.lock().unwrap().events.clone();
                        assert!(
                            !events
                                .iter()
                                .any(|e| e.op == "write" && e.name == HOME_NAME)
                        );
                    }
                });
                assert_closed(&state);
            }
            assert_eq!(
                original.len(),
                15,
                "setup snapshot retained, no stage leaked"
            );
        });
    }
    h.finish();
}

#[test]
fn recovery_binding() {
    let mut h = Harness::new(&["P4-REC-B01", "P4-REC-B02", "P4-REC-B03", "P4-REC-B04"]);
    for case in 1..=4 {
        h.case(&format!("P4-REC-B{case:02}"), || {
            let d = clone_directory(&fresh_memory(4));
            let state = d.state();
            let mut maintenance = boxed::<12_582_912>();
            let mut dwb = boxed::<4_194_304>();
            let mut slot = boxed::<1_048_576>();
            let runtime = actual_runtime(&d, &mut maintenance, &mut dwb, &mut slot);
            let core = PageConcurrency::new(runtime.session_fence()).unwrap();
            let foreign = PageConcurrency::new(runtime.session_fence()).unwrap();
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let mut first_backing = vec![[0xa7; PAGE_BYTES]; 4];
            let mut first_frames: Vec<_> = first_backing.iter_mut().map(DirtyFrame::new).collect();
            let mut first_operation = boxed::<114_688>();
            let mut second_backing = vec![[0xa5; PAGE_BYTES]; 4];
            let mut second_frames: Vec<_> =
                second_backing.iter_mut().map(DirtyFrame::new).collect();
            let mut second_operation = boxed::<114_688>();
            second_operation.fill(0xa5);
            clear(&state);
            match case {
                1 => {
                    let first = runtime
                        .bind(&owner, &core, &mut first_frames, &mut first_operation)
                        .unwrap();
                    clear(&state);
                    let error = match runtime.bind(
                        &owner,
                        &core,
                        &mut second_frames,
                        &mut second_operation,
                    ) {
                        Err(error) => error,
                        Ok(_) => panic!("second bind succeeded"),
                    };
                    assert_eq!(
                        (error.phase, error.effect),
                        (Plan4RecoveryPhase::Bind, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, Plan4RecoveryCause::Invariant));
                    assert_eq!((error.database_effect, error.lease_acquired), (Effect::CallerMustResolve, true));
                    assert!(!runtime.session_fence().is_fenced());
                    assert_eq!(runtime.storage.binding.load(std::sync::atomic::Ordering::Acquire), super::super::super::BOUND);
                    drop(first);
                }
                2 => {
                    let foreign_owner = foreign.try_owner(OwnerRole::Foreground).unwrap();
                    let error = match runtime.bind(
                        &foreign_owner,
                        &core,
                        &mut second_frames,
                        &mut second_operation,
                    ) {
                        Err(error) => error,
                        Ok(_) => panic!("foreign owner bound"),
                    };
                    assert_eq!(
                        (error.phase, error.effect),
                        (Plan4RecoveryPhase::Bind, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, Plan4RecoveryCause::Invariant));
                    assert_eq!((error.database_effect, error.lease_acquired), (Effect::CallerMustResolve, true));
                    assert!(!runtime.session_fence().is_fenced());
                    assert_eq!(runtime.storage.binding.load(std::sync::atomic::Ordering::Acquire), super::super::super::AVAILABLE);
                }
                3 => {
                    // The real foreground admission, not a fabricated Owner state.
                    std::mem::forget(owner.try_apply().unwrap());
                    let error = match runtime.bind(
                        &owner,
                        &core,
                        &mut second_frames,
                        &mut second_operation,
                    ) {
                        Err(error) => error,
                        Ok(_) => panic!("nonIdle owner bound"),
                    };
                    assert_eq!(
                        (error.phase, error.effect),
                        (Plan4RecoveryPhase::Bind, Effect::CallerMustResolve)
                    );
                    assert!(matches!(error.cause, Plan4RecoveryCause::Concurrency(e) if e.cause == crate::transaction::concurrency::Cause::Invariant && e.effect == Effect::CallerMustResolve));
                    assert!(runtime.session_fence().is_fenced());
                    assert_eq!(runtime.storage.binding.load(std::sync::atomic::Ordering::Acquire), super::super::super::AVAILABLE);
                    assert_eq!((error.database_effect, error.lease_acquired), (Effect::CallerMustResolve, true));
                }
                4 => {
                    let closed = runtime
                        .bind(&owner, &core, &mut first_frames, &mut first_operation)
                        .unwrap();
                    clear(&state);
                    let bank = closed.inner.recovery_test_bank().to_vec();
                    let pointer = closed.inner.recovery_test_bank().as_ptr() as usize;
                    let credit = closed.inner.recovery_test_credit();
                    let mut closed = match closed.into_live() {
                        Err(value) => value,
                        Ok(_) => panic!("early live capability"),
                    };
                    assert!(state.lock().unwrap().events.is_empty());
                    assert_eq!(closed.inner.recovery_test_bank(), bank);
                    assert_eq!(closed.inner.recovery_test_bank().as_ptr() as usize, pointer);
                    assert_eq!(closed.inner.recovery_test_credit(), credit);
                    finish_closed(&mut closed, &mut owner, &mut flusher);
                    assert!(closed.into_live().is_ok());
                    return;
                }
                _ => unreachable!(),
            }
            assert!(state.lock().unwrap().events.is_empty());
            drop(second_frames);
            assert!(second_operation.iter().all(|b| *b == 0xa5));
            assert!(second_backing.iter().flatten().all(|b| *b == 0xa5));
        });
    }
    h.finish();
}
