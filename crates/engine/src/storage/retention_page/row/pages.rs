//! Exact Row-profile page envelopes. Success is not a full64-cell census
//! and grants no page pin, issuer, descriptor, current-profile or I/O authority.
use super::{
    JobState, JobView, Namespace, ROOT_BYTES, Result, RetentionError, RetentionGenesisPage,
    RootView, decode_job, decode_root, header,
};
use crate::storage::page::{self, PAGE_BYTES, PageHeader, PageKind, PageView};

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
