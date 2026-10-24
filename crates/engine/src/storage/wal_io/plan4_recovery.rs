//! Effect-owning N=0 plan4 recovery. Selection and canonical reconstruction are
//! separate from the eventual original-runtime handoff. No Fresh is created.
//!
//! All large bytes remain in the caller's original maintenance/flush/WAL loans.
//! The six children form one actual recovery consumer; none of their raw codec
//! observations or intermediate states is a foreground capability.

use super::super::super::{BootstrapPlan, BootstrapPlanKind};
use super::super::{InstallPlan, WORKSPACE_BYTES};
use super::{MutableStorageOwner, wal};
use crate::{
    api::Namespace,
    storage::{
        control::{self, ControlError, Profile, SelectedControl},
        doublewrite_control::{self as dwb, DoublewriteError},
        page::{PAGE_BYTES, PageError},
        page_io::PageIoError,
        plan4_recovery_base as base, plan4_recovery_control as record,
        session_fence::SessionFence,
        wal_frame::{WalError, WalStreamIdentity},
    },
};
use std::sync::{Condvar, Mutex};
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    error::Effect,
    io::{FileIo, read_exact_at, sync, write_all_at},
    namespace::{
        DirectoryPort, FileIdentity, FileMetadata, FileName, NamespaceError, OpenMode, Selector,
        WriterNamespace,
    },
};

#[path = "plan4_recovery_analysis.rs"]
mod analysis;
#[path = "plan4_recovery_epoch.rs"]
mod epoch;
#[path = "plan4_recovery_physical.rs"]
mod physical;
#[path = "plan4_recovery_resources.rs"]
mod resources;
#[path = "plan4_recovery_select.rs"]
mod selection;
#[cfg(test)]
#[path = "plan4_recovery_tests.rs"]
mod tests;
pub(in crate::storage) use resources::{RecoverySourceCensus, source_census};

pub(crate) const MAINTENANCE_BYTES: usize = 12_582_912;
const BASE_BYTES: usize = 1_048_576;
const CONTROL_BYTES: usize = 4096;
const REPLAY_START: usize = 2 * BASE_BYTES;
const REPLAY_END: usize = 3_670_016;
const PHYSICAL_START: usize = REPLAY_END;
const PHYSICAL_END: usize = 4_718_592;
const LIVE_START: usize = PHYSICAL_END;
const LIVE_END: usize = 4_915_200;
const PACK_START: usize = LIVE_END;
const PACK_END: usize = 4_980_736;
const STATUS_START: usize = PACK_END;
const STATUS_END: usize = 5_013_504;
const CERT_START: usize = STATUS_END;
const CERT_END: usize = 5_029_888;
const INVENTORY_START: usize = CERT_END;
const INVENTORY_END: usize = 5_046_272;
const BASE_SCRATCH_START: usize = INVENTORY_END;
const IO_START: usize = 5_111_808;
const USED_END: usize = 5_177_344;
// A digest of the complete initial bounded namespace observation. It occupies
// 32 bytes of the retained remainder, never a new resident allocation.
const SOURCE_STAMP_AT: usize = USED_END;
const CURRENT_AT: usize = IO_START;
const POSSIBLE_CURRENT_AT: usize = CURRENT_AT + CONTROL_BYTES;
const SELECTOR_AT: usize = POSSIBLE_CURRENT_AT + CONTROL_BYTES;
const IO_WORK: usize = IO_START + PAGE_BYTES;
const HOME_NAME: &str = "d3-data-0000000100000001";
const BASE_NAMES: [&str; 2] = ["d3-rebase-0", "d3-rebase-1"];
const WAL_CAP: u64 = 256 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4RecoveryPhase {
    Preflight,
    Lease,
    Selection,
    Analysis,
    Pending,
    Snapshot,
    Redo,
    Rebase,
    Reclaim,
    LogOpen,
    Bind,
    CancelReservedFirst,
    Losers,
    Verify,
    Stabilize,
    Handoff,
}
#[derive(Debug)]
pub(crate) enum Plan4RecoveryCause {
    Capacity,
    Unsupported,
    IdentityMismatch,
    ContentMismatch,
    OrderMismatch,
    LengthMismatch,
    Missing,
    Busy,
    Invariant,
    Fenced,
    Poisoned,
    Exhausted,
    Namespace(NamespaceError),
    Io(xadb_foundation::Error),
    Control(ControlError),
    Doublewrite(DoublewriteError),
    Page(PageError),
    PageIo(PageIoError),
    Base(base::Error),
    Record(record::Error),
    Frame(WalError),
    Bootstrap(super::super::super::BootstrapError),
    Genesis(super::super::super::plan4_genesis::Plan4GenesisCause),
    Retention(super::plan4_pages::RetentionCause),
    Binding(super::plan4_pages::Plan4PageError),
    Concurrency(crate::transaction::concurrency::ConcurrencyError),
    Wal(wal::WalIoError),
}
#[derive(Debug)]
pub(crate) struct Plan4RecoveryError {
    pub(crate) phase: Plan4RecoveryPhase,
    pub(crate) cause: Plan4RecoveryCause,
    pub(crate) effect: Effect,
    pub(crate) database_effect: Effect,
    pub(crate) lease_acquired: bool,
}
type Result<T> = std::result::Result<T, Plan4RecoveryError>;
fn error(phase: Plan4RecoveryPhase, cause: Plan4RecoveryCause) -> Plan4RecoveryError {
    Plan4RecoveryError {
        phase,
        cause,
        effect: Effect::Unchanged,
        database_effect: Effect::Unchanged,
        lease_acquired: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryCallKind {
    Read,
    Open,
    Metadata,
    Inventory,
    Create,
    Write,
    Sync,
    Publish,
    DirectorySync,
    Remove,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan4RecoveryObservation {
    pub(crate) phase: Plan4RecoveryPhase,
    pub(crate) calls_attempted: u64,
    pub(crate) calls_completed: u64,
    pub(crate) mutations_attempted: u64,
    pub(crate) mutations_completed: u64,
    pub(crate) last_call: Option<RecoveryCallKind>,
    pub(crate) last_name: Option<FileName>,
    pub(crate) last_offset: u64,
    pub(crate) last_length: u64,
    pub(crate) source_lsn: u64,
    pub(crate) source_end: u64,
    pub(crate) actual_owned: u64,
    pub(crate) actual_wal: u64,
    pub(crate) inventory_capacity: usize,
}
impl Plan4RecoveryObservation {
    fn new() -> Self {
        Self {
            phase: Plan4RecoveryPhase::Lease,
            calls_attempted: 0,
            calls_completed: 0,
            mutations_attempted: 0,
            mutations_completed: 0,
            last_call: None,
            last_name: None,
            last_offset: 0,
            last_length: 0,
            source_lsn: 0,
            source_end: CONTROL_BYTES as u64,
            actual_owned: 0,
            actual_wal: 0,
            inventory_capacity: 0,
        }
    }
    fn begin(
        &mut self,
        kind: RecoveryCallKind,
        name: FileName,
        offset: u64,
        length: u64,
    ) -> Result<()> {
        self.calls_attempted = self
            .calls_attempted
            .checked_add(1)
            .ok_or_else(|| error(self.phase, Plan4RecoveryCause::Exhausted))?;
        if is_mutation(kind) {
            self.mutations_attempted = self
                .mutations_attempted
                .checked_add(1)
                .ok_or_else(|| error(self.phase, Plan4RecoveryCause::Exhausted))?;
        }
        self.last_call = Some(kind);
        self.last_name = Some(name);
        self.last_offset = offset;
        self.last_length = length;
        Ok(())
    }
    fn done(&mut self, kind: RecoveryCallKind) {
        self.calls_completed += 1;
        if is_mutation(kind) {
            self.mutations_completed += 1;
        }
    }
}
fn is_mutation(kind: RecoveryCallKind) -> bool {
    matches!(
        kind,
        RecoveryCallKind::Create
            | RecoveryCallKind::Write
            | RecoveryCallKind::Sync
            | RecoveryCallKind::Publish
            | RecoveryCallKind::DirectorySync
            | RecoveryCallKind::Remove
    )
}
fn name(value: &str) -> FileName {
    FileName::new(value).expect("closed recovery filename")
}

/// A selected source File remains owned throughout preinspection/physical
/// replay. It is dropped before exact retirement, never reopened by highest ID.
struct HeldSource<F> {
    file: F,
    name: FileName,
    metadata: FileMetadata,
    epoch: u32,
    header_digest: [u8; 32],
    anchor_lsn: u64,
    sealed: Option<(u64, u64, [u8; 32])>,
}
struct SourceFacts {
    epoch: u32,
    header_digest: [u8; 32],
    prefix_digest: [u8; 32],
    lsn: u64,
    end: u64,
    adopted_local: u64,
}
struct AnalysisSeal {
    state_digest: [u8; 32],
    target_mask: u64,
    status_mask: u64,
    remaining_wal: u64,
    remaining_steps: u64,
    home_matched: u64,
    snapshot_repair: u64,
    pending_matched: u64,
}
struct RestoredRetention {
    state_digest: [u8; 32],
    remaining_wal: u64,
    remaining_steps: u64,
    physical_topup: u64,
    metadata_growth: u64,
}

/// Issued only by the selected runtime after physical VERIFY, a durable new
/// epoch and Empty DWB readback. The borrowed image remains the accepted base;
/// this witness does not synthesize a dirty requirement or a DPT entry.
pub(in crate::storage) struct RecoveryPoolSeed<'a> {
    allocator: &'a [u8; PAGE_BYTES],
    request: crate::storage::dirty_frame::TargetRequest,
    lsn: crate::storage::page::PageLsn,
    digest: [u8; 32],
    binding: &'a std::sync::atomic::AtomicU8,
    fence: &'a SessionFence,
}
impl RecoveryPoolSeed<'_> {
    pub(in crate::storage) fn allocator(&self) -> &[u8; PAGE_BYTES] {
        self.allocator
    }
    pub(in crate::storage) fn request(&self) -> crate::storage::dirty_frame::TargetRequest {
        self.request
    }
    pub(in crate::storage) fn lsn(&self) -> crate::storage::page::PageLsn {
        self.lsn
    }
    pub(in crate::storage) fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(in crate::storage) fn binding(&self) -> &std::sync::atomic::AtomicU8 {
        self.binding
    }
    pub(in crate::storage) fn fence(&self) -> &SessionFence {
        self.fence
    }
}

/// A byte observation becomes this witness only inside selection/capture after
/// the actual CURRENT reference, immutable geometry and provenance agree.
/// Its BaseView borrows the original external caller allocation, not this owner.
pub(in crate::storage) struct VerifiedPlan4Base<'a> {
    view: base::BaseView<'a>,
    digest: [u8; 32],
}
impl<'a> VerifiedPlan4Base<'a> {
    pub(in crate::storage) fn view(&self) -> base::BaseView<'a> {
        self.view
    }
    pub(in crate::storage) fn capture_lsn(&self) -> u64 {
        self.view.header().capture_lsn
    }
    pub(in crate::storage) fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
pub(in crate::storage) struct SelectedNewEpoch {
    name: FileName,
    metadata: FileMetadata,
    header: record::EpochHeader,
    header_digest: [u8; 32],
    writer_digest: [u8; 32],
    current_digest: [u8; 32],
    current_generation: u64,
}
impl SelectedNewEpoch {
    pub(in crate::storage) fn name(&self) -> FileName {
        self.name
    }
    pub(in crate::storage) fn identity(&self) -> FileIdentity {
        self.metadata.identity
    }
    pub(in crate::storage) fn header(&self) -> record::EpochHeader {
        self.header
    }
    pub(in crate::storage) fn header_digest(&self) -> [u8; 32] {
        self.header_digest
    }
}
struct EpochTransferSeal;
struct RuntimePreparation {
    doublewrite: SelectedRecoveryDwb,
    descriptor_digest: [u8; 32],
    selector_digest: [u8; 32],
}
/// Readback-issued Empty selection, including burned issuance. Scalar getters
/// cannot construct this witness or reset a recovered flusher's batch counter.
pub(in crate::storage) struct SelectedRecoveryDwb {
    selected: SelectedControl,
    last_issued: u64,
}
impl SelectedRecoveryDwb {
    pub(in crate::storage) fn selected(&self) -> SelectedControl {
        self.selected
    }
    pub(in crate::storage) fn last_issued(&self) -> u64 {
        self.last_issued
    }
}
pub(super) struct SelectedEpochParts<'m, W: WriterNamespace> {
    pub(super) writer: W,
    pub(super) file: W::File,
    pub(super) slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    pub(super) fenced: SessionFence,
    pub(super) completion: Condvar,
    pub(super) prefix: wal::WalPrefix,
    pub(super) bootstrap: BootstrapPlan,
    pub(super) doublewrite: InstallPlan,
    _selected: EpochTransferSeal,
}

/// Holds the acquired W before any fallible read. CURRENT known/possible raw
/// bytes live in maintenance, so a wide decoded Current is never retained here.
pub(crate) struct Plan4Recovery<'a, 'm, 'w, W: WriterNamespace> {
    state: RecoveryState<'a, 'm, 'w, W>,
    fence: SessionFence,
    completion: Condvar,
}
struct RecoveryState<'a, 'm, 'w, W: WriterNamespace> {
    writer: W,
    source: Option<HeldSource<W::File>>,
    maintenance: &'a mut [u8; MAINTENANCE_BYTES],
    flush: &'w mut [u8; WORKSPACE_BYTES],
    slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    observation: Plan4RecoveryObservation,
    expected_namespace: Option<Namespace>,
    profile: Option<Profile>,
    filemap: Option<SelectedControl>,
    bootstrap_digest: [u8; 32],
    doublewrite: Option<SelectedControl>,
    doublewrite_header: Option<dwb::ControlHeader>,
    selected_base: Option<u8>,
    selected_previous: bool,
    current_present: bool,
    possible_current: bool,
    possible_doublewrite: bool,
    source_facts: Option<SourceFacts>,
    analysis: Option<AnalysisSeal>,
    restored: Option<RestoredRetention>,
    selected_epoch: Option<SelectedNewEpoch>,
}

pub(crate) enum Plan4RecoveryAcquireFailure<'a, 'm, 'w, W: WriterNamespace> {
    Before {
        error: Plan4RecoveryError,
        maintenance: &'a mut [u8; MAINTENANCE_BYTES],
        flush: &'w mut [u8; WORKSPACE_BYTES],
        slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    },
    Acquired(Plan4RecoveryFailure<'a, 'm, 'w, W>),
}
pub(crate) struct Plan4RecoveryFailure<'a, 'm, 'w, W: WriterNamespace> {
    owner: Plan4Recovery<'a, 'm, 'w, W>,
    error: Plan4RecoveryError,
}
impl<W: WriterNamespace> Plan4RecoveryFailure<'_, '_, '_, W> {
    pub(crate) fn error(&self) -> &Plan4RecoveryError {
        &self.error
    }
    pub(crate) fn observation(&self) -> &Plan4RecoveryObservation {
        &self.owner.state.observation
    }
}
impl<W: WriterNamespace> std::fmt::Debug for Plan4RecoveryFailure<'_, '_, '_, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl<W: WriterNamespace> std::fmt::Debug for Plan4RecoveryAcquireFailure<'_, '_, '_, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Before { error, .. } => error.fmt(f),
            Self::Acquired(failure) => failure.fmt(f),
        }
    }
}

impl<'a, 'm, 'w, W: WriterNamespace> Plan4Recovery<'a, 'm, 'w, W> {
    pub(crate) fn acquire<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        maintenance: &'a mut [u8; MAINTENANCE_BYTES],
        flush: &'w mut [u8; WORKSPACE_BYTES],
        slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    ) -> std::result::Result<Self, Plan4RecoveryAcquireFailure<'a, 'm, 'w, W>> {
        if expected.is_some_and(|n| n.timeline == 0) || !resources::fits::<W>() {
            return Err(Plan4RecoveryAcquireFailure::Before {
                error: error(Plan4RecoveryPhase::Preflight, Plan4RecoveryCause::Capacity),
                maintenance,
                flush,
                slot,
            });
        }
        let writer = match directory.acquire_writer() {
            Ok(writer) => writer,
            Err(cause) => {
                let effect = cause.effect;
                return Err(Plan4RecoveryAcquireFailure::Before {
                    error: Plan4RecoveryError {
                        phase: Plan4RecoveryPhase::Lease,
                        cause: Plan4RecoveryCause::Namespace(cause),
                        effect,
                        database_effect: Effect::Unchanged,
                        lease_acquired: false,
                    },
                    maintenance,
                    flush,
                    slot,
                });
            }
        };
        let state = RecoveryState {
            writer,
            source: None,
            maintenance,
            flush,
            slot,
            observation: Plan4RecoveryObservation::new(),
            expected_namespace: expected,
            profile: None,
            filemap: None,
            bootstrap_digest: [0; 32],
            doublewrite: None,
            doublewrite_header: None,
            selected_base: None,
            selected_previous: false,
            current_present: false,
            possible_current: false,
            possible_doublewrite: false,
            source_facts: None,
            analysis: None,
            restored: None,
            selected_epoch: None,
        };
        let mut owner = Self {
            state,
            fence: SessionFence::new(),
            completion: Condvar::new(),
        };
        let outcome = {
            let _unwind = owner.fence.unwind_guard();
            owner.state.select()
        };
        if let Err(error) = outcome {
            return Err(Plan4RecoveryAcquireFailure::Acquired(owner.failure(error)));
        }
        Ok(owner)
    }
    fn failure(self, mut error: Plan4RecoveryError) -> Plan4RecoveryFailure<'a, 'm, 'w, W> {
        error.lease_acquired = true;
        error.effect = Effect::CallerMustResolve;
        if self.state.observation.mutations_attempted != 0 {
            error.database_effect = Effect::CallerMustResolve;
        }
        self.fence.fence();
        self.completion.notify_all();
        Plan4RecoveryFailure { owner: self, error }
    }
    pub(crate) fn observation(&self) -> &Plan4RecoveryObservation {
        &self.state.observation
    }
    /// Returns the selected original runtime, still closed to foreground work.
    /// The recovered binding must complete durable CancelReservedFirst and old
    /// loser cleanup before its separate live continuation can be returned.
    pub(crate) fn recover(
        mut self,
    ) -> std::result::Result<Plan4RecoveryRuntime<'a, 'm, 'w, W>, Plan4RecoveryFailure<'a, 'm, 'w, W>>
    {
        let outcome = {
            let _unwind = self.fence.unwind_guard();
            self.state.recover_physical_and_open()
        };
        let prepared = match outcome {
            Ok(value) => value,
            Err(e) => return Err(self.failure(e)),
        };
        let Self {
            state,
            fence,
            completion,
        } = self;
        Ok(state.into_runtime(fence, completion, prepared))
    }
}
impl<W: WriterNamespace> RecoveryState<'_, '_, '_, W> {
    fn profile(&self) -> Profile {
        self.profile.expect("selected profile")
    }
    fn base_expected(&self) -> base::Expected {
        base::Expected {
            profile: self.profile(),
            filemap: self.filemap.expect("selected filemap"),
            bootstrap_digest: self.bootstrap_digest,
        }
    }
    fn recover_physical_and_open(&mut self) -> Result<RuntimePreparation> {
        self.preinspect()?;
        self.reserve_physical()?;
        self.verify_pre_effect_controls()?;
        self.verify_pending_slot()?;
        self.stabilize_source()?;
        self.physical()?;
        self.rebase()?;
        self.log_open()?;
        self.verify_physical()?;
        self.verify_selected_controls()?;
        let doublewrite = self.seal_doublewrite()?;
        let profile = self.profile();
        let p = Plan4RecoveryPhase::Handoff;
        // Validate the canonical construction here while failure can still
        // return the whole original owner. No BootstrapPlan survives this
        // lexical phase into the final deep snapshot decoder.
        {
            let scratch: &mut [[u8; PAGE_BYTES]; 3] = self.maintenance
                [IO_WORK..IO_WORK + 3 * PAGE_BYTES]
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("bootstrap scratch");
            BootstrapPlan::prepare_for_kind(
                profile,
                scratch,
                BootstrapPlanKind::RetentionCatalogBootstrap,
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Bootstrap(e)))?;
        }
        selection::read_named(
            &mut self.writer,
            &mut self.observation,
            name("DWB"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        let selected = control::decode_selector(
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            control::SelectorRole::Doublewrite,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        if selected != doublewrite.selected {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        let descriptor_digest = content_digest(&self.flush[..PAGE_BYTES]);
        let selector_digest = content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]);
        // Recheck the exact new snapshot before splitting external loans.
        let expected = self.base_expected();
        let (before, after) = self.maintenance.split_at_mut(BASE_SCRATCH_START);
        let scratch: &mut [[u8; PAGE_BYTES]; 4] = after[..4 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("base scratch");
        let view = base::decode(&before[BASE_BYTES..2 * BASE_BYTES], expected, scratch)
            .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?;
        if Some(view.digest()) != self.selected_epoch.as_ref().map(|e| e.header.base_digest) {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        self.observation.phase = p;
        Ok(RuntimePreparation {
            doublewrite,
            descriptor_digest,
            selector_digest,
        })
    }
}

impl<'a, 'm, 'w, W: WriterNamespace> RecoveryState<'a, 'm, 'w, W> {
    fn into_runtime(
        self,
        fenced: SessionFence,
        completion: Condvar,
        prepared: RuntimePreparation,
    ) -> Plan4RecoveryRuntime<'a, 'm, 'w, W> {
        let expected = self.base_expected();
        let Self {
            writer,
            source,
            maintenance,
            flush,
            slot,
            observation,
            restored,
            selected_epoch,
            ..
        } = self;
        let selected = selected_epoch.expect("completed LOG_OPEN");
        let source = source.expect("original newly opened WAL handle");
        let (old_base, rest) = maintenance.split_at_mut(BASE_BYTES);
        let (new_base, rest) = rest.split_at_mut(BASE_BYTES);
        let (replay, rest) = rest.split_at_mut(REPLAY_END - REPLAY_START);
        let (physical, rest) = rest.split_at_mut(BASE_BYTES);
        let (control, rest) = rest.split_at_mut(LIVE_END - LIVE_START);
        let (packing_and_scratch, reserved) = rest.split_at_mut(USED_END - LIVE_END);
        let scratch_at = BASE_SCRATCH_START - LIVE_END;
        let scratch: &mut [[u8; PAGE_BYTES]; 4] = packing_and_scratch
            [scratch_at..scratch_at + 4 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("retained codec scratch");
        // Same immutable bytes/expected values were decoded immediately before
        // transfer; this repeats no I/O or semantic adoption and cannot fail.
        let view =
            base::decode(new_base, expected, scratch).expect("prechecked immutable base transfer");
        let base = VerifiedPlan4Base {
            view,
            digest: selected.header.base_digest,
        };
        // The exact profile/kind construction was validated before transfer;
        // it starts only after the deep decoder returned its borrowed view.
        let bootstrap_scratch: &mut [[u8; PAGE_BYTES]; 3] = packing_and_scratch
            [IO_WORK - LIVE_END..IO_WORK - LIVE_END + 3 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("bootstrap scratch");
        let bootstrap = BootstrapPlan::prepare_for_kind(
            expected.profile,
            bootstrap_scratch,
            BootstrapPlanKind::RetentionCatalogBootstrap,
        )
        .expect("prechecked canonical runtime bootstrap");
        let installation = InstallPlan {
            profile: expected.profile,
            filemap: expected.filemap,
            selected: prepared.doublewrite.selected,
            descriptor_digest: prepared.descriptor_digest,
            selector_digest: prepared.selector_digest,
        };
        let prefix = wal::WalPrefix {
            stream: WalStreamIdentity {
                database: expected.profile.spec().namespace.database,
                timeline: expected.profile.spec().namespace.timeline,
                writer_epoch: selected.header.epoch,
            },
            file: source.metadata.identity,
            complete_lsn: selected.header.base_lsn,
            complete_offset: CONTROL_BYTES as u64,
            synced_lsn: selected.header.base_lsn,
            synced_offset: CONTROL_BYTES as u64,
        };
        let parts = SelectedEpochParts {
            writer,
            file: source.file,
            slot,
            fenced,
            completion,
            prefix,
            bootstrap,
            doublewrite: installation,
            _selected: EpochTransferSeal,
        };
        let storage = MutableStorageOwner::from_plan4_recovery(parts, flush);
        Plan4RecoveryRuntime {
            storage,
            base,
            selected,
            doublewrite: prepared.doublewrite,
            loans: Mutex::new(Some(RecoveryBindLoans {
                control: control.try_into().expect("original control loan"),
                replay: replay.try_into().expect("original replay loan"),
                restored: restored.expect("once restored"),
            })),
            remainder: Mutex::new(MaintenanceRemainder {
                old_base: old_base.try_into().expect("old base loan"),
                physical: physical.try_into().expect("physical loan"),
                packing_and_scratch,
                reserved,
            }),
            observation,
        }
    }
}

/// Only the newly selected base/control/replay lanes leave the original loan
/// carrier. Every other byte remains borrowed by the stable runtime wrapper.
struct RecoveryBindLoans<'a> {
    control: &'a mut [u8; 196_608],
    replay: &'a mut [u8; 1_572_864],
    restored: RestoredRetention,
}
/// Linear retained-credit transfer. Only Runtime's once-only loan extraction
/// after original-pool binding constructs this value. The live continuation
/// retains it until CancelReservedFirst/losers/VERIFY/STABILIZE are complete.
pub(in crate::storage) struct RecoveryRetentionBinding<'s, 'a> {
    base: &'s VerifiedPlan4Base<'a>,
    epoch: &'s SelectedNewEpoch,
    doublewrite: &'s SelectedRecoveryDwb,
    restored: RestoredRetention,
}
impl<'s, 'a> RecoveryRetentionBinding<'s, 'a> {
    pub(in crate::storage) fn base(&self) -> &'s VerifiedPlan4Base<'a> {
        self.base
    }
    pub(in crate::storage) fn epoch(&self) -> &'s SelectedNewEpoch {
        self.epoch
    }
    pub(in crate::storage) fn doublewrite(&self) -> &'s SelectedRecoveryDwb {
        self.doublewrite
    }
    pub(in crate::storage) fn cancel_reserved_first(&self) -> bool {
        self.base.view.census().reserved.is_some()
    }
    pub(in crate::storage) fn remaining_wal(&self) -> u64 {
        self.restored.remaining_wal
    }
    pub(in crate::storage) fn remaining_steps(&self) -> u64 {
        self.restored.remaining_steps
    }
    pub(in crate::storage) fn state_digest(&self) -> [u8; 32] {
        self.restored.state_digest
    }
}
struct MaintenanceRemainder<'a> {
    old_base: &'a mut [u8; BASE_BYTES],
    physical: &'a mut [u8; BASE_BYTES],
    packing_and_scratch: &'a mut [u8],
    reserved: &'a mut [u8],
}
pub(crate) struct Plan4RecoveryRuntime<'a, 'm, 'w, W: WriterNamespace> {
    storage: MutableStorageOwner<'m, 'w, W>,
    base: VerifiedPlan4Base<'a>,
    selected: SelectedNewEpoch,
    doublewrite: SelectedRecoveryDwb,
    loans: Mutex<Option<RecoveryBindLoans<'a>>>,
    remainder: Mutex<MaintenanceRemainder<'a>>,
    observation: Plan4RecoveryObservation,
}
impl<W: WriterNamespace> Plan4RecoveryRuntime<'_, '_, '_, W> {
    pub(crate) fn session_fence(&self) -> &SessionFence {
        self.storage.session_fence()
    }
    pub(crate) fn observation(&self) -> &Plan4RecoveryObservation {
        &self.observation
    }
    pub(super) fn pool_seed(&self) -> Result<RecoveryPoolSeed<'_>> {
        let p = Plan4RecoveryPhase::Bind;
        if self.session_fence().is_fenced() {
            return Err(error(p, Plan4RecoveryCause::Fenced));
        }
        let loans = self
            .loans
            .try_lock()
            .map_err(|_| error(p, Plan4RecoveryCause::Busy))?;
        let Some(loan) = loans.as_ref() else {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        };
        let _restored = &loan.restored;
        let image = self.base.view.allocator();
        let cert = self
            .base
            .view
            .certificate(0)
            .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        if self.selected.header.base_lsn != self.base.capture_lsn()
            || self.selected.header.base_digest != self.base.digest
            || self.doublewrite.selected.generation == 0
            || content_digest(image) != cert.full_digest
        {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        Ok(RecoveryPoolSeed {
            allocator: image,
            request: analysis::request(
                self.storage.profile(),
                0,
                cert.header.identity.page.incarnation,
            ),
            lsn: cert.header.lsn,
            digest: cert.full_digest,
            binding: &self.storage.binding,
            fence: self.session_fence(),
        })
    }
}

impl<'a, W: WriterNamespace> Plan4RecoveryRuntime<'a, '_, '_, W> {
    pub(super) fn take_recovery_binding(
        &self,
    ) -> Result<(
        RecoveryRetentionBinding<'_, 'a>,
        &'a mut [u8; 196_608],
        &'a mut [u8; 1_572_864],
    )> {
        let p = Plan4RecoveryPhase::Bind;
        if self.session_fence().is_fenced() {
            return Err(error(p, Plan4RecoveryCause::Fenced));
        }
        if self
            .storage
            .binding
            .load(std::sync::atomic::Ordering::Acquire)
            != super::BOUND
        {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        let mut loans = self
            .loans
            .try_lock()
            .map_err(|_| error(p, Plan4RecoveryCause::Busy))?;
        let loan = loans
            .take()
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        Ok((
            RecoveryRetentionBinding {
                base: &self.base,
                epoch: &self.selected,
                doublewrite: &self.doublewrite,
                restored: loan.restored,
            },
            loan.control,
            loan.replay,
        ))
    }
}

/// A recovered original pool remains closed to foreground split/Begin until
/// its actual old-epoch obligations and physical finalization are complete.
pub(crate) struct Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    runtime: &'s Plan4RecoveryRuntime<'a, 'm, 'w, W>,
    inner: super::plan4_pages::Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    phase: Plan4RecoveryPhase,
    observation: Plan4RecoveryObservation,
    waiting_flush: bool,
    ready: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan4RecoveryProgress {
    pub(crate) phase: Plan4RecoveryPhase,
    pub(crate) progressed: bool,
    pub(crate) ready: bool,
}

impl<'a, 'm, 'w, W: WriterNamespace> Plan4RecoveryRuntime<'a, 'm, 'w, W> {
    pub(crate) fn bind<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        owner: &crate::transaction::concurrency::Owner<'c, 's>,
        core: &'c crate::transaction::concurrency::PageConcurrency<'s>,
        frames: &'p mut [crate::storage::dirty_frame::DirtyFrame<'d>],
        operation: &'o mut [u8; 114_688],
    ) -> Result<Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>
    where
        'a: 's,
    {
        let p = Plan4RecoveryPhase::Bind;
        if !owner.check_binding(core).map_err(|e| {
            let effect = e.effect;
            self.bound_error(p, Plan4RecoveryCause::Concurrency(e), effect)
        })? {
            return Err(self.bound_error(p, Plan4RecoveryCause::Invariant, Effect::Unchanged));
        }
        let _unwind = self.session_fence().unwind_guard();
        let mut phase = Plan4RecoveryPhase::Bind;
        let inner = self.bind_retention(owner, core, frames, operation, &mut phase)?;
        self.wrap_recovered(inner, phase)
    }

    // The seed and original pool constructor finish before the consuming live
    // conversion starts. Neither phase has constructed a recovered facade.
    fn bind_original_pages<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        core: &'c crate::transaction::concurrency::PageConcurrency<'s>,
        frames: &'p mut [crate::storage::dirty_frame::DirtyFrame<'d>],
        operation: &'o mut [u8; 114_688],
    ) -> Result<super::plan4_pages::Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>>
    where
        'a: 's,
    {
        let p = Plan4RecoveryPhase::Bind;
        let seed = self
            .pool_seed()
            .map_err(|e| self.bound_error(p, e.cause, e.effect))?;
        self.storage
            .bind_plan4_recovery_pages(core, frames, operation, &seed)
            .map_err(|e| {
                let effect = e.effect;
                self.bound_error(p, Plan4RecoveryCause::Binding(e), effect)
            })
    }

    fn bind_retention<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        owner: &crate::transaction::concurrency::Owner<'c, 's>,
        core: &'c crate::transaction::concurrency::PageConcurrency<'s>,
        frames: &'p mut [crate::storage::dirty_frame::DirtyFrame<'d>],
        operation: &'o mut [u8; 114_688],
        phase: &mut Plan4RecoveryPhase,
    ) -> Result<super::plan4_pages::Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>
    where
        'a: 's,
    {
        let p = Plan4RecoveryPhase::Bind;
        let pages = self.bind_original_pages(core, frames, operation)?;
        let (binding, control, replay) = self
            .take_recovery_binding()
            .map_err(|e| self.bound_error(p, e.cause, Effect::CallerMustResolve))?;
        *phase = if binding.cancel_reserved_first() {
            Plan4RecoveryPhase::CancelReservedFirst
        } else {
            Plan4RecoveryPhase::Losers
        };
        pages
            .into_recovered_retention(
                owner,
                super::plan4_pages::EmptyRetentionControlLoan { control, replay },
                binding,
            )
            .map_err(|e| self.bound_error(p, Plan4RecoveryCause::Retention(e.cause), e.effect))
    }

    // All conversion locals have ended before these wide return values exist.
    // The same live owner is moved into the facade, with no duplicate pool.
    fn wrap_recovered<'s, 'c, 'p, 'd, 'o>(
        &'s self,
        inner: super::plan4_pages::Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        phase: Plan4RecoveryPhase,
    ) -> Result<Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        Ok(Plan4Recovered {
            runtime: self,
            inner,
            phase,
            observation: self.observation,
            waiting_flush: false,
            ready: false,
        })
    }
    fn bound_error(
        &self,
        phase: Plan4RecoveryPhase,
        cause: Plan4RecoveryCause,
        effect: Effect,
    ) -> Plan4RecoveryError {
        if effect != Effect::Unchanged {
            self.fence_bound();
        }
        Plan4RecoveryError {
            phase,
            cause,
            effect,
            database_effect: Effect::CallerMustResolve,
            lease_acquired: true,
        }
    }
    fn fence_bound(&self) {
        self.session_fence().fence();
        self.storage.wal().completion.notify_all();
    }
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn observation(&self) -> &Plan4RecoveryObservation {
        &self.observation
    }
    pub(crate) fn step(
        &mut self,
        owner: &mut crate::transaction::concurrency::Owner<'c, 's>,
        flusher: &mut crate::transaction::concurrency::Owner<'c, 's>,
    ) -> Result<Plan4RecoveryProgress> {
        use super::plan4_pages::{RetentionFlushStep, RetentionRecoveryStep};
        if self.ready {
            return Ok(Plan4RecoveryProgress {
                phase: Plan4RecoveryPhase::Handoff,
                progressed: false,
                ready: true,
            });
        }
        let runtime = self.runtime;
        let _unwind = runtime.session_fence().unwind_guard();
        self.observation.phase = self.phase;
        if self.waiting_flush {
            let (_, _, mut flush) = self.inner.split();
            let step = flush.continue_flush(flusher).map_err(|e| {
                runtime.bound_error(self.phase, Plan4RecoveryCause::Retention(e.cause), e.effect)
            })?;
            self.waiting_flush = matches!(
                step,
                RetentionFlushStep::WaitingWal
                    | RetentionFlushStep::WaitingSource
                    | RetentionFlushStep::RetiredPendingFinalize
            );
            return Ok(Plan4RecoveryProgress {
                phase: self.phase,
                progressed: !self.waiting_flush,
                ready: false,
            });
        }
        let step = self.inner.recovery_step(owner).map_err(|e| {
            runtime.bound_error(self.phase, Plan4RecoveryCause::Retention(e.cause), e.effect)
        })?;
        match step {
            RetentionRecoveryStep::CancelledReserved => {
                self.phase = Plan4RecoveryPhase::Losers;
            }
            RetentionRecoveryStep::RollbackStarted { .. }
            | RetentionRecoveryStep::Aborted { .. }
            | RetentionRecoveryStep::Finished { .. } => {
                if self.phase == Plan4RecoveryPhase::CancelReservedFirst {
                    return Err(runtime.bound_error(
                        self.phase,
                        Plan4RecoveryCause::Invariant,
                        Effect::CallerMustResolve,
                    ));
                }
                self.phase = Plan4RecoveryPhase::Losers;
            }
            RetentionRecoveryStep::FlushRequired { target, .. } => {
                if self.phase == Plan4RecoveryPhase::CancelReservedFirst {
                    return Err(runtime.bound_error(
                        self.phase,
                        Plan4RecoveryCause::Invariant,
                        Effect::CallerMustResolve,
                    ));
                }
                let (_, _, mut flush) = self.inner.split();
                let step = flush.try_flush(flusher, &target).map_err(|e| {
                    runtime.bound_error(
                        self.phase,
                        Plan4RecoveryCause::Retention(e.cause),
                        e.effect,
                    )
                })?;
                self.waiting_flush = matches!(
                    step,
                    RetentionFlushStep::WaitingWal
                        | RetentionFlushStep::WaitingSource
                        | RetentionFlushStep::RetiredPendingFinalize
                );
            }
            RetentionRecoveryStep::Verified => {
                if self.phase == Plan4RecoveryPhase::CancelReservedFirst {
                    return Err(runtime.bound_error(
                        self.phase,
                        Plan4RecoveryCause::Invariant,
                        Effect::CallerMustResolve,
                    ));
                }
                self.phase = Plan4RecoveryPhase::Verify;
                self.inner.recovery_verify(owner).map_err(|e| {
                    runtime.bound_error(
                        self.phase,
                        Plan4RecoveryCause::Retention(e.cause),
                        e.effect,
                    )
                })?;
                let (selected, last_issued) =
                    self.inner.recovery_doublewrite_selection().map_err(|e| {
                        runtime.bound_error(
                            self.phase,
                            Plan4RecoveryCause::Retention(e.cause),
                            e.effect,
                        )
                    })?;
                self.phase = Plan4RecoveryPhase::Stabilize;
                runtime
                    .stabilize_live(&mut self.observation, selected, last_issued)
                    .map_err(|e| {
                        runtime.bound_error(e.phase, e.cause, Effect::CallerMustResolve)
                    })?;
                self.inner.recovery_verify(owner).map_err(|e| {
                    runtime.bound_error(
                        self.phase,
                        Plan4RecoveryCause::Retention(e.cause),
                        e.effect,
                    )
                })?;
                self.phase = Plan4RecoveryPhase::Handoff;
                self.ready = true;
            }
        }
        self.observation.phase = self.phase;
        Ok(Plan4RecoveryProgress {
            phase: self.phase,
            progressed: !self.waiting_flush,
            ready: self.ready,
        })
    }
    /// Consumes the closed facade only after real cleanup/physical readback and
    /// control stabilization. Same owner, pool, generation, loans and fence move.
    pub(crate) fn into_live(
        self,
    ) -> std::result::Result<
        super::plan4_pages::Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        Self,
    > {
        if self.ready {
            Ok(self.inner)
        } else {
            Err(self)
        }
    }
}
