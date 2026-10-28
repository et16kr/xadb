//! Shared original-leaf arbitration. Metadata fixtures never certify durable
//! installation; the selected actual CREATE consumers cover that separately.
use super::*;
use crate::storage::dirty_frame::{LoadClaimStep, test_support as hooks};
use crate::transaction::concurrency::OwnerRole;
use std::{sync::mpsc::sync_channel, time::Duration};

fn busy<T>(result: Result<T>) {
    match result {
        Err(error) => assert_eq!(error, unchanged(Cause::Busy)),
        Ok(_) => panic!("ordinary admission crossed the CREATE claim"),
    }
}

#[test]
fn pair_shared_pin_and_load_first_preserve_candidates_and_quota() {
    for load in [false, true] {
        fixture(|pool, authority| {
            let pool = &*pool;
            let selected = pool
                .catalog_pair_candidates(authority.pair(), 1, authority.existing())
                .unwrap();
            let initial = snapshot(pool);
            assert_eq!(initial.total, 0);
            let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            if load {
                let claim = match pool
                    .claim_retention_load(&mut owner, request(11), 1, authority.existing())
                    .unwrap()
                {
                    LoadClaimStep::Claimed(claim) => claim,
                    _ => panic!("actual clean-victim load claim required"),
                };
                let before = snapshot(pool);
                let record = pool.entry_registry().unwrap().load;
                assert!(record.is_some());
                busy(pool.try_prepare_tx_pair(authority, &selected));
                assert_eq!(snapshot(pool), before);
                assert_eq!(pool.entry_registry().unwrap().load, record);
                claim.cancel().unwrap();
            } else {
                let pins = pool.reserve_target_set(&mut owner, &[request(0)]).unwrap();
                let before = snapshot(pool);
                busy(pool.try_prepare_tx_pair(authority, &selected));
                assert_eq!(snapshot(pool), before);
                pins.release_checked().unwrap();
            }
            owner.check_idle().unwrap();
            let after = snapshot(pool);
            assert_eq!(after.successors, initial.successors);
            assert_eq!(after.contents, initial.contents);
            assert_eq!((after.total, after.closed, after.claim), (0, None, None));
            let key = pool.try_prepare_tx_pair(authority, &selected).unwrap();
            assert!(key.matches_pool(pool).unwrap());
            assert!(!pool.fence.is_fenced());
            drop(key); // Deliberately unresolved metadata-only claim.
        });
    }
}

#[test]
fn pair_shared_capture_claim_before_pin_blocks_close() {
    fixture(|pool, authority| {
        let pool = &*pool;
        let selected = pool
            .catalog_pair_candidates(authority.pair(), 1, authority.existing())
            .unwrap();
        let (entered, arrived) = sync_channel(1);
        let (resume, release) = sync_channel(1);
        hooks::install_flush_pause(pool, hooks::FlushPoint::Claimed, entered, release);
        std::thread::scope(|scope| {
            let capture = scope.spawn(|| {
                let mut owner = pool.core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let mut output = [0x5a; PAGE_BYTES];
                assert!(
                    pool.capture_flush(&mut owner, request(0), &mut output)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(output, [0xa7; PAGE_BYTES]);
                owner.check_idle().unwrap();
            });
            arrived.recv_timeout(Duration::from_secs(5)).unwrap();
            let before = snapshot(pool);
            assert_eq!(before.total, 0, "capture has not acquired its pin");
            assert!(pool.entry_registry().unwrap().claim.is_some());
            busy(pool.try_prepare_tx_pair(authority, &selected));
            assert_eq!(snapshot(pool), before);
            assert!(pool.entry_registry().unwrap().claim.is_some());
            resume.send(hooks::Resume::Continue).unwrap();
            capture.join().unwrap();
        });
        assert!(pool.entry_registry().unwrap().claim.is_none());
        let key = pool.try_prepare_tx_pair(authority, &selected).unwrap();
        assert!(key.matches_pool(pool).unwrap());
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn pair_shared_close_first_gates_ordinary_entries_before_lookup_or_mutation() {
    fixture(|pool, authority| {
        let pool = &*pool;
        let selected = pool
            .catalog_pair_candidates(authority.pair(), 1, authority.existing())
            .unwrap();
        let (entered, arrived) = sync_channel(1);
        let (resume, release) = sync_channel(1);
        test_support::install_init_pause(
            pool,
            test_support::InitPoint::PairClosedBeforeClaim,
            entered,
            release,
        );
        let key = std::thread::scope(|scope| {
            let closer = scope.spawn(|| pool.try_prepare_tx_pair(authority, &selected).unwrap());
            arrived.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(matches!(
                pool.registry.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            let (started, starting) = sync_channel(1);
            let pin = scope.spawn(move || {
                let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
                started.send(()).unwrap();
                busy(pool.reserve_target_set(&mut owner, &[request(11)]));
                owner.check_idle().unwrap();
            });
            starting.recv_timeout(Duration::from_secs(5)).unwrap();
            resume.send(hooks::Resume::Continue).unwrap();
            let key = closer.join().unwrap();
            pin.join().unwrap();
            key
        });
        let before = snapshot(pool);
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flusher = pool.core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut output = [0x5a; PAGE_BYTES];
        for page in [0, 11, 12] {
            busy(pool.reserve_target_set(&mut owner, &[request(page)]));
            busy(pool.capture_flush(&mut flusher, request(page), &mut output));
        }
        for page in [1, 11, 12] {
            busy(pool.claim_retention_load(&mut owner, request(page), 1, &[]));
        }
        busy(pool.flush_preflight());
        assert_eq!(output, [0x5a; PAGE_BYTES]);
        assert_eq!(snapshot(pool), before);
        assert!(pool.entry_registry().unwrap().load.is_none());
        assert!(pool.entry_registry().unwrap().claim.is_none());
        // The authenticated existing path is the same original quota registry.
        pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)])
            .unwrap()
            .try_read()
            .unwrap()
            .release_checked()
            .unwrap();
        assert_eq!(snapshot(pool), before);
        owner.check_idle().unwrap();
        flusher.check_idle().unwrap();
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn pair_shared_selected_stamp_and_empty_generation_reject_aba() {
    for variant in 0..3 {
        fixture(|pool, authority| {
            let pool = &*pool;
            if variant == 2 {
                pool.directory_edit(|d| {
                    d.retire_retention(7);
                    d.retire_retention(8);
                })
                .unwrap();
            }
            let selected = pool
                .catalog_pair_candidates(authority.pair(), 1, authority.existing())
                .unwrap();
            match variant {
                0 => pool
                    .directory_edit(|d| {
                        d.replace_retention(
                            7,
                            Registration {
                                request: request(3),
                                generation: 2,
                            },
                        )
                    })
                    .unwrap(),
                1 => {
                    let mut content = pool.frames[7].content.write().unwrap();
                    content.page_lsn = PageLsn(5);
                    content.modification_counter = 1;
                }
                2 => pool
                    .directory_edit(|d| {
                        d.replace_retention(
                            7,
                            Registration {
                                request: request(3),
                                generation: 2,
                            },
                        );
                        d.retire_retention(7);
                    })
                    .unwrap(),
                _ => unreachable!(),
            }
            let before = snapshot(pool);
            rejected(
                pool.try_prepare_tx_pair(authority, &selected),
                Cause::Stale,
                Effect::Unchanged,
            );
            assert_eq!(snapshot(pool), before, "variant {variant}");
            assert!(!pool.fence.is_fenced());
        });
    }
}

#[test]
fn pair_shared_candidate_origin_is_checked_without_consuming_generation() {
    fixture(|pool, authority| {
        let pool = &*pool;
        let mut selected = pool
            .catalog_pair_candidates(authority.pair(), 1, authority.existing())
            .unwrap();
        let before = snapshot(pool);
        static FOREIGN_BINDING: AtomicU8 = AtomicU8::new(2);
        static FOREIGN_ANCHOR: u8 = 0;
        let binding = selected.binding;
        selected.binding = &FOREIGN_BINDING;
        rejected(
            pool.try_prepare_tx_pair(authority, &selected),
            Cause::BindingMismatch,
            Effect::Unchanged,
        );
        selected.binding = binding;
        let anchor = selected.anchor;
        selected.anchor = &FOREIGN_ANCHOR;
        rejected(
            pool.try_prepare_tx_pair(authority, &selected),
            Cause::BindingMismatch,
            Effect::Unchanged,
        );
        selected.anchor = anchor;
        assert_eq!(snapshot(pool), before);
        let key = pool.try_prepare_tx_pair(authority, &selected).unwrap();
        assert!(key.matches_pool(pool).unwrap());
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn pair_shared_close_after_load_observation_rechecks_resident_and_unrelated_victim() {
    for point in [
        test_support::InitPoint::LoadResidentObserved,
        test_support::InitPoint::LoadCleanSelected,
    ] {
        fixture_slots(10, |pool, authority| {
            let pool = &*pool;
            let selected = pool
                .catalog_pair_candidates(authority.pair(), 1, authority.existing())
                .unwrap();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            test_support::install_init_pause(pool, point, entered, release);
            std::thread::scope(|scope| {
                let load = scope.spawn(|| {
                    let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
                    // Protect the original nine CREATE targets so the final
                    // clean claim candidate is unrelated Job5 at index9.
                    let protected = [0, 1, 2, 7, 8, 9, 10, 3, 4].map(request);
                    let mut wanted = request(11);
                    // Both the victim and requested address are outside the
                    // CREATE target mask/pair, so only the global gate rejects.
                    wanted.identity.page.page_no = 13;
                    if point == test_support::InitPoint::LoadResidentObserved {
                        wanted = request(1);
                    }
                    busy(pool.claim_retention_load(&mut owner, wanted, 1, &protected));
                    owner.check_idle().unwrap();
                });
                arrived.recv_timeout(Duration::from_secs(5)).unwrap();
                let key = pool.try_prepare_tx_pair(authority, &selected).unwrap();
                let closed = snapshot(pool);
                assert_eq!(closed.total, 0);
                resume.send(hooks::Resume::Continue).unwrap();
                load.join().unwrap();
                assert_eq!(snapshot(pool), closed);
                assert!(pool.entry_registry().unwrap().load.is_none());
                assert!(!pool.fence.is_fenced());
                drop(key);
            });
        });
    }
}
