//! Row Begin and the empty full RollbackStart/AbortEnd
//! (XADB-MODULE-ROW-BEGIN-001). Each prepares one scope3 group with C's
//! producer (`semantic_row::transition::produce_borrowed`) on the original
//! borrowed resources, inside the one gate/progress shell under the fixed
//! RowProgress class, and installs it through the one existing-group install
//! body (`ExistingGroup::Row`). Its control part is written by
//! `history::row::record`, the body recovery replay uses, so the next
//! `history::validate_live` proves live and replay byte equality. These groups
//! publish no epoch and no Q. Statement marks, Row writes and Row Commit are
//! not supplied here.
use super::*;
use crate::storage::{
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        row::{self as job3, JobScope},
    },
    semantic_row::{
        self as row_semantic,
        transition::{self as row_group, ROW_REPLAY_SCRATCH_BYTES},
    },
};

/// The before loan of the Allocation proof image. The producer's own output
/// roles (Root 0, Jobs 1, Status 2, Undo 3, Leaf 4) name the other loans.
const ALLOCATOR_ROLE: u8 = 7;
/// Most targets of a supplied Row lifecycle group: Begin and AbortEnd take
/// Allocation, Root, Jobs and Status; the empty RollbackStart takes no Jobs.
const ROW_TARGETS: usize = 4;
const ROOT: u8 = row_group::ROOT as u8;
const JOB: u8 = row_group::JOB as u8;
const STATUS: u8 = row_group::STATUS as u8;
// The producer scratch is carved from the three after loans no Row role uses.
const _: () = assert!(ROW_REPLAY_SCRATCH_BYTES <= 3 * PAGE_BYTES);
const _: () = assert!(row_semantic::FRAME_BYTES + PAGE_BYTES <= semantic_catalog::FRAME_BYTES);

#[derive(Clone, Copy)]
enum RowLifecycle<'q> {
    Begin(&'q Plan4Target),
    RollbackStart(TxnRef),
    AbortEnd(TxnRef),
}

fn row_semantic_error(error: row_semantic::Error) -> RetentionError {
    unchanged(RetentionPhase::Prepare, RetentionCause::RowSemantic(error))
}

/// Immutable bytes and the original control cut of one prepared Row group,
/// as `CatalogPayload`. It owns no pin, WAL effect or publication authority.
pub(in super::super) struct RowPayload<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    prefix: WalPrefix,
    observation: row_group::Observation,
    slot: Option<usize>,
    frame: &'i [u8],
    frame_digest: [u8; 32],
    after: &'i [[u8; PAGE_BYTES]; 5],
    inventory: &'i [u8; PAGE_BYTES],
    control: ControlCut,
}
impl<'c, 's> GroupPayload<'c, 's> for RowPayload<'_, 'c, 's> {
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    fn prefix(&self) -> WalPrefix {
        self.prefix
    }
    fn lsn(&self) -> u64 {
        self.observation.header.lsn
    }
    fn frame(&self) -> &[u8] {
        self.frame
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.frame_digest
    }
    fn cut(&self) -> ControlCut {
        self.control
    }
}

/// One prepared Row group with its original targets and stamps. No
/// Clone/Copy and no raw constructor: only `prepare_existing_row` builds it
/// and the install body borrows it once.
pub(in super::super) struct PreparedRowGroup<'i, 'c, 's> {
    payload: RowPayload<'i, 'c, 's>,
    owner: OwnerIdentity,
    requests: [TargetRequest; ROW_TARGETS],
    roles: [u8; ROW_TARGETS],
    count: usize,
    stamps: [Option<FrameStamp>; ROW_TARGETS],
}
impl<'i, 'c, 's> PreparedRowGroup<'i, 'c, 's> {
    /// The supplied Row lifecycle operations; any other Row group has no
    /// caller here and is refused by the install body.
    pub(super) fn lifecycle(&self) -> bool {
        matches!(
            self.payload.observation.operation,
            row_semantic::Operation::Begin
                | row_semantic::Operation::RollbackStart
                | row_semantic::Operation::AbortEnd
        )
    }
    pub(super) fn check(
        &self,
        owner: &Owner<'c, 's>,
        pool: &SharedPool<'c, 's, '_, '_>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()> {
        check_group(&self.payload, self.owner, owner, pool, control, prefix)
    }
    pub(super) fn targets(&self) -> &[TargetRequest] {
        &self.requests[..self.count]
    }
    pub(super) fn stamp(&self, at: usize) -> Option<FrameStamp> {
        self.stamps
            .get(at)
            .copied()
            .flatten()
            .filter(|_| at < self.count)
    }
    pub(super) fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        let role = *self.roles.get(at)?;
        let observation = &self.payload.observation;
        (at < self.count && observation.order[..usize::from(observation.count)].contains(&role))
            .then(|| &self.payload.after[usize::from(role)])
    }
    pub(super) fn stage_parts(
        &self,
    ) -> (
        WalPrefix,
        crate::storage::wal_frame::WalFrameHeader,
        &'i [u8],
    ) {
        (
            self.payload.prefix,
            self.payload.observation.header,
            self.payload.frame,
        )
    }
    pub(super) fn mark_staged(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        mark_staged_group(&self.payload, control, requirement)
    }
    /// Applies only the control part of the installed group: the ATT/Aging
    /// rows and the changed Jobs page through `history::row::record` (which
    /// checks everything before its first write), Begin's adopted local and
    /// the XOI successor encoded before the stage.
    pub(super) fn apply_control(
        &self,
        control: &mut control::RetentionLedger<'_, '_, '_>,
        profile: crate::storage::control::Profile,
        requirement: WalRequirement<'_>,
    ) -> Result<()> {
        let payload = &self.payload;
        check_staged_group(payload, control, requirement)?;
        let layout = row_semantic::decode_layout(
            payload.frame,
            FrameExpectation {
                stream: payload.prefix.stream,
                previous_lsn: payload.prefix.complete_lsn,
            },
            profile,
        )
        .map_err(|e| unchanged(RetentionPhase::Install, RetentionCause::RowSemantic(e)))?;
        let (bank, rest) = control.bytes.split_at_mut(BANK_END);
        let jobs: &mut [[u8; JOB_BYTES]; JOB_COUNT] = rest[..JOB_END - BANK_END]
            .as_chunks_mut::<JOB_BYTES>()
            .0
            .try_into()
            .expect("64 job loan");
        history::row::record(
            bank,
            jobs,
            layout,
            &payload.observation,
            payload
                .observation
                .job_ordinal
                .map(|_| &payload.after[row_group::JOB]),
            payload.slot,
            profile,
        )
        .map_err(|cause| unchanged(RetentionPhase::Install, cause))?;
        // Every fallible check is above; record has checked the Begin actor.
        if payload.observation.operation == row_semantic::Operation::Begin {
            control.adopted_local = payload
                .observation
                .actor
                .expect("recorded Begin actor")
                .id()
                .local;
        }
        control.bytes[JOB_END..INVENTORY_END].copy_from_slice(payload.inventory);
        Ok(())
    }
}

impl<'i> GroupRows<'i> {
    /// A Row group: Tx handoffs and status patches own the ATT row (cleared
    /// by Commit/AbortEnd), every other operation the Aging row.
    fn row(
        observation: &row_group::Observation,
        slot: Option<usize>,
        view: row_semantic::LayoutView<'i>,
    ) -> Self {
        let operation = observation.operation;
        Self {
            transaction: (operation.handoff() || operation.status_patch())
                .then_some(slot)
                .flatten()
                .map(|slot| {
                    (
                        slot,
                        (!matches!(
                            operation,
                            row_semantic::Operation::Commit | row_semantic::Operation::AbortEnd
                        ))
                        .then(|| view.descriptor(0))
                        .flatten(),
                    )
                }),
            aging: (!operation.status_patch())
                .then(|| view.descriptor(usize::from(operation.handoff()))),
            job_ordinal: observation.job_ordinal,
        }
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// The actor's unique Row-scope TerminalLeased Job and its Status. A
    /// Catalog actor has none.
    fn row_terminal(&self, actor: TxnRef) -> Result<(usize, StatusSlotRef)> {
        let mut found = None;
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let job = job3::decode_job(bytes, self.storage.profile().spec().namespace)
                .map_err(queue_error)?;
            if job.scope() == Some(JobScope::Row)
                && job.source() == Some(actor)
                && job.state() == job3::JobState::TerminalLeased
            {
                let status = job
                    .status()
                    .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
                if found.replace((ordinal, status)).is_some() {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::Invariant,
                    ));
                }
            }
        }
        found.ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction))
    }

    /// Full transaction RollbackStart of an empty Row actor: no root of the
    /// actor exists and its Status has no undo head (the producer requires
    /// no Undo proof page then), so the group changes only the Status.
    fn prepare_row_rollback_start<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's>> {
        let (terminal, status) = self.row_terminal(actor)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        for bytes in self.control.jobs() {
            let job = job3::decode_job(bytes, self.storage.profile().spec().namespace)
                .map_err(queue_error)?;
            if job.source() == Some(actor) && job.state().is_root() {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
        }
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        self.prepare_existing_row(
            owner,
            row_group::Change::RollbackStart {
                terminal,
                statement: None,
            },
            Some(slot),
            [
                (allocator, ALLOCATOR_ROLE),
                (root, ROOT),
                (self.catalog_status_request(status), STATUS),
            ],
            private,
        )
    }

    fn prepare_row_abort_end<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's>> {
        let (terminal, status) = self.row_terminal(actor)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let job = target_from_source(
            self.storage,
            Plan4Page::Jobs {
                index: (terminal / 15) as u8,
            },
        )?
        .request;
        self.prepare_existing_row(
            owner,
            row_group::Change::AbortEnd { terminal },
            Some(slot),
            [
                (allocator, ALLOCATOR_ROLE),
                (root, ROOT),
                (job, JOB),
                (self.catalog_status_request(status), STATUS),
            ],
            private,
        )
    }

    /// The Row counterpart of `prepare_existing_catalog`: the same entry
    /// checks, canonical replay validation, original target read/arena
    /// equality/allocator membership, XOI successor and progress room
    /// (shared helpers), around C's scope3 producer instead of the Catalog
    /// one. Every refusal is Unchanged before any stage.
    fn prepare_existing_row<'i, const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        change: row_group::Change<'_>,
        slot: Option<usize>,
        mut selected: [(TargetRequest, u8); N],
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's>> {
        const {
            assert!(N >= 3 && N <= ROW_TARGETS);
        }
        let storage = self.storage;
        let profile = storage.profile();
        let _unwind = storage.session_fence().unwind_guard();
        let (owner_identity, prefix) = self.existing_entry(owner)?;
        selected.sort_unstable_by_key(|(target, _)| target_rank(*target));
        let expected = FrameExpectation {
            stream: prefix.stream,
            previous_lsn: prefix.complete_lsn,
        };
        let (frame_space, pages) = private.split_at_mut(semantic_catalog::FRAME_BYTES);
        let (after_space, before_space) = pages.split_at_mut(8 * PAGE_BYTES);
        let (after_space, scratch_space) = after_space.split_at_mut(5 * PAGE_BYTES);
        let after: &mut [[u8; PAGE_BYTES]; 5] = after_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("five Row after loans");
        let scratch: &mut [u8; ROW_REPLAY_SCRATCH_BYTES] = (&mut scratch_space
            [..ROW_REPLAY_SCRATCH_BYTES])
            .try_into()
            .expect("Row producer scratch in the unused after loans");
        let before: &mut [[u8; PAGE_BYTES]; 8] = before_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("eight original before loans");
        let (observation, stamps) = {
            let mut audit = self.history.replay_scope(owner)?;
            let mut replay = audit.checkout()?;
            if let Err(error) =
                history::validate_live(storage, self.control, replay.bytes_mut(), prefix)
            {
                let rejected = merge(Err(error), replay.return_checked());
                return merge(rejected, audit.finish_checked());
            }
            let requests: [TargetRequest; N] = std::array::from_fn(|i| selected[i].0);
            let mut stamps = [None; 9];
            let produced = (|| {
                let read = self
                    .access
                    .read(self.pool, owner, &requests)
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?;
                let observed = (|| {
                    self.observe_targets(
                        &read,
                        &selected,
                        before,
                        replay.bytes(),
                        &mut stamps,
                        ALLOCATOR_ROLE,
                    )?;
                    let has = |role| selected.iter().any(|(_, actual)| *actual == role);
                    let inputs = row_group::Inputs {
                        profile,
                        frame: expected,
                        change,
                        jobs: self.control.jobs(),
                        root: &before[usize::from(ROOT)],
                        jobs_page: has(JOB).then_some(&before[usize::from(JOB)]),
                        status: has(STATUS).then_some(&before[usize::from(STATUS)]),
                        undo: None,
                        previous: None,
                        table: None,
                        transaction: slot
                            .filter(|_| change.operation() != row_semantic::Operation::Begin)
                            .map(|index| {
                                self.control.row(index)[..1536]
                                    .try_into()
                                    .expect("original transaction descriptor")
                            }),
                        aging: (self.control.row(control::AGING_SLOT)[control::PHASE] != 0).then(
                            || {
                                self.control.row(control::AGING_SLOT)[..1536]
                                    .try_into()
                                    .expect("original Aging descriptor")
                            },
                        ),
                    };
                    row_group::produce_borrowed(&inputs, after, frame_space, scratch)
                        .map_err(row_semantic_error)
                })();
                merge(
                    observed,
                    read.release_checked()
                        .map_err(|error| frame(RetentionPhase::Prepare, error)),
                )
            })();
            let produced = merge(produced, replay.return_checked());
            let observation = merge(produced, audit.finish_checked())?;
            (observation, stamps)
        };
        let current = storage
            .wal()
            .prefix()
            .map_err(|error| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(error)))?;
        if current != prefix {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let order = &observation.order[..usize::from(observation.count)];
        // Every changed page is a selected target at that identity.
        for role in order {
            let target = selected
                .iter()
                .find(|(_, actual)| actual == role)
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            let image = recovery::request_from_image(profile, &after[usize::from(*role)])
                .map_err(|cause| unchanged(RetentionPhase::Prepare, cause))?;
            if image != target.0 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        let root = if order.contains(&ROOT) {
            &after[usize::from(ROOT)]
        } else {
            &before[usize::from(ROOT)]
        };
        let (bytes, spare) = frame_space.split_at_mut(observation.frame_bytes);
        let inventory: &mut [u8; PAGE_BYTES] = spare
            .get_mut(..PAGE_BYTES)
            .and_then(|page| page.try_into().ok())
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let future = {
            let layout = row_semantic::decode_layout(bytes, expected, profile)
                .map_err(row_semantic_error)?;
            prepare_inventory(
                self.control,
                profile,
                observation.header.lsn,
                GroupRows::row(&observation, slot, layout),
                root,
                &before[usize::from(ALLOCATOR_ROLE)],
                observation.job_ordinal.map(|_| &after[usize::from(JOB)]),
                inventory,
            )?
        };
        let mut requests = [selected[0].0; ROW_TARGETS];
        let mut roles = [ALLOCATOR_ROLE; ROW_TARGETS];
        for (at, (request, role)) in selected.into_iter().enumerate() {
            requests[at] = request;
            roles[at] = role;
        }
        check_progress_room(
            prefix,
            (observation.header.lsn, observation.frame_bytes),
            future,
            (&roles[..N], &stamps[..N]),
            order,
            root,
            profile,
        )?;
        Ok(PreparedRowGroup {
            payload: RowPayload {
                core: self.pool.bound_core(),
                binding: &storage.binding,
                fence: storage.session_fence(),
                prefix,
                observation,
                slot,
                frame: bytes,
                frame_digest: content_digest(bytes),
                after,
                inventory,
                control: ControlCut::capture(self.control),
            },
            owner: owner_identity,
            requests,
            roles,
            count: N,
            stamps: std::array::from_fn(|at| stamps[at]),
        })
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Row Begin on the original exclusive INIT Status: Begin's shared
    /// slot/Job selection and Status identity (issued-local gap kept on any
    /// later rejection), then the Row Terminal recipe (registry 16).
    pub(crate) fn row_begin(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<row_group::Observation> {
        self.row_lifecycle(owner, RowLifecycle::Begin(status), private, flusher)
    }

    /// Full RollbackStart of an empty Row actor.
    pub(crate) fn row_rollback_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<row_group::Observation> {
        self.row_lifecycle(owner, RowLifecycle::RollbackStart(actor), private, flusher)
    }

    /// AbortEnd of an Aborting empty Row actor (TerminalReady, outcome 2).
    pub(crate) fn row_abort_end(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<row_group::Observation> {
        self.row_lifecycle(owner, RowLifecycle::AbortEnd(actor), private, flusher)
    }

    fn row_lifecycle(
        &mut self,
        owner: &mut Owner<'c, 's>,
        action: RowLifecycle<'_>,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<row_group::Observation> {
        self.progress_lifecycle(
            owner,
            source::OperationClass::RowProgress,
            flusher,
            |writer, owner| {
                let prepared = match action {
                    RowLifecycle::Begin(status) => writer.prepare_row_begin(owner, status, private),
                    RowLifecycle::RollbackStart(actor) => writer
                        .catalog_preparation()
                        .prepare_row_rollback_start(owner, actor, private),
                    RowLifecycle::AbortEnd(actor) => writer
                        .catalog_preparation()
                        .prepare_row_abort_end(owner, actor, private),
                }?;
                #[cfg(test)]
                crate::transaction::concurrency::catalog_test_support::hit(
                    writer.pool.bound_core(),
                    crate::transaction::concurrency::catalog_test_support::Point::LifecyclePrepared,
                );
                writer
                    .catalog_application()
                    .apply_existing(owner, ExistingGroup::Row(&prepared))?;
                Ok(prepared.payload.observation)
            },
        )
    }

    fn prepare_row_begin<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's>> {
        let (slot, ordinal) = self.begin_selection(owner)?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let job = target_from_source(
            self.storage,
            Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            },
        )?
        .request;
        let (actor, location) = self.begin_identity(owner, status)?;
        self.catalog_preparation().prepare_existing_row(
            owner,
            row_group::Change::Begin {
                actor,
                status: location,
                scope: JobScope::Row,
            },
            Some(slot),
            [
                (allocator, ALLOCATOR_ROLE),
                (root, ROOT),
                (job, JOB),
                (status.request, STATUS),
            ],
            private,
        )
    }
}

/// Source terms of the Row lifecycle, each the Row counterpart of the
/// Catalog term of the same name (`catalog_live_source_bytes`); the caller
/// adds the shared scalar/replay/ledger/physical values.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct RowLifecycleTerms {
    /// Selected/request/stamp arrays and Change around the shared preparer.
    pub(in super::super) outer: usize,
    /// Typed read, Inputs, Allocation view and borrowed producer.
    pub(in super::super) produce: usize,
    /// Assembled arrays, layout decode, projection and XOI encoder input.
    pub(in super::super) inventory: usize,
    /// PreparedRowGroup and its Result carriers, cut capture and digests.
    pub(in super::super) assembled: usize,
    /// The control part: staged-group prelude, layout decode and `record`.
    pub(in super::super) control: usize,
    /// Prepared group and Observation carriers through the shell.
    pub(in super::super) carriers: usize,
    /// Entry packets that survive into common preparation/installation.
    pub(in super::super) caller: usize,
    /// `produce` with the borrowed observer of a producer-supplied frame
    /// (`observe_borrowed`) in place of C's producer. No lifecycle group
    /// observes a foreign frame, so `row_lifecycle` does not use it.
    pub(in super::super) observer: usize,
}

pub(in super::super) fn row_lifecycle_terms() -> RowLifecycleTerms {
    row_group_terms::<ROW_TARGETS>()
}

/// The same terms for a Row group over `N` targets. Every target-indexed
/// array, the typed read and the prepared group's own request/role/stamp
/// arrays are taken at N; at `ROW_TARGETS` this is exactly the lifecycle's.
/// The RowWrite census (S2-0 Row write amendment §4) takes N = 9, the whole
/// write target set; no group of that size is prepared yet, so its
/// prepared-group arrays are charged as one (request, role, stamp) element
/// per target beyond `ROW_TARGETS`.
pub(in super::super) fn row_group_terms<const N: usize>() -> RowLifecycleTerms {
    use crate::storage::page::{PageHeader, PageView};
    use foundation::codec::ContentHasher;
    use std::mem::size_of;
    type Prepared<'a> = PreparedRowGroup<'a, 'a, 'a>;
    const {
        assert!(N >= ROW_TARGETS && N <= 9);
    }
    // Zero at ROW_TARGETS. Each tuple carries its own padding, so this is no
    // less than the growth of the group and of its Result carriers.
    let wider = (N - ROW_TARGETS) * size_of::<(TargetRequest, u8, Option<FrameStamp>)>();
    let observation =
        size_of::<row_group::Observation>() + 3 * size_of::<Result<row_group::Observation>>();
    let outer = size_of::<[(TargetRequest, u8); N]>()
        + size_of::<row_group::Change<'_>>()
        + size_of::<Option<usize>>()
        + size_of::<FrameExpectation>()
        + size_of::<[TargetRequest; N]>()
        + size_of::<[Option<FrameStamp>; 9]>()
        + observation;
    // Sequential under the typed read: the shared target observation, then
    // the Row Inputs and the borrowed producer (its scratch is a loan).
    let observe = 8 * size_of::<&()>()
        + size_of::<u8>()
        + 2 * size_of::<Result<()>>()
        + size_of::<crate::storage::allocation_page::AllocationPage<'_>>()
        + size_of::<PageIdentity>()
        + 2 * size_of::<Result<FrameStamp>>();
    let produce = targets::call_bytes::<N>().max(
        size_of::<targets::CatalogRead<'_, '_, '_, '_, '_, '_, '_, N>>()
            + observe.max(
                size_of::<row_group::Inputs<'_>>()
                    + size_of::<&mut [u8; ROW_REPLAY_SCRATCH_BYTES]>()
                    + row_group::produce_borrowed_source_bytes(),
            ),
    );
    let observer = targets::call_bytes::<N>().max(
        size_of::<targets::CatalogRead<'_, '_, '_, '_, '_, '_, '_, N>>()
            + observe.max(
                size_of::<row_group::Inputs<'_>>()
                    + size_of::<&mut [u8; ROW_REPLAY_SCRATCH_BYTES]>()
                    + row_group::observe_borrowed_source_bytes(),
            ),
    );
    // Changed-target identity, layout decode and the shared XOI projection,
    // then the shared progress-room check (one Root decode).
    let identity = 2 * size_of::<TargetRequest>()
        + 2 * size_of::<std::result::Result<TargetRequest, RetentionCause>>()
        + 4 * size_of::<PageHeader>()
        + 2 * size_of::<PageView<'_>>()
        + 4 * size_of::<ContentHasher>()
        + 1536;
    let inventory = size_of::<row_semantic::LayoutView<'_>>()
        + 2 * size_of::<std::result::Result<row_semantic::LayoutView<'_>, row_semantic::Error>>()
        + size_of::<GroupRows<'_>>()
        + size_of::<ProjectedInventory<'_, '_, '_, '_, '_>>()
        + 2 * size_of::<Result<(u64, u64)>>()
        + identity
            .max(row_group::layout_source_bytes())
            .max(
                crate::storage::plan4_recovery_base::catalog::inventory::row_current_source_bytes(),
            )
            .max(queue::root_decode_source_bytes() + 8 * size_of::<&()>());
    let assembled = size_of::<[TargetRequest; N]>()
        + size_of::<[u8; N]>()
        + size_of::<Prepared<'_>>()
        + 2 * size_of::<Result<Prepared<'_>>>()
        + 3 * wider
        + 2 * size_of::<ControlCut>()
        + 3 * size_of::<ContentHasher>()
        + 1536;
    // record: its arguments, actor/status encodings and the after Jobs page
    // decode (Row codec), after the staged prelude and layout decode.
    let record = size_of::<(
        &mut [u8],
        &mut [[u8; JOB_BYTES]; JOB_COUNT],
        row_semantic::LayoutView<'_>,
        &row_group::Observation,
        Option<&[u8; PAGE_BYTES]>,
        Option<usize>,
    )>() + size_of::<([u8; 48], [u8; 40])>()
        + size_of::<Option<(usize, PageView<'_>)>>()
        + 2 * size_of::<std::result::Result<(), RetentionCause>>()
        + job3::source_bytes().max(4 * size_of::<ContentHasher>() + 1536);
    let control = 2 * size_of::<ControlCut>()
        + 3 * size_of::<ContentHasher>()
        + size_of::<row_semantic::LayoutView<'_>>()
        + 2 * size_of::<std::result::Result<row_semantic::LayoutView<'_>, row_semantic::Error>>()
        + 2 * size_of::<WalRequirement<'_>>()
        + 4 * size_of::<Result<()>>()
        + 6 * size_of::<&()>()
        + row_group::layout_source_bytes().max(record);
    let carriers = 3 * size_of::<Result<Prepared<'_>>>()
        + 3 * wider
        + 3 * size_of::<Result<row_group::Observation>>()
        + 2 * size_of::<RowLifecycle<'_>>()
        + size_of::<ExistingGroup<'_, '_, '_, '_>>();
    // Begin keeps slot/ordinal, three requests and the issued identity; the
    // rollback/abort entries keep the Terminal, its Status and the requests.
    let begin = size_of::<Result<(usize, usize)>>()
        + 3 * size_of::<TargetRequest>()
        + size_of::<Result<(TxnRef, StatusSlotRef)>>()
        + size_of::<(TxnRef, StatusSlotRef)>();
    let terminal = size_of::<Result<(usize, StatusSlotRef)>>()
        + 3 * size_of::<TargetRequest>()
        + 2 * size_of::<job3::JobView<'_>>()
        + size_of::<Result<job3::JobView<'_>>>()
        + 2 * size_of::<usize>();
    let caller = size_of::<TxnRef>()
        + 4 * size_of::<&[u8]>()
        + 4 * size_of::<usize>()
        + 2 * size_of::<RetentionError>()
        + begin.max(terminal);
    RowLifecycleTerms {
        outer,
        produce,
        inventory,
        assembled,
        control,
        carriers,
        caller,
        observer,
    }
}

#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Live-versus-replay fixture: run the original `history::validate_live`
    /// on the current control and return, beside its result, the replayed
    /// ATT rows and Job cells and the live ones. It reads only; the replay
    /// loan and audit are returned checked. No authority is produced.
    pub(in crate::storage) fn row_test_live_replay(
        &self,
        owner: &mut Owner<'c, 's>,
    ) -> (Result<()>, [Vec<u8>; 4]) {
        let prefix = self.storage.wal().prefix().unwrap();
        let mut audit = self.history.replay_scope(owner).unwrap();
        let mut replay = audit.checkout().unwrap();
        let checked =
            history::validate_live(self.storage, self.control, replay.bytes_mut(), prefix);
        let copied = [
            recovery::retention_replay_bank(replay.bytes())
                [..control::ATT_SLOTS * control::SLOT_BYTES + 4 * control::SLOT_BYTES]
                .to_vec(),
            recovery::retention_replay_cells(replay.bytes())
                .as_flattened()
                .to_vec(),
            self.control.bytes
                [..control::ATT_SLOTS * control::SLOT_BYTES + 4 * control::SLOT_BYTES]
                .to_vec(),
            self.control.bytes[BANK_END..JOB_END].to_vec(),
        ];
        let returned = merge(checked, replay.return_checked());
        (merge(returned, audit.finish_checked()), copied)
    }

    /// Corrupt one live control byte (fault fixtures only).
    pub(in crate::storage) fn row_test_flip_control(&mut self, offset: usize) {
        self.control.bytes[offset] ^= 1;
    }

    /// The S1-c Catalog scan sites and the ledger over the current mixed
    /// bank: the Catalog actor's Terminal by `actor_catalog_job`, the Row
    /// actor's cells skipped by it, and `check_ledger`.
    pub(in crate::storage) fn row_test_mixed_scans(
        &self,
        catalog: TxnRef,
        row: TxnRef,
    ) -> (Result<usize>, Result<usize>, Result<(u64, u64)>) {
        let preparation = self.catalog_preparation();
        let prefix = self.storage.wal().prefix().unwrap();
        (
            preparation.actor_catalog_job(catalog, true),
            preparation.actor_catalog_job(row, true),
            self.control.check_ledger(self.storage.profile(), prefix),
        )
    }
}
