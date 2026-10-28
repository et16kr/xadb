//! Scope3 opcodes and fixed bounded section sizes. These values grant no credit.
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
    Reserve = 11,
    Cancel = 12,
    AgingStart = 16,
    TerminalFinish = 17,
    Prune = 20,
    ReleaseRole = 21,
    RootFree = 22,
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
            11 => Ok(Self::Reserve),
            12 => Ok(Self::Cancel),
            16 => Ok(Self::AgingStart),
            17 => Ok(Self::TerminalFinish),
            20 => Ok(Self::Prune),
            21 => Ok(Self::ReleaseRole),
            22 => Ok(Self::RootFree),
            _ => Err(Error::Unsupported),
        }
    }
    pub(crate) fn handoff(self) -> bool {
        matches!(
            self,
            Self::Begin
                | Self::Forward
                | Self::Clr
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
            | Self::RollbackStart
            | Self::StatementFinish
            | Self::Reserve
            | Self::Cancel => FrameKind::Update,
            Self::Clr => FrameKind::Clr,
            Self::Commit => FrameKind::Commit,
            Self::AbortEnd => FrameKind::AbortEnd,
            _ => FrameKind::System,
        }
    }
    pub(crate) fn shape(self, cell: usize, predecessor: bool) -> Result<Shape> {
        let row = matches!(self, Self::Forward | Self::Clr | Self::Prune);
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
            Self::Forward | Self::Clr | Self::Prune => root_job | (1 << 1) | (1 << 3) | (1 << 5),
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
