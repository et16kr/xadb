//! Initial plan4 inspection retaining only writer exclusion. This owner cannot
//! enter analysis, redo, a mutable pool or READY.

use super::{
    BLOCKED_NAMES, Cause, Phase, RecoverySelectError, RecoverySelection,
    RecoverySelectionObservation, error, initial_tuple, name, open_exact, open_named, outer_bytes,
    read_control, read_selector,
};
use crate::{
    api::Namespace,
    storage::{
        control::{self, ControlError, ControlKind, SelectorRole},
        doublewrite_control::ControlState,
        page::{FileId, PAGE_BYTES, PageRef},
        wal_io,
    },
};
use foundation::{
    codec::{ContentHasher, content_digest},
    error::Effect,
    identity::Generation,
    io::read_exact_at,
    namespace::{DirectoryPort, FileMetadata, NamespaceError, WriterNamespace},
};

const MAINTENANCE_BYTES: usize = 12_582_912;
const STABLE_FILES: [(&str, u64); 14] = [
    ("d3-profile", 4096),
    (wal_io::WAL_NAME, 4096),
    ("WRITER", 4096),
    ("d3-bootstrap", 4096),
    ("FILEMAP", 4096),
    ("d3-data-0000000100000001", control::EXTENT_BYTES),
    ("d3-filemap-0", control::CONTAINER_BYTES),
    ("d3-filemap-1", control::CONTAINER_BYTES),
    ("DWB", 4096),
    ("d3-dwb-0", PAGE_BYTES as u64),
    ("d3-dwb-1", PAGE_BYTES as u64),
    ("d3-dwb-images", 16_711_680),
    ("d3-dwb-reserved", 8192),
    ("d3-dwb-stage", PAGE_BYTES as u64),
];
// Only authoritative bytes are content-stamped. Unselected DWB/FILEMAP banks
// and raw DWB slots have geometry/identity observations, not byte authority.
const CONTROL_FILES: [(&str, usize); 9] = [
    ("d3-profile", 4096),
    ("FILEMAP", 4096),
    ("d3-bootstrap", 4096),
    ("DWB", 4096),
    ("d3-dwb-0", 160),
    ("d3-dwb-stage", PAGE_BYTES),
    (wal_io::WAL_NAME, 4096),
    ("WRITER", 4096),
    ("d3-filemap-0", 320),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4SelectPhase {
    Preflight,
    Lease,
    Profile,
    FileMap,
    Bootstrap,
    Doublewrite,
    Epoch,
    Writer,
    Data,
    FinalRecheck,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4LeaseEffect {
    NotAttempted,
    AcquisitionFailed,
    AcquiredMayHaveChanged,
}
#[derive(Debug)]
pub(crate) enum Plan4SelectCause {
    Existing(RecoverySelectError),
    Genesis(wal_io::Plan4GenesisCause),
    Namespace(NamespaceError),
    Capacity,
    InvalidInput,
    UnsupportedClosure,
    IdentityMismatch,
}
#[derive(Debug)]
pub(crate) struct Plan4SelectError {
    pub(crate) phase: Plan4SelectPhase,
    pub(crate) effect: Effect,
    pub(crate) database_effect: Effect,
    pub(crate) lease_effect: Plan4LeaseEffect,
    pub(crate) cause: Plan4SelectCause,
}
type SelectionResult<T> = std::result::Result<T, Plan4SelectError>;
fn after_lease(phase: Plan4SelectPhase, cause: Plan4SelectCause) -> Plan4SelectError {
    Plan4SelectError {
        phase,
        effect: Effect::CallerMustResolve,
        database_effect: Effect::Unchanged,
        lease_effect: Plan4LeaseEffect::AcquiredMayHaveChanged,
        cause,
    }
}
fn existing(cause: RecoverySelectError) -> Plan4SelectError {
    let phase = match cause.phase {
        Phase::Preflight => Plan4SelectPhase::Preflight,
        Phase::Lease => Plan4SelectPhase::Lease,
        Phase::Profile => Plan4SelectPhase::Profile,
        Phase::FileMap => Plan4SelectPhase::FileMap,
        Phase::Bootstrap => Plan4SelectPhase::Bootstrap,
        Phase::Doublewrite => Plan4SelectPhase::Doublewrite,
        Phase::Epoch => Plan4SelectPhase::Epoch,
        Phase::Writer => Plan4SelectPhase::Writer,
    };
    after_lease(phase, Plan4SelectCause::Existing(cause))
}

pub(crate) struct Plan4GenesisSelection<W: WriterNamespace> {
    inner: RecoverySelection<W>,
}
pub(crate) struct Plan4GenesisObservation<'a> {
    inner: &'a RecoverySelectionObservation,
}
impl Plan4GenesisObservation<'_> {
    pub(crate) fn namespace(&self) -> Namespace {
        self.inner.namespace()
    }
    pub(crate) fn profile_id(&self) -> u32 {
        self.inner.profile_id()
    }
    pub(crate) fn bootstrap_plan(&self) -> u64 {
        self.inner.bootstrap_plan()
    }
    pub(crate) fn filemap_generation(&self) -> u64 {
        self.inner.filemap_generation()
    }
    pub(crate) fn doublewrite_generation(&self) -> u64 {
        self.inner.doublewrite_generation()
    }
    pub(crate) fn epoch_digest(&self) -> &[u8; 32] {
        self.inner.epoch_digest()
    }
    pub(crate) fn issued_object_high_water(&self) -> u64 {
        5
    }
    pub(crate) fn allocated_pages(&self) -> usize {
        9
    }
    pub(crate) fn free_pages(&self) -> usize {
        55
    }
    pub(crate) fn free_jobs(&self) -> usize {
        64
    }
    pub(crate) fn catalog_entries(&self) -> usize {
        0
    }
    pub(crate) fn catalog_anchor(&self) -> PageRef {
        catalog_page(7)
    }
    pub(crate) fn catalog_root(&self) -> PageRef {
        catalog_page(8)
    }
}
fn catalog_page(page_no: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0001).expect("fixed genesis file"),
        page_no,
        incarnation: Generation::new(1).expect("fixed genesis incarnation"),
    }
}

/// Source-owned values, excluding borrowed maintenance and adapter backing.
/// Common nested bounds retain their existing separate 16KiB limits.
pub(crate) struct Plan4SelectionSourceCensus {
    pub(crate) owner: usize,
    pub(crate) file: usize,
    pub(crate) outer: usize,
    pub(crate) preparation_nested: usize,
    pub(crate) reader_nested: usize,
    pub(crate) seed_preparation_nested: usize,
    pub(crate) seed_reader_nested: usize,
    pub(crate) peak: usize,
}
pub(crate) fn source_census<W: WriterNamespace>() -> Plan4SelectionSourceCensus {
    use std::mem::size_of;
    let owner = size_of::<Plan4GenesisSelection<W>>();
    let file = size_of::<W::File>();
    let outer = outer_bytes::<W>()
        + 2 * size_of::<[FileMetadata; 14]>()
        + 2 * size_of::<[[u8; 32]; 9]>()
        + 2 * size_of::<wal_io::Plan4GenesisSeed>()
        + 8 * size_of::<Plan4SelectError>()
        + 2 * size_of::<SelectionResult<Plan4GenesisSelection<W>>>()
        + 2 * size_of::<Plan4GenesisObservation<'_>>()
        + 4 * size_of::<ContentHasher>()
        + 2048;
    let preparation_nested = wal_io::recovery_preparation_bytes::<W>();
    let reader_nested = wal_io::recovery_reader_bytes::<W>();
    // New seed closures include their allocation rows, queue/tree codecs and
    // local result/file carriers. Old nested limits remain separately visible.
    let seed = wal_io::plan4_genesis_source_census::<W>();
    let seed_preparation_nested = seed.prepare_nested;
    let seed_reader_nested = seed.readback_nested;
    let nested = preparation_nested
        .max(reader_nested)
        .max(seed_preparation_nested)
        .max(seed_reader_nested);
    let peak = owner + 2 * file + outer + nested;
    Plan4SelectionSourceCensus {
        owner,
        file,
        outer,
        preparation_nested,
        reader_nested,
        seed_preparation_nested,
        seed_reader_nested,
        peak,
    }
}

impl<W: WriterNamespace> Plan4GenesisSelection<W> {
    pub(crate) fn acquire<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        maintenance: &mut [u8; MAINTENANCE_BYTES],
    ) -> SelectionResult<Self> {
        let census = source_census::<W>();
        let cause = if !wal_io::plan4_genesis_capacity::<W>()
            || std::mem::size_of::<W>() > 16384
            || census.owner > 16384
            || census.file > 16384
            || census.peak > 65536
        {
            Some(Plan4SelectCause::Capacity)
        } else if expected.is_some_and(|namespace| namespace.timeline == 0) {
            Some(Plan4SelectCause::InvalidInput)
        } else {
            None
        };
        if let Some(cause) = cause {
            return Err(Plan4SelectError {
                phase: Plan4SelectPhase::Preflight,
                effect: Effect::Unchanged,
                database_effect: Effect::Unchanged,
                lease_effect: Plan4LeaseEffect::NotAttempted,
                cause,
            });
        }
        let mut writer = directory
            .acquire_writer()
            .map_err(|cause| Plan4SelectError {
                phase: Plan4SelectPhase::Lease,
                effect: cause.effect,
                database_effect: Effect::Unchanged,
                lease_effect: Plan4LeaseEffect::AcquisitionFailed,
                cause: Plan4SelectCause::Namespace(cause),
            })?;
        let (pages, _) = maintenance[..3 * PAGE_BYTES].as_chunks_mut::<PAGE_BYTES>();
        let scratch: &mut [[u8; PAGE_BYTES]; 3] = pages.try_into().expect("fixed three-page loan");
        reject_blocked(&writer)?;
        let metadata = metadata_snapshot(&mut writer)?;
        let controls = control_snapshot(&mut writer, &mut scratch[0])?;
        let observation = inspect(&mut writer, expected, scratch)?;
        if control_snapshot(&mut writer, &mut scratch[0])? != controls
            || metadata_snapshot(&mut writer)? != metadata
        {
            return Err(after_lease(
                Plan4SelectPhase::FinalRecheck,
                Plan4SelectCause::IdentityMismatch,
            ));
        }
        reject_blocked(&writer)?;
        Ok(Self {
            inner: RecoverySelection {
                writer,
                observation,
            },
        })
    }
    pub(crate) fn observation(&self) -> Plan4GenesisObservation<'_> {
        Plan4GenesisObservation {
            inner: &self.inner.observation,
        }
    }
}

fn reject_blocked<W: WriterNamespace>(writer: &W) -> SelectionResult<()> {
    for path in BLOCKED_NAMES {
        if writer
            .metadata(&name(path))
            .map_err(|e| after_lease(Plan4SelectPhase::Preflight, Plan4SelectCause::Namespace(e)))?
            .is_some()
        {
            return Err(after_lease(
                Plan4SelectPhase::Preflight,
                Plan4SelectCause::UnsupportedClosure,
            ));
        }
    }
    Ok(())
}
fn metadata_snapshot<W: WriterNamespace>(writer: &mut W) -> SelectionResult<[FileMetadata; 14]> {
    // The first actual metadata initializes fixed storage; no invented identity
    // or heap allocation can be mistaken for a successful file observation.
    let (_, first) = open_named(
        writer,
        STABLE_FILES[0].0,
        Phase::Preflight,
        Some(STABLE_FILES[0].1),
    )
    .map_err(existing)?;
    let mut result = [first; 14];
    for (index, (path, length)) in STABLE_FILES.into_iter().enumerate().skip(1) {
        let (_, metadata) =
            open_named(writer, path, Phase::Preflight, Some(length)).map_err(existing)?;
        result[index] = metadata;
    }
    Ok(result)
}
fn control_snapshot<W: WriterNamespace>(
    writer: &mut W,
    scratch: &mut [u8; PAGE_BYTES],
) -> SelectionResult<[[u8; 32]; 9]> {
    let mut result = [[0; 32]; 9];
    for (index, (path, length)) in CONTROL_FILES.into_iter().enumerate() {
        let (mut file, _) = open_named(writer, path, Phase::Preflight, None).map_err(existing)?;
        read_exact_at(&mut file, &mut scratch[..length], 0)
            .map_err(|e| existing(error(Phase::Preflight, Cause::Io(e))))?;
        let mut hasher = ContentHasher::new();
        hasher.update(&scratch[..length]);
        if path == "d3-filemap-0" {
            read_exact_at(&mut file, &mut scratch[..256], PAGE_BYTES as u64)
                .map_err(|e| existing(error(Phase::FileMap, Cause::Io(e))))?;
            hasher.update(&scratch[..256]);
        }
        result[index] = hasher.digest();
    }
    Ok(result)
}

fn inspect<W: WriterNamespace>(
    writer: &mut W,
    expected: Option<Namespace>,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> SelectionResult<RecoverySelectionObservation> {
    read_control(
        writer,
        "d3-profile",
        &mut scratch[0][..control::PROFILE_BYTES],
        Phase::Profile,
    )
    .map_err(existing)?;
    let profile =
        control::decode_bootstrap_profile(&scratch[0][..control::PROFILE_BYTES], expected)
            .map_err(|e| existing(error(Phase::Profile, Cause::Control(e))))?;
    if profile.spec().generation != 1 || profile.spec().namespace.timeline != 1 {
        return Err(after_lease(
            Plan4SelectPhase::Profile,
            Plan4SelectCause::UnsupportedClosure,
        ));
    }
    let filemap = read_selector(
        writer,
        "FILEMAP",
        SelectorRole::FileMap,
        profile,
        &mut scratch[0],
        Phase::FileMap,
    )
    .map_err(existing)?;
    initial_tuple(filemap, ControlKind::FileMapRoot, 320, Phase::FileMap).map_err(existing)?;
    read_control(
        writer,
        "d3-bootstrap",
        &mut scratch[0][..wal_io::CONTROL_BYTES],
        Phase::Bootstrap,
    )
    .map_err(existing)?;
    let seed = wal_io::inspect_plan4_bootstrap(profile, filemap, scratch)
        .map_err(|e| after_lease(Plan4SelectPhase::Bootstrap, Plan4SelectCause::Genesis(e)))?;
    let initial = seed
        .initial_recovery_expectation(&mut scratch[0])
        .map_err(|e| after_lease(Plan4SelectPhase::Doublewrite, Plan4SelectCause::Genesis(e)))?;
    let doublewrite = read_selector(
        writer,
        "DWB",
        SelectorRole::Doublewrite,
        profile,
        &mut scratch[0],
        Phase::Doublewrite,
    )
    .map_err(existing)?;
    initial_tuple(
        doublewrite,
        ControlKind::Doublewrite,
        160,
        Phase::Doublewrite,
    )
    .map_err(existing)?;
    {
        let mut file = open_exact(writer, "d3-dwb-0", PAGE_BYTES as u64, Phase::Doublewrite)
            .map_err(existing)?;
        read_exact_at(&mut file, &mut scratch[0][..160], 0)
            .map_err(|e| existing(error(Phase::Doublewrite, Cause::Io(e))))?;
        let view = control::decode_selected_doublewrite(
            &scratch[0][..160],
            doublewrite.reference,
            profile,
        )
        .map_err(|e| existing(error(Phase::Doublewrite, Cause::Control(e))))?;
        if view.header().state != ControlState::Empty
            || view.header().batch_id != 0
            || doublewrite != initial.doublewrite
        {
            return Err(after_lease(
                Plan4SelectPhase::Doublewrite,
                Plan4SelectCause::UnsupportedClosure,
            ));
        }
    }
    read_control(writer, "d3-dwb-stage", &mut scratch[0], Phase::Doublewrite).map_err(existing)?;
    if content_digest(&scratch[0]) != initial.stage_digest {
        return Err(existing(error(
            Phase::Doublewrite,
            Cause::Control(ControlError::Corrupt),
        )));
    }
    read_control(
        writer,
        wal_io::WAL_NAME,
        &mut scratch[0][..wal_io::CONTROL_BYTES],
        Phase::Epoch,
    )
    .map_err(existing)?;
    let epoch_digest = wal_io::decode_epoch(&scratch[0][..wal_io::CONTROL_BYTES], profile)
        .map_err(|e| existing(error(Phase::Epoch, Cause::Control(e))))?;
    read_control(
        writer,
        "WRITER",
        &mut scratch[0][..wal_io::CONTROL_BYTES],
        Phase::Writer,
    )
    .map_err(existing)?;
    wal_io::decode_writer(&scratch[0][..wal_io::CONTROL_BYTES], profile, epoch_digest)
        .map_err(|e| existing(error(Phase::Writer, Cause::Control(e))))?;
    seed.verify_data(writer, scratch)
        .map_err(|e| after_lease(Plan4SelectPhase::Data, Plan4SelectCause::Genesis(e)))?;
    Ok(RecoverySelectionObservation {
        namespace: profile.spec().namespace,
        profile: profile.spec().id,
        plan: initial.plan,
        filemap: filemap.generation,
        doublewrite: doublewrite.generation,
        epoch_digest,
    })
}

#[cfg(test)]
#[path = "plan4_recovery_select_tests.rs"]
mod tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "plan4_genesis_native_tests.rs"]
pub(in crate::storage) mod native;
