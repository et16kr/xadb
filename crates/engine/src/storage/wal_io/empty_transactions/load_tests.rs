//! Only new occupied-home replacement cases; prior fixture APIs are setup.
use super::super::load as product;
use super::*;

macro_rules! cold_fixture {
    ($profile:expr, |$live:ident,$core:ident,$state:ident,$storage:ident,$pages:ident,$owner:ident $(,$foreign:ident)?| $body:block) => {
        fixture!(
            2,
            $profile,
            |$live, $core, $state, $storage, $pages, $owner $(,$foreign)?| $body,
            prepare | provision | {
                let mut dedicated = $core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                for page in &$pages {
                    provision
                        .split_with_flush()
                        .2
                        .try_flush_page(&mut dedicated, *page)
                        .unwrap();
                }
                provision.prepare_slot(1).unwrap();
                let page = match provision
                    .split_with_flush()
                    .0
                    .reserve_one(&mut $owner)
                    .unwrap()
                    .initialize()
                {
                    Ok(v) => v.page,
                    Err(_) => panic!("third INIT"),
                };
                $pages.push(page);
                provision
                    .split_with_flush()
                    .2
                    .try_flush_page(&mut dedicated, page)
                    .unwrap();
            }
        );
    };
}

#[test]
fn empty_load_resource_gate() {
    type W = port::Writer;
    let retained = super::super::super::retained_bytes::<W>();
    println!(
        "load_call={} retained={} metadata={} whole={} conversion={}",
        product::call_bytes::<W>(),
        retained,
        retained + product::call_bytes::<W>(),
        retained + product::call_bytes::<W>() + 65536,
        super::super::super::conversion_bytes::<W>()
    );
    assert!(product::fits::<W>());
    assert!(product::fits::<platform::namespace::LinuxWriterLease>());
}

#[test]
fn empty_load_occupied_round_trip_continues_original_actor() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let actor = live.begin(&mut owner, pages[1]).unwrap().actor;
        for step in 0..3 {
            if step == 1 {
                live.rollback_start(&mut owner, actor).unwrap();
            }
            if step == 2 {
                live.abort_end(&mut owner, actor).unwrap();
            }
            live.flush_status(&mut dedicated, pages[1]).unwrap();
            let index = if step % 2 == 0 { 2 } else { 1 };
            let other = if index == 2 { 1 } else { 2 };
            let expected = page_bytes(&live, index);
            let bank = live.bank.to_vec();
            let prefix = storage.wal().prefix().unwrap();
            let effects = state.lock().unwrap().events.len();
            // Free A replaces clean occupied B; B then replaces the other slot.
            let replacement = if step % 2 == 0 { pages[0] } else { pages[2] };
            live.load_into(&mut owner, index, replacement).unwrap();
            let observation = live.load_into(&mut owner, other, pages[1]).unwrap();
            assert_eq!(observation.after.identity.page, pages[1]);
            assert_eq!(observation.after.rec_lsn, None);
            assert_eq!(page_bytes(&live, other), expected);
            assert_eq!(live.bank.as_slice(), bank);
            assert_eq!(storage.wal().prefix().unwrap(), prefix);
            assert_eq!(state.lock().unwrap().events.len(), effects);
        }
        assert!(live.bank.iter().all(|b| *b == 0));
        assert!(!storage.session_fence().is_fenced());
    });
}

#[derive(Debug, PartialEq, Eq)]
struct LoadPoolSlot {
    frame: FrameStamp,
    dirty: bool,
    entry: Option<dirty_frame::DirtyEntry>,
    pins: Vec<u16>,
    total: u16,
    bytes: Vec<u8>,
}
fn pool_state<W: WriterNamespace>(
    live: &EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> Vec<LoadPoolSlot> {
    (0..3)
        .map(|index| {
            let mut bytes = boxed::<PAGE_BYTES>(0);
            let metadata = dirty_frame::test_support::diagnose(
                &live.inner.inner.inner.pool,
                index,
                &mut bytes,
            );
            LoadPoolSlot {
                frame: metadata.frame,
                dirty: metadata.dirty,
                entry: metadata.entry,
                pins: metadata.pins.to_vec(),
                total: metadata.total,
                bytes: bytes.to_vec(),
            }
        })
        .collect()
}

#[test]
fn empty_load_admission_preserves_dirty_and_clean_destinations() {
    cold_fixture!(1, |live, core, state, storage, pages, owner, foreign| {
        let mut foreign_owner = foreign.try_owner(OwnerRole::Maintenance).unwrap();
        let mut missing = pages[0];
        missing.page_no = 63;
        for variant in 0..5 {
            let before = pool_state(&live);
            let bank = live.bank.to_vec();
            let error = match variant {
                0 => live.load_into(&mut owner, 0, pages[0]),
                1 => live.load_into(&mut owner, 3, pages[0]),
                2 => live.load_into(&mut owner, 2, pages[1]),
                3 => live.load_into(&mut owner, 2, missing),
                4 => live.load_into(&mut foreign_owner, 2, pages[0]),
                _ => unreachable!(),
            }
            .unwrap_err();
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(pool_state(&live), before);
            assert_eq!(live.bank.as_slice(), bank);
            assert_eq!(state.lock().unwrap().reads, 0);
            assert!(state.lock().unwrap().events.is_empty());
        }
        live.begin(&mut owner, pages[1]).unwrap();
        let before = pool_state(&live);
        let bank = live.bank.to_vec();
        let counts = {
            let guard = state.lock().unwrap();
            (guard.events.len(), guard.reads)
        };
        assert_eq!(
            live.load_into(&mut owner, 2, pages[0]).unwrap_err().effect,
            Effect::Unchanged
        );
        assert_eq!(pool_state(&live), before);
        assert_eq!(live.bank.as_slice(), bank);
        let guard = state.lock().unwrap();
        assert_eq!((guard.events.len(), guard.reads), counts);
        assert!(!storage.session_fence().is_fenced());
    });
}

// A fresh control inside each fault test fixes the three selected-home read
// boundaries (directory, pre-history, final). Missing/extra reads fail validity.
fn home_read_boundaries() -> [usize; 3] {
    let boundaries;
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        live.load_into(&mut owner, 2, pages[0]).unwrap();
        let guard = state.lock().unwrap();
        let reads: Vec<_> = guard
            .read_events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.name == "d3-data-0000000100000001"
                    && e.offset == PAGE_BYTES as u64
                    && e.length == PAGE_BYTES
            })
            .map(|(index, _)| index)
            .collect();
        boundaries = <[usize; 3]>::try_from(reads).unwrap();
        assert!(guard.events.is_empty());
        assert!(!storage.session_fence().is_fenced());
        let _ = &core;
    });
    boundaries
}

#[test]
fn empty_load_read_errors_preserve_destination_and_retry() {
    let boundaries = home_read_boundaries();
    for at in [boundaries[0], boundaries[2]] {
        for action in [port::Action::BeforeError, port::Action::PrefixError] {
            cold_fixture!(1, |live, core, state, storage, pages, owner| {
                let before = pool_state(&live);
                let bank = live.bank.to_vec();
                let prefix = storage.wal().prefix().unwrap();
                state.lock().unwrap().read_plan = Some(port::Plan { at, action });
                assert_eq!(
                    live.load_into(&mut owner, 2, pages[0]).unwrap_err().effect,
                    Effect::Unchanged
                );
                assert_eq!(pool_state(&live), before);
                assert_eq!(live.bank.as_slice(), bank);
                assert_eq!(storage.wal().prefix().unwrap(), prefix);
                assert!(!storage.session_fence().is_fenced());
                let mut guard = state.lock().unwrap();
                assert_eq!(guard.configured_read_hits, 1);
                assert!(guard.events.is_empty());
                guard.read_plan = None;
                drop(guard);
                live.load_into(&mut owner, 2, pages[0]).unwrap();
                assert!(state.lock().unwrap().events.is_empty());
                println!("EMPTY_LOAD_RETRY_PASS {at} {action:?}");
                let _ = &core;
            });
        }
    }
}

#[test]
fn empty_load_required_eof_invalid_count_and_corrupt_home_fence() {
    let boundaries = home_read_boundaries();
    for at in [boundaries[0], boundaries[2]] {
        for action in [
            port::Action::Zero,
            port::Action::PrefixEof,
            port::Action::InvalidCount,
        ] {
            cold_fixture!(1, |live, core, state, storage, pages, owner| {
                let before = pool_state(&live);
                let bank = live.bank.to_vec();
                state.lock().unwrap().read_plan = Some(port::Plan { at, action });
                assert_eq!(
                    live.load_into(&mut owner, 2, pages[0]).unwrap_err().effect,
                    Effect::CallerMustResolve
                );
                assert!(storage.session_fence().is_fenced());
                assert_eq!(pool_state(&live), before);
                assert_eq!(live.bank.as_slice(), bank);
                let guard = state.lock().unwrap();
                assert_eq!(guard.configured_read_hits, 1);
                assert!(guard.events.is_empty());
                println!("EMPTY_LOAD_SOURCE_ERROR_PASS {at} {action:?}");
                let _ = &core;
            });
        }
    }
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let before = pool_state(&live);
        state
            .lock()
            .unwrap()
            .files
            .get_mut("d3-data-0000000100000001")
            .unwrap()
            .bytes[PAGE_BYTES + 130] ^= 1;
        assert_eq!(
            live.load_into(&mut owner, 2, pages[0]).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert!(storage.session_fence().is_fenced());
        assert_eq!(pool_state(&live), before);
        assert!(state.lock().unwrap().events.is_empty());
        let _ = &core;
    });
}

#[test]
fn empty_load_rejects_historical_free_after_terminal_eviction() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let home_name = "d3-data-0000000100000001";
        let old =
            state.lock().unwrap().files[home_name].bytes[2 * PAGE_BYTES..3 * PAGE_BYTES].to_vec();
        let actor = live.begin(&mut owner, pages[1]).unwrap().actor;
        live.rollback_start(&mut owner, actor).unwrap();
        live.abort_end(&mut owner, actor).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        live.flush_status(&mut dedicated, pages[1]).unwrap();
        live.load_into(&mut owner, 2, pages[0]).unwrap();
        state
            .lock()
            .unwrap()
            .files
            .get_mut(home_name)
            .unwrap()
            .bytes[2 * PAGE_BYTES..3 * PAGE_BYTES]
            .copy_from_slice(&old);
        let prefix = storage.wal().prefix().unwrap();
        assert!(live.bank.iter().all(|b| *b == 0));
        // The broad directory is valid; only exact latest WAL history rejects
        // this real but obsolete Free image after the terminal bank was freed.
        live.check_directory(&mut owner, prefix, None, 0).unwrap();
        let before = pool_state(&live);
        let effects = state.lock().unwrap().events.len();
        assert_eq!(
            live.load_into(&mut owner, 1, pages[1]).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert!(storage.session_fence().is_fenced());
        assert_eq!(pool_state(&live), before);
        assert!(live.bank.iter().all(|b| *b == 0));
        assert_eq!(state.lock().unwrap().events.len(), effects);
        assert_eq!(storage.wal().append.lock().unwrap().prefix, prefix);
    });
}

#[test]
fn empty_load_final_read_drift_rejects_after_complete_history() {
    let final_read = home_read_boundaries()[2];
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let before = pool_state(&live);
        let mut changed = state.lock().unwrap().files["d3-data-0000000100000001"]
            .bytes
            .clone();
        changed[PAGE_BYTES + 64..PAGE_BYTES + 72].copy_from_slice(&1u64.to_le_bytes());
        history_reseal_page(&mut changed[PAGE_BYTES..2 * PAGE_BYTES]);
        let expected_read = changed[PAGE_BYTES..2 * PAGE_BYTES].to_vec();
        state.lock().unwrap().read_edits.push((
            final_read,
            "d3-data-0000000100000001".into(),
            changed,
        ));
        assert_eq!(
            live.load_into(&mut owner, 2, pages[0]).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert!(storage.session_fence().is_fenced());
        assert_eq!(pool_state(&live), before);
        assert!(live.bank.iter().all(|b| *b == 0));
        let guard = state.lock().unwrap();
        assert!(guard.events.is_empty());
        assert!(guard.read_edits.is_empty());
        assert_eq!(guard.read_events[final_read].data, expected_read);
        assert_eq!(
            guard.read_events[..final_read]
                .iter()
                .filter(|e| e.name == wal::WAL_NAME)
                .count(),
            12
        );
        assert_eq!(guard.reads, final_read + 1);
        let _ = &core;
    });
}

#[test]
fn empty_load_forgotten_pin_and_poison_prevent_io() {
    for poison in [false, true] {
        cold_fixture!(1, |live, core, state, storage, pages, owner| {
            let bank = live.bank.to_vec();
            let mut reader = core.try_owner(OwnerRole::Foreground).unwrap();
            if poison {
                dirty_frame::test_support::poison_dpt(&live.inner.inner.inner.pool);
            } else {
                let request = live.inner.inner.inner.pool.target_request(1).unwrap();
                let held = live
                    .inner
                    .inner
                    .inner
                    .pool
                    .reserve_target_set(&mut reader, &[request])
                    .unwrap()
                    .try_read()
                    .unwrap();
                std::mem::forget(held);
            }
            let error = live.load_into(&mut owner, 2, pages[0]).unwrap_err();
            assert_eq!(
                error.effect,
                if poison {
                    Effect::CallerMustResolve
                } else {
                    Effect::Unchanged
                }
            );
            assert_eq!(storage.session_fence().is_fenced(), poison);
            assert_eq!(live.bank.as_slice(), bank);
            let guard = state.lock().unwrap();
            assert_eq!(guard.reads, 0);
            assert!(guard.events.is_empty());
        });
    }
}

#[test]
fn empty_load_retained_flush_claim_prevents_io() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut scratch = boxed::<PAGE_BYTES>(0);
        let request = live.inner.inner.inner.pool.target_request(0).unwrap();
        let capture = live
            .inner
            .inner
            .inner
            .pool
            .capture_flush(&mut flusher, request, &mut scratch)
            .unwrap()
            .unwrap();
        // The pool owns this claim; losing its non-Drop token must not release it.
        #[allow(clippy::forget_non_drop)]
        std::mem::forget(capture);
        let before = pool_state(&live);
        let bank = live.bank.to_vec();
        let prefix = storage.wal().prefix().unwrap();
        let error = live.load_into(&mut owner, 2, pages[0]).unwrap_err();
        assert!(matches!(error.cause, Cause::Busy));
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(!storage.session_fence().is_fenced());
        assert_eq!(pool_state(&live), before);
        assert_eq!(live.bank.as_slice(), bank);
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        let guard = state.lock().unwrap();
        assert_eq!(guard.reads, 0);
        assert!(guard.events.is_empty());
    });
}

#[test]
fn empty_load_read_unwind_fences_without_installation() {
    let at = home_read_boundaries()[2];
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let before = pool_state(&live);
        state.lock().unwrap().read_plan = Some(port::Plan {
            at,
            action: port::Action::BeforePanic,
        });
        assert!(
            catch_unwind(AssertUnwindSafe(|| live.load_into(&mut owner, 2, pages[0]))).is_err()
        );
        assert!(storage.session_fence().is_fenced());
        assert_eq!(pool_state(&live), before);
        assert!(live.bank.iter().all(|b| *b == 0));
        let guard = state.lock().unwrap();
        assert_eq!(guard.hit, 1);
        assert!(guard.events.is_empty());
        let _ = &core;
    });
}

#[test]
#[ignore = "DEV3 S5 expanded live-load profile/state combinations"]
fn empty_load_s5_profiles_states_and_replacement() {
    for profile in 1..=3 {
        for depth in 0..4 {
            cold_fixture!(profile, |live, core, state, storage, pages, owner| {
                let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let actor = (depth > 0).then(|| live.begin(&mut owner, pages[1]).unwrap().actor);
                if depth > 1 {
                    live.rollback_start(&mut owner, actor.unwrap()).unwrap();
                }
                if depth > 2 {
                    live.abort_end(&mut owner, actor.unwrap()).unwrap();
                }
                if depth > 0 {
                    live.flush_status(&mut dedicated, pages[1]).unwrap();
                }
                let expected = page_bytes(&live, 2);
                live.load_into(&mut owner, 2, pages[0]).unwrap();
                live.load_into(&mut owner, 1, pages[1]).unwrap();
                assert_eq!(page_bytes(&live, 1), expected);
                assert!(!storage.session_fence().is_fenced());
                let _ = &state;
                println!("EMPTY_LOAD_S5_PASS {profile} {depth}");
            });
        }
    }
}
