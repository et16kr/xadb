//! Fixed row overflow chunks without allocation, I/O, or lifetime ownership.
//! See `devdoc/contracts/dev3-overflow-format.md` and its revision in
//! `dev3-row-overflow.md#chunk-revision`: chunks are written back to front,
//! so a chunk carries the chain's total and digest but not its root. The
//! owner must keep the published chain immutable and pinned for the duration
//! of these borrowed views.
// Remove when the row storage consumer connects this expression.
#![allow(dead_code)]

use super::page::{
    DIGEST_OFFSET, HEADER_BYTES, PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind,
    PageRef, decode_page, encode_page,
};
use foundation::codec::ContentHasher;

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

/// What every chunk of one chain carries: the row's total bytes and digest.
/// Without a root it names no page; it is known before the first chunk is
/// written (dev3-row-overflow.md#chunk-revision).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OverflowChain {
    total: usize,
    digest: [u8; 32],
}

impl OverflowChain {
    pub(crate) fn new(total: usize, digest: [u8; 32]) -> Result<Self, OverflowError> {
        if !(INLINE_ROW_BYTES + 1..=MAX_ROW_BYTES).contains(&total) {
            return Err(OverflowError::InvalidInput);
        }
        Ok(Self { total, digest })
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

    /// Chunk `index`'s payload length: 16,092 except the last.
    pub(crate) fn payload_len(self, index: usize) -> Result<usize, OverflowError> {
        if index >= self.count() {
            return Err(OverflowError::InvalidInput);
        }
        Ok((self.total - index * MAX_OVERFLOW_PAYLOAD).min(MAX_OVERFLOW_PAYLOAD))
    }

    /// The row's head once chunk 0 is written at `root`.
    pub(crate) fn head(self, root: PageRef) -> OverflowHead {
        OverflowHead {
            root,
            total: self.total,
            digest: self.digest,
        }
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
        // The same range as `OverflowChain::new`, without its Result: the
        // row decode census counts this frame as it was.
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

    /// The root-free part that each chunk of this row's chain carries.
    pub(crate) fn chain(self) -> OverflowChain {
        OverflowChain {
            total: self.total,
            digest: self.digest,
        }
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
    chain: OverflowChain,
    index: usize,
    next: Option<PageRef>,
    payload: &'a [u8],
}

impl<'a> OverflowPageView<'a> {
    pub(crate) fn header(&self) -> PageHeader {
        self.header
    }

    pub(crate) fn chain(&self) -> OverflowChain {
        self.chain
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

/// The local checks of one chunk. Root identity and repeated addresses
/// across the chain belong to `OverflowChainVerifier`, whose visited set
/// starts with the root (dev3-row-overflow.md#chunk-revision).
fn validate_position(
    chain: OverflowChain,
    index: usize,
    current: PageRef,
    next: Option<PageRef>,
    payload_len: usize,
) -> Result<(), OverflowError> {
    if payload_len != chain.payload_len(index)? {
        return Err(OverflowError::InvalidInput);
    }
    match next {
        Some(next) if index + 1 < chain.count() && !same_address(next, current) => Ok(()),
        None if index + 1 == chain.count() => Ok(()),
        _ => Err(OverflowError::InvalidInput),
    }
}

/// Encode into a caller-owned prefix; errors preserve the entire output and
/// success preserves its unused tail. The caller supplies any scratch storage.
pub(crate) fn encode_overflow_cell(
    chain: OverflowChain,
    index: usize,
    current: PageRef,
    next: Option<PageRef>,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, OverflowError> {
    validate_position(chain, index, current, next, payload.len())?;
    let length = OVERFLOW_HEADER_BYTES + payload.len();
    if output.len() < length {
        return Err(OverflowError::InvalidInput);
    }
    // All fallible bounds and identity checks precede the first write.
    output[..OVERFLOW_HEADER_BYTES].fill(0);
    output[..4].copy_from_slice(b"XOV1");
    put_u16(output, 4, 1);
    put_u16(output, 6, index as u16);
    put_u16(output, 8, chain.count() as u16);
    put_u32(output, 12, chain.total as u32);
    put_u32(output, 16, payload.len() as u32);
    if let Some(next) = next {
        next.encode(&mut output[48..72])?;
    }
    output[72..104].copy_from_slice(&chain.digest);
    output[OVERFLOW_HEADER_BYTES..length].copy_from_slice(payload);
    Ok(length)
}

fn parse_cell<'a>(
    cell: &'a [u8],
    header: PageHeader,
    expected: OverflowChain,
    expected_index: usize,
) -> Result<OverflowPageView<'a>, OverflowError> {
    if cell.len() < OVERFLOW_HEADER_BYTES || &cell[..4] != b"XOV1" {
        return Err(OverflowError::Corrupt);
    }
    if read_u16(cell, 4) != 1 {
        return Err(OverflowError::UnsupportedFormat);
    }
    // `[24,48)` held the root before the chunk revision; it is reserved zero.
    if cell[10..12] != [0, 0]
        || cell[20..48].iter().any(|b| *b != 0)
        || cell[104..128].iter().any(|b| *b != 0)
    {
        return Err(OverflowError::Corrupt);
    }
    let mut digest = [0; 32];
    digest.copy_from_slice(&cell[72..104]);
    let chain = OverflowChain::new(read_u32(cell, 12) as usize, digest)
        .map_err(|_| OverflowError::Corrupt)?;
    let index = usize::from(read_u16(cell, 6));
    if usize::from(read_u16(cell, 8)) != chain.count() || index >= chain.count() {
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
    validate_position(chain, index, header.identity.page, next, payload.len())
        .map_err(|_| OverflowError::Corrupt)?;
    if chain != expected {
        return Err(OverflowError::HeadMismatch);
    }
    if index != expected_index {
        return Err(OverflowError::OrderMismatch);
    }
    Ok(OverflowPageView {
        header,
        chain,
        index,
        next,
        payload,
    })
}

pub(crate) fn encode_overflow_page(
    header: PageHeader,
    chain: OverflowChain,
    index: usize,
    cell: &[u8],
    output: &mut [u8],
) -> Result<(), OverflowError> {
    if header.kind != PageKind::Overflow
        || header.level != 0
        || output.len() != PAGE_BYTES
        || index >= chain.count()
    {
        return Err(OverflowError::InvalidInput);
    }
    parse_cell(cell, header, chain, index).map_err(|_| OverflowError::InvalidInput)?;
    encode_page(header, &[cell], output)?;
    Ok(())
}

/// The caller's expected identity comes from a stabilized row/previous edge,
/// never from the input page. This function does not acquire a frame guard.
/// Whether index 0 sits at the row's root is `OverflowChainVerifier`'s check.
pub(crate) fn decode_overflow_page(
    input: &[u8],
    expected: PageIdentity,
    chain: OverflowChain,
    index: usize,
) -> Result<OverflowPageView<'_>, OverflowError> {
    if index >= chain.count() {
        return Err(OverflowError::InvalidInput);
    }
    let page = decode_page(input, expected, PageKind::Overflow)?;
    if page.len() != 1 {
        return Err(OverflowError::Corrupt);
    }
    parse_cell(
        page.cell(0).ok_or(OverflowError::Corrupt)?,
        page.header(),
        chain,
        index,
    )
}

/// Decode a chunk page whose chain is not known in advance: a returned or
/// pending chain, or a retired chunk (dev3-row-overflow.md#chunk-revision).
/// The envelope is checked against `expected` and the cell grammar fully;
/// the caller compares the returned chain, index and next with its owner.
pub(crate) fn inspect_overflow_page(
    input: &[u8],
    expected: PageIdentity,
) -> Result<OverflowPageView<'_>, OverflowError> {
    let page = decode_page(input, expected, PageKind::Overflow)?;
    let cell = match (page.len(), page.cell(0)) {
        (1, Some(cell)) => cell,
        _ => return Err(OverflowError::Corrupt),
    };
    // parse_cell's own order: magic, then version, then fields.
    if cell.len() < OVERFLOW_HEADER_BYTES || &cell[..4] != b"XOV1" {
        return Err(OverflowError::Corrupt);
    }
    if read_u16(cell, 4) != 1 {
        return Err(OverflowError::UnsupportedFormat);
    }
    let mut digest = [0; 32];
    digest.copy_from_slice(&cell[72..104]);
    let chain = OverflowChain::new(read_u32(cell, 12) as usize, digest)
        .map_err(|_| OverflowError::Corrupt)?;
    let index = usize::from(read_u16(cell, 6));
    if index >= chain.count() {
        return Err(OverflowError::Corrupt);
    }
    parse_cell(cell, page.header(), chain, index)
}

/// Walk part of a chain front to back without its head: a pending chain
/// from its front (first index k−written) or a returned chain from its root
/// (first index 0). Each chunk follows the previous next, has the next
/// index and the same chain, and repeats no address; the last ends at index
/// k−1 with no next. No payload digest is checked (dev3-row-overflow.md
/// #chunk-revision). Failed pushes preserve the walk.
pub(crate) struct OverflowSegmentWalk {
    identity: PageIdentity,
    chain: OverflowChain,
    next: Option<PageRef>,
    index: usize,
    seen: [Option<PageRef>; MAX_OVERFLOW_CHUNKS],
    pushed: usize,
}

impl OverflowSegmentWalk {
    pub(crate) fn new(
        front: PageIdentity,
        chain: OverflowChain,
        first: usize,
    ) -> Result<Self, OverflowError> {
        if first >= chain.count() {
            return Err(OverflowError::InvalidInput);
        }
        Ok(Self {
            identity: front,
            chain,
            next: Some(front.page),
            index: first,
            seen: [None; MAX_OVERFLOW_CHUNKS],
            pushed: 0,
        })
    }

    /// The identity the next chunk must have; None once the walk ended.
    pub(crate) fn expected(&self) -> Option<PageIdentity> {
        self.next.map(|page| PageIdentity {
            page,
            ..self.identity
        })
    }

    pub(crate) fn push(&mut self, view: &OverflowPageView<'_>) -> Result<(), OverflowError> {
        let expected = self.expected().ok_or(OverflowError::InvalidInput)?;
        if view.chain != self.chain {
            return Err(OverflowError::HeadMismatch);
        }
        if view.index != self.index
            || view.header.identity != expected
            || self
                .seen
                .iter()
                .flatten()
                .any(|old| same_address(*old, expected.page))
        {
            return Err(OverflowError::OrderMismatch);
        }
        self.seen[self.pushed] = Some(expected.page);
        self.pushed += 1;
        self.index += 1;
        self.next = view.next;
        Ok(())
    }

    /// The pages pushed, in chain order, once the last chunk ended the walk.
    pub(crate) fn finish(&self) -> Result<&[Option<PageRef>], OverflowError> {
        if self.next.is_some() || self.index != self.chain.count() {
            return Err(OverflowError::Incomplete);
        }
        Ok(&self.seen[..self.pushed])
    }
}

/// Bounded streaming verification. Payload storage belongs to the caller.
/// Failed pushes preserve this verifier, including its digest and next address.
/// Chunk 0 must sit at the root identity; the visited set, which starts with
/// the root, rejects a later chunk at the root's or any visited address.
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
        if view.chain != self.head.chain() {
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

/// Source models of the O1 chunk readers (dev3-row-overflow.md#o1-record),
/// explicit sums of each frame's typed carriers, not ABI stack samples. A
/// caller adds the term to its own census; `page` is that census's page codec.
pub(crate) mod source {
    use super::*;
    use crate::storage::page::PageView;
    use std::mem::size_of;

    fn returned<T, E>() -> usize {
        size_of::<T>() + size_of::<Result<T, E>>()
    }

    /// `validate_position`: arguments, the expected length and its Result.
    fn position() -> usize {
        size_of::<(OverflowChain, usize, PageRef, Option<PageRef>, usize)>()
            + returned::<usize, OverflowError>()
            + size_of::<Result<(), OverflowError>>()
    }

    /// `parse_cell`: arguments, digest, chain, index, next and payload with
    /// their Results, the returned view, then the next decode or
    /// `validate_position`.
    fn parse() -> usize {
        size_of::<(&[u8], PageHeader, OverflowChain, usize)>()
            + size_of::<[u8; 32]>()
            + returned::<OverflowChain, OverflowError>()
            + size_of::<usize>()
            + size_of::<Option<PageRef>>()
            + size_of::<&[u8]>()
            + size_of::<Result<OverflowPageView<'_>, OverflowError>>()
            + returned::<PageRef, PageError>().max(position())
    }

    /// `inspect_overflow_page`: arguments, the page view and its Result, the
    /// cell, digest, chain and index, the returned view, then the page decode
    /// or `parse_cell`.
    pub(crate) fn inspect(page: usize) -> usize {
        size_of::<(&[u8], PageIdentity)>()
            + returned::<PageView<'_>, PageError>()
            + size_of::<&[u8]>()
            + size_of::<[u8; 32]>()
            + returned::<OverflowChain, OverflowError>()
            + size_of::<usize>()
            + size_of::<Result<OverflowPageView<'_>, OverflowError>>()
            + page.max(parse())
    }

    /// The walk a caller holds across its chunk loads.
    pub(crate) fn segment_walk() -> usize {
        size_of::<OverflowSegmentWalk>()
    }

    /// `OverflowSegmentWalk::push`: arguments, the expected identity and its
    /// Result, the visited-set scan and the push Result.
    pub(crate) fn segment_push() -> usize {
        size_of::<(&mut OverflowSegmentWalk, &OverflowPageView<'_>)>()
            + size_of::<Option<PageIdentity>>()
            + returned::<PageIdentity, OverflowError>()
            + size_of::<std::iter::Flatten<std::slice::Iter<'_, Option<PageRef>>>>()
            + size_of::<(&PageRef, &PageRef)>()
            + size_of::<Result<(), OverflowError>>()
    }

    /// `OverflowChainVerifier::push`: arguments, next and its Result, the
    /// head's chain, the expected identity, the visited-set scan, the cloned
    /// hasher and digest, then the caller's SHA closure `sha`.
    pub(crate) fn verifier_push(sha: usize) -> usize {
        size_of::<(&mut OverflowChainVerifier, &OverflowPageView<'_>)>()
            + returned::<PageRef, OverflowError>()
            + size_of::<OverflowChain>()
            + size_of::<PageIdentity>()
            + size_of::<std::iter::Flatten<std::slice::Iter<'_, Option<PageRef>>>>()
            + size_of::<ContentHasher>()
            + size_of::<[u8; 32]>()
            + size_of::<Result<(), OverflowError>>()
            + sha
    }
}

#[cfg(test)]
#[path = "overflow_tests.rs"]
mod tests;
