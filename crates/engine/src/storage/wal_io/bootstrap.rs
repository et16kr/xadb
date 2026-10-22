//! Fresh-only extent/FILEMAP installation, retaining the one WAL namespace owner.
//! No reopen, home-page mutation, transaction admission or recovery capability.

use std::sync::atomic::AtomicBool;

use super::{
    GenesisWalSession, MAX_FRAME_BYTES, WalFrameLocation, WalIoError, WalPrefix, WalSyncObservation,
};
use crate::api::Namespace;
use crate::storage::{
    allocation_page::{self, AllocationEntry, AllocationError, EXTENT_PAGES},
    control::{
        self, ChunkDescriptor, ControlError, ControlKind, ControlRef, FileMapEntry, FileState,
        MapIdentity, MapSummary, Profile, SelectedControl, SelectorRole,
    },
    page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn},
    page_io::{PageFileReader, PageIoError},
    session_fence::SessionFence,
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    identity::{Generation, ObjectId},
    io::{read_exact_at, sync, write_all_at},
    namespace::{FileName, NamespaceError, OpenMode, Selector, WriterNamespace},
};

const DESCRIPTOR_NAME: &str = "d3-bootstrap";
const SELECTOR_STAGE: &str = "d3-filemap-select-stage";
const FILE_NUMBER: u64 = 0x0000_0001_0000_0001;
const ROOT_BYTES: usize = 320;
const CHUNK_BYTES: usize = 256;
const FIXED_BYTES: u64 =
    5 * super::CONTROL_BYTES as u64 + control::EXTENT_BYTES + 2 * control::CONTAINER_BYTES;
const MAX_OWNED_BYTES: u64 = FIXED_BYTES - super::CONTROL_BYTES as u64 + super::WAL_FILE_CAP;
const PROTECTED_BYTES: u64 = FIXED_BYTES - control::EXTENT_BYTES;

#[path = "doublewrite_install.rs"]
mod doublewrite_install;
pub(crate) use doublewrite_install::{
    AllocationObservation, AllocationSession, AllocationSessionCause, AllocationSessionError,
    AllocationSessionPhase, Cause as DoublewriteInstallCause, DoublewriteInstallError,
    DoublewriteStorageSession, Phase as DoublewriteInstallPhase,
};
pub(crate) use doublewrite_install::{
    AllocationWriter, AllocatorFlushError, AllocatorFlusher, BoundAllocation, DirtyAllocationError,
    DirtyReader, FlushProgress, MutableStorageError, MutableStorageOwner,
};
#[allow(unused_imports)]
pub(crate) use doublewrite_install::{
    BeforeEffectError, BoundStatusBootstrap, BoundStatusWithFlush, InitAttemptError,
    InitializedStatusObservation, IntoFlushError, ReservedStatus, StatusBootstrapWriter,
    StatusFlushError, StatusFlusher, StatusInitError, StatusReadGuard, StatusReader,
};
pub(crate) use doublewrite_install::{
    ClosedRecovery, RecoveryAcquireError, RecoveryCause, RecoveryError, RecoveryFailure,
    RecoveryInspection, RecoveryPhase, RecoveryPrefix, RecoveryPrefixCause, RecoveryPrefixError,
    RecoveryPrefixObservation, RecoveryPrefixPhase, RecoveryProducerMode,
};
pub(in crate::storage) use doublewrite_install::{
    CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit,
};
pub(in crate::storage) use doublewrite_install::{DirtyInstallPermit, FlushFinalizePermit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Preflight,
    Role,
    Genesis,
    Prepare,
    DescriptorCreate,
    DescriptorWrite,
    DescriptorSync,
    DescriptorDirectory,
    DataCreate,
    DataWrite,
    DataSync,
    Bank0Create,
    Bank0Write,
    Bank0Sync,
    Bank1Create,
    Bank1Write,
    Bank1Sync,
    FilesDirectory,
    SelectorCreate,
    SelectorWrite,
    SelectorSync,
    SelectorPublish,
    SelectorDirectory,
    Readback,
}

#[cfg(test)]
#[path = "bootstrap_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;

#[derive(Debug)]
pub(crate) enum Cause {
    Wal(WalIoError),
    Control(ControlError),
    Allocation(AllocationError),
    PageIo(PageIoError),
    Namespace(NamespaceError),
    Io(xadb_foundation::Error),
    InvalidInput,
    UnsupportedPlan,
    Capacity,
    IdentityMismatch,
    LengthMismatch,
}

/// Genesis keeps its original effect; every later failure preserves all artifacts.
#[derive(Debug)]
pub(crate) struct BootstrapError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, BootstrapError>;

fn failed(phase: Phase, cause: Cause) -> BootstrapError {
    BootstrapError {
        phase,
        effect: Effect::CallerMustResolve,
        cause,
    }
}
fn control_error(error: ControlError) -> BootstrapError {
    failed(Phase::Prepare, Cause::Control(error))
}
fn allocation_error(error: AllocationError) -> BootstrapError {
    failed(Phase::Prepare, Cause::Allocation(error))
}
fn fixed_entry() -> std::result::Result<FileMapEntry, ControlError> {
    Ok(FileMapEntry {
        file: FileId::new(FILE_NUMBER).map_err(|_| ControlError::InvalidInput)?,
        mapped_bytes: control::EXTENT_BYTES,
        allocator: ObjectId::new(1).map_err(|_| ControlError::InvalidInput)?,
        allocator_generation: Generation::new(1).map_err(|_| ControlError::InvalidInput)?,
        state: FileState::Live,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BootstrapPlanKind {
    AllocatorOnly,
    StatusBootstrap,
}
impl BootstrapPlanKind {
    const fn version(self) -> u64 {
        match self {
            Self::AllocatorOnly => 1,
            Self::StatusBootstrap => 2,
        }
    }

    const fn issued_object_high_water(self) -> u64 {
        match self {
            Self::AllocatorOnly => 1,
            Self::StatusBootstrap => 2,
        }
    }
}

/// Hashes originate only from private preparation of the actual profile/geometry.
/// No arbitrary caller-supplied digest or durable resume authority is admitted.
struct BootstrapPlan {
    kind: BootstrapPlanKind,
    profile: Profile,
    selected: SelectedControl,
    allocation_digest: [u8; 32],
}

/// Pure expected initial bytes, with no durable/session/replay authority.
pub(in crate::storage) struct InitialRecoveryExpectation {
    pub(in crate::storage) plan: u64,
    pub(in crate::storage) filemap: SelectedControl,
    pub(in crate::storage) doublewrite: SelectedControl,
    pub(in crate::storage) stage_digest: [u8; 32],
}

#[derive(Debug)]
pub(in crate::storage) enum InitialRecoveryError {
    Bootstrap(BootstrapError),
    Doublewrite(DoublewriteInstallCause),
}
impl From<BootstrapError> for InitialRecoveryError {
    fn from(error: BootstrapError) -> Self {
        Self::Bootstrap(error)
    }
}

/// The caller has read exact XBS1 bytes into scratch[0][..4096]. The
/// authenticated digest is retained before preparation reuses all three pages.
/// This shares the producer's canonical encoders; it never inspects home bytes.
pub(in crate::storage) fn inspect_initial_bootstrap(
    profile: Profile,
    selected: SelectedControl,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> std::result::Result<InitialRecoveryExpectation, InitialRecoveryError> {
    let plan = prepare_recovery_bootstrap(profile, selected, scratch)?;
    let (doublewrite, stage_digest) =
        doublewrite_install::initial_recovery_expectation(&plan, &mut scratch[0])
            .map_err(InitialRecoveryError::Doublewrite)?;
    Ok(InitialRecoveryExpectation {
        plan: plan.kind.version(),
        filemap: plan.selected,
        doublewrite,
        stage_digest,
    })
}

/// Canonical genesis bytes only. Keeps the authenticated LSN0 allocator in
/// scratch[0]; no installed session, live role or recovery reservation is made.
fn prepare_recovery_bootstrap(
    profile: Profile,
    selected: SelectedControl,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> Result<BootstrapPlan> {
    let bytes = &scratch[0][..super::CONTROL_BYTES];
    super::check_control(bytes, profile, b"XBS1").map_err(control_error)?;
    let kind = match super::u64_at(bytes, 280) {
        1 => BootstrapPlanKind::AllocatorOnly,
        2 => BootstrapPlanKind::StatusBootstrap,
        _ => return Err(failed(Phase::Prepare, Cause::UnsupportedPlan)),
    };
    let actual_digest = content_digest(bytes);
    let plan = BootstrapPlan::prepare_for_kind(profile, scratch, kind)?;
    if selected != plan.selected {
        return Err(failed(Phase::Prepare, Cause::IdentityMismatch));
    }
    let expected = plan
        .encode_descriptor(&mut scratch[2])
        .map_err(control_error)?;
    if actual_digest != content_digest(expected) {
        return Err(control_error(ControlError::Corrupt));
    }
    Ok(plan)
}

/// Conservative source-owned nested join; caller page storage is separate.
pub(in crate::storage) fn recovery_preparation_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    let bootstrap =
        source_budget::<W>().prepare_nested + size_of::<[AllocationEntry; EXTENT_PAGES]>() + 1032;
    // prepare_recovery_bootstrap -> prepare_for_kind replaces the old
    // prepare -> prepare_for_kind result-wrapper depth. The existing two Plan
    // values and six largest Result carriers cover that same simultaneous set.
    // Descriptor re-encoding follows prepare_for_kind's return; DWB preparation
    // follows prepare_recovery_bootstrap's return, so those phases do not stack.
    bootstrap.max(doublewrite_install::recovery_preparation_bytes() + size_of::<BootstrapPlan>())
        + 2 * size_of::<InitialRecoveryExpectation>()
}

pub(in crate::storage) fn recovery_reader_bytes<W: WriterNamespace>() -> usize {
    source_budget::<W>().readback_nested
}
impl BootstrapPlan {
    fn prepare(profile: Profile, scratch: &mut [[u8; PAGE_BYTES]; 3]) -> Result<Self> {
        Self::prepare_for_kind(profile, scratch, BootstrapPlanKind::AllocatorOnly)
    }

    fn prepare_for_kind(
        profile: Profile,
        scratch: &mut [[u8; PAGE_BYTES]; 3],
        kind: BootstrapPlanKind,
    ) -> Result<Self> {
        super::genesis_profile(profile).map_err(control_error)?;
        if MAX_OWNED_BYTES > profile.owned_bytes() || PROTECTED_BYTES > 1 << 30 {
            return Err(failed(Phase::Prepare, Cause::Capacity));
        }
        let entry = fixed_entry().map_err(control_error)?;
        let identity = entry
            .metadata_identity(profile.spec().namespace, 0)
            .map_err(control_error)?;
        let [page, controls, final_controls] = scratch;
        {
            let mut entries = [AllocationEntry::Free(entry.allocator_generation); EXTENT_PAGES];
            entries[0] = AllocationEntry::Allocated {
                generation: entry.allocator_generation,
                object: entry.allocator,
                object_generation: entry.allocator_generation,
                kind: PageKind::Allocation,
            };
            let header = PageHeader {
                identity,
                kind: PageKind::Allocation,
                level: 0,
                lsn: PageLsn(0),
            };
            let length = allocation_page::encode_allocation_cell(
                header,
                profile.spec().namespace,
                &entries,
                controls,
            )
            .map_err(allocation_error)?;
            allocation_page::encode_allocation_page(
                header,
                profile.spec().namespace,
                &controls[..length],
                page,
            )
            .map_err(allocation_error)?;
        }
        let allocation_digest = content_digest(page);
        // The cell workspace is no longer live; retain only exact root/chunk bytes.
        let (root_bytes, chunk_bytes) = controls.split_at_mut(ROOT_BYTES);
        let map = MapIdentity {
            namespace: profile.spec().namespace,
            generation: 1,
            slot: 0,
        };
        let chunk = control::encode_filemap_chunk(
            map,
            0,
            &[entry],
            profile,
            &mut chunk_bytes[..CHUNK_BYTES],
        )
        .map_err(control_error)?;
        let descriptor = ChunkDescriptor {
            reference: ControlRef {
                kind: ControlKind::FileMapChunk,
                slot: 0,
                index: 0,
                generation: 1,
                encoded_len: CHUNK_BYTES as u32,
                digest: content_digest(chunk),
            },
            count: 1,
            first: entry.file,
            last: entry.file,
        };
        let summary = MapSummary {
            files: 1,
            mapped_bytes: control::EXTENT_BYTES,
            first: Some(entry.file),
            last: Some(entry.file),
        };
        let root = control::encode_filemap_root(map, summary, &[descriptor], profile, root_bytes)
            .map_err(control_error)?;
        let selected = SelectedControl {
            generation: 1,
            reference: ControlRef {
                kind: ControlKind::FileMapRoot,
                slot: 0,
                index: 0,
                generation: 1,
                encoded_len: ROOT_BYTES as u32,
                digest: content_digest(root),
            },
        };
        let plan = Self {
            kind,
            profile,
            selected,
            allocation_digest,
        };
        let (descriptor_bytes, selector_bytes) = final_controls.split_at_mut(super::CONTROL_BYTES);
        plan.encode_descriptor(descriptor_bytes)
            .map_err(control_error)?;
        control::encode_selector(
            SelectorRole::FileMap,
            selected,
            profile,
            &mut selector_bytes[..super::CONTROL_BYTES],
        )
        .map_err(control_error)?;
        Ok(plan)
    }

    fn allocation_identity(&self) -> std::result::Result<PageIdentity, ControlError> {
        fixed_entry()?.metadata_identity(self.profile.spec().namespace, 0)
    }

    fn encode_descriptor<'a>(
        &self,
        output: &'a mut [u8],
    ) -> std::result::Result<&'a [u8], ControlError> {
        super::genesis_profile(self.profile)?;
        let reference = self.selected.reference;
        if self.selected.generation != 1
            || reference.kind != ControlKind::FileMapRoot
            || reference.slot != 0
            || reference.index != 0
            || reference.generation != 1
            || reference.encoded_len != ROOT_BYTES as u32
        {
            return Err(ControlError::InvalidInput);
        }
        let mut encoded_ref = [0; control::REF_BYTES];
        reference.encode(&mut encoded_ref)?;
        let bytes = super::control_prefix(output, self.profile, b"XBS1")?;
        super::put64(bytes, 40, 1);
        bytes[48..80].copy_from_slice(&self.profile.digest());
        for (offset, value) in [
            (80, FILE_NUMBER),
            (88, control::EXTENT_BYTES),
            (96, 1),
            (104, 1),
            (112, 1),
            (120, self.kind.issued_object_high_water()),
            (128, 1),
            (136, 1),
            (240, FIXED_BYTES),
            (248, MAX_OWNED_BYTES),
            (256, PROTECTED_BYTES),
            (264, 2 * control::CONTAINER_BYTES),
            (272, 1),
            (280, self.kind.version()),
        ] {
            super::put64(bytes, offset, value);
        }
        bytes[144..208].copy_from_slice(&encoded_ref);
        bytes[208..240].copy_from_slice(&self.allocation_digest);
        super::finish_control(bytes);
        Ok(bytes)
    }

    fn decode_descriptor(&self, bytes: &[u8]) -> std::result::Result<(), ControlError> {
        if bytes.len() != super::CONTROL_BYTES
            || &bytes[..4] != b"XBS1"
            || super::u32_at(bytes, 8) != super::CONTROL_BYTES as u32
            || bytes[4064..] != content_digest(&bytes[..4064])
        {
            return Err(ControlError::Corrupt);
        }
        if bytes[4..6] != 1u16.to_le_bytes()
            || bytes[6] != 1
            || super::u64_at(bytes, 280) != self.kind.version()
        {
            return Err(ControlError::UnsupportedFormat);
        }
        for range in [7..8, 12..16, 146..148, 164..176, 288..4064] {
            if bytes[range].iter().any(|&byte| byte != 0) {
                return Err(ControlError::Corrupt);
            }
        }
        for (offset, value) in [
            (80, FILE_NUMBER),
            (88, control::EXTENT_BYTES),
            (96, 1),
            (104, 1),
            (112, 1),
            (120, self.kind.issued_object_high_water()),
            (128, 1),
            (136, 1),
            (152, 1),
            (240, FIXED_BYTES),
            (248, MAX_OWNED_BYTES),
            (256, PROTECTED_BYTES),
            (264, 2 * control::CONTAINER_BYTES),
            (272, 1),
        ] {
            if super::u64_at(bytes, offset) != value {
                return Err(ControlError::Corrupt);
            }
        }
        if bytes[144] != ControlKind::FileMapRoot as u8
            || bytes[145] != 0
            || super::u32_at(bytes, 148) != 0
            || super::u32_at(bytes, 160) != ROOT_BYTES as u32
        {
            return Err(ControlError::Corrupt);
        }
        if bytes[16..32] != self.profile.spec().namespace.database.bytes()
            || super::u64_at(bytes, 32) != self.profile.spec().namespace.timeline
            || super::u64_at(bytes, 40) != self.profile.spec().generation
            || bytes[48..80] != self.profile.digest()
            || bytes[176..208] != self.selected.reference.digest
            || bytes[208..240] != self.allocation_digest
        {
            return Err(ControlError::IdentityMismatch);
        }
        Ok(())
    }
}

/// All names and codec bytes are prepared before descriptor create. Streaming
/// subsequently reuses one caller page; no future bytes are regenerated by codecs.
struct InstallNames {
    descriptor: FileName,
    data: FileName,
    banks: [FileName; 2],
    stage: FileName,
}
impl InstallNames {
    fn new(plan: &BootstrapPlan) -> Result<Self> {
        let name =
            |value| FileName::new(value).map_err(|e| failed(Phase::Prepare, Cause::Namespace(e)));
        let mut other_bank = plan.selected.reference;
        other_bank.slot = 1;
        Ok(Self {
            descriptor: name(DESCRIPTOR_NAME)?,
            data: control::data_file_name(fixed_entry().map_err(control_error)?.file),
            banks: [
                control::control_file_name(plan.selected.reference).map_err(control_error)?,
                control::control_file_name(other_bank).map_err(control_error)?,
            ],
            stage: name(SELECTOR_STAGE)?,
        })
    }
}

fn write_control<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    bytes: &[u8],
    phases: [Phase; 3],
) -> Result<()> {
    let mut file = writer
        .create_new(name)
        .map_err(|e| failed(phases[0], Cause::Namespace(e)))?;
    write_all_at(&mut file, bytes, 0).map_err(|e| failed(phases[1], Cause::Io(e)))?;
    sync(&mut file).map_err(|e| failed(phases[2], Cause::Io(e)))
}

fn install<W: WriterNamespace>(
    writer: &mut W,
    names: &InstallNames,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> Result<()> {
    let [page, controls, final_controls] = scratch;
    write_control(
        writer,
        &names.descriptor,
        &final_controls[..super::CONTROL_BYTES],
        [
            Phase::DescriptorCreate,
            Phase::DescriptorWrite,
            Phase::DescriptorSync,
        ],
    )?;
    writer
        .sync_directory()
        .map_err(|e| failed(Phase::DescriptorDirectory, Cause::Namespace(e)))?;
    {
        let mut file = writer
            .create_new(&names.data)
            .map_err(|e| failed(Phase::DataCreate, Cause::Namespace(e)))?;
        write_all_at(&mut file, page, 0).map_err(|e| failed(Phase::DataWrite, Cause::Io(e)))?;
        page.fill(0);
        for index in 1..EXTENT_PAGES {
            write_all_at(&mut file, page, (index * PAGE_BYTES) as u64)
                .map_err(|e| failed(Phase::DataWrite, Cause::Io(e)))?;
        }
        sync(&mut file).map_err(|e| failed(Phase::DataSync, Cause::Io(e)))?;
    }
    for (bank, phases) in [
        [Phase::Bank0Create, Phase::Bank0Write, Phase::Bank0Sync],
        [Phase::Bank1Create, Phase::Bank1Write, Phase::Bank1Sync],
    ]
    .into_iter()
    .enumerate()
    {
        let mut file = writer
            .create_new(&names.banks[bank])
            .map_err(|e| failed(phases[0], Cause::Namespace(e)))?;
        for index in 0..=control::MAX_CHUNKS {
            if bank == 0 && index == 0 {
                page[..ROOT_BYTES].copy_from_slice(&controls[..ROOT_BYTES]);
            } else if bank == 0 && index == 1 {
                page[..CHUNK_BYTES]
                    .copy_from_slice(&controls[ROOT_BYTES..ROOT_BYTES + CHUNK_BYTES]);
            }
            write_all_at(&mut file, page, (index * PAGE_BYTES) as u64)
                .map_err(|e| failed(phases[1], Cause::Io(e)))?;
            if bank == 0 && index < 2 {
                page.fill(0);
            }
        }
        sync(&mut file).map_err(|e| failed(phases[2], Cause::Io(e)))?;
    }
    writer
        .sync_directory()
        .map_err(|e| failed(Phase::FilesDirectory, Cause::Namespace(e)))?;
    write_control(
        writer,
        &names.stage,
        &final_controls[super::CONTROL_BYTES..2 * super::CONTROL_BYTES],
        [
            Phase::SelectorCreate,
            Phase::SelectorWrite,
            Phase::SelectorSync,
        ],
    )?;
    writer
        .publish_selector(&names.stage, Selector::FileMap)
        .map_err(|e| failed(Phase::SelectorPublish, Cause::Namespace(e)))?;
    writer
        .sync_directory()
        .map_err(|e| failed(Phase::SelectorDirectory, Cause::Namespace(e)))
}

fn open_bound<W: WriterNamespace>(writer: &mut W, name: &FileName, length: u64) -> Result<W::File> {
    let (file, metadata) = writer
        .open_existing_with_metadata(name, OpenMode::ReadOnly)
        .map_err(|e| failed(Phase::Readback, Cause::Namespace(e)))?;
    if metadata.length != length {
        return Err(failed(Phase::Readback, Cause::LengthMismatch));
    }
    let named = writer
        .metadata(name)
        .map_err(|e| failed(Phase::Readback, Cause::Namespace(e)))?;
    if named != Some(metadata) {
        return Err(failed(Phase::Readback, Cause::IdentityMismatch));
    }
    Ok(file)
}

fn readback<W: WriterNamespace>(
    writer: &mut W,
    plan: &BootstrapPlan,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> Result<()> {
    let [root, reader_scratch, output] = scratch;
    let name =
        |value| FileName::new(value).map_err(|e| failed(Phase::Readback, Cause::Namespace(e)));
    {
        let mut file = open_bound(writer, &name(DESCRIPTOR_NAME)?, super::CONTROL_BYTES as u64)?;
        read_exact_at(&mut file, &mut output[..super::CONTROL_BYTES], 0)
            .map_err(|e| failed(Phase::Readback, Cause::Io(e)))?;
        plan.decode_descriptor(&output[..super::CONTROL_BYTES])
            .map_err(|e| failed(Phase::Readback, Cause::Control(e)))?;
    }
    // The unselected bank has length authority only; its bytes are no fallback.
    drop(open_bound(
        writer,
        &name("d3-filemap-1")?,
        control::CONTAINER_BYTES,
    )?);
    let mut reader = PageFileReader::open(
        writer,
        Some(plan.profile.spec().namespace),
        root,
        reader_scratch,
    )
    .map_err(|e| failed(Phase::Readback, Cause::PageIo(e)))?;
    if reader.profile() != plan.profile || reader.selected() != plan.selected {
        return Err(failed(Phase::Readback, Cause::IdentityMismatch));
    }
    let identity = plan
        .allocation_identity()
        .map_err(|e| failed(Phase::Readback, Cause::Control(e)))?;
    reader
        .read_page(identity, PageKind::Allocation, 0, output)
        .map_err(|e| failed(Phase::Readback, Cause::PageIo(e)))?;
    if content_digest(output) != plan.allocation_digest {
        return Err(failed(Phase::Readback, Cause::IdentityMismatch));
    }
    allocation_page::decode_allocation_page(output, identity, plan.profile.spec().namespace)
        .map_err(|e| failed(Phase::Readback, Cause::Allocation(e)))?;
    Ok(())
}

/// An immutable system-object role issued only by a completed plan2 bootstrap.
/// This borrow cannot initialize payload pages or grant transaction admission.
/// Identity getters remain observations after fencing; future mutation owners
/// must recheck health and the exact originating plan/fence.
pub(crate) struct StatusRole<'s> {
    plan: &'s BootstrapPlan,
    fence: &'s SessionFence,
}
impl StatusRole<'_> {
    pub(crate) fn namespace(&self) -> Namespace {
        self.plan.profile.spec().namespace
    }

    pub(crate) fn object(&self) -> ObjectId {
        ObjectId::new(2).expect("fixed nonzero status object")
    }

    pub(crate) fn generation(&self) -> Generation {
        Generation::new(1).expect("fixed nonzero status generation")
    }

    pub(crate) fn kind(&self) -> PageKind {
        PageKind::TransactionStatus
    }

    pub(crate) fn level(&self) -> u8 {
        0
    }

    pub(crate) fn issued_object_high_water(&self) -> u64 {
        self.plan.kind.issued_object_high_water()
    }

    pub(crate) fn uses_fence(&self, fence: &SessionFence) -> bool {
        std::ptr::eq(self.fence, fence)
    }
}

/// Immutable namespace capability plus one physical WAL slot. Returned readers
/// borrow this owner and all caller scratch; there is no raw writer escape hatch.
pub(crate) struct FreshStorageSession<'m, W: WriterNamespace> {
    wal: GenesisWalSession<'m, W>,
    plan: BootstrapPlan,
    reader_active: AtomicBool,
}

// Source-owned live values, not ABI stack. Counts include copies/return scratch
// across prepare→control/page codecs and readback→reader→cursor→decoder. The
// 64-entry preparation array and non-Copy owner/File values are charged outside
// nested locals; caller page buffers are never copied into this allowance.
struct SourceBudget {
    prepare_nested: usize,
    readback_nested: usize,
    fresh_role: usize,
    mutable_role: usize,
}
fn source_budget<W: WriterNamespace>() -> SourceBudget {
    use crate::storage::{
        allocation_page::{AllocationPage, ExtentAddress},
        control::{ChunkLocation, FileMapChunk, FileMapCursor, FileMapRoot, ProfileSpec},
        page::PageView,
    };
    use std::mem::size_of;
    use xadb_foundation::{codec::ContentHasher, namespace::FileMetadata};

    type RoleResult = std::result::Result<StatusRole<'static>, BootstrapError>;
    type MutableRoleResult = std::result::Result<StatusRole<'static>, MutableStorageError>;
    let result_carrier = [
        size_of::<BootstrapError>(),
        size_of::<Result<BootstrapPlan>>(),
        size_of::<std::result::Result<FileMapRoot<'_>, ControlError>>(),
        size_of::<std::result::Result<FileMapChunk<'_>, ControlError>>(),
        size_of::<std::result::Result<ChunkDescriptor, ControlError>>(),
        size_of::<std::result::Result<PageView<'_>, PageIoError>>(),
        size_of::<std::result::Result<AllocationPage<'_>, AllocationError>>(),
    ]
    .into_iter()
    .fold(0, usize::max);
    // Same pinned SHA-256 source allowance as allocation_budget::HASH.
    let hash = 4 * size_of::<ContentHasher>() + 1536;
    let common = 2 * size_of::<BootstrapPlan>()
        + 12 * size_of::<Profile>()
        + 8 * size_of::<ProfileSpec>()
        + 8 * size_of::<Namespace>()
        + 8 * size_of::<MapIdentity>()
        + 6 * size_of::<MapSummary>()
        + 8 * size_of::<FileMapEntry>()
        + 6 * size_of::<ChunkDescriptor>()
        + 8 * size_of::<ControlRef>()
        + 4 * size_of::<SelectedControl>()
        + 6 * size_of::<PageHeader>()
        + 6 * size_of::<PageIdentity>()
        + 8 * size_of::<AllocationEntry>()
        + 4 * size_of::<ExtentAddress>()
        + 4 * size_of::<PageView<'_>>()
        + 2 * size_of::<AllocationPage<'_>>()
        + 1024 // Fixed codec byte arrays/field tables; no page image.
        + 1536 // Bounded refs/scalars/iterators/private envelope and enum tags.
        + 6 * result_carrier
        + hash;
    // Account for the enclosing reader's padding as well as its non-File fields.
    let reader_control = size_of::<PageFileReader<'_, W>>().saturating_sub(size_of::<W::File>());
    let fresh_role = 2 * size_of::<StatusRole<'_>>()
        + 3 * size_of::<RoleResult>()
        + size_of::<Option<super::Cause>>()
        + size_of::<WalIoError>()
        + size_of::<BootstrapError>()
        + 16 * size_of::<usize>();
    SourceBudget {
        prepare_nested: common + 672,
        readback_nested: common
            + 160
            + 4 * size_of::<SelectedControl>()
            + 4 * size_of::<FileMapRoot<'_>>()
            + 2 * size_of::<FileMapCursor<'_>>()
            + 3 * size_of::<FileMapChunk<'_>>()
            + 4 * size_of::<ChunkLocation>()
            + 2 * size_of::<MapSummary>()
            + 4 * size_of::<FileName>()
            + 4 * size_of::<FileMetadata>()
            + reader_control,
        fresh_role,
        mutable_role: fresh_role
            + 2 * size_of::<MutableStorageError>()
            + 2 * size_of::<MutableRoleResult>()
            + size_of::<BootstrapError>()
            + 12 * size_of::<usize>(),
    }
}

fn source_budget_fits<W: WriterNamespace>() -> bool {
    use std::mem::size_of;
    let owner = size_of::<FreshStorageSession<'_, W>>();
    let file = size_of::<W::File>();
    if owner > 16384 || file > 16384 {
        return false;
    }
    let budget = source_budget::<W>();
    size_of::<BootstrapPlanKind>() <= 8
        && size_of::<StatusRole<'_>>() <= 32
        && size_of::<[AllocationEntry; EXTENT_PAGES]>() <= 8192
        && budget.prepare_nested <= 16384
        && budget.readback_nested <= 16384
        && budget.fresh_role <= 1024
        && budget.mutable_role <= 2048
        && owner + size_of::<[AllocationEntry; EXTENT_PAGES]>() + 1032 + budget.prepare_nested
            <= 65536
        && owner + file + 1032 + budget.prepare_nested <= 65536
        && owner + 2 * file + budget.readback_nested <= 65536
}

impl<'m, W: WriterNamespace> FreshStorageSession<'m, W> {
    pub(crate) fn create(
        writer: W,
        profile_id: u32,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> Result<Self> {
        Self::create_for_kind(
            writer,
            profile_id,
            slot,
            scratch,
            BootstrapPlanKind::AllocatorOnly,
        )
    }

    /// Install immutable Status role2/gen1 in a fresh namespace's plan2.
    /// A successful result follows the complete install and descriptor readback;
    /// failures preserve the existing cumulative artifact/effect contract.
    pub(crate) fn create_status(
        writer: W,
        profile_id: u32,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> Result<Self> {
        Self::create_for_kind(
            writer,
            profile_id,
            slot,
            scratch,
            BootstrapPlanKind::StatusBootstrap,
        )
    }

    fn create_for_kind(
        writer: W,
        profile_id: u32,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [[u8; PAGE_BYTES]; 3],
        kind: BootstrapPlanKind,
    ) -> Result<Self> {
        if !(1..=3).contains(&profile_id) || !source_budget_fits::<W>() {
            return Err(BootstrapError {
                phase: Phase::Preflight,
                effect: Effect::Unchanged,
                cause: Cause::InvalidInput,
            });
        }
        let mut wal = GenesisWalSession::create(writer, profile_id, slot, &mut scratch[0])
            .map_err(|error| BootstrapError {
                phase: Phase::Genesis,
                effect: error.effect,
                cause: Cause::Wal(error),
            })?;
        let plan = match kind {
            BootstrapPlanKind::AllocatorOnly => BootstrapPlan::prepare(wal.profile, scratch)?,
            BootstrapPlanKind::StatusBootstrap => {
                BootstrapPlan::prepare_for_kind(wal.profile, scratch, kind)?
            }
        };
        {
            let writer = wal._writer.get_mut().map_err(|_| {
                failed(
                    Phase::Prepare,
                    Cause::Wal(super::attempted(
                        super::Phase::Observe,
                        super::Cause::Poisoned,
                    )),
                )
            })?;
            {
                let names = InstallNames::new(&plan)?;
                install(writer, &names, scratch)?;
            }
            readback(writer, &plan, scratch)?;
        }
        Ok(Self {
            wal,
            plan,
            reader_active: AtomicBool::new(false),
        })
    }

    /// Borrow the completed plan2's immutable role. No I/O or payload authority.
    /// Poison/fence precedes plan rejection and never clears the shared fence.
    pub(crate) fn status_role(&self) -> Result<StatusRole<'_>> {
        let cause = if self.wal.append.is_poisoned()
            || self.wal.file.is_poisoned()
            || self.wal._writer.is_poisoned()
        {
            Some(super::Cause::Poisoned)
        } else if self.wal.fenced.is_fenced() {
            Some(super::Cause::Fenced)
        } else {
            None
        };
        if let Some(cause) = cause {
            self.wal.fenced.fence();
            self.wal.completion.notify_all();
            return Err(failed(
                Phase::Role,
                Cause::Wal(super::attempted(super::Phase::Observe, cause)),
            ));
        }
        if self.plan.kind != BootstrapPlanKind::StatusBootstrap {
            return Err(BootstrapError {
                phase: Phase::Role,
                effect: Effect::Unchanged,
                cause: Cause::UnsupportedPlan,
            });
        }
        Ok(StatusRole {
            plan: &self.plan,
            fence: &self.wal.fenced,
        })
    }

    pub(crate) fn open_reader<'a>(
        &'a self,
        root: &'a mut [u8; PAGE_BYTES],
        scratch: &'a mut [u8; PAGE_BYTES],
    ) -> std::result::Result<PageFileReader<'a, W>, PageIoError> {
        let _unwind = self.wal.fenced.unwind_guard();
        if self.wal.is_fenced() {
            return Err(PageIoError::Fenced);
        }
        PageFileReader::open_shared(
            &self.wal._writer,
            &self.wal.fenced,
            &self.reader_active,
            self.plan.profile,
            self.plan.selected,
            root,
            scratch,
        )
    }

    pub(crate) fn try_stage(
        &self,
        bytes: &[u8],
    ) -> std::result::Result<WalFrameLocation, WalIoError> {
        let file = fixed_entry()
            .map_err(|e| super::unchanged(super::Phase::Stage, super::Cause::Control(e)))?
            .file;
        #[cfg(test)]
        {
            self.wal
                .stage_inner(bytes, Some((file, EXTENT_PAGES as u64)), || {})
        }
        #[cfg(not(test))]
        {
            self.wal
                .stage_inner(bytes, Some((file, EXTENT_PAGES as u64)))
        }
    }

    pub(crate) fn sync_pending(&self) -> std::result::Result<WalSyncObservation, WalIoError> {
        self.wal.sync_pending()
    }

    pub(crate) fn prefix(&self) -> std::result::Result<WalPrefix, WalIoError> {
        self.wal.prefix()
    }

    pub(crate) fn is_fenced(&self) -> bool {
        self.wal.is_fenced()
    }
}
