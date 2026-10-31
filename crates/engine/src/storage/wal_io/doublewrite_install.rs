//! Consuming installation of the first Empty DWB selection into pristine storage.
//! The retained owner exposes clean reads only. No batch, home write or recovery.
//! See devdoc/contracts/dev3-doublewrite-install.md (D3-DWB-INSTALL-001).

use super::{BootstrapPlan, FreshStorageSession};
use crate::storage::{
    control::{
        self, ControlError, ControlKind, ControlRef, Profile, SelectedControl, SelectorRole,
    },
    doublewrite_control::{self, ControlHeader, ControlState, DoublewriteError},
    page::{PAGE_BYTES, PageLsn},
    page_io::{PageFileReader, PageIoError},
};
use foundation::{
    codec::content_digest,
    error::Effect,
    io::{read_exact_at, sync, write_all_at},
    namespace::{FileName, NamespaceError, OpenMode, Selector, WriterNamespace},
};
use std::sync::atomic::Ordering;

#[path = "allocation_reservation.rs"]
mod allocation_reservation;
pub(crate) use allocation_reservation::{
    AllocationObservation, AllocationSession, AllocationSessionError,
    Cause as AllocationSessionCause, Phase as AllocationSessionPhase,
};

#[path = "mutable_storage.rs"]
pub(in crate::storage::wal_io) mod mutable_storage;
#[cfg(test)]
pub(in crate::storage) use allocation_reservation::operation::SemanticError as Plan4TestSemanticError;
pub(in crate::storage) use mutable_storage::CatalogRetirementRejected;
#[cfg(test)]
pub(in crate::storage) use mutable_storage::Cause as Plan4TestMutableCause;
pub(in crate::storage) use mutable_storage::Plan4LoadPermit;
pub(crate) use mutable_storage::{
    AllocationWriter, AllocatorFlushError, AllocatorFlusher, BoundAllocation, DirtyAllocationError,
    DirtyReader, FlushProgress, MutableStorageError, MutableStorageOwner,
};
#[allow(unused_imports)]
pub(crate) use mutable_storage::{
    BeforeEffectError, BoundStatusBootstrap, BoundStatusWithFlush, EMPTY_TRANSACTION_BANK_BYTES,
    EmptyTransactionSnapshot, EmptyTransactions, InitAttemptError, InitializedStatusObservation,
    IntoEmptyTransactionsError, IntoFlushError, IntoOperationalStatusError, OperationalStatus,
    OperationalStatusReader, OperationalStatusWriter, ReadStatusSet, ReservedStatus,
    ResidentReadAdmission, ResidentStatus, ResidentStatusBinding, ResidentStatusReader,
    StatusBootstrapWriter, StatusFlushError, StatusFlusher, StatusInitError, StatusReadGuard,
    StatusReader,
};
pub(crate) use mutable_storage::{
    ClosedRecovery, RecoveryAcquireError, RecoveryCause, RecoveryError, RecoveryFailure,
    RecoveryInspection, RecoveryPhase, RecoveryPrefix, RecoveryPrefixCause, RecoveryPrefixError,
    RecoveryPrefixObservation, RecoveryPrefixPhase, RecoveryProducerMode,
};
pub(in crate::storage) use mutable_storage::{
    CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit,
};
pub(in crate::storage) use mutable_storage::{
    DirtyInstallPermit, FlushFinalizePermit, ReloadInstallPermit,
};
pub(crate) use mutable_storage::{
    IntoOperationalError, OperationalAllocation, ReloadError, ReloadObservation,
};
#[allow(unused_imports)]
pub(crate) use mutable_storage::{
    Plan4CatalogLookup, Plan4GenesisCensus, Plan4LoadObservation, Plan4Page, Plan4PageCause,
    Plan4PageError, Plan4PagePhase, Plan4Pages, Plan4ResidencyCursor, Plan4Resident, Plan4Target,
};

const WORKSPACE_BYTES: usize = 4 << 20;
const EMPTY_BYTES: usize = 160;
const RESERVED_BYTES: usize = 8192;
const IMAGE_BYTES: usize = doublewrite_control::POOL_BYTES - 4 * PAGE_BYTES;
const IMAGE_SLOT_BYTES: usize = doublewrite_control::POOL_SLOTS * doublewrite_control::SLOT_BYTES;
const FIXED_BYTES: u64 = super::FIXED_BYTES + doublewrite_control::POOL_BYTES as u64;
const MAX_OWNED_BYTES: u64 = super::MAX_OWNED_BYTES + doublewrite_control::POOL_BYTES as u64;
const PROTECTED_BYTES: u64 = super::PROTECTED_BYTES + doublewrite_control::POOL_BYTES as u64;

#[cfg(test)]
#[path = "doublewrite_install/tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    StatePreflight,
    NamespacePreflight,
    Prepare,
    DescriptorCreate,
    DescriptorWrite,
    DescriptorSync,
    DescriptorDirectory,
    ImagesCreate,
    ImagesWrite,
    ImagesSync,
    Bank0Create,
    Bank0Write,
    Bank0Sync,
    Bank1Create,
    Bank1Write,
    Bank1Sync,
    ReservedCreate,
    ReservedWrite,
    ReservedSync,
    FilesDirectory,
    SelectorCreate,
    SelectorWrite,
    SelectorSync,
    SelectorPublish,
    SelectorDirectory,
    Readback,
}

#[derive(Debug)]
pub(crate) enum Cause {
    InvalidState,
    Busy,
    Fenced,
    Poisoned,
    Capacity,
    Residue,
    Control(ControlError),
    Doublewrite(DoublewriteError),
    Namespace(NamespaceError),
    Io(foundation::Error),
    ContentMismatch,
    SelectionMismatch,
    IdentityMismatch,
    LengthMismatch,
    Invariant,
}

/// Consumed storage must be resolved by its caller even for a zero-effect reject.
/// Counters cover logical mutations, never partial-I/O retry subcalls or reads.
#[derive(Debug)]
pub(crate) struct DoublewriteInstallError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
    pub(crate) attempted: u8,
    pub(crate) completed: u8,
}
type Result<T> = std::result::Result<T, DoublewriteInstallError>;

#[derive(Default)]
struct Effects {
    attempted: u8,
    completed: u8,
}
impl Effects {
    fn error(&self, phase: Phase, cause: Cause) -> DoublewriteInstallError {
        DoublewriteInstallError {
            phase,
            effect: Effect::CallerMustResolve,
            cause,
            attempted: self.attempted,
            completed: self.completed,
        }
    }

    // Private synchronous closure: no caller callback or publication lock.
    fn run<T>(
        &mut self,
        phase: Phase,
        operation: impl FnOnce() -> std::result::Result<T, Cause>,
    ) -> Result<T> {
        self.attempted += 1;
        let value = operation().map_err(|cause| self.error(phase, cause))?;
        self.completed += 1;
        Ok(value)
    }
}

struct Names {
    selected: FileName,
    descriptor: FileName,
    images: FileName,
    bank0: FileName,
    bank1: FileName,
    reserved: FileName,
    selector_stage: FileName,
    profile: FileName,
}
impl Names {
    fn new() -> std::result::Result<Self, Cause> {
        let name = |value| FileName::new(value).map_err(Cause::Namespace);
        Ok(Self {
            selected: name("DWB")?,
            descriptor: name("d3-dwb-stage")?,
            images: name("d3-dwb-images")?,
            bank0: name("d3-dwb-0")?,
            bank1: name("d3-dwb-1")?,
            reserved: name("d3-dwb-reserved")?,
            selector_stage: name("d3-dwb-select-stage")?,
            profile: name("d3-profile")?,
        })
    }

    fn preflight<W: WriterNamespace>(&self, writer: &W) -> std::result::Result<(), Cause> {
        for name in [
            &self.selected,
            &self.descriptor,
            &self.images,
            &self.bank0,
            &self.bank1,
            &self.reserved,
            &self.selector_stage,
        ] {
            if writer.metadata(name).map_err(Cause::Namespace)?.is_some() {
                return Err(Cause::Residue);
            }
        }
        Ok(())
    }
}

/// Constructed only from the consumed owner's actual immutable bootstrap plan.
struct InstallPlan {
    profile: Profile,
    filemap: SelectedControl,
    selected: SelectedControl,
    descriptor_digest: [u8; 32],
    selector_digest: [u8; 32],
}
impl InstallPlan {
    fn prepare(
        bootstrap: &BootstrapPlan,
        workspace: &mut [u8; WORKSPACE_BYTES],
    ) -> std::result::Result<Self, Cause> {
        Self::prepare_inner(bootstrap, workspace)
    }

    // The pure preparation uses one page, independently of the installation's
    // larger zero-stream workspace. Keep one encoder for both consumers.
    fn prepare_inner(
        bootstrap: &BootstrapPlan,
        workspace: &mut [u8],
    ) -> std::result::Result<Self, Cause> {
        if workspace.len() < PAGE_BYTES {
            return Err(Cause::Capacity);
        }
        if MAX_OWNED_BYTES > bootstrap.profile.owned_bytes() || PROTECTED_BYTES > 1 << 30 {
            return Err(Cause::Capacity);
        }
        let empty = encode_empty(bootstrap.profile, &mut workspace[..EMPTY_BYTES])?;
        let selected = SelectedControl {
            generation: 1,
            reference: ControlRef {
                kind: ControlKind::Doublewrite,
                slot: 0,
                index: 0,
                generation: 1,
                encoded_len: EMPTY_BYTES as u32,
                digest: content_digest(empty),
            },
        };
        let selector = control::encode_selector(
            SelectorRole::Doublewrite,
            selected,
            bootstrap.profile,
            workspace,
        )
        .map_err(Cause::Control)?;
        let mut plan = Self {
            profile: bootstrap.profile,
            filemap: bootstrap.selected,
            selected,
            descriptor_digest: [0; 32],
            selector_digest: content_digest(selector),
        };
        plan.descriptor_digest = content_digest(plan.encode_descriptor(workspace)?);
        Ok(plan)
    }

    /// Internal descriptor only; no decoder can turn arbitrary XDI1 into authority.
    fn encode_descriptor<'a>(&self, output: &'a mut [u8]) -> std::result::Result<&'a [u8], Cause> {
        if output.len() < PAGE_BYTES {
            return Err(Cause::Capacity);
        }
        let out = &mut output[..PAGE_BYTES];
        out.fill(0);
        out[..4].copy_from_slice(b"XDI1");
        out[4..6].copy_from_slice(&1u16.to_le_bytes());
        out[6] = 1;
        out[8..12].copy_from_slice(&(PAGE_BYTES as u32).to_le_bytes());
        out[16..32].copy_from_slice(&self.profile.spec().namespace.database.bytes());
        out[48..80].copy_from_slice(&self.profile.digest());
        out[80..84].copy_from_slice(&1u32.to_le_bytes());
        self.filemap
            .reference
            .encode(&mut out[128..192])
            .map_err(Cause::Control)?;
        self.selected
            .reference
            .encode(&mut out[200..264])
            .map_err(Cause::Control)?;
        for (offset, value) in [
            (32, self.profile.spec().namespace.timeline),
            (40, self.profile.spec().generation),
            (88, 4096),
            (96, 4096),
            (104, 0),
            (112, 0),
            (120, self.filemap.generation),
            (192, self.selected.generation),
            (264, doublewrite_control::POOL_BYTES as u64),
            (272, IMAGE_BYTES as u64),
            (280, IMAGE_SLOT_BYTES as u64),
            (288, (IMAGE_BYTES - IMAGE_SLOT_BYTES) as u64),
            (296, PAGE_BYTES as u64),
            (304, RESERVED_BYTES as u64),
            (312, (control::SELECTOR_BYTES * 2) as u64),
            (320, WORKSPACE_BYTES as u64),
            (
                328,
                (doublewrite_control::POOL_BYTES - control::SELECTOR_BYTES) as u64,
            ),
            (336, FIXED_BYTES),
            (344, MAX_OWNED_BYTES),
            (352, PROTECTED_BYTES),
            (360, 1),
        ] {
            out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        let digest = content_digest(&out[..PAGE_BYTES - 32]);
        out[PAGE_BYTES - 32..].copy_from_slice(&digest);
        Ok(out)
    }
}

pub(super) fn initial_recovery_expectation(
    bootstrap: &BootstrapPlan,
    workspace: &mut [u8; PAGE_BYTES],
) -> std::result::Result<(SelectedControl, [u8; 32]), Cause> {
    let plan = InstallPlan::prepare_inner(bootstrap, workspace)?;
    Ok((plan.selected, plan.descriptor_digest))
}

pub(super) fn recovery_preparation_bytes() -> usize {
    use foundation::codec::ContentHasher;
    use std::mem::size_of;
    // Enclosing/returned plan, encode_empty header and inventory iterator,
    // selector reference bytes, descriptor qword table, codec result carriers,
    // and the same pinned SHA-256 call allowance as bootstrap source_budget.
    4 * size_of::<InstallPlan>()
        + 8 * size_of::<Profile>()
        + 8 * size_of::<SelectedControl>()
        + 8 * size_of::<ControlRef>()
        + 4 * size_of::<ControlHeader>()
        + 4 * size_of::<std::result::Result<InstallPlan, Cause>>()
        + 4 * size_of::<ContentHasher>()
        + 1536 // Pinned SHA implementation call closure.
        + 128 + 64 + 512 // Fixed header/ref/descriptor field tables.
        + 1024 // Bounded refs, iterators, scalar and enum-tag carriers.
}

fn encode_empty(profile: Profile, output: &mut [u8]) -> std::result::Result<&[u8], Cause> {
    doublewrite_control::encode_control(
        ControlHeader {
            namespace: profile.spec().namespace,
            generation: 1,
            batch_id: 0,
            state: ControlState::Empty,
            max_lsn: PageLsn(0),
        },
        &[],
        output,
    )
    .map_err(Cause::Doublewrite)
}

fn write_new<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    bytes: &[u8],
    phases: [Phase; 3],
    effects: &mut Effects,
) -> Result<()> {
    let mut file = effects.run(phases[0], || {
        writer.create_new(name).map_err(Cause::Namespace)
    })?;
    effects.run(phases[1], || {
        write_all_at(&mut file, bytes, 0).map_err(Cause::Io)
    })?;
    effects.run(phases[2], || sync(&mut file).map_err(Cause::Io))
}

fn directory<W: WriterNamespace>(
    writer: &mut W,
    phase: Phase,
    effects: &mut Effects,
) -> Result<()> {
    effects.run(phase, || writer.sync_directory().map_err(Cause::Namespace))
}

fn install<W: WriterNamespace>(
    writer: &mut W,
    names: &Names,
    plan: &InstallPlan,
    workspace: &mut [u8; WORKSPACE_BYTES],
    effects: &mut Effects,
) -> Result<()> {
    use Phase::*;
    write_new(
        writer,
        &names.descriptor,
        &workspace[..PAGE_BYTES],
        [DescriptorCreate, DescriptorWrite, DescriptorSync],
        effects,
    )?;
    directory(writer, DescriptorDirectory, effects)?;
    workspace.fill(0);
    {
        let mut images = effects.run(ImagesCreate, || {
            writer.create_new(&names.images).map_err(Cause::Namespace)
        })?;
        for offset in (0..IMAGE_BYTES).step_by(WORKSPACE_BYTES) {
            let length = WORKSPACE_BYTES.min(IMAGE_BYTES - offset);
            effects.run(ImagesWrite, || {
                write_all_at(&mut images, &workspace[..length], offset as u64).map_err(Cause::Io)
            })?;
        }
        effects.run(ImagesSync, || sync(&mut images).map_err(Cause::Io))?;
    }
    // Workspace was zero streamed. Regenerate only previously validated controls,
    // bound to the immutable plan; mismatch cannot authorize a later effect.
    let empty = encode_empty(plan.profile, workspace)
        .map_err(|_| effects.error(Bank0Write, Cause::Invariant))?;
    if content_digest(empty) != plan.selected.reference.digest {
        return Err(effects.error(Bank0Write, Cause::Invariant));
    }
    write_new(
        writer,
        &names.bank0,
        &workspace[..PAGE_BYTES],
        [Bank0Create, Bank0Write, Bank0Sync],
        effects,
    )?;
    workspace[..PAGE_BYTES].fill(0);
    write_new(
        writer,
        &names.bank1,
        &workspace[..PAGE_BYTES],
        [Bank1Create, Bank1Write, Bank1Sync],
        effects,
    )?;
    write_new(
        writer,
        &names.reserved,
        &workspace[..RESERVED_BYTES],
        [ReservedCreate, ReservedWrite, ReservedSync],
        effects,
    )?;
    directory(writer, FilesDirectory, effects)?;
    let selected = control::encode_selector(
        SelectorRole::Doublewrite,
        plan.selected,
        plan.profile,
        workspace,
    )
    .map_err(|_| effects.error(SelectorWrite, Cause::Invariant))?;
    if content_digest(selected) != plan.selector_digest {
        return Err(effects.error(SelectorWrite, Cause::Invariant));
    }
    write_new(
        writer,
        &names.selector_stage,
        selected,
        [SelectorCreate, SelectorWrite, SelectorSync],
        effects,
    )?;
    effects.run(SelectorPublish, || {
        writer
            .publish_selector(&names.selector_stage, Selector::Doublewrite)
            .map_err(Cause::Namespace)
    })?;
    directory(writer, SelectorDirectory, effects)
}

fn open_checked<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    length: usize,
) -> std::result::Result<W::File, Cause> {
    let (file, opened) = writer
        .open_existing_with_metadata(name, OpenMode::ReadOnly)
        .map_err(Cause::Namespace)?;
    if opened.length != length as u64 {
        return Err(Cause::LengthMismatch);
    }
    let named = writer
        .metadata(name)
        .map_err(Cause::Namespace)?
        .ok_or(Cause::IdentityMismatch)?;
    if named.identity != opened.identity {
        return Err(Cause::IdentityMismatch);
    }
    if named.length != opened.length {
        return Err(Cause::LengthMismatch);
    }
    Ok(file)
}

fn read_control<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    output: &mut [u8],
) -> std::result::Result<(), Cause> {
    let mut file = open_checked(writer, name, output.len())?;
    read_exact_at(&mut file, output, 0).map_err(Cause::Io)
}
fn require_zero(bytes: &[u8]) -> std::result::Result<(), Cause> {
    if bytes.iter().any(|&byte| byte != 0) {
        Err(Cause::ContentMismatch)
    } else {
        Ok(())
    }
}
fn require_digest(bytes: &[u8], expected: [u8; 32]) -> std::result::Result<(), Cause> {
    if content_digest(bytes) != expected {
        Err(Cause::ContentMismatch)
    } else {
        Ok(())
    }
}

fn readback<W: WriterNamespace>(
    writer: &mut W,
    names: &Names,
    plan: &InstallPlan,
    workspace: &mut [u8; WORKSPACE_BYTES],
) -> std::result::Result<(), Cause> {
    read_control(
        writer,
        &names.profile,
        &mut workspace[..control::PROFILE_BYTES],
    )?;
    require_digest(&workspace[..control::PROFILE_BYTES], plan.profile.digest())?;
    read_control(writer, &names.descriptor, &mut workspace[..PAGE_BYTES])?;
    require_digest(&workspace[..PAGE_BYTES], plan.descriptor_digest)?;
    read_control(
        writer,
        &names.selected,
        &mut workspace[..control::SELECTOR_BYTES],
    )?;
    let selected = control::decode_selector(
        &workspace[..control::SELECTOR_BYTES],
        SelectorRole::Doublewrite,
        plan.profile,
    )
    .map_err(Cause::Control)?;
    if selected != plan.selected {
        return Err(Cause::SelectionMismatch);
    }
    read_control(writer, &names.bank0, &mut workspace[..PAGE_BYTES])?;
    let empty = control::decode_selected_doublewrite(
        &workspace[..EMPTY_BYTES],
        plan.selected.reference,
        plan.profile,
    )
    .map_err(Cause::Control)?;
    if empty.header().state != ControlState::Empty
        || empty.header().batch_id != 0
        || empty.len() != 0
        || empty.header().max_lsn != PageLsn(0)
    {
        return Err(Cause::ContentMismatch);
    }
    require_zero(&workspace[EMPTY_BYTES..PAGE_BYTES])?;
    read_control(writer, &names.bank1, &mut workspace[..PAGE_BYTES])?;
    require_zero(&workspace[..PAGE_BYTES])?;
    read_control(writer, &names.reserved, &mut workspace[..RESERVED_BYTES])?;
    require_zero(&workspace[..RESERVED_BYTES])?;
    {
        let mut images = open_checked(writer, &names.images, IMAGE_BYTES)?;
        for offset in (0..IMAGE_BYTES).step_by(WORKSPACE_BYTES) {
            let bytes = &mut workspace[..WORKSPACE_BYTES.min(IMAGE_BYTES - offset)];
            read_exact_at(&mut images, bytes, offset as u64).map_err(Cause::Io)?;
            require_zero(bytes)?;
        }
    }
    if writer
        .metadata(&names.selector_stage)
        .map_err(Cause::Namespace)?
        .is_some()
    {
        return Err(Cause::Residue);
    }
    Ok(())
}

/// Retains the sole original writer/WAL owner and exclusive caller workspace.
/// Automatic Drop releases ownership only; no file is deleted, synced or replayed.
pub(crate) struct DoublewriteStorageSession<'m, 'w, W: WriterNamespace> {
    fresh: FreshStorageSession<'m, W>,
    _workspace: &'w mut [u8; WORKSPACE_BYTES],
    _plan: InstallPlan,
}
impl<W: WriterNamespace> DoublewriteStorageSession<'_, '_, W> {
    /// Borrow the existing immutable FILEMAP/home reader and common failure fence.
    pub(crate) fn open_reader<'a>(
        &'a self,
        root: &'a mut [u8; PAGE_BYTES],
        scratch: &'a mut [u8; PAGE_BYTES],
    ) -> std::result::Result<PageFileReader<'a, W>, PageIoError> {
        self.fresh.open_reader(root, scratch)
    }
    pub(crate) fn is_fenced(&self) -> bool {
        self.fresh.is_fenced()
    }
}

impl<'m, W: WriterNamespace> FreshStorageSession<'m, W> {
    /// Consume pristine storage and install the fixed Empty doublewrite selection.
    /// Owns all I/O ordering, with no held shared mutex guard or caller callback.
    /// Every error consumes/fences the owner and preserves existing artifacts.
    pub(crate) fn install_doublewrite<'w>(
        mut self,
        workspace: &'w mut [u8; WORKSPACE_BYTES],
    ) -> Result<DoublewriteStorageSession<'m, 'w, W>> {
        let mut effects = Effects::default();
        let result = {
            let wal = &mut self.core.wal;
            let _unwind = wal.fenced.unwind_guard();
            (|| {
                let reject = |cause| effects.error(Phase::StatePreflight, cause);
                if std::mem::size_of::<DoublewriteStorageSession<'m, 'w, W>>() > 16384
                    || std::mem::size_of::<InstallPlan>() > 4096
                {
                    return Err(reject(Cause::Capacity));
                }
                super::super::genesis_profile(wal.profile)
                    .map_err(|_| reject(Cause::InvalidState))?;
                if wal.profile != self.core.plan.profile {
                    return Err(reject(Cause::InvalidState));
                }
                if wal.fenced.is_fenced() {
                    return Err(reject(Cause::Fenced));
                }
                if self.core.reader_active.load(Ordering::Acquire) {
                    return Err(reject(Cause::Busy));
                }
                // get_mut checks poison but acquires no lock and performs no I/O.
                let writer = wal._writer.get_mut().map_err(|_| reject(Cause::Poisoned))?;
                wal.file.get_mut().map_err(|_| reject(Cause::Poisoned))?;
                let append = wal.append.get_mut().map_err(|_| reject(Cause::Poisoned))?;
                let prefix = append.prefix;
                if append.slot.is_none()
                    || append.state != super::super::SlotState::Idle
                    || prefix.complete_lsn != 0
                    || prefix.synced_lsn != 0
                    || prefix.complete_offset != 4096
                    || prefix.synced_offset != 4096
                    || prefix.stream.writer_epoch != 1
                    || prefix.stream.database != wal.profile.spec().namespace.database
                    || prefix.stream.timeline != wal.profile.spec().namespace.timeline
                {
                    return Err(reject(Cause::InvalidState));
                }
                let names = Names::new().map_err(|cause| effects.error(Phase::Prepare, cause))?;
                let plan = InstallPlan::prepare(&self.core.plan, workspace)
                    .map_err(|cause| effects.error(Phase::Prepare, cause))?;
                names
                    .preflight(writer)
                    .map_err(|cause| effects.error(Phase::NamespacePreflight, cause))?;
                install(writer, &names, &plan, workspace, &mut effects)?;
                readback(writer, &names, &plan, workspace)
                    .map_err(|cause| effects.error(Phase::Readback, cause))?;
                Ok(plan)
            })()
        };
        match result {
            Ok(plan) => Ok(DoublewriteStorageSession {
                fresh: self,
                _workspace: workspace,
                _plan: plan,
            }),
            Err(error) => {
                self.core.wal.fenced.fence();
                Err(error)
            }
        }
    }
}

pub(in crate::storage) use mutable_storage::StatusLoadPermit;

pub(in crate::storage) use mutable_storage::EmptyTransactionInstall;
pub(crate) use mutable_storage::EmptyTransactionObservation;

pub(in crate::storage) use allocation_reservation::operation::{
    SemanticError as CatalogAllocationError, plan4_base_allocation_source_bytes,
    plan4_system_base_allocation_source_bytes, validate_catalog_base_allocation,
    validate_plan4_base_allocation, validate_plan4_system_base_allocation,
    validate_row_base_allocation,
};

pub(in crate::storage) use mutable_storage::{
    RetentionFlushAuthority, RetentionInitAuthority, RetentionInitCommit, RetentionInitCompleted,
    RetentionInitInstall, RetentionInstall, RetentionLoadPermit, VerifiedLookupLoad,
};

pub(in crate::storage) use mutable_storage::RecoveryPoolSeed;

#[allow(unused_imports)] // Same original owner capabilities across the private module chain.
pub(in crate::storage) use mutable_storage::{
    AppliedRetirement, BoundVisibility, CapturedCatalogRefs, CatalogLookupLoan, CatalogSource,
    HorizonObservation, InstalledFrontier, ReferenceLease, RetirementIntent, RetirementReject,
    RetirementScope, RollbackSource, SnapshotGuard, SnapshotReservation, SourceFrontier,
    StagedRetirement, TxPairAuthority, TxPairCancelKind, TxPairCancelPermit, TxPairControlCommit,
    TxPairInstallPermit, TxPairRetirePermit, TxPairStageRejection, TxnPin, UndoPin,
    VISIBILITY_BYTES, VerifiedRetirement, VisibilityCause, VisibilityError, VisibilityLoan,
    VisibilityResult,
};
