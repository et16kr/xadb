//! DEV2's volatile development substrate for the disk-engine project. Currently volatile: no durable Engine API yet.
//! See DEV2-K0-001 in `devdoc/implementation/dev2-implementation.md`.
#![deny(missing_docs)]

pub mod api;
mod catalog;
pub mod diagnostics;
mod error;
mod identity;
mod metrics;
mod storage;
pub mod substrate;

pub use error::{Error, ErrorCode, Result};
