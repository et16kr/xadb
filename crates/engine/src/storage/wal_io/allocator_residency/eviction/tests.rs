use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, residency_test_support as resident, test_support as frame_test},
    page::PageLsn,
    wal_io::{FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
macro_rules! fixture {
    (|$bound:ident, $core:ident, $state:ident| $run:block) => { fixture!(3; |$bound, $core, $state| $run) };
    ($count:expr; |$bound:ident, $core:ident, $state:ident| $run:block) => {{
        let mut slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>();
        let mut pages: Vec<_> = (0..$count).map(|_| boxed::<PAGE_BYTES>()).collect();
        let mut operation = boxed::<98304>();
        let (writer, $state) = port::fresh();
        let fresh =
            FreshStorageSession::create(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let storage = fresh
            .install_doublewrite(&mut workspace)
            .unwrap()
            .into_mutable_storage();
        let $core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut frames: Vec<_> = pages.iter_mut().map(|p| DirtyFrame::new(p)).collect();
        let bound = storage
            .bind_allocator(&$core, &mut frames, &mut operation)
            .unwrap();
        let mut $bound = match bound.into_operational() {
            Ok(v) => v,
            Err(_) => panic!("conversion"),
        };
        $run
    }};
}
fn flush<'s, 'c>(
    bound: &mut OperationalAllocation<'s, 'c, '_, '_, '_, '_, '_, port::Writer>,
    core: &'c PageConcurrency<'s>,
) {
    let (_, _, mut flusher) = bound.split_with_flush();
    let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    assert!(matches!(
        flusher.try_flush_one(&mut owner).unwrap(),
        allocator_flush::FlushProgress::Complete(_)
    ));
}
fn no_target<'s, 'c>(
    guard: &EvictedAllocator<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    core: &'c PageConcurrency<'s>,
) {
    assert_eq!(resident::resident_slot(&guard.operational.inner.pool), None);
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let error = match guard
        .operational
        .inner
        .pool
        .reserve_targets(&mut owner, &[guard.snapshot.request()])
    {
        Err(e) => e,
        Ok(_) => panic!("nonresident hit"),
    };
    assert_eq!(
        error,
        dirty_frame::DirtyError {
            cause: dirty_frame::Cause::MissingTarget,
            effect: Effect::Unchanged
        }
    );
}

#[test]
fn eviction_layout_gate() {
    assert!(super::fits::<port::Writer>());
    assert!(super::super::fits::<port::Writer>());
    println!(
        "EVICTION_LAYOUT guard={} snapshot={} claim={} pool_delta={}",
        size_of::<EvictedAllocator<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>>(),
        size_of::<ReloadSnapshot<'_, '_>>(),
        size_of::<dirty_frame::AllocatorLoadClaim<'_, '_>>(),
        dirty_frame::init_pool_delta_bytes()
    );
}

#[test]
fn eviction_moves_authority_and_writer_flush_follow_identity() {
    fixture!(|bound, core, state| {
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        {
            let (mut writer, _, _) = bound.split_with_flush();
            writer.reserve_pages(&mut owner, 1).unwrap();
        }
        flush(&mut bound, &core);
        let descriptor = bound.inner.writer.descriptor;
        let files = state
            .lock()
            .unwrap()
            .files
            .iter()
            .map(|(n, f)| (n.clone(), f.bytes.clone()))
            .collect::<Vec<_>>();
        let mut guard = bound.evict_allocator().unwrap();
        no_target(&guard, &core);
        let moved = guard.load_allocator(2).unwrap();
        assert_eq!(
            (
                moved.before.generation,
                moved.after.generation,
                moved.after.page_lsn
            ),
            (1, 1, PageLsn(1))
        );
        let reads = state.lock().unwrap().reads;
        assert_eq!(
            guard.load_allocator(1).unwrap_err().effect,
            Effect::Unchanged
        );
        assert_eq!(state.lock().unwrap().reads, reads);
        drop(guard);
        assert_eq!(bound.inner.writer.descriptor, descriptor);
        assert_eq!(resident::resident_slot(&bound.inner.pool), Some(2));
        assert_eq!(
            files,
            state
                .lock()
                .unwrap()
                .files
                .iter()
                .map(|(n, f)| (n.clone(), f.bytes.clone()))
                .collect::<Vec<_>>()
        );
        {
            let (mut writer, _, _) = bound.split_with_flush();
            writer.cancel_reserved(&mut owner).unwrap();
        }
        flush(&mut bound, &core);
        let reloaded = bound.reload_allocator().unwrap();
        assert_eq!(
            (reloaded.before.generation, reloaded.after.generation),
            (1, 2)
        );
        let mut guard = bound.evict_allocator().unwrap();
        no_target(&guard, &core);
        let back = guard.load_allocator(0).unwrap();
        assert_eq!(
            (back.after.generation, back.after.page_lsn),
            (2, PageLsn(2))
        );
        drop(guard);
        assert!(!bound.inner.storage.session_fence().is_fenced());
    });
}

#[test]
fn eviction_zero_lsn_and_source_max_can_load_fresh_slot() {
    fixture!(|bound, core, state| {
        let _ = &state;
        resident::seed_generation(&mut bound.inner.pool, u64::MAX);
        let mut guard = bound.evict_allocator().unwrap();
        no_target(&guard, &core);
        assert_eq!(
            guard.load_allocator(0).unwrap_err().effect,
            Effect::Unchanged
        );
        let result = guard.load_allocator(1).unwrap();
        assert_eq!(
            (result.before.generation, result.after.generation),
            (u64::MAX, 1)
        );
        assert_eq!(
            (result.after.page_lsn, result.after.modification_counter),
            (PageLsn(0), 0)
        );
    });
}

#[test]
fn eviction_bounds_refusal_precedes_claim_and_reads() {
    fixture!(|bound, core, state| {
        let mut guard = bound.evict_allocator().unwrap();
        let reads = state.lock().unwrap().reads;
        for target in [3, 64, usize::MAX] {
            assert_eq!(
                guard.load_allocator(target).unwrap_err().effect,
                Effect::Unchanged
            );
            no_target(&guard, &core);
        }
        assert_eq!(state.lock().unwrap().reads, reads);
        assert_eq!(guard.load_allocator(1).unwrap().after.generation, 1);
    });
}

#[test]
fn eviction_read_failures_burn_generation_preserve_prior_and_retry() {
    for offset in 0..10 {
        fixture!(|bound, core, state| {
            let descriptor = bound.inner.writer.descriptor;
            let mut guard = bound.evict_allocator().unwrap();
            let prior = guard.operational.inner.writer.operation[32768..49152].to_vec();
            let start = state.lock().unwrap().reads;
            state.lock().unwrap().read_plan = Some(port::Plan {
                at: start + offset,
                action: port::Action::AfterError,
            });
            let error = guard.load_allocator(1).unwrap_err();
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(state.lock().unwrap().hit, 1);
            no_target(&guard, &core);
            assert_eq!(
                &guard.operational.inner.writer.operation[32768..49152],
                prior
            );
            // Deliberately damage retry scratch: it must never become the oracle.
            guard.operational.inner.writer.operation[65536..81920].fill(0xa5);
            state.lock().unwrap().read_plan = None;
            let loaded = guard.load_allocator(1).unwrap();
            assert_eq!(loaded.after.generation, 2);
            assert_eq!(guard.operational.inner.writer.descriptor, descriptor);
        });
    }
}

#[test]
fn eviction_dirty_pin_and_claim_refuse_without_io() {
    for variant in 0..3 {
        fixture!(|bound, core, state| {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            match variant {
                0 => {
                    let (mut writer, _, _) = bound.split_with_flush();
                    writer.reserve_pages(&mut owner, 1).unwrap();
                }
                1 => {
                    let request = bound.inner.pool.target_request(0).unwrap();
                    std::mem::forget(
                        bound
                            .inner
                            .pool
                            .reserve_targets(&mut owner, &[request])
                            .unwrap(),
                    );
                }
                2 => frame_test::seed_uncaptured_claim(&bound.inner.pool),
                _ => unreachable!(),
            }
            let reads = state.lock().unwrap().reads;
            let error = match bound.evict_allocator() {
                Err(e) => e,
                Ok(_) => panic!("busy eviction"),
            };
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(state.lock().unwrap().reads, reads);
            assert_eq!(resident::resident_slot(&bound.inner.pool), Some(0));
        });
    }
}

#[test]
fn eviction_source_corruption_fences_and_keeps_empty() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let mut guard = bound.evict_allocator().unwrap();
        state
            .lock()
            .unwrap()
            .files
            .get_mut("d3-data-0000000100000001")
            .unwrap()
            .bytes[0] ^= 1;
        assert_eq!(
            guard.load_allocator(1).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert_eq!(resident::resident_slot(&guard.operational.inner.pool), None);
        assert!(!guard.complete);
    });
}

#[test]
fn eviction_destination_poison_precedes_read() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let mut guard = bound.evict_allocator().unwrap();
        frame_test::poison_content(&guard.operational.inner.pool, 1);
        let reads = state.lock().unwrap().reads;
        assert_eq!(
            guard.load_allocator(1).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert_eq!(state.lock().unwrap().reads, reads);
    });
}

#[test]
fn eviction_drop_and_unwind_fence_without_io() {
    for unwind in [false, true] {
        fixture!(|bound, core, state| {
            let _ = &core;
            let reads = state.lock().unwrap().reads;
            let result = catch_unwind(AssertUnwindSafe(|| {
                let _guard = bound.evict_allocator().unwrap();
                if unwind {
                    panic!("eviction unwind");
                }
            }));
            assert_eq!(result.is_err(), unwind);
            assert!(bound.inner.storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().reads, reads);
        });
    }
}

#[test]
fn eviction_forgotten_guard_cannot_write_reload_or_rebind() {
    fixture!(|bound, core, state| {
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let descriptor = bound.inner.writer.descriptor;
        std::mem::forget(bound.evict_allocator().unwrap());
        let reads = state.lock().unwrap().reads;
        {
            let (mut writer, _, _) = bound.split_with_flush();
            assert_eq!(
                writer.reserve_pages(&mut owner, 1).unwrap_err().effect,
                Effect::Unchanged
            );
        }
        assert_eq!(
            bound.reload_allocator().unwrap_err().effect,
            Effect::Unchanged
        );
        assert_eq!(bound.inner.writer.descriptor, descriptor);
        assert_eq!(state.lock().unwrap().reads, reads);
        let mut page = boxed::<PAGE_BYTES>();
        let mut frames = [DirtyFrame::new(&mut page)];
        let mut operation = boxed::<98304>();
        assert!(
            bound
                .inner
                .storage
                .bind_allocator(&core, &mut frames, &mut operation)
                .is_err()
        );
        assert_eq!(resident::resident_slot(&bound.inner.pool), None);
    });
}

#[test]
fn eviction_stale_load_claim_cannot_publish() {
    fixture!(|bound, core, state| {
        let _ = (&core, &state);
        let guard = bound.evict_allocator().unwrap();
        let stale = guard
            .operational
            .inner
            .pool
            .claim_allocator_load(&guard.snapshot, 1)
            .unwrap();
        let _current = guard
            .operational
            .inner
            .pool
            .claim_allocator_load(&guard.snapshot, 1)
            .unwrap();
        let prior: &[u8; PAGE_BYTES] = (&guard.operational.inner.writer.operation[32768..49152])
            .try_into()
            .unwrap();
        let permit = ReloadInstallPermit {
            snapshot: &guard.snapshot,
            bytes: prior,
        };
        assert_eq!(
            guard
                .operational
                .inner
                .pool
                .install_evicted_allocator(stale, permit)
                .unwrap_err()
                .effect,
            Effect::CallerMustResolve
        );
        assert_eq!(resident::resident_slot(&guard.operational.inner.pool), None);
    });
}

#[test]
fn eviction_epoch_exhaustion_precedes_removal() {
    fixture!(|bound, core, state| {
        let _ = &core;
        resident::seed_eviction_epoch(&mut bound.inner.pool, u64::MAX);
        let reads = state.lock().unwrap().reads;
        let error = match bound.evict_allocator() {
            Err(e) => e,
            Ok(_) => panic!("epoch wrapped"),
        };
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(resident::resident_slot(&bound.inner.pool), Some(0));
        assert_eq!(state.lock().unwrap().reads, reads);
    });
}

#[test]
#[ignore = "DEV3 S5 expanded three-slot lifecycle; not an intermediate test"]
fn eviction_s5_three_slot_history() {
    fixture!(|bound, core, state| {
        let _ = &state;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        for round in 0..32 {
            {
                let (mut writer, _, _) = bound.split_with_flush();
                writer.reserve_pages(&mut owner, 1).unwrap();
            }
            flush(&mut bound, &core);
            {
                let mut guard = bound.evict_allocator().unwrap();
                guard.load_allocator((round + 1) % 3).unwrap();
            }
            {
                let (mut writer, _, _) = bound.split_with_flush();
                writer.cancel_reserved(&mut owner).unwrap();
            }
            flush(&mut bound, &core);
            {
                let mut guard = bound.evict_allocator().unwrap();
                guard.load_allocator((round + 2) % 3).unwrap();
            }
        }
    });
}

#[test]
fn eviction_last_slot_bit_and_reload_are_valid() {
    fixture!(64; |bound, core, state| {
        let _ = (&core, &state);
        { let mut guard = bound.evict_allocator().unwrap(); assert_eq!(guard.load_allocator(63).unwrap().after.generation, 1); }
        assert_eq!(resident::resident_slot(&bound.inner.pool), Some(63));
        assert_eq!(bound.reload_allocator().unwrap().after.generation, 2);
        { let mut guard = bound.evict_allocator().unwrap(); assert_eq!(guard.load_allocator(0).unwrap().after.generation, 2); }
        assert_eq!(resident::resident_slot(&bound.inner.pool), Some(0));
    });
}

#[test]
fn eviction_older_valid_home_is_not_published() {
    fixture!(|bound, core, state| {
        let home = "d3-data-0000000100000001";
        let pristine = state.lock().unwrap().files[home].bytes[..PAGE_BYTES].to_vec();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        {
            let (mut writer, _, _) = bound.split_with_flush();
            writer.reserve_pages(&mut owner, 1).unwrap();
        }
        flush(&mut bound, &core);
        let mut guard = bound.evict_allocator().unwrap();
        let descriptor = guard.operational.inner.writer.descriptor;
        state.lock().unwrap().files.get_mut(home).unwrap().bytes[..PAGE_BYTES]
            .copy_from_slice(&pristine);
        let error = guard.load_allocator(1).unwrap_err();
        assert!(matches!(error.cause, Cause::HomeMismatch));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert_eq!(resident::resident_slot(&guard.operational.inner.pool), None);
        assert_eq!(guard.operational.inner.writer.descriptor, descriptor);
    });
}

#[test]
fn eviction_changed_epoch_rejects_saved_receipt_before_read() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let mut guard = bound.evict_allocator().unwrap();
        let reads = state.lock().unwrap().reads;
        resident::seed_eviction_epoch(&mut guard.operational.inner.pool, 2);
        assert_eq!(
            guard.load_allocator(1).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert_eq!(state.lock().unwrap().reads, reads);
    });
}

#[test]
fn eviction_common_fence_during_load_prevents_publication() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let storage = bound.inner.storage;
        let mut guard = bound.evict_allocator().unwrap();
        let (entered, arrival) = std::sync::mpsc::sync_channel(0);
        let (release, resume) = std::sync::mpsc::sync_channel(0);
        {
            let mut s = state.lock().unwrap();
            s.read_pause = Some(port::Pause {
                at: s.reads + 9,
                after: false,
                entered,
                release: resume,
                unwind: false,
            });
        }
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                arrival
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                storage.session_fence().fence();
                release.send(()).unwrap();
            });
            assert_eq!(
                guard.load_allocator(1).unwrap_err().effect,
                Effect::CallerMustResolve
            );
            worker.join().unwrap();
        });
        assert_eq!(resident::resident_slot(&guard.operational.inner.pool), None);
    });
}

#[test]
fn eviction_foreign_origin_claim_cannot_publish() {
    let foreign_binding = std::sync::atomic::AtomicU8::new(2);
    fixture!(|bound, core, state| {
        let _ = (&core, &state);
        let guard = bound.evict_allocator().unwrap();
        let mut claim = guard
            .operational
            .inner
            .pool
            .claim_allocator_load(&guard.snapshot, 1)
            .unwrap();
        resident::replace_claim_binding(&mut claim, &foreign_binding);
        let prior: &[u8; PAGE_BYTES] = (&guard.operational.inner.writer.operation[32768..49152])
            .try_into()
            .unwrap();
        let permit = ReloadInstallPermit {
            snapshot: &guard.snapshot,
            bytes: prior,
        };
        assert_eq!(
            guard
                .operational
                .inner
                .pool
                .install_evicted_allocator(claim, permit)
                .unwrap_err()
                .effect,
            Effect::CallerMustResolve
        );
        assert_eq!(resident::resident_slot(&guard.operational.inner.pool), None);
    });
}

#[test]
fn eviction_dpt_mismatch_fences_before_removal() {
    for requirement_only in [false, true] {
        fixture!(|bound, core, state| {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let (requirement, entry);
            {
                let (mut writer, _, _) = bound.split_with_flush();
                writer.reserve_pages(&mut owner, 1).unwrap();
                requirement = frame_test::actual_requirement(writer.pool, 0).unwrap();
                entry = frame_test::diagnose(writer.pool, 0, &mut boxed::<PAGE_BYTES>()).entry;
            }
            flush(&mut bound, &core);
            if requirement_only {
                frame_test::replace_requirement(&bound.inner.pool, 0, requirement);
            } else {
                frame_test::seed_dpt(&bound.inner.pool, 0, entry);
            }
            let reads = state.lock().unwrap().reads;
            let error = match bound.evict_allocator() {
                Err(e) => e,
                Ok(_) => panic!("mismatched DPT"),
            };
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert_eq!(resident::resident_slot(&bound.inner.pool), Some(0));
            assert_eq!(state.lock().unwrap().reads, reads);
        });
    }
}

#[test]
fn eviction_pending_writer_refuses_before_removal() {
    fixture!(|bound, core, state| {
        let _ = &core;
        bound.inner.writer.terminal_pending = true;
        let reads = state.lock().unwrap().reads;
        let error = match bound.evict_allocator() {
            Err(e) => e,
            Ok(_) => panic!("pending writer"),
        };
        assert!(matches!(error.cause, Cause::Busy));
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(resident::resident_slot(&bound.inner.pool), Some(0));
        assert_eq!(state.lock().unwrap().reads, reads);
    });
}
