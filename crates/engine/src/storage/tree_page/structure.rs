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

/// Space to leave for a subsequent row edit, not the edit itself. The owner
/// derives this from the validated cell and its rollback obligation. Splitting
/// never installs the pending row, changes its version, or consumes its Undo.
#[derive(Clone, Copy)]
pub(crate) struct LeafSpaceRequest<'a> {
    pub(crate) key: &'a [u8],
    pub(crate) entry_cost: usize,
}

impl<'bytes> TreePageView<'bytes> {
    /// Split an observed leaf while retaining its rollback space. `weights`
    /// is the owner's maximum restorable cell+slot cost for each current row,
    /// including its current cost. This layer validates numeric bounds but
    /// cannot establish the historical Undo/owner proof behind those weights.
    ///
    /// Choose among boundaries of the EXISTING rows, so both resulting leaves
    /// are nonempty even when the request is a new row. The pending row only
    /// changes the capacity calculation. Minimize the larger resulting weight
    /// (including the request), breaking ties at the earlier boundary. Errors
    /// preserve the reference scratch; successful candidates preserve all row
    /// bytes and logical versions and are still not installation authority.
    pub(crate) fn prepare_leaf_split<'cells>(
        &self,
        weights: &[usize],
        pending: Option<LeafSpaceRequest<'_>>,
        right: PageRef,
        scratch: &'cells mut [&'bytes [u8]],
    ) -> Result<SplitCandidate<'cells, 'bytes>, StructureError> {
        let header = self.header();
        if !is_leaf(header.kind)?
            || weights.len() != self.len()
            || same_address(header.identity.page, right)
        {
            return Err(TreeError::InvalidInput.into());
        }
        if self.len() < 2 {
            return Err(TreeError::NotSplittable.into());
        }
        if scratch.len() < self.page.len() {
            return Err(StructureError::ScratchTooSmall);
        }
        let mut total = 0usize;
        let mut actual = 0usize;
        for (index, &weight) in weights.iter().enumerate() {
            let cost = self.page.cell(index + 1).ok_or(TreeError::Corrupt)?.len() + 4;
            if !(cost..=MAX_TREE_ENTRY_COST).contains(&weight) {
                return Err(TreeError::InvalidInput.into());
            }
            total = total
                .checked_add(weight)
                .filter(|n| *n <= TREE_ENTRY_CAPACITY)
                .ok_or(TreeError::NoSpace)?;
            actual += cost;
        }
        let request = if let Some(pending) = pending {
            let position = self.lower_bound(pending.key)?;
            // A canonical key bounds its length. Even an empty value needs
            // the four-byte cell prefix and four-byte slot beside that key.
            if !(pending.key.len() + 8..=MAX_TREE_ENTRY_COST).contains(&pending.entry_cost) {
                return Err(TreeError::InvalidInput.into());
            }
            let increment = if position.equal {
                pending.entry_cost.saturating_sub(weights[position.index])
            } else {
                pending.entry_cost
            };
            Some((
                checked_key(header.kind, self.arity, pending.key)?,
                increment,
            ))
        } else {
            None
        };
        let mut left_weight = 0;
        let mut left_bytes = 0;
        let mut best: Option<(usize, usize, usize)> = None;
        for boundary in 1..self.len() {
            left_weight += weights[boundary - 1];
            left_bytes += self.page.cell(boundary).ok_or(TreeError::Corrupt)?.len() + 4;
            let mut left = left_weight;
            let mut right_weight = total - left_weight;
            if let Some((key, increment)) = request {
                let separator = checked_key(header.kind, self.arity, self.entry(boundary)?.key)?;
                if key.compare(separator) == Ordering::Less {
                    left += increment;
                } else {
                    right_weight += increment;
                }
            }
            if left <= TREE_ENTRY_CAPACITY && right_weight <= TREE_ENTRY_CAPACITY {
                let score = left.max(right_weight);
                if best.is_none_or(|(_, _, previous)| score < previous) {
                    best = Some((boundary, left_bytes, score));
                }
            }
        }
        let (index, left_bytes, _) = best.ok_or(TreeError::NotSplittable)?;
        let split = SplitPlan {
            index,
            left_bytes,
            right_bytes: actual - left_bytes,
            separator: self.entry(index)?.key,
            right_leftmost: None,
        };
        for (slot, target) in scratch[..self.page.len()].iter_mut().enumerate() {
            *target = self.page.cell(slot).expect("decoded immutable leaf slot");
        }
        Ok(SplitCandidate {
            header,
            arity: self.arity,
            cells: &scratch[..self.page.len()],
            right,
            split,
        })
    }

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
