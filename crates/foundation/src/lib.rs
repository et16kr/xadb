//! Shared semantics and effect contracts below the engine/SQL/server boundary.
//! Concrete OS effects belong to xadb-platform and are supplied by the caller.
//! Internal formats may change between fresh internal builds.
#![deny(missing_docs)]
pub mod codec;
pub mod error;
pub mod identity;
pub mod io;
pub mod namespace;
pub mod network;
pub mod runtime;
pub mod value;

pub use error::{Error, ErrorCode, Result};
