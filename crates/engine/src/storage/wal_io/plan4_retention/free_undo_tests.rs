//! New actual SystemInit consumers; independent raw literals supply expectations.
use super::*;
use crate::{
    storage::{
        dirty_frame::{DirtyFrame, init_test_support as probe, status_load_test_support as slots},
        page::{FileId, PageKind, PageRef},
        retention_page::JobSlotRef,
        wal_frame::{FrameExpectation, MAX_FRAME_BYTES, WalStreamIdentity},
        wal_io::{FreshStorageSession, plan4_test_support as port},
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::{identity::Generation, namespace::DirectoryPort};
use std::sync::{Arc, Mutex};

type Live<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w> =
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, port::Writer>;
type State = Arc<Mutex<port::State>>;
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().ok().unwrap()
}
fn page(number: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x100000001).unwrap(),
        page_no: number,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}
fn job() -> JobSlotRef {
    JobSlotRef::new(page(2, 1), 0, Generation::new(2).unwrap()).unwrap()
}
fn wal_bytes(state: &State) -> Vec<u8> {
    state.lock().unwrap().files[crate::storage::wal_io::WAL_NAME]
        .bytes
        .clone()
}
fn effects(state: &State) -> (usize, usize, usize) {
    let s = state.lock().unwrap();
    (s.events.len(), s.mutations(), s.entropy_calls)
}
macro_rules! original {
    (|$live:ident, $owner:ident, $core:ident, $foreign:ident, $state:ident, $storage:ident| $body:block) => {{
        let directory = port::Directory::fresh();
        let $state = directory.state();
        $state.lock().unwrap().live_writes = true;
        let mut wal = boxed::<MAX_FRAME_BYTES>();
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let $storage = FreshStorageSession::create_retention_catalog(
            directory.acquire_writer().unwrap(), 1, &mut wal, &mut maintenance,
        ).unwrap().install_doublewrite(&mut dwb).unwrap().into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        let $foreign = PageConcurrency::new($storage.session_fence()).unwrap();
        let mut $owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut control = boxed::<RETENTION_CONTROL_BYTES>();
        let mut replay = boxed::<RETENTION_REPLAY_BYTES>();
        let old = $storage.bind_plan4_pages(&$core, &mut frames, &mut operation).unwrap();
        let mut $live = match old.into_empty_retention(&mut $owner, EmptyRetentionControlLoan {
            control: &mut control, replay: &mut replay,
        }) {
            Ok(v) => v,
            Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => panic!("activation: {error:?}"),
        };
        $body
        drop($live); drop($owner); drop($storage);
        let s = $state.lock().unwrap();
        assert_eq!((s.writers, s.handles), (0, 0));
    }};
}
fn flush<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    core: &'c PageConcurrency<'s>,
    target: &Plan4Target,
) {
    let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let (_, _, mut f) = live.split();
    assert!(matches!(
        f.try_flush(&mut owner, target).unwrap(),
        RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
    ));
    assert_eq!(f.phase(), RetentionFlushPhase::Ready);
}
fn prepare<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    owner: &Owner<'c, 's>,
    core: &'c PageConcurrency<'s>,
    undo: bool,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..8 {
        let step = if undo {
            live.prepare_undo_slot(owner, &mut cursor)
        } else {
            live.prepare_status_slot(owner, &mut cursor)
        }
        .unwrap();
        match step {
            None => return,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                flush(live, core, &target)
            }
            _ => panic!("unexpected preparation"),
        }
    }
    panic!("bounded preparation");
}
fn initialize<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    owner: &mut Owner<'c, 's>,
    core: &'c PageConcurrency<'s>,
    undo: bool,
    expected_page: PageRef,
    lsn: u64,
) -> Plan4Target {
    prepare(live, owner, core, undo);
    let token = if undo {
        live.reserve_undo(owner)
    } else {
        live.reserve_status(owner)
    }
    .unwrap();
    assert_eq!(token.page(), expected_page);
    let target = match token.initialize() {
        Ok(v) => v,
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("initialize: {error:?}"),
    };
    assert_eq!(
        target.kind(),
        if undo {
            PageKind::Undo
        } else {
            PageKind::TransactionStatus
        }
    );
    let (prefix, durable, reserved, installation, pending, claim, pins) =
        live.free_undo_test_state().unwrap();
    assert_eq!(
        (prefix.complete_lsn, prefix.synced_lsn, durable),
        (lsn, lsn, Some(lsn))
    );
    assert_eq!(
        (reserved, installation, pending, claim, pins),
        (false, false, false, false, 0)
    );
    let RetentionResidencyStep::Resident {
        loaded: false,
        frame,
        ..
    } = live
        .residency_step(&target, &mut Plan4ResidencyCursor::new())
        .unwrap()
    else {
        panic!("installed resident")
    };
    assert_eq!(
        (frame.page_lsn, frame.rec_lsn),
        (PageLsn(lsn), Some(PageLsn(lsn)))
    );
    owner.check_idle().unwrap();
    target
}
fn read<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    owner: &mut Owner<'c, 's>,
    target: &Plan4Target,
    expected: &[u8],
) {
    let (_, reader, _) = live.split();
    let guard = reader.read(owner, target).unwrap();
    assert_eq!(guard.bytes().unwrap().as_slice(), expected);
    assert_eq!(probe::claim_state(reader.pool), (false, 2));
    guard.release_checked().unwrap();
    assert_eq!(probe::claim_state(reader.pool), (false, 0));
}
fn assert_page(live: &Live<'_, '_, '_, '_, '_, '_, '_, '_>, number: u64, expected: &[u8]) {
    let index = (0..4)
        .find(|&i| {
            slots::slot(&live.inner.pool, i).is_some_and(|s| s.0.identity.page.page_no == number)
        })
        .unwrap();
    let mut bytes = boxed::<PAGE_BYTES>();
    probe::snapshot(&live.inner.pool, index, &mut bytes);
    assert_eq!(bytes.as_slice(), expected);
}
fn memory_terminal(live: &Live<'_, '_, '_, '_, '_, '_, '_, '_>, oracle: &FreeUndoOracle, lsn: u64) {
    assert_page(live, 0, &oracle.memory_allocator(lsn));
    assert_eq!(
        &live.control.row(control::ALLOCATION_SLOT)[..1536],
        oracle.memory_descriptor(lsn)
    );
}

#[test]
fn free_undo_original_owner_gap_cancel_reuse_flush_reload() {
    let oracle = FreeUndoOracle::new();
    original!(|live, owner, core, _foreign, state, storage| {
        let status = initialize(&mut live, &mut owner, &core, false, page(9, 2), 2);
        memory_terminal(&live, &oracle, 2);
        read(&mut live, &mut owner, &status, oracle.memory_payload(2));
        let mut cursor = Plan4ResidencyCursor::new();
        for _ in 0..8 {
            match live
                .prepare_empty_targets(&owner, &status, job(), &mut cursor)
                .unwrap()
            {
                None => break,
                Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                    flush(&mut live, &core, &target)
                }
                _ => panic!("empty lifecycle prepare"),
            }
        }
        let actor = crate::storage::retention_page::tests::actor(1, 2);
        for index in 0..5 {
            let (mut writer, _, _) = live.split();
            let observed = match index {
                0 => writer.aging_start(&mut owner),
                1 => writer.begin_empty(&mut owner, &status, job()),
                2 => writer.rollback_start(&mut owner, actor),
                3 => writer.abort_end_empty(&mut owner, actor),
                4 => writer.finish_empty_terminal(&mut owner, job()),
                _ => unreachable!(),
            }
            .unwrap();
            assert_eq!(observed.lsn.0, 3 + index);
        }
        for (number, name) in [(1, "root"), (2, "jobs"), (9, "status")] {
            assert_page(
                &live,
                number,
                oracle.reference(&format!("memory-{name}-finish.page")),
            );
        }
        assert_eq!(
            &live.control.row(control::AGING_SLOT)[..1536],
            oracle.reference("memory-aging-finish.descriptor")
        );
        let undo = initialize(&mut live, &mut owner, &core, true, page(10, 2), 9);
        memory_terminal(&live, &oracle, 9);
        read(&mut live, &mut owner, &undo, oracle.memory_payload(9));
        flush(&mut live, &core, &undo);
        let RetentionResidencyStep::Resident {
            index,
            loaded: false,
            frame,
        } = live
            .residency_step(&undo, &mut Plan4ResidencyCursor::new())
            .unwrap()
        else {
            panic!("flushed resident")
        };
        assert!(frame.rec_lsn.is_none());
        let anchor = live.target(Plan4Page::CatalogAnchor).unwrap();
        assert!(live.load_into(index, &anchor).unwrap().before.is_some());
        assert!(live.load_into(index, &undo).unwrap().before.is_some());
        read(&mut live, &mut owner, &undo, oracle.memory_payload(9));
        prepare(&mut live, &owner, &core, true);
        let token = live.reserve_undo(&mut owner).unwrap();
        assert_eq!(token.page(), page(11, 2));
        match token.cancel() {
            Ok(()) => {}
            Err(RetentionReservationError::Before { error, .. })
            | Err(RetentionReservationError::Fatal(error)) => panic!("cancel: {error:?}"),
        }
        memory_terminal(&live, &oracle, 11);
        let reused = initialize(&mut live, &mut owner, &core, true, page(11, 3), 13);
        memory_terminal(&live, &oracle, 13);
        read(&mut live, &mut owner, &reused, oracle.memory_payload(13));
        let actual = wal_bytes(&state);
        let mut offset = 4096;
        for lsn in 1..=13 {
            let expected = oracle.memory_frame(lsn);
            assert_eq!(
                &actual[offset..offset + expected.len()],
                expected,
                "frame {lsn}"
            );
            offset += expected.len();
        }
        assert_eq!(actual.len(), offset);
        assert!(!storage.session_fence().is_fenced());
        owner.check_idle().unwrap();
    });
}

#[test]
fn free_undo_original_role_rejection_and_corruption_cleanup() {
    original!(|live, owner, core, foreign, state, storage| {
        prepare(&mut live, &owner, &core, true);
        let mut foreign_owner = foreign.try_owner(OwnerRole::Foreground).unwrap();
        let before = (effects(&state), live.free_undo_test_state().unwrap());
        let error = match live.reserve_undo(&mut foreign_owner) {
            Ok(_) => panic!("foreign owner"),
            Err(e) => e,
        };
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(
            matches!(error.cause, RetentionCause::Frame(e) if e.cause == dirty_frame::Cause::BindingMismatch)
        );
        assert_eq!(
            (effects(&state), live.free_undo_test_state().unwrap()),
            before
        );
        foreign_owner.check_idle().unwrap();
        let undo = initialize(&mut live, &mut owner, &core, true, page(9, 2), 2);
        let before = (
            state.lock().unwrap().mutations(),
            live.free_undo_test_state().unwrap(),
        );
        let error = match live.status_target(page(9, 2)) {
            Ok(_) => panic!("wrong logical role"),
            Err(e) => e,
        };
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(matches!(error.cause, RetentionCause::Membership));
        assert_eq!(
            (
                state.lock().unwrap().mutations(),
                live.free_undo_test_state().unwrap()
            ),
            before
        );
        assert!(!storage.session_fence().is_fenced());
        let RetentionResidencyStep::Resident { index, .. } = live
            .residency_step(&undo, &mut Plan4ResidencyCursor::new())
            .unwrap()
        else {
            panic!("resident")
        };
        probe::corrupt_retention_content(&live.inner.pool, index, 15920, b'!');
        let (_, reader, _) = live.split();
        let error = match reader.read(&mut owner, &undo) {
            Ok(_) => panic!("corrupt Undo"),
            Err(e) => e,
        };
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(matches!(error.cause, RetentionCause::Undo(_)), "{error:?}");
        assert!(storage.session_fence().is_fenced());
        assert_eq!(probe::claim_state(reader.pool), (false, 0));
        assert!(
            owner.check_idle().is_err(),
            "the original session remains fenced"
        );
        drop(foreign_owner);
    });
}

#[test]
fn free_undo_scope_budget_literals_and_source_census() {
    use super::free_undo_literals::FrameFault;
    let oracle = FreeUndoOracle::new();
    let profile =
        crate::storage::control::decode_bootstrap_profile(oracle.reference("profile.bin"), None)
            .unwrap();
    let stream = WalStreamIdentity {
        database: profile.spec().namespace.database,
        timeline: 1,
        writer_epoch: 1,
    };
    let fresh = || {
        let mut arena = boxed::<RETENTION_REPLAY_BYTES>();
        for index in 0..9 {
            arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                .copy_from_slice(oracle.reference(&format!("P{index:02}.bin")));
        }
        (
            population::ReplayAllocation::new(),
            arena,
            boxed::<65_536>(),
            vec![[0xa7; PAGE_BYTES]; 3],
            vec![[0; 352]; 64],
        )
    };
    for kind in ["status", "undo", "cancel"] {
        let (mut cursor, mut arena, mut bank, mut after, mut claims) = fresh();
        cursor
            .apply(
                profile,
                FrameExpectation {
                    stream,
                    previous_lsn: 0,
                },
                oracle.native_frame(1),
                &mut arena[..],
                &mut bank[..],
                after.as_mut_slice().try_into().unwrap(),
                claims.as_mut_slice().try_into().unwrap(),
            )
            .unwrap();
        assert!(cursor.reserved());
        let terminal = cursor
            .apply(
                profile,
                FrameExpectation {
                    stream,
                    previous_lsn: 1,
                },
                oracle.pure_terminal(kind),
                &mut arena[..],
                &mut bank[..],
                after.as_mut_slice().try_into().unwrap(),
                claims.as_mut_slice().try_into().unwrap(),
            )
            .unwrap();
        assert!(!cursor.reserved());
        assert_eq!(
            terminal.initialized.map(|r| r.kind),
            match kind {
                "status" => Some(PageKind::TransactionStatus),
                "undo" => Some(PageKind::Undo),
                _ => None,
            }
        );
        assert_eq!(
            &control::row(&bank[..], control::ALLOCATION_SLOT)[..1536],
            &oracle.pure_terminal(kind)[352..1888]
        );
    }
    for fault in [
        FrameFault::LegacyScope,
        FrameFault::UnknownFlags,
        FrameFault::Short208,
        FrameFault::UndoNativeKind,
        FrameFault::UndoGeneration,
    ] {
        let raw = oracle.bad_frame(fault);
        let (mut cursor, mut arena, mut bank, mut after, mut claims) = fresh();
        // INIT faults start from the same independent valid reserve; reserve
        // faults remain at genesis. Never derive identity or role from a mutant.
        let init = raw.len() == 38_208;
        if init {
            cursor
                .apply(
                    profile,
                    FrameExpectation {
                        stream,
                        previous_lsn: 0,
                    },
                    oracle.native_frame(1),
                    &mut arena[..],
                    &mut bank[..],
                    after.as_mut_slice().try_into().unwrap(),
                    claims.as_mut_slice().try_into().unwrap(),
                )
                .unwrap();
        }
        // Materialization may overwrite the discardable after-page loan before
        // final validation. Canonical replay state and its ledger remain unchanged.
        let saved = (arena.clone(), bank.clone(), cursor.reserved());
        assert!(
            cursor
                .apply(
                    profile,
                    FrameExpectation {
                        stream,
                        previous_lsn: u64::from(init)
                    },
                    &raw,
                    &mut arena[..],
                    &mut bank[..],
                    after.as_mut_slice().try_into().unwrap(),
                    claims.as_mut_slice().try_into().unwrap(),
                )
                .is_err()
        );
        assert_eq!(
            (&arena, &bank, cursor.reserved()),
            (&saved.0, &saved.1, saved.2)
        );
    }
    fn census<W: WriterNamespace>(label: &str) {
        let c = resources::source_census::<W>();
        println!(
            "FREE_UNDO_SOURCE {label} read={} decoder={} owner={} retained={} producer={} replay={} history_closure={} seed={} peak={} metadata={} whole={}",
            read::undo_read_source_bytes(),
            read::undo_decode_source_bytes(),
            c.original_owner,
            c.retained,
            c.producer,
            c.replay,
            c.history_closure,
            c.recovered_seed,
            c.peak,
            c.metadata,
            c.whole
        );
        assert!(resources::fits::<W>(), "{c:?}");
        assert_eq!(
            (c.caller_operation, c.caller_writer_replay),
            (114_688, RETENTION_REPLAY_BYTES)
        );
    }
    census::<port::Writer>("memory");
    #[cfg(target_os = "linux")]
    census::<super::super::native::Writer>("native");
}

#[test]
fn free_undo_new_role_preflight_and_install_failure_boundaries() {
    use crate::storage::dirty_frame::{pressure_test_support, test_support as frame_probe};
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::mpsc::sync_channel,
        time::Duration,
    };
    let selected = match std::env::var("XADB_FREE_UNDO_FAILURE_CASES") {
        Ok(text) => {
            assert!(!text.is_empty(), "empty Free Undo failure selection");
            let mut ids = std::collections::BTreeSet::new();
            for id in text.split(',') {
                assert!(
                    id.len() == 1 && (b'0'..=b'6').contains(&id.as_bytes()[0]),
                    "unknown Free Undo failure case {id:?}"
                );
                let case = usize::from(id.as_bytes()[0] - b'0');
                assert!(ids.insert(case), "duplicate Free Undo failure case {case}");
            }
            ids
        }
        Err(std::env::VarError::NotPresent) => (0..7).collect(),
        Err(std::env::VarError::NotUnicode(_)) => panic!("non-UTF8 Free Undo failure selection"),
    };
    println!(
        "FREE_UNDO_FAILURE_SELECTED {}",
        selected
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let oracle = FreeUndoOracle::new();
    for case in 0..7 {
        if !selected.contains(&case) {
            println!("FREE_UNDO_FAILURE_SKIP {case}");
            continue;
        }
        println!("FREE_UNDO_FAILURE_BEGIN {case}");
        original!(|live, owner, core, _foreign, state, storage| {
            let _status = initialize(&mut live, &mut owner, &core, false, page(9, 2), 2);
            prepare(&mut live, &owner, &core, true);
            let index = live.inner.pool.init_payload_index().unwrap();
            let mut untouched = boxed::<PAGE_BYTES>();
            let payload_before = probe::snapshot(&live.inner.pool, index, &mut untouched);
            assert_eq!(
                (
                    payload_before.page_lsn,
                    payload_before.counter,
                    payload_before.rec_lsn
                ),
                (PageLsn(0), 0, None)
            );
            if case <= 1 {
                if case == 0 {
                    // This is coherent counter exhaustion, not a dirty-frame /
                    // DPT mismatch. Keep the actual LSN, recLSN and requirement.
                    let mut bytes = boxed::<PAGE_BYTES>();
                    let mut entry = probe::snapshot(&live.inner.pool, 0, &mut bytes)
                        .entry
                        .unwrap();
                    entry.modification_counter = u64::MAX;
                    frame_probe::seed_counter(&live.inner.pool, 0, u64::MAX);
                    frame_probe::seed_dpt(&live.inner.pool, 0, Some(entry));
                } else {
                    pressure_test_support::duplicate_registration(&mut live.inner.pool);
                }
                let before = (wal_bytes(&state), state.lock().unwrap().mutations());
                let error = match live.reserve_undo(&mut owner) {
                    Ok(_) => panic!("bad source accepted"),
                    Err(e) => e,
                };
                if case == 0 {
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::Exhausted));
                    assert!(!storage.session_fence().is_fenced());
                } else {
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    assert!(matches!(error.cause, RetentionCause::Frame(_)));
                    assert!(storage.session_fence().is_fenced());
                }
                assert_eq!(
                    (wal_bytes(&state), state.lock().unwrap().mutations()),
                    before
                );
                assert_eq!(probe::claim_state(&live.inner.pool), (false, 0));
                assert_eq!(live.control.population.test_state(), (false, false));
            } else if case == 2 {
                let token = live.reserve_undo(&mut owner).unwrap();
                assert_eq!(token.page(), page(10, 2));
                drop(token);
                assert!(storage.session_fence().is_fenced());
                assert_eq!(live.control.population.test_state(), (true, false));
                assert_eq!(probe::claim_state(&live.inner.pool), (false, 0));
                let raw = wal_bytes(&state);
                assert_eq!(&raw[63_296..], oracle.native_frame(3));
                assert_eq!(raw.len(), 84_496);
            } else {
                if matches!(case, 3 | 4) {
                    let point = if case == 3 {
                        probe::InitPoint::AllocatorImageInstalled
                    } else {
                        probe::InitPoint::PayloadImageInstalled
                    };
                    let (arrived, arrival) = sync_channel(1);
                    let (resume, release) = sync_channel(1);
                    let (fenced, fence_arrival) = sync_channel(1);
                    let (unwind, unwind_release) = sync_channel(1);
                    probe::install_init_pause(&live.inner.pool, point, arrived, release);
                    frame_probe::install_fence_pause(&live.inner.pool, fenced, unwind_release);
                    let fence = storage.session_fence();
                    let token = live.reserve_undo(&mut owner).unwrap();
                    state.lock().unwrap().events.clear();
                    std::thread::scope(|scope| {
                        let core_ref = &core;
                        let controller = scope.spawn(move || {
                            arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                            assert!(!fence.is_fenced());
                            assert!(core_ref.diagnostic_version_busy());
                            resume.try_send(frame_probe::Resume::Panic).unwrap();
                            fence_arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                            assert!(fence.is_fenced());
                            assert!(
                                core_ref.diagnostic_version_busy(),
                                "fence precedes version release"
                            );
                            unwind.try_send(frame_probe::Resume::Continue).unwrap();
                        });
                        assert!(
                            catch_unwind(AssertUnwindSafe(|| {
                                let _ = token.initialize();
                            }))
                            .is_err()
                        );
                        controller.join().unwrap();
                    });
                    assert!(
                        state.lock().unwrap().events.is_empty(),
                        "RAM cut precedes physical WAL append"
                    );
                } else {
                    let token = live.reserve_undo(&mut owner).unwrap();
                    {
                        let mut s = state.lock().unwrap();
                        s.events.clear();
                        s.fault_hits = 0;
                        s.fault = Some(port::Fault {
                            key: port::Key::new(
                                if case == 5 { "write" } else { "sync" },
                                crate::storage::wal_io::WAL_NAME,
                                if case == 5 { 84_496 } else { 0 },
                                1,
                            ),
                            action: if case == 5 {
                                port::Action::PrefixError
                            } else {
                                port::Action::AfterError
                            },
                        });
                    }
                    let error = match token.initialize() {
                        Err(RetentionReservationError::Fatal(e)) => e,
                        _ => panic!("uncertain INIT durability must retain fatal ownership"),
                    };
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Durable, Effect::CallerMustResolve)
                    );
                    let RetentionCause::Wal(error) = error.cause else {
                        panic!("original WAL cause")
                    };
                    let crate::storage::wal_io::Cause::Io(error) = error.cause else {
                        panic!("original I/O cause")
                    };
                    assert_eq!(error.certainty(), foundation::error::Certainty::Unknown);
                    assert_eq!(error.effect(), Effect::CallerMustResolve);
                    assert_eq!(
                        error.retry_scope(),
                        foundation::error::RetryScope::AfterRecovery
                    );
                    assert_eq!(state.lock().unwrap().fault_hits, 1);
                }
                assert!(storage.session_fence().is_fenced());
                assert!(!core.diagnostic_version_busy());
                let mut allocator = boxed::<PAGE_BYTES>();
                let a = probe::snapshot(&live.inner.pool, 0, &mut allocator);
                let mut payload = boxed::<PAGE_BYTES>();
                let p = probe::snapshot(&live.inner.pool, index, &mut payload);
                assert_eq!(allocator.as_slice(), oracle.native_allocator(4));
                assert_eq!(
                    payload.as_slice(),
                    if case == 3 {
                        untouched.as_slice()
                    } else {
                        oracle.native_page(10)
                    }
                );
                let metadata = case >= 5;
                assert_eq!(
                    (a.page_lsn, p.page_lsn),
                    (PageLsn(4), PageLsn(if case == 3 { 0 } else { 4 }))
                );
                assert_eq!(
                    (a.counter, p.counter),
                    if metadata { (4, 1) } else { (3, 0) }
                );
                assert_eq!(
                    (a.rec_lsn, p.rec_lsn),
                    (Some(PageLsn(1)), metadata.then_some(PageLsn(4)))
                );
                assert!(a.dirty);
                assert_eq!(p.dirty, metadata);
                assert_eq!(a.requirement.unwrap().lsn, if metadata { 4 } else { 3 });
                assert_eq!(p.requirement.map(|r| r.lsn), metadata.then_some(4));
                assert_eq!((a.total, p.total), (0, 0));
                assert!(a.claim_present && p.claim_present);
                assert!(!a.initializer_pins_held && !p.initializer_pins_held);
                assert_eq!(live.control.population.test_state(), (true, true));
                assert_eq!(live.control.pending.unwrap().location().lsn, 4);
                assert_eq!(
                    live.control
                        .durable
                        .as_ref()
                        .unwrap()
                        .observation()
                        .frame
                        .lsn,
                    3
                );
                assert_eq!(
                    &live.control.row(control::ALLOCATION_SLOT)[..1536],
                    oracle.native_descriptor(if metadata { 4 } else { 3 })
                );
                let append = storage
                    .wal()
                    .append
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                assert_eq!(
                    (append.prefix.complete_lsn, append.prefix.synced_lsn),
                    (4, 3)
                );
                drop(append);
                let raw = wal_bytes(&state);
                let expected_len = match case {
                    3 | 4 => 84_496,
                    5 => 84_496 + 8192,
                    _ => 122_704,
                };
                assert_eq!(raw.len(), expected_len);
                assert_eq!(
                    &raw[84_496..],
                    &oracle.native_frame(4)[..expected_len - 84_496]
                );
                let s = state.lock().unwrap();
                assert!(
                    s.files["d3-data-0000000100000001"].bytes[10 * PAGE_BYTES..11 * PAGE_BYTES]
                        .iter()
                        .all(|b| *b == 0)
                );
            }
        });
        println!("FREE_UNDO_FAILURE_PASS {case}");
    }
}
