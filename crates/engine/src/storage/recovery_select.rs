//! Closed, read-only selection of the initial DEV3 disk closure.
//! No BOOT_RESERVE, semantic WAL validation, repair, reader or READY authority.
//! Owner contract: devdoc/contracts/dev3-recovery-select.md.
#![allow(dead_code)] // This first closed barrier has no normal-open consumer yet.

#[path = "plan4_recovery_select.rs"]
mod plan4;
#[cfg(all(test, target_os = "linux"))]
pub(in crate::storage) use plan4::native as plan4_native_support;
#[allow(unused_imports)]
pub(crate) use plan4::{
    Plan4GenesisObservation, Plan4GenesisSelection, Plan4LeaseEffect, Plan4SelectCause,
    Plan4SelectError, Plan4SelectPhase, source_census as plan4_selection_source_census,
};

use super::{
    control::{self, ControlError, ControlKind, Profile, SelectedControl, SelectorRole},
    doublewrite_control::ControlState,
    page::PAGE_BYTES,
    page_io::{PageFileReader, PageIoError},
    wal_io::{self, BootstrapCause, BootstrapError, InitialRecoveryError},
};
use crate::api::Namespace;
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    io::read_exact_at,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceError, NamespaceErrorKind,
        NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};

pub(crate) const BLOCKED_NAMES: [&str; 7] = [
    "CURRENT",
    "d3-current-stage",
    "d3-rebase-0",
    "d3-rebase-1",
    "d3-writer-stage",
    "d3-filemap-select-stage",
    "d3-dwb-select-stage",
];
const GEOMETRY: [(&str, u64); 7] = [
    ("d3-data-0000000100000001", control::EXTENT_BYTES),
    ("d3-filemap-0", control::CONTAINER_BYTES),
    ("d3-filemap-1", control::CONTAINER_BYTES),
    ("d3-dwb-0", PAGE_BYTES as u64),
    ("d3-dwb-1", PAGE_BYTES as u64),
    ("d3-dwb-images", 16_711_680),
    ("d3-dwb-reserved", 8192),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Preflight,
    Lease,
    Profile,
    FileMap,
    Bootstrap,
    Doublewrite,
    Epoch,
    Writer,
}
#[derive(Debug)]
pub(crate) enum Cause {
    Capacity,
    UnsupportedClosure,
    UnsupportedBootstrapPlan,
    LengthMismatch,
    IdentityMismatch,
    Namespace(NamespaceError),
    Io(xadb_foundation::Error),
    Control(ControlError),
    PageIo(PageIoError),
    Bootstrap(BootstrapError),
    Doublewrite(wal_io::DoublewriteInstallCause),
}
#[derive(Debug)]
pub(crate) struct RecoverySelectError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, RecoverySelectError>;
fn error(phase: Phase, cause: Cause) -> RecoverySelectError {
    RecoverySelectError {
        phase,
        effect: Effect::Unchanged,
        cause,
    }
}
fn name(value: &str) -> FileName {
    FileName::new(value).expect("bounded constant recovery filename")
}

/// Immutable read observations, borrowed through the closed owner. They neither
/// certify fsync nor grant a reusable selected-control/replay capability.
pub(crate) struct RecoverySelectionObservation {
    namespace: Namespace,
    profile: u32,
    plan: u64,
    filemap: u64,
    doublewrite: u64,
    epoch_digest: [u8; 32],
}
impl RecoverySelectionObservation {
    pub(crate) fn namespace(&self) -> Namespace {
        self.namespace
    }
    pub(crate) fn profile_id(&self) -> u32 {
        self.profile
    }
    pub(crate) fn bootstrap_plan(&self) -> u64 {
        self.plan
    }
    pub(crate) fn filemap_generation(&self) -> u64 {
        self.filemap
    }
    pub(crate) fn doublewrite_generation(&self) -> u64 {
        self.doublewrite
    }
    pub(crate) fn epoch_digest(&self) -> &[u8; 32] {
        &self.epoch_digest
    }
}

/// Retains exclusion only. Drop performs no I/O, cleanup, allocation or replay.
pub(crate) struct RecoverySelection<W: WriterNamespace> {
    writer: W,
    observation: RecoverySelectionObservation,
}

/// Source ownership allowance, excluding adapter backing storage and caller pages.
/// This is neither an ABI stack estimate nor a BOOT_RESERVE reservation.
pub(crate) struct SourceBudget {
    pub(crate) owner: usize,
    pub(crate) file: usize,
    pub(crate) outer: usize,
    pub(crate) preparation_nested: usize,
    pub(crate) reader_nested: usize,
    pub(crate) preparation: usize,
    pub(crate) reader: usize,
    pub(crate) io: usize,
    pub(crate) peak: usize,
}
pub(crate) fn source_budget<W: WriterNamespace>() -> SourceBudget {
    use std::mem::size_of;
    let owner = size_of::<RecoverySelection<W>>();
    // Explicit outer type carriers join the existing inventoried bootstrap and
    // FILEMAP/codec/hash nested calls. Caller images remain separately borrowed.
    let outer = outer_bytes::<W>();
    let preparation_nested = wal_io::recovery_preparation_bytes::<W>();
    let reader_nested = wal_io::recovery_reader_bytes::<W>();
    let preparation = owner + preparation_nested + outer;
    let reader = size_of::<W>()
        + size_of::<PageFileReader<'_, CheckedNamespace<'_, W>>>()
        + size_of::<W::File>()
        + reader_nested
        + outer;
    let io = owner + size_of::<W::File>() + reader_nested + outer;
    SourceBudget {
        owner,
        file: size_of::<W::File>(),
        outer,
        preparation_nested,
        reader_nested,
        preparation,
        reader,
        io,
        peak: preparation.max(reader).max(io),
    }
}
fn outer_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    // acquire/read_selector/open_named coexist only along one synchronous call
    // chain; output page storage stays borrowed. Include returned/error copies,
    // all filename and opened/named metadata values, wrapper and observation.
    8 * size_of::<FileName>()
        + 8 * size_of::<FileMetadata>()
        + 6 * size_of::<Profile>()
        + 6 * size_of::<SelectedControl>()
        + 4 * size_of::<RecoverySelectionObservation>()
        + 8 * size_of::<RecoverySelectError>()
        + 4 * size_of::<std::result::Result<(W::File, FileMetadata), RecoverySelectError>>()
        + 4 * size_of::<[u8; 32]>()
        + size_of::<CheckedNamespace<'_, W>>()
        + 1024 // Bounded references/scalars/iterators and enum tags.
}
fn capacity<W: WriterNamespace>() -> bool {
    use std::mem::size_of;
    size_of::<W>() <= 16384
        && size_of::<W::File>() <= 16384
        && size_of::<RecoverySelection<W>>() <= 16384
        && source_budget::<W>().peak <= 65536
}

impl<W: WriterNamespace> RecoverySelection<W> {
    /// Acquire exclusion and inspect only the initial control closure. Scratch is
    /// borrowed for this call, remains invalid on error, and is reusable afterward.
    ///
    /// # Errors
    /// All later failures leave DB bytes unchanged and return no owner. Acquisition
    /// preserves the original namespace effect, including possible LOCK creation.
    pub(crate) fn acquire<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> Result<Self> {
        if !capacity::<W>() {
            return Err(error(Phase::Preflight, Cause::Capacity));
        }
        if expected.is_some_and(|value| value.timeline == 0) {
            return Err(error(
                Phase::Preflight,
                Cause::Control(ControlError::InvalidInput),
            ));
        }
        let mut writer = directory
            .acquire_writer()
            .map_err(|cause| RecoverySelectError {
                phase: Phase::Lease,
                effect: cause.effect,
                cause: Cause::Namespace(cause),
            })?;
        for blocked in BLOCKED_NAMES {
            if writer
                .metadata(&name(blocked))
                .map_err(|e| error(Phase::Preflight, Cause::Namespace(e)))?
                .is_some()
            {
                return Err(error(Phase::Preflight, Cause::UnsupportedClosure));
            }
        }
        read_control(
            &mut writer,
            "d3-profile",
            &mut scratch[0][..control::PROFILE_BYTES],
            Phase::Profile,
        )?;
        let profile =
            control::decode_bootstrap_profile(&scratch[0][..control::PROFILE_BYTES], expected)
                .map_err(|e| error(Phase::Profile, Cause::Control(e)))?;
        if profile.spec().generation != 1 || profile.spec().namespace.timeline != 1 {
            return Err(error(Phase::Profile, Cause::UnsupportedClosure));
        }
        let filemap = read_selector(
            &mut writer,
            "FILEMAP",
            SelectorRole::FileMap,
            profile,
            &mut scratch[0],
            Phase::FileMap,
        )?;
        initial_tuple(filemap, ControlKind::FileMapRoot, 320, Phase::FileMap)?;
        {
            let [root, work, _] = scratch;
            let mut checked = CheckedNamespace(&mut writer);
            let reader =
                PageFileReader::open(&mut checked, Some(profile.spec().namespace), root, work)
                    .map_err(|e| error(Phase::FileMap, Cause::PageIo(e)))?;
            if reader.profile() != profile || reader.selected() != filemap {
                return Err(error(Phase::FileMap, Cause::IdentityMismatch));
            }
        }
        read_control(
            &mut writer,
            "d3-bootstrap",
            &mut scratch[0][..wal_io::CONTROL_BYTES],
            Phase::Bootstrap,
        )?;
        let initial =
            wal_io::inspect_initial_bootstrap(profile, filemap, scratch).map_err(|e| {
                let cause = match e {
                    InitialRecoveryError::Bootstrap(e) => match &e.cause {
                        BootstrapCause::UnsupportedPlan => Cause::UnsupportedBootstrapPlan,
                        BootstrapCause::IdentityMismatch => Cause::IdentityMismatch,
                        BootstrapCause::Control(cause) => Cause::Control(*cause),
                        _ => Cause::Bootstrap(e),
                    },
                    InitialRecoveryError::Doublewrite(cause) => Cause::Doublewrite(cause),
                };
                error(Phase::Bootstrap, cause)
            })?;
        let doublewrite = read_selector(
            &mut writer,
            "DWB",
            SelectorRole::Doublewrite,
            profile,
            &mut scratch[0],
            Phase::Doublewrite,
        )?;
        initial_tuple(
            doublewrite,
            ControlKind::Doublewrite,
            160,
            Phase::Doublewrite,
        )?;
        {
            let mut file = open_exact(
                &mut writer,
                "d3-dwb-0",
                PAGE_BYTES as u64,
                Phase::Doublewrite,
            )?;
            read_exact_at(&mut file, &mut scratch[0][..160], 0)
                .map_err(|e| error(Phase::Doublewrite, Cause::Io(e)))?;
            let view = control::decode_selected_doublewrite(
                &scratch[0][..160],
                doublewrite.reference,
                profile,
            )
            .map_err(|e| error(Phase::Doublewrite, Cause::Control(e)))?;
            if view.header().state != ControlState::Empty || view.header().batch_id != 0 {
                return Err(error(Phase::Doublewrite, Cause::UnsupportedClosure));
            }
            if doublewrite != initial.doublewrite {
                return Err(error(Phase::Doublewrite, Cause::IdentityMismatch));
            }
        }
        read_control(
            &mut writer,
            "d3-dwb-stage",
            &mut scratch[0],
            Phase::Doublewrite,
        )?;
        if &scratch[0][..4] == b"XDW1" {
            return Err(error(Phase::Doublewrite, Cause::UnsupportedClosure));
        }
        if content_digest(&scratch[0]) != initial.stage_digest {
            return Err(error(
                Phase::Doublewrite,
                Cause::Control(ControlError::Corrupt),
            ));
        }
        for (path, length) in GEOMETRY {
            drop(open_exact(&mut writer, path, length, Phase::Doublewrite)?);
        }
        let epoch_digest = {
            let (mut file, metadata) = open_checked(&mut writer, wal_io::WAL_NAME, Phase::Epoch)?;
            if metadata.length < wal_io::CONTROL_BYTES as u64 {
                return Err(error(Phase::Epoch, Cause::LengthMismatch));
            }
            if metadata.length > wal_io::CONTROL_BYTES as u64 {
                return Err(error(Phase::Epoch, Cause::UnsupportedClosure));
            }
            read_exact_at(&mut file, &mut scratch[0][..wal_io::CONTROL_BYTES], 0)
                .map_err(|e| error(Phase::Epoch, Cause::Io(e)))?;
            wal_io::decode_epoch(&scratch[0][..wal_io::CONTROL_BYTES], profile)
                .map_err(|e| error(Phase::Epoch, Cause::Control(e)))?
        };
        read_control(
            &mut writer,
            "WRITER",
            &mut scratch[0][..wal_io::CONTROL_BYTES],
            Phase::Writer,
        )?;
        wal_io::decode_writer(&scratch[0][..wal_io::CONTROL_BYTES], profile, epoch_digest)
            .map_err(|e| error(Phase::Writer, Cause::Control(e)))?;
        Ok(Self {
            writer,
            observation: RecoverySelectionObservation {
                namespace: profile.spec().namespace,
                profile: profile.spec().id,
                plan: initial.plan,
                filemap: initial.filemap.generation,
                doublewrite: doublewrite.generation,
                epoch_digest,
            },
        })
    }

    pub(crate) fn observation(&self) -> &RecoverySelectionObservation {
        &self.observation
    }
}

fn initial_tuple(
    selected: SelectedControl,
    kind: ControlKind,
    length: u32,
    phase: Phase,
) -> Result<()> {
    let reference = selected.reference;
    if selected.generation != 1
        || reference.kind != kind
        || reference.slot != 0
        || reference.index != 0
        || reference.generation != 1
        || reference.encoded_len != length
    {
        return Err(error(phase, Cause::UnsupportedClosure));
    }
    Ok(())
}
fn read_selector<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    role: SelectorRole,
    profile: Profile,
    scratch: &mut [u8; PAGE_BYTES],
    phase: Phase,
) -> Result<SelectedControl> {
    read_control(writer, path, &mut scratch[..control::SELECTOR_BYTES], phase)?;
    control::decode_selector(&scratch[..control::SELECTOR_BYTES], role, profile)
        .map_err(|e| error(phase, Cause::Control(e)))
}
fn open_checked<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    phase: Phase,
) -> Result<(W::File, FileMetadata)> {
    open_named(writer, path, phase, None)
}
fn open_named<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    phase: Phase,
    expected: Option<u64>,
) -> Result<(W::File, FileMetadata)> {
    let filename = name(path);
    let (file, opened) = writer
        .open_existing_with_metadata(&filename, OpenMode::ReadOnly)
        .map_err(|e| error(phase, Cause::Namespace(e)))?;
    if expected.is_some_and(|length| length != opened.length) {
        return Err(error(phase, Cause::LengthMismatch));
    }
    let named = writer
        .metadata(&filename)
        .map_err(|e| error(phase, Cause::Namespace(e)))?
        .ok_or_else(|| error(phase, Cause::IdentityMismatch))?;
    if named.identity != opened.identity {
        return Err(error(phase, Cause::IdentityMismatch));
    }
    if named.length != opened.length {
        return Err(error(phase, Cause::LengthMismatch));
    }
    Ok((file, opened))
}
fn open_exact<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    length: u64,
    phase: Phase,
) -> Result<W::File> {
    let (file, _) = open_named(writer, path, phase, Some(length))?;
    Ok(file)
}
fn read_control<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    bytes: &mut [u8],
    phase: Phase,
) -> Result<()> {
    let mut file = open_exact(writer, path, bytes.len() as u64, phase)?;
    read_exact_at(&mut file, bytes, 0).map_err(|e| error(phase, Cause::Io(e)))
}

/// Tightens opened/named identity checking for the existing FILEMAP reader.
/// All unsupported effects fail typed and unchanged; no hidden forwarding effect.
struct CheckedNamespace<'a, W: WriterNamespace>(&'a mut W);
fn forbidden() -> NamespaceError {
    NamespaceError::new(
        NamespaceErrorKind::InvalidName,
        Effect::Unchanged,
        "read-only recovery selection",
    )
}
impl<W: WriterNamespace> WriterNamespace for CheckedNamespace<'_, W> {
    type File = W::File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        Err(forbidden())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        Err(forbidden())
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<Self::File> {
        self.open_existing_with_metadata(name, mode)
            .map(|(file, _)| file)
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        if mode != OpenMode::ReadOnly {
            return Err(forbidden());
        }
        let (file, opened) = self.0.open_existing_with_metadata(name, mode)?;
        let expected = match name.as_str() {
            "d3-profile" | "FILEMAP" => control::PROFILE_BYTES as u64,
            "d3-filemap-0" | "d3-filemap-1" => control::CONTAINER_BYTES,
            _ => return Err(forbidden()),
        };
        if opened.length != expected {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "recovery fixed opened length mismatch",
            ));
        }
        let named = self.0.metadata(name)?.ok_or_else(forbidden)?;
        if named.identity != opened.identity {
            return Err(NamespaceError::new(
                NamespaceErrorKind::InvalidType,
                Effect::Unchanged,
                "recovery opened identity changed",
            ));
        }
        if named.length != opened.length {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "recovery opened length changed",
            ));
        }
        Ok((file, opened))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.0.metadata(name)
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        Err(forbidden())
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        Err(forbidden())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        Err(forbidden())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        Err(forbidden())
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "recovery_select_native_tests.rs"]
mod native;
#[cfg(test)]
#[path = "recovery_select_tests.rs"]
mod tests;
