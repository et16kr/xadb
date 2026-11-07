//! LOC-B01/B02 (dev3-cursor-locator.md#locator-tests): the locator table and
//! its share of a core on literal keys. Every anchor, RowId, revision, leaf
//! and bound is a literal.
use super::*;
use crate::api::DatabaseId;
use crate::storage::page::FileId;
use crate::transaction::concurrency::PageConcurrency;
use foundation::identity::{Generation, ObjectId};

fn page(page_no: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new((1 << 32) | 1).unwrap(),
        page_no,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}

fn anchor(page_no: u64, incarnation: u64) -> PageIdentity {
    PageIdentity {
        database: DatabaseId::new([7; 16]).unwrap(),
        page: page(page_no, incarnation),
        owner: ObjectId::new(6).unwrap(),
        owner_generation: Generation::new(1).unwrap(),
    }
}

fn row(value: u64) -> RowId {
    RowId::new(value).unwrap()
}

fn bounds(lower: Option<u64>, upper: Option<u64>) -> RowRouteBounds {
    RowRouteBounds {
        lower: lower.map(u64::to_be_bytes),
        upper: upper.map(u64::to_be_bytes),
    }
}

fn counts(
    lookups: u64,
    hits: u64,
    stale: u64,
    misses: u64,
    evictions: u64,
    rechecks: u64,
) -> LocatorCounts {
    LocatorCounts {
        lookups,
        hits,
        stale,
        misses,
        evictions,
        rechecks,
    }
}

/// LOC-B01: a miss, a record and a hit at the same revision; a stale lookup
/// at a later revision and its refresh; an entry recorded at a later
/// revision than the anchor's is damaged; the anchor's whole identity is
/// the key (another incarnation of the same page, another owner generation
/// of the same PageRef, another database); 256 consecutive RowIds take
/// distinct slots and a key 256 RowIds on shares a slot and evicts; a route
/// that does not hold the RowId is not recorded; a recheck confirms only the
/// hint itself; an entry whose bounds do not hold its RowId is damaged at its
/// own revision and at a later one, never a miss or stale; a locator serves
/// only its own core.
#[test]
fn loc_b01_lookup_record_and_eviction() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut locator = RowLocator::new(&core).unwrap();
    let t = anchor(20, 2);
    let left = bounds(None, Some(30));
    let right = bounds(Some(30), None);
    assert_eq!(locator.lookup(t, row(10), 4), Ok(None));
    locator.record(t, row(10), 4, page(33, 2), left);
    locator.record(t, row(40), 4, page(34, 2), right);
    assert_eq!(
        locator.lookup(t, row(10), 4),
        Ok(Some(Hint {
            leaf: page(33, 2),
            bounds: left
        }))
    );
    assert_eq!(
        locator.lookup(t, row(40), 4),
        Ok(Some(Hint {
            leaf: page(34, 2),
            bounds: right
        }))
    );
    assert_eq!(locator.counts(), counts(3, 2, 0, 1, 0, 0));
    assert_eq!(locator.working_set(), 2);

    // A split moved the anchor revision: stale, then the new route refreshes.
    assert_eq!(locator.lookup(t, row(10), 5), Ok(None));
    locator.record(t, row(10), 5, page(35, 2), bounds(None, Some(20)));
    assert_eq!(
        locator.lookup(t, row(10), 5),
        Ok(Some(Hint {
            leaf: page(35, 2),
            bounds: bounds(None, Some(20))
        }))
    );
    assert_eq!(locator.counts(), counts(5, 3, 1, 1, 0, 0));
    // A revision below the entry's: the anchor went back, never a miss.
    assert_eq!(locator.lookup(t, row(10), 4), Err(DamagedHint));
    assert_eq!(locator.counts(), counts(6, 3, 1, 1, 0, 0));
    assert_eq!(locator.working_set(), 2);

    // The anchor's whole identity is the key: the same page with another
    // incarnation, the same PageRef with another owner generation, or
    // another database is another table's anchor.
    let other_incarnation = anchor(20, 3);
    let other_generation = PageIdentity {
        owner_generation: Generation::new(2).unwrap(),
        ..t
    };
    let other_database = PageIdentity {
        database: DatabaseId::new([8; 16]).unwrap(),
        ..t
    };
    for other in [other_incarnation, other_generation, other_database] {
        assert_eq!(locator.lookup(other, row(10), 5), Ok(None));
        assert_eq!(locator.lookup(other, row(40), 4), Ok(None));
    }
    assert_eq!(locator.counts(), counts(12, 3, 1, 7, 0, 0));

    // 256 consecutive RowIds take 256 distinct slots; 256 on, a RowId
    // shares the slot and its record evicts.
    let slots: std::collections::BTreeSet<usize> = (100..356)
        .map(|r| RowLocator::test_slot(t, row(r)))
        .collect();
    assert_eq!(slots.len(), 256);
    assert_eq!(
        RowLocator::test_slot(t, row(40)),
        RowLocator::test_slot(t, row(40 + 256))
    );
    locator.record(t, row(40 + 256), 5, page(34, 2), right);
    assert_eq!(locator.counts().evictions, 1);
    assert_eq!(locator.lookup(t, row(40), 4), Ok(None));
    assert_eq!(locator.counts().misses, 8);
    assert_eq!(locator.working_set(), 2);

    // A route whose bounds do not hold the RowId is not recorded.
    locator.record(t, row(50), 5, page(33, 2), left);
    assert_eq!(locator.test_entry(t, row(50)), None);

    // A recheck confirms only the hint itself.
    assert_eq!(
        locator.confirm(t, row(10), 5, page(35, 2), bounds(None, Some(20))),
        Ok(())
    );
    for (key, revision, leaf, at) in [
        (10, 5, page(36, 2), bounds(None, Some(20))),
        (10, 5, page(35, 2), bounds(None, Some(25))),
        (10, 6, page(35, 2), bounds(None, Some(20))),
        (11, 5, page(35, 2), bounds(None, Some(20))),
    ] {
        assert_eq!(
            locator.confirm(t, row(key), revision, leaf, at),
            Err(DamagedHint)
        );
    }
    assert_eq!(locator.counts().rechecks, 5);

    // A damaged entry (its bounds no longer hold the RowId), recorded at
    // revision 5: refused at its own revision and at a later one, where a
    // healthy entry would be stale. Neither is a hit or stale.
    assert!(locator.test_redirect(t, row(10), page(36, 2), Some(right)));
    let before = locator.counts();
    assert_eq!(locator.lookup(t, row(10), 5), Err(DamagedHint));
    assert_eq!(locator.lookup(t, row(10), 6), Err(DamagedHint));
    assert_eq!(
        locator.counts(),
        LocatorCounts {
            lookups: before.lookups + 2,
            ..before
        }
    );

    // A locator serves only its own core.
    let fence2 = SessionFence::new();
    let core2 = PageConcurrency::new(&fence2).unwrap();
    assert!(locator.serves_core(&core));
    assert!(!locator.serves_core(&core2));
}

/// LOC-B02: one locator has 256 entries and stays within one share
/// (32,768 bytes) of the 1 MiB locator allocation. A core gives at most
/// LOCATOR_SHARES (32) live locators: the 33rd is refused with nothing
/// reserved, a dropped locator's share serves a new one, and all 32
/// together stay within 1 MiB. Another core's shares are its own.
#[test]
fn loc_b02_locators_of_a_core_stay_within_one_mebibyte() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let one = RowLocator::new(&core).unwrap();
    assert_eq!(one.entries.len(), 256);
    assert_eq!(one.working_set(), 0);
    assert!(one.bytes() <= 32_768, "{}", one.bytes());
    println!(
        "LOC_B02 bytes={} header={} entry={}",
        one.bytes(),
        std::mem::size_of::<RowLocator<'_, '_>>(),
        std::mem::size_of::<Option<Entry>>()
    );
    drop(one);
    assert_eq!(core.test_locator_shares(), 0);

    assert_eq!(LOCATOR_SHARES, 32);
    let mut live: Vec<RowLocator<'_, '_>> = (0..LOCATOR_SHARES)
        .map(|_| RowLocator::new(&core).expect("a share"))
        .collect();
    assert_eq!(core.test_locator_shares(), 32);
    assert!(RowLocator::new(&core).is_none(), "a 33rd locator");
    assert_eq!(core.test_locator_shares(), 32);
    let total: usize = live.iter().map(RowLocator::bytes).sum();
    assert!(total <= 1_048_576, "{total}");

    // Another core's shares are its own.
    let fence2 = SessionFence::new();
    let core2 = PageConcurrency::new(&fence2).unwrap();
    let other = RowLocator::new(&core2).unwrap();
    assert_eq!(
        (core.test_locator_shares(), core2.test_locator_shares()),
        (32, 1)
    );
    drop(other);

    live.pop();
    assert_eq!(core.test_locator_shares(), 31);
    live.push(RowLocator::new(&core).expect("the returned share"));
    assert!(RowLocator::new(&core).is_none());
    drop(live);
    assert_eq!(
        (core.test_locator_shares(), core2.test_locator_shares()),
        (0, 0)
    );
    println!("LOC_B02 shares=32 total={total}");
}

/// LOC-B03: race 64 actual callers for 32 shares, hold every result until
/// the observer samples the ledger, then return them concurrently. Two
/// bounded waves prove both refusal and reuse without relying on a winner.
#[test]
fn loc_b03_concurrent_share_admission_and_return() {
    use std::sync::Barrier;
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    for _ in 0..2 {
        let start = Barrier::new(65);
        let acquired = Barrier::new(65);
        let release = Barrier::new(65);
        let (held, admitted, bytes) = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..64)
                .map(|_| {
                    scope.spawn(|| {
                        start.wait();
                        let locator = RowLocator::new(&core);
                        let bytes = locator.as_ref().map(RowLocator::bytes);
                        acquired.wait();
                        release.wait();
                        drop(locator);
                        bytes
                    })
                })
                .collect();
            start.wait();
            acquired.wait();
            let held = core.test_locator_shares();
            // Release before assertions so a failed oracle cannot strand workers.
            release.wait();
            let sizes: Vec<_> = handles
                .into_iter()
                .filter_map(|h| h.join().unwrap())
                .collect();
            (held, sizes.len(), sizes.iter().sum::<usize>())
        });
        assert_eq!((held, admitted), (32, 32));
        assert!(bytes <= 1_048_576);
        assert_eq!(core.test_locator_shares(), 0);
    }
}

/// LOC-B04: the file omitted from the slot hash remains part of equality.
/// Also cover owner equality and the inclusive/exclusive RowId endpoints.
#[test]
fn loc_b04_file_collision_owner_and_rowid_boundaries() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut locator = RowLocator::new(&core).unwrap();
    let t = anchor(20, 2);
    let other_file = PageIdentity {
        page: PageRef {
            file: FileId::new((1 << 32) | 2).unwrap(),
            ..t.page
        },
        ..t
    };
    let other_owner = PageIdentity {
        owner: ObjectId::new(192).unwrap(),
        ..t
    };
    let key = row(u64::MAX);
    let edge = bounds(Some(u64::MAX), None);
    assert_eq!(
        RowLocator::test_slot(t, key),
        RowLocator::test_slot(other_file, key)
    );
    assert_eq!(
        RowLocator::test_slot(t, key),
        RowLocator::test_slot(other_owner, key)
    );
    locator.record(t, key, 1, page(33, 2), edge);
    assert_eq!(locator.lookup(other_file, key, 1), Ok(None));
    assert_eq!(locator.lookup(other_owner, key, 1), Ok(None));
    assert!(locator.lookup(t, key, 1).unwrap().is_some());
    locator.record(other_file, key, 1, page(34, 2), edge);
    assert_eq!(locator.counts().evictions, 1);
    assert_eq!(locator.lookup(t, key, 1), Ok(None));
    assert_eq!(
        locator.lookup(other_file, key, 1).unwrap().unwrap().leaf,
        page(34, 2)
    );
    assert_eq!(locator.working_set(), 1);

    for (value, range, accepted) in [
        (1, bounds(None, Some(1)), false),
        (1, bounds(Some(1), Some(2)), true),
        (2, bounds(Some(1), Some(2)), false),
        (u64::MAX, bounds(None, Some(u64::MAX)), false),
        (u64::MAX, edge, true),
    ] {
        let mut fresh = RowLocator::new(&core).unwrap();
        fresh.record(t, row(value), 1, page(33, 2), range);
        assert_eq!(fresh.lookup(t, row(value), 1).unwrap().is_some(), accepted);
        assert_eq!(fresh.working_set(), usize::from(accepted));
    }
}
