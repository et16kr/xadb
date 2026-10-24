//! DEV3 v1 page envelopes; cell semantics and durability belong to later leaves.
//! See `devdoc/contracts/dev3-page-format.md`. This codec performs no I/O or
//! allocation and does not grant a frame pin, latch, or database capability.
// Remove this narrow allowance when S2 connects the first buffer/page consumer.
#![allow(dead_code)]

use crate::api::DatabaseId;
use xadb_foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

pub(crate) const PAGE_BYTES: usize = 16_384;
pub(crate) const HEADER_BYTES: usize = 128;
pub(crate) const DIGEST_OFFSET: usize = PAGE_BYTES - 32;
pub(crate) const PAGE_REF_BYTES: usize = 24;
pub(crate) const MAX_PAGE_SLOTS: usize = (DIGEST_OFFSET - HEADER_BYTES) / 5;

/// A reserved writer epoch and counter, not an allocator or a filesystem name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileId(u64);

impl FileId {
    pub(crate) fn new(value: u64) -> Result<Self, PageError> {
        if value >> 32 == 0 || value as u32 == 0 {
            return Err(PageError::InvalidInput);
        }
        Ok(Self(value))
    }
}

/// End position of a complete WAL group; zero denotes an unlogged INIT image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageLsn(pub(crate) u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageRef {
    pub(crate) file: FileId,
    pub(crate) page_no: u64,
    pub(crate) incarnation: Generation,
}

impl PageRef {
    /// Encode exactly 24 bytes, preserving the entire output on a size error.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != PAGE_REF_BYTES {
            return Err(PageError::InvalidInput);
        }
        put_u64(output, 0, self.file.0);
        put_u64(output, 8, self.page_no);
        put_u64(output, 16, self.incarnation.get());
        Ok(())
    }

    /// Validate a wire reference, not the existence or ownership of its target.
    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != PAGE_REF_BYTES {
            return Err(PageError::Corrupt);
        }
        Ok(Self {
            file: FileId::new(read_u64(input, 0)).map_err(|_| PageError::Corrupt)?,
            page_no: read_u64(input, 8),
            incarnation: Generation::new(read_u64(input, 16)).map_err(|_| PageError::Corrupt)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageIdentity {
    pub(crate) database: DatabaseId,
    pub(crate) page: PageRef,
    pub(crate) owner: ObjectId,
    pub(crate) owner_generation: Generation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum PageKind {
    RowLeaf = 1,
    RowInternal = 2,
    IndexLeaf = 3,
    IndexInternal = 4,
    CatalogLeaf = 5,
    CatalogInternal = 6,
    Overflow = 7,
    Undo = 8,
    TransactionStatus = 9,
    Allocation = 10,
    TreeAnchor = 11,
    RetentionRoot = 12,
    RetentionJobs = 13,
}

impl PageKind {
    pub(crate) fn from_byte(value: u8) -> Result<Self, PageError> {
        match value {
            1 => Ok(Self::RowLeaf),
            2 => Ok(Self::RowInternal),
            3 => Ok(Self::IndexLeaf),
            4 => Ok(Self::IndexInternal),
            5 => Ok(Self::CatalogLeaf),
            6 => Ok(Self::CatalogInternal),
            7 => Ok(Self::Overflow),
            8 => Ok(Self::Undo),
            9 => Ok(Self::TransactionStatus),
            10 => Ok(Self::Allocation),
            11 => Ok(Self::TreeAnchor),
            12 => Ok(Self::RetentionRoot),
            13 => Ok(Self::RetentionJobs),
            _ => Err(PageError::Corrupt),
        }
    }

    pub(crate) fn accepts_level(self, level: u8) -> bool {
        match self {
            Self::RowInternal | Self::IndexInternal | Self::CatalogInternal => {
                (1..16).contains(&level)
            }
            _ => level == 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageHeader {
    pub(crate) identity: PageIdentity,
    pub(crate) kind: PageKind,
    pub(crate) level: u8,
    pub(crate) lsn: PageLsn,
}

/// All failures are known and have no external effects. Encode errors also
/// preserve the caller's entire output. No retry or fencing policy is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageError {
    InvalidInput,
    UnsupportedFormat,
    Corrupt,
    IdentityMismatch,
    KindMismatch,
    NoSpace,
}

/// Validated envelope borrowing immutable bytes. Does not validate cell keys,
/// child references, undo records, or allocation ownership.
pub(crate) struct PageView<'a> {
    bytes: &'a [u8],
    header: PageHeader,
    slots: usize,
}

impl<'a> PageView<'a> {
    pub(crate) fn header(&self) -> PageHeader {
        self.header
    }

    pub(crate) fn len(&self) -> usize {
        self.slots
    }

    pub(crate) fn cell(&self, slot: usize) -> Option<&'a [u8]> {
        self.cell_range(slot).map(|range| &self.bytes[range])
    }

    /// Exact span from the already validated immutable directory, not a pin.
    pub(super) fn cell_range(&self, slot: usize) -> Option<std::ops::Range<usize>> {
        if slot >= self.slots {
            return None;
        }
        let entry = HEADER_BYTES + 4 * slot;
        let start = usize::from(read_u16(self.bytes, entry));
        let length = usize::from(read_u16(self.bytes, entry + 2));
        Some(start..start + length)
    }
}

/// Encode a compact page into exactly PAGE_BYTES caller-owned bytes.
///
/// Header, cells and available space are checked before any output mutation.
/// There is no allocation, I/O, callback, or lock acquisition. The caller owns
/// quota and synchronization; a successful return does not make the page durable.
pub(crate) fn encode_page(
    header: PageHeader,
    cells: &[&[u8]],
    output: &mut [u8],
) -> Result<(), PageError> {
    if output.len() != PAGE_BYTES || !header.kind.accepts_level(header.level) {
        return Err(PageError::InvalidInput);
    }
    let lower = cells
        .len()
        .checked_mul(4)
        .and_then(|size| HEADER_BYTES.checked_add(size))
        .filter(|end| *end <= DIGEST_OFFSET)
        .ok_or(PageError::NoSpace)?;
    let mut upper = DIGEST_OFFSET;
    for cell in cells {
        if cell.is_empty() {
            return Err(PageError::InvalidInput);
        }
        upper = upper
            .checked_sub(cell.len())
            .filter(|start| *start >= lower)
            .ok_or(PageError::NoSpace)?;
    }

    // From here on every bound and integer narrowing follows from the 16 KiB
    // capacity checks. No fallible step remains after the first output write.
    output.fill(0);
    output[..4].copy_from_slice(b"XDP1");
    put_u16(output, 4, 1);
    output[6] = header.kind as u8;
    output[7] = header.level;
    output[8..24].copy_from_slice(&header.identity.database.bytes());
    put_u64(output, 24, header.identity.page.file.0);
    put_u64(output, 32, header.identity.page.page_no);
    put_u64(output, 40, header.identity.page.incarnation.get());
    put_u64(output, 48, header.identity.owner.get());
    put_u64(output, 56, header.identity.owner_generation.get());
    put_u64(output, 64, header.lsn.0);
    put_u16(output, 72, cells.len() as u16);
    put_u16(output, 74, lower as u16);
    put_u16(output, 76, upper as u16);
    let mut end = DIGEST_OFFSET;
    for (slot, cell) in cells.iter().enumerate() {
        let start = end - cell.len();
        put_u16(output, HEADER_BYTES + 4 * slot, start as u16);
        put_u16(output, HEADER_BYTES + 4 * slot + 2, cell.len() as u16);
        output[start..end].copy_from_slice(cell);
        end = start;
    }
    let digest = content_digest(&output[..DIGEST_OFFSET]);
    output[DIGEST_OFFSET..].copy_from_slice(&digest);
    Ok(())
}

/// Validate every envelope byte and the expected physical/logical identity
/// before exposing any cell. No allocation or mutation occurs.
///
/// The expected identity must come from a stabilized owner, not this untrusted
/// header. A caller following a tree edge must additionally check child level
/// and the cell's semantic contract before using it.
pub(crate) fn decode_page(
    input: &[u8],
    expected: PageIdentity,
    expected_kind: PageKind,
) -> Result<PageView<'_>, PageError> {
    if input.len() != PAGE_BYTES || &input[..4] != b"XDP1" {
        return Err(PageError::Corrupt);
    }
    if read_u16(input, 4) != 1 {
        return Err(PageError::UnsupportedFormat);
    }
    if content_digest(&input[..DIGEST_OFFSET]) != input[DIGEST_OFFSET..] {
        return Err(PageError::Corrupt);
    }
    let kind = PageKind::from_byte(input[6])?;
    let level = input[7];
    if !kind.accepts_level(level) || input[78..HEADER_BYTES].iter().any(|b| *b != 0) {
        return Err(PageError::Corrupt);
    }
    let mut database = [0; 16];
    database.copy_from_slice(&input[8..24]);
    let identity = PageIdentity {
        database: DatabaseId::new(database).map_err(|_| PageError::Corrupt)?,
        page: PageRef::decode(&input[24..48])?,
        owner: ObjectId::new(read_u64(input, 48)).map_err(|_| PageError::Corrupt)?,
        owner_generation: Generation::new(read_u64(input, 56)).map_err(|_| PageError::Corrupt)?,
    };
    let slots = usize::from(read_u16(input, 72));
    let lower = usize::from(read_u16(input, 74));
    let upper = usize::from(read_u16(input, 76));
    if lower != HEADER_BYTES + 4 * slots || lower > upper || upper > DIGEST_OFFSET {
        return Err(PageError::Corrupt);
    }
    let mut end = DIGEST_OFFSET;
    for slot in 0..slots {
        let entry = HEADER_BYTES + 4 * slot;
        let start = usize::from(read_u16(input, entry));
        let length = usize::from(read_u16(input, entry + 2));
        if length == 0 || start < lower || start + length != end {
            return Err(PageError::Corrupt);
        }
        end = start;
    }
    if end != upper || input[lower..upper].iter().any(|b| *b != 0) {
        return Err(PageError::Corrupt);
    }
    if identity != expected {
        return Err(PageError::IdentityMismatch);
    }
    if kind != expected_kind {
        return Err(PageError::KindMismatch);
    }
    Ok(PageView {
        bytes: input,
        header: PageHeader {
            identity,
            kind,
            level,
            lsn: PageLsn(read_u64(input, 64)),
        },
        slots,
    })
}

fn read_u16(bytes: &[u8], start: usize) -> u16 {
    u16::from_le_bytes([bytes[start], bytes[start + 1]])
}

fn read_u64(bytes: &[u8], start: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&bytes[start..start + 8]);
    u64::from_le_bytes(value)
}

fn put_u16(bytes: &mut [u8], start: usize, value: u16) {
    bytes[start..start + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], start: usize, value: u64) {
    bytes[start..start + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
#[path = "page_tests.rs"]
mod tests;
