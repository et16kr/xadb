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
            BootstrapCause as Cause, BootstrapError, BootstrapPhase as Phase,
            DoublewriteInstallCause, DoublewriteInstallError, DoublewriteInstallPhase,
            DoublewriteStorageSession, FreshStorageSession,
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
    /// Consume the actual pristine owner and execute its doublewrite initializer.
    pub fn install_doublewrite<'w>(
        self,
        workspace: &'w mut [u8; 4194304],
    ) -> Result<DoublewriteStorage<'m, 'w, W>, DoublewriteFailure> {
        self.0
            .install_doublewrite(workspace)
            .map(DoublewriteStorage)
            .map_err(Into::into)
    }
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

/// Native-test projection of the exact consumed-owner failure and lower cause.
#[derive(Debug)]
pub struct DoublewriteFailure {
    /// Original installation phase.
    pub phase: &'static str,
    /// Cumulative effect, including the consumed Fresh artifacts.
    pub effect: Effect,
    /// Original typed cause category.
    pub cause: &'static str,
    /// Original lower error fields, for trace comparison only.
    pub detail: String,
    /// Original foundation error when the cause is I/O.
    pub lower_io: Option<xadb_foundation::error::Error>,
    /// Original namespace error, including its effect classification.
    pub lower_namespace: Option<xadb_foundation::namespace::NamespaceError>,
    /// Attempted logical mutations, excluding retries and reads.
    pub attempted: u8,
    /// Successfully returned logical mutations.
    pub completed: u8,
}
impl From<DoublewriteInstallError> for DoublewriteFailure {
    fn from(error: DoublewriteInstallError) -> Self {
        use DoublewriteInstallCause as C;
        use DoublewriteInstallPhase as P;
        let phase = match error.phase {
            P::StatePreflight => "state_preflight",
            P::NamespacePreflight => "namespace_preflight",
            P::Prepare => "prepare",
            P::DescriptorCreate => "descriptor_create",
            P::DescriptorWrite => "descriptor_write",
            P::DescriptorSync => "descriptor_sync",
            P::DescriptorDirectory => "descriptor_directory",
            P::ImagesCreate => "images_create",
            P::ImagesWrite => "images_write",
            P::ImagesSync => "images_sync",
            P::Bank0Create => "bank0_create",
            P::Bank0Write => "bank0_write",
            P::Bank0Sync => "bank0_sync",
            P::Bank1Create => "bank1_create",
            P::Bank1Write => "bank1_write",
            P::Bank1Sync => "bank1_sync",
            P::ReservedCreate => "reserved_create",
            P::ReservedWrite => "reserved_write",
            P::ReservedSync => "reserved_sync",
            P::FilesDirectory => "files_directory",
            P::SelectorCreate => "selector_create",
            P::SelectorWrite => "selector_write",
            P::SelectorSync => "selector_sync",
            P::SelectorPublish => "selector_publish",
            P::SelectorDirectory => "selector_directory",
            P::Readback => "readback",
        };
        let cause = match &error.cause {
            C::InvalidState => "invalid_state",
            C::Busy => "busy",
            C::Fenced => "fenced",
            C::Poisoned => "poisoned",
            C::Capacity => "capacity",
            C::Residue => "residue",
            C::Control(_) => "control",
            C::Doublewrite(_) => "doublewrite",
            C::Namespace(_) => "namespace",
            C::Io(_) => "io",
            C::ContentMismatch => "content_mismatch",
            C::SelectionMismatch => "selection_mismatch",
            C::IdentityMismatch => "identity_mismatch",
            C::LengthMismatch => "length_mismatch",
            C::Invariant => "invariant",
        };
        Self {
            phase,
            effect: error.effect,
            cause,
            detail: format!("{:?}", error.cause),
            lower_io: match &error.cause {
                C::Io(e) => Some(e.clone()),
                _ => None,
            },
            lower_namespace: match &error.cause {
                C::Namespace(e) => Some(e.clone()),
                _ => None,
            },
            attempted: error.attempted,
            completed: error.completed,
        }
    }
}

/// Thin native facade; owns the real session and offers only a clean read probe.
pub struct DoublewriteStorage<'m, 'w, W: WriterNamespace>(DoublewriteStorageSession<'m, 'w, W>);
impl<W: WriterNamespace> DoublewriteStorage<'_, '_, W> {
    /// Concrete owner bytes, excluding the separately borrowed 1MiB/4MiB buffers.
    pub fn metadata_bytes() -> usize {
        std::mem::size_of::<DoublewriteStorageSession<'_, '_, W>>()
    }
    /// Read the original allocation page through the common reader/buffer fence.
    /// Test workspaces are external to the private product path.
    pub fn read_allocation(
        &self,
        database: [u8; 16],
    ) -> Result<([u8; 32], usize), StorageProbeFailure> {
        use StorageProbeFailure as E;
        let identity = PageIdentity {
            database: DatabaseId::new(database).map_err(|_| E::InvalidInput)?,
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
        let buffer =
            ReadBuffer::new(reader, &mut frames, &mut load).map_err(|e| E::Buffer(e.into()))?;
        let pin = buffer
            .pin(identity, PageKind::Allocation, 0)
            .map_err(|e| E::Buffer(e.into()))?;
        let guard = pin.read().map_err(|e| E::Buffer(e.into()))?;
        let count = guard.view().map_err(|e| E::Buffer(e.into()))?.len();
        Ok((content_digest(guard.bytes()), count))
    }
}
