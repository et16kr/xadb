//! Bounded leaf candidates for row/catalog producers and logical undo.
//! Candidates borrow caller storage and confer no pin, WAL or apply authority.

use super::*;
use crate::storage::page::PageLsn;

/// Metadata plus a maximum normal leaf and one inserted entry. Each leaf entry
/// costs at least 16 bytes; index entries cost more, even for a NULL key.
pub(crate) const MAX_LEAF_EDIT_CELLS: usize = TREE_ENTRY_CAPACITY / 16 + 2;

#[derive(Clone, Copy, Debug)]
pub(crate) enum LeafEdit<'a> {
    Insert(&'a [u8]),
    Replace(&'a [u8]),
    /// Physical removal, for a permitted reclamation or an undo of Missing.
    /// A user's DELETE instead installs the owning row's versioned tombstone.
    Remove(&'a [u8]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafEditError {
    Tree(TreeError),
    KeyExists,
    KeyMissing,
    ScratchTooSmall,
}

impl From<TreeError> for LeafEditError {
    fn from(error: TreeError) -> Self {
        Self::Tree(error)
    }
}

/// Immutable candidate sequence. The source page, supplied cell and caller
/// reference scratch remain borrowed until the last use of the plan.
#[derive(Debug)]
pub(crate) struct LeafEditPlan<'cells, 'bytes> {
    header: PageHeader,
    arity: u8,
    cells: &'cells [&'bytes [u8]],
    entry_bytes: usize,
}

impl<'bytes> TreePageView<'bytes> {
    /// Validate an edit completely before writing the required scratch prefix.
    /// All errors preserve scratch as well as the immutable source and cell.
    /// The returned candidate may need a split; no authoritative page changes.
    pub(crate) fn prepare_leaf_edit<'cells>(
        &self,
        edit: LeafEdit<'bytes>,
        scratch: &'cells mut [&'bytes [u8]],
    ) -> Result<LeafEditPlan<'cells, 'bytes>, LeafEditError> {
        let header = self.header();
        if !is_leaf(header.kind)? {
            return Err(TreeError::InvalidInput.into());
        }
        let key = match edit {
            LeafEdit::Insert(cell) | LeafEdit::Replace(cell) => {
                parse_entry(header.kind, self.arity, cell)?.key
            }
            LeafEdit::Remove(key) => key,
        };
        let position = self.lower_bound(key)?;
        match edit {
            LeafEdit::Insert(_) if position.equal => return Err(LeafEditError::KeyExists),
            LeafEdit::Replace(_) | LeafEdit::Remove(_) if !position.equal => {
                return Err(LeafEditError::KeyMissing);
            }
            _ => {}
        }
        // The decoded source bounds these counts by 1012 and the edited count
        // by 1013. position is an entry index; slot zero is always metadata.
        let count = match edit {
            LeafEdit::Insert(_) => self.page.len() + 1,
            LeafEdit::Replace(_) => self.page.len(),
            LeafEdit::Remove(_) => self.page.len() - 1,
        };
        if scratch.len() < count {
            return Err(LeafEditError::ScratchTooSmall);
        }
        let at = position.index + 1;
        let cell_at = |slot| match edit {
            LeafEdit::Insert(cell) if slot == at => Some(cell),
            LeafEdit::Insert(_) if slot > at => self.page.cell(slot - 1),
            LeafEdit::Replace(cell) if slot == at => Some(cell),
            LeafEdit::Remove(_) if slot >= at => self.page.cell(slot + 1),
            _ => self.page.cell(slot),
        };
        let layout = validate_cells(header, self.arity, count, cell_at, MAX_TREE_SPLIT_COST)?;
        // The immutable mapping was checked for every slot above. No fallible
        // operation remains after the first scratch write.
        for (slot, target) in scratch[..count].iter_mut().enumerate() {
            *target = cell_at(slot).expect("validated immutable leaf candidate");
        }
        Ok(LeafEditPlan {
            header,
            arity: self.arity,
            cells: &scratch[..count],
            entry_bytes: layout.entry_bytes,
        })
    }
}

impl<'bytes> LeafEditPlan<'_, 'bytes> {
    pub(crate) fn entry_bytes(&self) -> usize {
        self.entry_bytes
    }

    pub(crate) fn needs_split(&self) -> bool {
        self.entry_bytes > TREE_ENTRY_CAPACITY
    }

    /// Encode one full candidate, preserving output on every error. The owner
    /// supplies the complete group's LSN and separately revalidates/stages it;
    /// this representation does not certify WAL coverage or LSN ordering.
    pub(crate) fn encode(&self, lsn: PageLsn, output: &mut [u8]) -> Result<(), TreeError> {
        encode_tree_page(
            PageHeader { lsn, ..self.header },
            self.arity,
            self.cells,
            output,
        )
    }

    /// Reuse the frozen byte-balanced split; neither page is installed here.
    pub(crate) fn split(&self) -> Result<SplitPlan<'bytes>, TreeError> {
        choose_split(self.header, self.arity, self.cells)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod extended_tests {
    #[test]
    #[ignore = "DEV3 S5: extended leaf edit histories"]
    fn leaf_edit_extended_histories_match_ordered_map() {
        super::tests::history(256, 1024);
    }
}
