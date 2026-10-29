//! Row point/cursor decisions over borrowed representations.
//!
//! These private C0 helpers carry no snapshot, pin, table, cursor-credit or
//! observation authority. `driver` obtains their inputs from the original
//! RowReadOperation, validates the view and finishes checked cleanup before
//! publishing output or advancing progress. See
//! `devdoc/contracts/dev3-module-consumers.md` (MCB00).
// Remove when a statement caller connects the driver's live entry points.
#![allow(dead_code)]

use crate::storage::{
    page::PageKind,
    row_record::RecordVersion,
    transaction_record::{TransactionState, TxnRef, VersionStamp},
    tree_page::{TreeError, TreePageView},
};
use foundation::identity::RowId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadError {
    InvalidRange,
    UnsupportedPage,
    Tree(TreeError),
    InvalidVersion,
    UnexpectedStatus,
    MissingUndo,
    InvalidProgress,
    CursorInvalidated,
    Cancelled,
    OutputTooSmall { required: usize },
}

impl From<TreeError> for ReadError {
    fn from(error: TreeError) -> Self {
        Self::Tree(error)
    }
}

/// Pure half-open RowId bounds, not a range lock or a registered read set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowRange {
    start: Option<RowId>,
    end: Option<RowId>,
}

impl RowRange {
    fn new(start: Option<RowId>, end: Option<RowId>) -> Result<Self, ReadError> {
        if start.zip(end).is_some_and(|(start, end)| start > end) {
            return Err(ReadError::InvalidRange);
        }
        Ok(Self { start, end })
    }

    fn contains(self, row: RowId) -> bool {
        self.start.is_none_or(|start| row >= start) && self.end.is_none_or(|end| row < end)
    }

    fn is_empty(self) -> bool {
        self.start.is_some() && self.start == self.end
    }
}

/// A selected physical index is only a search result. Source capture must
/// recheck the actual key/row identity while holding the original page guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SeekPosition {
    index: usize,
    row: RowId,
}

fn require_row_leaf(leaf: &TreePageView<'_>) -> Result<(), ReadError> {
    if leaf.header().kind != PageKind::RowLeaf || leaf.header().level != 0 {
        return Err(ReadError::UnsupportedPage);
    }
    Ok(())
}

fn at(leaf: &TreePageView<'_>, index: usize) -> Result<SeekPosition, ReadError> {
    let bytes: [u8; 8] = leaf
        .entry(index)?
        .key
        .try_into()
        .map_err(|_| ReadError::Tree(TreeError::Corrupt))?;
    let row =
        RowId::new(u64::from_be_bytes(bytes)).map_err(|_| ReadError::Tree(TreeError::Corrupt))?;
    Ok(SeekPosition { index, row })
}

fn seek_point(leaf: &TreePageView<'_>, row: RowId) -> Result<Option<SeekPosition>, ReadError> {
    require_row_leaf(leaf)?;
    let position = leaf.lower_bound(&row.get().to_be_bytes())?;
    if position.equal {
        at(leaf, position.index).map(Some)
    } else {
        Ok(None)
    }
}

/// Reseek from the full last examined key on every call. No retained page slot
/// and no RowId successor arithmetic are needed, including at u64::MAX.
fn seek_next(
    leaf: &TreePageView<'_>,
    range: RowRange,
    last_examined: Option<RowId>,
) -> Result<Option<SeekPosition>, ReadError> {
    require_row_leaf(leaf)?;
    if last_examined.is_some_and(|row| !range.contains(row)) {
        return Err(ReadError::InvalidProgress);
    }
    if range.is_empty() {
        return Ok(None);
    }
    let key = last_examined.or(range.start).map_or(1, RowId::get);
    let position = leaf.lower_bound(&key.to_be_bytes())?;
    let index = position.index + usize::from(last_examined.is_some() && position.equal);
    if index == leaf.len() {
        return Ok(None);
    }
    let selected = at(leaf, index)?;
    Ok(range.contains(selected.row).then_some(selected))
}

/// A pure decision about an already validated version; none of these variants
/// is a source capability or a user-visible Found/NotFound result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visibility {
    Current,
    ReadStatus,
    ReadMissingBefore,
}

/// `snapshot_q` and `own_actor` are observations from the eventual original
/// statement view. Supplying numbers here cannot acquire/pin a snapshot or
/// bypass source validation. A transaction stamp requires the exact status
/// resolved by C; an invisible INSERT still requires C's Missing Undo check.
fn visible(
    snapshot_q: u64,
    own_actor: Option<TxnRef>,
    version: RecordVersion,
    status: Option<TransactionState>,
) -> Result<Visibility, ReadError> {
    let current = match version.stamp {
        VersionStamp::Committed(q) => {
            if status.is_some() {
                return Err(ReadError::UnexpectedStatus);
            }
            q.get() <= snapshot_q
        }
        VersionStamp::Transaction(reference) => {
            if reference.id() != version.logical.transaction() {
                return Err(ReadError::InvalidVersion);
            }
            // Every retained first-INSERT transaction stamp has its Missing U.
            if version.undo.is_none() {
                return Err(ReadError::MissingUndo);
            }
            match status {
                None => return Ok(Visibility::ReadStatus),
                Some(TransactionState::Active) => own_actor == Some(reference),
                Some(TransactionState::Committed(q)) => q.get() <= snapshot_q,
                Some(TransactionState::Aborting(_) | TransactionState::Aborted) => false,
            }
        }
    };
    if current {
        Ok(Visibility::Current)
    } else if version.undo.is_some() {
        Ok(Visibility::ReadMissingBefore)
    } else {
        // A missing retained version is corruption, never ordinary absence.
        Err(ReadError::MissingUndo)
    }
}

fn check_output(required: usize, available: usize) -> Result<(), ReadError> {
    if available < required {
        Err(ReadError::OutputTooSmall { required })
    } else {
        Ok(())
    }
}

/// Only the pure progress fields to be embedded in the real, view-borrowing
/// cursor. This is private, not Clone, and cannot create a live cursor or credit.
struct CursorProgress {
    range: RowRange,
    revision: u64,
    last_examined: Option<RowId>,
    last_returned: Option<RowId>,
    exhausted: bool,
}

impl CursorProgress {
    fn new(range: RowRange, revision: u64) -> Self {
        Self {
            range,
            revision,
            last_examined: None,
            last_returned: None,
            exhausted: false,
        }
    }

    fn check_revision(&self, revision: u64) -> Result<(), ReadError> {
        if revision != self.revision {
            return Err(ReadError::CursorInvalidated);
        }
        Ok(())
    }

    /// The live adapter checks origin/fence/statement first. Revision precedes
    /// cancellation and even an already exhausted result; neither error moves
    /// the keys. Cancellation is a sampled value, not a cancellation authority.
    fn seek(
        &self,
        leaf: &TreePageView<'_>,
        revision: u64,
        cancelled: bool,
    ) -> Result<Option<SeekPosition>, ReadError> {
        self.check_revision(revision)?;
        if self.exhausted {
            return Ok(None);
        }
        if cancelled {
            return Err(ReadError::Cancelled);
        }
        seek_next(leaf, self.range, self.last_examined)
    }

    fn check_advance(&self, row: RowId, revision: u64) -> Result<(), ReadError> {
        self.check_revision(revision)?;
        if self.exhausted
            || !self.range.contains(row)
            || self.last_examined.is_some_and(|last| row <= last)
        {
            return Err(ReadError::InvalidProgress);
        }
        Ok(())
    }

    /// Call only after Missing-before validation and checked source cleanup.
    fn finish_invisible(&mut self, row: RowId, revision: u64) -> Result<(), ReadError> {
        self.check_advance(row, revision)?;
        self.last_examined = Some(row);
        Ok(())
    }

    /// Call only after complete output/observation and checked source cleanup.
    /// OutputTooSmall, Cancelled, FlushRequired and errors never call this.
    fn finish_returned(&mut self, row: RowId, revision: u64) -> Result<(), ReadError> {
        self.check_advance(row, revision)?;
        self.last_examined = Some(row);
        self.last_returned = Some(row);
        Ok(())
    }

    /// Call after a seek returns None and its actual page/operation is released.
    fn finish_end(&mut self, revision: u64) -> Result<(), ReadError> {
        self.check_revision(revision)?;
        self.exhausted = true;
        Ok(())
    }
}

mod driver;

#[cfg(test)]
mod tests;
