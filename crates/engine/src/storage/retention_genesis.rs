//! Exact plan4 Unstarted/Free queue bytes. Values here grant no live authority.

use super::page::{
    self, FileId, PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind, PageLsn, PageRef,
};
use crate::api::Namespace;
use std::mem::size_of;
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionGenesisPage {
    Root,
    Jobs { index: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionGenesisError {
    InvalidInput,
    UnsupportedFormat,
    Corrupt,
    IdentityMismatch,
    Page(PageError),
}
impl From<PageError> for RetentionGenesisError {
    fn from(error: PageError) -> Self {
        match error {
            PageError::UnsupportedFormat => Self::UnsupportedFormat,
            PageError::IdentityMismatch => Self::IdentityMismatch,
            error => Self::Page(error),
        }
    }
}
type Result<T> = std::result::Result<T, RetentionGenesisError>;

pub(crate) fn expected_header(
    namespace: Namespace,
    page: RetentionGenesisPage,
) -> Result<PageHeader> {
    if namespace.timeline != 1 {
        return Err(RetentionGenesisError::InvalidInput);
    }
    let (page_no, kind) = match page {
        RetentionGenesisPage::Root => (1, PageKind::RetentionRoot),
        RetentionGenesisPage::Jobs { index } if index < 5 => {
            (u64::from(index) + 2, PageKind::RetentionJobs)
        }
        _ => return Err(RetentionGenesisError::InvalidInput),
    };
    Ok(PageHeader {
        identity: PageIdentity {
            database: namespace.database,
            page: reference(page_no),
            owner: ObjectId::new(4).expect("fixed queue object"),
            owner_generation: Generation::new(1).expect("fixed queue generation"),
        },
        kind,
        level: 0,
        lsn: PageLsn(0),
    })
}

fn reference(page_no: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0001).expect("fixed first file"),
        page_no,
        incarnation: Generation::new(1).expect("fixed genesis incarnation"),
    }
}
fn free_job(output: &mut [u8; 1024]) {
    output.fill(0);
    output[..4].copy_from_slice(b"XQJ1");
    output[4..6].copy_from_slice(&1u16.to_le_bytes());
    output[8..16].copy_from_slice(&1u64.to_le_bytes());
}
fn root_cell(namespace: Namespace, output: &mut [u8; 512]) {
    output.fill(0);
    output[..4].copy_from_slice(b"XQR1");
    output[4..6].copy_from_slice(&1u16.to_le_bytes());
    output[8..12].copy_from_slice(&512u32.to_le_bytes());
    output[12..16].copy_from_slice(&64u32.to_le_bytes());
    output[64..72].copy_from_slice(&1u64.to_le_bytes());
    output[96..104].copy_from_slice(&u64::MAX.to_le_bytes());
    {
        let mut job = [0; 1024];
        free_job(&mut job);
        let mut hash = ContentHasher::new();
        for _ in 0..64 {
            hash.update(&job);
        }
        output[128..160].copy_from_slice(&hash.digest());
    }
    {
        // Non-Free-job aggregate uses canonical budget row tags. Free jobs
        // themselves have an absent, all-zero budget rather than these tags.
        let mut aggregate = [0; 640];
        for index in 0..5 {
            aggregate[index * 128] = index as u8 + 1;
        }
        output[160..192].copy_from_slice(&content_digest(&aggregate));
    }
    output[192..208].copy_from_slice(&namespace.database.bytes());
    for (offset, value) in [(208, 1u64), (216, 4), (224, 1)] {
        output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    reference(1)
        .encode(&mut output[232..256])
        .expect("fixed root reference");
    for index in 0..5 {
        let at = 256 + index * 24;
        reference(index as u64 + 2)
            .encode(&mut output[at..at + 24])
            .expect("fixed job page reference");
    }
}

/// Pure encoding; all errors preserve the complete caller output.
pub(crate) fn encode(
    namespace: Namespace,
    page: RetentionGenesisPage,
    output: &mut [u8],
) -> Result<()> {
    let header = expected_header(namespace, page)?;
    if output.len() != PAGE_BYTES {
        return Err(RetentionGenesisError::InvalidInput);
    }
    match page {
        RetentionGenesisPage::Root => {
            let mut cell = [0; 512];
            root_cell(namespace, &mut cell);
            page::encode_page(header, &[&cell], output)?;
        }
        RetentionGenesisPage::Jobs { index } => {
            let mut cell = [0; 1024];
            free_job(&mut cell);
            let cells: [&[u8]; 15] = [&cell; 15];
            page::encode_page(header, &cells[..if index == 4 { 4 } else { 15 }], output)?;
        }
    }
    Ok(())
}

/// Validates only the exact initial state, never occupied or reused queue cells.
pub(crate) fn validate(
    bytes: &[u8],
    namespace: Namespace,
    expected: RetentionGenesisPage,
) -> Result<()> {
    let header = expected_header(namespace, expected)?;
    let view = page::decode_page(bytes, header.identity, header.kind)?;
    if view.header() != header {
        return Err(RetentionGenesisError::Corrupt);
    }
    match expected {
        RetentionGenesisPage::Root => {
            if view.len() != 1 {
                return Err(RetentionGenesisError::Corrupt);
            }
            let actual = view.cell(0).ok_or(RetentionGenesisError::Corrupt)?;
            if actual.len() != 512 || &actual[..4] != b"XQR1" {
                return Err(RetentionGenesisError::Corrupt);
            }
            if actual[4..6] != [1, 0] {
                return Err(RetentionGenesisError::UnsupportedFormat);
            }
            let mut canonical = [0; 512];
            root_cell(namespace, &mut canonical);
            if actual != canonical {
                return Err(RetentionGenesisError::Corrupt);
            }
        }
        RetentionGenesisPage::Jobs { index } => {
            if view.len() != if index == 4 { 4 } else { 15 } {
                return Err(RetentionGenesisError::Corrupt);
            }
            let mut canonical = [0; 1024];
            free_job(&mut canonical);
            for slot in 0..view.len() {
                let actual = view.cell(slot).ok_or(RetentionGenesisError::Corrupt)?;
                if actual.len() != 1024 || &actual[..4] != b"XQJ1" {
                    return Err(RetentionGenesisError::Corrupt);
                }
                if actual[4..6] != [1, 0] {
                    return Err(RetentionGenesisError::UnsupportedFormat);
                }
                if actual != canonical {
                    return Err(RetentionGenesisError::Corrupt);
                }
            }
        }
    }
    Ok(())
}

/// Conservative source-value call closure. Caller page backing is separate.
pub(in crate::storage) fn source_bytes() -> usize {
    let carrier = size_of::<Result<PageHeader>>().max(size_of::<Result<page::PageView<'_>>>());
    512 + 1024
        + 640
        + 15 * size_of::<&[u8]>()
        + 6 * size_of::<PageHeader>()
        + 8 * size_of::<PageRef>()
        + 4 * size_of::<Namespace>()
        + 4 * size_of::<page::PageView<'_>>()
        + 8 * carrier
        + 4 * size_of::<ContentHasher>()
        + 1536
        + 1024
}

#[cfg(test)]
#[path = "retention_genesis_tests.rs"]
pub(in crate::storage) mod tests;
