use foundation::error::{Certainty, Effect, RetryScope};
use std::fmt;

/// Engine operation result; current substrate errors leave the public root unchanged.
pub type Result<T> = std::result::Result<T, Error>;

/// Internal K0 error identities. No SQL/customer error catalog is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Invalid name, namespace, option or other structural input.
    InvalidInput,
    /// Column type/count/nullability mismatch.
    SchemaMismatch,
    /// A table already has the requested byte-exact name.
    TableExists,
    /// The requested name is absent from this snapshot.
    TableNotFound,
    /// Handle or RowUID does not belong to this instance, table or snapshot.
    StaleHandle,
    /// Replace/delete target is absent.
    RowNotFound,
    /// A documented count/byte bound was exceeded.
    LimitExceeded,
    /// Explicit quota or fallible allocation could not be reserved.
    QuotaExceeded,
    /// Object/row counter or epoch cannot issue another identity.
    IdentityExhausted,
}

impl ErrorCode {
    /// Stable internal identifier, independent of diagnostic formatting.
    pub const fn id(self) -> &'static str {
        match self {
            Self::InvalidInput => "XADB-ENGINE-0001",
            Self::SchemaMismatch => "XADB-ENGINE-0002",
            Self::TableExists => "XADB-ENGINE-0003",
            Self::TableNotFound => "XADB-ENGINE-0004",
            Self::StaleHandle => "XADB-ENGINE-0005",
            Self::RowNotFound => "XADB-ENGINE-0006",
            Self::LimitExceeded => "XADB-ENGINE-0007",
            Self::QuotaExceeded => "XADB-ENGINE-0008",
            Self::IdentityExhausted => "XADB-ENGINE-0009",
        }
    }
}

/// Typed substrate error. Issued IDs may be consumed even though the root is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Machine-readable failure category.
    pub code: ErrorCode,
    /// Static context without allocation or caller data.
    pub context: &'static str,
}

impl Error {
    pub(crate) const fn new(code: ErrorCode, context: &'static str) -> Self {
        Self { code, context }
    }
    /// Effect on the publicly visible row/catalog root, not on the ID high-water.
    pub const fn effect(&self) -> Effect {
        Effect::Unchanged
    }
    /// No storage effects are attempted by the volatile substrate.
    pub const fn certainty(&self) -> Certainty {
        Certainty::Known
    }
    /// Do not automatically replay unchanged inputs.
    pub const fn retry_scope(&self) -> RetryScope {
        RetryScope::Never
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.id(), self.context)
    }
}
impl std::error::Error for Error {}
