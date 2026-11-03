//! Scope3 opcodes and fixed bounded section sizes. These values grant no credit.
//!
//! DeleteForward/DeleteClr/DeletePrune (8, 9, 23) are the Row DELETE first
//! path (contract dev3-row-delete.md#delete-shape). Their `cell` is the old
//! row's cell C; the tombstone's 180-byte cell is the fixed term below.
//!
//! UpdateForward/UpdateClr/UpdatePrune (10, 13, 24) are the Row UPDATE first
//! path (contract dev3-row-update.md#update-shape). They carry two cells: the
//! old row's C (`Shape::cell`) and the new row's C' (`Shape::new_cell`).
use super::{Error, MAX_CELL, MIN_CELL, Result};
use crate::storage::{
    row_record::ROW_HEADER_BYTES, undo_record::UNDO_RECORD_BYTES, wal_frame::FrameKind,
};

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
    UpdateForward = 10,
    Reserve = 11,
    Cancel = 12,
    UpdateClr = 13,
    AgingStart = 16,
    TerminalFinish = 17,
    Prune = 20,
    ReleaseRole = 21,
    RootFree = 22,
    DeletePrune = 23,
    UpdatePrune = 24,
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
            10 => Ok(Self::UpdateForward),
            11 => Ok(Self::Reserve),
            12 => Ok(Self::Cancel),
            13 => Ok(Self::UpdateClr),
            16 => Ok(Self::AgingStart),
            17 => Ok(Self::TerminalFinish),
            20 => Ok(Self::Prune),
            21 => Ok(Self::ReleaseRole),
            22 => Ok(Self::RootFree),
            23 => Ok(Self::DeletePrune),
            24 => Ok(Self::UpdatePrune),
            _ => Err(Error::Unsupported),
        }
    }
    /// The nine Row-cell operations: INSERT's Forward/Clr/Prune, DELETE's
    /// and UPDATE's.
    pub(crate) fn row(self) -> bool {
        matches!(
            self,
            Self::Forward
                | Self::Clr
                | Self::Prune
                | Self::DeleteForward
                | Self::DeleteClr
                | Self::DeletePrune
        ) || self.update()
    }
    /// The three UPDATE operations, the only ones with a new cell C'.
    pub(crate) fn update(self) -> bool {
        matches!(
            self,
            Self::UpdateForward | Self::UpdateClr | Self::UpdatePrune
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
            Self::UpdateForward => Self::Forward,
            Self::UpdateClr => Self::Clr,
            Self::UpdatePrune => Self::Prune,
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
                | Self::UpdateForward
                | Self::UpdateClr
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
            | Self::UpdateForward
            | Self::RollbackStart
            | Self::StatementFinish
            | Self::Reserve
            | Self::Cancel => FrameKind::Update,
            Self::Clr | Self::DeleteClr | Self::UpdateClr => FrameKind::Clr,
            Self::Commit => FrameKind::Commit,
            Self::AbortEnd => FrameKind::AbortEnd,
            _ => FrameKind::System,
        }
    }
    /// The shape of every operation but UPDATE's (whose C' this cannot name).
    /// Product code names C' through `shape_cells`; tests keep this form.
    #[cfg(test)]
    pub(crate) fn shape(self, cell: usize, predecessor: bool) -> Result<Shape> {
        self.shape_cells(cell, 0, predecessor)
    }
    /// `shape` with UPDATE's new cell C' (dev3-row-update.md#update-shape):
    /// C' is in 185..=2228 for the three UPDATE operations and 0 otherwise.
    pub(crate) fn shape_cells(
        self,
        cell: usize,
        new_cell: usize,
        predecessor: bool,
    ) -> Result<Shape> {
        let row = self.row();
        let cells = |c: usize| (MIN_CELL..=MAX_CELL).contains(&c);
        if (row && !cells(cell))
            || (!row && (cell != 0 || predecessor))
            || if self.update() {
                !cells(new_cell)
            } else {
                new_cell != 0
            }
        {
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
            // Leaf C and C' (C' twice for Prune); U 432 and 432+(C-180).
            Self::UpdateForward => (5, 4844 + 2 * cell + new_cell, 3 + h, 2, 1, 5, 81720),
            Self::UpdateClr => (5, 4844 + 2 * cell + new_cell, 3 + h, 2, 0, 5, 81720),
            Self::UpdatePrune => (5, 4844 + cell + 2 * new_cell, 3 + h, 2, 0, 5, 81720),
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
            new_cell: u16::try_from(new_cell).map_err(|_| Error::InvalidInput)?,
            predecessor,
        })
    }
    #[cfg(test)]
    pub(crate) fn frame_bytes(self, cell: usize, predecessor: bool) -> Result<usize> {
        self.frame_bytes_cells(cell, 0, predecessor)
    }
    /// `frame_bytes` with UPDATE's new cell C' (see `shape_cells`).
    pub(crate) fn frame_bytes_cells(
        self,
        cell: usize,
        new_cell: usize,
        predecessor: bool,
    ) -> Result<usize> {
        Ok(if self.handoff() { 3776 } else { 2176 }
            + self.shape_cells(cell, new_cell, predecessor)?.body_bytes())
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
            | Self::DeletePrune
            | Self::UpdateForward
            | Self::UpdateClr
            | Self::UpdatePrune => root_job | (1 << 1) | (1 << 3) | (1 << 5),
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
    /// UPDATE's new row cell C' (185..=2228, so a u16 that the layout's
    /// padding holds); 0 for every other operation.
    pub(crate) new_cell: u16,
    pub(crate) predecessor: bool,
}
/// Size of a tombstone tree cell (XRW1 header 168 plus the 12-byte cell head).
pub(crate) const TOMBSTONE_CELL: usize = ROW_HEADER_BYTES + 12;
/// The one-slot Undo record without a payload.
const UNDO_RECORD: usize = UNDO_RECORD_BYTES;

/// Native presence (1 before, 2 after, 3 both) and the before/after body
/// sizes of one native kind of a Row-cell operation (dev3-row-delete.md
/// #delete-shape, dev3-row-update.md#update-shape). Kind 1 is the RowLeaf
/// cell, kind 3 the U record; every other kind keeps both sides at its fixed
/// size. `new_cell` is UPDATE's C' (0 for every other operation).
pub(super) fn native_sides(
    operation: Operation,
    kind: u8,
    cell: usize,
    new_cell: usize,
) -> Option<(u8, usize, usize)> {
    // The Live delete/update U: 432 plus the old row's canonical L = C - 180.
    // Only DELETE and UPDATE operations use it; every other one may carry 0.
    let live = || Some(UNDO_RECORD + cell.checked_sub(TOMBSTONE_CELL)?);
    Some(match (kind, operation) {
        (1, Operation::Forward) => (2, 0, cell),
        (1, Operation::Clr) => (1, cell, 0),
        (1, Operation::Prune) => (3, cell, cell),
        (1, Operation::DeleteForward) => (3, cell, TOMBSTONE_CELL),
        (1, Operation::DeleteClr) => (3, TOMBSTONE_CELL, cell),
        (1, Operation::DeletePrune) => (1, TOMBSTONE_CELL, 0),
        (1, Operation::UpdateForward) => (3, cell, new_cell),
        (1, Operation::UpdateClr) => (3, new_cell, cell),
        (1, Operation::UpdatePrune) => (3, new_cell, new_cell),
        (1, _) => return None,
        (3, Operation::DeleteForward | Operation::UpdateForward) => (3, UNDO_RECORD, live()?),
        (
            3,
            Operation::DeleteClr
            | Operation::DeletePrune
            | Operation::UpdateClr
            | Operation::UpdatePrune,
        ) => (3, live()?, UNDO_RECORD),
        (3, _) => (3, UNDO_RECORD, UNDO_RECORD),
        (5, _) => (3, 224, 224),
        (8, _) => (3, 512, 512),
        (9, _) => (3, 1024, 1024),
        _ => return None,
    })
}

/// C from a Row-cell operation's native total (the inverse of `shape`).
/// Non-row operations carry no cell; an UPDATE total names two cells, which
/// `update_cells_from_native` reads instead.
pub(super) fn cell_from_native(operation: Operation, native_bytes: usize) -> Result<usize> {
    let extra = || native_bytes.checked_sub(5024).ok_or(Error::Corrupt);
    Ok(match operation {
        Operation::UpdateForward | Operation::UpdateClr | Operation::UpdatePrune => {
            return Err(Error::InvalidInput);
        }
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

/// (C, C') of an UPDATE operation (dev3-row-update.md#update-shape): C'
/// from the one kind-1 record header of the native section `natives`
/// (UpdateForward's after size `@16`, UpdateClr's and UpdatePrune's before
/// size `@12`), then C from the section's total. This pre-scan runs before
/// the frame-size check, so it trusts no length: it walks at most the five
/// records of a Row operation, each at least its 128-byte header and within
/// the section, and the section must end after them. Every refusal,
/// a second kind-1 record included, is Corrupt. The caller's
/// `validate_native_shape` re-checks every record against (C, C'), this
/// header included.
pub(super) fn update_cells_from_native(
    operation: Operation,
    natives: &[u8],
) -> Result<(usize, usize)> {
    if !operation.update() {
        return Err(Error::InvalidInput);
    }
    let side = if operation == Operation::UpdateForward {
        16
    } else {
        12
    };
    let mut at = 0usize;
    let mut new_cell = None;
    for _ in 0..5 {
        if at == natives.len() {
            break;
        }
        let header = natives.get(at..at + 128).ok_or(Error::Corrupt)?;
        let length = super::u32_at(header, 20) as usize;
        if header[0] == 1
            && new_cell
                .replace(super::u32_at(header, side) as usize)
                .is_some()
        {
            return Err(Error::Corrupt);
        }
        at = at
            .checked_add(length)
            .filter(|&end| length >= 128 && end <= natives.len())
            .ok_or(Error::Corrupt)?;
    }
    if at != natives.len() {
        return Err(Error::Corrupt);
    }
    let new_cell = new_cell.ok_or(Error::Corrupt)?;
    let twice = usize::from(operation == Operation::UpdatePrune) + 1;
    let extra = natives
        .len()
        .checked_sub(4844)
        .and_then(|rest| rest.checked_sub(twice * new_cell))
        .ok_or(Error::Corrupt)?;
    let cell = if operation == Operation::UpdatePrune {
        extra
    } else if extra % 2 == 0 {
        extra / 2
    } else {
        return Err(Error::Corrupt);
    };
    if !(MIN_CELL..=MAX_CELL).contains(&cell) || !(MIN_CELL..=MAX_CELL).contains(&new_cell) {
        return Err(Error::Corrupt);
    }
    Ok((cell, new_cell))
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
