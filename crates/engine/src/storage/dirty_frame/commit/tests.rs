//! Actual pre-stage pool/Intent ownership only. No simulated semantic grant,
//! WAL, durable receipt or Commit success is constructed by these inputs.
use super::*;
use crate::{storage::page_io::test_support::expected, transaction::concurrency::OwnerRole};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn request(index: usize) -> TargetRequest {
    let mut identity = expected();
    identity.page.page_no = (index + 10) as u64;
    TargetRequest {
        identity,
        kind: PageKind::RowLeaf,
        level: 0,
    }
}

fn with_pool(run: impl FnOnce(&SharedPool<'_, '_, '_, '_>)) {
    let mut pages = [
        [1; PAGE_BYTES],
        [2; PAGE_BYTES],
        [3; PAGE_BYTES],
        [4; PAGE_BYTES],
        [5; PAGE_BYTES],
    ];
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

fn requests() -> [TargetRequest; 4] {
    [request(3), request(1), request(0), request(2)]
}

fn clean(pool: &SharedPool<'_, '_, '_, '_>) {
    assert_eq!(pool.registry.lock().unwrap().total, 0);
    assert!(!pool.core.diagnostic_version_busy());
    for frame in pool.frames {
        assert!(frame.content.try_write().is_ok());
    }
}

#[test]
fn commit_targets_intent_preserves_pins_and_checked_cancel() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut other = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let intent = pool
            .reserve_target_set(&mut owner, &requests())
            .unwrap()
            .try_write()
            .unwrap()
            .prepare_commit()
            .unwrap();
        assert_eq!(pool.registry.lock().unwrap().counts[..5], [1, 1, 1, 1, 0]);
        assert_eq!(pool.registry.lock().unwrap().total, 4);
        assert!(pool.core.diagnostic_version_busy());
        for at in 0..4 {
            let stable = intent.stable[at].unwrap();
            assert_eq!(stable.stamp.identity, request(at).identity);
            assert_eq!(stable.stamp.generation, 1);
            assert_eq!(stable.stamp.page_lsn, PageLsn(0));
            assert_eq!(stable.stamp.modification_counter, 0);
            assert_eq!(stable.stamp.rec_lsn, None);
        }
        match other.try_apply() {
            Err(error) => assert_eq!(error.cause, concurrency::Cause::InstallationPending),
            Ok(_) => panic!("new apply passed original Commit Intent"),
        }
        // Intent still permits a finite reader on unrelated existing content.
        let read = pool
            .reserve_targets(&mut other, &[request(4)])
            .unwrap()
            .try_read()
            .unwrap();
        assert_eq!(read.bytes(0).unwrap(), &[5; PAGE_BYTES]);
        read.release_checked().unwrap();
        intent.cancel_unstaged().unwrap();
        clean(pool);
        assert!(!pool.fence.is_fenced());
        owner
            .try_apply()
            .unwrap()
            .try_version()
            .unwrap()
            .release_checked()
            .unwrap();
    });
}

#[test]
fn commit_targets_prepare_rejection_returns_original_guard_before_pins() {
    // Maintenance can own an ordinary writer, but not a foreground Commit.
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Maintenance).unwrap();
        let result = pool
            .reserve_target_set(&mut owner, &requests())
            .unwrap()
            .try_write()
            .unwrap()
            .prepare_commit();
        match result {
            Err(error) => {
                assert_eq!(
                    error.cause,
                    Cause::Concurrency(concurrency::Cause::InvalidRole)
                );
                assert_eq!(error.effect, Effect::Unchanged);
            }
            Ok(_) => panic!("maintenance became a Commit owner"),
        }
        drop(result);
        clean(pool);
        assert!(!pool.fence.is_fenced());
        owner
            .try_apply()
            .unwrap()
            .try_version()
            .unwrap()
            .release_checked()
            .unwrap();
    });
    // A coherent clean frame at MAX isolates exhaustion from DPT corruption.
    for exhausted in [0, 2, 3] {
        with_pool(|pool| {
            {
                let mut frame = pool.frames[exhausted].content.write().unwrap();
                frame.modification_counter = u64::MAX;
                frame.page_lsn = PageLsn(1);
            }
            let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            let result = pool
                .reserve_target_set(&mut owner, &requests())
                .unwrap()
                .try_write()
                .unwrap()
                .prepare_commit();
            match result {
                Err(error) => {
                    assert_eq!(error.cause, Cause::CounterExhausted);
                    assert_eq!(error.effect, Effect::Unchanged);
                }
                Ok(_) => panic!("MAX counter entered Commit Intent"),
            }
            drop(result);
            clean(pool);
            assert!(!pool.fence.is_fenced());
            owner
                .try_apply()
                .unwrap()
                .try_version()
                .unwrap()
                .release_checked()
                .unwrap();
        });
    }
}

#[test]
fn commit_targets_intent_unwind_fences_and_releases_physical_ownership() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _intent = pool
                    .reserve_target_set(&mut owner, &requests())
                    .unwrap()
                    .try_write()
                    .unwrap()
                    .prepare_commit()
                    .unwrap();
                panic!("bounded Commit Intent unwind input");
            }))
            .is_err()
        );
        assert!(pool.fence.is_fenced());
        assert_eq!(pool.registry.lock().unwrap().total, 0);
        // Unwind may poison guards; try_lock must still show no live lock.
        assert!(!pool.core.diagnostic_version_busy());
        for frame in pool.frames {
            assert!(!matches!(
                frame.content.try_write(),
                Err(TryLockError::WouldBlock)
            ));
        }
    });
}

#[test]
fn commit_common_source_bounds() {
    for (count, bytes) in [(4, source_bytes::<4>()), (9, source_bytes::<9>())] {
        println!(
            "COMMIT_COMMON_SOURCE targets={count} source_bytes={bytes} ceiling=65536 semantic_consumer=NOT_RUN"
        );
        assert!(bytes <= 65_536);
    }
}

#[test]
fn commit_targets_capacity_preflight_preserves_fence_priority() {
    assert!(source_bytes::<64>() > 65_536);
    for fenced in [false, true] {
        let mut pages = vec![[0; PAGE_BYTES]; 64];
        let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
        for (index, frame) in frames.iter_mut().enumerate() {
            frame.register(request(index)).unwrap();
        }
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let binding = AtomicU8::new(2);
        let pool = SharedPool::new(&core, &fence, &binding, &frames).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let requests = std::array::from_fn::<_, 64, _>(request);
        let writer = pool
            .reserve_target_set(&mut owner, &requests)
            .unwrap()
            .try_write()
            .unwrap();
        if fenced {
            fence.fence();
        }
        let result = writer.prepare_commit();
        match result {
            Err(error) => {
                assert_eq!(
                    error.cause,
                    if fenced {
                        Cause::Fenced
                    } else {
                        Cause::Capacity
                    }
                );
                assert_eq!(
                    error.effect,
                    if fenced {
                        Effect::CallerMustResolve
                    } else {
                        Effect::Unchanged
                    }
                );
            }
            Ok(_) => panic!("oversize physical Commit preparation admitted"),
        }
        drop(result);
        clean(&pool);
        assert_eq!(fence.is_fenced(), fenced);
        if !fenced {
            owner
                .try_apply()
                .unwrap()
                .try_version()
                .unwrap()
                .release_checked()
                .unwrap();
        }
    }
}
