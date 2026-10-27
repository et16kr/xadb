//! Exact version3 retention cells for the first Row profile. This pure observer
//! grants no issuer, pin, reservation or storage authority. Current genesis
//! and recovery still select Profile2. A pure census does not validate actual
//! Status/Undo/allocator/descriptor graphs or grant current-profile authority.
use super::catalog::{
    CREATE_PEAK as CATALOG_PEAK, MAX_CATALOG_CELL, MIN_CATALOG_CELL, RELEASE_BYTES,
};
use super::{
    ContentHasher, FileId, Generation, JOB_BYTES, JOB_COUNT, PageRef, ROOT_BYTES, RemainingBudget,
    ResourcePeak, Result, RetentionError, RetentionGenesisPage, StatusSlotRef, SystemWorkKind,
    SystemWorkRef, TxnRef, content_digest, get, header, put, zero,
};
use crate::{api::Namespace, storage::undo_ref::UndoRef};
use foundation::identity::{ObjectHandle, ObjectId, RowId, SchemaEpoch};

#[path = "row/census.rs"]
mod census;
#[path = "row/job.rs"]
mod job;
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

/// Only the canonical free cell recipe used by the Root3 genesis commitment.
/// This is not an occupied Job3 decoder or an inventory/census capability.
fn free_job(generation: Generation, output: &mut [u8; JOB_BYTES]) {
    output.fill(0);
    output[..4].copy_from_slice(b"XQJ1");
    output[4..6].copy_from_slice(&3u16.to_le_bytes());
    put(output, 8, generation.get());
}

#[cfg(test)]
#[path = "row/root_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "row/job_tests.rs"]
mod job_tests;
