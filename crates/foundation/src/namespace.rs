//! Bounded database-directory effects below WAL, recovery and retention policy.
//!
//! A writer namespace owns exclusion for its entire lifetime and keeps exclusion
//! alive in every returned file. File sync and directory sync are separate effects.
//! These contracts do not turn a successful namespace call into a durable commit.
use crate::{
    error::{Certainty, Effect, RetryScope},
    io::FileIo,
};
use std::{fmt, io};

/// Maximum byte length of an engine-generated filename.
pub const MAX_FILE_NAME_BYTES: usize = 128;
/// Maximum entries one inventory request may retain, including the lock file.
pub const MAX_DIRECTORY_ENTRIES: usize = 65_536;

/// A bounded ASCII filename containing one component, never a filesystem path.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileName {
    bytes: [u8; MAX_FILE_NAME_BYTES],
    len: u8,
}

impl FileName {
    /// Validate an engine-generated filename without allocation or OS effects.
    ///
    /// # Errors
    /// Empty names, dot components, names over 128 bytes and characters outside
    /// ASCII letters, digits, underscore, hyphen and dot are rejected unchanged.
    pub fn new(value: &str) -> NamespaceResult<Self> {
        if value.is_empty()
            || value.len() > MAX_FILE_NAME_BYTES
            || matches!(value, "." | "..")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(NamespaceError::new(
                NamespaceErrorKind::InvalidName,
                Effect::Unchanged,
                "single bounded ASCII filename required",
            ));
        }
        let mut bytes = [0; MAX_FILE_NAME_BYTES];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self {
            bytes,
            len: value.len() as u8,
        })
    }

    /// Borrow the validated ASCII name; its storage is owned by this value.
    pub fn as_str(&self) -> &str {
        // Construction checks every byte and the private representation cannot
        // be mutated by callers. Failure here is an internal invariant violation.
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("FileName stores validated ASCII")
    }
}

impl fmt::Debug for FileName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("FileName")
            .field(&self.as_str())
            .finish()
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Access requested for an existing file; neither variant creates or truncates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Reading and explicit file sync are permitted by the adapter.
    ReadOnly,
    /// Positional reading and writing are permitted by the adapter.
    ReadWrite,
}

/// The two names that can be selected by same-directory atomic replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector {
    /// Active writer epoch and segment selection.
    Writer,
    /// Checkpoint and supported fallback selection.
    Current,
}

impl Selector {
    /// Return the fixed filename; logical selector contents belong to the engine.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Writer => "WRITER",
            Self::Current => "CURRENT",
        }
    }
}

/// Physical file observation, separate from the engine's logical file identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    /// Device identifier reported by the supported filesystem adapter.
    pub device: u64,
    /// Inode identifier reported by the supported filesystem adapter.
    pub inode: u64,
}

/// Metadata of a regular, single-link file within the owned directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    /// Physical observation used to detect replaced names, never a WAL identity.
    pub identity: FileIdentity,
    /// Current length, limited to the signed 64-bit file-offset domain.
    pub length: u64,
}

/// One entry in a complete bounded inventory, including `LOCK` when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// Validated single-component name.
    pub name: FileName,
    /// Regular-file metadata observed during enumeration.
    pub metadata: FileMetadata,
}

/// Namespace failure category; effect certainty is recorded separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamespaceErrorKind {
    /// Another independent handle or process owns the writer lock.
    Busy,
    /// An exclusive-create target already exists.
    AlreadyExists,
    /// A required existing name is absent.
    NotFound,
    /// Filename, root path or reserved-name use is invalid.
    InvalidName,
    /// A symlink, unexpected file kind, hard link or replaced name was observed.
    InvalidType,
    /// A file-size, inventory or allocation bound was exceeded.
    LimitExceeded,
    /// An operating-system effect or observation failed.
    Io,
}

/// A namespace error without an invented transaction rollback or commit verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespaceError {
    /// Operation-specific category, independent of the OS's formatted message.
    pub kind: NamespaceErrorKind,
    /// Whether a persistent namespace effect may have occurred.
    pub effect: Effect,
    /// Original OS category where available.
    pub io_kind: Option<io::ErrorKind>,
    /// Static context; contains no hidden path or dynamic OS message.
    pub context: &'static str,
}

impl NamespaceError {
    /// Construct an error with explicit effect and no OS category.
    pub const fn new(kind: NamespaceErrorKind, effect: Effect, context: &'static str) -> Self {
        Self {
            kind,
            effect,
            io_kind: None,
            context,
        }
    }

    /// Preserve the OS category and caller-selected conservative effect.
    pub fn io(
        kind: NamespaceErrorKind,
        effect: Effect,
        context: &'static str,
        error: io::Error,
    ) -> Self {
        Self {
            kind,
            effect,
            io_kind: Some(error.kind()),
            context,
        }
    }

    /// Unknown means prior effects cannot be excluded; it does not mean rollback.
    pub const fn certainty(&self) -> Certainty {
        match self.effect {
            Effect::Unchanged => Certainty::Known,
            Effect::CallerMustResolve => Certainty::Unknown,
        }
    }

    /// Never automatically replay namespace mutations or writer acquisition.
    pub const fn retry_scope(&self) -> RetryScope {
        match self.effect {
            Effect::Unchanged => RetryScope::Never,
            Effect::CallerMustResolve => RetryScope::AfterRecovery,
        }
    }
}

impl fmt::Display for NamespaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "XADB namespace {:?}: {}",
            self.kind, self.context
        )
    }
}
impl std::error::Error for NamespaceError {}

/// Result preserving namespace category and effect uncertainty.
pub type NamespaceResult<T> = std::result::Result<T, NamespaceError>;

/// An already-open caller-owned directory from which an exclusive writer is acquired.
pub trait DirectoryPort {
    /// Owning writer capability. Dropping it never unlinks the permanent lock file.
    type Writer: WriterNamespace;

    /// Try to acquire the writer lock without waiting or retrying.
    ///
    /// # Errors
    /// Busy means no writer capability was returned. If this attempt created the
    /// permanent lock file before failing, the namespace effect is still uncertain.
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer>;
}

/// Excluded namespace effects; WAL order, selector contents and retirement are caller-owned.
pub trait WriterNamespace {
    /// Owned positional file retaining writer exclusion until the file is dropped.
    type File: FileIo;

    /// Read one nonzero 128-bit incarnation from OS entropy without DB effects.
    ///
    /// # Errors
    /// Missing entropy, short/error reads or an all-zero value fail unchanged.
    /// Logical collision checks and durable identity selection belong to the engine.
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]>;

    /// Exclusively create a regular file without replacing or truncating a name.
    ///
    /// # Errors
    /// Lock and selector names are reserved; selectors must use prepared temporary
    /// files and [`Self::publish_selector`]. Existing names fail unchanged.
    /// Other post-attempt errors may leave a new file;
    /// implementations must not hide that effect by automatically unlinking it.
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<Self::File>;

    /// Open an existing regular file, without create, truncate or append semantics.
    ///
    /// # Errors
    /// Missing, reserved, linked or invalid file types and OS errors leave DB bytes unchanged.
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<Self::File>;

    /// Observe one regular file without following links; absence is `None`.
    ///
    /// # Errors
    /// Invalid file kinds, size range or metadata errors fail unchanged.
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>>;

    /// Return a complete bounded inventory; never silently skip invalid names or types.
    ///
    /// # Errors
    /// Bounds, invalid entries or enumeration failure discard the partial inventory.
    /// A zero limit succeeds only if the directory contains no entries except dot entries.
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>>;

    /// Explicitly persist this directory's namespace; does not sync file contents.
    ///
    /// # Errors
    /// Errors are uncertain even if preceding file syncs succeeded.
    fn sync_directory(&mut self) -> NamespaceResult<()>;

    /// Rename a prepared temporary file onto one selector in the same directory.
    /// The caller must sync the temporary file first and the directory afterward.
    ///
    /// # Errors
    /// Preflight errors are unchanged; errors after rename is attempted are uncertain.
    fn publish_selector(&mut self, temporary: &FileName, selected: Selector)
    -> NamespaceResult<()>;

    /// Unlink a file the caller has already marked Retiring in its retention ledger.
    /// Lock and selector files cannot be retired through this API. Directory sync is separate.
    ///
    /// # Errors
    /// Preflight errors are unchanged; attempted-unlink failures are uncertain.
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()>;
}
