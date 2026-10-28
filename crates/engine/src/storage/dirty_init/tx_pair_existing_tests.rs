//! Metadata/capability checks only; actual semantic installation is tested by
//! the selected live Reserve/Cancel/Forward consumers on the same build.
use super::*;
use crate::transaction::concurrency::OwnerRole;

fn rejected_subset<T>(result: Result<T>, expected: Cause) {
    match result {
        Err(error) => assert_eq!(error, unchanged(expected)),
        Ok(_) => panic!("unexpected subset admission"),
    }
}
fn exact<const N: usize>() {
    fixture(|pool, authority| {
        let key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let wanted: [_; N] = std::array::from_fn(|at| authority.existing()[N - 1 - at]);
        let read = pool
            .reserve_tx_pair_existing(&mut owner, &key, &wanted)
            .unwrap()
            .try_read()
            .unwrap();
        let mut image = [0; PAGE_BYTES];
        for at in 0..N {
            let stamp = read.copy_snapshot(at, &mut image).unwrap();
            assert_eq!(stamp.identity, authority.existing()[at].identity);
            assert_eq!(read.observe(at).unwrap().frame, stamp);
            assert_eq!(read.bytes(at).unwrap(), &[0xa7; PAGE_BYTES]);
            assert_eq!(image, [0xa7; PAGE_BYTES]);
        }
        {
            let pins = pool.entry_registry().unwrap();
            assert_eq!(pins.total, N as u16);
            assert!(!pins.init_claim.unwrap().initializer_pins_held);
        }
        read.release_checked().unwrap();
        let before = snapshot(pool);
        let write = pool
            .reserve_tx_pair_existing(&mut owner, &key, &wanted)
            .unwrap()
            .try_write()
            .unwrap();
        // The original write methods still require a true observed stamp and
        // sealed installation permission; the wrapper issues neither.
        for at in 0..N {
            let stamp = FrameStamp {
                identity: authority.existing()[at].identity,
                generation: 1,
                page_lsn: PageLsn(0),
                modification_counter: 0,
                rec_lsn: None,
            };
            write.matches_stamp(at, &stamp).unwrap();
            let _proof = write.preflight_dpt(at, &stamp).unwrap();
        }
        write.release_checked().unwrap();
        owner.check_idle().unwrap();
        assert_eq!(snapshot(pool), before);
        drop(key); // Intentional unresolved metadata fixture fence.
    });
}

#[test]
fn pair_existing_exact_subsets_preserve_original_pin_and_content_lifetime() {
    exact::<1>();
    exact::<2>();
    exact::<3>();
    exact::<4>();
    exact::<5>();
    exact::<6>();
    exact::<7>();
}

#[test]
fn pair_existing_rejections_leave_quota_and_claim_unchanged() {
    fixture(|pool, authority| {
        let mut key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let before = snapshot(pool);
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[]),
            Cause::Shape,
        );
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0); 8]),
            Cause::Shape,
        );
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0), request(0)]),
            Cause::DuplicateTarget,
        );
        for invalid in [request(11), request(12), request(3)] {
            rejected_subset(
                pool.reserve_tx_pair_existing(&mut owner, &key, &[invalid]),
                Cause::MissingTarget,
            );
        }
        let mut invalid = request(0);
        invalid.identity.owner = ObjectId::new(8).unwrap();
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[invalid]),
            Cause::MissingTarget,
        );
        invalid = request(9);
        invalid.identity.page.incarnation = Generation::new(3).unwrap();
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[invalid]),
            Cause::MissingTarget,
        );
        assert_eq!(snapshot(pool), before);
        static FOREIGN_BINDING: AtomicU8 = AtomicU8::new(2);
        let original_binding = key.binding;
        key.binding = &FOREIGN_BINDING;
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)]),
            Cause::BindingMismatch,
        );
        key.binding = original_binding;
        assert_eq!(snapshot(pool), before);
        key.active = false;
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)]),
            Cause::BindingMismatch,
        );
        key.active = true;
        let original = key.identity.reserve;
        key.identity.reserve = PageLsn(original.0 + 1);
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)]),
            Cause::Busy,
        );
        key.identity.reserve = original;
        assert_eq!(snapshot(pool), before);
        // Valid synthetic pin quota: per-frame saturation and whole quota.
        for total in [34, 68] {
            {
                let mut state = pool.registry.lock().unwrap();
                state.counts[0] = 34;
                state.counts[1] = total - 34;
                state.total = total;
            }
            let full = snapshot(pool);
            rejected_subset(
                pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)]),
                Cause::Capacity,
            );
            assert_eq!(snapshot(pool), full);
        }
        {
            let mut state = pool.registry.lock().unwrap();
            state.counts = [0; MAX_FRAMES];
            state.total = 0;
        }
        assert_eq!(snapshot(pool), before);
    });
}

#[test]
fn pair_existing_whole_forward_exclusion_is_bidirectional() {
    fixture(|pool, authority| {
        let key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
        let mut owner_a = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut owner_b = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let requests = [0, 1, 2, 7, 8, 9, 10, 11, 12].map(request);
        let before = snapshot(pool);
        let subset = pool
            .reserve_tx_pair_existing(&mut owner_a, &key, &[request(0)])
            .unwrap();
        rejected_subset(
            pool.claim_tx_pair_targets(&mut owner_b, &key, &requests),
            Cause::Busy,
        );
        assert_eq!(pool.entry_registry().unwrap().total, 1);
        subset.release_checked().unwrap();
        assert_eq!(snapshot(pool), before);
        let whole = pool
            .claim_tx_pair_targets(&mut owner_a, &key, &requests)
            .unwrap();
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner_b, &key, &[request(0)]),
            Cause::Busy,
        );
        assert_eq!(pool.entry_registry().unwrap().total, 9);
        whole.release_checked().unwrap();
        assert_eq!(snapshot(pool), before);
    });
}

#[test]
fn pair_existing_content_busy_and_unwind_release_pins_without_returning_claim() {
    fixture(|pool, authority| {
        let key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let guard = pool.frames[0].content.try_write().unwrap();
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)])
                .unwrap()
                .try_read(),
            Cause::Busy,
        );
        rejected_subset(
            pool.reserve_tx_pair_existing(&mut owner, &key, &[request(0)])
                .unwrap()
                .try_write(),
            Cause::Busy,
        );
        drop(guard);
        owner.check_idle().unwrap();
        assert_eq!(pool.entry_registry().unwrap().total, 0);
        assert!(!pool.fence.is_fenced());
    });
    for write in [false, true] {
        fixture(|pool, authority| {
            let key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
            let original = key.identity;
            let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let pinned = pool
                        .reserve_tx_pair_existing(&mut owner, &key, &[request(0)])
                        .unwrap();
                    if write {
                        let _write = pinned.try_write().unwrap();
                        panic!("PAIR_EXISTING_WRITE_UNWIND");
                    } else {
                        let _read = pinned.try_read().unwrap();
                        panic!("PAIR_EXISTING_READ_UNWIND");
                    }
                }))
                .is_err()
            );
            assert!(pool.fence.is_fenced());
            let state = pool.registry.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(state.total, 0);
            assert_eq!(state.counts, [0; MAX_FRAMES]);
            assert_eq!(
                state.init_claim.unwrap().identity,
                InitClaimIdentity::TxPair(original)
            );
            assert!(!state.init_claim.unwrap().initializer_pins_held);
            assert_eq!(state.directory.closed_pair(), Some([7, 8]));
        });
    }
}
