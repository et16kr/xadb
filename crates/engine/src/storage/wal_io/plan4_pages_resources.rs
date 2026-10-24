//! Source-owned simultaneous values, separate from caller backing and ABI stack.
use super::*;
use crate::storage::{
    anchor::{AnchorRoot, TreeAnchor},
    dirty_frame::{Plan4LoadSnapshot, ReadTargets},
    page::{PageHeader, PageView},
    tree_page::{SearchPosition, TreeExpectation, TreePageView},
};
use std::{
    mem::size_of,
    sync::{MutexGuard, OnceLock},
};
use xadb_foundation::codec::ContentHasher;

// Type-only copies of the pre-G5 directory fields. No value/storage is created.
#[allow(dead_code)]
enum OldMode {
    Fixed,
    AllocatorReloadable,
    OperationalStatus,
    ResidentStatus,
    ResidentMixed,
}
#[allow(dead_code)]
struct OldSlot {
    generation: u64,
    request: OnceLock<TargetRequest>,
}
#[allow(dead_code)]
struct OldDirectory {
    mode: OldMode,
    entries: [OldSlot; dirty_frame::MAX_FRAMES],
    closed_index: Option<usize>,
    resident: u64,
    eviction_epoch: u64,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct Plan4SourceCensus {
    pub(in crate::storage) owner: usize,
    pub(in crate::storage) bound: usize,
    pub(in crate::storage) retained: usize,
    pub(in crate::storage) bind_nested: usize,
    pub(in crate::storage) load_nested: usize,
    pub(in crate::storage) catalog_nested: usize,
    pub(in crate::storage) queue_nested: usize,
    pub(in crate::storage) set3_nested: usize,
    pub(in crate::storage) set4_nested: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) whole: usize,
    pub(in crate::storage) old_common_delta: usize,
    pub(in crate::storage) caller_operation: usize,
    pub(in crate::storage) caller_flush: usize,
    pub(in crate::storage) temporary_files: usize,
}

fn set_bytes<const N: usize>(candidate_nested: usize, load_nested: usize) -> usize {
    // Outer request array and prepare_set's borrowed views survive each nested
    // load; pin/read acquisition begins only after those helpers have returned.
    let outer = size_of::<[TargetRequest; N]>()
        + 2 * size_of::<Plan4ResidencyCursor>()
        + 2 * size_of::<Plan4Target>()
        + 2 * size_of::<Plan4Resident>()
        + 2 * size_of::<Result<Plan4Resident>>()
        + 2 * size_of::<Result<()>>()
        + 24 * size_of::<&()>()
        + 256;
    outer
        + load_nested
            .max(candidate_nested)
            .max(dirty_frame::target_set_call_bytes::<N>())
}

pub(in crate::storage) fn source_census<W: WriterNamespace>() -> Plan4SourceCensus {
    type B<'a, W> = Plan4Pages<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    let core = dirty_frame::plan4_core_census();
    let canonical_nested = genesis::owned_source_bytes::<W>();
    let owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    let bound = size_of::<B<'_, W>>();
    let retained = owner + bound;
    let common = 4 * size_of::<Plan4PageError>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<TargetRequest>()
        + 4 * size_of::<FrameStamp>()
        + 2 * size_of::<wal::WalPrefix>()
        + 4 * size_of::<PageIdentity>()
        + 32 * size_of::<&()>()
        + 32 * size_of::<u64>();
    let bind_read = common
        + size_of::<BindingClaim<'_, '_, '_, W>>()
        + 2 * size_of::<Result<PageIdentity>>()
        + size_of::<MutexGuard<'_, W>>()
        + canonical_nested;
    // SharedPool::new -> new_plan4 has its directory and return carrier before
    // Plan4Pages exists. The bound/Result move is a separate return phase.
    let pool_construct = core.pool
        + size_of::<dirty_frame::Result<SharedPool<'_, '_, '_, '_>>>()
        + core.directory
        + core.load_nested
        + 1024;
    let bound_return = bound
        + size_of::<Result<B<'_, W>>>()
        + size_of::<BindingClaim<'_, '_, '_, W>>()
        + 4 * size_of::<Plan4PageError>()
        + 2 * size_of::<MutexGuard<'_, Option<&mut [u8; FLUSH_BYTES]>>>()
        + 512;
    let bind_nested = bind_read.max(pool_construct).max(bound_return);
    let load_outer = common
        + size_of::<Plan4LoadSnapshot<'_, '_>>()
        + size_of::<dirty_frame::Result<Plan4LoadSnapshot<'_, '_>>>()
        + size_of::<Plan4LoadPermit<'_, '_, '_>>()
        + 2 * size_of::<Plan4LoadObservation>()
        + 2 * size_of::<Result<Plan4LoadObservation>>()
        + size_of::<MutexGuard<'_, W>>()
        + 2 * size_of::<Plan4Target>()
        + 512;
    // The canonical readback bound includes both File carriers and checked
    // readonly namespace/reader/control codecs. read_twice calls do not overlap.
    let load_nested = load_outer + canonical_nested.max(core.load_nested);
    let set3_nested = set_bytes::<3>(core.candidate_nested, load_nested);
    let set4_nested = set_bytes::<4>(core.candidate_nested, load_nested);
    let codec = 4 * size_of::<PageView<'_>>()
        + 4 * size_of::<PageHeader>()
        + 2 * size_of::<TreeAnchor>()
        + 2 * size_of::<AnchorRoot>()
        + 2 * size_of::<TreeExpectation>()
        + 2 * size_of::<TreePageView<'_>>()
        + 2 * size_of::<SearchPosition>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<ContentHasher>()
        + 1536
        + 512;
    let catalog_outer = size_of::<[TargetRequest; 3]>()
        + size_of::<[u8; 8]>()
        + 2 * size_of::<Plan4CatalogLookup>()
        + 2 * size_of::<Result<Plan4CatalogLookup>>()
        + 16 * size_of::<&()>()
        + 256;
    let catalog_nested = catalog_outer
        + set3_nested
            .max(size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, 3>>() + codec + canonical_nested);
    // root512+hasher+counts survive all three windows, including their loads.
    // The two 4-target windows and final 3-target window execute sequentially.
    let queue_outer = 512
        + size_of::<ContentHasher>()
        + 3 * size_of::<TargetRequest>()
        + size_of::<[TargetRequest; 4]>()
        + size_of::<[TargetRequest; 3]>()
        + 2 * size_of::<Plan4ResidencyCursor>()
        + 2 * size_of::<Plan4GenesisCensus>()
        + 2 * size_of::<Result<Plan4GenesisCensus>>()
        + 32
        + 256;
    let queue_nested = queue_outer
        + set4_nested
            .max(set3_nested)
            .max(size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, 4>>() + codec + canonical_nested);
    let active = load_nested.max(catalog_nested).max(queue_nested);
    let peak = (owner + bind_nested).max(retained + active);
    Plan4SourceCensus {
        owner,
        bound,
        retained,
        bind_nested,
        load_nested,
        catalog_nested,
        queue_nested,
        set3_nested,
        set4_nested,
        peak,
        // Existing dirty metadata + reserved flush component ceiling is kept.
        whole: peak + 65536,
        old_common_delta: core.mode.abs_diff(size_of::<OldMode>())
            + core.directory.abs_diff(size_of::<OldDirectory>()),
        caller_operation: OPERATION_BYTES,
        caller_flush: FLUSH_BYTES,
        temporary_files: 2,
    }
}

pub(super) fn fits<W: WriterNamespace>() -> bool {
    let c = source_census::<W>();
    // The retained Mutable owner inherits its existing 32 KiB limit. The
    // original Fresh constructor has its own stricter admission in genesis.
    // Each executing component still has the independent 64 KiB local limit,
    // including the bound pool that remains live throughout each active call.
    // The old dirty-metadata/whole-owner limits remain separate below.
    c.owner <= 32768
        && size_of::<W::File>() <= 16384
        && dirty_frame::fits()
        && wal::durability_types_fit()
        && genesis::fits::<W>()
        && c.old_common_delta == 0
        && c.owner + c.bind_nested <= 65536
        && c.owner + c.load_nested <= 65536
        && c.owner + c.catalog_nested <= 65536
        && c.owner + c.queue_nested <= 65536
        && c.peak <= 65536
        && c.peak <= 196608
        && c.whole <= 262144
}
