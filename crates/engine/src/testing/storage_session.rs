//! Feature-gated native assembly for the actual fresh-storage owner.
use super::{
    BufferTraceFailure, PageReadFailure,
    genesis_wal::{GenesisWalFailure, GenesisWalPrefix},
};
use crate::{
    api::DatabaseId,
    storage::{
        page::{FileId, PageIdentity, PageKind, PageRef},
        read_buffer::{ReadBuffer, ReadFrame},
        wal_io::{
            BootstrapCause as Cause, BootstrapError, BootstrapPhase as Phase, FreshStorageSession,
        },
    },
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    identity::{Generation, ObjectId},
    namespace::WriterNamespace,
};

/// Exact creation phase and cumulative effect; original genesis failure retained.
#[derive(Debug)]
pub struct StorageFailure {
    /// Creation phase of the fresh storage owner.
    pub phase: &'static str,
    /// Cumulative effect, never a transaction outcome.
    pub effect: Effect,
    /// Lower error category.
    pub cause: &'static str,
    /// Original WAL phase/effect/category when genesis failed.
    pub wal: Option<GenesisWalFailure>,
}
impl From<BootstrapError> for StorageFailure {
    fn from(error: BootstrapError) -> Self {
        let phase = match error.phase {
            Phase::Preflight => "preflight",
            Phase::Genesis => "genesis",
            Phase::Prepare => "prepare",
            Phase::DescriptorCreate => "descriptor_create",
            Phase::DescriptorWrite => "descriptor_write",
            Phase::DescriptorSync => "descriptor_sync",
            Phase::DescriptorDirectory => "descriptor_directory",
            Phase::DataCreate => "data_create",
            Phase::DataWrite => "data_write",
            Phase::DataSync => "data_sync",
            Phase::Bank0Create => "bank0_create",
            Phase::Bank0Write => "bank0_write",
            Phase::Bank0Sync => "bank0_sync",
            Phase::Bank1Create => "bank1_create",
            Phase::Bank1Write => "bank1_write",
            Phase::Bank1Sync => "bank1_sync",
            Phase::FilesDirectory => "files_directory",
            Phase::SelectorCreate => "selector_create",
            Phase::SelectorWrite => "selector_write",
            Phase::SelectorSync => "selector_sync",
            Phase::SelectorPublish => "selector_publish",
            Phase::SelectorDirectory => "selector_directory",
            Phase::Readback => "readback",
        };
        let (cause, wal) = match error.cause {
            Cause::Wal(e) => ("wal", Some(e.into())),
            Cause::Control(_) => ("control", None),
            Cause::Allocation(_) => ("allocation", None),
            Cause::PageIo(_) => ("page_io", None),
            Cause::Namespace(_) => ("namespace", None),
            Cause::Io(_) => ("io", None),
            Cause::InvalidInput => ("invalid_input", None),
            Cause::Capacity => ("capacity", None),
            Cause::IdentityMismatch => ("identity_mismatch", None),
            Cause::LengthMismatch => ("length_mismatch", None),
        };
        Self {
            phase,
            effect: error.effect,
            cause,
            wal,
        }
    }
}

/// Read/buffer/WAL projection for the native integration probe.
#[derive(Debug)]
pub enum StorageProbeFailure {
    /// Unexpected invalid primitive identity.
    InvalidInput,
    /// Actual selected reader failure.
    Page(PageReadFailure),
    /// Actual buffer operation failure.
    Buffer(BufferTraceFailure),
    /// Actual physical WAL failure.
    Wal(GenesisWalFailure),
    /// The second reader was not rejected as Busy.
    MissingBusy,
}

/// Thin wrapper retaining the private product owner and caller slot lifetime.
pub struct FreshStorage<'m, W: WriterNamespace>(FreshStorageSession<'m, W>);
impl<'m, W: WriterNamespace> FreshStorage<'m, W> {
    /// Execute the actual genesis/extent/FILEMAP constructor.
    pub fn create(
        writer: W,
        profile: u32,
        slot: &'m mut [u8; 1048576],
        scratch: &mut [[u8; 16384]; 3],
    ) -> Result<Self, StorageFailure> {
        FreshStorageSession::create(writer, profile, slot, scratch)
            .map(Self)
            .map_err(Into::into)
    }
    /// Observe physical WAL prefix without any commit/READY authority.
    pub fn prefix(&self) -> Result<GenesisWalPrefix, GenesisWalFailure> {
        self.0.prefix().map(Into::into).map_err(Into::into)
    }
    /// Concrete owner bytes, excluding caller workspace and files' backing data.
    pub fn metadata_bytes() -> usize {
        std::mem::size_of::<FreshStorageSession<'m, W>>()
    }
    /// Read through the shared buffer while retaining a pin/guard across WAL I/O.
    /// Stack workspaces here belong only to this testing adapter.
    pub fn read_and_append(
        &self,
        bytes: &[u8],
    ) -> Result<([u8; 32], usize, GenesisWalPrefix), StorageProbeFailure> {
        use StorageProbeFailure as E;
        let prefix = self.prefix().map_err(E::Wal)?;
        let identity = PageIdentity {
            database: DatabaseId::new(prefix.database).map_err(|_| E::InvalidInput)?,
            page: PageRef {
                file: FileId::new(0x100000001).map_err(|_| E::InvalidInput)?,
                page_no: 0,
                incarnation: Generation::new(1).map_err(|_| E::InvalidInput)?,
            },
            owner: ObjectId::new(1).map_err(|_| E::InvalidInput)?,
            owner_generation: Generation::new(1).map_err(|_| E::InvalidInput)?,
        };
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let mut frames = [ReadFrame::new()];
        let reader = self
            .0
            .open_reader(&mut root, &mut scratch)
            .map_err(|e| E::Page(e.into()))?;
        let mut other_root = [0; 16384];
        let mut other_scratch = [0; 16384];
        if !matches!(
            self.0.open_reader(&mut other_root, &mut other_scratch),
            Err(crate::storage::page_io::PageIoError::Busy)
        ) {
            return Err(E::MissingBusy);
        }
        let buffer =
            ReadBuffer::new(reader, &mut frames, &mut load).map_err(|e| E::Buffer(e.into()))?;
        let pin = buffer
            .pin(identity, PageKind::Allocation, 0)
            .map_err(|e| E::Buffer(e.into()))?;
        let guard = pin.read().map_err(|e| E::Buffer(e.into()))?;
        let count = guard.view().map_err(|e| E::Buffer(e.into()))?.len();
        let digest = content_digest(guard.bytes());
        self.0.try_stage(bytes).map_err(|e| E::Wal(e.into()))?;
        let prefix = self
            .0
            .sync_pending()
            .map_err(|e| E::Wal(e.into()))?
            .prefix
            .into();
        if digest != content_digest(guard.bytes()) {
            return Err(E::InvalidInput);
        }
        Ok((digest, count, prefix))
    }
}
