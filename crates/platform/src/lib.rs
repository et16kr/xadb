//! Concrete OS effects implementing xadb-foundation contracts.
//! Linux is the only implemented and locally tested platform. New platforms
//! require their own adapters and qualification, not just another cfg branch.
#![deny(missing_docs)]
#[cfg(not(target_os = "linux"))]
compile_error!("xadb-platform currently supports Linux only");

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub mod namespace;
#[cfg(target_os = "linux")]
pub mod network;
#[cfg(target_os = "linux")]
pub mod performance;
#[cfg(target_os = "linux")]
pub mod runtime;
