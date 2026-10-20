//! Fixed row overflow chunks without allocation, I/O, or lifetime ownership.
//! See `devdoc/contracts/dev3-overflow-format.md`. The owner must keep the
//! published chain immutable and pinned for the duration of these borrowed views.
// Remove when the row storage consumer connects this expression.
#![allow(dead_code)]

use super::page::{
    DIGEST_OFFSET, HEADER_BYTES, PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind,
    PageRef, decode_page, encode_page,
};
use xadb_foundation::codec::ContentHasher;

pub(crate) const OVERFLOW_HEAD_BYTES: usize = 64;
pub(crate) const OVERFLOW_HEADER_BYTES: usize = 128;
pub(crate) const MAX_OVERFLOW_PAYLOAD: usize =
    DIGEST_OFFSET - HEADER_BYTES - 4 - OVERFLOW_HEADER_BYTES;
pub(crate) const INLINE_ROW_BYTES: usize = 2048;
pub(crate) const MAX_ROW_BYTES: usize = 128 * 1024;
pub(crate) const MAX_OVERFLOW_CHUNKS: usize = MAX_ROW_BYTES.div_ceil(MAX_OVERFLOW_PAYLOAD);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverflowError {
    InvalidInput,
    UnsupportedFormat,
    Corrupt,
    HeadMismatch,
    OrderMismatch,
    Incomplete,
    Page(PageError),
}

impl From<PageError> for OverflowError {
    fn from(value: PageError) -> Self {
        Self::Page(value)
    }
}

/// Stored by the owning row. This identity is not a pin or allocation token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OverflowHead {
    root: PageRef,
    total: usize,
    digest: [u8; 32],
}

impl OverflowHead {
    pub(crate) fn new(
        root: PageRef,
        total: usize,
        digest: [u8; 32],
    ) -> Result<Self, OverflowError> {
        if !(INLINE_ROW_BYTES + 1..=MAX_ROW_BYTES).contains(&total) {
            return Err(OverflowError::InvalidInput);
        }
        Ok(Self {
            root,
            total,
            digest,
        })
    }

    pub(crate) fn root(self) -> PageRef {
        self.root
    }

    pub(crate) fn total(self) -> usize {
        self.total
    }

    pub(crate) fn count(self) -> usize {
        self.total.div_ceil(MAX_OVERFLOW_PAYLOAD)
    }

    pub(crate) fn digest(self) -> [u8; 32] {
        self.digest
    }

    fn payload_len(self, index: usize) -> Result<usize, OverflowError> {
        if index >= self.count() {
            return Err(OverflowError::InvalidInput);
        }
        Ok((self.total - index * MAX_OVERFLOW_PAYLOAD).min(MAX_OVERFLOW_PAYLOAD))
    }

    /// Exact-size output; no write occurs on failure.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), OverflowError> {
        if output.len() != OVERFLOW_HEAD_BYTES {
            return Err(OverflowError::InvalidInput);
        }
        self.root.encode(&mut output[..24])?;
        output[24..28].copy_from_slice(&(self.total as u32).to_le_bytes());
        output[28..30].copy_from_slice(&(self.count() as u16).to_le_bytes());
        output[30..32].fill(0);
        output[32..64].copy_from_slice(&self.digest);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, OverflowError> {
        if input.len() != OVERFLOW_HEAD_BYTES || input[30..32] != [0, 0] {
            return Err(OverflowError::Corrupt);
        }
        let root = PageRef::decode(&input[..24]).map_err(|_| OverflowError::Corrupt)?;
        let mut digest = [0; 32];
        digest.copy_from_slice(&input[32..64]);
        let head = Self::new(root, read_u32(input, 24) as usize, digest)
            .map_err(|_| OverflowError::Corrupt)?;
        if usize::from(read_u16(input, 28)) != head.count() {
            return Err(OverflowError::Corrupt);
        }
        Ok(head)
    }
}

pub(crate) struct OverflowPageView<'a> {
    header: PageHeader,
    head: OverflowHead,
    index: usize,
    next: Option<PageRef>,
    payload: &'a [u8],
}

impl<'a> OverflowPageView<'a> {
    pub(crate) fn header(&self) -> PageHeader {
        self.header
    }

    pub(crate) fn head(&self) -> OverflowHead {
        self.head
    }

    pub(crate) fn index(&self) -> usize {
        self.index
    }

    pub(crate) fn next(&self) -> Option<PageRef> {
        self.next
    }

    pub(crate) fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}

fn validate_position(
    head: OverflowHead,
    index: usize,
    current: PageRef,
    next: Option<PageRef>,
    payload_len: usize,
) -> Result<(), OverflowError> {
    if payload_len != head.payload_len(index)?
        || (index == 0 && current != head.root)
        || (index > 0 && same_address(current, head.root))
    {
        return Err(OverflowError::InvalidInput);
    }
    match next {
        Some(next)
            if index + 1 < head.count()
                && !same_address(next, current)
                && !same_address(next, head.root) =>
        {
            Ok(())
        }
        None if index + 1 == head.count() => Ok(()),
        _ => Err(OverflowError::InvalidInput),
    }
}

/// Encode into a caller-owned prefix; errors preserve the entire output and
/// success preserves its unused tail. The caller supplies any scratch storage.
pub(crate) fn encode_overflow_cell(
    head: OverflowHead,
    index: usize,
    current: PageRef,
    next: Option<PageRef>,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, OverflowError> {
    validate_position(head, index, current, next, payload.len())?;
    let length = OVERFLOW_HEADER_BYTES + payload.len();
    if output.len() < length {
        return Err(OverflowError::InvalidInput);
    }
    // All fallible bounds and identity checks precede the first write.
    output[..OVERFLOW_HEADER_BYTES].fill(0);
    output[..4].copy_from_slice(b"XOV1");
    put_u16(output, 4, 1);
    put_u16(output, 6, index as u16);
    put_u16(output, 8, head.count() as u16);
    put_u32(output, 12, head.total as u32);
    put_u32(output, 16, payload.len() as u32);
    head.root.encode(&mut output[24..48])?;
    if let Some(next) = next {
        next.encode(&mut output[48..72])?;
    }
    output[72..104].copy_from_slice(&head.digest);
    output[OVERFLOW_HEADER_BYTES..length].copy_from_slice(payload);
    Ok(length)
}

fn parse_cell<'a>(
    cell: &'a [u8],
    header: PageHeader,
    expected: OverflowHead,
    expected_index: usize,
) -> Result<OverflowPageView<'a>, OverflowError> {
    if cell.len() < OVERFLOW_HEADER_BYTES || &cell[..4] != b"XOV1" {
        return Err(OverflowError::Corrupt);
    }
    if read_u16(cell, 4) != 1 {
        return Err(OverflowError::UnsupportedFormat);
    }
    if cell[10..12] != [0, 0]
        || cell[20..24].iter().any(|b| *b != 0)
        || cell[104..128].iter().any(|b| *b != 0)
    {
        return Err(OverflowError::Corrupt);
    }
    let mut digest = [0; 32];
    digest.copy_from_slice(&cell[72..104]);
    let root = PageRef::decode(&cell[24..48]).map_err(|_| OverflowError::Corrupt)?;
    let head = OverflowHead::new(root, read_u32(cell, 12) as usize, digest)
        .map_err(|_| OverflowError::Corrupt)?;
    let index = usize::from(read_u16(cell, 6));
    if usize::from(read_u16(cell, 8)) != head.count() || index >= head.count() {
        return Err(OverflowError::Corrupt);
    }
    let next = if cell[48..72].iter().all(|b| *b == 0) {
        None
    } else {
        Some(PageRef::decode(&cell[48..72]).map_err(|_| OverflowError::Corrupt)?)
    };
    let payload = &cell[OVERFLOW_HEADER_BYTES..];
    if read_u32(cell, 16) as usize != payload.len() {
        return Err(OverflowError::Corrupt);
    }
    validate_position(head, index, header.identity.page, next, payload.len())
        .map_err(|_| OverflowError::Corrupt)?;
    if head != expected {
        return Err(OverflowError::HeadMismatch);
    }
    if index != expected_index {
        return Err(OverflowError::OrderMismatch);
    }
    Ok(OverflowPageView {
        header,
        head,
        index,
        next,
        payload,
    })
}

pub(crate) fn encode_overflow_page(
    header: PageHeader,
    head: OverflowHead,
    index: usize,
    cell: &[u8],
    output: &mut [u8],
) -> Result<(), OverflowError> {
    if header.kind != PageKind::Overflow
        || header.level != 0
        || output.len() != PAGE_BYTES
        || index >= head.count()
    {
        return Err(OverflowError::InvalidInput);
    }
    parse_cell(cell, header, head, index).map_err(|_| OverflowError::InvalidInput)?;
    encode_page(header, &[cell], output)?;
    Ok(())
}

/// The caller's expected identity comes from a stabilized row/previous edge,
/// never from the input page. This function does not acquire a frame guard.
pub(crate) fn decode_overflow_page(
    input: &[u8],
    expected: PageIdentity,
    head: OverflowHead,
    index: usize,
) -> Result<OverflowPageView<'_>, OverflowError> {
    if index >= head.count() {
        return Err(OverflowError::InvalidInput);
    }
    let page = decode_page(input, expected, PageKind::Overflow)?;
    if page.len() != 1 {
        return Err(OverflowError::Corrupt);
    }
    parse_cell(
        page.cell(0).ok_or(OverflowError::Corrupt)?,
        page.header(),
        head,
        index,
    )
}

/// Bounded streaming verification. Payload storage belongs to the caller.
/// Failed pushes preserve this verifier, including its digest and next address.
pub(crate) struct OverflowChainVerifier {
    root_identity: PageIdentity,
    head: OverflowHead,
    next: Option<PageRef>,
    index: usize,
    seen: [Option<PageRef>; MAX_OVERFLOW_CHUNKS],
    hasher: ContentHasher,
}

impl OverflowChainVerifier {
    pub(crate) fn new(
        root_identity: PageIdentity,
        head: OverflowHead,
    ) -> Result<Self, OverflowError> {
        if root_identity.page != head.root {
            return Err(OverflowError::InvalidInput);
        }
        Ok(Self {
            root_identity,
            head,
            next: Some(head.root),
            index: 0,
            seen: [None; MAX_OVERFLOW_CHUNKS],
            hasher: ContentHasher::new(),
        })
    }

    pub(crate) fn push(&mut self, view: &OverflowPageView<'_>) -> Result<(), OverflowError> {
        let next = self.next.ok_or(OverflowError::InvalidInput)?;
        if view.head != self.head {
            return Err(OverflowError::HeadMismatch);
        }
        if view.index != self.index
            || view.header.identity
                != (PageIdentity {
                    page: next,
                    ..self.root_identity
                })
            || self
                .seen
                .iter()
                .flatten()
                .any(|old| same_address(*old, view.header.identity.page))
        {
            return Err(OverflowError::OrderMismatch);
        }
        let mut hasher = self.hasher.clone();
        hasher.update(view.payload);
        if self.index + 1 == self.head.count() && hasher.digest() != self.head.digest {
            return Err(OverflowError::Corrupt);
        }
        // Validated view fields imply exact total length and next presence.
        self.seen[self.index] = Some(view.header.identity.page);
        self.index += 1;
        self.next = view.next;
        self.hasher = hasher;
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<(), OverflowError> {
        if self.index != self.head.count() || self.next.is_some() {
            return Err(OverflowError::Incomplete);
        }
        Ok(())
    }
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
#[path = "overflow_tests.rs"]
mod tests;
