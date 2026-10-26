//! Actual scope2 preparation on the original bound. The only constructors of
//! prepared groups are the checked source/target paths in this module.
use super::super::*;
use super::{TxPairAuthority, TxPairClaimKey, TxPairControlCommit, TxPairInstallPermit};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry},
        page::{PageKind, PageRef},
        retention_page::catalog::{self as queue, JobState},
        semantic_catalog::{self, Change, Inputs, Observation, Operation},
        session_fence::SessionFence,
        wal_frame::FrameExpectation,
        wal_io::{DirtyInstallPermit, WalPrefix},
    },
    transaction::concurrency::{OwnerIdentity, PageConcurrency},
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch},
};
use std::sync::atomic::AtomicU8;

#[path = "catalog_retire.rs"]
pub(in super::super) mod retirement;

const PRIVATE_BYTES: usize = semantic_catalog::PRIVATE_BYTES;
const BANK_END: usize = 65_536;
const JOB_END: usize = 131_072;
const INVENTORY_END: usize = 147_456;
const PROOF_ROLE: u8 = 8;

// The original pool's physical lock order, without exporting its private helper.
fn target_rank(request: TargetRequest) -> (u64, u64, u64) {
    let mut bytes = [0; 24];
    request
        .identity
        .page
        .encode(&mut bytes)
        .expect("fixed PageRef output");
    (
        u64::from_le_bytes(bytes[..8].try_into().expect("fixed file id")),
        request.identity.page.page_no,
        request.identity.page.incarnation.get(),
    )
}

struct CreatePlan {
    plan: semantic_catalog::queue::ReservePlan,
    ordinal: usize,
    object: ObjectHandle,
    existing: [TargetRequest; 7],
    pair: [TargetRequest; 2],
}
impl CreatePlan {
    fn selected(&self) -> [(TargetRequest, u8); 7] {
        self.existing.map(|target| {
            (
                target,
                match (target.identity.owner.get(), target.kind) {
                    (_, PageKind::Allocation) => 0,
                    (_, PageKind::RetentionRoot) => 1,
                    (_, PageKind::RetentionJobs) => 2,
                    (5, PageKind::CatalogLeaf) => 3,
                    (_, PageKind::TransactionStatus) => 4,
                    (_, PageKind::Undo) => 5,
                    (5, PageKind::TreeAnchor) => PROOF_ROLE,
                    _ => unreachable!("actual CREATE selected roles"),
                },
            )
        })
    }
}

/// Owns both the durable Tx reservation and its original unpublished pool
/// claim. The caller cannot run another writer through this borrowed bound.
pub(crate) struct ReservedCatalogCreate<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    bound: &'r mut Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'r mut Owner<'c, 's>,
    private: &'r mut [u8; PRIVATE_BYTES],
    definition: &'r crate::api::TableSpec,
    plan: CreatePlan,
    key: TxPairClaimKey<'c, 's>,
    reserve: PageLsn,
    active: bool,
}
impl<W: WriterNamespace> Drop for ReservedCatalogCreate<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        if self.active {
            self.bound.inner.storage.session_fence().fence();
            self.bound.inner.storage.wal().completion.notify_all();
        }
    }
}

#[cfg(test)]
impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    ReservedCatalogCreate<'_, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Read-only access to the actual retained bound while the reservation
    /// still owns its exclusive caller borrow. No Owner/claim escapes.
    pub(in crate::storage) fn catalog_test_bound(
        &self,
    ) -> &Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        self.bound
    }
}

impl<'s, 'c, W: WriterNamespace> ReservedCatalogCreate<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W> {
    pub(crate) fn object(&self) -> ObjectHandle {
        self.plan.object
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged cancellation rejection returns the original reservation"
    )]
    pub(crate) fn cancel(mut self) -> std::result::Result<(), RetentionReservationError<Self>> {
        match self.cancel_inner() {
            Ok(()) => {
                self.active = false;
                Ok(())
            }
            Err(error)
                if error.effect == Effect::Unchanged && self.bound.control.pending.is_none() =>
            {
                Err(RetentionReservationError::Before {
                    error,
                    reservation: self,
                })
            }
            Err(error) => Err(RetentionReservationError::Fatal(fatal(
                self.bound.inner.storage,
                error.phase,
                error.cause,
            ))),
        }
    }
    fn cancel_inner(&mut self) -> Result<()> {
        let storage = self.bound.inner.storage;
        self.bound
            .flush
            .io
            .preflight::<W>()
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                effect: e.effect,
                cause: RetentionCause::Flush(e),
            })?;
        let slot = self
            .bound
            .control
            .slot(self.plan.plan.actor)
            .ok_or_else(|| {
                unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
            })?;
        let prepared = self.bound.prepare_existing_catalog(
            self.owner,
            Change::Cancel {
                ordinal: self.plan.ordinal,
            },
            Some(slot),
            self.plan.selected(),
            self.private,
        )?;
        let observation = self.bound.apply_existing_catalog(self.owner, prepared)?;
        let source = self.bound.history.source_cut()?;
        let requirement = source
            .requirement()
            .ok_or_else(|| fatal(storage, RetentionPhase::Install, RetentionCause::Invariant))?;
        if requirement.location().lsn != observation.header.lsn
            || self
                .bound
                .control
                .durable
                .as_ref()
                .is_none_or(|durable| !durable.confirms(&requirement))
        {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        source.unchanged()?;
        self.bound
            .return_catalog_pair(
                self.owner,
                &mut self.key,
                &self.plan,
                self.reserve,
                super::TxPairCancelKind::Cancelled,
                Some(requirement),
            )
            .map_err(|error| fatal(storage, error.phase, error.cause))
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged rejection retains the original linear reservation"
    )]
    pub(crate) fn initialize(
        mut self,
    ) -> std::result::Result<ObjectHandle, RetentionReservationError<Self>> {
        match self.initialize_inner() {
            Ok(object) => {
                self.active = false;
                Ok(object)
            }
            Err(error)
                if error.effect == Effect::Unchanged && self.bound.control.pending.is_none() =>
            {
                Err(RetentionReservationError::Before {
                    error,
                    reservation: self,
                })
            }
            Err(error) => Err(RetentionReservationError::Fatal(fatal(
                self.bound.inner.storage,
                error.phase,
                error.cause,
            ))),
        }
    }

    fn initialize_inner(&mut self) -> Result<ObjectHandle> {
        use crate::storage::catalog_record::CatalogTableInput;
        let storage = self.bound.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let slot = self
            .bound
            .control
            .slot(self.plan.plan.actor)
            .ok_or_else(|| {
                unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
            })?;
        let mut prepared = self.bound.prepare_existing_catalog(
            self.owner,
            Change::Forward {
                ordinal: self.plan.ordinal,
                definition: CatalogTableInput::Spec(self.definition),
            },
            Some(slot),
            self.plan.selected(),
            self.private,
        )?;
        prepared.add_initialized_pair(self.plan.pair)?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        prepared.check(
            self.owner,
            &self.bound.inner.pool,
            &self.bound.control,
            prefix,
        )?;
        let requests: &[TargetRequest; 9] =
            prepared.targets().try_into().expect("whole Forward set");
        let authority = TxPairAuthority {
            core: self.bound.inner.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            actor: self.plan.plan.actor,
            reserve: self.reserve,
            existing: &self.plan.existing,
            pair: &self.plan.pair,
        };
        if !self.key.matches_authority(&authority) {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let view = prepared.layout(storage.profile())?;
        let job_page = queue::decode_jobs_page(
            &prepared.after[2],
            storage.profile().spec().namespace,
            (self.plan.ordinal / 15) as u8,
        )
        .map_err(queue_error)?;
        let job_after: &[u8; 1024] = job_page
            .cell(self.plan.ordinal % 15)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?
            .try_into()
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let after: [Option<&[u8; PAGE_BYTES]>; 9] = std::array::from_fn(|at| prepared.after(at));
        let transaction_before = content_digest(&self.bound.control.row(slot)[..1536]);
        let aging_before = content_digest(&self.bound.control.row(control::AGING_SLOT)[..1536]);
        let job_before = content_digest(&self.bound.control.jobs()[self.plan.ordinal]);
        let inventory_before = content_digest(&self.bound.control.bytes[JOB_END..INVENTORY_END]);
        let inventory_after = prepared
            .inventory_after()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut write = self
            .bound
            .inner
            .pool
            .claim_tx_pair_targets(self.owner, &self.key, requests)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let installed = (|| {
            for (at, after_image) in after.iter().enumerate() {
                let actual = write
                    .observe(at)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?
                    .map(|o| o.frame);
                if actual != prepared.stamp(at) {
                    return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
                }
                if after_image.is_some()
                    && actual.is_some_and(|stamp| {
                        stamp.modification_counter == u64::MAX
                            || stamp.page_lsn.0 >= prepared.observation.header.lsn
                    })
                {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::Exhausted,
                    ));
                }
            }
            write
                .enter_stage()
                .map_err(|e| frame(RetentionPhase::Stage, e))?;
            let requirement = match storage.wal().stage_verified_required(
                prepared.prefix,
                prepared.observation.header,
                prepared.frame,
            ) {
                Ok(requirement) => requirement,
                Err(error) if error.effect == Effect::Unchanged => {
                    write
                        .accept_stage_rejection(super::TxPairStageRejection { key: &self.key })
                        .map_err(|e| frame(RetentionPhase::Stage, e))?;
                    return Err(RetentionError {
                        phase: RetentionPhase::Stage,
                        effect: error.effect,
                        cause: RetentionCause::Wal(error),
                    });
                }
                Err(error) => {
                    return Err(fatal(
                        storage,
                        RetentionPhase::Stage,
                        RetentionCause::Wal(error),
                    ));
                }
            };
            catalog_mark_staged(&prepared, &mut self.bound.control, requirement)?;
            self.bound.control.row_mut(slot)[control::PHASE] = control::STAGED;
            let permit = TxPairInstallPermit {
                key: &self.key,
                requirement,
                targets: requests,
                before: &prepared.stamps,
                after: &after,
            };
            let control = TxPairControlCommit {
                authority: &authority,
                original: &mut self.bound.control,
                requirement,
                transaction_slot: slot,
                job_ordinal: self.plan.ordinal,
                transaction_before,
                aging_before,
                job_before,
                inventory_before,
                transaction_after: view.descriptor(0).expect("Forward Tx descriptor"),
                aging_after: view.descriptor(1).expect("Forward Aging descriptor"),
                job_after,
                inventory_after,
                applied: false,
            };
            let completed = write
                .install_forward(&permit, control)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            self.bound.history.installed(requirement)?;
            Ok((requirement, completed))
        })();
        let (requirement, completed) = merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
        .map_err(|e| {
            if self.bound.control.pending.is_some() {
                fatal(storage, RetentionPhase::Install, e.cause)
            } else {
                e
            }
        })?;
        let durable = storage
            .wal()
            .wait_durable(&requirement)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        if !durable.confirms(&requirement) {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        self.bound
            .inner
            .pool
            .finish_tx_pair(&mut self.key, completed)
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
        self.bound.control.durable = Some(durable);
        self.bound.control.pending = None;
        Ok(self.plan.object)
    }
}

/// No Clone/Copy and no raw constructor. This borrows immutable private
/// operation bytes, not replay/control/page guards or a second storage owner.
pub(crate) struct PreparedCatalog<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    owner: OwnerIdentity,
    prefix: WalPrefix,
    observation: Observation,
    slot: Option<usize>,
    requests: [TargetRequest; 9],
    roles: [u8; 9],
    count: usize,
    stamps: [Option<FrameStamp>; 9],
    frame: &'i [u8],
    frame_digest: [u8; 32],
    after: &'i [[u8; PAGE_BYTES]; 8],
    before: &'i [[u8; PAGE_BYTES]; 8],
    inventory: Option<&'i [u8; PAGE_BYTES]>,
    control: ControlCut,
}

#[derive(Clone, Copy)]
struct ControlCut {
    bank: [u8; 32],
    jobs: [u8; 32],
    inventory: [u8; 32],
    issued: u64,
    adopted: u64,
}
impl ControlCut {
    fn capture(control: &control::RetentionControl<'_, '_, '_>) -> Self {
        Self {
            bank: content_digest(&control.bytes[..BANK_END]),
            jobs: content_digest(&control.bytes[BANK_END..JOB_END]),
            inventory: content_digest(&control.bytes[JOB_END..INVENTORY_END]),
            issued: control.issued_local,
            adopted: control.adopted_local,
        }
    }
    fn matches(self, control: &control::RetentionControl<'_, '_, '_>) -> bool {
        self.bank == content_digest(&control.bytes[..BANK_END])
            && self.jobs == content_digest(&control.bytes[BANK_END..JOB_END])
            && self.inventory == content_digest(&control.bytes[JOB_END..INVENTORY_END])
            && self.issued == control.issued_local
            && self.adopted == control.adopted_local
    }
}

impl<'i, 'c, 's> PreparedCatalog<'i, 'c, 's> {
    pub(in crate::storage) fn operation(&self) -> Operation {
        self.observation.operation
    }
    pub(in crate::storage) fn observation(&self) -> Observation {
        self.observation
    }
    pub(in crate::storage) fn prefix(&self) -> WalPrefix {
        self.prefix
    }
    pub(in crate::storage) fn owner(&self) -> OwnerIdentity {
        self.owner
    }
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    pub(in crate::storage) fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    pub(in crate::storage) fn transaction_slot(&self) -> Option<usize> {
        self.slot
    }
    pub(in crate::storage) fn targets(&self) -> &[TargetRequest] {
        &self.requests[..self.count]
    }
    pub(in crate::storage) fn stamp(&self, at: usize) -> Option<FrameStamp> {
        self.stamps
            .get(at)
            .copied()
            .flatten()
            .filter(|_| at < self.count)
    }
    pub(in crate::storage) fn frame(&self) -> &'i [u8] {
        self.frame
    }
    pub(in crate::storage) fn frame_digest(&self) -> [u8; 32] {
        self.frame_digest
    }
    pub(in crate::storage) fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        let role = *self.roles.get(at)?;
        (at < self.count
            && self.observation.order[..usize::from(self.observation.count)].contains(&role))
        .then(|| &self.after[usize::from(role)])
    }
    pub(in crate::storage) fn before(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        let role = *self.roles.get(at)?;
        (at < self.count && role < PROOF_ROLE && self.stamps[at].is_some())
            .then(|| &self.before[usize::from(role)])
    }
    pub(in super::super) fn before_role(&self, role: u8) -> Option<&'i [u8; PAGE_BYTES]> {
        self.roles[..self.count]
            .iter()
            .position(|actual| *actual == role)
            .and_then(|at| self.before(at))
    }
    fn inventory_after(&self) -> Option<&'i [u8; PAGE_BYTES]> {
        self.inventory
    }
    fn add_initialized_pair(&mut self, pair: [TargetRequest; 2]) -> Result<()> {
        if self.operation() != Operation::Forward || self.count != 7 {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        for (side, request) in pair.into_iter().enumerate() {
            let view = crate::storage::page::decode_page(
                &self.after[6 + side],
                request.identity,
                request.kind,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            if view.header().lsn != PageLsn(self.observation.header.lsn)
                || view.header().level != request.level
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
            self.requests[7 + side] = request;
            self.roles[7 + side] = (6 + side) as u8;
            self.stamps[7 + side] = None;
        }
        let mut order: [usize; 9] = std::array::from_fn(|at| at);
        order.sort_unstable_by_key(|at| target_rank(self.requests[*at]));
        self.requests = order.map(|at| self.requests[at]);
        self.roles = order.map(|at| self.roles[at]);
        self.stamps = order.map(|at| self.stamps[at]);
        self.count = 9;
        Ok(())
    }
    pub(in super::super) fn check(
        &self,
        owner: &Owner<'c, 's>,
        pool: &SharedPool<'c, 's, '_, '_>,
        control: &control::RetentionControl<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()> {
        let actual = owner
            .visibility_identity(self.core)
            .map_err(|error| RetentionError {
                phase: RetentionPhase::Prepare,
                effect: error.effect,
                cause: RetentionCause::Concurrency(error),
            })?;
        if actual != self.owner
            || !std::ptr::eq(self.core, pool.bound_core())
            || !pool.uses_storage_origin(self.binding, self.fence)
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        if prefix != self.prefix || control.pending.is_some() || !self.control.matches(control) {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        if content_digest(self.frame) != self.frame_digest {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
    pub(in super::super) fn confirms_requirement(&self, requirement: WalRequirement<'_>) -> bool {
        let location = requirement.location();
        requirement.uses_fence(self.fence)
            && location.stream == self.prefix.stream
            && location.file == self.prefix.file
            && location.previous_lsn == self.prefix.complete_lsn
            && location.lsn == self.observation.header.lsn
            && location.start == self.prefix.complete_offset
            && location.length == self.frame.len()
            && location.start.checked_add(location.length as u64) == Some(location.end)
    }
    pub(in super::super) fn layout(
        &self,
        profile: crate::storage::control::Profile,
    ) -> Result<semantic_catalog::LayoutView<'i>> {
        semantic_catalog::decode_layout(
            self.frame,
            FrameExpectation {
                stream: self.prefix.stream,
                previous_lsn: self.prefix.complete_lsn,
            },
            profile,
        )
        .map_err(semantic)
    }
}

/// The original staged requirement is retained before any byte publication.
/// In particular Commit keeps the complete old ATT/control cut until its
/// durable continuation performs the whole installation.
pub(in super::super) fn catalog_mark_staged<'s>(
    prepared: &PreparedCatalog<'_, '_, '_>,
    control: &mut control::RetentionControl<'_, '_, 's>,
    requirement: WalRequirement<'s>,
) -> Result<()> {
    if !prepared.confirms_requirement(requirement)
        || control.pending.is_some()
        || !prepared.control.matches(control)
    {
        prepared.fence.fence();
        return Err(RetentionError {
            phase: RetentionPhase::Install,
            effect: Effect::CallerMustResolve,
            cause: RetentionCause::Invariant,
        });
    }
    control.pending = Some(requirement);
    Ok(())
}

/// Applies only the control part of an already installed actual group. The
/// caller still owns the complete original write/version target set; this
/// helper performs no pool access and cannot publish Q or release a stage.
pub(in super::super) fn catalog_apply(
    prepared: &PreparedCatalog<'_, '_, '_>,
    control: &mut control::RetentionControl<'_, '_, '_>,
    profile: crate::storage::control::Profile,
    requirement: WalRequirement<'_>,
) -> Result<()> {
    if !prepared.confirms_requirement(requirement)
        || !prepared.control.matches(control)
        || control.pending.is_none_or(|pending| {
            !pending.uses_fence(prepared.fence) || pending.location() != requirement.location()
        })
    {
        prepared.fence.fence();
        return Err(RetentionError {
            phase: RetentionPhase::Install,
            effect: Effect::CallerMustResolve,
            cause: RetentionCause::Invariant,
        });
    }
    let view = prepared.layout(profile)?;
    let operation = prepared.operation();
    let inventory = prepared
        .inventory_after()
        .ok_or_else(|| unchanged(RetentionPhase::Install, RetentionCause::Invariant))?;
    let tx = operation.handoff() || operation == Operation::RollbackStart;
    let mut actor = [0; 48];
    let mut status = [0; 40];
    if tx {
        prepared
            .observation
            .actor
            .ok_or_else(|| unchanged(RetentionPhase::Install, RetentionCause::Invariant))?
            .encode(&mut actor)
            .map_err(|e| unchanged(RetentionPhase::Install, RetentionCause::Page(e)))?;
        prepared
            .observation
            .status
            .ok_or_else(|| unchanged(RetentionPhase::Install, RetentionCause::Invariant))?
            .encode(&mut status)
            .map_err(|e| unchanged(RetentionPhase::Install, RetentionCause::Page(e)))?;
        if prepared.slot.is_none_or(|slot| slot >= control::ATT_SLOTS)
            || view.descriptor(0).is_none()
        {
            return Err(unchanged(
                RetentionPhase::Install,
                RetentionCause::Invariant,
            ));
        }
    }
    let jobs = if let Some(ordinal) = prepared.observation.job_ordinal {
        Some(
            queue::decode_jobs_page(
                &prepared.after[2],
                profile.spec().namespace,
                (ordinal / 15) as u8,
            )
            .map_err(queue_error)?,
        )
    } else {
        None
    };
    if operation != Operation::RollbackStart
        && view.descriptor(usize::from(operation.handoff())).is_none()
    {
        return Err(unchanged(
            RetentionPhase::Install,
            RetentionCause::Invariant,
        ));
    }
    // Every fallible check is above. No capability is manufactured by decoding.
    if tx {
        let row = control.row_mut(prepared.slot.expect("checked original slot"));
        if matches!(operation, Operation::Commit | Operation::AbortEnd) {
            row.fill(0);
        } else {
            row[..1536].copy_from_slice(view.descriptor(0).expect("checked descriptor"));
            row[control::ACTOR].copy_from_slice(&actor);
            row[control::LOCATION].copy_from_slice(&status);
            row[control::PHASE] = control::STABLE;
        }
    }
    if operation != Operation::RollbackStart {
        let row = control.row_mut(control::AGING_SLOT);
        row[..1536].copy_from_slice(
            view.descriptor(usize::from(operation.handoff()))
                .expect("checked Aging"),
        );
        row[control::PHASE] = control::STABLE;
    }
    if let Some(jobs) = jobs {
        let base = (prepared.observation.job_ordinal.expect("checked Jobs") / 15) * 15;
        for i in 0..jobs.len() {
            control::jobs_mut(control.bytes)[base + i]
                .copy_from_slice(jobs.cell(i).expect("checked cell"));
        }
    }
    if operation == Operation::Begin {
        control.adopted_local = prepared
            .observation
            .actor
            .expect("checked actor")
            .id()
            .local;
    }
    control.bytes[JOB_END..INVENTORY_END].copy_from_slice(inventory);
    Ok(())
}

fn semantic(error: semantic_catalog::Error) -> RetentionError {
    unchanged(
        RetentionPhase::Prepare,
        RetentionCause::CatalogSemantic(error),
    )
}
fn queue_error(error: retention_page::RetentionError) -> RetentionError {
    unchanged(RetentionPhase::Prepare, RetentionCause::Retention(error))
}
fn request(identity: PageIdentity, kind: PageKind) -> TargetRequest {
    TargetRequest {
        identity,
        kind,
        level: 0,
    }
}
fn one() -> Generation {
    Generation::new(1).expect("one")
}

/// Nested check/mark/control-apply source values only. The caller separately
/// retains PreparedCatalog, its original write continuation and borrowed loans.
/// The full producer, schema and inventory preparation have already returned.
pub(in super::super) fn catalog_control_source_bytes() -> usize {
    use foundation::codec::ContentHasher;
    use std::mem::size_of;
    2 * size_of::<ControlCut>()
        + 3 * size_of::<ContentHasher>()
        + 2 * size_of::<semantic_catalog::LayoutView<'_>>()
        + 2 * size_of::<Result<semantic_catalog::LayoutView<'_>>>()
        + 2 * size_of::<OwnerIdentity>()
        + 2 * size_of::<WalPrefix>()
        + 4 * size_of::<WalRequirement<'_>>()
        + size_of::<([u8; 48], [u8; 40])>()
        + 4 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<Option<crate::storage::page::PageView<'_>>>()
        + 4 * size_of::<TxnRef>()
        + 4 * size_of::<StatusSlotRef>()
        + 8 * size_of::<Result<()>>()
        + size_of::<([usize; 12], [bool; 8], [&[u8]; 12])>()
        // These are sequential calls; the parent values above remain live.
        + semantic_catalog::layout_source_bytes().max(queue::source_bytes())
}

/// Borrowed successor projection. The shared encoder is a codec, while this
/// projection retains the actual verified layout and original control source.
struct ProjectedInventory<'b, 'i, 'a, 'c, 's> {
    control: &'b control::RetentionControl<'a, 'c, 's>,
    observation: Observation,
    slot: Option<usize>,
    view: semantic_catalog::LayoutView<'i>,
    root: &'i [u8; PAGE_BYTES],
    allocator: &'i [u8; PAGE_BYTES],
    jobs: Option<crate::storage::page::PageView<'i>>,
}
impl crate::storage::plan4_recovery_base::catalog::inventory::InventoryRows
    for ProjectedInventory<'_, '_, '_, '_, '_>
{
    fn transaction(&self, index: usize) -> Option<&[u8; 1536]> {
        let operation = self.observation.operation;
        if (operation.handoff() || operation == Operation::RollbackStart)
            && self.slot == Some(index)
        {
            if matches!(operation, Operation::Commit | Operation::AbortEnd) {
                None
            } else {
                self.view.descriptor(0)
            }
        } else {
            let bytes = &self.control.row(index)[..1536];
            bytes
                .iter()
                .any(|v| *v != 0)
                .then(|| bytes.try_into().expect("original descriptor"))
        }
    }
    fn allocation(&self) -> Option<&[u8; 1536]> {
        let bytes = &self.control.row(control::ALLOCATION_SLOT)[..1536];
        bytes
            .iter()
            .any(|v| *v != 0)
            .then(|| bytes.try_into().expect("original descriptor"))
    }
    fn aging(&self) -> Option<&[u8; 1536]> {
        let operation = self.observation.operation;
        if operation != Operation::RollbackStart {
            self.view.descriptor(usize::from(operation.handoff()))
        } else {
            let bytes = &self.control.row(control::AGING_SLOT)[..1536];
            bytes
                .iter()
                .any(|v| *v != 0)
                .then(|| bytes.try_into().expect("original descriptor"))
        }
    }
    fn root(&self) -> &[u8; PAGE_BYTES] {
        self.root
    }
    fn allocator(&self) -> &[u8; PAGE_BYTES] {
        self.allocator
    }
    fn job(
        &self,
        ordinal: usize,
    ) -> std::result::Result<&[u8], crate::storage::plan4_recovery_base::Error> {
        if self.observation.job_ordinal == Some(ordinal) {
            self.jobs
                .as_ref()
                .and_then(|page| page.cell(ordinal % 15))
                .ok_or(crate::storage::plan4_recovery_base::Error::Corrupt)
        } else {
            self.control
                .jobs()
                .get(ordinal)
                .map(|cell| &cell[..])
                .ok_or(crate::storage::plan4_recovery_base::Error::Corrupt)
        }
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "borrowed original phase sections"
)]
fn prepare_inventory(
    control: &control::RetentionControl<'_, '_, '_>,
    profile: crate::storage::control::Profile,
    observation: Observation,
    slot: Option<usize>,
    view: semantic_catalog::LayoutView<'_>,
    root_before: &[u8; PAGE_BYTES],
    allocator_before: &[u8; PAGE_BYTES],
    after: &[[u8; PAGE_BYTES]; 8],
    output: &mut [u8; PAGE_BYTES],
) -> Result<(u64, u64)> {
    let changed = |role| observation.order[..usize::from(observation.count)].contains(&role);
    let rows = ProjectedInventory {
        control,
        observation,
        slot,
        view,
        root: if changed(1) { &after[1] } else { root_before },
        allocator: if changed(0) {
            &after[0]
        } else {
            allocator_before
        },
        jobs: observation
            .job_ordinal
            .map(|ordinal| {
                queue::decode_jobs_page(&after[2], profile.spec().namespace, (ordinal / 15) as u8)
            })
            .transpose()
            .map_err(queue_error)?,
    };
    let generation = control
        .source
        .base()
        .map_or(1, |base| base.view().header().generation);
    crate::storage::plan4_recovery_base::catalog::inventory::encode_current(
        profile,
        generation,
        observation.header.lsn,
        &rows,
        output,
    )
    .map_err(|error| unchanged(RetentionPhase::Prepare, RetentionCause::Base(error)))
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn reserve_catalog_create<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        actor: TxnRef,
        undo: &Plan4Target,
        definition: &'r crate::api::TableSpec,
        private: &'r mut [u8; PRIVATE_BYTES],
    ) -> Result<ReservedCatalogCreate<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let plan = self.plan_catalog_create(owner, actor, undo, definition, private)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let prepared = self.prepare_existing_catalog(
            owner,
            Change::Reserve(&plan.plan),
            Some(slot),
            plan.selected(),
            private,
        )?;
        if prepared.observation.job_ordinal != Some(plan.ordinal) {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        self.flush.io.preflight::<W>().map_err(|e| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: e.effect,
            cause: RetentionCause::Flush(e),
        })?;
        let candidates = self
            .inner
            .pool
            .retention_pair_candidates(&plan.pair, 1, &plan.existing)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if candidates
            .iter()
            .any(|candidate| candidate.kind == dirty_frame::StatusCandidateKind::Flush)
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let reserve = PageLsn(prepared.observation.header.lsn);
        let authority = TxPairAuthority {
            core: self.inner.pool.bound_core(),
            binding: &self.inner.storage.binding,
            fence: self.inner.storage.session_fence(),
            actor,
            reserve,
            existing: &plan.existing,
            pair: &plan.pair,
        };
        let mut key = self
            .inner
            .pool
            .prepare_tx_pair(&authority, candidates.map(|candidate| candidate.index))
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if let Err(error) = self.apply_existing_catalog(owner, prepared) {
            if error.effect == Effect::Unchanged && self.control.pending.is_none() {
                self.return_catalog_pair(
                    owner,
                    &mut key,
                    &plan,
                    reserve,
                    super::TxPairCancelKind::Unstaged,
                    None,
                )
                .map_err(|cleanup| fatal(self.inner.storage, cleanup.phase, cleanup.cause))?;
                return Err(error);
            }
            return Err(fatal(self.inner.storage, error.phase, error.cause));
        }
        Ok(ReservedCatalogCreate {
            bound: self,
            owner,
            private,
            definition,
            plan,
            key,
            reserve,
            active: true,
        })
    }

    fn return_catalog_pair(
        &mut self,
        owner: &mut Owner<'c, 's>,
        key: &mut TxPairClaimKey<'c, 's>,
        plan: &CreatePlan,
        reserve: PageLsn,
        kind: super::TxPairCancelKind,
        requirement: Option<WalRequirement<'s>>,
    ) -> Result<()> {
        let storage = self.inner.storage;
        self.flush.io.preflight::<W>().map_err(|e| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: e.effect,
            cause: RetentionCause::Flush(e),
        })?;
        if self.flush.phase != flush::RetentionFlushPhase::Ready {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &[allocator])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let stamp = read
            .observe(0)
            .map(|observation| observation.frame)
            .map_err(|e| frame(RetentionPhase::Prepare, e));
        let allocator_after = merge(
            stamp,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )?;
        let authority = TxPairAuthority {
            core: self.inner.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            actor: plan.plan.actor,
            reserve,
            existing: &plan.existing,
            pair: &plan.pair,
        };
        let selected_generation = self.flush.io.snapshot().known.generation;
        let io = super::PairIoQuiescence {
            original: &mut self.flush,
            binding: &storage.binding,
            fence: storage.session_fence(),
            selected_generation,
            pair: plan.pair,
        };
        self.inner
            .pool
            .cancel_tx_pair(
                key,
                super::TxPairCancelPermit {
                    authority: &authority,
                    io,
                    kind,
                    requirement,
                    allocator_after,
                },
            )
            .map_err(|e| frame(RetentionPhase::Install, e))
    }

    fn plan_catalog_create(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        undo: &Plan4Target,
        definition: &crate::api::TableSpec,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<CreatePlan> {
        use crate::storage::{
            catalog_record, page,
            transaction_record::{LogicalRowVersion, VersionStamp},
            tree_page::{self, TreeExpectation, edit::LeafEdit},
            undo_record,
            undo_ref::UndoRef,
        };
        self.health()?;
        if self.control.pending.is_some()
            || self.control.population.reserved()
            || self.flush.phase != flush::RetentionFlushPhase::Ready
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        if undo.request.kind != PageKind::Undo
            || undo.request.level != 0
            || undo.request.identity.owner.get() != 3
            || undo.request.identity.owner_generation != one()
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        let terminal = self.actor_catalog_job(actor, true)?;
        let terminal = queue::decode_job(
            &self.control.jobs()[terminal],
            self.inner.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?;
        let status = terminal
            .status()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let ordinal = self.first_catalog_job(owner)?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = self.target(Plan4Page::Root)?.request;
        let jobs = self
            .target(Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            })?
            .request;
        let anchor = self.target(Plan4Page::CatalogAnchor)?.request;
        let catalog = self.target(Plan4Page::CatalogLeaf)?.request;
        let mut existing = [
            allocator,
            root,
            jobs,
            anchor,
            catalog,
            self.catalog_status_request(status),
            undo.request,
        ];
        existing.sort_unstable_by_key(|request| target_rank(*request));
        let position = |request| {
            existing
                .iter()
                .position(|r| *r == request)
                .expect("existing role")
        };
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &existing)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let planned = (|| {
            let namespace = self.inner.storage.profile().spec().namespace;
            let allocation = allocation_page::decode_allocation_page(
                read.bytes(position(allocator))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                allocator.identity,
                namespace,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Allocation(e)))?;
            let (_, root) = queue::decode_root_page(
                read.bytes(position(root))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                namespace,
            )
            .map_err(queue_error)?;
            let object = ObjectHandle {
                object: ObjectId::new(root.object_high().checked_add(1).ok_or_else(|| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted)
                })?)
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?,
                generation: one(),
                schema_epoch: SchemaEpoch::new(1).expect("first schema"),
            };
            let mut pair = [allocator.identity.page; 2];
            let mut count = 0;
            for index in 9..64 {
                if let AllocationEntry::Free(generation) = allocation.entry(index).map_err(|e| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::Allocation(e))
                })? {
                    pair[count] = PageRef {
                        page_no: index as u64,
                        incarnation: generation.next().map_err(|_| {
                            unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted)
                        })?,
                        ..allocator.identity.page
                    };
                    count += 1;
                    if count == 2 {
                        break;
                    }
                }
            }
            if count != 2 {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
            }
            let undo_page = page::decode_page(
                read.bytes(position(undo.request))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                undo.request.identity,
                PageKind::Undo,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            if undo_page.len() != 1 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Membership,
                ));
            }
            let free = undo_page
                .cell(0)
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            if free.len() != 432 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
            let old_generation = Generation::new(u64::from_le_bytes(
                free[8..16].try_into().expect("checked Free cell"),
            ))
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            undo_record::decode_free_undo_record(free, old_generation)
                .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Undo(e)))?;
            let reference = UndoRef::new(
                undo.request.identity.page,
                0,
                old_generation
                    .next()
                    .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            // The first schema/leaf-capacity validation happens before Reserve.
            // Both temporary cells borrow the existing private frame lane.
            let (value, rest) = private.split_at_mut(catalog_record::MAX_CATALOG_CELL_BYTES);
            let cell = &mut rest[..semantic_catalog::MAX_CELL];
            let length = catalog_record::encode_catalog_record(
                catalog_record::CatalogExpectation {
                    namespace,
                    object: object.object,
                },
                catalog_record::CatalogHeader {
                    identity: object,
                    retired: false,
                    stamp: VersionStamp::Transaction(actor),
                    logical: LogicalRowVersion::new(actor.id(), 1)
                        .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?,
                    undo: Some(reference),
                    anchor: pair[0],
                },
                catalog_record::CatalogDefinition::Table(definition),
                value,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Catalog(e)))?;
            let projection = semantic_catalog::catalog_projection_digest(
                namespace,
                object.object,
                Some(&value[..length]),
            )
            .map_err(semantic)?;
            let leaf = tree_page::decode_tree_page(
                read.bytes(position(catalog))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                TreeExpectation {
                    identity: catalog.identity,
                    kind: PageKind::CatalogLeaf,
                    level: 0,
                    arity: 0,
                },
            )
            .map_err(|e| {
                unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::CatalogSemantic(semantic_catalog::Error::Tree(e)),
                )
            })?;
            let encoded = tree_page::encode_leaf_cell(
                PageKind::CatalogLeaf,
                0,
                &object.object.get().to_be_bytes(),
                &value[..length],
                cell,
            )
            .map_err(|e| {
                unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::CatalogSemantic(semantic_catalog::Error::Tree(e)),
                )
            })?;
            semantic_catalog::images::validate_catalog_leaf(&leaf, namespace).map_err(semantic)?;
            let mut cells = [&[][..]; semantic_catalog::images::CATALOG_EDIT_CELLS];
            if leaf
                .prepare_leaf_edit(LeafEdit::Insert(encoded), &mut cells)
                .map_err(|e| {
                    unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::CatalogSemantic(semantic_catalog::Error::LeafEdit(e)),
                    )
                })?
                .needs_split()
            {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
            }
            Ok(CreatePlan {
                plan: semantic_catalog::queue::ReservePlan {
                    actor,
                    status,
                    undo: reference,
                    pair,
                    catalog_bytes: (length + 12) as u32,
                    projection,
                },
                ordinal,
                object,
                existing,
                pair: std::array::from_fn(|at| {
                    request(
                        PageIdentity {
                            database: allocator.identity.database,
                            page: pair[at],
                            owner: object.object,
                            owner_generation: object.generation,
                        },
                        if at == 0 {
                            PageKind::TreeAnchor
                        } else {
                            PageKind::RowLeaf
                        },
                    )
                }),
            })
        })();
        merge(
            planned,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    pub(crate) fn catalog_aging_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = self.target(Plan4Page::Root)?.request;
        let prepared = self.prepare_existing_catalog(
            owner,
            Change::AgingStart,
            None,
            [(allocator, 0), (root, 1)],
            private,
        )?;
        self.apply_existing_catalog(owner, prepared)
    }

    /// Selects the canonical next free Job from the original current Root.
    /// No reservation or identity is created by this bounded observation.
    fn first_catalog_job(&self, owner: &mut Owner<'c, 's>) -> Result<usize> {
        let root = self.target(Plan4Page::Root)?.request;
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &[root])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let selected = (|| {
            let (_, root) = queue::decode_root_page(
                read.bytes(0)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                self.inner.storage.profile().spec().namespace,
            )
            .map_err(queue_error)?;
            for offset in 0..64 {
                let ordinal = (root.cursor() as usize + offset) % 64;
                if queue::decode_job(
                    &self.control.jobs()[ordinal],
                    self.inner.storage.profile().spec().namespace,
                )
                .map_err(queue_error)?
                .state()
                    == JobState::Free
                {
                    return Ok(ordinal);
                }
            }
            Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))
        })();
        merge(
            selected,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    pub(crate) fn catalog_begin(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        self.health()?;
        let slot = self
            .control
            .free_slot()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))?;
        let ordinal = self.first_catalog_job(owner)?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = self.target(Plan4Page::Root)?.request;
        let job = self
            .target(Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            })?
            .request;
        if status.request.kind != PageKind::TransactionStatus
            || status.request.identity.owner.get() != 2
            || status.request.identity.owner_generation != one()
            || status.request.level != 0
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &[status.request])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let generation = (|| {
            let home = crate::storage::status_home::decode_status_home_page(
                read.bytes(0)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                status.request.identity,
                self.inner.storage.profile().spec().namespace,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            let cell = home
                .cell(0)
                .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            let crate::storage::transaction_record::TransactionStatusCell::Free(generation) =
                cell.core()
            else {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::WrongState,
                ));
            };
            generation
                .next()
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))
        })();
        let generation = merge(
            generation,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )?;
        let prefix = self.inner.storage.wal().prefix().map_err(|e| {
            fatal(
                self.inner.storage,
                RetentionPhase::Prepare,
                RetentionCause::Wal(e),
            )
        })?;
        self.control.issued_local = self
            .control
            .issued_local
            .checked_add(1)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?;
        let actor = TxnRef::new(
            crate::transaction::TxnId {
                database: prefix.stream.database,
                timeline: prefix.stream.timeline,
                writer_epoch: prefix.stream.writer_epoch,
                local: self.control.issued_local,
            },
            generation,
        )
        .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
        let location = StatusSlotRef::new(status.request.identity.page, 0, generation)
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
        let prepared = self.prepare_existing_catalog(
            owner,
            Change::Begin {
                actor,
                status: location,
            },
            Some(slot),
            [(allocator, 0), (root, 1), (job, 2), (status.request, 4)],
            private,
        )?;
        self.apply_existing_catalog(owner, prepared)
    }

    fn actor_catalog_job(&self, actor: TxnRef, terminal: bool) -> Result<usize> {
        let mut found = None;
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let job = queue::decode_job(bytes, self.inner.storage.profile().spec().namespace)
                .map_err(queue_error)?;
            if job.source() == Some(actor)
                && job.state().is_terminal() == terminal
                && found.replace(ordinal).is_some()
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        found.ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction))
    }

    fn catalog_status_request(&self, status: StatusSlotRef) -> TargetRequest {
        request(
            PageIdentity {
                database: self.inner.storage.profile().spec().namespace.database,
                page: status.page(),
                owner: ObjectId::new(2).expect("Status owner"),
                owner_generation: one(),
            },
            PageKind::TransactionStatus,
        )
    }

    pub(crate) fn catalog_rollback_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let terminal = self.actor_catalog_job(actor, true)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let terminal_job = queue::decode_job(
            &self.control.jobs()[terminal],
            self.inner.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?;
        let status = self.catalog_status_request(
            terminal_job
                .status()
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?,
        );
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = self.target(Plan4Page::Root)?.request;
        let mut root_job = None;
        for bytes in self.control.jobs() {
            let job = queue::decode_job(bytes, self.inner.storage.profile().spec().namespace)
                .map_err(queue_error)?;
            if job.source() == Some(actor)
                && job.state().is_root()
                && root_job.replace(job).is_some()
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        let prepared = if let Some(job) = root_job {
            let undo = job
                .undo()
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            let target = request(
                PageIdentity {
                    database: allocator.identity.database,
                    page: undo.page(),
                    owner: ObjectId::new(3).expect("Undo owner"),
                    owner_generation: one(),
                },
                PageKind::Undo,
            );
            self.prepare_existing_catalog(
                owner,
                Change::RollbackStart { terminal },
                Some(slot),
                [(allocator, 0), (root, 1), (status, 4), (target, 5)],
                private,
            )?
        } else {
            self.prepare_existing_catalog(
                owner,
                Change::RollbackStart { terminal },
                Some(slot),
                [(allocator, 0), (root, 1), (status, 4)],
                private,
            )?
        };
        self.apply_existing_catalog(owner, prepared)
    }

    pub(crate) fn catalog_abort_end(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let terminal = self.actor_catalog_job(actor, true)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let status = queue::decode_job(
            &self.control.jobs()[terminal],
            self.inner.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?
        .status()
        .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = self.target(Plan4Page::Root)?.request;
        let job = self
            .target(Plan4Page::Jobs {
                index: (terminal / 15) as u8,
            })?
            .request;
        let prepared = self.prepare_existing_catalog(
            owner,
            Change::AbortEnd { terminal },
            Some(slot),
            [
                (allocator, 0),
                (root, 1),
                (job, 2),
                (self.catalog_status_request(status), 4),
            ],
            private,
        )?;
        self.apply_existing_catalog(owner, prepared)
    }

    fn apply_existing_catalog(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        match prepared.count {
            2 => self.apply_existing_catalog_n::<2>(owner, prepared),
            3 => self.apply_existing_catalog_n::<3>(owner, prepared),
            4 => self.apply_existing_catalog_n::<4>(owner, prepared),
            5 => self.apply_existing_catalog_n::<5>(owner, prepared),
            6 => self.apply_existing_catalog_n::<6>(owner, prepared),
            7 => self.apply_existing_catalog_n::<7>(owner, prepared),
            8 => self.apply_existing_catalog_n::<8>(owner, prepared),
            9 => self.apply_existing_catalog_n::<9>(owner, prepared),
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            )),
        }
    }

    fn apply_existing_catalog_n<const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        let storage = self.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        // Commit must pass the real durable continuation, and reference removal
        // must pass original retirement admission. Neither uses this fast path.
        if matches!(
            prepared.operation(),
            Operation::Commit
                | Operation::Forward
                | Operation::Clr
                | Operation::Prune
                | Operation::RootFree
                | Operation::TerminalFinish
        ) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        prepared.check(owner, &self.inner.pool, &self.control, prefix)?;
        let requests: &[TargetRequest; N] = prepared
            .targets()
            .try_into()
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut write = self
            .inner
            .pool
            .reserve_target_set(owner, requests)
            .map_err(|e| frame(RetentionPhase::Install, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Install, e))?;
        let installed = (|| {
            let mut dpt = [const { None }; N];
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                let stamp = prepared
                    .stamp(at)
                    .ok_or_else(|| unchanged(RetentionPhase::Install, RetentionCause::Invariant))?;
                write
                    .matches_stamp(at, &stamp)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                if prepared.after(at).is_some() {
                    *dpt_entry = Some(
                        write
                            .preflight_dpt(at, &stamp)
                            .map_err(|e| frame(RetentionPhase::Install, e))?,
                    );
                }
            }
            let requirement = storage
                .wal()
                .stage_verified_required(
                    prepared.prefix,
                    prepared.observation.header,
                    prepared.frame,
                )
                .map_err(|e| {
                    if e.effect == Effect::Unchanged {
                        RetentionError {
                            phase: RetentionPhase::Stage,
                            effect: e.effect,
                            cause: RetentionCause::Wal(e),
                        }
                    } else {
                        fatal(storage, RetentionPhase::Stage, RetentionCause::Wal(e))
                    }
                })?;
            catalog_mark_staged(&prepared, &mut self.control, requirement)?;
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                let Some(after) = prepared.after(at) else {
                    continue;
                };
                let permit =
                    DirtyInstallPermit::from_retention(live::RetentionInstall::allocation(
                        self.inner.pool.bound_core(),
                        prepared.stamp(at).expect("checked existing"),
                        requirement,
                        after,
                    ));
                write
                    .install_page(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                write
                    .install_metadata(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                write
                    .install_dpt(dpt_entry.take().expect("preflight DPT"), &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
            }
            catalog_apply(&prepared, &mut self.control, storage.profile(), requirement)?;
            self.history.installed(requirement)?;
            Ok(requirement)
        })();
        let requirement = merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
        .map_err(|e| {
            if self.control.pending.is_some() {
                fatal(storage, RetentionPhase::Install, e.cause)
            } else {
                e
            }
        })?;
        let durable = storage
            .wal()
            .wait_durable(&requirement)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        if !durable.confirms(&requirement) {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        self.control.durable = Some(durable);
        self.control.pending = None;
        Ok(prepared.observation)
    }

    /// The actual Commit adapter consumes this receipt with the same original
    /// owner and rechecks its prefix, control cut and every target stamp.
    pub(crate) fn prepare_catalog_commit<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.health()?;
        let visibility = self
            .catalog_visibility
            .as_ref()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        if !visibility.matches_core(self.inner.pool.bound_core()) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let mut terminal = None;
        for (ordinal, cell) in self.control.jobs().iter().enumerate() {
            let job = queue::decode_job(cell, self.inner.storage.profile().spec().namespace)
                .map_err(queue_error)?;
            if job.state().is_terminal() && job.source() == Some(actor) {
                if terminal.is_some() || job.state() != JobState::TerminalLeased {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::Invariant,
                    ));
                }
                terminal = Some((
                    ordinal,
                    job.status().ok_or_else(|| {
                        unchanged(RetentionPhase::Prepare, RetentionCause::Invariant)
                    })?,
                ));
            }
        }
        let (terminal, status) = terminal.ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|error| frame(RetentionPhase::Prepare, error))?;
        let root = self.target(Plan4Page::Root)?.request;
        let job = self
            .target(Plan4Page::Jobs {
                index: (terminal / 15) as u8,
            })?
            .request;
        let status = request(
            PageIdentity {
                database: allocator.identity.database,
                page: status.page(),
                owner: ObjectId::new(2).expect("Status owner"),
                owner_generation: one(),
            },
            PageKind::TransactionStatus,
        );
        self.prepare_existing_catalog(
            owner,
            Change::Commit { terminal },
            Some(slot),
            [(allocator, 0), (root, 1), (job, 2), (status, 4)],
            private,
        )
    }

    fn catalog_preparation(&self) -> CatalogPreparation<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogPreparation {
            storage: self.inner.storage,
            pool: &self.inner.pool,
            control: &self.control,
            history: &self.history,
            visibility: self.catalog_visibility.as_ref(),
        }
    }

    fn prepare_existing_catalog<'i, const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        change: Change<'_>,
        slot: Option<usize>,
        selected: [(TargetRequest, u8); N],
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_existing_catalog(owner, change, slot, selected, private)
    }
}

/// A temporary view of the original preparation resources. It owns no pool,
/// owner, backing bytes or authority and cannot survive a mutable writer step.
struct CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    control: &'b control::RetentionControl<'a, 'c, 's>,
    history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    fn prepare_existing_catalog<'i, const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        change: Change<'_>,
        slot: Option<usize>,
        mut selected: [(TargetRequest, u8); N],
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        const {
            assert!(N >= 1 && N <= 9);
        }
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        health(storage, self.pool)?;
        if self.control.pending.is_some() || self.control.population.reserved() {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        if self
            .visibility
            .is_none_or(|visibility| !visibility.matches_core(self.pool.bound_core()))
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        owner.check_idle().map_err(|error| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: error.effect,
            cause: RetentionCause::Concurrency(error),
        })?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|error| frame(RetentionPhase::Prepare, error))?;
        let owner_identity =
            owner
                .visibility_identity(self.pool.bound_core())
                .map_err(|error| RetentionError {
                    phase: RetentionPhase::Prepare,
                    effect: error.effect,
                    cause: RetentionCause::Concurrency(error),
                })?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|error| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(error)))?;
        self.control.check_ledger(storage.profile(), prefix)?;
        selected.sort_unstable_by_key(|(target, _)| target_rank(*target));
        let (frame_space, pages) = private.split_at_mut(semantic_catalog::FRAME_BYTES);
        let (after_space, before_space) = pages.split_at_mut(8 * PAGE_BYTES);
        let after: &mut [[u8; PAGE_BYTES]; 8] = after_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("eight original after loans");
        let before: &mut [[u8; PAGE_BYTES]; 8] = before_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("eight original before loans");
        let (observation, requests, stamps) = {
            let runtime = &self.history;
            let mut audit = runtime.replay_scope(owner)?;
            let mut replay = audit.checkout()?;
            // Canonical replay does not use the physical request/stamp arrays.
            // Form them only after its complete nested source closure has ended.
            if let Err(error) =
                history::validate_live(storage, self.control, replay.bytes_mut(), prefix)
            {
                let rejected = merge(Err(error), replay.return_checked());
                return merge(rejected, audit.finish_checked());
            }
            let requests: [TargetRequest; N] = std::array::from_fn(|i| selected[i].0);
            let mut stamps = [None; 9];
            let prepared = (|| {
                let read = self
                    .pool
                    .reserve_target_set(owner, &requests)
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?
                    .try_read()
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?;
                let observed = (|| {
                    for (at, (target, role)) in selected.iter().enumerate() {
                        if *role == PROOF_ROLE {
                            let image = read
                                .bytes(at)
                                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
                            stamps[at] = Some(
                                read.observe(at)
                                    .map_err(|e| frame(RetentionPhase::Prepare, e))?
                                    .frame,
                            );
                            if image
                                != history::arena_page(
                                    replay.bytes(),
                                    target.identity.page.page_no as usize,
                                )
                            {
                                return Err(fatal(
                                    storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Invariant,
                                ));
                            }
                            continue;
                        }
                        if usize::from(*role) >= before.len() {
                            return Err(unchanged(
                                RetentionPhase::Prepare,
                                RetentionCause::Unsupported,
                            ));
                        }
                        let output = &mut before[usize::from(*role)];
                        stamps[at] = Some(
                            read.copy_snapshot(at, output)
                                .map_err(|error| frame(RetentionPhase::Prepare, error))?,
                        );
                        if output
                            != history::arena_page(
                                replay.bytes(),
                                target.identity.page.page_no as usize,
                            )
                        {
                            return Err(fatal(
                                storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            ));
                        }
                    }
                    let allocator = allocation_page::decode_allocation_page(
                        &before[0],
                        requests[0].identity,
                        storage.profile().spec().namespace,
                    )
                    .map_err(|error| {
                        fatal(
                            storage,
                            RetentionPhase::Census,
                            RetentionCause::Allocation(error),
                        )
                    })?;
                    for (target, _) in &selected {
                        let page = usize::try_from(target.identity.page.page_no).map_err(|_| {
                            unchanged(RetentionPhase::Prepare, RetentionCause::Membership)
                        })?;
                        if allocator.entry(page).map_err(|error| {
                            fatal(
                                storage,
                                RetentionPhase::Census,
                                RetentionCause::Allocation(error),
                            )
                        })? != (AllocationEntry::Allocated {
                            generation: target.identity.page.incarnation,
                            object: target.identity.owner,
                            object_generation: target.identity.owner_generation,
                            kind: target.kind,
                        }) {
                            return Err(fatal(
                                storage,
                                RetentionPhase::Census,
                                RetentionCause::Membership,
                            ));
                        }
                    }
                    let has = |role| selected.iter().any(|(_, actual)| *actual == role);
                    let inputs = Inputs {
                        profile: storage.profile(),
                        frame: FrameExpectation {
                            stream: prefix.stream,
                            previous_lsn: prefix.complete_lsn,
                        },
                        change,
                        jobs: self.control.jobs(),
                        root: &before[1],
                        jobs_page: has(2).then_some(&before[2]),
                        allocator: Some(&before[0]),
                        catalog_anchor: selected
                            .iter()
                            .position(|(_, role)| *role == PROOF_ROLE)
                            .map(|at| {
                                read.bytes(at)
                                    .map_err(|e| frame(RetentionPhase::Prepare, e))
                            })
                            .transpose()?,
                        catalog: has(3).then_some(&before[3]),
                        status: has(4).then_some(&before[4]),
                        undo: has(5).then_some(&before[5]),
                        transaction: slot.filter(|_| change.operation() != Operation::Begin).map(
                            |index| {
                                self.control.row(index)[..1536]
                                    .try_into()
                                    .expect("original transaction descriptor")
                            },
                        ),
                        aging: (self.control.row(control::AGING_SLOT)[control::PHASE] != 0).then(
                            || {
                                self.control.row(control::AGING_SLOT)[..1536]
                                    .try_into()
                                    .expect("original Aging descriptor")
                            },
                        ),
                    };
                    semantic_catalog::produce(&inputs, after, frame_space).map_err(semantic)
                })();
                merge(
                    observed,
                    read.release_checked()
                        .map_err(|error| frame(RetentionPhase::Prepare, error)),
                )
            })();
            let prepared = merge(prepared, replay.return_checked());
            let observation = merge(prepared, audit.finish_checked())?;
            (observation, requests, stamps)
        };
        let current = storage
            .wal()
            .prefix()
            .map_err(|error| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(error)))?;
        if current != prefix {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let mut targets = [requests[0]; 9];
        let mut roles = [PROOF_ROLE; 9];
        for (at, (request, role)) in selected.into_iter().enumerate() {
            targets[at] = request;
            roles[at] = role;
        }
        let (bytes, spare) = frame_space.split_at_mut(observation.frame_bytes);
        let in_before = spare.len() < PAGE_BYTES;
        let (future_wal, future_steps) = {
            let layout = semantic_catalog::decode_layout(
                bytes,
                FrameExpectation {
                    stream: prefix.stream,
                    previous_lsn: prefix.complete_lsn,
                },
                storage.profile(),
            )
            .map_err(semantic)?;
            // Forward fills nearly the whole frame, but its pair has no before
            // images. Other operations use one page after their exact frame end;
            // CLR therefore keeps both real pair-before pages intact.
            if in_before {
                if observation.operation != Operation::Forward
                    || roles[..N].iter().any(|role| matches!(role, 6 | 7))
                {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::Invariant,
                    ));
                }
                let (head, tail) = before.split_at_mut(6);
                prepare_inventory(
                    self.control,
                    storage.profile(),
                    observation,
                    slot,
                    layout,
                    &head[1],
                    &head[0],
                    after,
                    &mut tail[0],
                )?
            } else {
                prepare_inventory(
                    self.control,
                    storage.profile(),
                    observation,
                    slot,
                    layout,
                    &before[1],
                    &before[0],
                    after,
                    (&mut spare[..PAGE_BYTES])
                        .try_into()
                        .expect("named inventory page after exact frame"),
                )?
            }
        };
        if prefix
            .complete_offset
            .checked_add(observation.frame_bytes as u64)
            .and_then(|value| value.checked_add(future_wal))
            .is_none_or(|value| value > crate::storage::wal_io::WAL_FILE_CAP)
            || observation.header.lsn.checked_add(future_steps).is_none()
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
        }
        // Prepay bounded native/RAM progress through all outstanding credit.
        for at in 0..N {
            if observation.order[..usize::from(observation.count)].contains(&roles[at])
                && stamps[at].is_some_and(|stamp| {
                    stamp
                        .modification_counter
                        .checked_add(1)
                        .and_then(|value| value.checked_add(future_steps))
                        .is_none()
                })
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Exhausted,
                ));
            }
        }
        let root_image = if observation.order[..usize::from(observation.count)].contains(&1) {
            &after[1]
        } else {
            &before[1]
        };
        let (_, root) = queue::decode_root_page(root_image, storage.profile().spec().namespace)
            .map_err(queue_error)?;
        if root.revision().checked_add(future_steps).is_none() {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Exhausted,
            ));
        }
        let inventory = if in_before {
            &before[6]
        } else {
            (&spare[..PAGE_BYTES])
                .try_into()
                .expect("named inventory image")
        };
        Ok(PreparedCatalog {
            core: self.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            owner: owner_identity,
            prefix,
            observation,
            slot,
            requests: targets,
            roles,
            count: N,
            stamps,
            frame: bytes,
            frame_digest: content_digest(bytes),
            after,
            before,
            inventory: Some(inventory),
            control: ControlCut::capture(self.control),
        })
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn catalog_preparation(&self) -> CatalogPreparation<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogPreparation {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
        }
    }
}

/// Phase values exclude the original bound/core/backing loans and include
/// returned values/results that can coexist at the call boundary. Private bytes
/// are caller loans: every before/after/inventory slice is within the same
/// 425984-byte allocation. No phase adds another owned native page.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct CatalogLiveSource {
    pub(in super::super) prepare: usize,
    pub(in super::super) prepare_replay: usize,
    pub(in super::super) prepare_produce: usize,
    pub(in super::super) prepare_inventory: usize,
    pub(in super::super) prepare_return: usize,
    pub(in super::super) plan: usize,
    pub(in super::super) reserve: usize,
    pub(in super::super) forward: usize,
    pub(in super::super) cancel: usize,
    pub(in super::super) ordinary: usize,
    pub(in super::super) retirement: usize,
    pub(in super::super) peak: usize,
}

/// Original public entry locals that survive into the common preparer or
/// installer. Begin's read is consumed before this boundary; the Rollback
/// search loop has ended but its terminal/root Job views remain in scope.
fn ordinary_caller_source_bytes() -> usize {
    use std::mem::size_of;
    let arguments = size_of::<TxnRef>()
        + 4 * size_of::<&[u8]>()
        + 4 * size_of::<usize>()
        + 2 * size_of::<RetentionError>();
    let begin = 3 * size_of::<TargetRequest>()
        + size_of::<Generation>()
        + size_of::<WalPrefix>()
        + size_of::<StatusSlotRef>();
    let rollback = 4 * size_of::<TargetRequest>()
        + 2 * size_of::<queue::JobView<'_>>()
        + size_of::<Option<queue::JobView<'_>>>()
        + size_of::<crate::storage::undo_ref::UndoRef>();
    let commit = 4 * size_of::<TargetRequest>()
        + size_of::<Option<(usize, StatusSlotRef)>>()
        + size_of::<StatusSlotRef>()
        + size_of::<&visibility::BoundVisibility<'_, '_, '_>>();
    // AgingStart and AbortEnd are subsets of these actual caller packets.
    arguments + begin.max(rollback).max(commit)
}

pub(in super::super) fn catalog_live_source_bytes<W: WriterNamespace>() -> CatalogLiveSource {
    use crate::storage::{dirty_frame, tree_page::edit};
    use std::mem::size_of;
    type Prepared<'a> = PreparedCatalog<'a, 'a, 'a>;
    type Reservation<'a, W> = ReservedCatalogCreate<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    let scalar = size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 2 * size_of::<&()>() // temporary view receiver and wrapper argument
        + 2 * size_of::<WalPrefix>()
        + size_of::<OwnerIdentity>()
        + 6 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 24 * size_of::<usize>()
        + 20 * size_of::<&[u8]>();
    // Only the selected input is present during canonical replay. Physical
    // request/stamp arrays are created afterward and retained through return.
    // There is no PreparedCatalog until replay/reader/inventory have ended.
    let validation_outer = scalar
        + size_of::<[(TargetRequest, u8); 9]>()
        + size_of::<Change<'_>>()
        + size_of::<Option<usize>>();
    let prepare_outer =
        validation_outer + size_of::<[TargetRequest; 9]>() + size_of::<[Option<FrameStamp>; 9]>();
    let replay_carriers = size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>()
        + 3 * size_of::<Result<Observation>>();
    let prepare_replay = validation_outer + replay_carriers + history::source_bytes::<W>().peak;
    let prepare_produce = prepare_outer
        + replay_carriers
        + dirty_frame::target_set_call_bytes::<9>().max(
            size_of::<dirty_frame::ReadTargets<'_, '_, '_, '_, '_, '_, 9>>()
                + size_of::<Inputs<'_>>()
                + size_of::<crate::storage::allocation_page::AllocationPage<'_>>()
                + size_of::<Result<Observation>>()
                + semantic_catalog::source_bytes(),
        );
    // After cleanup, final role/target arrays have been formed, but no return
    // carrier exists. The exact shared encoder owns no duplicate output page.
    let assembled = size_of::<[TargetRequest; 9]>()
        + size_of::<[u8; 9]>()
        + size_of::<Observation>()
        + size_of::<Result<Observation>>();
    let prepare_inventory = prepare_outer
        + assembled
        + size_of::<semantic_catalog::LayoutView<'_>>()
        + 2 * size_of::<
            std::result::Result<semantic_catalog::LayoutView<'_>, semantic_catalog::Error>,
        >()
        + size_of::<ProjectedInventory<'_, '_, '_, '_, '_>>()
        + 2 * size_of::<Result<(u64, u64)>>()
        + crate::storage::plan4_recovery_base::catalog::inventory::current_source_bytes()
            .max(semantic_catalog::layout_source_bytes());
    let reader_return = prepare_outer
        + replay_carriers
        + 2 * size_of::<(Observation, [TargetRequest; 9], [Option<FrameStamp>; 9])>();
    let prepare_return = (prepare_outer
        + assembled
        + size_of::<Prepared<'_>>()
        + 2 * size_of::<Result<Prepared<'_>>>()
        + 2 * size_of::<ControlCut>()
        + 3 * size_of::<foundation::codec::ContentHasher>()
        + 1536
        + queue::source_bytes())
    .max(reader_return);
    // check_ledger precedes array/replay setup. Its descriptor transaction,
    // Aging and queue scans are sequential and bounded by this existing leaf.
    let ledger = scalar
        + size_of::<[(TargetRequest, u8); 9]>()
        + 640
        + 2 * size_of::<crate::storage::obligation::DescriptorSpec>()
        + queue::source_bytes().max(semantic_catalog::transaction_source_bytes());
    let prepare = prepare_replay
        .max(prepare_produce)
        .max(prepare_inventory)
        .max(prepare_return)
        .max(ledger);
    let plan = scalar
        + size_of::<CreatePlan>()
        + 2 * size_of::<Result<CreatePlan>>()
        + 2 * size_of::<[TargetRequest; 7]>()
        + size_of::<[&[u8]; semantic_catalog::images::CATALOG_EDIT_CELLS]>()
        + size_of::<edit::LeafEditPlan<'_, '_>>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + dirty_frame::target_set_call_bytes::<7>()
        + semantic_catalog::source_census().catalog_image;
    let physical = crate::storage::wal_io::DURABILITY_CALL_BYTES
        + 2 * size_of::<std::result::Result<WalRequirement<'_>, crate::storage::wal_io::WalIoError>>(
        )
        + 2 * size_of::<std::result::Result<DurableWal<'_>, crate::storage::wal_io::WalIoError>>();
    // The consumed receipt exists once in the actual install callee. Returned
    // Observation/requirement values are included; its caller no longer owns a
    // second receipt after the move.
    let ordinary_install = size_of::<Prepared<'_>>()
        + scalar
        + size_of::<[Option<dirty_frame::DptStamp<'_>>; 9]>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 2 * size_of::<DurableWal<'_>>()
        + size_of::<DirtyInstallPermit<'_, '_, '_>>()
        + 3 * size_of::<Result<Observation>>()
        + dirty_frame::target_set_call_bytes::<9>()
        + catalog_control_source_bytes().max(physical);
    let ordinary = (ordinary_caller_source_bytes() + prepare.max(ordinary_install))
        .max(retirement::prepare_caller_source_bytes() + prepare)
        .max(retirement::selection_source_bytes());
    let token = size_of::<Reservation<'_, W>>();
    let token_return = token
        + 2 * size_of::<Result<Reservation<'_, W>>>()
        + 2 * size_of::<RetentionReservationError<Reservation<'_, W>>>();
    // The pair claim/authority/candidates do not yet exist during preparation.
    // They are created only after the prepared receipt returns successfully.
    let reserve_prepare = size_of::<CreatePlan>()
        + size_of::<TxnRef>()
        + size_of::<usize>()
        + 5 * size_of::<&[u8]>()
        + 2 * size_of::<RetentionError>();
    let reserve_owned = size_of::<CreatePlan>()
        + size_of::<TxPairClaimKey<'_, '_>>()
        + size_of::<TxPairAuthority<'_, '_, '_>>()
        + size_of::<[dirty_frame::StatusCandidate; 2]>()
        + scalar;
    let reserve = plan
        .max(reserve_prepare + prepare)
        .max(reserve_owned + ordinary_install.max(dirty_frame::tx_pair_source_census()))
        .max(token_return + scalar);
    let forward_install = token
        + scalar
        + size_of::<Prepared<'_>>()
        + size_of::<semantic_catalog::LayoutView<'_>>()
        + size_of::<crate::storage::page::PageView<'_>>()
        + size_of::<[Option<&[u8; PAGE_BYTES]>; 9]>()
        + 4 * 32
        + size_of::<TxPairControlCommit<'_, '_, '_, '_>>()
        + size_of::<TxPairInstallPermit<'_, '_, '_>>()
        + dirty_frame::tx_pair_source_census()
        + catalog_control_source_bytes().max(physical);
    let forward = (token + prepare)
        .max(forward_install)
        .max(token_return + scalar);
    let pair_return = scalar
        + size_of::<TxPairAuthority<'_, '_, '_>>()
        + size_of::<super::TxPairCancelPermit<'_, '_, '_, '_>>()
        + size_of::<super::PairIoQuiescence<'_, '_, '_, '_>>()
        + dirty_frame::tx_pair_source_census().max(dirty_frame::target_set_call_bytes::<1>());
    // This caller is the one original reservation, not an ordinary Begin/Job
    // entry. It has no second request/Job caller packet.
    let cancel =
        (token + prepare.max(ordinary_install).max(pair_return)).max(token_return + scalar);
    let retirement = (retirement::prepare_caller_source_bytes() + prepare)
        .max(retirement::selection_source_bytes())
        .max(retirement::source_bytes());
    CatalogLiveSource {
        prepare,
        prepare_replay,
        prepare_produce,
        prepare_inventory,
        prepare_return,
        plan,
        reserve,
        forward,
        cancel,
        ordinary,
        retirement,
        peak: prepare
            .max(plan)
            .max(reserve)
            .max(forward)
            .max(cancel)
            .max(ordinary)
            .max(retirement),
    }
}
