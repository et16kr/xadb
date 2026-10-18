//! Persistence errors retain the decision boundary instead of guessing rollback.
use std::fmt;

/// Internal durable-kernel error identity, separate from the K0 substrate catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurableErrorCode {
    /// The supplied path, option, or operation is invalid before any effect.
    InvalidInput,
    /// A required record, identity, checksum, or chain is inconsistent.
    Corruption,
    /// A selected file requires an unsupported current-build format or capability.
    UnsupportedFormat,
    /// A declared byte, file, sequence, or workspace bound was exhausted.
    LimitExceeded,
    /// Fallible allocation or explicit resource reservation failed.
    QuotaExceeded,
    /// A file or namespace operation failed; inspect outcome and fenced state.
    Io,
    /// Another writer owns this database.
    Busy,
    /// This engine must be recovered before it can admit more work.
    Fenced,
    /// Required selectors are absent at open. This does not distinguish an
    /// interrupted creation/restore from later deletion and permits no repair.
    IncompleteCreate,
    /// The transaction layer rejected the operation before a durable attempt.
    Transaction,
}

/// What is known about the affected transaction when an operation fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionOutcome {
    /// No transaction commit was attempted by this operation.
    NotAttempted,
    /// The transaction was discarded before its first commit-byte attempt.
    Aborted,
    /// A complete or partial commit may exist; recovery must decide.
    Unknown,
    /// Sync established the commit; a later publication/reply step failed.
    Committed,
}

/// A durable operation failure with explicit effect and recovery obligations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableError {
    /// Stable internal failure category.
    pub code: DurableErrorCode,
    /// Allocation-free diagnostic context.
    pub context: &'static str,
    /// Transaction decision, independent of metadata effects.
    pub outcome: TransactionOutcome,
    /// Whether normal admission is barred until a new engine recovers the files.
    pub fenced: bool,
    /// OS category when the originating boundary supplied one.
    pub io_kind: Option<std::io::ErrorKind>,
}

pub(crate) type Result<T> = std::result::Result<T, DurableError>;

impl DurableError {
    pub(crate) const fn new(code: DurableErrorCode, context: &'static str) -> Self {
        Self {
            code,
            context,
            outcome: TransactionOutcome::NotAttempted,
            fenced: false,
            io_kind: None,
        }
    }

    pub(crate) const fn invalid(context: &'static str) -> Self {
        Self::new(DurableErrorCode::Corruption, context)
    }

    pub(crate) const fn limit(context: &'static str) -> Self {
        Self::new(DurableErrorCode::LimitExceeded, context)
    }

    pub(crate) const fn allocation() -> Self {
        Self::new(
            DurableErrorCode::QuotaExceeded,
            "durable workspace allocation",
        )
    }

    pub(crate) fn foundation(error: xadb_foundation::Error) -> Self {
        use xadb_foundation::ErrorCode;
        let code = match error.code {
            ErrorCode::UnsupportedFormat => DurableErrorCode::UnsupportedFormat,
            ErrorCode::LimitExceeded | ErrorCode::Overflow => DurableErrorCode::LimitExceeded,
            ErrorCode::QuotaExceeded => DurableErrorCode::QuotaExceeded,
            ErrorCode::IoRead | ErrorCode::IoWriteUnknown => DurableErrorCode::Io,
            _ => DurableErrorCode::Corruption,
        };
        Self {
            io_kind: error.io_kind,
            ..Self::new(code, error.context)
        }
    }
}

impl fmt::Display for DurableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}: {} ({:?}, fenced={})",
            self.code, self.context, self.outcome, self.fenced
        )
    }
}

impl std::error::Error for DurableError {}
