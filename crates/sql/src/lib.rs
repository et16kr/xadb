//! XADB SQL input processing, independent of storage and session effects.
//! The initial frontend does not execute SQL or provide a database connection.
#![deny(missing_docs)]

pub mod frontend;
