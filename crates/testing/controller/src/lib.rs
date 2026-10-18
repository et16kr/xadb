//! Minimal independent XADBTest controller and its public input/result boundary.
//! Standalone case PASS binds retained case/worker bytes, not a live source checkout.
//! Stage checkers additionally verify pre/post source identity. No release qualification.
#[cfg(not(target_os = "linux"))]
compile_error!("XADBTest DEV0 supports Linux only");

pub mod catalog;
mod controller;
pub mod native;
mod oracle;
mod process;
pub mod spec;
pub mod verdict;

pub use controller::{RunOptions, RunReport, file_sha256, run, sha256};
