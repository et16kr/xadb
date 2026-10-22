//! Source-owned live-value bound for D3-ALLOC-RESERVE-001, separate from ABI stack.
//! Counts include Copy arguments along the deepest call chain. Moved non-Copy
//! owners count once; caller buffers and a port's own storage remain separate.
use super::{
    AllocationSession, Batch,
    operation::{AnalysisExpected, Plan},
};
use crate::storage::{
    allocation_page::{AllocationEntry, AllocationPage},
    control::{
        ChunkDescriptor, ControlRef, FileMapChunk, FileMapCursor, FileMapEntry, FileMapRoot,
        MapSummary, Profile, SelectedControl,
    },
    obligation::{DescriptorSpec, OwnerState, RemainingBudget},
    page::{PageHeader, PageIdentity, PageView},
    page_io::PageFileReader,
    wal_frame::{RedoRecordView, WalFrameHeader},
    wal_io::WalPrefix,
};
use std::mem::size_of;
use xadb_foundation::{
    codec::ContentHasher,
    namespace::{FileName, WriterNamespace},
};

// These counts intentionally include values whose lifetimes do not overlap.
// Bulk entry/descriptor arrays and Plan copies are charged separately below.
pub(in super::super) const COMMON: usize = 4 * size_of::<AllocationPage<'static>>()
    + 4 * size_of::<PageHeader>()
    + 4 * size_of::<PageIdentity>()
    + 4 * size_of::<WalFrameHeader>()
    + 3 * size_of::<WalPrefix>()
    + 4 * size_of::<Profile>()
    + 4 * size_of::<Batch>()
    + 4 * size_of::<OwnerState>()
    + 4 * size_of::<ControlRef>()
    + 2 * size_of::<RedoRecordView<'static>>()
    + 8 * 32 // digest arrays
    + 32 * size_of::<u64>() // scalar/index/length slots
    + 24 * size_of::<&'static [u8]>() // references and iterator control
    + 512 // narrow codec byte arrays
    + 2048; // nested expectation/owner/namespace/result and iterator carriers

// Fixed sha2-0.10.9 state plus nested block/output/compression storage, reviewed
// for the pinned x86/soft implementation. A dependency change requires review.
pub(in super::super) const HASH: usize = 4 * size_of::<ContentHasher>() + 1536;
pub(in super::super) const ENTRIES: usize = 72 * size_of::<AllocationEntry>() + COMMON + HASH;
pub(in super::super) const ENCODE: usize = size_of::<DescriptorSpec>() + 1536 + COMMON + HASH;
// validate's retained descriptor -> validate_live.current -> expected/decoded
// descriptor, with one spare descriptor for returns. The producer has already
// returned. Include two copied row iterators with their start/end positions.
pub(in super::super) const DESCRIPTORS: usize = 5 * size_of::<DescriptorSpec>()
    + 2 * (size_of::<[RemainingBudget; 5]>() + 2 * size_of::<usize>())
    + COMMON
    + HASH;
pub(in super::super) const TEMPORARIES: usize = max(ENTRIES, max(ENCODE, DESCRIPTORS));
// The trusted workspace-budget wrappers retain two additional copied Plans.
pub(in super::super) const PLANS: usize =
    6 * size_of::<Plan<'static>>() + 2 * size_of::<AnalysisExpected<'static>>() + 128;

// The reader and its retained File are not in this pool. A second File is open
// only while read_page reads the allocator; both are counted in fits below.
pub(in super::super) const CONSTRUCTOR: usize = 12 * size_of::<Profile>()
    + 8 * size_of::<SelectedControl>()
    + 4 * size_of::<FileMapRoot<'static>>()
    + 2 * size_of::<FileMapCursor<'static>>()
    + 3 * size_of::<FileMapChunk<'static>>()
    + 4 * size_of::<ChunkDescriptor>()
    + 4 * size_of::<FileMapEntry>()
    + 8 * size_of::<ControlRef>()
    + 4 * size_of::<MapSummary>()
    + 4 * size_of::<PageView<'static>>()
    + 2 * size_of::<AllocationPage<'static>>()
    + 6 * size_of::<PageHeader>()
    + 4 * size_of::<PageIdentity>()
    + 4 * size_of::<FileName>()
    + 8 * size_of::<AllocationEntry>()
    + 2 * size_of::<WalPrefix>()
    + 2048 // references, scalar/iterator/error carriers and small control bytes
    + HASH;

const fn max(left: usize, right: usize) -> usize {
    if left > right { left } else { right }
}

pub(in super::super) fn fits<W: WriterNamespace>() -> bool {
    let owner = size_of::<AllocationSession<'_, '_, '_, '_, W>>();
    let file = size_of::<W::File>();
    let reader = size_of::<PageFileReader<'_, W>>();
    owner <= 16384
        && file <= 16384
        && COMMON <= 7168
        && HASH <= 2048
        && TEMPORARIES <= 16384
        && CONSTRUCTOR <= 16384
        && PLANS <= 4096
        && owner
            .checked_add(reader)
            .and_then(|sum| sum.checked_add(file))
            .and_then(|sum| sum.checked_add(CONSTRUCTOR))
            .is_some_and(|sum| sum <= 65536)
        && owner + PLANS + TEMPORARIES <= 65536
}
