//! Actual one-image DWB execution after sealed capture and WAL durability.
//! The exclusive bound control retains this state through RAM finalization.

use super::super::allocation_reservation::operation::SemanticError;
use super::{MutableStorageOwner, RetentionFlushAuthority, StatusFlushAuthority};
use crate::storage::{
    allocation_page::{self, AllocationError},
    control::{
        self, ControlError, ControlKind, ControlRef, FileMapEntry, Profile, SelectedControl,
        SelectorRole,
    },
    dirty_frame::{CapturedPage, FrameStamp, TargetRequest},
    doublewrite_control::{
        self as dwb, ControlHeader, ControlState, DoublewriteError, PendingImage,
    },
    page::{PAGE_BYTES, PageKind, PageLsn},
    session_fence::SessionFence,
    wal_io::{DurableWal, WalFrameLocation},
};
use foundation::{
    codec::content_digest,
    error::Effect,
    io::{read_exact_at, sync, write_all_at},
    namespace::{FileIdentity, FileName, NamespaceError, OpenMode, Selector, WriterNamespace},
};
use std::{
    mem::size_of,
    sync::{MutexGuard, atomic::AtomicU8},
};

pub(super) const WORKSPACE_BYTES: usize = 4 << 20;
const SLOT_START: usize = PAGE_BYTES;
const CONTROLS_START: usize = SLOT_START + dwb::SLOT_BYTES;
const CURRENT_CONTROL: usize = CONTROLS_START + 3 * PAGE_BYTES;
const SELECTORS_START: usize = CURRENT_CONTROL + PAGE_BYTES;
const CURRENT_SELECTOR: usize = SELECTORS_START + 3 * control::SELECTOR_BYTES;
const HOME_START: usize = CURRENT_SELECTOR + control::SELECTOR_BYTES;
const ROOT_START: usize = HOME_START + PAGE_BYTES;
pub(super) const WORKSPACE_USED: usize = ROOT_START + 2 * PAGE_BYTES;
const IMAGES_BYTES: u64 = (dwb::POOL_BYTES - 4 * PAGE_BYTES) as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Ready,
    Prepared,
    Executing,
    Retired,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionStep {
    StageWrite,
    StageSync,
    BankWrite,
    BankSync,
    SelectorCreate,
    SelectorWrite,
    SelectorSync,
    Publish,
    DirectorySync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Preflight,
    Prepare,
    Selection { index: u8, step: SelectionStep },
    ImageWrite,
    ImageSync,
    HomeWrite,
    HomeSync,
    Finalize,
}

#[derive(Clone, Debug)]
pub(super) enum Cause {
    Busy,
    Capacity,
    Fenced,
    Poisoned,
    Invariant,
    IdentityMismatch,
    LengthMismatch,
    SelectionMismatch,
    ContentMismatch,
    MissingMapping,
    Residue,
    Control(ControlError),
    Doublewrite(DoublewriteError),
    Allocation(AllocationError),
    Status(SemanticError),
    Namespace(NamespaceError),
    Io(foundation::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IoObservation {
    pub(super) state: State,
    pub(super) phase: Phase,
    pub(super) known: SelectedControl,
    pub(super) possible: [Option<SelectedControl>; 2],
    pub(super) last_issued: u64,
    pub(super) attempted: u8,
    pub(super) completed: u8,
    pub(super) homes_synced: bool,
    pub(super) retirement_selected: bool,
}

#[derive(Clone, Debug)]
pub(super) struct FlushIoError {
    pub(super) phase: Phase,
    pub(super) cause: Cause,
    pub(super) effect: Effect,
}
pub(super) type Result<T> = std::result::Result<T, FlushIoError>;
type Inner<T> = std::result::Result<T, Cause>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetClass {
    Allocator,
    Status,
    MixedStatus,
    MixedUndo,
    Retention,
}

/// This private selector borrows an actual upper authority. A page kind alone
/// cannot select the Status predicates or manufacture the authority.
#[derive(Clone, Copy)]
enum PolicyRef<'a, 'c, 's> {
    Allocator,
    Status(&'a StatusFlushAuthority<'c, 's>),
    Retention(&'a RetentionFlushAuthority<'c, 's>),
}
impl PolicyRef<'_, '_, '_> {
    fn class(self) -> TargetClass {
        match self {
            Self::Allocator => TargetClass::Allocator,
            Self::Retention(_) => TargetClass::Retention,
            Self::Status(authority) => {
                if authority.mixed() {
                    if authority.request().kind == PageKind::Undo {
                        TargetClass::MixedUndo
                    } else {
                        TargetClass::MixedStatus
                    }
                } else {
                    TargetClass::Status
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
// stamp.identity plus kind/level is the complete TargetRequest. Keeping its
// identity once avoids a second owned PageIdentity in each return carrier.
struct Prepared {
    class: TargetClass,
    kind: PageKind,
    level: u8,
    stamp: FrameStamp,
    attempt: u64,
    requirement: WalFrameLocation,
    selections: [SelectedControl; 3],
    batch: u64,
    digest: [u8; 32],
    mapping: FileMapEntry,
    home_identity: FileIdentity,
    home_offset: u64,
}

struct MappedHome {
    mapping: FileMapEntry,
    identity: FileIdentity,
    offset: u64,
}

/// Constructible only after this executor confirms home sync and retirement.
/// Not Clone/Copy: the caller borrows it to authorize conditional RAM finalize.
pub(super) struct RetiredBatch<'s> {
    binding: &'s AtomicU8,
    attempt: u64,
    stamp: FrameStamp,
    kind: PageKind,
    level: u8,
    selected: SelectedControl,
    batch: u64,
}
impl RetiredBatch<'_> {
    pub(super) fn matches(&self, capture: &CapturedPage<'_, '_>) -> bool {
        std::ptr::eq(self.binding, capture.binding())
            && self.attempt == capture.attempt()
            && self.stamp == capture.stamp()
            && self.stamp.identity == capture.request().identity
            && self.kind == capture.request().kind
            && self.level == capture.request().level
    }
}

/// Owns only the original borrowed workspace; never creates a second arena.
pub(super) struct FlushIo<'s, 'w> {
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    workspace: &'w mut [u8; WORKSPACE_BYTES],
    observation: IoObservation,
    prepared: Option<Prepared>,
    failure: Option<FlushIoError>,
}

/// Holds exclusive I/O state while the parent finishes RAM cleanup/claim return.
/// Dropping an incomplete transition leaves the actual retired state unchanged.
pub(super) struct ReadyTransition<'a, 's, 'w> {
    io: &'a mut FlushIo<'s, 'w>,
}
impl ReadyTransition<'_, '_, '_> {
    pub(super) fn complete(self) {
        self.io.prepared = None;
        self.io.observation.state = State::Ready;
    }
}

impl<'s, 'w> FlushIo<'s, 'w> {
    pub(super) fn new(
        initial: SelectedControl,
        workspace: &'w mut [u8; WORKSPACE_BYTES],
        binding: &'s AtomicU8,
        fence: &'s SessionFence,
    ) -> Self {
        Self {
            binding,
            fence,
            workspace,
            observation: IoObservation {
                state: State::Ready,
                phase: Phase::Preflight,
                known: initial,
                possible: [Some(initial), None],
                last_issued: 0,
                attempted: 0,
                completed: 0,
                homes_synced: false,
                retirement_selected: false,
            },
            prepared: None,
            failure: None,
        }
    }

    /// Only actual recovery selection can supply this sealed DWB issuance.
    /// An Empty record can still have burned a batch; preserve that high-water
    /// before the first new capture instead of treating it as fresh genesis.
    pub(super) fn from_plan4_recovery(
        selected: &super::plan4_recovery::SelectedRecoveryDwb,
        workspace: &'w mut [u8; WORKSPACE_BYTES],
        binding: &'s AtomicU8,
        fence: &'s SessionFence,
    ) -> Self {
        let mut io = Self::new(selected.selected(), workspace, binding, fence);
        io.observation.last_issued = selected.last_issued();
        io
    }

    #[cfg(test)]
    pub(super) fn test_pair_origin_fault(&mut self) {
        static FOREIGN: AtomicU8 = AtomicU8::new(0);
        self.binding = &FOREIGN;
    }

    /// Compare the actual retained owner, without manufacturing provenance.
    pub(super) fn uses_origin(&self, binding: &AtomicU8, fence: &SessionFence) -> bool {
        std::ptr::eq(self.binding, binding) && std::ptr::eq(self.fence, fence)
    }

    pub(super) fn snapshot(&self) -> IoObservation {
        self.observation
    }

    fn error(&self, cause: Cause, effect: Effect) -> FlushIoError {
        FlushIoError {
            phase: self.observation.phase,
            cause,
            effect,
        }
    }

    /// Cheap stored-state and concrete resource checks. Performs no reads/I/O.
    pub(super) fn preflight<W: WriterNamespace>(&mut self) -> Result<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.fence.is_fenced() {
            return Err(FlushIoError {
                phase: Phase::Preflight,
                cause: Cause::Fenced,
                effect: Effect::CallerMustResolve,
            });
        }
        if self.observation.state != State::Ready {
            return Err(FlushIoError {
                phase: Phase::Preflight,
                cause: Cause::Busy,
                effect: Effect::Unchanged,
            });
        }
        let selected = self.observation.known;
        if selected.reference.kind != ControlKind::Doublewrite
            || selected.reference.slot > 1
            || selected.reference.index != 0
            || selected.reference.encoded_len != 160
            || selected.generation == 0
            || selected.reference.generation == 0
            || self.observation.possible != [Some(selected), None]
        {
            self.observation.phase = Phase::Preflight;
            return self.complete_result(Err(Cause::Invariant));
        }
        if WORKSPACE_USED != 164_000
            || WORKSPACE_USED > WORKSPACE_BYTES
            || selected.generation.checked_add(3).is_none()
            || selected.reference.generation.checked_add(3).is_none()
            || self.observation.last_issued.checked_add(1).is_none()
            || !fits::<W>()
        {
            return Err(FlushIoError {
                phase: Phase::Preflight,
                cause: Cause::Capacity,
                effect: Effect::Unchanged,
            });
        }
        Ok(())
    }

    /// Only the bound capture owner receives this private destination.
    /// NoDirty/Busy leaves Ready; prepare seals bytes before any operational I/O.
    pub(super) fn image_mut(&mut self) -> Result<&mut [u8; PAGE_BYTES]> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.fence.is_fenced() {
            return Err(self.error(Cause::Fenced, Effect::CallerMustResolve));
        }
        if self.observation.state != State::Ready {
            return Err(self.error(Cause::Busy, Effect::Unchanged));
        }
        Ok((&mut self.workspace[..PAGE_BYTES])
            .try_into()
            .expect("fixed capture interval"))
    }

    pub(super) fn prepare<W: WriterNamespace>(
        &mut self,
        storage: &MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, '_>,
    ) -> Result<()> {
        self.prepare_with_policy(storage, capture, PolicyRef::Allocator)
    }

    /// Temporary disjoint loans; no tail is retained alongside the whole DWB
    /// workspace. The semantic authority must finish before prepare/execute.
    pub(super) fn retention_source_workspace(&mut self) -> Result<(&[u8; PAGE_BYTES], &mut [u8])> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.fence.is_fenced() {
            return Err(self.error(Cause::Fenced, Effect::CallerMustResolve));
        }
        if self.observation.state != State::Ready {
            return Err(self.error(Cause::Busy, Effect::Unchanged));
        }
        let (used, tail) = self.workspace.split_at_mut(WORKSPACE_USED);
        let image: &[u8; PAGE_BYTES] = (&used[..PAGE_BYTES]).try_into().expect("fixed DWB image");
        Ok((image, tail))
    }

    pub(super) fn prepare_retention<W: WriterNamespace>(
        &mut self,
        storage: &MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, '_>,
        authority: &RetentionFlushAuthority<'_, '_>,
    ) -> Result<()> {
        self.prepare_with_policy(storage, capture, PolicyRef::Retention(authority))
    }

    pub(super) fn prepare_status<W: WriterNamespace>(
        &mut self,
        storage: &MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, '_>,
        authority: &StatusFlushAuthority<'_, '_>,
    ) -> Result<()> {
        self.prepare_with_policy(storage, capture, PolicyRef::Status(authority))
    }

    fn prepare_with_policy<W: WriterNamespace>(
        &mut self,
        storage: &MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, '_>,
        policy: PolicyRef<'_, '_, '_>,
    ) -> Result<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let _unwind = self.fence.unwind_guard();
        let result = (|| {
            if self.observation.state != State::Ready || self.prepared.is_some() {
                return Err(Cause::Invariant);
            }
            self.preflight::<W>().map_err(|error| error.cause)?;
            self.observation.phase = Phase::Prepare;
            live(storage)?;
            validate_capture(storage, capture, policy)?;
            if !std::ptr::eq(self.binding, capture.binding())
                || !std::ptr::eq(self.fence, storage.session_fence())
            {
                return Err(Cause::IdentityMismatch);
            }
            self.observation.attempted = 0;
            self.observation.completed = 0;
            self.observation.homes_synced = false;
            self.observation.retirement_selected = false;
            // Image checksum/Free decoding ends before FILEMAP and home I/O.
            validate_image(storage, self.workspace, capture, policy)?;
            read_selection(storage, self.workspace, &self.observation)?;
            let home = {
                let mapping = read_mapping(storage, self.workspace, capture, policy)?;
                let offset = capture
                    .stamp()
                    .identity
                    .page
                    .page_no
                    .checked_mul(PAGE_BYTES as u64)
                    .filter(|offset| {
                        offset
                            .checked_add(PAGE_BYTES as u64)
                            .is_some_and(|end| end <= mapping.mapped_bytes)
                    })
                    .ok_or(Cause::Capacity)?;
                let identity =
                    read_home(storage, self.workspace, &mapping, offset, capture, policy)?;
                MappedHome {
                    mapping,
                    identity,
                    offset,
                }
            };
            // No decoder or File remains alive. Construct Prepared directly in
            // retained storage, avoiding an extra full Prepared return carrier.
            self.encode_prepared(storage.profile(), capture, &home, policy.class())?;
            self.observation.state = State::Prepared;
            live(storage)
        })();
        self.complete_result(result)
    }

    fn encode_prepared(
        &mut self,
        profile: Profile,
        capture: &CapturedPage<'_, '_>,
        home: &MappedHome,
        class: TargetClass,
    ) -> Inner<()> {
        let batch = self
            .observation
            .last_issued
            .checked_add(1)
            .ok_or(Cause::Capacity)?;
        let digest = content_digest(&self.workspace[..PAGE_BYTES]);
        let selections = encode_selections(
            self.workspace,
            profile,
            self.observation.known,
            batch,
            capture,
            digest,
        )?;
        {
            let (image, rest) = self.workspace.split_at_mut(PAGE_BYTES);
            let (slot, controls) = rest.split_at_mut(dwb::SLOT_BYTES);
            let pending = control::decode_selected_doublewrite(
                &controls[PAGE_BYTES..PAGE_BYTES + 248],
                selections[1].reference,
                profile,
            )
            .map_err(Cause::Control)?;
            dwb::encode_slot(&pending, 0, image, slot).map_err(Cause::Doublewrite)?;
        }
        self.prepared = Some(Prepared {
            class,
            kind: capture.request().kind,
            level: capture.request().level,
            stamp: capture.stamp(),
            attempt: capture.attempt(),
            requirement: capture.requirement().location(),
            selections,
            batch,
            digest,
            mapping: home.mapping,
            home_identity: home.identity,
            home_offset: home.offset,
        });
        Ok(())
    }

    pub(super) fn execute<W: WriterNamespace>(
        &mut self,
        storage: &'s MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, 's>,
        durable: &DurableWal<'s>,
    ) -> Result<RetiredBatch<'s>> {
        self.execute_with_policy(storage, capture, PolicyRef::Allocator, durable)
    }

    pub(super) fn execute_status<W: WriterNamespace>(
        &mut self,
        storage: &'s MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, 's>,
        authority: &StatusFlushAuthority<'_, 's>,
        durable: &DurableWal<'s>,
    ) -> Result<RetiredBatch<'s>> {
        self.execute_with_policy(storage, capture, PolicyRef::Status(authority), durable)
    }

    pub(super) fn execute_retention<W: WriterNamespace>(
        &mut self,
        storage: &'s MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, 's>,
        authority: &RetentionFlushAuthority<'_, 's>,
        durable: &DurableWal<'s>,
    ) -> Result<RetiredBatch<'s>> {
        self.execute_with_policy(storage, capture, PolicyRef::Retention(authority), durable)
    }

    fn execute_with_policy<W: WriterNamespace>(
        &mut self,
        storage: &'s MutableStorageOwner<'_, '_, W>,
        capture: &CapturedPage<'_, 's>,
        policy: PolicyRef<'_, '_, 's>,
        durable: &DurableWal<'s>,
    ) -> Result<RetiredBatch<'s>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let _unwind = self.fence.unwind_guard();
        let result = (|| {
            live(storage)?;
            validate_capture(storage, capture, policy)?;
            if !std::ptr::eq(self.binding, capture.binding())
                || !std::ptr::eq(self.fence, storage.session_fence())
            {
                return Err(Cause::IdentityMismatch);
            }
            let prepared = self.prepared.as_ref().ok_or(Cause::Invariant)?;
            if self.observation.state != State::Prepared
                || prepared.class != policy.class()
                || !prepared_matches(prepared, capture)
                || !durable.confirms(&capture.requirement())
                || content_digest(&self.workspace[..PAGE_BYTES]) != prepared.digest
            {
                return Err(Cause::Invariant);
            }
            // Recheck the exact trusted mapping without namespace or FILEMAP
            // reads. These temporary identities end before the Prepared copy.
            validate_mapping(storage, &prepared.mapping, capture, policy)?;
            let prepared = *prepared;
            self.observation.state = State::Executing;
            select(storage, self.workspace, &mut self.observation, &prepared, 0)?;
            {
                let mut file = open_exact(
                    storage,
                    &name("d3-dwb-images"),
                    IMAGES_BYTES,
                    OpenMode::ReadWrite,
                )?
                .0;
                run(storage, &mut self.observation, Phase::ImageWrite, || {
                    write_all_at(
                        &mut file,
                        &self.workspace[SLOT_START..SLOT_START + dwb::SLOT_BYTES],
                        0,
                    )
                    .map_err(Cause::Io)
                })?;
                run(storage, &mut self.observation, Phase::ImageSync, || {
                    sync(&mut file).map_err(Cause::Io)
                })?;
            }
            select(storage, self.workspace, &mut self.observation, &prepared, 1)?;
            {
                let (mut file, identity) = open_exact(
                    storage,
                    &control::data_file_name(prepared.mapping.file),
                    prepared.mapping.mapped_bytes,
                    OpenMode::ReadWrite,
                )?;
                if identity != prepared.home_identity {
                    return Err(Cause::IdentityMismatch);
                }
                run(storage, &mut self.observation, Phase::HomeWrite, || {
                    write_all_at(
                        &mut file,
                        &self.workspace[..PAGE_BYTES],
                        prepared.home_offset,
                    )
                    .map_err(Cause::Io)
                })?;
                run(storage, &mut self.observation, Phase::HomeSync, || {
                    sync(&mut file).map_err(Cause::Io)
                })?;
            }
            self.observation.homes_synced = true;
            select(storage, self.workspace, &mut self.observation, &prepared, 2)?;
            if self.observation.attempted != 31 || self.observation.completed != 31 {
                return Err(Cause::Invariant);
            }
            self.observation.state = State::Retired;
            Ok(RetiredBatch {
                binding: capture.binding(),
                attempt: prepared.attempt,
                stamp: prepared.stamp,
                kind: prepared.kind,
                level: prepared.level,
                selected: prepared.selections[2],
                batch: prepared.batch,
            })
        })();
        self.complete_result(result)
    }

    fn complete_result<T>(&mut self, result: Inner<T>) -> Result<T> {
        result.map_err(|cause| {
            self.fence.fence();
            self.observation.state = State::Failed;
            let error = self.error(cause, Effect::CallerMustResolve);
            self.failure = Some(error.clone());
            error
        })
    }

    /// Validate before claim return; the returned borrow prevents intervening I/O.
    pub(super) fn ready_transition(
        &mut self,
        retired: &RetiredBatch<'_>,
    ) -> Result<ReadyTransition<'_, 's, 'w>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.fence.is_fenced() {
            return Err(self.error(Cause::Fenced, Effect::CallerMustResolve));
        }
        let valid = self.prepared.is_some_and(|prepared| {
            self.observation.state == State::Retired
                && std::ptr::eq(self.binding, retired.binding)
                && self.observation.known == retired.selected
                && self.observation.last_issued == retired.batch
                && self.observation.homes_synced
                && self.observation.retirement_selected
                && prepared.attempt == retired.attempt
                && prepared.stamp == retired.stamp
                && prepared.kind == retired.kind
                && prepared.level == retired.level
                && match prepared.class {
                    TargetClass::Allocator => retired.kind == PageKind::Allocation,
                    TargetClass::Status | TargetClass::MixedStatus => {
                        retired.kind == PageKind::TransactionStatus
                    }
                    TargetClass::MixedUndo => retired.kind == PageKind::Undo,
                    TargetClass::Retention => matches!(
                        retired.kind,
                        PageKind::Allocation
                            | PageKind::RetentionRoot
                            | PageKind::RetentionJobs
                            | PageKind::TransactionStatus
                            | PageKind::Undo
                            | PageKind::CatalogLeaf
                            | PageKind::TreeAnchor
                            | PageKind::RowLeaf
                    ),
                }
                && prepared.selections[2] == retired.selected
                && prepared.batch == retired.batch
        });
        if !valid {
            self.fence.fence();
            self.observation.phase = Phase::Finalize;
            self.observation.state = State::Failed;
            let error = self.error(Cause::Invariant, Effect::CallerMustResolve);
            self.failure = Some(error.clone());
            return Err(error);
        }
        Ok(ReadyTransition { io: self })
    }
}

fn prepared_matches(prepared: &Prepared, capture: &CapturedPage<'_, '_>) -> bool {
    prepared.stamp == capture.stamp()
        && prepared.stamp.identity == capture.request().identity
        && prepared.kind == capture.request().kind
        && prepared.level == capture.request().level
        && prepared.attempt == capture.attempt()
        && prepared.requirement == capture.requirement().location()
}

fn validate_capture<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    capture: &CapturedPage<'_, '_>,
    policy: PolicyRef<'_, '_, '_>,
) -> Inner<()> {
    let stamp = capture.stamp();
    let request = capture.request();
    if !std::ptr::eq(capture.binding(), &storage.binding)
        || !capture.requirement().uses_fence(storage.session_fence())
        || capture.attempt() == 0
        || stamp.generation == 0
        || stamp.modification_counter == 0
        || stamp.page_lsn.0 == 0
        || stamp
            .rec_lsn
            .is_none_or(|lsn| lsn.0 == 0 || lsn.0 > stamp.page_lsn.0)
        || request.identity != stamp.identity
        || request.level != 0
        || stamp.identity.database != storage.profile().spec().namespace.database
        || capture.requirement().location().lsn != stamp.page_lsn.0
    {
        return Err(Cause::IdentityMismatch);
    }
    match policy {
        PolicyRef::Allocator if request.kind != PageKind::Allocation => {
            Err(Cause::IdentityMismatch)
        }
        PolicyRef::Status(authority)
            if !authority.accepts_kind(request.kind)
                || !(1..64).contains(&request.identity.page.page_no)
                || !authority.matching(capture) =>
        {
            Err(Cause::IdentityMismatch)
        }
        PolicyRef::Retention(authority) if !authority.matching(capture) => {
            Err(Cause::IdentityMismatch)
        }
        _ => Ok(()),
    }
}

fn validate_image<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &[u8; WORKSPACE_BYTES],
    capture: &CapturedPage<'_, '_>,
    policy: PolicyRef<'_, '_, '_>,
) -> Inner<()> {
    let lsn = match policy {
        PolicyRef::Allocator => {
            let page = allocation_page::decode_allocation_page(
                &workspace[..PAGE_BYTES],
                capture.stamp().identity,
                storage.profile().spec().namespace,
            )
            .map_err(Cause::Allocation)?;
            page.header().lsn
        }
        PolicyRef::Status(authority) => authority
            .image_lsn(
                (&workspace[..PAGE_BYTES])
                    .try_into()
                    .expect("fixed capture interval"),
            )
            .map_err(Cause::Status)?,
        PolicyRef::Retention(authority) => authority
            .image_lsn(
                (&workspace[..PAGE_BYTES])
                    .try_into()
                    .expect("fixed capture interval"),
            )
            .map_err(Cause::Status)?,
    };
    if lsn != capture.stamp().page_lsn {
        return Err(Cause::ContentMismatch);
    }
    Ok(())
}

fn live<W: WriterNamespace>(storage: &MutableStorageOwner<'_, '_, W>) -> Inner<()> {
    // Atomic/poison observations acquire no DB state mutex.
    let wal = storage.wal();
    if wal._writer.is_poisoned() || wal.file.is_poisoned() || wal.append.is_poisoned() {
        return Err(Cause::Poisoned);
    }
    if storage.session_fence().is_fenced() {
        return Err(Cause::Fenced);
    }
    Ok(())
}

fn port<'a, W: WriterNamespace>(
    storage: &'a MutableStorageOwner<'_, '_, W>,
) -> Inner<MutexGuard<'a, W>> {
    live(storage)?;
    let writer = storage.wal()._writer.lock().map_err(|_| Cause::Poisoned)?;
    if storage.session_fence().is_fenced() {
        return Err(Cause::Fenced);
    }
    Ok(writer)
}

fn name(value: &'static str) -> FileName {
    FileName::new(value).expect("fixed canonical operational filename")
}

fn open_exact<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    name: &FileName,
    length: u64,
    mode: OpenMode,
) -> Inner<(W::File, FileIdentity)> {
    let mut writer = port(storage)?;
    let (file, opened) = writer
        .open_existing_with_metadata(name, mode)
        .map_err(Cause::Namespace)?;
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
    // End the I/O-port ownership before the returned local file is used.
    drop(writer);
    if opened.length != length {
        return Err(Cause::LengthMismatch);
    }
    Ok((file, opened.identity))
}

fn read_file<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    name: &FileName,
    length: u64,
    output: &mut [u8],
    offset: u64,
) -> Inner<()> {
    let (mut file, _) = open_exact(storage, name, length, OpenMode::ReadOnly)?;
    read_exact_at(&mut file, output, offset).map_err(Cause::Io)
}

/// The storage owner's bound Profile selects the profile grammar through
/// `control::decode_installed_profile`; a valid but different installed
/// profile reports `Control(IdentityMismatch)`, as a namespace mismatch does.
fn installed_profile(bytes: &[u8], bound: Profile) -> Inner<Profile> {
    control::decode_installed_profile(bytes, bound).map_err(Cause::Control)
}

fn read_selection<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &mut [u8; WORKSPACE_BYTES],
    observed: &IoObservation,
) -> Inner<()> {
    let selector = &mut workspace[CURRENT_SELECTOR..CURRENT_SELECTOR + control::SELECTOR_BYTES];
    read_file(
        storage,
        &name("d3-profile"),
        control::PROFILE_BYTES as u64,
        &mut selector[..control::PROFILE_BYTES],
        0,
    )?;
    let profile = installed_profile(&selector[..control::PROFILE_BYTES], storage.profile())?;
    read_file(
        storage,
        &name("DWB"),
        control::SELECTOR_BYTES as u64,
        selector,
        0,
    )?;
    let selected = control::decode_selector(selector, SelectorRole::Doublewrite, profile)
        .map_err(Cause::Control)?;
    if selected != observed.known {
        return Err(Cause::SelectionMismatch);
    }
    let bytes = &mut workspace[CURRENT_CONTROL..CURRENT_CONTROL + PAGE_BYTES];
    read_file(
        storage,
        &control::control_file_name(selected.reference).map_err(Cause::Control)?,
        PAGE_BYTES as u64,
        bytes,
        0,
    )?;
    let length = selected.reference.encoded_len as usize;
    if length != 160 || bytes[length..].iter().any(|&byte| byte != 0) {
        return Err(Cause::ContentMismatch);
    }
    let view = control::decode_selected_doublewrite(&bytes[..length], selected.reference, profile)
        .map_err(Cause::Control)?;
    if view.header().state != ControlState::Empty
        || view.header().batch_id != observed.last_issued
        || view.header().max_lsn != PageLsn(0)
        || view.len() != 0
    {
        return Err(Cause::SelectionMismatch);
    }
    if port(storage)?
        .metadata(&name("d3-dwb-select-stage"))
        .map_err(Cause::Namespace)?
        .is_some()
    {
        return Err(Cause::Residue);
    }
    Ok(())
}

fn read_mapping<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &mut [u8; WORKSPACE_BYTES],
    capture: &CapturedPage<'_, '_>,
    policy: PolicyRef<'_, '_, '_>,
) -> Inner<FileMapEntry> {
    // The cursor and its codec carriers return before policy identity checks.
    let entry = read_mapping_entry(storage, workspace, capture.request().identity.page.file)?;
    validate_mapping(storage, &entry, capture, policy)?;
    Ok(entry)
}

fn read_mapping_entry<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &mut [u8; WORKSPACE_BYTES],
    requested_file: crate::storage::page::FileId,
) -> Inner<FileMapEntry> {
    let selector = &mut workspace[CURRENT_SELECTOR..CURRENT_SELECTOR + control::SELECTOR_BYTES];
    read_file(
        storage,
        &name("FILEMAP"),
        control::SELECTOR_BYTES as u64,
        selector,
        0,
    )?;
    let profile = storage.profile();
    let selected = control::decode_selector(selector, SelectorRole::FileMap, profile)
        .map_err(Cause::Control)?;
    if selected != storage.plan.filemap {
        return Err(Cause::SelectionMismatch);
    }
    let (root, scratch) = workspace[ROOT_START..WORKSPACE_USED].split_at_mut(PAGE_BYTES);
    let root_length = selected.reference.encoded_len as usize;
    if root_length > root.len() {
        return Err(Cause::Capacity);
    }
    let (mut container, _) = open_exact(
        storage,
        &control::control_file_name(selected.reference).map_err(Cause::Control)?,
        control::CONTAINER_BYTES,
        OpenMode::ReadOnly,
    )?;
    read_exact_at(&mut container, &mut root[..root_length], 0).map_err(Cause::Io)?;
    let root = control::decode_filemap_root(&root[..root_length], selected.reference, profile)
        .map_err(Cause::Control)?;
    let mut cursor = control::FileMapCursor::new(root);
    let mut found = None;
    while let Some(location) = cursor.next() {
        let length = location.reference.encoded_len as usize;
        if length > scratch.len() {
            return Err(Cause::Capacity);
        }
        read_exact_at(&mut container, &mut scratch[..length], location.offset)
            .map_err(Cause::Io)?;
        let chunk = cursor.accept(&scratch[..length]).map_err(Cause::Control)?;
        for index in 0..chunk.summary().files as usize {
            let entry = chunk.entry(index).ok_or(Cause::MissingMapping)?;
            if entry.file == requested_file && found.replace(entry).is_some() {
                return Err(Cause::Invariant);
            }
        }
    }
    cursor.finish().map_err(Cause::Control)?;
    found.ok_or(Cause::MissingMapping)
}

fn validate_mapping<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    entry: &FileMapEntry,
    capture: &CapturedPage<'_, '_>,
    policy: PolicyRef<'_, '_, '_>,
) -> Inner<()> {
    if entry.state != control::FileState::Live {
        return Err(Cause::IdentityMismatch);
    }
    match policy {
        PolicyRef::Allocator => {
            if entry
                .metadata_identity(
                    storage.profile().spec().namespace,
                    capture.stamp().identity.page.page_no,
                )
                .map_err(Cause::Control)?
                != capture.stamp().identity
            {
                return Err(Cause::IdentityMismatch);
            }
        }
        PolicyRef::Status(authority) => {
            let request = authority.request();
            let allocator = authority.allocator();
            if !authority.matching(capture)
                || !authority.accepts_kind(request.kind)
                || request.level != 0
                || !(1..64).contains(&request.identity.page.page_no)
                || allocator.page.page_no != 0
                || request.identity.database != allocator.database
                || request.identity.page.file != allocator.page.file
                || entry.file != request.identity.page.file
                || entry
                    .metadata_identity(storage.profile().spec().namespace, 0)
                    .map_err(Cause::Control)?
                    != *allocator
            {
                return Err(Cause::IdentityMismatch);
            }
        }
        PolicyRef::Retention(authority) => {
            let request = authority.request();
            let allocator = authority.allocator();
            if !authority.matching(capture)
                || request.level != 0
                || request.identity.page.page_no >= 64
                || allocator.page.page_no != 0
                || request.identity.database != allocator.database
                || request.identity.page.file != allocator.page.file
                || entry.file != request.identity.page.file
                || entry
                    .metadata_identity(storage.profile().spec().namespace, 0)
                    .map_err(Cause::Control)?
                    != *allocator
            {
                return Err(Cause::IdentityMismatch);
            }
        }
    }
    Ok(())
}

fn read_home<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &mut [u8; WORKSPACE_BYTES],
    mapping: &FileMapEntry,
    offset: u64,
    capture: &CapturedPage<'_, '_>,
    policy: PolicyRef<'_, '_, '_>,
) -> Inner<FileIdentity> {
    // Close the File before either page/hash or Free semantics are entered.
    let identity = read_home_bytes(storage, workspace, mapping, offset)?;
    let image = &workspace[..PAGE_BYTES];
    let home = &workspace[HOME_START..HOME_START + PAGE_BYTES];
    match policy {
        PolicyRef::Allocator => {
            let page = allocation_page::decode_allocation_page(
                home,
                capture.stamp().identity,
                storage.profile().spec().namespace,
            )
            .map_err(Cause::Allocation)?;
            if page.header().lsn.0 > capture.stamp().page_lsn.0
                || (page.header().lsn == capture.stamp().page_lsn && home != image)
            {
                return Err(Cause::ContentMismatch);
            }
        }
        PolicyRef::Status(authority) => {
            if home.iter().any(|byte| *byte != 0) {
                let lsn = authority
                    .envelope_lsn(home.try_into().expect("fixed home interval"))
                    .map_err(Cause::Status)?;
                // INIT authority permits only its exact image. A live empty
                // authority admits only whole images reconstructed from that
                // selected page's original complete WAL history.
                if lsn.0 > capture.stamp().page_lsn.0 || !authority.accepts_home(home, image) {
                    return Err(Cause::ContentMismatch);
                }
            }
        }
        PolicyRef::Retention(authority) => {
            // Zero home needs explicit INIT provenance too. A low pageLSN alone
            // cannot authenticate a pre-base image after old WAL reclamation.
            if !authority.accepts_home(
                home.try_into().expect("fixed home interval"),
                image.try_into().expect("fixed capture interval"),
            ) {
                return Err(Cause::ContentMismatch);
            }
            if home.iter().any(|byte| *byte != 0) {
                let lsn = authority
                    .envelope_lsn(home.try_into().expect("fixed home interval"))
                    .map_err(Cause::Status)?;
                if lsn.0 > capture.stamp().page_lsn.0
                    || (lsn == capture.stamp().page_lsn && home != image)
                {
                    return Err(Cause::ContentMismatch);
                }
            }
        }
    }
    Ok(identity)
}

fn read_home_bytes<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &mut [u8; WORKSPACE_BYTES],
    mapping: &FileMapEntry,
    offset: u64,
) -> Inner<FileIdentity> {
    let (mut file, identity) = open_exact(
        storage,
        &control::data_file_name(mapping.file),
        mapping.mapped_bytes,
        OpenMode::ReadOnly,
    )?;
    let home = &mut workspace[HOME_START..HOME_START + PAGE_BYTES];
    read_exact_at(&mut file, home, offset).map_err(Cause::Io)?;
    Ok(identity)
}

fn control_start(index: usize) -> usize {
    CONTROLS_START + index * PAGE_BYTES
}
fn selector_start(index: usize) -> usize {
    SELECTORS_START + index * control::SELECTOR_BYTES
}

fn encode_selections(
    workspace: &mut [u8; WORKSPACE_BYTES],
    profile: Profile,
    known: SelectedControl,
    batch: u64,
    capture: &CapturedPage<'_, '_>,
    digest: [u8; 32],
) -> Inner<[SelectedControl; 3]> {
    let stamp = capture.stamp();
    let request = capture.request();
    let entry = PendingImage {
        slot: 0,
        page: stamp.identity.page,
        owner: stamp.identity.owner,
        owner_generation: stamp.identity.owner_generation,
        kind: request.kind,
        level: request.level,
        lsn: stamp.page_lsn,
        image_digest: digest,
    };
    dwb::validate_image(entry, profile.spec().namespace, &workspace[..PAGE_BYTES])
        .map_err(Cause::Doublewrite)?;
    let mut selected = known;
    let mut selections = [known; 3];
    for (index, output) in selections.iter_mut().enumerate() {
        let bytes = &mut workspace[control_start(index)..control_start(index) + PAGE_BYTES];
        bytes.fill(0);
        let header = ControlHeader {
            namespace: profile.spec().namespace,
            generation: selected
                .reference
                .generation
                .checked_add(1)
                .ok_or(Cause::Capacity)?,
            batch_id: batch,
            state: if index == 1 {
                ControlState::Pending
            } else {
                ControlState::Empty
            },
            max_lsn: if index == 1 {
                stamp.page_lsn
            } else {
                PageLsn(0)
            },
        };
        let entries = if index == 1 {
            std::slice::from_ref(&entry)
        } else {
            &[]
        };
        let encoded = dwb::encode_control(header, entries, bytes).map_err(Cause::Doublewrite)?;
        selected = SelectedControl {
            generation: selected.generation.checked_add(1).ok_or(Cause::Capacity)?,
            reference: ControlRef {
                kind: ControlKind::Doublewrite,
                slot: 1 - selected.reference.slot,
                index: 0,
                generation: header.generation,
                encoded_len: encoded.len() as u32,
                digest: content_digest(encoded),
            },
        };
        control::encode_selector(
            SelectorRole::Doublewrite,
            selected,
            profile,
            &mut workspace[selector_start(index)..selector_start(index) + control::SELECTOR_BYTES],
        )
        .map_err(Cause::Control)?;
        *output = selected;
    }
    Ok(selections)
}

fn run<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    observation: &mut IoObservation,
    phase: Phase,
    effect: impl FnOnce() -> Inner<()>,
) -> Inner<()> {
    live(storage)?;
    observation.phase = phase;
    observation.attempted = observation
        .attempted
        .checked_add(1)
        .ok_or(Cause::Invariant)?;
    effect()?;
    observation.completed = observation
        .completed
        .checked_add(1)
        .ok_or(Cause::Invariant)?;
    Ok(())
}

fn select<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    workspace: &[u8; WORKSPACE_BYTES],
    observation: &mut IoObservation,
    prepared: &Prepared,
    index: usize,
) -> Inner<()> {
    let phase = |step| Phase::Selection {
        index: index as u8,
        step,
    };
    let selected = prepared.selections[index];
    let bytes = &workspace[control_start(index)..control_start(index) + PAGE_BYTES];
    {
        let (mut file, _) = open_exact(
            storage,
            &name("d3-dwb-stage"),
            PAGE_BYTES as u64,
            OpenMode::ReadWrite,
        )?;
        run(
            storage,
            observation,
            phase(SelectionStep::StageWrite),
            || write_all_at(&mut file, bytes, 0).map_err(Cause::Io),
        )?;
        run(
            storage,
            observation,
            phase(SelectionStep::StageSync),
            || sync(&mut file).map_err(Cause::Io),
        )?;
    }
    {
        let (mut file, _) = open_exact(
            storage,
            &control::control_file_name(selected.reference).map_err(Cause::Control)?,
            PAGE_BYTES as u64,
            OpenMode::ReadWrite,
        )?;
        run(
            storage,
            observation,
            phase(SelectionStep::BankWrite),
            || write_all_at(&mut file, bytes, 0).map_err(Cause::Io),
        )?;
        run(storage, observation, phase(SelectionStep::BankSync), || {
            sync(&mut file).map_err(Cause::Io)
        })?;
    }
    {
        // Keep File outside generic closure/Result forwarding: at most four
        // source/return carriers, with one actual physical handle.
        live(storage)?;
        observation.phase = phase(SelectionStep::SelectorCreate);
        observation.attempted = observation
            .attempted
            .checked_add(1)
            .ok_or(Cause::Invariant)?;
        let mut file = port(storage)?
            .create_new(&name("d3-dwb-select-stage"))
            .map_err(Cause::Namespace)?;
        observation.completed = observation
            .completed
            .checked_add(1)
            .ok_or(Cause::Invariant)?;
        run(
            storage,
            observation,
            phase(SelectionStep::SelectorWrite),
            || {
                write_all_at(
                    &mut file,
                    &workspace
                        [selector_start(index)..selector_start(index) + control::SELECTOR_BYTES],
                    0,
                )
                .map_err(Cause::Io)
            },
        )?;
        run(
            storage,
            observation,
            phase(SelectionStep::SelectorSync),
            || sync(&mut file).map_err(Cause::Io),
        )?;
    }
    // Conservatively retain both possible selections before the first replace.
    observation.possible = [Some(observation.known), Some(selected)];
    run(storage, observation, phase(SelectionStep::Publish), || {
        port(storage)?
            .publish_selector(&name("d3-dwb-select-stage"), Selector::Doublewrite)
            .map_err(Cause::Namespace)
    })?;
    run(
        storage,
        observation,
        phase(SelectionStep::DirectorySync),
        || port(storage)?.sync_directory().map_err(Cause::Namespace),
    )?;
    // All I/O-port guards have ended before publishing DB control state.
    observation.known = selected;
    observation.possible = [Some(selected), None];
    if index == 0 {
        observation.last_issued = prepared.batch;
    }
    if index == 2 {
        observation.retirement_selected = true;
    }
    live(storage)
}

pub(super) fn fits<W: WriterNamespace>() -> bool {
    let file = size_of::<W::File>();
    let owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    size_of::<FlushIo<'_, '_>>() <= 2048
        && size_of::<Prepared>() <= 1024
        && size_of::<RetiredBatch<'_>>() <= 256
        && size_of::<FlushIoError>() <= 128
        && file <= 8192
        && temporary_types_fit()
        && status_types_fit()
        && encode_outer_bytes() <= 3072
        && execute_outer_bytes() <= 4096
        && io_phase_layout()
            .iter()
            .all(|(_, actual, cap)| actual <= cap)
        && owner
            .checked_add(size_of::<FlushIo<'_, '_>>())
            .and_then(|n| file.checked_mul(4).and_then(|files| n.checked_add(files)))
            .and_then(|n| n.checked_add(8192))
            .is_some_and(|n| n <= 65536)
}

// These caps make the source-value ledger's 8 KiB temporary reservation a
// checked target-layout premise. Neither this nor File-carrier counts claim a
// compiler stack size. The original page/slot/control scratch is in workspace.
fn temporary_types_fit() -> bool {
    temporary_layout()
        .iter()
        .all(|(_, actual, cap)| actual <= cap)
}
fn temporary_layout() -> [(&'static str, usize, usize); 28] {
    use crate::storage::{
        dirty_frame::TargetRequest,
        page::{PageHeader, PageView},
    };
    use foundation::{codec::ContentHasher, namespace::FileMetadata};
    [
        ("Prepared", size_of::<Prepared>(), 768),
        ("PreparedResult", size_of::<Inner<Prepared>>(), 768),
        ("IoObservation", size_of::<IoObservation>(), 256),
        ("Selected3", size_of::<[SelectedControl; 3]>(), 192),
        (
            "Selected3Result",
            size_of::<Inner<[SelectedControl; 3]>>(),
            256,
        ),
        ("FrameStamp", size_of::<FrameStamp>(), 112),
        ("TargetRequest", size_of::<TargetRequest>(), 80),
        ("WalFrameLocation", size_of::<WalFrameLocation>(), 96),
        ("Profile", size_of::<Profile>(), 96),
        ("FileMapRoot", size_of::<control::FileMapRoot<'_>>(), 192),
        (
            "FileMapRootResult",
            size_of::<std::result::Result<control::FileMapRoot<'_>, ControlError>>(),
            192,
        ),
        (
            "FileMapRootInner",
            size_of::<Inner<control::FileMapRoot<'_>>>(),
            192,
        ),
        (
            "FileMapCursor",
            size_of::<control::FileMapCursor<'_>>(),
            224,
        ),
        ("FileMapChunk", size_of::<control::FileMapChunk<'_>>(), 112),
        (
            "FileMapChunkResult",
            size_of::<std::result::Result<control::FileMapChunk<'_>, ControlError>>(),
            112,
        ),
        (
            "FileMapChunkInner",
            size_of::<Inner<control::FileMapChunk<'_>>>(),
            112,
        ),
        ("FileMapEntry", size_of::<FileMapEntry>(), 96),
        ("ChunkLocation", size_of::<control::ChunkLocation>(), 80),
        ("PageHeader", size_of::<PageHeader>(), 96),
        ("PageView", size_of::<PageView<'_>>(), 128),
        (
            "AllocationPage",
            size_of::<allocation_page::AllocationPage<'_>>(),
            192,
        ),
        ("ControlView", size_of::<dwb::ControlView<'_>>(), 96),
        ("PendingImage", size_of::<PendingImage>(), 96),
        ("ContentHasher", size_of::<ContentHasher>(), 128),
        ("FileName", size_of::<FileName>(), 160),
        ("FileMetadata", size_of::<FileMetadata>(), 32),
        ("Cause", size_of::<Cause>(), 96),
        ("UnitResult", size_of::<Inner<()>>(), 96),
    ]
}

// The Status codec runs as checksum/envelope then canonical Free, never with
// a FILEMAP cursor, retained page view, generic File or Prepared initializer.
// These are initialized source-value envelopes, not ABI stack measurements.
fn status_codec_bytes() -> usize {
    use crate::storage::{
        page::{PageHeader, PageIdentity, PageView},
        status_home::StatusHome,
        transaction_record::TransactionStatusCell,
    };
    use foundation::codec::ContentHasher;
    let references = 16 * size_of::<&()>()
        + 16 * size_of::<&[u8]>()
        + size_of::<[usize; 8]>()
        + 2 * size_of::<Option<std::ops::Range<usize>>>();
    let scalar = size_of::<PageLsn>().max(size_of::<std::result::Result<PageLsn, SemanticError>>());
    let view = size_of::<PageView<'_>>()
        .max(size_of::<
            std::result::Result<PageView<'_>, crate::storage::page::PageError>,
        >())
        .max(size_of::<std::result::Result<PageView<'_>, SemanticError>>());
    let envelope = 4 * size_of::<ContentHasher>()
        + 1536
        + 3 * size_of::<PageIdentity>()
        + 3 * view
        + 2 * size_of::<PageHeader>()
        + 6 * scalar
        + 3 * 32
        + references;
    let free = 4 * size_of::<StatusHome>().max(size_of::<
        std::result::Result<StatusHome, crate::storage::page::PageError>,
    >()) + 4 * size_of::<TransactionStatusCell>().max(size_of::<
        std::result::Result<TransactionStatusCell, crate::storage::page::PageError>,
    >()) + 6 * scalar
        + 64
        + references;
    envelope.max(free)
}

fn io_scaffolding_bytes() -> usize {
    12 * size_of::<&()>()
        + 8 * size_of::<&[u8]>()
        + 4 * size_of::<Cause>().max(size_of::<Inner<()>>())
        + 96
}

fn encode_outer_bytes() -> usize {
    2 * size_of::<[SelectedControl; 3]>().max(size_of::<Inner<[SelectedControl; 3]>>())
        + 2 * size_of::<FrameStamp>()
        + size_of::<WalFrameLocation>()
        + size_of::<Profile>()
        + 64
        + 2 * size_of::<MappedHome>()
        + size_of::<Prepared>()
        + size_of::<TargetRequest>()
        + size_of::<TargetClass>()
        + 2 * size_of::<Cause>().max(size_of::<Inner<()>>())
        + 512
}

fn execute_outer_bytes() -> usize {
    size_of::<Prepared>()
        + 8 * size_of::<RetiredBatch<'_>>()
            .max(size_of::<Inner<RetiredBatch<'_>>>())
            .max(size_of::<Result<RetiredBatch<'_>>>())
        + size_of::<SelectedControl>()
        + 3 * size_of::<foundation::namespace::FileMetadata>()
        + 4 * size_of::<Cause>().max(size_of::<Inner<()>>())
        + size_of::<IoObservation>()
        + 256
        + size_of::<TargetClass>()
        + size_of::<PolicyRef<'_, '_, '_>>()
}

/// New retention policy uses the same DWB executor and backing. The borrowed
/// authority and temporary tail loans do not enlarge the retained executor.
pub(super) fn retention_source_bytes<W: WriterNamespace>() -> usize {
    let common = io_phase_layout()
        .iter()
        .map(|(_, actual, _)| *actual)
        .max()
        .unwrap_or(0);
    let policy_codec = 3 * size_of::<crate::storage::page::PageView<'_>>()
        + 3 * size_of::<
            std::result::Result<
                crate::storage::page::PageView<'_>,
                crate::storage::page::PageError,
            >,
        >()
        + 4 * size_of::<foundation::codec::ContentHasher>()
        + 3 * size_of::<std::result::Result<PageLsn, SemanticError>>()
        + 4 * 32
        + 512;
    common
        + policy_codec
        + size_of::<RetentionFlushAuthority<'_, '_>>()
        + 2 * size_of::<W::File>()
        + 2 * size_of::<Inner<(W::File, FileIdentity)>>()
        + size_of::<MutexGuard<'_, W>>()
        + size_of::<PolicyRef<'_, '_, '_>>()
        + size_of::<(&[u8; PAGE_BYTES], &mut [u8])>()
        + size_of::<Result<(&[u8; PAGE_BYTES], &mut [u8])>>()
}

fn io_phase_layout() -> [(&'static str, usize, usize); 10] {
    let scaffold = io_scaffolding_bytes();
    [
        ("StatusCodec", status_codec_bytes(), 3840),
        ("IoScaffolding", scaffold, 768),
        ("EncodeOuter", encode_outer_bytes(), 3072),
        ("ExecuteOuter", execute_outer_bytes(), 4096),
        ("StatusImagePhase", status_codec_bytes() + scaffold, 8192),
        (
            "SelectionPhase",
            // Plus the installed-grammar wrapper over the profile decode.
            scaffold
                + 3072
                + 1536
                + 1024
                + (control::installed_profile_source_bytes()
                    - control::row_profile::source_bytes()),
            8192,
        ),
        ("FileMapPhase", scaffold + 3056 + 1536 + 1024, 8192),
        (
            "StatusHomePhase",
            status_codec_bytes() + scaffold + size_of::<FileMapEntry>() + size_of::<FileIdentity>(),
            8192,
        ),
        // There are no filename/port carriers in direct workspace encoding.
        ("EncodePhase", encode_outer_bytes() + 3072 + 1536, 8192),
        (
            "ExecutePhase",
            // The execution phase has no page/FILEMAP/control decoding.
            // Its forwarding return family is charged eight times above.
            execute_outer_bytes() + 1536 + 1024,
            8192,
        ),
    ]
}

fn status_types_fit() -> bool {
    status_temporary_layout()
        .iter()
        .all(|(_, actual, cap)| actual <= cap)
}

// This table is evaluated after the old 28-row table has returned. Neither
// table is live in any codec/I/O phase; no larger combined product array exists.
fn status_temporary_layout() -> [(&'static str, usize, usize); 18] {
    use crate::storage::{
        page::{PageError, PageIdentity, PageView},
        status_home::StatusHome,
        transaction_record::TransactionStatusCell,
    };
    [
        ("TargetClass", size_of::<TargetClass>(), 8),
        ("PolicyRef", size_of::<PolicyRef<'_, '_, '_>>(), 16),
        ("MappedHome", size_of::<MappedHome>(), 160),
        ("FileMapEntryInner", size_of::<Inner<FileMapEntry>>(), 96),
        ("FileMapEntryOption", size_of::<Option<FileMapEntry>>(), 96),
        ("ControlHeader", size_of::<ControlHeader>(), 96),
        ("RetiredInner", size_of::<Inner<RetiredBatch<'_>>>(), 256),
        ("RetiredResult", size_of::<Result<RetiredBatch<'_>>>(), 256),
        ("PageIdentity", size_of::<PageIdentity>(), 80),
        (
            "PageViewResult",
            size_of::<std::result::Result<PageView<'_>, PageError>>(),
            128,
        ),
        (
            "PageViewSemanticResult",
            size_of::<std::result::Result<PageView<'_>, SemanticError>>(),
            128,
        ),
        ("StatusHome", size_of::<StatusHome>(), 256),
        (
            "StatusHomeResult",
            size_of::<std::result::Result<StatusHome, PageError>>(),
            256,
        ),
        ("StatusCell", size_of::<TransactionStatusCell>(), 224),
        (
            "StatusCellResult",
            size_of::<std::result::Result<TransactionStatusCell, PageError>>(),
            224,
        ),
        (
            "PageLsnResult",
            size_of::<std::result::Result<PageLsn, SemanticError>>(),
            32,
        ),
        (
            "OptionRange",
            size_of::<Option<std::ops::Range<usize>>>(),
            32,
        ),
        (
            "StatusHash",
            4 * size_of::<foundation::codec::ContentHasher>() + 1536,
            2048,
        ),
    ]
}

// The parent owns a claim even in the pre-prepare capture window. Its unwind
// and Bound Drop guards fence that window. Once prepared, this owner also
// fences its own abandonment. Dropping a temporary view never drops this owner.
impl Drop for FlushIo<'_, '_> {
    fn drop(&mut self) {
        if self.observation.state != State::Ready {
            self.fence.fence();
        }
    }
}

#[cfg(test)]
#[path = "flush_io_profile_tests.rs"]
mod profile_tests;

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    pub(crate) fn seed_headroom(
        io: &mut FlushIo<'_, '_>,
        batch: Option<u64>,
        control: Option<u64>,
        selector: Option<u64>,
    ) {
        assert_eq!(io.observation.state, State::Ready);
        if let Some(batch) = batch {
            io.observation.last_issued = batch;
        }
        if let Some(control) = control {
            io.observation.known.reference.generation = control;
        }
        if let Some(selector) = selector {
            io.observation.known.generation = selector;
        }
        io.observation.possible = [Some(io.observation.known), None];
    }
    pub(crate) fn layout() -> Vec<(&'static str, usize, usize)> {
        let mut values = temporary_layout().to_vec();
        values.extend([
            ("FlushIo", size_of::<FlushIo<'_, '_>>(), 2048),
            ("RetiredBatch", size_of::<RetiredBatch<'_>>(), 256),
            ("FlushIoError", size_of::<FlushIoError>(), 128),
        ]);
        values
    }
    pub(crate) fn status_layout() -> Vec<(&'static str, usize, usize)> {
        let mut values = status_temporary_layout().to_vec();
        values.extend(io_phase_layout());
        values
    }
    pub(crate) fn corrupt_selection(io: &mut FlushIo<'_, '_>, possible: bool) {
        assert_eq!(io.observation.state, State::Ready);
        if possible {
            io.observation.possible[1] = Some(io.observation.known);
        } else {
            io.observation.known.reference.encoded_len = 248;
            io.observation.possible[0] = Some(io.observation.known);
        }
    }
    /// Corrupt only retained association after valid preparation. The actual
    /// capture, authority and durability proof remain untouched in the caller.
    pub(crate) fn corrupt_target_association(io: &mut FlushIo<'_, '_>, variant: u8) {
        assert_eq!(io.observation.state, State::Prepared);
        let prepared = io.prepared.as_mut().expect("prepared association");
        match variant {
            0 => {
                prepared.class = match prepared.class {
                    TargetClass::Allocator => TargetClass::Status,
                    TargetClass::Status
                    | TargetClass::MixedStatus
                    | TargetClass::MixedUndo
                    | TargetClass::Retention => TargetClass::Allocator,
                };
            }
            1 => {
                prepared.kind = match prepared.kind {
                    PageKind::TransactionStatus => PageKind::Allocation,
                    _ => PageKind::TransactionStatus,
                };
            }
            2 => prepared.level = 1,
            _ => panic!("unknown prepared association corruption"),
        }
    }
}
