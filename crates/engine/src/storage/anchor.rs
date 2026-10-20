//! Stable tree-root descriptors inside the common page envelope.
//! Validation does not acquire a frame pin/latch or perform an SMO.
//! Wire contract: `devdoc/contracts/dev3-reference-format.md`.
// Remove when the mutable tree connects this representation.
#![allow(dead_code)]

use super::page::{
    PageError, PageHeader, PageIdentity, PageKind, PageRef, PageView, decode_page, encode_page,
};

const ANCHOR_CELL_BYTES: usize = 48;

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

/// Encode one anchor cell in a page, validating all fields before output writes.
/// Exact page size is required; errors preserve output. No I/O or pin effects.
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

/// Validate the anchor envelope, its single cell, and all descriptor fields.
pub(crate) fn decode_anchor(input: &[u8], expected: PageIdentity) -> Result<TreeAnchor, PageError> {
    let page = decode_page(input, expected, PageKind::TreeAnchor)?;
    if page.len() != 1 {
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
    Ok(TreeAnchor {
        header: page.header(),
        root,
    })
}

#[cfg(test)]
#[path = "anchor_tests.rs"]
mod tests;
