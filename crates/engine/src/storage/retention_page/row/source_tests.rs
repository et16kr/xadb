//! Frame inventories read from the Row codec source. Each frame sums size_of
//! over the values whose scope encloses its deepest nested call, plus earlier
//! temporaries; SHA is the codebase's pinned 1536B closure constant. These
//! sums check the production bounds; they are not machine-stack measurements.
use super::*;
use crate::{
    storage::{
        allocation_page::AllocationError,
        page::{FileId, PageError, PageIdentity, PageKind},
        retention_genesis::RetentionGenesisError,
    },
    transaction::TxnId,
};
use std::{
    array::{IntoIter, TryFromSliceError},
    iter::Enumerate,
    mem::{size_of, size_of_val},
    ops::{Range, RangeInclusive},
};

pub(in crate::storage) const SHA: usize = 1536;
type Pair<'a> = (JobView<'a>, JobView<'a>);

fn generation_result() -> usize {
    size_of_val(&Generation::new(1))
}
fn database_result() -> usize {
    size_of_val(&crate::api::DatabaseId::new([1; 16]))
}
pub(in crate::storage) fn content_digest_frame() -> usize {
    size_of::<&[u8]>() + size_of::<[u8; 32]>() + size_of::<ContentHasher>()
}
fn page_ref_decode_frame() -> usize {
    size_of::<&[u8]>()
        + size_of::<std::result::Result<PageRef, PageError>>()
        + size_of::<std::result::Result<FileId, PageError>>()
        + generation_result()
}
/// header → expected_header → reference, the deepest identity helper chain.
pub(in crate::storage) fn header_chain() -> usize {
    let header = size_of::<(Namespace, RetentionGenesisPage, u64)>()
        + size_of::<Result<PageHeader>>()
        + size_of::<PageHeader>()
        + size_of::<RetentionGenesisError>()
        + size_of::<std::result::Result<PageHeader, RetentionGenesisError>>();
    let expected = size_of::<(Namespace, RetentionGenesisPage)>()
        + size_of::<std::result::Result<PageHeader, RetentionGenesisError>>()
        + size_of::<(u64, PageKind)>()
        + size_of::<PageHeader>()
        + size_of::<PageIdentity>()
        + size_of_val(&ObjectId::new(4))
        + generation_result();
    let reference = size_of::<u64>()
        + size_of::<PageRef>()
        + size_of::<std::result::Result<FileId, PageError>>()
        + 2 * generation_result();
    header + expected + reference
}
/// page::decode_page while its digest runs, with all later locals included.
pub(in crate::storage) fn decode_page_frame() -> usize {
    size_of::<(&[u8], PageIdentity, PageKind)>()
        + size_of::<std::result::Result<PageView<'_>, PageError>>()
        + size_of::<[u8; 32]>()
        + size_of::<std::result::Result<PageKind, PageError>>()
        + size_of::<(PageKind, u8, [u8; 16])>()
        + size_of::<PageIdentity>()
        + database_result()
        + size_of::<std::result::Result<PageRef, PageError>>()
        + size_of_val(&ObjectId::new(4))
        + generation_result()
        + 7 * size_of::<usize>()
        + size_of::<Range<usize>>()
        + 2 * size_of::<std::slice::Iter<'_, u8>>()
        + size_of::<PageView<'_>>()
        + size_of::<PageHeader>()
}
/// page::encode_page while its digest runs.
pub(in crate::storage) fn encode_page_frame() -> usize {
    size_of::<(PageHeader, &[&[u8]], &mut [u8])>()
        + size_of::<std::result::Result<(), PageError>>()
        + 4 * size_of::<usize>()
        + 2 * size_of::<Option<usize>>()
        + size_of::<std::slice::Iter<'_, &[u8]>>()
        + size_of::<Enumerate<std::slice::Iter<'_, &[u8]>>>()
        + size_of::<(usize, &&[u8])>()
        + size_of::<[u8; 32]>()
}
/// decode_root_page while decode_root runs, with its earlier temporaries.
pub(in crate::storage) fn root_page_frame() -> usize {
    size_of::<(&[u8], Namespace)>()
        + size_of::<Result<(PageHeader, RootView<'_>)>>()
        + size_of::<(PageHeader, RootView<'_>)>()
        + 3 * size_of::<PageHeader>()
        + size_of::<Result<PageHeader>>()
        + size_of::<(PageIdentity, PageKind)>()
        + size_of::<PageView<'_>>()
        + size_of::<std::result::Result<PageView<'_>, PageError>>()
        + size_of::<Option<&[u8]>>()
        + size_of::<Result<&[u8]>>()
        + size_of::<RootView<'_>>()
        + size_of::<Result<RootView<'_>>>()
}
/// encode_root_page while decode_root or encode_page runs.
fn encode_root_page_frame() -> usize {
    size_of::<(&[u8; ROOT_BYTES], Namespace, &mut [u8])>()
        + size_of::<Result<()>>()
        + size_of::<RootView<'_>>()
        + size_of::<Result<RootView<'_>>>()
        + size_of::<Result<PageHeader>>()
        + size_of::<PageHeader>()
        + size_of::<[&[u8]; 1]>()
        + size_of::<&[&[u8]]>()
        + size_of::<std::result::Result<(), PageError>>()
}
fn decode_root_common() -> usize {
    size_of::<(&[u8], Namespace)>()
        + size_of::<Result<RootView<'_>>>()
        + size_of::<&[u8; ROOT_BYTES]>()
        + size_of::<std::result::Result<&[u8; ROOT_BYTES], TryFromSliceError>>()
        + size_of::<[u8; 16]>()
        + size_of::<std::result::Result<PageRef, PageError>>()
        + 2 * size_of::<PageRef>()
        + size_of::<Result<PageHeader>>()
        + size_of::<PageHeader>()
        + size_of::<(Range<u8>, u8)>()
        + size_of::<RootView<'_>>()
        + 3 * size_of::<[u64; 3]>()
        + 2 * size_of::<Option<u64>>()
}
/// decode_root's initial branch while initial_root fills the canonical cell.
pub(in crate::storage) fn decode_root_initial_frame() -> usize {
    decode_root_common() + ROOT_BYTES + size_of::<Result<()>>()
}
fn decode_root_aging_frame() -> usize {
    decode_root_common()
        + size_of::<SystemWorkRef>()
        + size_of::<std::result::Result<SystemWorkRef, AllocationError>>()
        + size_of::<(Namespace, SystemWorkKind, Generation)>()
}
fn system_work_decode_frame() -> usize {
    size_of::<&[u8]>()
        + 2 * size_of::<std::result::Result<SystemWorkRef, AllocationError>>()
        + size_of::<[u8; 16]>()
        + size_of::<Namespace>()
        + database_result()
        + size_of::<std::result::Result<SystemWorkKind, AllocationError>>()
        + 2 * size_of::<u64>()
        + size_of::<std::result::Result<Generation, AllocationError>>()
}
fn initial_root_args() -> usize {
    size_of::<(Namespace, &mut [u8; ROOT_BYTES])>() + size_of::<Result<()>>() + ROOT_BYTES
}
/// initial_root while initial_jobs_digest runs.
pub(in crate::storage) fn initial_root_jobs_frame() -> usize {
    initial_root_args() + size_of::<[u8; 32]>() + size_of::<([u8; 2], [u8; 4], [u8; 4])>()
}
fn initial_root_rows_frame() -> usize {
    initial_root_args() + 640 + size_of::<[u8; 32]>()
}
fn initial_root_header_frame() -> usize {
    initial_root_args()
        + size_of::<[(usize, u64); 3]>()
        + size_of::<IntoIter<(usize, u64), 3>>()
        + size_of::<(usize, u64)>()
        + size_of::<Result<PageHeader>>()
        + size_of::<PageHeader>()
        + size_of::<std::result::Result<(), PageError>>()
        + size_of::<(Range<u8>, u8)>()
}
/// initial_jobs_digest while one hash update runs.
pub(in crate::storage) fn initial_jobs_digest_frame() -> usize {
    size_of::<[u8; 32]>()
        + JOB_BYTES
        + generation_result()
        + size_of::<Generation>()
        + size_of::<ContentHasher>()
        + size_of::<(Range<usize>, usize)>()
}

/// Every Root decode/encode chain the Root closure claims to cover.
pub(in crate::storage) fn root_chains() -> [(&'static str, usize); 8] {
    let initial = decode_root_initial_frame();
    [
        (
            "decode_initial_jobs",
            root_page_frame()
                + initial
                + initial_root_jobs_frame()
                + initial_jobs_digest_frame()
                + SHA,
        ),
        (
            "decode_initial_rows",
            root_page_frame() + initial + initial_root_rows_frame() + content_digest_frame() + SHA,
        ),
        (
            "decode_initial_header",
            root_page_frame() + initial + initial_root_header_frame() + header_chain(),
        ),
        (
            "decode_refs",
            root_page_frame() + decode_root_common() + header_chain() + page_ref_decode_frame(),
        ),
        (
            "decode_aging",
            root_page_frame() + decode_root_aging_frame() + system_work_decode_frame(),
        ),
        (
            "decode_page",
            root_page_frame() + decode_page_frame() + content_digest_frame() + SHA,
        ),
        (
            "encode_initial_jobs",
            encode_root_page_frame()
                + initial
                + initial_root_jobs_frame()
                + initial_jobs_digest_frame()
                + SHA,
        ),
        (
            "encode_page",
            encode_root_page_frame() + encode_page_frame() + content_digest_frame() + SHA,
        ),
    ]
}

fn txn_decode_chain() -> usize {
    let txn_ref = size_of::<&[u8]>()
        + 2 * size_of::<std::result::Result<TxnRef, PageError>>()
        + size_of::<TxnId>()
        + size_of::<std::result::Result<TxnId, PageError>>()
        + size_of::<Generation>()
        + generation_result();
    let txn_id = size_of::<&[u8]>()
        + size_of::<std::result::Result<TxnId, PageError>>()
        + size_of::<([u8; 16], [u8; 4])>()
        + size_of::<TxnId>()
        + database_result();
    txn_ref + txn_id
}
fn status_decode_frame() -> usize {
    size_of::<&[u8]>()
        + 2 * size_of::<std::result::Result<StatusSlotRef, PageError>>()
        + size_of::<PageRef>()
        + size_of::<std::result::Result<PageRef, PageError>>()
        + size_of::<(u16, Generation)>()
        + generation_result()
}
fn undo_decode_frame() -> usize {
    size_of::<&[u8]>()
        + 2 * size_of::<std::result::Result<UndoRef, PageError>>()
        + size_of::<PageRef>()
        + size_of::<std::result::Result<PageRef, PageError>>()
        + size_of::<(u16, [u8; 8], Generation)>()
        + generation_result()
}
/// decode_job while a nested validator or identity decoder runs.
fn decode_job_frame() -> usize {
    size_of::<(&[u8], Namespace)>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<&[u8; JOB_BYTES]>()
        + size_of::<std::result::Result<&[u8; JOB_BYTES], TryFromSliceError>>()
        + size_of::<(JobState, (u8, u8), Generation, Option<JobScope>)>()
        + generation_result()
        + size_of::<JobView<'_>>()
        + size_of::<TxnRef>()
        + size_of::<std::result::Result<TxnRef, PageError>>()
        + size_of::<StatusSlotRef>()
        + size_of::<std::result::Result<StatusSlotRef, PageError>>()
        + size_of::<Result<()>>()
}
fn validate_root_frame() -> usize {
    size_of::<(JobView<'_>, StatusSlotRef)>()
        + size_of::<Result<()>>()
        + size_of::<&[u8; JOB_BYTES]>()
        + size_of::<UndoRef>()
        + size_of::<std::result::Result<UndoRef, PageError>>()
        + size_of::<ObjectHandle>()
        + size_of::<Result<ObjectHandle>>()
        + size_of::<[PageRef; 2]>()
        + 2 * size_of::<std::result::Result<PageRef, PageError>>()
        + size_of::<RangeInclusive<u32>>()
        + size_of::<std::slice::Iter<'_, PageRef>>()
        + size_of::<(&(), Option<JobScope>)>()
}
fn object_frame() -> usize {
    size_of::<&[u8; JOB_BYTES]>()
        + size_of::<Result<ObjectHandle>>()
        + size_of::<ObjectHandle>()
        + 3 * generation_result()
}
fn validate_budget_frame() -> usize {
    size_of::<JobView<'_>>()
        + size_of::<Result<()>>()
        + size_of::<RemainingBudget>()
        + size_of::<ResourcePeak>()
        + size_of::<(Range<usize>, usize, &[u8])>()
        + size_of::<[(usize, u64); 14]>()
        + size_of::<IntoIter<(usize, u64), 14>>()
        + size_of::<(usize, u64)>()
}
fn cleanup_chain() -> usize {
    let cleanup = size_of::<JobView<'_>>()
        + size_of::<RemainingBudget>()
        + size_of::<(u64, u64)>()
        + size_of::<Option<bool>>()
        + size_of::<ResourcePeak>()
        + size_of::<u64>();
    let previous = size_of::<JobView<'_>>() + size_of::<Option<bool>>() + size_of::<&[u8; 640]>();
    cleanup + previous
}
/// decode_job's deepest nested branch.
pub(in crate::storage) fn job_cell_chain() -> usize {
    decode_job_frame()
        + (validate_root_frame() + (undo_decode_frame() + page_ref_decode_frame()))
            .max(validate_root_frame() + object_frame())
            .max(validate_budget_frame() + cleanup_chain())
            .max(txn_decode_chain())
            .max(status_decode_frame() + page_ref_decode_frame())
}
fn verify_census_frame() -> usize {
    size_of::<(
        RootView<'_>,
        Namespace,
        &[[u8; JOB_BYTES]; JOB_COUNT],
        &mut [u8; 640],
    )>() + size_of::<Result<()>>()
        + size_of::<JobCells<'_>>()
}
fn selected_census_frame() -> usize {
    size_of::<(RootView<'_>, Namespace, JobCells<'_>, &mut [u8; 640])>()
        + size_of::<Result<()>>()
        + size_of::<ContentHasher>()
        + 2 * size_of::<[u64; 3]>()
        + size_of::<usize>()
        + 2 * size_of::<(Range<usize>, usize)>()
        + size_of::<&[u8; JOB_BYTES]>()
        + 2 * size_of::<JobView<'_>>()
        + 2 * size_of::<Result<JobView<'_>>>()
        + size_of::<Result<()>>()
        + size_of::<Option<RowId>>()
        + size_of::<Option<ObjectHandle>>()
        + 2 * size_of::<[u8; 32]>()
}
fn verify_pair_args() -> usize {
    size_of::<(JobView<'_>, JobView<'_>, Namespace, JobCells<'_>)>()
        + size_of::<Result<()>>()
        + 2 * size_of::<TxnRef>()
        + 2 * size_of::<StatusSlotRef>()
        + size_of::<bool>()
        + size_of::<IntoIter<Pair<'_>, 2>>()
        + size_of::<Pair<'_>>()
}
/// verify_pair while the Catalog-commit scan decodes another cell.
fn verify_pair_scan_frame() -> usize {
    verify_pair_args()
        + 2 * size_of::<Option<TxnRef>>()
        + 2 * size_of::<Option<StatusSlotRef>>()
        + 2 * size_of::<Option<ObjectHandle>>()
        + size_of::<Option<(ObjectHandle, ObjectHandle)>>()
        + size_of::<(bool, Range<usize>, usize, &[u8; JOB_BYTES])>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<JobView<'_>>()
        + 2 * size_of::<Option<TxnRef>>()
}
/// verify_pair inside the pair-alias closure and same_address.
fn verify_pair_closure_chain() -> usize {
    verify_pair_args()
        + 2 * size_of::<Option<[PageRef; 2]>>()
        + size_of::<[PageRef; 2]>()
        + size_of::<Enumerate<IntoIter<PageRef, 2>>>()
        + size_of::<(usize, PageRef)>()
        + size_of::<Option<StatusSlotRef>>()
        + size_of::<PageRef>()
        + size_of::<Option<UndoRef>>()
        + size_of::<Option<(UndoRef, UndoRef)>>()
        + size_of::<Option<(RowId, RowId)>>()
        + size_of::<&[PageRef; 2]>()
        + size_of::<Enumerate<std::slice::Iter<'_, PageRef>>>()
        + 3 * size_of::<&()>()
        + size_of::<(usize, &PageRef)>()
        + 2 * size_of::<PageRef>()
        + size_of::<bool>()
}
fn verify_terminal_frame() -> usize {
    size_of::<(JobView<'_>, Namespace, JobCells<'_>)>()
        + size_of::<Result<()>>()
        + size_of::<(bool, Range<usize>, usize)>()
        + size_of::<JobView<'_>>()
        + size_of::<Result<JobView<'_>>>()
        + 2 * size_of::<Option<TxnRef>>()
        + 2 * size_of::<Option<StatusSlotRef>>()
        + 2 * size_of::<Option<JobScope>>()
        + size_of::<(bool, bool, u64)>()
}
fn jobs_page_frame() -> usize {
    size_of::<(&[u8], Namespace, u8)>()
        + size_of::<Result<PageView<'_>>>()
        + size_of::<PageHeader>()
        + size_of::<Result<PageHeader>>()
        + size_of::<PageView<'_>>()
        + size_of::<std::result::Result<PageView<'_>, PageError>>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<Option<&[u8]>>()
        + size_of::<Result<&[u8]>>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<JobView<'_>>()
        + size_of::<Result<()>>()
        + size_of::<PageHeader>()
}
fn encode_jobs_page_frame() -> usize {
    size_of::<(Namespace, u8, u64, &[&[u8]], &mut [u8])>()
        + size_of::<Result<()>>()
        + size_of::<PageHeader>()
        + size_of::<Result<PageHeader>>()
        + size_of::<std::slice::Iter<'_, &[u8]>>()
        + size_of::<&&[u8]>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<Result<()>>()
        + size_of::<std::result::Result<(), PageError>>()
}
fn validate_page_job_frame() -> usize {
    size_of::<(JobView<'_>, u64)>() + size_of::<Result<()>>() + size_of::<(JobState, Generation)>()
}
fn summarize_frame() -> usize {
    size_of::<(JobCells<'_>, Namespace, &mut [u8; 640])>()
        + size_of::<Result<([u64; 3], [u8; 32])>>()
        + size_of::<ContentHasher>()
        + size_of::<[u64; 3]>()
        + size_of::<(Range<usize>, usize, &[u8; JOB_BYTES])>()
        + size_of::<JobView<'_>>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<Result<()>>()
        + size_of::<[u8; 32]>()
}
fn add_budget_frame() -> usize {
    size_of::<(&mut [u8; 640], JobView<'_>)>()
        + size_of::<Result<()>>()
        + 2 * size_of::<(Range<usize>, usize)>()
        + size_of::<std::iter::StepBy<Range<usize>>>()
        + 4 * size_of::<u64>()
        + size_of::<Option<u64>>()
        + size_of::<&[u8; 640]>()
}

/// Every Job3/page/census chain covered by the Row codec bound.
pub(in crate::storage) fn job_chains() -> [(&'static str, usize); 12] {
    let census = verify_census_frame() + selected_census_frame();
    [
        (
            "census_pair_scan",
            census + verify_pair_scan_frame() + job_cell_chain(),
        ),
        ("census_pair_closure", census + verify_pair_closure_chain()),
        (
            "census_terminal",
            census + verify_terminal_frame() + job_cell_chain(),
        ),
        ("census_cell", census + job_cell_chain()),
        ("census_budget", census + add_budget_frame()),
        ("census_hash", census + content_digest_frame() + SHA),
        ("summarize", summarize_frame() + job_cell_chain().max(SHA)),
        (
            "decode_jobs_page",
            jobs_page_frame()
                + job_cell_chain()
                    .max(validate_page_job_frame())
                    .max(decode_page_frame() + content_digest_frame() + SHA),
        ),
        (
            "encode_jobs_page",
            encode_jobs_page_frame()
                + job_cell_chain()
                    .max(validate_page_job_frame())
                    .max(header_chain())
                    .max(encode_page_frame() + content_digest_frame() + SHA),
        ),
        (
            "encode_job",
            size_of::<(&[u8], Namespace, &mut [u8])>()
                + size_of::<Result<()>>()
                + size_of::<JobView<'_>>()
                + size_of::<Result<JobView<'_>>>()
                + job_cell_chain(),
        ),
        (
            "encode_root",
            size_of::<(&[u8], Namespace, &mut [u8])>()
                + size_of::<Result<()>>()
                + size_of::<RootView<'_>>()
                + root_chains()[0].1,
        ),
        (
            "free_job",
            size_of::<(Generation, &mut [u8; JOB_BYTES], [u8; 2])>(),
        ),
    ]
}

#[test]
fn row_codec_source_bounds_cover_measured_frames() {
    let root = root_decode_source_bytes();
    let whole = source_bytes();
    let roots = root_chains();
    let jobs = job_chains();
    for (name, bytes) in roots {
        println!("ROW_SOURCE_ROOT_CHAIN {name}={bytes}");
    }
    for (name, bytes) in jobs {
        println!("ROW_SOURCE_JOB_CHAIN {name}={bytes}");
    }
    let root_max = roots.iter().map(|(_, bytes)| *bytes).max().unwrap();
    let job_max = jobs.iter().map(|(_, bytes)| *bytes).max().unwrap();
    let catalog_root = super::super::catalog::root_decode_source_bytes();
    let catalog_whole = super::super::catalog::source_bytes();
    println!(
        "ROW_SOURCE root_decode={root} root_measured={root_max} whole={whole} job_measured={job_max} catalog_root_decode={catalog_root} catalog_whole={catalog_whole}"
    );
    // The Root3 initial branch nests canonical + initial_root + initial Job.
    assert!(root >= 2 * ROOT_BYTES + JOB_BYTES + SHA);
    assert!(root >= root_max);
    assert!(whole >= root && whole >= job_max);
    // The switch replaces the Catalog codec charges; it must not need more.
    assert!(root <= catalog_root && whole <= catalog_whole);
}
