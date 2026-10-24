//! Independent bounded history oracle for DEV2, with no production semantics dependency.
//! See `devdoc/implementation/dev2-implementation.md`, decision DEV2-P0-001.
#![deny(missing_docs)]

pub mod catalog;
pub mod frontend;
mod history;
mod model;
mod validation;

pub use history::{CheckResult, SearchBudget, Verdict, check};
