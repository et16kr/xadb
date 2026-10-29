//! Fresh genesis WAL installation and one bounded physical append/sync slot.
//! These observations authorize neither DB READY nor page flush/commit/recovery.
//! See devdoc/contracts/dev3-wal-io.md. No existing epoch is reopened for append.
#![allow(dead_code)] // Awaiting the separately reviewed semantic transaction owner.

use std::{
    sync::{Condvar, Mutex, MutexGuard, atomic::AtomicU64},
    time::Duration,
};

use super::{
    control::{self, ControlError, Profile, ProfileSpec},
    session_fence::SessionFence,
    wal_frame::{MAX_FRAME_BYTES, WalError, WalStreamIdentity},
};
use crate::api::{DatabaseId, Namespace};
use foundation::{
    codec::content_digest,
    error::Effect,
    io::{read_exact_at, sync, write_all_at},
    namespace::{FileIdentity, FileName, NamespaceError, OpenMode, Selector, WriterNamespace},
};

#[cfg(test)]
use super::wal_frame::{self, FrameExpectation};
#[cfg(test)]
use std::sync::atomic::Ordering;

pub(crate) const CONTROL_BYTES: usize = 4096;
pub(crate) const WAL_FILE_CAP: u64 = 256 << 20;
pub(crate) const WAL_NAME: &str = "d3-wal-00000001";
const PROFILE_NAME: &str = "d3-profile";
const STAGE_NAME: &str = "d3-writer-stage";

mod bootstrap;
pub(in crate::storage) mod disk_api;
mod physical;
pub(in crate::storage) use bootstrap::CatalogRetirementRejected;
pub(in crate::storage) use bootstrap::Plan4LoadPermit;
#[cfg(test)]
pub(in crate::storage) use bootstrap::plan4_test_mutate_page;
#[cfg(test)]
pub(in crate::storage) use bootstrap::plan4_test_support;
#[allow(unused_imports)] // Private downstream assembly remains under test-support.
pub(crate) use bootstrap::{
    AllocationObservation, AllocationSession, AllocationSessionCause, AllocationSessionError,
    AllocationSessionPhase, BootstrapError, Cause as BootstrapCause, DoublewriteInstallCause,
    DoublewriteInstallError, DoublewriteInstallPhase, DoublewriteStorageSession,
    FreshStorageSession, Phase as BootstrapPhase, StatusRole, UndoRole,
};
#[allow(unused_imports)] // The scoped dirty owner is being integrated with its consumers.
pub(crate) use bootstrap::{
    AllocationWriter, AllocatorFlushError, AllocatorFlusher, BoundAllocation, DirtyAllocationError,
    DirtyReader, FlushProgress, MutableStorageError, MutableStorageOwner,
};
#[allow(unused_imports)]
pub(crate) use bootstrap::{
    BeforeEffectError, BoundStatusBootstrap, BoundStatusWithFlush, EMPTY_TRANSACTION_BANK_BYTES,
    EmptyTransactionSnapshot, EmptyTransactions, InitAttemptError, InitializedStatusObservation,
    IntoEmptyTransactionsError, IntoFlushError, IntoOperationalStatusError, OperationalStatus,
    OperationalStatusReader, OperationalStatusWriter, ReadStatusSet, ReservedStatus,
    ResidentReadAdmission, ResidentStatus, ResidentStatusBinding, ResidentStatusReader,
    StatusBootstrapWriter, StatusFlushError, StatusFlusher, StatusInitError, StatusReadGuard,
    StatusReader,
};
#[allow(unused_imports)] // Closed prefix observations await the next recovery barrier.
pub(crate) use bootstrap::{
    ClosedRecovery, RecoveryAcquireError, RecoveryCause, RecoveryError, RecoveryFailure,
    RecoveryInspection, RecoveryPhase, RecoveryPrefix, RecoveryPrefixCause, RecoveryPrefixError,
    RecoveryPrefixObservation, RecoveryPrefixPhase, RecoveryProducerMode,
};
pub(in crate::storage) use bootstrap::{
    CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit,
};
pub(in crate::storage) use bootstrap::{
    DirtyInstallPermit, FlushFinalizePermit, ReloadInstallPermit,
};
pub(in crate::storage) use bootstrap::{
    InitialRecoveryError, inspect_initial_bootstrap, recovery_preparation_bytes,
    recovery_reader_bytes,
};
#[allow(unused_imports)] // The consuming allocator reload is assembled by scoped consumers.
pub(crate) use bootstrap::{
    IntoOperationalError, OperationalAllocation, ReloadError, ReloadObservation,
};
#[allow(unused_imports)]
pub(crate) use bootstrap::{
    Plan4CatalogLookup, Plan4GenesisCensus, Plan4LoadObservation, Plan4Page, Plan4PageCause,
    Plan4PageError, Plan4PagePhase, Plan4Pages, Plan4ResidencyCursor, Plan4Resident, Plan4Target,
};
#[allow(unused_imports)]
pub(in crate::storage) use bootstrap::{
    Plan4GenesisCause, Plan4GenesisError, Plan4GenesisPhase, Plan4GenesisSeed,
    inspect_plan4_bootstrap, plan4_genesis_capacity, plan4_genesis_source_census,
};
#[cfg(test)]
pub(in crate::storage) use bootstrap::{
    Plan4TestMutableCause, Plan4TestSemanticError, plan4_test_owner_boundary,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Preflight,
    Entropy,
    ProfileCreate,
    ProfileWrite,
    ProfileSync,
    EpochCreate,
    EpochWrite,
    EpochSync,
    InitialDirectorySync,
    SelectorCreate,
    SelectorWrite,
    SelectorSync,
    SelectorPublish,
    SelectedDirectorySync,
    Readback,
    Stage,
    Write,
    Sync,
    Publish,
    Observe,
}

#[derive(Debug)]
pub(crate) enum Cause {
    InvalidInput,
    Busy,
    NoPending,
    Fenced,
    Control(ControlError),
    Frame(WalError),
    Namespace(NamespaceError),
    Io(foundation::Error),
    IdentityMismatch,
    LengthMismatch,
    MappingMismatch,
    Capacity,
    ArithmeticOverflow,
    Poisoned,
}

/// Cumulative component effect and the original lower cause remain distinct.
#[derive(Debug)]
pub(crate) struct WalIoError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, WalIoError>;
fn error(phase: Phase, effect: Effect, cause: Cause) -> WalIoError {
    WalIoError {
        phase,
        effect,
        cause,
    }
}
fn unchanged(phase: Phase, cause: Cause) -> WalIoError {
    error(phase, Effect::Unchanged, cause)
}
fn attempted(phase: Phase, cause: Cause) -> WalIoError {
    error(phase, Effect::CallerMustResolve, cause)
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked control"),
    )
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("checked control"),
    )
}
fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn genesis_profile(profile: Profile) -> std::result::Result<(), ControlError> {
    let spec = profile.spec();
    if spec.generation != 1 || spec.namespace.timeline != 1 {
        return Err(ControlError::InvalidInput);
    }
    Ok(())
}
fn finish_control(bytes: &mut [u8]) {
    let digest = content_digest(&bytes[..CONTROL_BYTES - 32]);
    bytes[CONTROL_BYTES - 32..CONTROL_BYTES].copy_from_slice(&digest);
}
fn control_prefix<'a>(
    output: &'a mut [u8],
    profile: Profile,
    magic: &[u8; 4],
) -> std::result::Result<&'a mut [u8], ControlError> {
    genesis_profile(profile)?;
    if output.len() < CONTROL_BYTES {
        return Err(ControlError::NoSpace);
    }
    let bytes = &mut output[..CONTROL_BYTES];
    bytes.fill(0);
    bytes[..4].copy_from_slice(magic);
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6] = 1;
    put32(bytes, 8, CONTROL_BYTES as u32);
    bytes[16..32].copy_from_slice(&profile.spec().namespace.database.bytes());
    put64(bytes, 32, 1);
    Ok(bytes)
}

pub(crate) fn encode_epoch(
    profile: Profile,
    output: &mut [u8],
) -> std::result::Result<&[u8], ControlError> {
    let bytes = control_prefix(output, profile, b"XWE1")?;
    put32(bytes, 40, 1);
    put64(bytes, 48, 1);
    bytes[56..88].copy_from_slice(&profile.digest());
    put64(bytes, 96, CONTROL_BYTES as u64);
    put32(bytes, 104, MAX_FRAME_BYTES as u32);
    put64(bytes, 112, WAL_FILE_CAP);
    finish_control(bytes);
    Ok(bytes)
}

pub(crate) fn encode_writer<'a>(
    profile: Profile,
    epoch: &[u8],
    output: &'a mut [u8],
) -> std::result::Result<&'a [u8], ControlError> {
    let epoch_digest = decode_epoch(epoch, profile)?;
    let bytes = control_prefix(output, profile, b"XWS1")?;
    put64(bytes, 40, 1);
    put32(bytes, 48, 1);
    put64(bytes, 56, 1);
    bytes[64..96].copy_from_slice(&profile.digest());
    bytes[96..128].copy_from_slice(&epoch_digest);
    put64(bytes, 136, CONTROL_BYTES as u64);
    finish_control(bytes);
    Ok(bytes)
}

fn check_control(
    bytes: &[u8],
    profile: Profile,
    magic: &[u8; 4],
) -> std::result::Result<(), ControlError> {
    genesis_profile(profile)?;
    if bytes.len() != CONTROL_BYTES || &bytes[..4] != magic {
        return Err(ControlError::Corrupt);
    }
    if bytes[4..6] != 1u16.to_le_bytes() || bytes[6] != 1 {
        return Err(ControlError::UnsupportedFormat);
    }
    if bytes[7] != 0
        || u32_at(bytes, 8) != CONTROL_BYTES as u32
        || bytes[12..16] != [0; 4]
        || bytes[4064..] != content_digest(&bytes[..4064])
    {
        return Err(ControlError::Corrupt);
    }
    if bytes[16..32] != profile.spec().namespace.database.bytes() || u64_at(bytes, 32) != 1 {
        return Err(ControlError::IdentityMismatch);
    }
    Ok(())
}

pub(crate) fn decode_epoch(
    bytes: &[u8],
    profile: Profile,
) -> std::result::Result<[u8; 32], ControlError> {
    check_control(bytes, profile, b"XWE1")?;
    if u32_at(bytes, 40) != 1 || u64_at(bytes, 48) != 1 || bytes[56..88] != profile.digest() {
        return Err(ControlError::IdentityMismatch);
    }
    if bytes[44..48] != [0; 4]
        || u64_at(bytes, 88) != 0
        || u64_at(bytes, 96) != CONTROL_BYTES as u64
        || u32_at(bytes, 104) != MAX_FRAME_BYTES as u32
        || bytes[108..112] != [0; 4]
        || u64_at(bytes, 112) != WAL_FILE_CAP
        || bytes[120..4064].iter().any(|&b| b != 0)
    {
        return Err(ControlError::Corrupt);
    }
    Ok(content_digest(bytes))
}

pub(crate) fn decode_writer(
    bytes: &[u8],
    profile: Profile,
    epoch_digest: [u8; 32],
) -> std::result::Result<(), ControlError> {
    check_control(bytes, profile, b"XWS1")?;
    if u64_at(bytes, 40) != 1
        || u32_at(bytes, 48) != 1
        || u64_at(bytes, 56) != 1
        || bytes[64..96] != profile.digest()
        || bytes[96..128] != epoch_digest
    {
        return Err(ControlError::IdentityMismatch);
    }
    if bytes[52..56] != [0; 4]
        || u64_at(bytes, 128) != 0
        || u64_at(bytes, 136) != CONTROL_BYTES as u64
        || bytes[144..4064].iter().any(|&b| b != 0)
    {
        return Err(ControlError::Corrupt);
    }
    Ok(())
}

/// A completed byte interval, without semantic apply or commit authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalFrameLocation {
    pub(crate) stream: WalStreamIdentity,
    pub(crate) file: FileIdentity,
    pub(crate) previous_lsn: u64,
    pub(crate) lsn: u64,
    pub(crate) start: u64,
    pub(crate) length: usize,
    pub(crate) end: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalPrefix {
    pub(crate) stream: WalStreamIdentity,
    pub(crate) file: FileIdentity,
    pub(crate) complete_lsn: u64,
    pub(crate) complete_offset: u64,
    pub(crate) synced_lsn: u64,
    pub(crate) synced_offset: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalSyncObservation {
    pub(crate) frame: WalFrameLocation,
    pub(crate) prefix: WalPrefix,
}

/// A stage-issued exact interval. Copies share a fact, never an I/O claim.
/// The borrowed origin prevents moving or dropping the issuing session.
#[derive(Clone, Copy)]
pub(in crate::storage) struct WalRequirement<'s> {
    origin: &'s SessionFence,
    exact: WalFrameLocation,
}
impl std::fmt::Debug for WalRequirement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalRequirement")
            .field("exact", &self.exact)
            .finish_non_exhaustive()
    }
}
impl WalRequirement<'_> {
    pub(in crate::storage) fn location(self) -> WalFrameLocation {
        self.exact
    }
    pub(in crate::storage) fn uses_fence(self, fence: &SessionFence) -> bool {
        std::ptr::eq(self.origin, fence)
    }
}

/// Confirmed durability for one authentic requirement, not a caller LSN.
#[derive(Debug)]
pub(in crate::storage) struct DurableWal<'s> {
    requirement: WalRequirement<'s>,
    prefix: WalPrefix,
}
impl DurableWal<'_> {
    pub(in crate::storage) fn confirms(&self, requirement: &WalRequirement<'_>) -> bool {
        std::ptr::eq(self.requirement.origin, requirement.origin)
            && self.requirement.exact == requirement.exact
    }
    pub(in crate::storage) fn observation(&self) -> WalSyncObservation {
        WalSyncObservation {
            frame: self.requirement.exact,
            prefix: self.prefix,
        }
    }
}

#[derive(Debug)]
pub(in crate::storage) enum EnsureDurable<'s> {
    AlreadySynced(DurableWal<'s>),
    SyncedHere(DurableWal<'s>),
    AwaitWalOwner,
}

const COMPLETION_RECHECK: Duration = Duration::from_millis(10);
pub(in crate::storage) const DURABILITY_CALL_BYTES: usize = 4096;

pub(in crate::storage) fn durability_types_fit() -> bool {
    std::mem::size_of::<WalRequirement<'_>>() <= 128
        && std::mem::size_of::<Option<WalRequirement<'_>>>() <= 128
        && std::mem::size_of::<DurableWal<'_>>() <= 256
        && std::mem::size_of::<EnsureDurable<'_>>() <= 512
        && std::mem::size_of::<Result<EnsureDurable<'_>>>() <= 512
        && std::mem::size_of::<Condvar>() <= 64
        && std::mem::size_of::<WalFrameLocation>() <= 128
        && std::mem::size_of::<WalPrefix>() <= 128
        && std::mem::size_of::<WalIoError>() <= 128
        && std::mem::size_of::<IoFence<'_>>() <= 64
        && std::mem::size_of::<MutexGuard<'_, AppendState<'_>>>() <= 32
        && std::mem::size_of::<std::sync::LockResult<MutexGuard<'_, AppendState<'_>>>>() <= 64
        && std::mem::size_of::<
            std::sync::LockResult<(
                MutexGuard<'_, AppendState<'_>>,
                std::sync::WaitTimeoutResult,
            )>,
        >() <= 64
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Idle,
    Complete(WalFrameLocation),
    IoOwned(WalFrameLocation),
}
struct AppendState<'m> {
    slot: Option<&'m mut [u8; MAX_FRAME_BYTES]>,
    state: SlotState,
    prefix: WalPrefix,
}

/// Owns a fresh genesis namespace and its selected file for this session only.
/// The slot remains borrowed even when an uncertain I/O claim cannot be returned.
pub(crate) struct GenesisWalSession<'m, W: WriterNamespace> {
    runtime: WalRuntime<'m, W>,
}

// Original resources, moved once from the fresh constructor. This private core
// has no constructor accepting observed prefixes or replacement fences.
struct WalRuntime<'m, W: WriterNamespace> {
    _writer: Mutex<W>,
    file: Mutex<W::File>,
    append: Mutex<AppendState<'m>>,
    completion: Condvar,
    // Physical first-write fact for this original runtime only, never a semantic
    // Commit grant or a recovered prefix. No borrowed core/self-reference.
    first_write_lsn: AtomicU64,
    #[cfg(test)]
    durability_waits: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    durability_before_wait: Mutex<Option<durability_test_support::BeforeWait>>,
    fenced: SessionFence,
    profile: Profile,
}

fn name(value: &str) -> Result<FileName> {
    FileName::new(value).map_err(|e| unchanged(Phase::Preflight, Cause::Namespace(e)))
}

fn write_new<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    bytes: &[u8],
    phases: [Phase; 3],
) -> Result<()> {
    let mut file = writer
        .create_new(name)
        .map_err(|e| attempted(phases[0], Cause::Namespace(e)))?;
    write_all_at(&mut file, bytes, 0).map_err(|e| attempted(phases[1], Cause::Io(e)))?;
    sync(&mut file).map_err(|e| attempted(phases[2], Cause::Io(e)))
}

fn open_installed<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    mode: OpenMode,
) -> Result<(W::File, FileIdentity)> {
    let (file, metadata) = writer
        .open_existing_with_metadata(name, mode)
        .map_err(|e| attempted(Phase::Readback, Cause::Namespace(e)))?;
    if metadata.length != CONTROL_BYTES as u64 {
        return Err(attempted(Phase::Readback, Cause::LengthMismatch));
    }
    let named = writer
        .metadata(name)
        .map_err(|e| attempted(Phase::Readback, Cause::Namespace(e)))?;
    if named != Some(metadata) {
        return Err(attempted(Phase::Readback, Cause::IdentityMismatch));
    }
    Ok((file, metadata.identity))
}

/// Checked physical arithmetic is independent of logical LSN issuance.
fn interval_end(start: u64, length: usize) -> std::result::Result<u64, Cause> {
    let length = u64::try_from(length).map_err(|_| Cause::ArithmeticOverflow)?;
    let end = start.checked_add(length).ok_or(Cause::ArithmeticOverflow)?;
    if start > i64::MAX as u64 || end > i64::MAX as u64 {
        return Err(Cause::ArithmeticOverflow);
    }
    Ok(end)
}

impl<'m, W: WriterNamespace> GenesisWalSession<'m, W> {
    /// Install only in a complete LOCK-only namespace; no existing files are reused.
    /// Failure after the first create attempt preserves every surviving artifact.
    pub(crate) fn create(
        writer: W,
        profile_id: u32,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [u8; 16384],
    ) -> Result<Self> {
        Self::create_with_profile(writer, profile_id, false, slot, scratch)
    }

    /// Same original namespace/WAL owner with the explicit current plan4 tuple.
    fn create_catalog(
        writer: W,
        profile_id: u32,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [u8; 16384],
    ) -> Result<Self> {
        Self::create_with_profile(writer, profile_id, true, slot, scratch)
    }

    fn create_with_profile(
        mut writer: W,
        profile_id: u32,
        catalog: bool,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [u8; 16384],
    ) -> Result<Self> {
        if !(1..=3).contains(&profile_id) || std::mem::size_of::<Self>() > 16384 {
            return Err(unchanged(Phase::Preflight, Cause::InvalidInput));
        }
        {
            let inventory = writer
                .list_owned(1)
                .map_err(|e| unchanged(Phase::Preflight, Cause::Namespace(e)))?;
            if inventory.len() != 1
                || inventory[0].name.as_str() != "LOCK"
                || inventory[0].metadata.length != 0
            {
                return Err(unchanged(Phase::Preflight, Cause::InvalidInput));
            }
        }
        let incarnation = writer
            .random_incarnation()
            .map_err(|e| unchanged(Phase::Entropy, Cause::Namespace(e)))?;
        let database = DatabaseId::new(incarnation)
            .map_err(|_| unchanged(Phase::Entropy, Cause::InvalidInput))?;
        let namespace = Namespace {
            database,
            timeline: 1,
        };
        let profile_name = name(PROFILE_NAME)?;
        let wal_name = name(WAL_NAME)?;
        let stage_name = name(STAGE_NAME)?;
        let selector_name = name("WRITER")?;
        let (profile_bytes, rest) = scratch.split_at_mut(CONTROL_BYTES);
        let (epoch_bytes, rest) = rest.split_at_mut(CONTROL_BYTES);
        let (selector_bytes, readback) = rest.split_at_mut(CONTROL_BYTES);
        // The catalog arm is the current Profile3 plan4 owner; the other arm
        // stays the DEV2 Profile1 owner. Each decodes with its own grammar.
        let (encode_profile, decode_profile): (
            fn(ProfileSpec, &mut [u8]) -> std::result::Result<&[u8], ControlError>,
            fn(&[u8], Option<Namespace>) -> std::result::Result<Profile, ControlError>,
        ) = if catalog {
            (
                control::row_profile::encode_profile,
                control::row_profile::decode_profile,
            )
        } else {
            (control::encode_profile, control::decode_bootstrap_profile)
        };
        encode_profile(
            ProfileSpec {
                namespace,
                generation: 1,
                id: profile_id,
            },
            profile_bytes,
        )
        .map_err(|e| unchanged(Phase::Preflight, Cause::Control(e)))?;
        let profile = decode_profile(profile_bytes, Some(namespace))
            .map_err(|e| unchanged(Phase::Preflight, Cause::Control(e)))?;
        encode_epoch(profile, epoch_bytes)
            .map_err(|e| unchanged(Phase::Preflight, Cause::Control(e)))?;
        encode_writer(profile, epoch_bytes, selector_bytes)
            .map_err(|e| unchanged(Phase::Preflight, Cause::Control(e)))?;
        // Encoding, names and all fixed capacity checks precede the first create.
        // Maximum genesis files: WAL cap + profile + selected WRITER, once each.
        if WAL_FILE_CAP
            .checked_add(2 * CONTROL_BYTES as u64)
            .is_none_or(|n| n > profile.owned_bytes())
        {
            return Err(unchanged(Phase::Preflight, Cause::Capacity));
        }
        write_new(
            &mut writer,
            &profile_name,
            profile_bytes,
            [
                Phase::ProfileCreate,
                Phase::ProfileWrite,
                Phase::ProfileSync,
            ],
        )?;
        write_new(
            &mut writer,
            &wal_name,
            epoch_bytes,
            [Phase::EpochCreate, Phase::EpochWrite, Phase::EpochSync],
        )?;
        writer
            .sync_directory()
            .map_err(|e| attempted(Phase::InitialDirectorySync, Cause::Namespace(e)))?;
        write_new(
            &mut writer,
            &stage_name,
            selector_bytes,
            [
                Phase::SelectorCreate,
                Phase::SelectorWrite,
                Phase::SelectorSync,
            ],
        )?;
        writer
            .publish_selector(&stage_name, Selector::Writer)
            .map_err(|e| attempted(Phase::SelectorPublish, Cause::Namespace(e)))?;
        writer
            .sync_directory()
            .map_err(|e| attempted(Phase::SelectedDirectorySync, Cause::Namespace(e)))?;
        {
            let (mut file, _) = open_installed(&mut writer, &profile_name, OpenMode::ReadOnly)?;
            read_exact_at(&mut file, readback, 0)
                .map_err(|e| attempted(Phase::Readback, Cause::Io(e)))?;
            let observed = decode_profile(readback, Some(namespace))
                .map_err(|e| attempted(Phase::Readback, Cause::Control(e)))?;
            if observed != profile {
                return Err(attempted(Phase::Readback, Cause::IdentityMismatch));
            }
        }
        let (mut file, identity) = open_installed(&mut writer, &wal_name, OpenMode::ReadWrite)?;
        read_exact_at(&mut file, readback, 0)
            .map_err(|e| attempted(Phase::Readback, Cause::Io(e)))?;
        let epoch_digest = decode_epoch(readback, profile)
            .map_err(|e| attempted(Phase::Readback, Cause::Control(e)))?;
        if readback != epoch_bytes {
            return Err(attempted(Phase::Readback, Cause::IdentityMismatch));
        }
        {
            let (mut selector, _) =
                open_installed(&mut writer, &selector_name, OpenMode::ReadOnly)?;
            read_exact_at(&mut selector, readback, 0)
                .map_err(|e| attempted(Phase::Readback, Cause::Io(e)))?;
            decode_writer(readback, profile, epoch_digest)
                .map_err(|e| attempted(Phase::Readback, Cause::Control(e)))?;
        }
        let prefix = WalPrefix {
            stream: WalStreamIdentity {
                database,
                timeline: 1,
                writer_epoch: 1,
            },
            file: identity,
            complete_lsn: 0,
            complete_offset: CONTROL_BYTES as u64,
            synced_lsn: 0,
            synced_offset: CONTROL_BYTES as u64,
        };
        Ok(Self {
            runtime: WalRuntime {
                _writer: Mutex::new(writer),
                file: Mutex::new(file),
                append: Mutex::new(AppendState {
                    slot: Some(slot),
                    state: SlotState::Idle,
                    prefix,
                }),
                completion: Condvar::new(),
                first_write_lsn: AtomicU64::new(0),
                #[cfg(test)]
                durability_waits: std::sync::atomic::AtomicUsize::new(0),
                #[cfg(test)]
                durability_before_wait: Mutex::new(None),
                fenced: SessionFence::new(),
                profile,
            },
        })
    }

    pub(crate) fn is_fenced(&self) -> bool {
        self.runtime.is_fenced()
    }

    pub(crate) fn prefix(&self) -> Result<WalPrefix> {
        self.runtime.prefix()
    }

    pub(crate) fn try_stage(&self, bytes: &[u8]) -> Result<WalFrameLocation> {
        self.runtime.try_stage(bytes)
    }

    pub(crate) fn sync_pending(&self) -> Result<WalSyncObservation> {
        self.runtime.sync_pending()
    }

    pub(in crate::storage) fn ensure_durable<'s>(
        &self,
        requirement: &WalRequirement<'s>,
    ) -> Result<EnsureDurable<'s>> {
        self.runtime.ensure_durable(requirement)
    }
}

struct IoFence<'a> {
    fenced: &'a SessionFence,
    completion: &'a Condvar,
    armed: bool,
}
impl Drop for IoFence<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.fenced.fence();
            self.completion.notify_all();
        }
    }
}

#[cfg(test)]
#[path = "wal_io/durability_test_support.rs"]
mod durability_test_support;
#[cfg(test)]
#[path = "wal_io/durability_tests.rs"]
mod durability_tests;
#[cfg(test)]
#[path = "wal_io/first_write_tests.rs"]
mod first_write_tests;
#[cfg(test)]
#[path = "wal_io/test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "wal_io/tests.rs"]
mod tests;

pub(in crate::storage) use bootstrap::StatusLoadPermit;

#[allow(unused_imports)] // Sealed authority is also named by sibling compiler probes.
pub(in crate::storage) use bootstrap::EmptyTransactionInstall;
#[allow(unused_imports)] // The live consumer is connected in the next integration step.
pub(crate) use bootstrap::EmptyTransactionObservation;

pub(in crate::storage) use bootstrap::{
    encode_plan4_catalog_genesis, plan4_base_allocation_source_bytes,
    plan4_catalog_genesis_source_bytes, plan4_system_base_allocation_source_bytes,
    validate_plan4_base_allocation, validate_plan4_system_base_allocation,
};

#[allow(
    unused_imports,
    reason = "sealed sibling authority names are kept for internal consumers"
)]
pub(in crate::storage) use bootstrap::{
    RetentionFlushAuthority, RetentionInitAuthority, RetentionInitCommit, RetentionInitCompleted,
    RetentionInitInstall, RetentionInstall, RetentionLoadPermit, VerifiedLookupLoad,
};

pub(in crate::storage) use bootstrap::RecoveryPoolSeed;

#[allow(unused_imports)] // Same original owner capabilities across the private module chain.
pub(in crate::storage) use bootstrap::{
    AppliedRetirement, BoundVisibility, CapturedCatalogRefs, CatalogLookupLoan, CatalogSource,
    HorizonObservation, InstalledFrontier, ReferenceLease, RetirementIntent, RetirementReject,
    RetirementScope, RollbackSource, SnapshotGuard, SnapshotReservation, SourceFrontier,
    StagedRetirement, TxPairAuthority, TxPairCancelKind, TxPairCancelPermit, TxPairControlCommit,
    TxPairInstallPermit, TxPairRetirePermit, TxPairStageRejection, TxnPin, UndoPin,
    VISIBILITY_BYTES, VerifiedRetirement, VisibilityCause, VisibilityError, VisibilityLoan,
    VisibilityResult,
};

pub(in crate::storage) use bootstrap::{
    CatalogAllocationError, validate_catalog_base_allocation, validate_row_base_allocation,
};
