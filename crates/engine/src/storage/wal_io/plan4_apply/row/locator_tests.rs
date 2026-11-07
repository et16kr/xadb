//! LOC-B01/B02 (dev3-cursor-locator.md#locator-tests): the locator table on
//! literal keys. Every anchor, RowId, revision, leaf and bound is a literal.
use super::*;
use crate::api::DatabaseId;
use crate::storage::page::FileId;
use crate::transaction::concurrency::PageConcurrency;
use foundation::identity::{Generation, ObjectId};
use std::sync::atomic::AtomicU8;

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
    resets: u64,
) -> LocatorCounts {
    LocatorCounts {
        lookups,
        hits,
        stale,
        misses,
        evictions,
        resets,
    }
}

/// LOC-B01: a miss, a record and a hit at the same revision; a stale lookup
/// at another revision and its refresh; another anchor identity (same page,
/// another incarnation) is a different key; a collision evicts; an entry
/// whose bounds do not hold its RowId is damaged, never a miss; a route that
/// does not hold the RowId is not recorded; another session origin empties
/// the table.
#[test]
fn loc_b01_lookup_record_and_eviction() {
    let mut locator = RowLocator::new().unwrap();
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
    assert_eq!(locator.working_set(), 2);

    // The same page with another incarnation is another table's anchor.
    let other = anchor(20, 3);
    assert_eq!(locator.lookup(other, row(40), 5), Ok(None));
    assert_eq!(locator.counts().misses, 2);

    // A key whose slot another key holds evicts it.
    let slot = RowLocator::test_slot(t, row(40));
    let colliding = (41..u64::MAX)
        .find(|&r| RowLocator::test_slot(t, row(r)) == slot)
        .unwrap();
    locator.record(t, row(colliding), 5, page(34, 2), right);
    assert_eq!(locator.counts().evictions, 1);
    assert_eq!(locator.lookup(t, row(40), 4), Ok(None));
    assert_eq!(locator.counts().misses, 3);
    assert_eq!(locator.working_set(), 2);

    // A route whose bounds do not hold the RowId is not recorded.
    locator.record(t, row(50), 5, page(33, 2), left);
    assert_eq!(locator.test_entry(t, row(50)), None);

    // A damaged entry (its bounds no longer hold the RowId).
    assert!(locator.test_redirect(t, row(10), page(36, 2)));
    let at = RowLocator::test_slot(t, row(10));
    if let Some(entry) = locator.entries[at].as_mut() {
        entry.bounds = right;
    }
    assert_eq!(locator.lookup(t, row(10), 5), Err(DamagedHint));

    // Another session origin empties the table once; the same one keeps it.
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(0);
    let fence2 = SessionFence::new();
    let core2 = PageConcurrency::new(&fence2).unwrap();
    let binding2 = AtomicU8::new(0);
    locator.bind_origin(&core, &binding, &fence);
    assert_eq!(locator.counts().resets, 0);
    assert_eq!(locator.working_set(), 2);
    locator.bind_origin(&core, &binding, &fence);
    assert_eq!(locator.working_set(), 2);
    locator.bind_origin(&core2, &binding2, &fence2);
    assert_eq!((locator.counts().resets, locator.working_set()), (1, 0));
}

/// LOC-B02: the table has 8,192 entries and the whole locator, header
/// included, stays within its 1 MiB share of metadata/cache.
#[test]
fn loc_b02_locator_stays_within_one_mebibyte() {
    let locator = RowLocator::new().unwrap();
    assert_eq!(locator.entries.len(), 8192);
    assert_eq!(locator.working_set(), 0);
    assert!(locator.bytes() <= 1_048_576, "{}", locator.bytes());
    println!(
        "LOC_B02 bytes={} entry={}",
        locator.bytes(),
        std::mem::size_of::<Option<Entry>>()
    );
}
