//! Row locator (dev3-cursor-locator.md#locator): a bounded, caller-owned hint
//! from a table anchor and RowId to the RowLeaf that routed the RowId and the
//! leaf's route bounds, recorded with the anchor root revision of that route.
//! A hint only lets a point read skip the RowInternal pages of a tree whose
//! revision is unchanged; the leaf issue, row seek and every visibility
//! judgement run as without it. The table is fixed at construction and never
//! grows; a full slot is overwritten (an eviction is not a logical delete).
use super::*;
use crate::storage::{page::PageRef, tree_page::search::RowRouteBounds};

/// Entries of the one direct-mapped table (a power of two).
const ROW_LOCATOR_ENTRIES: usize = 8192;
/// The locator's share of metadata/cache (dev3-resource-bounds.md): the
/// header and the whole table stay within it.
const ROW_LOCATOR_BYTES: usize = 1_048_576;

const _: () = assert!(ROW_LOCATOR_ENTRIES.is_power_of_two());
const _: () = assert!(
    std::mem::size_of::<RowLocator<'static, 'static>>()
        + ROW_LOCATOR_ENTRIES * std::mem::size_of::<Option<Entry>>()
        <= ROW_LOCATOR_BYTES
);

/// One hint: the key (anchor identity with its incarnation, RowId), the
/// anchor root revision of the route and the leaf it reached with the
/// leaf's bounds. No slot, row version or value.
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
    /// The key's entry was recorded at another revision.
    pub(crate) stale: u64,
    /// No entry for the key.
    pub(crate) misses: u64,
    /// A record replaced another key's entry.
    pub(crate) evictions: u64,
    /// The table was emptied for a table of another session origin.
    pub(crate) resets: u64,
}

/// A hint the lookup found for the current revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Hint {
    pub(super) leaf: PageRef,
    pub(super) bounds: RowRouteBounds,
}

/// An entry of the key whose bounds do not hold its RowId: only `record`
/// writes entries, so this is a damaged locator, never a miss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DamagedHint;

/// The caller-owned locator. It remembers the original core, binding and
/// session fence of the first table it serves and cannot outlive them; a
/// table of another origin empties it first, so no hint crosses an open.
pub(crate) struct RowLocator<'c, 's> {
    origin: Option<(&'c PageConcurrency<'s>, &'s AtomicU8, &'s SessionFence)>,
    entries: Box<[Option<Entry>]>,
    counts: LocatorCounts,
}

impl<'c, 's> RowLocator<'c, 's> {
    /// Reserve the whole table once. None: the allocation was refused and
    /// nothing was reserved.
    pub(crate) fn new() -> Option<Self> {
        let mut entries = Vec::new();
        entries.try_reserve_exact(ROW_LOCATOR_ENTRIES).ok()?;
        entries.resize(ROW_LOCATOR_ENTRIES, None);
        Some(Self {
            origin: None,
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
    /// Bytes reserved by the locator: its header and the whole table.
    pub(crate) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.entries.len() * std::mem::size_of::<Option<Entry>>()
    }
    /// Bind to `table`'s session origin, emptying the table for a new one.
    pub(super) fn bind(&mut self, table: &CheckedTable<'c, 's>) {
        self.bind_origin(table.core, table.binding, table.fence);
    }
    fn bind_origin(
        &mut self,
        core: &'c PageConcurrency<'s>,
        binding: &'s AtomicU8,
        fence: &'s SessionFence,
    ) {
        let same = self.origin.is_some_and(|(c, b, f)| {
            std::ptr::eq(c, core) && std::ptr::eq(b, binding) && std::ptr::eq(f, fence)
        });
        if !same {
            if self.origin.is_some() {
                self.clear();
                self.counts.resets += 1;
            }
            self.origin = Some((core, binding, fence));
        }
    }
    fn clear(&mut self) {
        self.entries.iter_mut().for_each(|e| *e = None);
    }
    fn slot(anchor: PageIdentity, row: RowId) -> usize {
        // SplitMix64's finalizer over the table's anchor page, owner and the
        // RowId; only the slot choice, never an identity.
        let mut x = row.get()
            ^ anchor.page.page_no.rotate_left(32)
            ^ anchor.owner.get().rotate_left(17)
            ^ anchor.page.incarnation.get().rotate_left(47);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        (x as usize) & (ROW_LOCATOR_ENTRIES - 1)
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
                if !contains(e.bounds, row) {
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
    /// Replace the leaf of (`anchor`, `row`)'s entry, keeping its key,
    /// revision and bounds: a damaged hint. False: no such entry.
    pub(in crate::storage) fn test_redirect(
        &mut self,
        anchor: PageIdentity,
        row: RowId,
        leaf: PageRef,
    ) -> bool {
        match &mut self.entries[Self::slot(anchor, row)] {
            Some(e) if e.anchor == anchor && e.row == row => {
                e.leaf = leaf;
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
