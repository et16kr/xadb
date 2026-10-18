//! Error identity, effect certainty, and retry guidance below transaction ownership.
//! `uncertain_io_is_never_an_automatic_retry_or_rollback` tests the mapping.
use std::fmt;

/// Result carrying foundation error semantics, without transaction rollback claims.
pub type Result<T> = std::result::Result<T, Error>;

/// Current internal error catalog; stable identifiers are returned by [`Self::id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Zero or otherwise invalid identity.
    InvalidIdentity,
    /// Object identity, generation, or schema epoch mismatch.
    StaleHandle,
    /// Checked arithmetic or sequence exhausted.
    Overflow,
    /// Non-NULL types cannot be implicitly coerced.
    TypeMismatch,
    /// Declared input or capacity limit exceeded.
    LimitExceeded,
    /// Malformed or corrupt encoding.
    InvalidFormat,
    /// Recognized envelope with an unsupported format version.
    UnsupportedFormat,
    /// Explicit reservation or fallible allocation failed.
    QuotaExceeded,
    /// Cooperative cancellation observed before the caller-owned effect.
    Cancelled,
    /// Admission is no longer open.
    Closed,
    /// Read failed; persistent state is unchanged but the buffer is invalid.
    IoRead,
    /// Write, resize, or sync outcome requires caller resolution.
    IoWriteUnknown,
    /// A required schedule or fault did not execute as declared.
    InvalidSchedule,
    /// Shared state is poisoned and cannot resume normal service.
    Poisoned,
}

/// Effect on the owned operation, not a transaction-level rollback result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Persistent state unchanged; caller-owned output buffers may be partial.
    Unchanged,
    /// Partial or uncertain effects require the owning layer to resolve them.
    CallerMustResolve,
}

/// Guidance for the effect owner; never an instruction to replay writes automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryScope {
    /// Do not retry this unchanged request automatically.
    Never,
    /// The operation may be retried after discarding invalid read output.
    Operation,
    /// Resolve uncertain state through the owning recovery policy first.
    AfterRecovery,
}

/// Whether the failed operation has a known effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Certainty {
    /// The effect classification is known.
    Known,
    /// Successful or partial effects cannot be excluded.
    Unknown,
}

/// Foundation errors do not claim a transaction rollback that they cannot perform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Determines effect, certainty, and retry classification.
    pub code: ErrorCode,
    /// Static diagnostic context; formatting never includes a hidden dynamic payload.
    pub context: &'static str,
    /// OS error category when created from an I/O error.
    pub io_kind: Option<std::io::ErrorKind>,
}

impl Error {
    /// Construct a classified error with static context and no I/O category.
    pub const fn new(code: ErrorCode, context: &'static str) -> Self {
        Self {
            code,
            context,
            io_kind: None,
        }
    }

    /// Retain the OS category, discarding dynamic OS error text.
    /// The effect owner must select a code consistent with the operation.
    pub fn io(code: ErrorCode, context: &'static str, source: std::io::Error) -> Self {
        Self {
            code,
            context,
            io_kind: Some(source.kind()),
        }
    }

    /// Return conservative effect classification; read buffers may still contain a prefix.
    pub fn effect(&self) -> Effect {
        match self.code {
            ErrorCode::IoWriteUnknown | ErrorCode::Poisoned => Effect::CallerMustResolve,
            _ => Effect::Unchanged,
        }
    }

    /// Return Known for unchanged effects and Unknown when the caller must resolve them.
    pub fn certainty(&self) -> Certainty {
        match self.effect() {
            Effect::Unchanged => Certainty::Known,
            Effect::CallerMustResolve => Certainty::Unknown,
        }
    }

    /// Return the required retry boundary; recovery is the caller's responsibility.
    pub fn retry_scope(&self) -> RetryScope {
        match self.code {
            ErrorCode::IoWriteUnknown | ErrorCode::Poisoned => RetryScope::AfterRecovery,
            ErrorCode::IoRead => RetryScope::Operation,
            _ => RetryScope::Never,
        }
    }
}

impl ErrorCode {
    /// Return the internal XADB-FOUND identifier, independent of formatted text.
    pub const fn id(self) -> &'static str {
        match self {
            Self::InvalidIdentity => "XADB-FOUND-0001",
            Self::StaleHandle => "XADB-FOUND-0002",
            Self::Overflow => "XADB-FOUND-0003",
            Self::TypeMismatch => "XADB-FOUND-0004",
            Self::LimitExceeded => "XADB-FOUND-0005",
            Self::InvalidFormat => "XADB-FOUND-0006",
            Self::UnsupportedFormat => "XADB-FOUND-0007",
            Self::QuotaExceeded => "XADB-FOUND-0008",
            Self::Cancelled => "XADB-FOUND-0009",
            Self::Closed => "XADB-FOUND-0010",
            Self::IoRead => "XADB-FOUND-0011",
            Self::IoWriteUnknown => "XADB-FOUND-0012",
            Self::InvalidSchedule => "XADB-FOUND-0013",
            Self::Poisoned => "XADB-FOUND-0014",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.id(), self.context)
    }
}
impl std::error::Error for Error {}
