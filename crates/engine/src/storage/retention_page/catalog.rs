//! Scope2 queue bytes for the original Catalog CREATE consumer. Views borrow
//! canonical cells; they grant no reservation, pin, publication or apply right.
//! The legacy v1 owned values deliberately retain their original size.

use super::{
    ContentHasher, FileId, Generation, JOB_BYTES, JOB_COUNT, PAGE_BYTES, PageHeader, PageKind,
    PageRef, PageView, ROOT_BYTES, RemainingBudget, Result, RetentionError, RetentionGenesisPage,
    StatusSlotRef, SystemWorkKind, SystemWorkRef, TxnRef, content_digest, get, header, page, put,
    zero,
};
use crate::{api::Namespace, storage::undo_ref::UndoRef};
use foundation::identity::{ObjectHandle, ObjectId, SchemaEpoch};

#[path = "catalog/job.rs"]
mod job;
#[path = "catalog/root.rs"]
mod root;
pub(crate) use job::{JobState, JobView, decode_job, encode_job, free_job};
pub(crate) use root::{RootView, decode_root, encode_root, initial_root};

/// Trusted first-consumer reservation recipe, not a live allocation authority.
pub(crate) const CREATE_PEAK: super::ResourcePeak = super::ResourcePeak {
    frames: 9,
    memory: 425_984,
    locks: 0,
    registry: 9,
    files: 5,
    doublewrite: 1,
};
pub(crate) const RELEASE_BYTES: u64 = 55_528;
pub(crate) const MIN_CATALOG_CELL: u32 = 254;
pub(crate) const MAX_CATALOG_CELL: u32 = 4_600;

fn dynamic_page(reference: PageRef) -> bool {
    reference.file == FileId::new(0x0000_0001_0000_0001).expect("first file")
        && (9..64).contains(&reference.page_no)
}
fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}

pub(crate) fn decode_root_page(
    input: &[u8],
    namespace: Namespace,
) -> Result<(PageHeader, RootView<'_>)> {
    let expected = header(namespace, RetentionGenesisPage::Root, 0)?;
    let page = page::decode_page(input, expected.identity, PageKind::RetentionRoot)?;
    if page.len() != 1 || page.header().level != 0 {
        return Err(RetentionError::Corrupt);
    }
    let root = decode_root(page.cell(0).ok_or(RetentionError::Corrupt)?, namespace)?;
    if root.apply() != page.header().lsn.0 {
        return Err(RetentionError::OrderMismatch);
    }
    Ok((page.header(), root))
}

/// Validate the entire candidate before writing the caller's output page.
pub(crate) fn encode_root_page(
    cell: &[u8; ROOT_BYTES],
    namespace: Namespace,
    output: &mut [u8],
) -> Result<()> {
    let root = decode_root(cell, namespace)?;
    page::encode_page(
        header(namespace, RetentionGenesisPage::Root, root.apply())?,
        &[cell],
        output,
    )?;
    Ok(())
}

pub(crate) fn decode_jobs_page(
    input: &[u8],
    namespace: Namespace,
    index: u8,
) -> Result<PageView<'_>> {
    let expected = header(namespace, RetentionGenesisPage::Jobs { index }, 0)?;
    let page = page::decode_page(input, expected.identity, PageKind::RetentionJobs)?;
    if page.header().level != 0 || page.len() != if index == 4 { 4 } else { 15 } {
        return Err(RetentionError::Corrupt);
    }
    for slot in 0..page.len() {
        validate_page_job(
            decode_job(page.cell(slot).ok_or(RetentionError::Corrupt)?, namespace)?,
            page.header().lsn.0,
        )?;
    }
    Ok(page)
}

fn validate_page_job(job: JobView<'_>, lsn: u64) -> Result<()> {
    if job.last_step() > lsn
        || (lsn == 0 && (job.state() != JobState::Free || job.generation().get() != 1))
    {
        return Err(RetentionError::OrderMismatch);
    }
    Ok(())
}

pub(crate) fn encode_jobs_page(
    namespace: Namespace,
    index: u8,
    lsn: u64,
    cells: &[&[u8]],
    output: &mut [u8],
) -> Result<()> {
    let expected = header(namespace, RetentionGenesisPage::Jobs { index }, lsn)?;
    if cells.len() != if index == 4 { 4 } else { 15 } || output.len() != PAGE_BYTES {
        return Err(RetentionError::InvalidInput);
    }
    for cell in cells {
        validate_page_job(decode_job(cell, namespace)?, lsn)?;
    }
    page::encode_page(expected, cells, output)?;
    Ok(())
}

/// The one accumulator lives in caller scratch. Additive debt is checked sum;
/// one Aging executor's peak is max. It is not global concurrent Tx admission.
fn add_budget(output: &mut [u8; 640], job: JobView<'_>) -> Result<()> {
    if job.state() == JobState::Free {
        return Ok(());
    }
    for row in 0..5 {
        for at in (8..120).step_by(8) {
            let offset = row * 128 + at;
            let value = get(job.budget(), offset);
            let current = get(output, offset);
            put(
                output,
                offset,
                if (64..112).contains(&at) {
                    current.max(value)
                } else {
                    current
                        .checked_add(value)
                        .ok_or(RetentionError::BudgetMismatch)?
                },
            );
        }
    }
    Ok(())
}

/// One scope2 group changes at most one Job cell. This view retains the
/// original stable census and substitutes caller scratch without copying 64KiB.
#[derive(Clone, Copy)]
pub(crate) struct JobCells<'a> {
    original: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    replacement: Option<(usize, &'a [u8; JOB_BYTES])>,
}
impl<'a> JobCells<'a> {
    pub(crate) fn original(original: &'a [[u8; JOB_BYTES]; JOB_COUNT]) -> Self {
        Self {
            original,
            replacement: None,
        }
    }
    pub(crate) fn replace(self, ordinal: usize, cell: &'a [u8; JOB_BYTES]) -> Result<Self> {
        if ordinal >= JOB_COUNT || self.replacement.is_some() {
            return Err(RetentionError::InvalidInput);
        }
        Ok(Self {
            original: self.original,
            replacement: Some((ordinal, cell)),
        })
    }
    pub(crate) fn cell(self, ordinal: usize) -> &'a [u8; JOB_BYTES] {
        if let Some((selected, cell)) = self.replacement {
            if selected == ordinal {
                return cell;
            }
        }
        &self.original[ordinal]
    }
}

/// Canonical sums used while constructing an unpublished Root. Full census
/// still checks actor, identity, high-water and terminal relationships below.
pub(crate) fn summarize_jobs(
    cells: JobCells<'_>,
    namespace: Namespace,
    aggregate: &mut [u8; 640],
) -> Result<([u64; 3], [u8; 32])> {
    aggregate.fill(0);
    for row in 0..5 {
        aggregate[row * 128] = row as u8 + 1;
    }
    let mut hash = ContentHasher::new();
    let mut masks = [0u64; 3];
    for ordinal in 0..JOB_COUNT {
        let cell = cells.cell(ordinal);
        let job = decode_job(cell, namespace)?;
        masks[job.state().mask_index()] |= 1u64 << ordinal;
        add_budget(aggregate, job)?;
        hash.update(cell);
    }
    Ok((masks, hash.digest()))
}

/// Full stable64-cell census. Actual Status/Undo/allocator/ATT relations remain
/// the original semantic caller's separate bidirectional obligation.
pub(crate) fn verify_census(
    root: RootView<'_>,
    namespace: Namespace,
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    aggregate: &mut [u8; 640],
) -> Result<()> {
    verify_selected_census(root, namespace, JobCells::original(cells), aggregate)
}

pub(crate) fn verify_selected_census(
    root: RootView<'_>,
    namespace: Namespace,
    cells: JobCells<'_>,
    aggregate: &mut [u8; 640],
) -> Result<()> {
    aggregate.fill(0);
    for row in 0..5 {
        aggregate[row * 128] = row as u8 + 1;
    }
    let mut hash = ContentHasher::new();
    let mut masks = [0u64; 3];
    let mut active = 0usize;
    for ordinal in 0..JOB_COUNT {
        let cell = cells.cell(ordinal);
        let job = decode_job(cell, namespace)?;
        masks[job.state().mask_index()] |= 1u64 << ordinal;
        if job.last_step() > root.apply()
            || (root.aging().is_none() && job.generation().get() != 1)
            || job.commit_q() > root.commit_c()
            || job.undo_sequence() > root.undo_high()
            || job
                .object()
                .is_some_and(|object| object.object.get() > root.object_high())
        {
            return Err(RetentionError::OrderMismatch);
        }
        if job.state() == JobState::TerminalLeased {
            active += 1;
        }
        if job.state().is_root() {
            verify_terminal(job, namespace, cells)?;
        }
        for previous in 0..ordinal {
            verify_pair(job, decode_job(cells.cell(previous), namespace)?)?;
        }
        add_budget(aggregate, job)?;
        hash.update(cell);
    }
    if active > 32
        || masks != root.masks()
        || hash.digest() != *root.job_digest()
        || content_digest(aggregate) != *root.budget_digest()
    {
        return Err(RetentionError::Corrupt);
    }
    Ok(())
}

fn verify_terminal(root: JobView<'_>, namespace: Namespace, cells: JobCells<'_>) -> Result<()> {
    let mut found = false;
    for ordinal in 0..JOB_COUNT {
        let terminal = decode_job(cells.cell(ordinal), namespace)?;
        if !terminal.state().is_terminal() || terminal.source() != root.source() {
            continue;
        }
        if found || terminal.status() != root.status() || terminal.lease_lsn() >= root.lease_lsn() {
            return Err(RetentionError::IdentityMismatch);
        }
        found = true;
        let valid = match root.state() {
            JobState::RootReserved => terminal.state() == JobState::TerminalLeased,
            JobState::RootRetained => {
                terminal.state() == JobState::TerminalLeased || terminal.outcome() == 1
            }
            JobState::RootReleasingCommitted => {
                terminal.outcome() == 1 && terminal.commit_q() == root.commit_q()
            }
            JobState::RootReleasingCompensated => {
                terminal.outcome() == 2
                    || root.release_cursor() == 0 && terminal.state() == JobState::TerminalLeased
            }
            _ => false,
        };
        if !valid {
            return Err(RetentionError::OrderMismatch);
        }
    }
    if !found {
        return Err(RetentionError::IdentityMismatch);
    }
    Ok(())
}

fn verify_pair(a: JobView<'_>, b: JobView<'_>) -> Result<()> {
    if a.state() == JobState::Free || b.state() == JobState::Free {
        return Ok(());
    }
    let (actor_a, actor_b) = (a.source().expect("occupied"), b.source().expect("occupied"));
    let (status_a, status_b) = (a.status().expect("occupied"), b.status().expect("occupied"));
    if actor_a.id() == actor_b.id() {
        if actor_a != actor_b
            || status_a != status_b
            || a.state().is_terminal() == b.state().is_terminal()
        {
            return Err(RetentionError::IdentityMismatch);
        }
    } else if same_address(status_a.page(), status_b.page()) {
        return Err(RetentionError::IdentityMismatch);
    }
    if a.undo()
        .is_some_and(|u| same_address(u.page(), status_b.page()))
        || b.undo()
            .is_some_and(|u| same_address(u.page(), status_a.page()))
        || a.undo()
            .zip(b.undo())
            .is_some_and(|(u, v)| same_address(u.page(), v.page()))
        || a.state().is_root()
            && b.state().is_root()
            && (a.undo_sequence() == b.undo_sequence() || a.object() == b.object())
    {
        return Err(RetentionError::IdentityMismatch);
    }
    for (left, right) in [(a, b), (b, a)] {
        if let Some(pair) = left.pair() {
            for candidate in pair {
                if same_address(candidate, right.status().expect("occupied").page())
                    || right
                        .undo()
                        .is_some_and(|u| same_address(candidate, u.page()))
                    || right
                        .pair()
                        .is_some_and(|other| other.iter().any(|p| same_address(candidate, *p)))
                {
                    return Err(RetentionError::IdentityMismatch);
                }
            }
        }
    }
    Ok(())
}

/// Fixed bound over this codec's actual scalar/views, nested validation and
/// return carriers. Caller-owned page/Job arrays are charged by their owners.
pub(crate) fn source_bytes() -> usize {
    use std::mem::size_of;
    ROOT_BYTES
        + JOB_BYTES
        + 640
        + 8 * size_of::<JobView<'_>>()
        + 4 * size_of::<RootView<'_>>()
        + 8 * size_of::<Result<JobView<'_>>>()
        + 4 * size_of::<Result<RootView<'_>>>()
        + 4 * size_of::<ContentHasher>()
        + 4 * size_of::<SystemWorkRef>()
        + 8 * size_of::<TxnRef>()
        + 8 * size_of::<StatusSlotRef>()
        + 8 * size_of::<UndoRef>()
        + 12 * size_of::<PageRef>()
        + 4 * size_of::<ObjectHandle>()
        + 4 * size_of::<RemainingBudget>()
        + 4 * size_of::<PageView<'_>>()
        + 4 * size_of::<PageHeader>()
        + 16 * size_of::<Result<()>>()
        + 1024
}
