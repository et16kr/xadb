//! Borrowed row/index/catalog tree cells in the DEV3 page envelope.
//! See `devdoc/contracts/dev3-tree-format.md`. This representation performs no
//! allocation, I/O, pinning, tree mutation, or transaction publication.
// Remove when a tree consumer connects this representation leaf.
#![allow(dead_code)]

use std::cmp::Ordering;

use super::{
    key::{MAX_KEY_COLUMNS, PhysicalKey},
    page::{
        DIGEST_OFFSET, HEADER_BYTES, PAGE_REF_BYTES, PageError, PageHeader, PageIdentity, PageKind,
        PageRef, PageView, decode_page, encode_page,
    },
};

pub(crate) const TREE_METADATA_BYTES: usize = 32;
pub(crate) const TREE_ENTRY_CAPACITY: usize =
    DIGEST_OFFSET - HEADER_BYTES - TREE_METADATA_BYTES - 4;
pub(crate) const MAX_TREE_ENTRY_COST: usize = TREE_ENTRY_CAPACITY / 2;
pub(crate) const MAX_TREE_SPLIT_COST: usize = TREE_ENTRY_CAPACITY + MAX_TREE_ENTRY_COST;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeError {
    Page(PageError),
    InvalidInput,
    Corrupt,
    UnsupportedFormat,
    SchemaMismatch,
    LevelMismatch,
    NoSpace,
    EntryTooLarge,
    NotSplittable,
}

impl From<PageError> for TreeError {
    fn from(value: PageError) -> Self {
        Self::Page(value)
    }
}

/// Comes from a stabilized owner/anchor/schema, never the untrusted page header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TreeExpectation {
    pub(crate) identity: PageIdentity,
    pub(crate) kind: PageKind,
    pub(crate) level: u8,
    pub(crate) arity: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryPayload<'a> {
    Value(&'a [u8]),
    RightChild(PageRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TreeEntry<'a> {
    pub(crate) key: &'a [u8],
    pub(crate) payload: EntryPayload<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SearchPosition {
    pub(crate) index: usize,
    pub(crate) equal: bool,
}

/// Entry index excludes metadata. Internal pivot is removed from both children;
/// leaf boundary's separator stays in the right child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SplitPlan<'a> {
    pub(crate) index: usize,
    pub(crate) left_bytes: usize,
    pub(crate) right_bytes: usize,
    pub(crate) separator: &'a [u8],
    pub(crate) right_leftmost: Option<PageRef>,
}

pub(crate) struct TreePageView<'a> {
    page: PageView<'a>,
    arity: u8,
    leftmost: Option<PageRef>,
}

impl<'a> TreePageView<'a> {
    pub(crate) fn header(&self) -> PageHeader {
        self.page.header()
    }

    pub(crate) fn len(&self) -> usize {
        self.page.len() - 1
    }

    pub(crate) fn entry(&self, index: usize) -> Result<TreeEntry<'a>, TreeError> {
        let slot = index.checked_add(1).ok_or(TreeError::InvalidInput)?;
        let cell = self.page.cell(slot).ok_or(TreeError::InvalidInput)?;
        parse_entry(self.header().kind, self.arity, cell)
    }

    pub(crate) fn lower_bound(&self, query: &[u8]) -> Result<SearchPosition, TreeError> {
        let query = checked_key(self.header().kind, self.arity, query)
            .map_err(|_| TreeError::InvalidInput)?;
        let mut left = 0;
        let mut right = self.len();
        while left < right {
            let middle = left + (right - left) / 2;
            let key = checked_key(self.header().kind, self.arity, self.entry(middle)?.key)?;
            if key.compare(query) == Ordering::Less {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        let equal = left < self.len()
            && checked_key(self.header().kind, self.arity, self.entry(left)?.key)?.compare(query)
                == Ordering::Equal;
        Ok(SearchPosition { index: left, equal })
    }

    /// Equality goes to the separator's right child.
    pub(crate) fn route_child(&self, query: &[u8]) -> Result<PageRef, TreeError> {
        if self.leftmost.is_none() {
            return Err(TreeError::InvalidInput);
        }
        let position = self.lower_bound(query)?;
        self.child(position.index + usize::from(position.equal))
    }

    fn child(&self, index: usize) -> Result<PageRef, TreeError> {
        if index == 0 {
            return self.leftmost.ok_or(TreeError::InvalidInput);
        }
        match self.entry(index - 1)?.payload {
            EntryPayload::RightChild(child) => Ok(child),
            EntryPayload::Value(_) => Err(TreeError::InvalidInput),
        }
    }

    /// Carries the parent owner's identity to the selected child's exact level.
    pub(crate) fn child_expectation(&self, index: usize) -> Result<TreeExpectation, TreeError> {
        let header = self.header();
        let page = self.child(index)?;
        let level = header.level.checked_sub(1).ok_or(TreeError::InvalidInput)?;
        let kind = match (header.kind, level == 0) {
            (PageKind::RowInternal, true) => PageKind::RowLeaf,
            (PageKind::RowInternal, false) => PageKind::RowInternal,
            (PageKind::IndexInternal, true) => PageKind::IndexLeaf,
            (PageKind::IndexInternal, false) => PageKind::IndexInternal,
            (PageKind::CatalogInternal, true) => PageKind::CatalogLeaf,
            (PageKind::CatalogInternal, false) => PageKind::CatalogInternal,
            _ => return Err(TreeError::InvalidInput),
        };
        Ok(TreeExpectation {
            identity: PageIdentity {
                page,
                ..header.identity
            },
            kind,
            level,
            arity: self.arity,
        })
    }
}

fn is_leaf(kind: PageKind) -> Result<bool, TreeError> {
    match kind {
        PageKind::RowLeaf | PageKind::IndexLeaf | PageKind::CatalogLeaf => Ok(true),
        PageKind::RowInternal | PageKind::IndexInternal | PageKind::CatalogInternal => Ok(false),
        _ => Err(TreeError::InvalidInput),
    }
}

fn validate_schema(kind: PageKind, arity: u8) -> Result<bool, TreeError> {
    let leaf = is_leaf(kind)?;
    let valid = match kind {
        PageKind::IndexLeaf | PageKind::IndexInternal => {
            (1..=MAX_KEY_COLUMNS).contains(&usize::from(arity))
        }
        _ => arity == 0,
    };
    if !valid {
        return Err(TreeError::InvalidInput);
    }
    Ok(leaf)
}

fn validate_shape(kind: PageKind, level: u8, arity: u8) -> Result<bool, TreeError> {
    let leaf = validate_schema(kind, arity)?;
    if (leaf && level != 0) || (!leaf && !(1..16).contains(&level)) {
        return Err(TreeError::InvalidInput);
    }
    Ok(leaf)
}

#[derive(Clone, Copy)]
enum CheckedKey<'a> {
    Number(u64),
    Index(PhysicalKey<'a>),
}

impl CheckedKey<'_> {
    fn compare(self, other: Self) -> Ordering {
        match (self, other) {
            (Self::Number(left), Self::Number(right)) => left.cmp(&right),
            (Self::Index(left), Self::Index(right)) => left.cmp(&right),
            // Callers compare only keys validated against the same page schema.
            (Self::Number(_), Self::Index(_)) => Ordering::Less,
            (Self::Index(_), Self::Number(_)) => Ordering::Greater,
        }
    }
}

fn checked_key(kind: PageKind, arity: u8, key: &[u8]) -> Result<CheckedKey<'_>, TreeError> {
    match kind {
        PageKind::IndexLeaf | PageKind::IndexInternal => {
            PhysicalKey::decode(key, usize::from(arity))
                .map(CheckedKey::Index)
                .map_err(|_| TreeError::Corrupt)
        }
        PageKind::RowLeaf
        | PageKind::RowInternal
        | PageKind::CatalogLeaf
        | PageKind::CatalogInternal => {
            let bytes: [u8; 8] = key.try_into().map_err(|_| TreeError::Corrupt)?;
            let value = u64::from_be_bytes(bytes);
            if value == 0 {
                return Err(TreeError::Corrupt);
            }
            Ok(CheckedKey::Number(value))
        }
        _ => Err(TreeError::InvalidInput),
    }
}

fn parse_entry(kind: PageKind, arity: u8, cell: &[u8]) -> Result<TreeEntry<'_>, TreeError> {
    if cell.len() < 4 {
        return Err(TreeError::Corrupt);
    }
    let key_end = 4 + usize::from(u16::from_le_bytes([cell[0], cell[1]]));
    let leaf = is_leaf(kind)?;
    let tail = if leaf {
        usize::from(u16::from_le_bytes([cell[2], cell[3]]))
    } else {
        if cell[2..4] != [0, 0] {
            return Err(TreeError::Corrupt);
        }
        PAGE_REF_BYTES
    };
    if key_end + tail != cell.len() {
        return Err(TreeError::Corrupt);
    }
    let key = &cell[4..key_end];
    checked_key(kind, arity, key)?;
    let payload = if leaf {
        EntryPayload::Value(&cell[key_end..])
    } else {
        EntryPayload::RightChild(PageRef::decode(&cell[key_end..]).map_err(|_| TreeError::Corrupt)?)
    };
    Ok(TreeEntry { key, payload })
}

fn same_address(left: PageRef, right: PageRef) -> bool {
    left.file == right.file && left.page_no == right.page_no
}

struct Layout {
    leftmost: Option<PageRef>,
    entry_bytes: usize,
}

/// Validate concrete cells before encode, decode publication, or split. No
/// scratch proportional to page size or entry count is hidden in this routine.
fn validate_cells<'a>(
    header: PageHeader,
    arity: u8,
    count: usize,
    cell_at: impl Fn(usize) -> Option<&'a [u8]>,
    capacity: usize,
) -> Result<Layout, TreeError> {
    let leaf = validate_shape(header.kind, header.level, arity)?;
    let metadata = cell_at(0).ok_or(TreeError::Corrupt)?;
    if metadata.len() != TREE_METADATA_BYTES || &metadata[..4] != b"XBT1" {
        return Err(TreeError::Corrupt);
    }
    if metadata[4..6] != [1, 0] {
        return Err(TreeError::UnsupportedFormat);
    }
    if metadata[7] != 0 {
        return Err(TreeError::Corrupt);
    }
    if validate_schema(header.kind, metadata[6]).is_err() {
        return Err(TreeError::Corrupt);
    }
    if metadata[6] != arity {
        return Err(TreeError::SchemaMismatch);
    }
    let leftmost = if leaf {
        if metadata[8..].iter().any(|byte| *byte != 0) {
            return Err(TreeError::Corrupt);
        }
        None
    } else {
        if count < 2 {
            return Err(TreeError::Corrupt);
        }
        let child = PageRef::decode(&metadata[8..]).map_err(|_| TreeError::Corrupt)?;
        if same_address(child, header.identity.page) {
            return Err(TreeError::Corrupt);
        }
        Some(child)
    };
    let mut entry_bytes = 0usize;
    let mut previous = None;
    for slot in 1..count {
        let cell = cell_at(slot).ok_or(TreeError::Corrupt)?;
        let cost = cell.len().checked_add(4).ok_or(TreeError::EntryTooLarge)?;
        if cost > MAX_TREE_ENTRY_COST {
            return Err(TreeError::EntryTooLarge);
        }
        entry_bytes = entry_bytes
            .checked_add(cost)
            .filter(|total| *total <= capacity)
            .ok_or(TreeError::NoSpace)?;
        let entry = parse_entry(header.kind, arity, cell)?;
        let key = checked_key(header.kind, arity, entry.key)?;
        if previous.is_some_and(|previous: CheckedKey<'_>| previous.compare(key) != Ordering::Less)
        {
            return Err(TreeError::Corrupt);
        }
        previous = Some(key);
        if let EntryPayload::RightChild(child) = entry.payload {
            if same_address(child, header.identity.page)
                || leftmost.is_some_and(|leftmost| same_address(child, leftmost))
            {
                return Err(TreeError::Corrupt);
            }
            // Prior cells were fully validated. Re-read only their 24-byte
            // reference suffix, avoiding repeated variable-length key decoding.
            for prior in 1..slot {
                let bytes = cell_at(prior).ok_or(TreeError::Corrupt)?;
                let start = bytes
                    .len()
                    .checked_sub(PAGE_REF_BYTES)
                    .ok_or(TreeError::Corrupt)?;
                let other = PageRef::decode(&bytes[start..]).map_err(|_| TreeError::Corrupt)?;
                if same_address(child, other) {
                    return Err(TreeError::Corrupt);
                }
            }
        }
    }
    Ok(Layout {
        leftmost,
        entry_bytes,
    })
}

pub(crate) fn encode_metadata(
    kind: PageKind,
    arity: u8,
    leftmost: Option<PageRef>,
    output: &mut [u8],
) -> Result<&[u8], TreeError> {
    let leaf = validate_schema(kind, arity)?;
    if leaf == leftmost.is_some() {
        return Err(TreeError::InvalidInput);
    }
    if output.len() < TREE_METADATA_BYTES {
        return Err(TreeError::NoSpace);
    }
    let mut reference = [0; PAGE_REF_BYTES];
    if let Some(leftmost) = leftmost {
        leftmost.encode(&mut reference)?;
    }
    output[..TREE_METADATA_BYTES].fill(0);
    output[..4].copy_from_slice(b"XBT1");
    output[4..6].copy_from_slice(&1u16.to_le_bytes());
    output[6] = arity;
    output[8..TREE_METADATA_BYTES].copy_from_slice(&reference);
    Ok(&output[..TREE_METADATA_BYTES])
}

pub(crate) fn encode_leaf_cell<'a>(
    kind: PageKind,
    arity: u8,
    key: &[u8],
    value: &[u8],
    output: &'a mut [u8],
) -> Result<&'a [u8], TreeError> {
    if !validate_schema(kind, arity)? {
        return Err(TreeError::InvalidInput);
    }
    checked_key(kind, arity, key).map_err(|_| TreeError::InvalidInput)?;
    let length = key
        .len()
        .checked_add(value.len())
        .and_then(|length| length.checked_add(4))
        .filter(|length| *length <= MAX_TREE_ENTRY_COST - 4)
        .ok_or(TreeError::EntryTooLarge)?;
    if output.len() < length {
        return Err(TreeError::NoSpace);
    }
    output[..2].copy_from_slice(&(key.len() as u16).to_le_bytes());
    output[2..4].copy_from_slice(&(value.len() as u16).to_le_bytes());
    output[4..4 + key.len()].copy_from_slice(key);
    output[4 + key.len()..length].copy_from_slice(value);
    Ok(&output[..length])
}

pub(crate) fn encode_internal_cell<'a>(
    kind: PageKind,
    arity: u8,
    key: &[u8],
    right: PageRef,
    output: &'a mut [u8],
) -> Result<&'a [u8], TreeError> {
    if validate_schema(kind, arity)? {
        return Err(TreeError::InvalidInput);
    }
    checked_key(kind, arity, key).map_err(|_| TreeError::InvalidInput)?;
    let length = 4 + key.len() + PAGE_REF_BYTES;
    if output.len() < length {
        return Err(TreeError::NoSpace);
    }
    let mut reference = [0; PAGE_REF_BYTES];
    right.encode(&mut reference)?;
    output[..2].copy_from_slice(&(key.len() as u16).to_le_bytes());
    output[2..4].fill(0);
    output[4..4 + key.len()].copy_from_slice(key);
    output[4 + key.len()..length].copy_from_slice(&reference);
    Ok(&output[..length])
}

/// `cells[0]` is metadata. Validation completes before any output write.
pub(crate) fn encode_tree_page(
    header: PageHeader,
    expected_arity: u8,
    cells: &[&[u8]],
    output: &mut [u8],
) -> Result<(), TreeError> {
    validate_cells(
        header,
        expected_arity,
        cells.len(),
        |slot| cells.get(slot).copied(),
        TREE_ENTRY_CAPACITY,
    )?;
    encode_page(header, cells, output).map_err(TreeError::Page)
}

pub(crate) fn decode_tree_page(
    input: &[u8],
    expected: TreeExpectation,
) -> Result<TreePageView<'_>, TreeError> {
    validate_shape(expected.kind, expected.level, expected.arity)?;
    let page = decode_page(input, expected.identity, expected.kind)?;
    if page.header().level != expected.level {
        return Err(TreeError::LevelMismatch);
    }
    let layout = validate_cells(
        page.header(),
        expected.arity,
        page.len(),
        |slot| page.cell(slot),
        TREE_ENTRY_CAPACITY,
    )?;
    Ok(TreePageView {
        page,
        arity: expected.arity,
        leftmost: layout.leftmost,
    })
}

/// Select a byte-balanced boundary for one normal page plus one allowed entry.
/// Only reads caller cells; returned key borrows them.
pub(crate) fn choose_split<'a>(
    header: PageHeader,
    arity: u8,
    cells: &[&'a [u8]],
) -> Result<SplitPlan<'a>, TreeError> {
    let layout = validate_cells(
        header,
        arity,
        cells.len(),
        |slot| cells.get(slot).copied(),
        MAX_TREE_SPLIT_COST,
    )?;
    let leaf = is_leaf(header.kind)?;
    let entries = cells.len() - 1;
    if entries < if leaf { 2 } else { 3 } {
        return Err(TreeError::NotSplittable);
    }
    let mut left_bytes = 0;
    let mut best = None;
    for index in 0..entries {
        let cost = cells[index + 1].len() + 4;
        let right_bytes = layout.entry_bytes - left_bytes - if leaf { 0 } else { cost };
        if index >= 1
            && (leaf || index + 1 < entries)
            && left_bytes <= TREE_ENTRY_CAPACITY
            && right_bytes <= TREE_ENTRY_CAPACITY
        {
            let score = left_bytes.max(right_bytes);
            if best.is_none_or(|(_, _, _, best_score)| score < best_score) {
                best = Some((index, left_bytes, right_bytes, score));
            }
        }
        left_bytes += cost;
    }
    let (index, left_bytes, right_bytes, _) = best.ok_or(TreeError::NotSplittable)?;
    let pivot = parse_entry(header.kind, arity, cells[index + 1])?;
    Ok(SplitPlan {
        index,
        left_bytes,
        right_bytes,
        separator: pivot.key,
        right_leftmost: match pivot.payload {
            EntryPayload::Value(_) => None,
            EntryPayload::RightChild(child) => Some(child),
        },
    })
}

#[cfg(test)]
#[path = "tree_page_tests.rs"]
mod tests;
