//! Exact version3 retention cells for the first Row profile. This pure observer
//! grants no issuer, pin, reservation or storage authority. Current genesis
//! and recovery still select Profile2. A pure census does not validate actual
//! Status/Undo/allocator/descriptor graphs or grant current-profile authority.
use super::catalog::{
    CREATE_PEAK as CATALOG_PEAK, MAX_CATALOG_CELL, MIN_CATALOG_CELL, RELEASE_BYTES,
};
use super::{
    ContentHasher, FileId, Generation, JOB_BYTES, JOB_COUNT, PageHeader, PageRef, PageView,
    ROOT_BYTES, RemainingBudget, ResourcePeak, Result, RetentionError, RetentionGenesisPage,
    StatusSlotRef, SystemWorkKind, SystemWorkRef, TxnRef, content_digest, get, header, put, zero,
};
use crate::{api::Namespace, storage::undo_ref::UndoRef};
use foundation::identity::{ObjectHandle, ObjectId, RowId, SchemaEpoch};

#[path = "row/census.rs"]
mod census;
#[path = "row/job.rs"]
mod job;
#[path = "row/pages.rs"]
mod pages;
#[allow(
    unused_imports,
    reason = "pure Job3 supply awaits the closed current-profile owner"
)]
pub(crate) use census::{JobCells, summarize_jobs, verify_census, verify_selected_census};
#[allow(
    unused_imports,
    reason = "pure Job3 supply awaits the closed current-profile owner"
)]
pub(crate) use job::{JobScope, JobState, JobView, decode_job, encode_job};
#[allow(
    unused_imports,
    reason = "pure Row page supply awaits the closed current-profile owner"
)]
pub(crate) use pages::{decode_jobs_page, decode_root_page, encode_jobs_page, encode_root_page};

pub(crate) const ROW_PEAK: ResourcePeak = ResourcePeak {
    frames: 9,
    memory: 425_984,
    locks: 0,
    registry: 16,
    files: 5,
    doublewrite: 1,
};

fn dynamic_page(reference: PageRef) -> bool {
    reference.file == FileId::new(0x0000_0001_0000_0001).expect("first file")
        && (9..64).contains(&reference.page_no)
}
fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}

#[path = "row/root.rs"]
mod root;
#[allow(
    unused_imports,
    reason = "pure Root3 supply awaits the closed current-profile owner"
)]
pub(crate) use root::{RootView, decode_root, encode_root, initial_root};

/// Only the canonical free cell recipe used by the Root3 genesis commitment
/// and the uncalled Profile3 genesis Job pages. This is not an occupied Job3
/// decoder or an inventory/census capability.
pub(in crate::storage) fn free_job(generation: Generation, output: &mut [u8; JOB_BYTES]) {
    output.fill(0);
    output[..4].copy_from_slice(b"XQJ1");
    output[4..6].copy_from_slice(&3u16.to_le_bytes());
    put(output, 8, generation.get());
}

/// Actual Root3 decode closure through either Root page wrapper. It excludes
/// Job3 decoding and census, but includes the initial-root comparison reached
/// before Aging validation: canonical512 + initial_root512 + initial Job1024
/// nest; rows640 is a later branch. Root3 decoding has the Root2 call shape, so
/// the same per-frame multiplicities are counted over the Row types; the Row
/// frame inventory in source_tests checks this bound, not the Root2 number.
pub(crate) fn root_decode_source_bytes() -> usize {
    use crate::storage::{
        allocation_page::AllocationError,
        page::{PageError, PageIdentity, PageKind},
    };
    use std::mem::size_of;
    let hashing = 4 * size_of::<ContentHasher>()
        + 1536 // pinned SHA block/output/compression closure
        + 4 * size_of::<[u8; 32]>();
    let views = 4 * size_of::<RootView<'_>>()
        + 4 * size_of::<Result<RootView<'_>>>()
        + 4 * size_of::<PageHeader>()
        + 2 * size_of::<Result<(PageHeader, RootView<'_>)>>()
        + 4 * size_of::<PageView<'_>>()
        + 2 * size_of::<std::result::Result<PageView<'_>, PageError>>()
        + 2 * size_of::<PageIdentity>();
    let identity = 4 * size_of::<SystemWorkRef>()
        + 2 * size_of::<std::result::Result<SystemWorkRef, AllocationError>>()
        + 6 * size_of::<PageRef>()
        + 4 * size_of::<std::result::Result<PageRef, PageError>>()
        + 4 * size_of::<Result<PageHeader>>();
    let scalar = size_of::<([u64; 3], [usize; 12], [u8; 24], [u8; 16])>()
        + size_of::<(Namespace, RetentionGenesisPage, PageKind, u8, bool)>()
        + 16 * size_of::<&()>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<std::result::Result<(), PageError>>();
    views + identity + scalar + hashing + 2 * ROOT_BYTES + JOB_BYTES.max(640)
}

/// Fixed bound over every Row codec entry: Root3/Job3 cells, both page
/// wrappers, summary and the selected64-cell census with nested validation
/// and return carriers. Entries are separate calls, so the bound is the max of
/// the Root decode closure and the deepest Job/census chain. Caller-owned
/// page/Job arrays and the 640B aggregate scratch are charged by their owners.
pub(crate) fn source_bytes() -> usize {
    use crate::storage::page::PageError;
    use std::mem::size_of;
    type Pair<'a> = (JobView<'a>, JobView<'a>);
    // Frames held above every nested call: census/page wrapper views, cell
    // ranges and selection, verify_pair actors/iterators and decode_job's own
    // decoded values.
    let frames = 16 * size_of::<JobView<'_>>()
        + 8 * size_of::<Result<JobView<'_>>>()
        + 4 * size_of::<JobCells<'_>>()
        + 2 * size_of::<std::array::IntoIter<Pair<'_>, 2>>()
        + 2 * size_of::<RootView<'_>>()
        + 2 * size_of::<PageView<'_>>()
        + 2 * size_of::<std::result::Result<PageView<'_>, PageError>>()
        + 4 * size_of::<PageHeader>()
        + 2 * size_of::<Result<PageHeader>>()
        + 4 * size_of::<Namespace>()
        + 4 * size_of::<(JobState, Option<JobScope>, Generation)>()
        + 4 * size_of::<[u64; 3]>()
        + 32 * size_of::<&()>()
        + 16 * size_of::<Result<()>>()
        + 512; // ranges, indexes, flags and scalar carriers
    // verify_pair (Catalog commit scan or pair-alias closure) → decode_job →
    // validate_root/validate_budget/cleanup → identity decoders.
    let pair_and_cell = 4 * size_of::<Option<TxnRef>>()
        + 2 * size_of::<std::result::Result<TxnRef, PageError>>()
        + 4 * size_of::<Option<StatusSlotRef>>()
        + 2 * size_of::<std::result::Result<StatusSlotRef, PageError>>()
        + 4 * size_of::<Option<UndoRef>>()
        + 2 * size_of::<std::result::Result<UndoRef, PageError>>()
        + 2 * size_of::<Option<(UndoRef, UndoRef)>>()
        + 2 * size_of::<Option<ObjectHandle>>()
        + 2 * size_of::<Option<(ObjectHandle, ObjectHandle)>>()
        + 2 * size_of::<Result<ObjectHandle>>()
        + 2 * size_of::<Option<(RowId, RowId)>>()
        + 4 * size_of::<Option<[PageRef; 2]>>()
        + 2 * size_of::<std::iter::Enumerate<std::array::IntoIter<PageRef, 2>>>()
        + 8 * size_of::<PageRef>()
        + 4 * size_of::<std::result::Result<PageRef, PageError>>()
        // validate_budget's debt/peak and 14-entry (offset, value) table.
        + 3 * size_of::<RemainingBudget>()
        + 2 * size_of::<ResourcePeak>()
        + 2 * size_of::<std::array::IntoIter<(usize, u64), 14>>();
    // Cell/page hashing and the aggregate digest never nest in a pair check.
    let hashing = 2 * size_of::<ContentHasher>() + 4 * size_of::<[u8; 32]>() + 1536;
    root_decode_source_bytes().max(frames + pair_and_cell.max(hashing))
}

#[cfg(test)]
#[path = "row/root_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "row/job_tests.rs"]
mod job_tests;

#[cfg(test)]
#[path = "row/pages_tests.rs"]
mod pages_tests;

#[cfg(test)]
#[path = "row/source_tests.rs"]
pub(in crate::storage) mod source_tests;
