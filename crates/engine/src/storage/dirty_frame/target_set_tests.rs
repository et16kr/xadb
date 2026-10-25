//! Whole-set authority tests. Raw in-memory bytes are not disk/load fixtures.
use super::*;
use crate::storage::page_io::test_support::expected;
use crate::transaction::concurrency::{OWNER_SLOTS, OwnerRole};
use foundation::identity::Generation;
use std::{
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc,
    time::Duration,
};

fn request(index: usize) -> TargetRequest {
    let mut identity = expected();
    identity.page.page_no = (index * 2 + 1) as u64;
    TargetRequest {
        identity,
        kind: PageKind::RowLeaf,
        level: 0,
    }
}
fn with_pool<const N: usize>(run: impl FnOnce(&SharedPool<'_, '_, '_, '_>)) {
    let mut pages = vec![[0; PAGE_BYTES]; N];
    for (index, page) in pages.iter_mut().enumerate() {
        page.fill((index + 1) as u8);
    }
    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
    for (index, frame) in frames.iter_mut().enumerate() {
        frame.register(request(index)).unwrap();
    }
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let pool = SharedPool::new(&core, &fence, &binding, &frames).unwrap();
    run(&pool);
}
fn counts(pool: &SharedPool<'_, '_, '_, '_>) -> ([u16; MAX_FRAMES], u16) {
    let pins = pool.pins.lock().unwrap();
    (pins.counts, pins.total)
}
fn idle(pool: &SharedPool<'_, '_, '_, '_>) {
    assert_eq!(counts(pool), ([0; MAX_FRAMES], 0));
    for frame in pool.frames {
        assert!(frame.content.try_write().is_ok());
    }
}
fn error<T>(result: Result<T>, cause: Cause, effect: Effect) {
    match result {
        Err(actual) => assert_eq!(actual, DirtyError { cause, effect }),
        Ok(_) => panic!("unexpected whole-set success"),
    }
}
fn ordered<const N: usize>(pattern: usize) {
    with_pool::<N>(|pool| {
        let requests = std::array::from_fn::<_, N, _>(|at| {
            request(match pattern {
                0 => at,
                1 => N - 1 - at,
                2 => (at + 1) % N,
                _ => {
                    if at % 2 == 0 {
                        at / 2
                    } else {
                        N - 1 - at / 2
                    }
                }
            })
        });
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let pinned = pool.reserve_target_set(&mut owner, &requests).unwrap();
        assert_eq!(
            pinned.pins.as_ref().unwrap().indices,
            std::array::from_fn::<_, N, _>(|at| at)
        );
        assert_eq!(pinned.pins.as_ref().unwrap().generations, [1; N]);
        assert_eq!(counts(pool).1, N as u16);
        let read = pinned.try_read().unwrap();
        for at in 0..N {
            assert_eq!(read.target_request(at).unwrap(), request(at));
            assert_eq!(read.bytes(at).unwrap(), &[at as u8 + 1; PAGE_BYTES]);
            let seen = read.observe(at).unwrap();
            assert_eq!(seen.frame.identity, request(at).identity);
            assert_eq!(
                (seen.frame.generation, seen.frame.page_lsn, seen.entry),
                (1, PageLsn(0), None)
            );
        }
        read.release_checked().unwrap();
        let write = pool
            .reserve_target_set(&mut owner, &requests)
            .unwrap()
            .try_write()
            .unwrap();
        for at in 0..N {
            assert_eq!(
                write.guards[at].as_ref().unwrap().bytes,
                &[at as u8 + 1; PAGE_BYTES]
            );
        }
        write.release_checked().unwrap();
        idle(pool);
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn target_set_orders_three_and_reads_every_page() {
    ordered::<3>(2);
}
#[test]
fn target_set_maximum_capacity_reads_and_writes() {
    ordered::<64>(1);
}

#[test]
fn target_set_aliases_rejected_atomically() {
    with_pool::<3>(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        for variant in 0..3 {
            let mut alias = request(0);
            match variant {
                1 => alias.identity.page.incarnation = Generation::new(99).unwrap(),
                2 => alias.kind = PageKind::Allocation,
                _ => (),
            }
            error(
                pool.reserve_target_set(&mut owner, &[request(0), request(2), alias]),
                Cause::DuplicateTarget,
                Effect::Unchanged,
            );
            idle(pool);
        }
    });
}
#[test]
fn target_set_missing_last_rejected_atomically() {
    with_pool::<64>(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut requests = std::array::from_fn::<_, 64, _>(request);
        requests[63] = request(64);
        error(
            pool.reserve_target_set(&mut owner, &requests),
            Cause::MissingTarget,
            Effect::Unchanged,
        );
        idle(pool);
    });
}
#[test]
fn target_set_pin_limits_preserve_prior_owners() {
    with_pool::<64>(|pool| {
        let mut a = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut b = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut c = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let all = pool
            .reserve_target_set(&mut a, &std::array::from_fn::<_, 64, _>(request))
            .unwrap();
        let extra = pool
            .reserve_target_set(&mut b, &[request(0), request(1), request(2), request(3)])
            .unwrap();
        let before = counts(pool);
        assert_eq!(before.1, 68);
        error(
            pool.reserve_target_set(&mut c, &[request(63)]),
            Cause::Capacity,
            Effect::Unchanged,
        );
        assert_eq!(counts(pool), before);
        extra.release_checked().unwrap();
        all.release_checked().unwrap();
        idle(pool);
        // A valid aggregate counter state isolates the per-frame ceiling.
        {
            let mut pins = pool.pins.lock().unwrap();
            pins.counts[2] = 34;
            pins.total = 34;
        }
        let before = counts(pool);
        error(
            pool.reserve_target_set(&mut a, &[request(0), request(1), request(2)]),
            Cause::Capacity,
            Effect::Unchanged,
        );
        assert_eq!(counts(pool), before);
        {
            let mut pins = pool.pins.lock().unwrap();
            pins.counts[2] = 0;
            pins.total = 0;
        }
        idle(pool);
    });
}
fn busy<const N: usize>(at: usize, write: bool) {
    with_pool::<N>(|pool| {
        let held = pool.frames[at].content.write().unwrap();
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let pinned = pool
            .reserve_target_set(&mut owner, &std::array::from_fn::<_, N, _>(request))
            .unwrap();
        if write {
            error(pinned.try_write(), Cause::Busy, Effect::Unchanged);
        } else {
            error(pinned.try_read(), Cause::Busy, Effect::Unchanged);
        }
        assert_eq!(counts(pool), ([0; MAX_FRAMES], 0));
        for (index, frame) in pool.frames.iter().enumerate() {
            if index != at {
                assert!(frame.content.try_write().is_ok());
            }
        }
        assert!(!pool.fence.is_fenced());
        drop(held);
        let retry = pool
            .reserve_target_set(&mut owner, &std::array::from_fn::<_, N, _>(request))
            .unwrap()
            .try_write()
            .unwrap();
        retry.release_checked().unwrap();
        idle(pool);
    });
}
#[test]
fn target_set_busy_read_releases_all_partial_guards() {
    for at in [0, 1, 2] {
        busy::<3>(at, false);
    }
}
#[test]
fn target_set_busy_write_releases_all_partial_guards() {
    for at in [0, 1, 2] {
        busy::<3>(at, true);
    }
}
#[test]
fn target_set_poisoned_last_releases_every_count() {
    for write in [false, true] {
        with_pool::<3>(|pool| {
            test_support::poison_content(pool, 2);
            let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            let pinned = pool
                .reserve_target_set(&mut owner, &[request(2), request(0), request(1)])
                .unwrap();
            if write {
                error(
                    pinned.try_write(),
                    Cause::Poisoned,
                    Effect::CallerMustResolve,
                );
            } else {
                error(
                    pinned.try_read(),
                    Cause::Poisoned,
                    Effect::CallerMustResolve,
                );
            }
            assert!(pool.fence.is_fenced());
            assert_eq!(counts(pool), ([0; MAX_FRAMES], 0));
            for frame in &pool.frames[..2] {
                assert!(frame.content.try_write().is_ok());
            }
            assert!(matches!(
                pool.frames[2].content.try_write(),
                Err(TryLockError::Poisoned(_))
            ));
        });
    }
}
fn stale<const N: usize>(at: usize, action: usize) {
    with_pool::<N>(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut pinned = pool
            .reserve_target_set(&mut owner, &std::array::from_fn::<_, N, _>(request))
            .unwrap();
        let before = counts(pool);
        pinned.pins.as_mut().unwrap().generations[at] = 2;
        match action {
            0 => error(
                pinned.release_checked(),
                Cause::Invariant,
                Effect::CallerMustResolve,
            ),
            1 => error(
                pinned.try_read(),
                Cause::Invariant,
                Effect::CallerMustResolve,
            ),
            _ => error(
                pinned.try_write(),
                Cause::Invariant,
                Effect::CallerMustResolve,
            ),
        }
        assert!(pool.fence.is_fenced());
        assert_eq!(counts(pool), before);
        for frame in pool.frames {
            assert!(frame.content.try_write().is_ok());
        }
    });
}
#[test]
fn target_set_stale_last_release_preserves_every_count() {
    stale::<3>(2, 0);
}
#[test]
fn target_set_late_stale_read_and_write_fence() {
    stale::<3>(2, 1);
    stale::<3>(2, 2);
}
#[test]
fn target_set_duplicate_last_release_preserves_every_count() {
    with_pool::<3>(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut pinned = pool
            .reserve_target_set(&mut owner, &[request(0), request(1), request(2)])
            .unwrap();
        let before = counts(pool);
        pinned.pins.as_mut().unwrap().indices[2] = 0;
        error(
            pinned.release_checked(),
            Cause::Invariant,
            Effect::CallerMustResolve,
        );
        assert!(pool.fence.is_fenced());
        assert_eq!(counts(pool), before);
    });
}
#[test]
fn target_set_unwind_fences_before_unlock() {
    for write in [false, true] {
        with_pool::<3>(|pool| {
            let (arrived, notice) = mpsc::sync_channel(1);
            let (resume, wait) = mpsc::sync_channel(1);
            test_support::install_fence_pause(pool, arrived, wait);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    catch_unwind(AssertUnwindSafe(|| {
                        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
                        let pins = pool
                            .reserve_target_set(&mut owner, &[request(0), request(1), request(2)])
                            .unwrap();
                        if write {
                            let _held = pins.try_write().unwrap();
                            panic!("target-set write unwind");
                        } else {
                            let _held = pins.try_read().unwrap();
                            panic!("target-set read unwind");
                        }
                    }))
                });
                notice.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(pool.fence.is_fenced());
                assert_eq!(counts(pool).1, 3);
                for at in 0..3 {
                    assert!(test_support::content_busy(pool, at));
                }
                resume.send(test_support::Resume::Continue).unwrap();
                assert!(worker.join().unwrap().is_err());
            });
            assert_eq!(counts(pool), ([0; MAX_FRAMES], 0));
            // A write guard dropped during unwind poisons each lock, but releases it.
            for frame in pool.frames {
                assert!(!matches!(
                    frame.content.try_write(),
                    Err(TryLockError::WouldBlock)
                ));
            }
        });
    }
}
#[test]
fn target_set_thread_readers_exclude_writer_and_resume() {
    with_pool::<3>(|pool| {
        std::thread::scope(|scope| {
            let (ready, notice) = mpsc::sync_channel(1);
            let (release, wait) = mpsc::sync_channel(1);
            let worker = scope.spawn(move || {
                let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
                let read = pool
                    .reserve_target_set(&mut owner, &[request(2), request(0), request(1)])
                    .unwrap()
                    .try_read()
                    .unwrap();
                ready.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
                read.release_checked().unwrap();
            });
            notice.recv_timeout(Duration::from_secs(5)).unwrap();
            let mut reader = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            let mut writer = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            let read = pool
                .reserve_target_set(&mut reader, &[request(1), request(2), request(0)])
                .unwrap()
                .try_read()
                .unwrap();
            assert_eq!(counts(pool).1, 6);
            error(
                pool.reserve_target_set(&mut writer, &[request(0), request(1), request(2)])
                    .unwrap()
                    .try_write(),
                Cause::Busy,
                Effect::Unchanged,
            );
            assert_eq!(counts(pool).1, 6);
            read.release_checked().unwrap();
            release.send(()).unwrap();
            worker.join().unwrap();
            pool.reserve_target_set(&mut writer, &[request(2), request(1), request(0)])
                .unwrap()
                .try_write()
                .unwrap()
                .release_checked()
                .unwrap();
            idle(pool);
        })
    });
}
#[test]
fn target_set_stamp_and_read_bounds() {
    with_pool::<3>(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let requests = [request(2), request(1), request(0)];
        let read = pool
            .reserve_target_set(&mut owner, &requests)
            .unwrap()
            .try_read()
            .unwrap();
        let stamps = std::array::from_fn::<_, 3, _>(|at| read.observe(at).unwrap().frame);
        for at in [3, usize::MAX] {
            error(read.bytes(at), Cause::Shape, Effect::Unchanged);
            error(read.target_request(at), Cause::Shape, Effect::Unchanged);
        }
        read.release_checked().unwrap();
        let write = pool
            .reserve_target_set(&mut owner, &requests)
            .unwrap()
            .try_write()
            .unwrap();
        for (at, stamp) in stamps.iter().enumerate() {
            write.matches_stamp(at, stamp).unwrap();
            let token = write.preflight_dpt(at, stamp).unwrap();
            assert_eq!((token.index, token.generation, token.counter), (at, 1, 0));
        }
        let mut stale = stamps[2];
        stale.modification_counter = 1;
        error(
            write.matches_stamp(2, &stale),
            Cause::Stale,
            Effect::Unchanged,
        );
        error(
            write.matches_stamp(3, &stamps[0]),
            Cause::Shape,
            Effect::Unchanged,
        );
        write.release_checked().unwrap();
        idle(pool);
    });
}
#[test]
fn target_set_default_wrapper_shape_and_order() {
    with_pool::<3>(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        error(
            pool.reserve_targets(&mut owner, &[]),
            Cause::Shape,
            Effect::Unchanged,
        );
        error(
            pool.reserve_targets(&mut owner, &[request(0), request(1), request(2)]),
            Cause::Shape,
            Effect::Unchanged,
        );
        let read = pool
            .reserve_targets(&mut owner, &[request(2), request(0)])
            .unwrap()
            .try_read()
            .unwrap();
        assert_eq!(read.target_request(0).unwrap(), request(0));
        assert_eq!(read.target_request(1).unwrap(), request(2));
        read.release_checked().unwrap();
        idle(pool);
    });
}
#[test]
fn target_set_layout() {
    let values = [
        target_set_call_bytes::<2>(),
        target_set_call_bytes::<3>(),
        target_set_call_bytes::<8>(),
        target_set_call_bytes::<64>(),
    ];
    assert!(values[0] <= CALL_BYTES);
    assert!(values.iter().all(|value| *value <= 16384));
    let fixed = size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, 0>>()
        .max(size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, 0>>());
    let per_slot = (size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, 64>>()
        - size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, 0>>())
    .max(
        size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, 64>>()
            - size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, 0>>(),
    ) / 64;
    let capacity_sum = usize::from(MAX_PINS) + OWNER_SLOTS;
    assert_eq!(capacity_sum, 102);
    let retained = OWNER_SLOTS * fixed + capacity_sum * per_slot;
    assert!(retained <= 16384);
    assert!(fits());
    println!(
        "TARGET_SET_LAYOUT calls={values:?} fixed={fixed} per_slot={per_slot} capacity_sum={capacity_sum} retained={retained} pool={}",
        size_of::<SharedPool<'_, '_, '_, '_>>()
    );
}

#[test]
#[ignore = "final S5: 256 normal + 128 busy + 192 stale combinations"]
fn target_set_s5_expanded() {
    macro_rules! normal { ($($n:literal),*) => { $(for pattern in 0..4 { ordered::<$n>(pattern); })* }; }
    normal!(
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48,
        49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64
    );
    for at in 0..64 {
        busy::<64>(at, false);
        busy::<64>(at, true);
        for action in 0..3 {
            stale::<64>(at, action);
        }
    }
}
