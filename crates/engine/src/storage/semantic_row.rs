//! Scope3 Row observations. These pure inputs do not grant admission, statement
//! ownership, WAL staging, installation, history or selected recovery authority.
#![allow(dead_code)] // The current-profile semantic and live owners are pending.

mod core;
pub(crate) mod ledger;
mod operation;
mod wire;
pub(crate) use core::{CoreView, decode_core};
pub(crate) use operation::{Operation, Shape};
#[allow(unused_imports, reason = "scope3 live/replay consumers are pending")]
pub(crate) use wire::{LayoutView, decode_layout};

#[cfg(test)]
mod wire_tests;

pub(crate) const FRAME_BYTES: usize = 98_304;
pub(crate) const MIN_CELL: usize = 185;
pub(crate) const MAX_CELL: usize = 2_228;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    InvalidInput,
    Unsupported,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
    Exhausted,
    Queue(super::retention_page::RetentionError),
    Descriptor(super::obligation::ObligationError),
    Page(super::page::PageError),
    Wal(super::wal_frame::WalError),
}
impl From<super::page::PageError> for Error {
    fn from(value: super::page::PageError) -> Self {
        Self::Page(value)
    }
}
impl From<super::wal_frame::WalError> for Error {
    fn from(value: super::wal_frame::WalError) -> Self {
        Self::Wal(value)
    }
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| byte == 0)
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
type Result<T> = std::result::Result<T, Error>;
impl From<super::retention_page::RetentionError> for Error {
    fn from(value: super::retention_page::RetentionError) -> Self {
        Self::Queue(value)
    }
}
impl From<super::obligation::ObligationError> for Error {
    fn from(value: super::obligation::ObligationError) -> Self {
        Self::Descriptor(value)
    }
}
