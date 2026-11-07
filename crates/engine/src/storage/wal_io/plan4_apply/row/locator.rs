//! Row locator (dev3-cursor-locator.md#locator): a bounded, caller-owned hint
//! from a table anchor and RowId to the RowLeaf that routed the RowId and the
//! leaf's route bounds, recorded with the anchor root revision of that route.
//! A hint only lets a point read skip the RowInternal pages of a tree whose
//! revision is unchanged; the leaf issue, row seek and every visibility
//! judgement run as without it. Each locator holds one of its core's
//! `LOCATOR_SHARES` shares for its whole life, so the live locators of one
//! storage session together stay within the 1 MiB locator allocation. The
//! table is fixed at creation and never grows; a full slot is overwritten
//! (an eviction is not a logical delete).
use super::*;
use crate::storage::{page::PageRef, tree_page::search::RowRouteBounds};
use crate::transaction::concurrency::{LOCATOR_SHARES, LocatorShare};

/// Entries of one locator's direct-mapped table (a power of two).
const ROW_LOCATOR_ENTRIES: usize = 256;
/// The locator allocation of metadata/cache (dev3-resource-bounds.md).
const ROW_LOCATOR_ALLOCATION: usize = 1_048_576;
/// One share of it: every live locator of a core holds one.
const ROW_LOCATOR_SHARE: usize = ROW_LOCATOR_ALLOCATION / LOCATOR_SHARES;
/// The allocator's own header and rounding of the one table allocation.
const ALLOCATION_SLACK: usize = 64;

const _: () = assert!(ROW_LOCATOR_ENTRIES.is_power_of_two());
const _: () = assert!(LOCATOR_SHARES * ROW_LOCATOR_SHARE <= ROW_LOCATOR_ALLOCATION);
const _: () = assert!(
    std::mem::size_of::<RowLocator<'static, 'static>>()
        + ROW_LOCATOR_ENTRIES * std::mem::size_of::<Option<Entry>>()
        + ALLOCATION_SLACK
        <= ROW_LOCATOR_SHARE
);

/// One hint: the key (the anchor's whole identity, RowId), the anchor root
/// revision of the route and the leaf it reached with the leaf's bounds. No
/// slot, row version or value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    anchor: PageIdentity,
    row: RowId,
    revision: u64,
    leaf: PageRef,
    bounds: RowRouteBounds,
}

/// Counters for the caller; never an input to an answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocatorCounts {
    /// Lookups of a tree whose root level is at least one.
    pub(crate) lookups: u64,
    pub(crate) hits: u64,
    /// The key's entry was recorded at an earlier revision.
    pub(crate) stale: u64,
    /// No entry for the key.
    pub(crate) misses: u64,
    /// A record replaced another key's entry.
    pub(crate) evictions: u64,
    /// Hits whose leaf did not hold the RowId, routed again from the root.
    pub(crate) rechecks: u64,
}

/// A hint the lookup found for the current revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Hint {
    pub(super) leaf: PageRef,
    pub(super) bounds: RowRouteBounds,
}

/// A damaged locator, never a miss: the key's entry has bounds that do not
/// hold its RowId or a revision above the anchor's (only `record` writes
/// entries, and a healthy session's revision never decreases), or a recheck
/// routed elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DamagedHint;

/// The caller-owned locator of one core (one storage session). It holds
/// one of the core's locator shares from `new` to its drop and cannot
/// outlive the core; a table of another core is refused, so no hint crosses
/// an open.
pub(crate) struct RowLocator<'c, 's> {
    share: LocatorShare<'c, 's>,
    entries: Box<[Option<Entry>]>,
    counts: LocatorCounts,
}

impl<'c, 's> RowLocator<'c, 's> {
    /// Take one of `core`'s locator shares and reserve the whole table once.
    /// None: every share is held, or the allocation was refused (its share
    /// is returned); nothing was reserved.
    pub(crate) fn new(core: &'c PageConcurrency<'s>) -> Option<Self> {
        let share = core.take_locator_share()?;
        let mut entries = Vec::new();
        // A refused allocation drops the share, which returns it.
        entries.try_reserve_exact(ROW_LOCATOR_ENTRIES).ok()?;
        entries.resize(ROW_LOCATOR_ENTRIES, None);
        Some(Self {
            share,
            entries: entries.into_boxed_slice(),
            counts: LocatorCounts::default(),
        })
    }
    pub(crate) fn counts(&self) -> LocatorCounts {
        self.counts
    }
    /// Entries now held: the locator's working set.
    pub(crate) fn working_set(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }
    /// Bytes the locator takes of its share: its header, the whole table and
    /// the allocator's slack.
    pub(crate) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.entries.len() * std::mem::size_of::<Option<Entry>>()
            + ALLOCATION_SLACK
    }
    /// Whether `table` is of this locator's core (its storage session).
    pub(super) fn serves(&self, table: &CheckedTable<'c, 's>) -> bool {
        self.serves_core(table.core)
    }
    fn serves_core(&self, core: &PageConcurrency<'_>) -> bool {
        std::ptr::eq(
            (self.share.core() as *const PageConcurrency<'s>).cast::<()>(),
            (core as *const PageConcurrency<'_>).cast::<()>(),
        )
    }
    fn slot(anchor: PageIdentity, row: RowId) -> usize {
        // SplitMix64's finalizer over the anchor's identity but its file
        // (FileId exposes no value) gives the table's offset; consecutive
        // RowIds then take consecutive slots. Only the slot choice, never an
        // identity: the entry compares the whole PageIdentity.
        let database = anchor.database.bytes();
        let (high, low) = database.split_at(8);
        let mut x = anchor.page.page_no
            ^ anchor.page.incarnation.get().rotate_left(47)
            ^ anchor.owner.get().rotate_left(17)
            ^ anchor.owner_generation.get().rotate_left(29)
            ^ u64::from_le_bytes(high.try_into().expect("8 bytes"))
            ^ u64::from_le_bytes(low.try_into().expect("8 bytes")).rotate_left(7);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        (x.wrapping_add(row.get()) as usize) & (ROW_LOCATOR_ENTRIES - 1)
    }
    /// The hint of (`anchor`, `row`) recorded at `revision`. Counted as one
    /// lookup; the caller asks only for a tree of root level one or more.
    pub(super) fn lookup(
        &mut self,
        anchor: PageIdentity,
        row: RowId,
        revision: u64,
    ) -> std::result::Result<Option<Hint>, DamagedHint> {
        self.counts.lookups += 1;
        match self.entries[Self::slot(anchor, row)] {
            Some(e) if e.anchor == anchor && e.row == row => {
                if !contains(e.bounds, row) || e.revision > revision {
                    return Err(DamagedHint);
                }
                if e.revision == revision {
                    self.counts.hits += 1;
                    Ok(Some(Hint {
                        leaf: e.leaf,
                        bounds: e.bounds,
                    }))
                } else {
                    self.counts.stale += 1;
                    Ok(None)
                }
            }
            _ => {
                self.counts.misses += 1;
                Ok(None)
            }
        }
    }
    /// A hit's recheck: the route walked from the root for `row` at
    /// `revision`, after the hinted leaf did not hold the RowId, must be the
    /// hint itself. The call holds the locator exclusively, so the hit's
    /// entry is still there.
    pub(super) fn confirm(
        &mut self,
        anchor: PageIdentity,
        row: RowId,
        revision: u64,
        leaf: PageRef,
        bounds: RowRouteBounds,
    ) -> std::result::Result<(), DamagedHint> {
        self.counts.rechecks += 1;
        match self.entries[Self::slot(anchor, row)] {
            Some(e)
                if e.anchor == anchor
                    && e.row == row
                    && e.revision == revision
                    && e.leaf == leaf
                    && e.bounds == bounds =>
            {
                Ok(())
            }
            _ => Err(DamagedHint),
        }
    }
    /// Record the route C just issued for `row`. A route whose bounds do not
    /// hold the RowId is not recorded.
    pub(super) fn record(
        &mut self,
        anchor: PageIdentity,
        row: RowId,
        revision: u64,
        leaf: PageRef,
        bounds: RowRouteBounds,
    ) {
        if !contains(bounds, row) {
            return;
        }
        let slot = &mut self.entries[Self::slot(anchor, row)];
        if slot.is_some_and(|e| e.anchor != anchor || e.row != row) {
            self.counts.evictions += 1;
        }
        *slot = Some(Entry {
            anchor,
            row,
            revision,
            leaf,
            bounds,
        });
    }
}

/// `lower <= row < upper` in the Row key order (8-byte big-endian RowIds).
fn contains(bounds: RowRouteBounds, row: RowId) -> bool {
    let key = row.get().to_be_bytes();
    bounds.lower.is_none_or(|lower| lower <= key) && bounds.upper.is_none_or(|upper| key < upper)
}

/// One entry's bytes, for the RowRead census.
#[cfg(test)]
pub(super) fn entry_bytes() -> usize {
    std::mem::size_of::<Option<Entry>>()
}

#[cfg(test)]
impl RowLocator<'_, '_> {
    /// The slot of (`anchor`, `row`), for collision cases.
    pub(in crate::storage) fn test_slot(anchor: PageIdentity, row: RowId) -> usize {
        Self::slot(anchor, row)
    }
    /// Replace the leaf of (`anchor`, `row`)'s entry, and its bounds when
    /// given, keeping its key and revision: a damaged hint. False: no such
    /// entry.
    pub(in crate::storage) fn test_redirect(
        &mut self,
        anchor: PageIdentity,
        row: RowId,
        leaf: PageRef,
        bounds: Option<RowRouteBounds>,
    ) -> bool {
        match &mut self.entries[Self::slot(anchor, row)] {
            Some(e) if e.anchor == anchor && e.row == row => {
                e.leaf = leaf;
                if let Some(bounds) = bounds {
                    e.bounds = bounds;
                }
                true
            }
            _ => false,
        }
    }
    /// The entry of (`anchor`, `row`): its revision and leaf.
    pub(in crate::storage) fn test_entry(
        &self,
        anchor: PageIdentity,
        row: RowId,
    ) -> Option<(u64, PageRef)> {
        match self.entries[Self::slot(anchor, row)] {
            Some(e) if e.anchor == anchor && e.row == row => Some((e.revision, e.leaf)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "locator_tests.rs"]
mod tests;
