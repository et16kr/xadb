//! Read-only recovery assembly of the current genesis producer prefix.
//! Canonical RAM before images come from authenticated bootstrap/prior WAL,
//! never mixed physical homes. No MutableStorageOwner, live role or READY is
//! reconstructed. See devdoc/contracts/dev3-recovery-prefix.md.
#![allow(dead_code)] // Closed analysis precedes the separately owned recovery barriers.

use super::super::super::super as wal;
use super::super::super::{self as bootstrap, BootstrapError, Cause as BootstrapCause};
use super::{
    super::allocation_reservation::{
        budget,
        operation::{self, AnalysisExpected, Batch, OperationBudget, SemanticError},
    },
    status_init_operation as init,
};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{self, AllocationEntry, ReservationOwner, SystemWorkKind, SystemWorkRef},
        control::{
            self, ControlError, ControlKind, ControlRef, Profile, SelectedControl, SelectorRole,
        },
        doublewrite_control::{self as dwb, ControlHeader, ControlState, PendingImage},
        obligation::{self, DescriptorSpec, RemainingBudget},
        page::{PAGE_BYTES, PageIdentity, PageKind, PageRef},
        page_io::{PageFileReader, PageIoError},
        wal_frame::{
            self, FrameExpectation, FrameKind, SemanticFrameScan, TailPolicy, WalError,
            WalFrameHeader, WalStreamIdentity,
        },
    },
};
use std::mem::size_of;
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    error::Effect,
    identity::{Generation, ObjectId},
    io::read_exact_at,
    namespace::{
        DirectoryEntry, DirectoryPort, FileIdentity, FileMetadata, FileName, NamespaceError,
        NamespaceErrorKind, NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};

pub(crate) const WORKSPACE_BYTES: usize = 262144;
const FRAME: usize = 65536;
const ALLOC: usize = 65536;
const AFTER_ALLOC: usize = 81920;
const STATUS: usize = 98304;
const AFTER_STATUS: usize = 114688;
const BOOT: usize = 131072;
const DESCRIPTOR: usize = 180224;
const MAX_SUPPORTED_FRAME: usize = 38000;
const BLOCKED: [&str; 6] = [
    "CURRENT",
    "d3-current-stage",
    "d3-rebase-0",
    "d3-rebase-1",
    "d3-writer-stage",
    "d3-filemap-select-stage",
];
const GEOMETRY: [(&str, u64); 8] = [
    ("d3-data-0000000100000001", control::EXTENT_BYTES),
    ("d3-filemap-0", control::CONTAINER_BYTES),
    ("d3-filemap-1", control::CONTAINER_BYTES),
    ("d3-dwb-0", PAGE_BYTES as u64),
    ("d3-dwb-1", PAGE_BYTES as u64),
    ("d3-dwb-stage", PAGE_BYTES as u64),
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
    Scan,
    Semantic,
    Ledger,
    Snapshot,
}
#[derive(Debug)]
pub(crate) enum Cause {
    Capacity,
    UnsupportedClosure,
    UnsupportedBootstrapPlan,
    RequiredPrefixMissing,
    LengthMismatch,
    IdentityMismatch,
    ArithmeticOverflow,
    Namespace(NamespaceError),
    Io(xadb_foundation::Error),
    Control(ControlError),
    PageIo(PageIoError),
    Bootstrap(BootstrapError),
    Doublewrite(dwb::DoublewriteError),
    Wal(WalError),
    Semantic(SemanticError),
}
#[derive(Debug)]
pub(crate) struct RecoveryPrefixError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, RecoveryPrefixError>;
fn error(phase: Phase, cause: Cause) -> RecoveryPrefixError {
    RecoveryPrefixError {
        phase,
        effect: Effect::Unchanged,
        cause,
    }
}
fn semantic(cause: SemanticError) -> RecoveryPrefixError {
    error(Phase::Semantic, Cause::Semantic(cause))
}
fn arithmetic() -> RecoveryPrefixError {
    error(Phase::Ledger, Cause::ArithmeticOverflow)
}
fn name(value: &str) -> FileName {
    FileName::new(value).expect("bounded constant prefix filename")
}
fn get16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("validated short field"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProducerMode {
    Exclusive,
    Dirty,
    Status,
}
impl ProducerMode {
    pub(crate) fn memory(self) -> u64 {
        self.budget().memory()
    }
    fn budget(self) -> OperationBudget {
        match self {
            Self::Exclusive => OperationBudget::Exclusive81920,
            Self::Dirty => OperationBudget::Dirty98304,
            Self::Status => OperationBudget::StatusInit114688,
        }
    }
}
pub(crate) struct TailObservation {
    offset: u64,
    physical_bytes: u64,
    declared_bytes: Option<u64>,
    inspected_bytes: usize,
}
impl TailObservation {
    pub(crate) fn offset(&self) -> u64 {
        self.offset
    }
    pub(crate) fn physical_bytes(&self) -> u64 {
        self.physical_bytes
    }
    pub(crate) fn declared_bytes(&self) -> Option<u64> {
        self.declared_bytes
    }
    pub(crate) fn inspected_bytes(&self) -> usize {
        self.inspected_bytes
    }
}
/// Borrowed observations have no Clone/Copy or resource-release capability.
pub(crate) struct RecoveryPrefixObservation {
    profile: Profile,
    plan: u64,
    filemap: SelectedControl,
    doublewrite: SelectedControl,
    dwb_header: ControlHeader,
    pending: Option<PendingImage>,
    source: FileMetadata,
    complete_offset: u64,
    complete_lsn: u64,
    tail: Option<TailObservation>,
    mode: Option<ProducerMode>,
    active: Option<Batch>,
    high_water: u64,
    inventory_items: u64,
    inventory_digest: [u8; 32],
    remaining: RemainingBudget,
    retained: u64,
    owned: u64,
    protected: u64,
    terminal_hold: u64,
    status_initialized: bool,
    pending_matched: bool,
    stage_bytes: u64,
    replay_topup: u64,
}
impl RecoveryPrefixObservation {
    pub(crate) fn namespace(&self) -> Namespace {
        self.profile.spec().namespace
    }
    pub(crate) fn profile_id(&self) -> u32 {
        self.profile.spec().id
    }
    pub(crate) fn bootstrap_plan(&self) -> u64 {
        self.plan
    }
    pub(crate) fn filemap_generation(&self) -> u64 {
        self.filemap.generation
    }
    pub(crate) fn doublewrite_generation(&self) -> u64 {
        self.doublewrite.generation
    }
    pub(crate) fn doublewrite_state(&self) -> ControlState {
        self.dwb_header.state
    }
    pub(crate) fn doublewrite_batch(&self) -> u64 {
        self.dwb_header.batch_id
    }
    pub(crate) fn pending(&self) -> Option<&PendingImage> {
        self.pending.as_ref()
    }
    pub(crate) fn pending_lsn(&self) -> Option<u64> {
        self.pending.map(|p| p.lsn.0)
    }
    pub(crate) fn source_identity(&self) -> FileIdentity {
        self.source.identity
    }
    pub(crate) fn source_length(&self) -> u64 {
        self.source.length
    }
    pub(crate) fn complete_offset(&self) -> u64 {
        self.complete_offset
    }
    pub(crate) fn complete_lsn(&self) -> u64 {
        self.complete_lsn
    }
    pub(crate) fn tail(&self) -> Option<&TailObservation> {
        self.tail.as_ref()
    }
    pub(crate) fn mode(&self) -> Option<ProducerMode> {
        self.mode
    }
    pub(crate) fn producer_mode(&self) -> Option<u64> {
        self.mode.map(ProducerMode::memory)
    }
    pub(crate) fn active_owner(&self) -> Option<SystemWorkRef> {
        self.active.map(|b| b.owner)
    }
    pub(crate) fn active_mask(&self) -> u64 {
        self.active.map_or(0, |b| b.mask)
    }
    pub(crate) fn active_birth(&self) -> Option<u64> {
        self.active.map(|b| b.birth)
    }
    pub(crate) fn work_high_water(&self) -> u64 {
        self.high_water
    }
    pub(crate) fn inventory_items(&self) -> u64 {
        self.inventory_items
    }
    pub(crate) fn inventory_digest(&self) -> &[u8; 32] {
        &self.inventory_digest
    }
    pub(crate) fn retained_wal_bytes(&self) -> u64 {
        self.retained
    }
    pub(crate) fn remaining_wal(&self) -> u64 {
        self.remaining.wal
    }
    pub(crate) fn remaining_pages(&self) -> u64 {
        self.remaining.pages
    }
    pub(crate) fn remaining_frames(&self) -> u64 {
        self.remaining.peak.frames
    }
    pub(crate) fn remaining_memory(&self) -> u64 {
        self.remaining.peak.memory
    }
    pub(crate) fn remaining_files(&self) -> u64 {
        self.remaining.peak.files
    }
    pub(crate) fn remaining_protected(&self) -> u64 {
        self.remaining.protected
    }
    pub(crate) fn protected_bytes(&self) -> u64 {
        self.protected
    }
    pub(crate) fn owned_bytes(&self) -> u64 {
        self.owned
    }
    pub(crate) fn terminal_hold_mask(&self) -> u64 {
        self.terminal_hold
    }
    pub(crate) fn replay_page_topup_bytes(&self) -> u64 {
        self.replay_topup
    }
    pub(crate) fn source_stabilized(&self) -> bool {
        false
    }
}
/// Source closes before the namespace owner. Drop performs no sync/cancel/repair.
pub(crate) struct RecoveryPrefix<'a, W: WriterNamespace> {
    source: W::File,
    writer: W,
    workspace: &'a mut [u8; WORKSPACE_BYTES],
    observation: RecoveryPrefixObservation,
}

pub(crate) struct SourceCensus {
    pub(crate) outer: usize,
    pub(crate) bootstrap: usize,
    pub(crate) replay: usize,
    pub(crate) stream: usize,
    pub(crate) peak: usize,
}
pub(crate) fn source_census<W: WriterNamespace>() -> SourceCensus {
    // Move-only owner includes W, retained WAL File and the observation once.
    // Add the one temporary File and four open/check return carriers; control
    // iterations never retain multiple temporary files. Error/Copy pools cover
    // acquire -> read_selector/read_control -> open_exact/open_checked -> port.
    let outer = size_of::<RecoveryPrefix<'_, W>>()
        + size_of::<W::File>()
        + 8 * size_of::<RecoveryPrefixError>()
        + 4 * size_of::<Result<(W::File, FileMetadata)>>()
        + 8 * size_of::<FileName>()
        + 8 * size_of::<FileMetadata>()
        + 8 * size_of::<Profile>()
        + 8 * size_of::<SelectedControl>()
        + 4 * size_of::<PendingImage>()
        + 4 * size_of::<ControlHeader>()
        + 4 * size_of::<[u8; 32]>() // epoch/XBS1/control digests across return carriers
        + 4 * size_of::<WalFrameHeader>()
        + 4 * size_of::<Batch>()
        + 4 * size_of::<FrameExpectation>()
        + 64 * size_of::<u64>()
        + 32 * size_of::<&[u8]>();
    // The existing typed bootstrap pool includes its two BootstrapPlans, six
    // maximum Result carriers and 64 AllocationEntries. Its result depth is
    // preserved by prepare_recovery_bootstrap (see that helper's census map).
    // PageFileReader and its File are included only in the separate reader arm.
    let bootstrap = wal::recovery_preparation_bytes::<W>().max(
        wal::recovery_reader_bytes::<W>()
            + size_of::<PageFileReader<'_, CheckedNamespace<'_, W>>>(),
    ) + 2
        * size_of::<std::result::Result<bootstrap::BootstrapPlan, BootstrapError>>();
    // analyze -> adopt retains one decoded descriptor; shared validation may
    // decode another. Their entry/descriptor/codec depth is budget::TEMPORARIES.
    // Conservatively add the Status codec pool despite its nonoverlap, plus
    // pure Expected arguments, latest/returned descriptor and RAM image borrows.
    let replay = budget::TEMPORARIES
        + budget::PLANS
        + init::status_codec_bytes()
        + 2 * size_of::<init::InitExpected<'static>>()
        + 2 * size_of::<DescriptorSpec>()
        + size_of::<RemainingBudget>()
        + 2 * size_of::<CanonicalImages<'static>>();
    // Oversized hashing never nests a descriptor/entry/bootstrap decoder.
    // Hash chunk bytes live in caller workspace; pinned SHA state/block/output
    // storage and digest/scalar/error return carriers are source-owned here.
    let stream = 4 * size_of::<ContentHasher>() + 1536 + 4 * size_of::<[u8; 32]>() + 1024;
    SourceCensus {
        outer,
        bootstrap,
        replay,
        stream,
        peak: outer + bootstrap.max(replay).max(stream),
    }
}
fn fits<W: WriterNamespace>() -> bool {
    size_of::<W>() <= 16384 && size_of::<W::File>() <= 16384 && source_census::<W>().peak <= 65536
}

impl<'a, W: WriterNamespace> RecoveryPrefix<'a, W> {
    /// Borrow all workspace for a read-only, canonical prefix analysis. Acquisition
    /// errors retain the original namespace effect; all later errors are unchanged.
    /// The incomplete suffix and source bytes are never truncated or rewritten.
    pub(crate) fn acquire<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        workspace: &'a mut [u8; WORKSPACE_BYTES],
    ) -> Result<Self> {
        let mut acquired = false;
        Self::acquire_internal(directory, expected, workspace, false, &mut acquired)
    }
    fn acquire_for_effect<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        workspace: &'a mut [u8; WORKSPACE_BYTES],
        acquired: &mut bool,
    ) -> Result<Self> {
        Self::acquire_internal(directory, expected, workspace, true, acquired)
    }
    fn acquire_internal<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        workspace: &'a mut [u8; WORKSPACE_BYTES],
        reserved_only: bool,
        acquired: &mut bool,
    ) -> Result<Self> {
        if !fits::<W>() {
            return Err(error(Phase::Preflight, Cause::Capacity));
        }
        if expected.is_some_and(|n| n.timeline == 0) {
            return Err(error(
                Phase::Preflight,
                Cause::Control(ControlError::InvalidInput),
            ));
        }
        let mut writer = directory
            .acquire_writer()
            .map_err(|cause| RecoveryPrefixError {
                phase: Phase::Lease,
                effect: cause.effect,
                cause: Cause::Namespace(cause),
            })?;
        *acquired = true;
        for path in BLOCKED {
            if writer
                .metadata(&name(path))
                .map_err(|e| error(Phase::Preflight, Cause::Namespace(e)))?
                .is_some()
                && !(reserved_only && matches!(path, "d3-rebase-0" | "d3-rebase-1"))
            {
                return Err(error(Phase::Preflight, Cause::UnsupportedClosure));
            }
        }
        let (profile, filemap, plan, allocator) = {
            let (pages, _) = workspace[BOOT..BOOT + 3 * PAGE_BYTES].as_chunks_mut::<PAGE_BYTES>();
            let scratch: &mut [[u8; PAGE_BYTES]; 3] =
                pages.try_into().expect("fixed bootstrap pages");
            read_control(
                &mut writer,
                "d3-profile",
                &mut scratch[0][..4096],
                Phase::Profile,
            )?;
            let profile = control::decode_bootstrap_profile(&scratch[0][..4096], expected)
                .map_err(|e| error(Phase::Profile, Cause::Control(e)))?;
            if profile.spec().generation != 1 || profile.spec().namespace.timeline != 1 {
                return Err(error(Phase::Profile, Cause::UnsupportedClosure));
            }
            let selected = read_selector(
                &mut writer,
                "FILEMAP",
                SelectorRole::FileMap,
                profile,
                &mut scratch[0],
                Phase::FileMap,
            )?;
            if selected.generation != 1
                || selected.reference.kind != ControlKind::FileMapRoot
                || selected.reference.slot != 0
                || selected.reference.index != 0
                || selected.reference.generation != 1
                || selected.reference.encoded_len != 320
            {
                return Err(error(Phase::FileMap, Cause::UnsupportedClosure));
            }
            {
                let [root, work, _] = scratch;
                let mut checked = CheckedNamespace(&mut writer);
                let reader =
                    PageFileReader::open(&mut checked, Some(profile.spec().namespace), root, work)
                        .map_err(|e| error(Phase::FileMap, Cause::PageIo(e)))?;
                if reader.profile() != profile || reader.selected() != selected {
                    return Err(error(Phase::FileMap, Cause::IdentityMismatch));
                }
            }
            read_control(
                &mut writer,
                "d3-bootstrap",
                &mut scratch[0][..4096],
                Phase::Bootstrap,
            )?;
            let plan =
                bootstrap::prepare_recovery_bootstrap(profile, selected, scratch).map_err(|e| {
                    let cause = match &e.cause {
                        BootstrapCause::UnsupportedPlan => Cause::UnsupportedBootstrapPlan,
                        BootstrapCause::Control(c) => Cause::Control(*c),
                        BootstrapCause::IdentityMismatch => Cause::IdentityMismatch,
                        _ => Cause::Bootstrap(e),
                    };
                    error(Phase::Bootstrap, cause)
                })?;
            let allocator = plan
                .allocation_identity()
                .map_err(|e| error(Phase::Bootstrap, Cause::Control(e)))?;
            (profile, selected, plan.kind.version(), allocator)
        };
        workspace.copy_within(BOOT..BOOT + PAGE_BYTES, ALLOC);
        let (selected, header, pending, stage_bytes) = {
            let scratch: &mut [u8; PAGE_BYTES] = (&mut workspace[BOOT..BOOT + PAGE_BYTES])
                .try_into()
                .expect("fixed control page");
            let selected = read_selector(
                &mut writer,
                "DWB",
                SelectorRole::Doublewrite,
                profile,
                scratch,
                Phase::Doublewrite,
            )?;
            if selected.generation != selected.reference.generation
                || selected.reference.index != 0
                || u64::from(selected.reference.slot) != (selected.generation - 1) % 2
                || ![160, 248].contains(&selected.reference.encoded_len)
            {
                return Err(error(Phase::Doublewrite, Cause::UnsupportedClosure));
            }
            let bank = control::control_file_name(selected.reference)
                .map_err(|e| error(Phase::Doublewrite, Cause::Control(e)))?;
            let mut file = open_exact(
                &mut writer,
                bank.as_str(),
                PAGE_BYTES as u64,
                Phase::Doublewrite,
            )?;
            let len = selected.reference.encoded_len as usize;
            read_exact_at(&mut file, &mut scratch[..len], 0)
                .map_err(|e| error(Phase::Doublewrite, Cause::Io(e)))?;
            let view =
                control::decode_selected_doublewrite(&scratch[..len], selected.reference, profile)
                    .map_err(|e| error(Phase::Doublewrite, Cause::Control(e)))?;
            let header = view.header();
            let pending = view.entry(0);
            validate_dwb_tuple(selected, &header, view.len())?;
            drop(file);
            for (path, length) in GEOMETRY {
                drop(open_exact(&mut writer, path, length, Phase::Doublewrite)?);
            }
            let stage_bytes = if writer
                .metadata(&name("d3-dwb-select-stage"))
                .map_err(|e| error(Phase::Doublewrite, Cause::Namespace(e)))?
                .is_some()
            {
                let (file, metadata) =
                    open_checked(&mut writer, "d3-dwb-select-stage", None, Phase::Doublewrite)?;
                if metadata.length > 4096 {
                    return Err(error(Phase::Doublewrite, Cause::LengthMismatch));
                }
                drop(file);
                metadata.length
            } else {
                0
            };
            (selected, header, pending, stage_bytes)
        };
        let (mut source, metadata) = open_checked(&mut writer, wal::WAL_NAME, None, Phase::Epoch)?;
        if metadata.length < 4096 {
            return Err(error(Phase::Epoch, Cause::LengthMismatch));
        }
        if metadata.length > wal::WAL_FILE_CAP {
            return Err(error(Phase::Epoch, Cause::UnsupportedClosure));
        }
        {
            let scratch = &mut workspace[BOOT..BOOT + 4096];
            read_exact_at(&mut source, scratch, 0)
                .map_err(|e| error(Phase::Epoch, Cause::Io(e)))?;
            let epoch = wal::decode_epoch(scratch, profile)
                .map_err(|e| error(Phase::Epoch, Cause::Control(e)))?;
            read_control(&mut writer, "WRITER", scratch, Phase::Writer)?;
            wal::decode_writer(scratch, profile, epoch)
                .map_err(|e| error(Phase::Writer, Cause::Control(e)))?;
        }
        let mut observation = RecoveryPrefixObservation {
            profile,
            plan,
            filemap,
            doublewrite: selected,
            dwb_header: header,
            pending,
            source: metadata,
            complete_offset: 4096,
            complete_lsn: 0,
            tail: None,
            mode: None,
            active: None,
            high_water: 0,
            inventory_items: 0,
            inventory_digest: content_digest(&[]),
            remaining: RemainingBudget::default(),
            retained: metadata.length - 4096,
            owned: 0,
            protected: 0,
            terminal_hold: 0,
            status_initialized: false,
            pending_matched: pending.is_none(),
            stage_bytes,
            replay_topup: 0,
        };
        analyze(&mut source, workspace, &mut observation, allocator)?;
        if !observation.pending_matched || header.max_lsn.0 > observation.complete_lsn {
            return Err(error(Phase::Ledger, Cause::RequiredPrefixMissing));
        }
        account(&mut observation)?;
        let current = writer
            .metadata(&name(wal::WAL_NAME))
            .map_err(|e| error(Phase::Snapshot, Cause::Namespace(e)))?
            .ok_or_else(|| error(Phase::Snapshot, Cause::IdentityMismatch))?;
        if current.identity != metadata.identity {
            return Err(error(Phase::Snapshot, Cause::IdentityMismatch));
        }
        if current.length != metadata.length {
            return Err(error(Phase::Snapshot, Cause::LengthMismatch));
        }
        Ok(Self {
            source,
            writer,
            workspace,
            observation,
        })
    }
    pub(crate) fn observation(&self) -> &RecoveryPrefixObservation {
        &self.observation
    }
    pub(crate) fn allocator_image(&self) -> &[u8; PAGE_BYTES] {
        self.workspace[ALLOC..ALLOC + PAGE_BYTES]
            .try_into()
            .expect("fixed allocator image")
    }
    pub(crate) fn status_image(&self) -> Option<&[u8; PAGE_BYTES]> {
        self.observation.status_initialized.then(|| {
            self.workspace[STATUS..STATUS + PAGE_BYTES]
                .try_into()
                .expect("fixed Status image")
        })
    }
    pub(crate) fn descriptor_bytes(&self) -> Option<&[u8; 1536]> {
        (self.observation.complete_lsn != 0).then(|| {
            self.workspace[DESCRIPTOR..DESCRIPTOR + 1536]
                .try_into()
                .expect("fixed descriptor")
        })
    }
}

fn validate_dwb_tuple(
    selected: SelectedControl,
    header: &ControlHeader,
    count: usize,
) -> Result<()> {
    let valid = if header.batch_id == 0 {
        selected.generation == 1 && header.state == ControlState::Empty && count == 0
    } else {
        let base = header.batch_id.checked_mul(3).ok_or_else(arithmetic)?;
        match header.state {
            ControlState::Pending => selected.generation == base && count == 1,
            ControlState::Empty => {
                count == 0
                    && (base.checked_sub(1) == Some(selected.generation)
                        || base.checked_add(1) == Some(selected.generation))
            }
        }
    };
    if !valid {
        return Err(error(Phase::Doublewrite, Cause::UnsupportedClosure));
    }
    Ok(())
}
fn account(observation: &mut RecoveryPrefixObservation) -> Result<()> {
    // remaining.protected already includes terminal WAL and PageToken costs.
    // Terminal address hold is a diagnostic union and is never charged again.
    observation.owned = super::super::FIXED_BYTES
        .checked_sub(4096)
        .and_then(|v| v.checked_add(observation.retained))
        .and_then(|v| v.checked_add(observation.stage_bytes))
        .ok_or_else(arithmetic)?;
    let page_credit = observation
        .remaining
        .protected
        .checked_sub(observation.remaining.wal)
        .ok_or_else(arithmetic)?;
    if !page_credit.is_multiple_of(PAGE_BYTES as u64) {
        return Err(error(
            Phase::Ledger,
            Cause::Semantic(SemanticError::BudgetMismatch),
        ));
    }
    let replay_pages = if observation.complete_lsn > 0 {
        1u64.checked_add(u64::from(observation.status_initialized))
            .ok_or_else(arithmetic)?
    } else {
        0
    };
    let needed = replay_pages
        .checked_add(observation.inventory_items)
        .ok_or_else(arithmetic)?;
    observation.replay_topup = needed
        .saturating_sub(page_credit / PAGE_BYTES as u64)
        .checked_mul(PAGE_BYTES as u64)
        .ok_or_else(arithmetic)?;
    observation.protected = super::super::PROTECTED_BYTES
        .checked_add(observation.retained)
        .and_then(|v| v.checked_add(observation.remaining.protected))
        .and_then(|v| v.checked_add(observation.replay_topup))
        .ok_or_else(arithmetic)?;
    let owned_peak = observation
        .owned
        .checked_add(observation.remaining.wal)
        .and_then(|v| {
            4096u64
                .checked_sub(observation.stage_bytes)
                .and_then(|credit| v.checked_add(credit))
        })
        .ok_or_else(arithmetic)?;
    let wal_peak = observation
        .complete_offset
        .checked_add(observation.remaining.wal)
        .ok_or_else(arithmetic)?;
    if owned_peak > observation.profile.owned_bytes()
        || observation.protected > 1 << 30
        || wal_peak > wal::WAL_FILE_CAP
    {
        return Err(error(Phase::Ledger, Cause::Capacity));
    }
    Ok(())
}
fn analyze<F: xadb_foundation::io::FileIo>(
    source: &mut F,
    workspace: &mut [u8; WORKSPACE_BYTES],
    observation: &mut RecoveryPrefixObservation,
    allocator: PageIdentity,
) -> Result<()> {
    analyze_with(source, workspace, observation, allocator, |_, _, _, _| true)
}
// Pure canonical continuation hook. Physical pages never enter adopt_frame.
fn analyze_with<F: xadb_foundation::io::FileIo>(
    source: &mut F,
    workspace: &mut [u8; WORKSPACE_BYTES],
    observation: &mut RecoveryPrefixObservation,
    allocator: PageIdentity,
    mut visit: impl FnMut(WalFrameHeader, &[u8], &[u8; PAGE_BYTES], Option<&[u8; PAGE_BYTES]>) -> bool,
) -> Result<()> {
    let (frame, rest) = workspace.split_at_mut(FRAME);
    let (before, rest) = rest.split_at_mut(PAGE_BYTES);
    let (after, rest) = rest.split_at_mut(PAGE_BYTES);
    let (status, rest) = rest.split_at_mut(PAGE_BYTES);
    let (next_status, rest) = rest.split_at_mut(PAGE_BYTES);
    let (_, rest) = rest.split_at_mut(3 * PAGE_BYTES);
    let descriptor = &mut rest[..1536];
    let before: &mut [u8; PAGE_BYTES] = before.try_into().expect("fixed canonical allocator");
    let after: &mut [u8; PAGE_BYTES] = after.try_into().expect("fixed prospective allocator");
    let status: &mut [u8; PAGE_BYTES] = status.try_into().expect("fixed canonical Status");
    let next_status: &mut [u8; PAGE_BYTES] =
        next_status.try_into().expect("fixed prospective Status");
    while observation.complete_offset < observation.source.length {
        let offset = observation.complete_offset;
        let remaining = observation.source.length - offset;
        let expected = FrameExpectation {
            stream: WalStreamIdentity {
                database: observation.profile.spec().namespace.database,
                timeline: 1,
                writer_epoch: 1,
            },
            previous_lsn: observation.complete_lsn,
        };
        let header_len = 160usize.min(remaining as usize);
        read_exact_at(source, &mut frame[..header_len], offset)
            .map_err(|e| error(Phase::Scan, Cause::Io(e)))?;
        if header_len < 160 {
            match wal_frame::scan_semantic_prefix(
                &frame[..header_len],
                expected,
                TailPolicy::ActiveUnrequiredEof,
            )
            .map_err(|e| error(Phase::Scan, Cause::Wal(e)))?
            {
                SemanticFrameScan::Incomplete => {}
                _ => return Err(error(Phase::Scan, Cause::Wal(WalError::Corrupt))),
            }
            observation.tail = Some(TailObservation {
                offset,
                physical_bytes: remaining,
                declared_bytes: None,
                inspected_bytes: header_len,
            });
            break;
        }
        let length = wal_frame::semantic_frame_length(&frame[..160], expected)
            .map_err(|e| error(Phase::Scan, Cause::Wal(e)))?;
        if remaining < length as u64 {
            wal_frame::scan_semantic_prefix(
                &frame[..160],
                expected,
                TailPolicy::ActiveUnrequiredEof,
            )
            .map_err(|e| error(Phase::Scan, Cause::Wal(e)))?;
            observation.tail = Some(TailObservation {
                offset,
                physical_bytes: remaining,
                declared_bytes: Some(length as u64),
                inspected_bytes: 160,
            });
            break;
        }
        if length > MAX_SUPPORTED_FRAME {
            validate_oversized(source, frame, offset, length)?;
            return Err(error(Phase::Scan, Cause::UnsupportedClosure));
        }
        read_exact_at(source, &mut frame[160..length], offset + 160)
            .map_err(|e| error(Phase::Scan, Cause::Io(e)))?;
        let header = match wal_frame::scan_semantic_prefix(
            &frame[..length],
            expected,
            TailPolicy::ForbidIncomplete,
        )
        .map_err(|e| error(Phase::Scan, Cause::Wal(e)))?
        {
            SemanticFrameScan::Complete { envelope, consumed } if consumed == length => {
                envelope.header
            }
            _ => return Err(error(Phase::Scan, Cause::Wal(WalError::Corrupt))),
        };
        adopt_frame(
            observation,
            allocator,
            CanonicalImages {
                before: &mut *before,
                after: &mut *after,
                status: &mut *status,
                next_status: &mut *next_status,
                descriptor: &mut *descriptor,
            },
            header,
            &frame[..length],
        )?;
        observation.complete_offset = offset.checked_add(length as u64).ok_or_else(arithmetic)?;
        observation.complete_lsn = header.lsn;
        account(observation)?;
        if !visit(
            header,
            &frame[..length],
            before,
            observation.status_initialized.then_some(&*status),
        ) {
            return Ok(());
        }
    }
    Ok(())
}
fn validate_oversized<F: xadb_foundation::io::FileIo>(
    source: &mut F,
    chunk: &mut [u8],
    offset: u64,
    length: usize,
) -> Result<()> {
    let mut hash = ContentHasher::new();
    let mut done = 0usize;
    let body = length - 32;
    while done < body {
        let count = chunk.len().min(body - done);
        read_exact_at(source, &mut chunk[..count], offset + done as u64)
            .map_err(|e| error(Phase::Scan, Cause::Io(e)))?;
        hash.update(&chunk[..count]);
        done += count;
    }
    read_exact_at(source, &mut chunk[..32], offset + body as u64)
        .map_err(|e| error(Phase::Scan, Cause::Io(e)))?;
    if hash.digest() != chunk[..32] {
        return Err(error(Phase::Scan, Cause::Wal(WalError::Corrupt)));
    }
    Ok(())
}
fn profile_ref(profile: Profile) -> ControlRef {
    ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: profile.spec().generation,
        encoded_len: 4096,
        digest: profile.digest(),
    }
}
struct CanonicalImages<'a> {
    before: &'a mut [u8; PAGE_BYTES],
    after: &'a mut [u8; PAGE_BYTES],
    status: &'a mut [u8; PAGE_BYTES],
    next_status: &'a mut [u8; PAGE_BYTES],
    descriptor: &'a mut [u8],
}
fn adopt_frame(
    observation: &mut RecoveryPrefixObservation,
    allocator: PageIdentity,
    images: CanonicalImages<'_>,
    header: WalFrameHeader,
    bytes: &[u8],
) -> Result<()> {
    let CanonicalImages {
        before,
        after,
        status,
        next_status,
        descriptor,
    } = images;
    use SemanticError::*;
    if observation.status_initialized {
        return Err(semantic(StateMismatch));
    }
    if bytes.len() - 192 < 8 || &bytes[160..164] != b"XSG1" || get16(bytes, 164) != 1 {
        return Err(semantic(Corrupt));
    }
    let opcode = get16(bytes, 166);
    if !matches!(opcode, 8 | 10) {
        return Err(semantic(UnsupportedOperation));
    }
    if header.kind != FrameKind::System || bytes.len() < 2144 {
        return Err(semantic(Corrupt));
    }
    if header.previous_lsn != observation.complete_lsn
        || header.lsn
            != observation
                .complete_lsn
                .checked_add(1)
                .ok_or_else(arithmetic)?
    {
        return Err(semantic(OrderMismatch));
    }
    let begin = opcode == 8;
    let batch = if begin {
        if observation.active.is_some() {
            return Err(semantic(StateMismatch));
        }
        let count = get16(bytes, 290) as usize;
        if !(1..=63).contains(&count) || (observation.plan == 2 && count != 1) {
            return Err(semantic(StateMismatch));
        }
        let owner = SystemWorkRef::new(
            observation.profile.spec().namespace,
            SystemWorkKind::AllocationMaintenance,
            observation
                .high_water
                .checked_add(1)
                .ok_or_else(arithmetic)?,
            Generation::new(1).expect("fixed generation"),
        )
        .map_err(|_| semantic(IdentityMismatch))?;
        let page = allocation_page::decode_allocation_page(
            before,
            allocator,
            observation.profile.spec().namespace,
        )
        .map_err(|_| semantic(PageMismatch))?;
        let mut mask = 0u64;
        for index in 1..64 {
            if let AllocationEntry::Free(g) =
                page.entry(index).map_err(|_| semantic(PageMismatch))?
                && g.get() != u64::MAX
                && mask.count_ones() < (count as u32)
            {
                mask |= 1u64 << index;
            }
        }
        if mask.count_ones() as usize != count {
            return Err(semantic(PageMismatch));
        }
        Batch {
            owner,
            mask,
            birth: header.lsn,
        }
    } else {
        observation.active.ok_or_else(|| semantic(StateMismatch))?
    };
    let desc = obligation::decode_descriptor(
        &bytes[352..1888],
        ReservationOwner::System(batch.owner),
        profile_ref(observation.profile),
    )
    .map_err(|e| {
        semantic(if e == obligation::ObligationError::IdentityMismatch {
            IdentityMismatch
        } else {
            Corrupt
        })
    })?;
    let mode = if let Some(mode) = observation.mode {
        mode
    } else if observation.plan == 2 {
        ProducerMode::Status
    } else {
        match desc.remaining[2].peak.memory {
            81920 => ProducerMode::Exclusive,
            98304 => ProducerMode::Dirty,
            _ => return Err(semantic(BudgetMismatch)),
        }
    };
    let expected = FrameExpectation {
        stream: header.stream,
        previous_lsn: observation.complete_lsn,
    };
    if !begin && bytes.len() == init::FRAME_BYTES {
        if observation.plan != 2 || mode != ProducerMode::Status || batch.count() != 1 {
            return Err(semantic(UnsupportedOperation));
        }
        let index = batch.mask.trailing_zeros() as usize;
        let page = allocation_page::decode_allocation_page(
            before,
            allocator,
            observation.profile.spec().namespace,
        )
        .map_err(|_| semantic(PageMismatch))?;
        let AllocationEntry::Reserved {
            generation,
            claimant,
        } = page.entry(index).map_err(|_| semantic(PageMismatch))?
        else {
            return Err(semantic(InventoryMismatch));
        };
        if claimant != ReservationOwner::System(batch.owner) {
            return Err(semantic(InventoryMismatch));
        }
        let payload = PageIdentity {
            database: allocator.database,
            page: PageRef {
                page_no: index as u64,
                incarnation: generation,
                ..allocator.page
            },
            owner: ObjectId::new(2).expect("fixed Status object"),
            owner_generation: Generation::new(1).expect("fixed Status generation"),
        };
        let plan = init::InitExpected {
            profile: observation.profile,
            allocator,
            payload,
            frame: expected,
            batch,
            before_descriptor: (&*descriptor).try_into().expect("fixed descriptor"),
        };
        init::materialize_analysis(&plan, before, bytes, after, next_status).map_err(semantic)?;
        match_pending(
            observation,
            header.lsn,
            allocator,
            PageKind::Allocation,
            after,
        )?;
        match_pending(
            observation,
            header.lsn,
            payload,
            PageKind::TransactionStatus,
            next_status,
        )?;
        status.copy_from_slice(next_status);
        observation.status_initialized = true;
    } else {
        let expected = AnalysisExpected {
            profile: observation.profile,
            identity: allocator,
            frame: expected,
            batch,
            cancel: !begin,
            before_descriptor: (!begin).then_some(&*descriptor),
        };
        operation::materialize_analysis(expected, before, bytes, after, mode.budget())
            .map_err(semantic)?;
        match_pending(
            observation,
            header.lsn,
            allocator,
            PageKind::Allocation,
            after,
        )?;
    }
    // Commit the RAM analysis once, only after every byte/transition check.
    before.copy_from_slice(after);
    descriptor.copy_from_slice(&bytes[352..1888]);
    observation.mode = Some(mode);
    observation.inventory_items = desc.inventory.items;
    observation.inventory_digest = desc.inventory.digest;
    observation.remaining = desc.remaining[2];
    if begin {
        observation.active = Some(batch);
        observation.high_water = batch.owner.id();
    } else {
        observation.active = None;
        observation.terminal_hold |= batch.mask;
    }
    Ok(())
}
fn match_pending(
    observation: &mut RecoveryPrefixObservation,
    lsn: u64,
    identity: PageIdentity,
    kind: PageKind,
    image: &[u8; PAGE_BYTES],
) -> Result<()> {
    if let Some(pending) = observation.pending
        && pending.lsn.0 == lsn
        && pending.page == identity.page
    {
        if pending.owner != identity.owner
            || pending.owner_generation != identity.owner_generation
            || pending.kind != kind
            || pending.level != 0
        {
            return Err(error(Phase::Doublewrite, Cause::IdentityMismatch));
        }
        if pending.slot != 0 || pending.image_digest != content_digest(image) {
            return Err(error(
                Phase::Doublewrite,
                Cause::Doublewrite(dwb::DoublewriteError::ImageMismatch),
            ));
        }
        observation.pending_matched = true;
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
    read_control(writer, path, &mut scratch[..4096], phase)?;
    control::decode_selector(&scratch[..4096], role, profile)
        .map_err(|e| error(phase, Cause::Control(e)))
}
fn open_checked<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    expected: Option<u64>,
    phase: Phase,
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
    open_checked(writer, path, Some(length), phase).map(|(file, _)| file)
}
fn read_control<W: WriterNamespace>(
    writer: &mut W,
    path: &str,
    output: &mut [u8],
    phase: Phase,
) -> Result<()> {
    let mut file = open_exact(writer, path, output.len() as u64, phase)?;
    read_exact_at(&mut file, output, 0).map_err(|e| error(phase, Cause::Io(e)))
}
struct CheckedNamespace<'a, W: WriterNamespace>(&'a mut W);
fn forbidden() -> NamespaceError {
    NamespaceError::new(
        NamespaceErrorKind::InvalidName,
        Effect::Unchanged,
        "read-only canonical prefix",
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
    fn open_existing(&mut self, n: &FileName, m: OpenMode) -> NamespaceResult<Self::File> {
        self.open_existing_with_metadata(n, m).map(|(f, _)| f)
    }
    fn open_existing_with_metadata(
        &mut self,
        n: &FileName,
        m: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        if m != OpenMode::ReadOnly {
            return Err(forbidden());
        }
        let (f, opened) = self.0.open_existing_with_metadata(n, m)?;
        let length = match n.as_str() {
            "d3-profile" | "FILEMAP" => 4096,
            "d3-filemap-0" | "d3-filemap-1" => control::CONTAINER_BYTES,
            _ => return Err(forbidden()),
        };
        if opened.length != length {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "prefix fixed opened length",
            ));
        }
        let named = self.0.metadata(n)?.ok_or_else(forbidden)?;
        if opened.identity != named.identity {
            return Err(NamespaceError::new(
                NamespaceErrorKind::InvalidType,
                Effect::Unchanged,
                "prefix opened identity changed",
            ));
        }
        if opened.length != named.length {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "prefix opened length changed",
            ));
        }
        Ok((f, opened))
    }
    fn metadata(&self, n: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.0.metadata(n)
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
#[path = "recovery_prefix_native_tests.rs"]
mod native;
#[cfg(test)]
#[path = "recovery_prefix_tests.rs"]
mod tests;

#[path = "recovery_effect/mod.rs"]
mod effect;
pub(crate) use effect::{
    ClosedRecovery, RecoveryAcquireError, RecoveryCause, RecoveryError, RecoveryFailure,
    RecoveryInspection, RecoveryPhase,
};
