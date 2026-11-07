//! Stable tree-root descriptors inside the common page envelope.
//! Validation does not acquire a frame pin/latch or perform an SMO.
//! Wire contract: `devdoc/contracts/dev3-reference-format.md`; the optional
//! Row retired list (cell 1, XTR1) is `dev3-tree-smo.md#empty-leaf-unlink`.
// Remove when the mutable tree connects this representation.
#![allow(dead_code)]

use super::page::{
    FileId, PageError, PageHeader, PageIdentity, PageKind, PageRef, PageView, decode_page,
    encode_page,
};

const ANCHOR_CELL_BYTES: usize = 48;
const RETIRED_HEADER_BYTES: usize = 16;
const RETIRED_ENTRY_BYTES: usize = 40;
/// First-extent dynamic ordinals 9..63 bound the retired list.
pub(crate) const MAX_RETIRED: usize = 55;
pub(crate) const RETIRED_CELL_BYTES: usize =
    RETIRED_HEADER_BYTES + RETIRED_ENTRY_BYTES * MAX_RETIRED;
const FIRST_FILE: u64 = 0x0000_0001_0000_0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeKind {
    Row,
    Index,
    Catalog,
}

impl TreeKind {
    fn leaf(self) -> PageKind {
        match self {
            Self::Row => PageKind::RowLeaf,
            Self::Index => PageKind::IndexLeaf,
            Self::Catalog => PageKind::CatalogLeaf,
        }
    }

    fn root_kind(self, level: u8) -> PageKind {
        if level == 0 {
            self.leaf()
        } else {
            match self {
                Self::Row => PageKind::RowInternal,
                Self::Index => PageKind::IndexInternal,
                Self::Catalog => PageKind::CatalogInternal,
            }
        }
    }
}

/// Root descriptor. Encoding validates level, revision, and anchor non-aliasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnchorRoot {
    pub(crate) tree: TreeKind,
    pub(crate) page: PageRef,
    pub(crate) level: u8,
    pub(crate) revision: u64,
}

impl AnchorRoot {
    fn validate(self, anchor: PageRef) -> Result<(), PageError> {
        if self.level >= 16
            || self.revision == 0
            || (self.page.file == anchor.file && self.page.page_no == anchor.page_no)
        {
            return Err(PageError::InvalidInput);
        }
        Ok(())
    }
}

/// A decoded descriptor. It is a value copy, not a stable live tree capability;
/// callers must acquire pins/latches and revalidate after any unlocked interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TreeAnchor {
    header: PageHeader,
    root: AnchorRoot,
}

impl TreeAnchor {
    pub(crate) fn root(self) -> AnchorRoot {
        self.root
    }

    /// Validate the root envelope against the anchor's qualified identity.
    /// The returned view borrows input and does not validate tree cell semantics.
    pub(crate) fn decode_root(self, input: &[u8]) -> Result<PageView<'_>, PageError> {
        let identity = PageIdentity {
            page: self.root.page,
            ..self.header.identity
        };
        let page = decode_page(input, identity, self.root.tree.root_kind(self.root.level))?;
        if page.header().level != self.root.level {
            return Err(PageError::Corrupt);
        }
        Ok(page)
    }
}

/// One page a Row tree retired while it stays allocated to the tree object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetiredPage {
    pub(crate) page: PageRef,
    pub(crate) kind: PageKind,
    pub(crate) level: u8,
    /// Complete LSN of the retiring Structural group (its owner id).
    pub(crate) retire_lsn: u64,
}

impl RetiredPage {
    /// Encode one exact 40-byte entry. Validation belongs to the whole list.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != RETIRED_ENTRY_BYTES {
            return Err(PageError::InvalidInput);
        }
        self.page.encode(&mut output[..24])?;
        output[24] = self.kind as u8;
        output[25] = self.level;
        output[26..32].fill(0);
        output[32..40].copy_from_slice(&self.retire_lsn.to_le_bytes());
        Ok(())
    }

    fn decode(entry: &[u8]) -> Result<Self, PageError> {
        if entry.len() != RETIRED_ENTRY_BYTES || entry[26..32].iter().any(|b| *b != 0) {
            return Err(PageError::Corrupt);
        }
        let mut lsn = [0; 8];
        lsn.copy_from_slice(&entry[32..40]);
        Ok(Self {
            page: PageRef::decode(&entry[..24])?,
            kind: PageKind::from_byte(entry[24])?,
            level: entry[25],
            retire_lsn: u64::from_le_bytes(lsn),
        })
    }
}

/// Borrowed XTR1 cell of a decoded Row anchor, or no cell. A value from
/// `retired` was validated with its anchor; it grants no ownership by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetiredList<'a> {
    cell: &'a [u8],
}

impl<'a> RetiredList<'a> {
    pub(crate) const EMPTY: RetiredList<'static> = RetiredList { cell: &[] };

    pub(crate) fn len(self) -> usize {
        self.cell.len().saturating_sub(RETIRED_HEADER_BYTES) / RETIRED_ENTRY_BYTES
    }
    pub(crate) fn is_empty(self) -> bool {
        self.cell.is_empty()
    }
    /// Exact XTR1 bytes, empty when the anchor has one cell.
    pub(crate) fn cell(self) -> &'a [u8] {
        self.cell
    }
    pub(crate) fn get(self, index: usize) -> Option<RetiredPage> {
        if index >= self.len() {
            return None;
        }
        let at = RETIRED_HEADER_BYTES + index * RETIRED_ENTRY_BYTES;
        Some(
            RetiredPage::decode(&self.cell[at..at + RETIRED_ENTRY_BYTES])
                .expect("validated retired entry"),
        )
    }
    /// The entry with this exact reference, incarnation included.
    pub(crate) fn find(self, page: PageRef) -> Option<RetiredPage> {
        (0..self.len())
            .map(|i| self.get(i).expect("index below len"))
            .find(|item| item.page == page)
    }

    /// Merge `added` into this list in physical order, writing a new XTR1
    /// cell. Precondition: `added` is sorted by (file, page_no) and shares no
    /// physical page with this list; a caller that breaks it gets a cell
    /// the anchor encoder that uses it refuses (InvalidInput), never a
    /// written list. Errors preserve nothing useful in `output`.
    pub(crate) fn extend(
        self,
        added: &[RetiredPage],
        output: &mut [u8; RETIRED_CELL_BYTES],
    ) -> Result<usize, PageError> {
        let count = self.len() + added.len();
        if added.is_empty() || count > MAX_RETIRED {
            return Err(PageError::InvalidInput);
        }
        let used = RETIRED_HEADER_BYTES + count * RETIRED_ENTRY_BYTES;
        output[..RETIRED_HEADER_BYTES].fill(0);
        output[..4].copy_from_slice(b"XTR1");
        output[4..6].copy_from_slice(&1_u16.to_le_bytes());
        output[6..8].copy_from_slice(&(count as u16).to_le_bytes());
        let (mut old, mut new) = (0, 0);
        for slot in 0..count {
            let take_old = new == added.len()
                || old < self.len()
                    && self.get(old).expect("old entry").page.page_no < added[new].page.page_no;
            let item = if take_old {
                old += 1;
                self.get(old - 1).expect("old entry")
            } else {
                new += 1;
                added[new - 1]
            };
            let at = RETIRED_HEADER_BYTES + slot * RETIRED_ENTRY_BYTES;
            item.encode(&mut output[at..at + RETIRED_ENTRY_BYTES])?;
        }
        Ok(used)
    }
}

/// XTR1 grammar against its anchor: first-file dynamic pages in strict
/// physical order, never the anchor or current root address (an incarnation
/// alias included), Row kinds/levels, and retire LSNs in `1..=anchor_lsn`.
fn check_retired(
    cell: &[u8],
    anchor: PageRef,
    root: AnchorRoot,
    anchor_lsn: u64,
) -> Result<(), PageError> {
    if cell.len() < RETIRED_HEADER_BYTES || &cell[..4] != b"XTR1" {
        return Err(PageError::Corrupt);
    }
    if cell[4..6] != [1, 0] {
        return Err(PageError::UnsupportedFormat);
    }
    let count = usize::from(u16::from_le_bytes([cell[6], cell[7]]));
    if !(1..=MAX_RETIRED).contains(&count)
        || cell.len() != RETIRED_HEADER_BYTES + count * RETIRED_ENTRY_BYTES
        || cell[8..16].iter().any(|b| *b != 0)
        || root.tree != TreeKind::Row
        || root.revision < 2
    {
        return Err(PageError::Corrupt);
    }
    let first = FileId::new(FIRST_FILE).expect("first file");
    let mut previous = None;
    for at in (RETIRED_HEADER_BYTES..cell.len()).step_by(RETIRED_ENTRY_BYTES) {
        let item = RetiredPage::decode(&cell[at..at + RETIRED_ENTRY_BYTES])?;
        let same =
            |other: PageRef| other.file == item.page.file && other.page_no == item.page.page_no;
        if item.page.file != first
            || !(9..64).contains(&item.page.page_no)
            || same(anchor)
            || same(root.page)
            || previous.is_some_and(|page_no| page_no >= item.page.page_no)
            || !matches!(
                (item.kind, item.level),
                (PageKind::RowLeaf, 0) | (PageKind::RowInternal, 1..=15)
            )
            || item.retire_lsn == 0
            || item.retire_lsn > anchor_lsn
        {
            return Err(PageError::Corrupt);
        }
        previous = Some(item.page.page_no);
    }
    Ok(())
}

/// Encode one anchor cell in a page, validating all fields before output writes.
/// Exact page size is required; errors preserve output. No I/O or pin effects.
/// This is the pre-list encoder unchanged; a listed anchor goes through
/// `encode_listed_anchor` (dev3-tree-smo.md#empty-leaf-unlink, UNL-C01).
pub(crate) fn encode_anchor(
    header: PageHeader,
    root: AnchorRoot,
    output: &mut [u8],
) -> Result<(), PageError> {
    if header.kind != PageKind::TreeAnchor || header.level != 0 {
        return Err(PageError::InvalidInput);
    }
    root.validate(header.identity.page)?;
    let mut cell = [0; ANCHOR_CELL_BYTES];
    cell[..4].copy_from_slice(b"XTA1");
    cell[4..6].copy_from_slice(&1_u16.to_le_bytes());
    cell[6] = root.tree.leaf() as u8;
    cell[7] = root.level;
    root.page.encode(&mut cell[8..32])?;
    cell[32..40].copy_from_slice(&root.revision.to_le_bytes());
    encode_page(header, &[&cell], output)
}

/// Encode the anchor and, when `retired` is not empty, that exact XTR1 cell
/// as cell 1. Only a Row anchor of revision 2 or later may carry a list.
/// An empty list is `encode_anchor`.
pub(crate) fn encode_listed_anchor(
    header: PageHeader,
    root: AnchorRoot,
    retired: &[u8],
    output: &mut [u8],
) -> Result<(), PageError> {
    if retired.is_empty() {
        return encode_anchor(header, root, output);
    }
    if header.kind != PageKind::TreeAnchor || header.level != 0 {
        return Err(PageError::InvalidInput);
    }
    root.validate(header.identity.page)?;
    check_retired(retired, header.identity.page, root, header.lsn.0)
        .map_err(|_| PageError::InvalidInput)?;
    let cell = descriptor_cell(root)?;
    encode_page(header, &[&cell, retired], output)
}

/// The XTA1 descriptor cell of a validated root.
fn descriptor_cell(root: AnchorRoot) -> Result<[u8; ANCHOR_CELL_BYTES], PageError> {
    let mut cell = [0; ANCHOR_CELL_BYTES];
    cell[..4].copy_from_slice(b"XTA1");
    cell[4..6].copy_from_slice(&1_u16.to_le_bytes());
    cell[6] = root.tree.leaf() as u8;
    cell[7] = root.level;
    root.page.encode(&mut cell[8..32])?;
    cell[32..40].copy_from_slice(&root.revision.to_le_bytes());
    Ok(cell)
}

/// Validate the anchor envelope, its descriptor cell and, on a Row anchor
/// only, the optional XTR1 cell. Root-only callers ignore the list.
pub(crate) fn decode_anchor(input: &[u8], expected: PageIdentity) -> Result<TreeAnchor, PageError> {
    let page = decode_page(input, expected, PageKind::TreeAnchor)?;
    if !matches!(page.len(), 1 | 2) {
        return Err(PageError::Corrupt);
    }
    let cell = page.cell(0).ok_or(PageError::Corrupt)?;
    if cell.len() != ANCHOR_CELL_BYTES || &cell[..4] != b"XTA1" {
        return Err(PageError::Corrupt);
    }
    if cell[4..6] != [1, 0] {
        return Err(PageError::UnsupportedFormat);
    }
    if cell[40..].iter().any(|b| *b != 0) {
        return Err(PageError::Corrupt);
    }
    let tree = match cell[6] {
        1 => TreeKind::Row,
        3 => TreeKind::Index,
        5 => TreeKind::Catalog,
        _ => return Err(PageError::Corrupt),
    };
    let mut revision = [0; 8];
    revision.copy_from_slice(&cell[32..40]);
    let root = AnchorRoot {
        tree,
        page: PageRef::decode(&cell[8..32])?,
        level: cell[7],
        revision: u64::from_le_bytes(revision),
    };
    root.validate(expected.page)
        .map_err(|_| PageError::Corrupt)?;
    check_list(&page, expected.page, root)?;
    Ok(TreeAnchor {
        header: page.header(),
        root,
    })
}

/// Cell 1 of an anchor envelope, when present, is an XTR1 list valid for
/// its root (`check_retired`). Kept out of `decode_anchor` so the list adds
/// one Result to the root-only decoder frame; this frame and the walk run
/// after `decode_page` has returned.
#[inline(never)]
fn check_list(page: &PageView<'_>, anchor: PageRef, root: AnchorRoot) -> Result<(), PageError> {
    match page.cell(1) {
        None => Ok(()),
        Some(cell) => check_retired(cell, anchor, root, page.header().lsn.0),
    }
}

/// The validated retired list of a Row anchor; `RetiredList::EMPTY` when
/// the anchor has only its descriptor cell. The whole anchor is decoded
/// first, then cell 1 is borrowed from the same verified envelope.
pub(crate) fn retired(input: &[u8], expected: PageIdentity) -> Result<RetiredList<'_>, PageError> {
    decode_anchor(input, expected)?;
    let page = decode_page(input, expected, PageKind::TreeAnchor)?;
    Ok(page
        .cell(1)
        .map_or(RetiredList::EMPTY, |cell| RetiredList { cell }))
}

/// The callee term of a root-only `decode_anchor` call for a census that
/// already counts its own anchor values (the descriptor locals of the
/// pre-list decoder) and passes the page codec term it counted
/// (dev3-tree-smo.md#empty-leaf-unlink, UNL-C01): the `check_list` Result
/// the XTR1 list adds to the decoder frame, then the larger of the page
/// codec and `check_list` with its walk, which run after `decode_page` has
/// returned. A caller replaces its page term with this. `TreeAnchor` keeps
/// its size; that is not this term's argument.
pub(crate) fn decode_source_bytes(page: usize) -> usize {
    use std::mem::size_of;
    2 * size_of::<Result<(), PageError>>() + page.max(list_check_bytes())
}

/// What `decode_source_bytes` adds above a page term a flat census already
/// holds inside its nested-decode allowance.
pub(crate) fn decode_increment_bytes(page: usize) -> usize {
    decode_source_bytes(page) - page
}

/// One complete `decode_anchor` call for a census that counts nothing of the
/// decoder itself (new U1+ code): its whole frame, then the
/// `decode_source_bytes` callee (UNL-C01).
pub(crate) fn whole_decode_source_bytes(page: usize) -> usize {
    decoder_frame_bytes() + decode_source_bytes(page)
}

/// One complete `retired` call for a list reader (UNL-C01): its arguments,
/// the anchor Result, the second envelope view and the borrowed list, then
/// the whole `decode_anchor` frame with its callee, or the second page
/// decode.
pub(crate) fn retired_source_bytes(page: usize) -> usize {
    use std::mem::size_of;
    let frame = size_of::<(&[u8], PageIdentity)>()
        + 2 * size_of::<Result<TreeAnchor, PageError>>()
        + size_of::<PageView<'static>>()
        + 2 * size_of::<Result<PageView<'static>, PageError>>()
        + 2 * size_of::<Option<&[u8]>>()
        + 2 * size_of::<RetiredList<'static>>()
        + size_of::<Result<RetiredList<'static>, PageError>>();
    frame + whole_decode_source_bytes(page).max(page)
}

/// One complete `encode_listed_anchor` call (UNL-C01): its arguments, the
/// descriptor cell and its Result, the two-cell table and the Results, then
/// the largest of the page codec, `descriptor_cell`, the walk and the
/// empty-list `encode_anchor` frame with its page codec.
pub(crate) fn listed_encode_source_bytes(page: usize) -> usize {
    use std::mem::size_of;
    let cell = size_of::<AnchorRoot>()
        + size_of::<[u8; ANCHOR_CELL_BYTES]>()
        + 2 * size_of::<Result<(), PageError>>();
    let root_only = size_of::<(PageHeader, AnchorRoot, &mut [u8])>()
        + size_of::<[u8; ANCHOR_CELL_BYTES]>()
        + size_of::<[&[u8]; 1]>()
        + 4 * size_of::<Result<(), PageError>>()
        + page;
    size_of::<(PageHeader, AnchorRoot, &[u8], &mut [u8])>()
        + size_of::<[u8; ANCHOR_CELL_BYTES]>()
        + 2 * size_of::<Result<[u8; ANCHOR_CELL_BYTES], PageError>>()
        + size_of::<[&[u8]; 2]>()
        + 6 * size_of::<Result<(), PageError>>()
        + page.max(cell).max(walk_source_bytes()).max(root_only)
}

/// The whole `decode_anchor` frame without its callees: arguments, the
/// envelope view and its Result, the descriptor cell, kind, revision bytes,
/// root, the reference and check Results and the returned anchor.
fn decoder_frame_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&[u8], PageIdentity)>()
        + size_of::<PageView<'static>>()
        + 2 * size_of::<Result<PageView<'static>, PageError>>()
        + size_of::<&[u8]>()
        + size_of::<TreeKind>()
        + size_of::<[u8; 8]>()
        + 2 * size_of::<AnchorRoot>()
        + 2 * size_of::<Result<PageRef, PageError>>()
        + 6 * size_of::<Result<(), PageError>>()
        + size_of::<PageHeader>()
        + size_of::<TreeAnchor>()
}

/// `check_list`: arguments, cell 1 and its Result, then the walk.
fn list_check_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&PageView<'static>, PageRef, AnchorRoot)>()
        + 2 * size_of::<Option<&[u8]>>()
        + size_of::<PageHeader>()
        + 2 * size_of::<Result<(), PageError>>()
        + walk_source_bytes()
}

/// `RetiredList::extend`: arguments, counters, the merge cursor and one
/// item with its entry decode and encode (UNL-C01).
pub(crate) fn extend_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(
        RetiredList<'static>,
        &[RetiredPage],
        &mut [u8; RETIRED_CELL_BYTES],
    )>() + 6 * size_of::<usize>()
        + size_of::<std::ops::Range<usize>>()
        + size_of::<bool>()
        + 2 * size_of::<Option<RetiredPage>>()
        + 2 * size_of::<RetiredPage>()
        + 2 * size_of::<Result<usize, PageError>>()
        + 2 * size_of::<Result<(), PageError>>()
        + 2 * size_of::<&[u8]>()
        + entry_source_bytes()
}

/// `check_retired`: arguments, the first file, the walk cursor and previous
/// ordinal, one entry and its Results, the `same` closure, then the entry
/// decode it nests.
fn walk_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&[u8], PageRef, AnchorRoot, u64)>()
        + size_of::<FileId>()
        + size_of::<Option<u64>>()
        + size_of::<std::iter::StepBy<std::ops::Range<usize>>>()
        + 2 * size_of::<usize>()
        + 2 * size_of::<RetiredPage>()
        + 2 * size_of::<Result<RetiredPage, PageError>>()
        + 2 * size_of::<Result<(), PageError>>()
        + size_of::<&RetiredPage>()
        + entry_source_bytes()
}

/// One 40-byte entry decode or encode: the slice, the LSN bytes and the
/// field Results.
fn entry_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<&[u8]>()
        + size_of::<[u8; 8]>()
        + 2 * size_of::<Result<PageRef, PageError>>()
        + 2 * size_of::<Result<PageKind, PageError>>()
        + 2 * size_of::<Result<(), PageError>>()
}

#[cfg(test)]
#[path = "anchor_tests.rs"]
mod tests;
