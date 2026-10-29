//! Closed scope2 Catalog CREATE groups. Byte views below do not grant WAL,
//! page installation or retirement rights. The original owner supplies stable
//! before images and issues those capabilities only after semantic validation.
#![allow(dead_code)] // The live and replay consumers are connected in this change.

use super::{
    catalog_record::CatalogError,
    page::PageError,
    retention_page::RetentionError,
    undo_record::UndoError,
    wal_frame::{FrameKind, WalError},
};

#[cfg(test)]
mod catalog3_tests;
mod core;
mod effects;
mod format;
mod frame;
pub(crate) mod images;
pub(crate) mod inventory;
#[cfg(test)]
mod leaf_tests;
pub(crate) mod ledger;
mod projection;
pub(crate) mod queue;
mod resources;
mod transition;
#[cfg(test)]
mod transition_tests;
mod wire;
pub(crate) use core::{CoreView, decode_core};
pub(crate) use effects::{EffectIndices, reference_changes};
pub(crate) use projection::catalog_projection_digest;
#[cfg(test)]
pub(crate) use resources::source_branches;
pub(crate) use resources::{
    borrowed_source_bytes, catalog_read_codec_bytes, layout_source_bytes, page_read_codec_bytes,
    profile3_transaction_source_bytes, schema_source_bytes, source_bytes, source_census,
    status_read_codec_bytes, transaction_source_bytes, undo_read_codec_bytes,
};
#[cfg(test)]
pub(crate) use transition::observe;
pub(crate) use transition::{Change, Inputs, Observation, observe_borrowed, produce};
pub(crate) use wire::{LayoutView, decode_layout};

pub(crate) const REPLAY_SCRATCH_BYTES: usize = 3_328;

pub(crate) const FRAME_BYTES: usize = 163_840;
pub(crate) const PRIVATE_BYTES: usize = 425_984;
pub(crate) const MIN_CELL: usize = 254;
pub(crate) const MAX_CELL: usize = 4_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    InvalidInput,
    Unsupported,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
    Exhausted,
    Page(PageError),
    Wal(WalError),
    Catalog(CatalogError),
    Undo(UndoError),
    Queue(RetentionError),
    Descriptor(super::obligation::ObligationError),
    Aging(super::retention_obligation::AgingObligationError),
    Allocation(super::allocation_page::AllocationError),
    Tree(super::tree_page::TreeError),
    LeafEdit(super::tree_page::edit::LeafEditError),
}
type Result<T> = std::result::Result<T, Error>;
macro_rules! conversion {
    ($error:ty, $variant:ident) => {
        impl From<$error> for Error {
            fn from(error: $error) -> Self {
                Self::$variant(error)
            }
        }
    };
}
conversion!(PageError, Page);
conversion!(WalError, Wal);
conversion!(CatalogError, Catalog);
conversion!(UndoError, Undo);
conversion!(RetentionError, Queue);
conversion!(super::obligation::ObligationError, Descriptor);
conversion!(super::retention_obligation::AgingObligationError, Aging);
conversion!(super::allocation_page::AllocationError, Allocation);
conversion!(super::tree_page::TreeError, Tree);
conversion!(super::tree_page::edit::LeafEditError, LeafEdit);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum Operation {
    Begin = 1,
    Forward = 2,
    RollbackStart = 3,
    Clr = 4,
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
    pub(crate) fn outer(self) -> FrameKind {
        match self {
            Self::Begin => FrameKind::Begin,
            Self::Forward | Self::RollbackStart | Self::Reserve | Self::Cancel => FrameKind::Update,
            Self::Clr => FrameKind::Clr,
            Self::Commit => FrameKind::Commit,
            Self::AbortEnd => FrameKind::AbortEnd,
            _ => FrameKind::System,
        }
    }
    pub(crate) fn shape(self, catalog_cell_bytes: usize) -> Result<Shape> {
        let c = catalog_cell_bytes;
        if matches!(self, Self::Forward | Self::Clr | Self::Prune) {
            if !(MIN_CELL..=MAX_CELL).contains(&c) {
                return Err(Error::InvalidInput);
            }
        } else if c != 0 {
            return Err(Error::InvalidInput);
        }
        let (native_count, native_bytes, edges, counts, items, redo_count, redo_bytes) = match self
        {
            Self::Begin | Self::Commit | Self::AbortEnd => (3, 3904, 0, 0, 0, 3, 49032),
            Self::AgingStart => (1, 1152, 0, 0, 0, 1, 16344),
            Self::Reserve => (4, 16864, 0, 0, 3, 4, 65376),
            Self::Forward => (8, 17904 + c, 3, 2, 3, 8, 130960),
            Self::Cancel => (3, 15872, 0, 0, 3, 3, 49032),
            Self::Clr => (6, 17568 + c, 3, 2, 0, 6, 98064),
            Self::RollbackStart => (1, 576, 0, 0, 0, 1, 288),
            Self::Prune => (5, 5024 + 2 * c, 3, 2, 0, 5, 81720),
            Self::ReleaseRole | Self::RootFree => (3, 4320, 0, 0, 0, 3, 49032),
            Self::TerminalFinish => (3, 3904, 0, 0, 0, 3, 49032),
        };
        Ok(Shape {
            native_count,
            native_bytes,
            edges,
            counts,
            items,
            redo_count,
            redo_bytes,
        })
    }
    pub(crate) fn frame_bytes(self, catalog_cell_bytes: usize) -> Result<usize> {
        Ok((if self.handoff() { 3776 } else { 2176 })
            + self.shape(catalog_cell_bytes)?.body_bytes())
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
}
impl Shape {
    fn auxiliary_bytes(self) -> usize {
        usize::from(self.edges) * 128
            + usize::from(self.counts) * 96
            + usize::from(self.items) * 208
    }
    fn body_bytes(self) -> usize {
        self.native_bytes + self.auxiliary_bytes() + self.redo_bytes
    }
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("checked field"))
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("checked field"))
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("checked field"))
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
