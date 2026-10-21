//! Feature-gated assembly for fresh physical WAL tests; no database readiness.
use crate::storage::wal_io::{self, GenesisWalSession};
use xadb_foundation::{error::Effect, namespace::WriterNamespace};

/// Test projection retaining the cumulative effect and original cause category.
#[derive(Debug, PartialEq, Eq)]
pub struct GenesisWalFailure {
    /// Installation/append phase in which the error was observed.
    pub phase: &'static str,
    /// Cumulative component effect, not a transaction outcome.
    pub effect: Effect,
    /// Original cause category.
    pub cause: &'static str,
}
impl From<wal_io::WalIoError> for GenesisWalFailure {
    fn from(error: wal_io::WalIoError) -> Self {
        use wal_io::{Cause, Phase};
        Self {
            phase: match error.phase {
                Phase::Preflight => "preflight",
                Phase::Entropy => "entropy",
                Phase::ProfileCreate => "profile_create",
                Phase::ProfileWrite => "profile_write",
                Phase::ProfileSync => "profile_sync",
                Phase::EpochCreate => "epoch_create",
                Phase::EpochWrite => "epoch_write",
                Phase::EpochSync => "epoch_sync",
                Phase::InitialDirectorySync => "initial_directory_sync",
                Phase::SelectorCreate => "selector_create",
                Phase::SelectorWrite => "selector_write",
                Phase::SelectorSync => "selector_sync",
                Phase::SelectorPublish => "selector_publish",
                Phase::SelectedDirectorySync => "selected_directory_sync",
                Phase::Readback => "readback",
                Phase::Stage => "stage",
                Phase::Write => "write",
                Phase::Sync => "sync",
                Phase::Publish => "publish",
                Phase::Observe => "observe",
            },
            effect: error.effect,
            cause: match error.cause {
                Cause::InvalidInput => "invalid_input",
                Cause::Busy => "busy",
                Cause::NoPending => "no_pending",
                Cause::Fenced => "fenced",
                Cause::Control(_) => "control",
                Cause::Frame(_) => "frame",
                Cause::Namespace(_) => "namespace",
                Cause::Io(_) => "io",
                Cause::IdentityMismatch => "identity_mismatch",
                Cause::LengthMismatch => "length_mismatch",
                Cause::MappingMismatch => "mapping_mismatch",
                Cause::Capacity => "capacity",
                Cause::ArithmeticOverflow => "arithmetic_overflow",
                Cause::Poisoned => "poisoned",
            },
        }
    }
}

/// Copyable physical observations only; no dirty page/commit authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenesisWalPrefix {
    /// Actual entropy-derived database identity.
    pub database: [u8; 16],
    /// Opened epoch file identity `(device, inode)`.
    pub file: (u64, u64),
    /// Last completely staged logical LSN.
    pub complete_lsn: u64,
    /// Byte end of the complete interval.
    pub complete_offset: u64,
    /// Last logical LSN with successful file sync and publication.
    pub synced_lsn: u64,
    /// Corresponding physical byte end.
    pub synced_offset: u64,
}
impl From<wal_io::WalPrefix> for GenesisWalPrefix {
    fn from(value: wal_io::WalPrefix) -> Self {
        Self {
            database: value.stream.database.bytes(),
            file: (value.file.device, value.file.inode),
            complete_lsn: value.complete_lsn,
            complete_offset: value.complete_offset,
            synced_lsn: value.synced_lsn,
            synced_offset: value.synced_offset,
        }
    }
}

/// Thin native test adapter owning the actual private product session.
pub struct GenesisWal<'m, W: WriterNamespace>(GenesisWalSession<'m, W>);
impl<'m, W: WriterNamespace> GenesisWal<'m, W> {
    /// Install a fresh genesis using the real constructor and caller workspace.
    pub fn create(
        writer: W,
        profile: u32,
        slot: &'m mut [u8; 1048576],
        scratch: &mut [u8; 16384],
    ) -> Result<Self, GenesisWalFailure> {
        GenesisWalSession::create(writer, profile, slot, scratch)
            .map(Self)
            .map_err(Into::into)
    }
    /// Run the real decoder, bounded copy and publication; return physical prefix.
    pub fn stage(&self, bytes: &[u8]) -> Result<GenesisWalPrefix, GenesisWalFailure> {
        self.0.try_stage(bytes)?;
        self.prefix()
    }
    /// Run actual write/sync and return its confirmed physical observation.
    pub fn sync(&self) -> Result<GenesisWalPrefix, GenesisWalFailure> {
        self.0
            .sync_pending()
            .map(|o| o.prefix.into())
            .map_err(Into::into)
    }
    /// Observe the current complete/synced physical prefixes.
    pub fn prefix(&self) -> Result<GenesisWalPrefix, GenesisWalFailure> {
        self.0.prefix().map(Into::into).map_err(Into::into)
    }
    /// Check the actual session fence without acquiring the append gate.
    pub fn is_fenced(&self) -> bool {
        self.0.is_fenced()
    }
    /// Concrete session metadata; excludes separately supplied slot/scratch.
    pub fn metadata_bytes() -> usize {
        std::mem::size_of::<GenesisWalSession<'m, W>>()
    }
}
