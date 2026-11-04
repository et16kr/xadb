//! Untrusted structural candidates for the shared tree format.
//!
//! These borrow the observed page and caller scratch. They neither reserve
//! pages nor authorize WAL, installation, retirement or a root publication.
//! A live owner must bind and revalidate the complete structural group.

use super::*;
use crate::storage::page::PageLsn;

#[derive(Clone, Copy, Debug)]
pub(crate) enum InternalEdit<'a> {
    /// Add a separator after the child that was split. The separator cell
    /// names the new right sibling; `left` is the exact observed old child.
    Insert { left: PageRef, cell: &'a [u8] },
    /// Unlink a child whose emptiness and retirement are proved by the owner.
    /// This representation does not establish either of those properties.
    RemoveChild(PageRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StructureError {
    Tree(TreeError),
    ChildChanged,
    SeparatorExists,
    ScratchTooSmall,
}

impl From<TreeError> for StructureError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

/// Removing a child from a two-child internal node leaves a single child.
/// Such a node is not encodable in XBT1. A root may collapse by lowering the
/// anchor level. Below the root, directly splicing the survivor would skip a
/// level: the owner needs a separately validated, level-preserving ancestor
/// repair in the same group that retires this node.
pub(crate) enum InternalEditPlan<'cells, 'bytes> {
    Page(InternalCandidate<'cells, 'bytes>),
    Collapse { survivor: PageRef },
}

pub(crate) struct InternalCandidate<'cells, 'bytes> {
    header: PageHeader,
    arity: u8,
    cells: &'cells [&'bytes [u8]],
    entry_bytes: usize,
}

impl<'bytes> TreePageView<'bytes> {
    /// Validate the entire mapping before writing either scratch prefix.
    /// On success, metadata and cell references remain borrowed by the plan.
    /// All errors, and `Collapse`, preserve both scratch buffers.
    pub(crate) fn prepare_internal_edit<'cells>(
        &self,
        edit: InternalEdit<'bytes>,
        metadata: &'bytes mut [u8; TREE_METADATA_BYTES],
        scratch: &'cells mut [&'bytes [u8]],
    ) -> Result<InternalEditPlan<'cells, 'bytes>, StructureError> {
        let header = self.header();
        if is_leaf(header.kind)? {
            return Err(TreeError::InvalidInput.into());
        }
        let (at, count, leftmost) = match edit {
            InternalEdit::Insert { left, cell } => {
                let key = parse_entry(header.kind, self.arity, cell)?.key;
                let position = self.lower_bound(key)?;
                if position.equal {
                    return Err(StructureError::SeparatorExists);
                }
                if self.child(position.index)? != left {
                    return Err(StructureError::ChildChanged);
                }
                (position.index + 1, self.page.len() + 1, self.child(0)?)
            }
            InternalEdit::RemoveChild(child) => {
                let mut found = None;
                for index in 0..=self.len() {
                    if self.child(index)? == child {
                        found = Some(index);
                        break;
                    }
                }
                let index = found.ok_or(StructureError::ChildChanged)?;
                if self.len() == 1 {
                    return Ok(InternalEditPlan::Collapse {
                        survivor: self.child(usize::from(index == 0))?,
                    });
                }
                (
                    index.max(1),
                    self.page.len() - 1,
                    self.child(usize::from(index == 0))?,
                )
            }
        };
        if scratch.len() < count {
            return Err(StructureError::ScratchTooSmall);
        }
        let mut next_metadata = [0; TREE_METADATA_BYTES];
        encode_metadata(header.kind, self.arity, Some(leftmost), &mut next_metadata)?;
        let entry_at = |slot| match edit {
            InternalEdit::Insert { cell, .. } if slot == at => Some(cell),
            InternalEdit::Insert { .. } if slot > at => self.page.cell(slot - 1),
            InternalEdit::RemoveChild(_) if slot >= at => self.page.cell(slot + 1),
            _ => self.page.cell(slot),
        };
        let layout = validate_cells(
            header,
            self.arity,
            count,
            |slot| {
                if slot == 0 {
                    Some(next_metadata.as_slice())
                } else {
                    entry_at(slot)
                }
            },
            MAX_TREE_SPLIT_COST,
        )?;
        // All fallible work is above. Source and edit bytes are immutable.
        metadata.copy_from_slice(&next_metadata);
        scratch[0] = metadata;
        for (slot, target) in scratch.iter_mut().enumerate().take(count).skip(1) {
            *target = entry_at(slot).expect("validated immutable internal mapping");
        }
        Ok(InternalEditPlan::Page(InternalCandidate {
            header,
            arity: self.arity,
            cells: &scratch[..count],
            entry_bytes: layout.entry_bytes,
        }))
    }
}

impl<'bytes> InternalCandidate<'_, 'bytes> {
    pub(crate) fn needs_split(&self) -> bool {
        self.entry_bytes > TREE_ENTRY_CAPACITY
    }

    pub(crate) fn encode(&self, lsn: PageLsn, output: &mut [u8]) -> Result<(), TreeError> {
        encode_tree_page(
            PageHeader { lsn, ..self.header },
            self.arity,
            self.cells,
            output,
        )
    }

    pub(crate) fn split(&self, right: PageRef) -> Result<SplitCandidate<'_, 'bytes>, TreeError> {
        SplitCandidate::prepare(self.header, self.arity, self.cells, right)
    }
}

/// A byte-balanced pair of immutable candidates. The separator is retained
/// in the right leaf, or promoted out of both internal children. No page-sized
/// image or entry-count-sized reference array is hidden in this carrier.
pub(crate) struct SplitCandidate<'cells, 'bytes> {
    header: PageHeader,
    arity: u8,
    cells: &'cells [&'bytes [u8]],
    right: PageRef,
    split: SplitPlan<'bytes>,
}

impl<'cells, 'bytes> SplitCandidate<'cells, 'bytes> {
    pub(super) fn prepare(
        header: PageHeader,
        arity: u8,
        cells: &'cells [&'bytes [u8]],
        right: PageRef,
    ) -> Result<Self, TreeError> {
        let split = choose_split(header, arity, cells)?;
        if same_address(header.identity.page, right) {
            return Err(TreeError::InvalidInput);
        }
        if !is_leaf(header.kind)? {
            // The new sibling cannot alias ANY existing child, including the
            // promoted pivot's right child, even with a new incarnation.
            let leftmost = PageRef::decode(&cells[0][8..])?;
            if same_address(leftmost, right) {
                return Err(TreeError::InvalidInput);
            }
            for cell in &cells[1..] {
                if let EntryPayload::RightChild(child) =
                    parse_entry(header.kind, arity, cell)?.payload
                    && same_address(child, right)
                {
                    return Err(TreeError::InvalidInput);
                }
            }
        }
        Ok(Self {
            header,
            arity,
            cells,
            right,
            split,
        })
    }

    pub(crate) fn separator(&self) -> &'bytes [u8] {
        self.split.separator
    }

    pub(crate) fn left(&self) -> PageRef {
        self.header.identity.page
    }

    pub(crate) fn right(&self) -> PageRef {
        self.right
    }

    /// Encode the one-separator parent for a root split. The owner must
    /// separately update the existing stable anchor's revision/root in the
    /// same complete group; this function does not publish an anchor.
    pub(crate) fn encode_new_root(
        &self,
        root: PageRef,
        lsn: PageLsn,
        metadata: &mut [u8; TREE_METADATA_BYTES],
        cell: &mut [u8],
        output: &mut [u8],
    ) -> Result<(), TreeError> {
        if self.header.level >= 15
            || same_address(root, self.left())
            || same_address(root, self.right())
        {
            return Err(TreeError::InvalidInput);
        }
        if !is_leaf(self.header.kind)? {
            if same_address(root, PageRef::decode(&self.cells[0][8..])?) {
                return Err(TreeError::InvalidInput);
            }
            for cell in &self.cells[1..] {
                if let EntryPayload::RightChild(child) =
                    parse_entry(self.header.kind, self.arity, cell)?.payload
                    && same_address(root, child)
                {
                    return Err(TreeError::InvalidInput);
                }
            }
        }
        let kind = match self.header.kind {
            PageKind::RowLeaf | PageKind::RowInternal => PageKind::RowInternal,
            PageKind::IndexLeaf | PageKind::IndexInternal => PageKind::IndexInternal,
            PageKind::CatalogLeaf | PageKind::CatalogInternal => PageKind::CatalogInternal,
            _ => return Err(TreeError::InvalidInput),
        };
        encode_metadata(kind, self.arity, Some(self.left()), metadata)?;
        let cell = encode_internal_cell(kind, self.arity, self.separator(), self.right(), cell)?;
        encode_tree_page(
            PageHeader {
                identity: PageIdentity {
                    page: root,
                    ..self.header.identity
                },
                kind,
                level: self.header.level + 1,
                lsn,
            },
            self.arity,
            &[metadata, cell],
            output,
        )
    }

    /// Encode either candidate into caller storage. Scratch is workspace and
    /// may change on failure; `output` is preserved on every error. The source
    /// cells and all other output images remain immutable. Publication of a
    /// single side is forbidden: installation belongs to the system owner.
    pub(crate) fn encode_side<'scratch>(
        &'scratch self,
        right: bool,
        lsn: PageLsn,
        metadata: &'scratch mut [u8; TREE_METADATA_BYTES],
        scratch: &mut [&'scratch [u8]],
        output: &mut [u8],
    ) -> Result<(), TreeError> {
        let leaf = is_leaf(self.header.kind)?;
        let entries = if right {
            &self.cells[self.split.index + 1 + usize::from(!leaf)..]
        } else {
            &self.cells[1..self.split.index + 1]
        };
        if scratch.len() < entries.len() + 1 {
            return Err(TreeError::NoSpace);
        }
        let leftmost = if leaf {
            None
        } else if right {
            self.split.right_leftmost
        } else {
            Some(PageRef::decode(&self.cells[0][8..])?)
        };
        encode_metadata(self.header.kind, self.arity, leftmost, metadata)?;
        scratch[0] = metadata;
        scratch[1..entries.len() + 1].copy_from_slice(entries);
        let header = PageHeader {
            identity: PageIdentity {
                page: if right { self.right } else { self.left() },
                ..self.header.identity
            },
            lsn,
            ..self.header
        };
        encode_tree_page(header, self.arity, &scratch[..entries.len() + 1], output)
    }
}

#[cfg(test)]
mod tests;
