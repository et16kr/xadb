//! Scope3 opcodes and fixed bounded section sizes. These values grant no credit.
//!
//! DeleteForward/DeleteClr/DeletePrune (8, 9, 23) are the Row DELETE first
//! path (contract dev3-row-delete.md#delete-shape). Their `cell` is the old
//! row's cell C; the tombstone's 180-byte cell is the fixed term below.
use super::{Error, MAX_CELL, MIN_CELL, Result};
use crate::storage::wal_frame::FrameKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum Operation {
    Begin = 1,
    Forward = 2,
    RollbackStart = 3,
    Clr = 4,
    StatementFinish = 5,
    Commit = 6,
    AbortEnd = 7,
    DeleteForward = 8,
    DeleteClr = 9,
    Reserve = 11,
    Cancel = 12,
    AgingStart = 16,
    TerminalFinish = 17,
    Prune = 20,
    ReleaseRole = 21,
    RootFree = 22,
    DeletePrune = 23,
}
impl Operation {
    pub(crate) fn decode(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::Begin),
            2 => Ok(Self::Forward),
            3 => Ok(Self::RollbackStart),
            4 => Ok(Self::Clr),
            5 => Ok(Self::StatementFinish),
            6 => Ok(Self::Commit),
            7 => Ok(Self::AbortEnd),
            8 => Ok(Self::DeleteForward),
            9 => Ok(Self::DeleteClr),
            11 => Ok(Self::Reserve),
            12 => Ok(Self::Cancel),
            16 => Ok(Self::AgingStart),
            17 => Ok(Self::TerminalFinish),
            20 => Ok(Self::Prune),
            21 => Ok(Self::ReleaseRole),
            22 => Ok(Self::RootFree),
            23 => Ok(Self::DeletePrune),
            _ => Err(Error::Unsupported),
        }
    }
    /// The six Row-cell operations: INSERT's Forward/Clr/Prune and DELETE's.
    pub(crate) fn row(self) -> bool {
        matches!(
            self,
            Self::Forward
                | Self::Clr
                | Self::Prune
                | Self::DeleteForward
                | Self::DeleteClr
                | Self::DeletePrune
        )
    }
    /// The INSERT operation with the same role (Forward/Clr/Prune) in the
    /// chain, statement and descriptor rules; every other operation is itself.
    /// Leaf/Undo effects and sizes still differ and are never taken from this.
    pub(crate) fn role(self) -> Self {
        match self {
            Self::DeleteForward => Self::Forward,
            Self::DeleteClr => Self::Clr,
            Self::DeletePrune => Self::Prune,
            other => other,
        }
    }
    pub(crate) fn handoff(self) -> bool {
        matches!(
            self,
            Self::Begin
                | Self::Forward
                | Self::Clr
                | Self::DeleteForward
                | Self::DeleteClr
                | Self::Commit
                | Self::AbortEnd
                | Self::Reserve
                | Self::Cancel
        )
    }
    pub(crate) fn status_patch(self) -> bool {
        matches!(self, Self::RollbackStart | Self::StatementFinish)
    }
    pub(crate) fn outer(self) -> FrameKind {
        match self {
            Self::Begin => FrameKind::Begin,
            Self::Forward
            | Self::DeleteForward
            | Self::RollbackStart
            | Self::StatementFinish
            | Self::Reserve
            | Self::Cancel => FrameKind::Update,
            Self::Clr | Self::DeleteClr => FrameKind::Clr,
            Self::Commit => FrameKind::Commit,
            Self::AbortEnd => FrameKind::AbortEnd,
            _ => FrameKind::System,
        }
    }
    pub(crate) fn shape(self, cell: usize, predecessor: bool) -> Result<Shape> {
        let row = self.row();
        if (row && !(MIN_CELL..=MAX_CELL).contains(&cell)) || (!row && (cell != 0 || predecessor)) {
            return Err(Error::InvalidInput);
        }
        let h = u16::from(predecessor);
        let (native_count, native_bytes, edges, counts, items, redo_count, redo_bytes) = match self
        {
            Self::Begin | Self::Commit | Self::AbortEnd | Self::TerminalFinish => {
                (3, 3904, 0, 0, 0, 3, 49032)
            }
            Self::Reserve => (3, 4320, 0, 0, 1, 3, 49032),
            Self::Forward => (5, 5024 + cell, 3 + h, 2, 1, 5, 81720),
            Self::Cancel => (2, 3328, 0, 0, 1, 2, 32688),
            Self::Clr => (5, 5024 + cell, 3 + h, 2, 0, 5, 81720),
            Self::RollbackStart | Self::StatementFinish => (1, 576, 0, 0, 0, 1, 288),
            Self::AgingStart => (1, 1152, 0, 0, 0, 1, 16344),
            Self::Prune => (5, 5024 + 2 * cell, 3 + h, 2, 0, 5, 81720),
            // Old cell C and tombstone 180 on the leaf; U 432 and 432+(C-180).
            Self::DeleteForward => (5, 5024 + 2 * cell, 3 + h, 2, 1, 5, 81720),
            Self::DeleteClr => (5, 5024 + 2 * cell, 3 + h, 2, 0, 5, 81720),
            // Tombstone removed; U 432+(C-180) to its 432 Releasing record.
            Self::DeletePrune => (5, 5024 + cell, 3 + h, 2, 0, 5, 81720),
            Self::ReleaseRole | Self::RootFree => (3, 4320, 0, 0, 0, 3, 49032),
        };
        Ok(Shape {
            native_count,
            native_bytes,
            edges,
            counts,
            items,
            redo_count,
            redo_bytes,
            cell,
            predecessor,
        })
    }
    pub(crate) fn frame_bytes(self, cell: usize, predecessor: bool) -> Result<usize> {
        Ok(if self.handoff() { 3776 } else { 2176 } + self.shape(cell, predecessor)?.body_bytes())
    }
    /// Each first-profile changed page has exactly one native cell.
    pub(super) fn native_mask(self) -> u16 {
        let root_job = (1 << 8) | (1 << 9);
        match self {
            Self::Begin | Self::Commit | Self::AbortEnd | Self::TerminalFinish => {
                root_job | (1 << 5)
            }
            Self::Reserve | Self::ReleaseRole | Self::RootFree => root_job | (1 << 3),
            Self::Forward
            | Self::Clr
            | Self::Prune
            | Self::DeleteForward
            | Self::DeleteClr
            | Self::DeletePrune => root_job | (1 << 1) | (1 << 3) | (1 << 5),
            Self::Cancel => root_job,
            Self::RollbackStart | Self::StatementFinish => 1 << 5,
            Self::AgingStart => 1 << 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) native_count: u16,
    pub(crate) native_bytes: usize,
    pub(crate) edges: u16,
    pub(crate) counts: u16,
    pub(crate) items: u16,
    pub(crate) redo_count: u16,
    pub(crate) redo_bytes: usize,
    pub(crate) cell: usize,
    pub(crate) predecessor: bool,
}
/// Size of a tombstone tree cell (XRW1 header 168 plus the 12-byte cell head).
pub(crate) const TOMBSTONE_CELL: usize = 180;
/// The one-slot Undo record without a payload.
const UNDO_RECORD: usize = 432;

/// Native presence (1 before, 2 after, 3 both) and the before/after body
/// sizes of one native kind of a Row-cell operation (dev3-row-delete.md
/// #delete-shape). Kind 1 is the RowLeaf cell, kind 3 the U record; every
/// other kind keeps both sides at its fixed size.
pub(super) fn native_sides(
    operation: Operation,
    kind: u8,
    cell: usize,
) -> Option<(u8, usize, usize)> {
    // The Live delete U: 432 plus the old row's canonical L = C - 180. Only
    // DELETE operations use it; every other operation may carry C = 0.
    let live = || Some(UNDO_RECORD + cell.checked_sub(TOMBSTONE_CELL)?);
    Some(match (kind, operation) {
        (1, Operation::Forward) => (2, 0, cell),
        (1, Operation::Clr) => (1, cell, 0),
        (1, Operation::Prune) => (3, cell, cell),
        (1, Operation::DeleteForward) => (3, cell, TOMBSTONE_CELL),
        (1, Operation::DeleteClr) => (3, TOMBSTONE_CELL, cell),
        (1, Operation::DeletePrune) => (1, TOMBSTONE_CELL, 0),
        (1, _) => return None,
        (3, Operation::DeleteForward) => (3, UNDO_RECORD, live()?),
        (3, Operation::DeleteClr | Operation::DeletePrune) => (3, live()?, UNDO_RECORD),
        (3, _) => (3, UNDO_RECORD, UNDO_RECORD),
        (5, _) => (3, 224, 224),
        (8, _) => (3, 512, 512),
        (9, _) => (3, 1024, 1024),
        _ => return None,
    })
}

/// C from a Row-cell operation's native total (the inverse of `shape`).
/// Non-row operations carry no cell.
pub(super) fn cell_from_native(operation: Operation, native_bytes: usize) -> Result<usize> {
    let extra = || native_bytes.checked_sub(5024).ok_or(Error::Corrupt);
    Ok(match operation {
        Operation::Forward | Operation::Clr | Operation::DeletePrune => extra()?,
        Operation::Prune | Operation::DeleteForward | Operation::DeleteClr => {
            let twice = extra()?;
            if twice % 2 != 0 {
                return Err(Error::Corrupt);
            }
            twice / 2
        }
        _ => 0,
    })
}

impl Shape {
    pub(super) fn auxiliary_bytes(self) -> usize {
        usize::from(self.edges) * 128
            + usize::from(self.counts) * 96
            + usize::from(self.items) * 208
    }
    fn body_bytes(self) -> usize {
        self.native_bytes + self.auxiliary_bytes() + self.redo_bytes
    }
}
