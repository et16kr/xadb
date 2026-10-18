//! DEV2 typed transactions and disk-backed WAL/checkpoint recovery, alongside
//! the temporary volatile row substrate. Current-build durability qualification
//! is recorded separately in `devdoc/implementation/dev2-implementation.md`.
#![deny(missing_docs)]

pub mod api;
mod catalog;
pub mod database;
pub mod diagnostics;
mod durable;
mod error;
mod identity;
mod metrics;
mod publication;
mod storage;
pub mod substrate;
pub mod transaction;

pub use error::{Error, ErrorCode, Result};
