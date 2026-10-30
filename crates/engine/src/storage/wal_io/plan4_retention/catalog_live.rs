//! Actual scope2 preparation on the original bound. The only constructors of
//! prepared groups are the checked source/target paths in this module.
use super::super::*;
use super::{TxPairAuthority, TxPairClaimKey, TxPairControlCommit, TxPairInstallPermit};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry},
        page::{PageKind, PageRef},
        retention_page::catalog3::{self as queue, JobState},
        semantic_catalog::{self, Change, Inputs, Observation, Operation},
        session_fence::SessionFence,
        wal_frame::FrameExpectation,
        wal_io::WalPrefix,
    },
    transaction::concurrency::{OwnerIdentity, PageConcurrency},
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch},
};
use std::sync::atomic::AtomicU8;

#[path = "catalog_lifecycle.rs"]
pub(in super::super) mod lifecycle;
#[path = "catalog_retire.rs"]
pub(in super::super) mod retirement;

#[path = "catalog_targets.rs"]
mod targets;
use targets::CatalogTargetAccess;

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

/// Temporary disjoint borrows of the original CREATE resources. This owns
/// no session, backing, claim or source admission and cannot outlive its caller.
struct CatalogCreateAccess<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    control: &'b mut control::RetentionLedger<'a, 'c, 's>,
    history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
    flush: &'b mut flush::RetentionFlushControl<'c, 's, 'w>,
}
#[path = "catalog_create.rs"]
mod create;
#[path = "catalog_shared_create.rs"]
pub(in super::super) mod shared_create;
use shared_create::{CatalogCancelled, CatalogCreateAdmission, CatalogCreateError, CatalogCreated};

/// Owns the durable Tx reservation, original pair claim and continuous flush
/// lease. Only the necessary original resources are borrowed; no session or
/// owner is reconstructed. The shared entry binds the original statement and
/// source admission; the legacy raw entry exists only in test fixtures.
pub(crate) struct ReservedCatalogCreate<
    'r,
    't,
    'v,
    'i,
    'a,
    's,
    'c,
    'p,
    'd,
    'm,
    'w,
    W: WriterNamespace,
> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'r SharedPool<'c, 's, 'p, 'd>,
    control: &'r mut control::RetentionLedger<'a, 'c, 's>,
    history: &'r history::runtime::RuntimeHistory<'a, 'c, 's>,
    visibility: Option<&'r visibility::BoundVisibility<'a, 'c, 's>>,
    io: create::CatalogPairIoLease<'r, 'c, 's, 'w>,
    owner: &'r mut Owner<'c, 's>,
    private: Option<&'i mut [u8; PRIVATE_BYTES]>,
    definition: &'i crate::api::TableSpec,
    admission: Option<CatalogCreateAdmission<'r, 't, 'v, 'a, 'c, 's>>,
    plan: CreatePlan,
    key: TxPairClaimKey<'c, 's>,
    reserve: PageLsn,
    active: bool,
}
impl<W: WriterNamespace> Drop
    for ReservedCatalogCreate<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>
{
    fn drop(&mut self) {
        if self.active {
            self.storage.session_fence().fence();
            self.storage.wal().completion.notify_all();
        }
    }
}

#[cfg(test)]
impl<'s, 'c, 'p, 'd, W: WriterNamespace>
    ReservedCatalogCreate<'_, '_, '_, '_, '_, 's, 'c, 'p, 'd, '_, '_, W>
{
    pub(in crate::storage) fn catalog_test_lease_identity(&self) -> (usize, u64) {
        self.io.test_identity()
    }
    pub(in crate::storage) fn catalog_test_lease_fault(&mut self, kind: u8) {
        self.io.test_fault(kind);
    }
    pub(in crate::storage) fn catalog_test_bound(&self) -> &Self {
        self
    }
    pub(in crate::storage) fn catalog_test_pool(&self) -> &SharedPool<'c, 's, 'p, 'd> {
        self.pool
    }
    pub(in crate::storage) fn catalog_test_epochs(&self) -> (bool, u64, Vec<(u8, u64, u64)>) {
        self.visibility.expect("original visibility").test_epochs()
    }
    pub(in crate::storage) fn catalog_test_q(&self) -> u64 {
        self.visibility.expect("original visibility").test_q()
    }
    pub(in crate::storage) fn catalog_test_wal_state(&self) -> (WalPrefix, u64) {
        let wal = self.storage.wal();
        let prefix = wal.append.lock().unwrap_or_else(|e| e.into_inner()).prefix;
        (
            prefix,
            wal.first_write_lsn
                .load(std::sync::atomic::Ordering::Acquire),
        )
    }
}
#[cfg(test)]
impl<W: WriterNamespace> super::super::CatalogTestState
    for ReservedCatalogCreate<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>
{
    fn catalog_test_with_pool<T>(&self, run: impl FnOnce(&SharedPool<'_, '_, '_, '_>) -> T) -> T {
        run(self.pool)
    }
    fn recovery_test_bank(&self) -> &[u8] {
        &self.control.bytes[..65_536]
    }
    fn catalog_test_jobs(&self) -> &[[u8; 1024]; 64] {
        self.control.jobs()
    }
    fn free_undo_test_state(
        &self,
    ) -> Result<super::super::recovery_test_observation::FreeUndoTestState> {
        let prefix = self
            .storage
            .wal()
            .prefix()
            .map_err(|e| fatal(self.storage, RetentionPhase::Census, RetentionCause::Wal(e)))?;
        let (reserved, installing) = self.control.population.test_state();
        let (claim, pins) = dirty_frame::init_test_support::claim_state(self.pool);
        Ok((
            prefix,
            self.control
                .durable
                .as_ref()
                .map(|d| d.observation().frame.lsn),
            reserved,
            installing,
            self.control.pending.is_some(),
            claim,
            pins,
        ))
    }
}

impl<'i, 's, 'c, W: WriterNamespace>
    ReservedCatalogCreate<'_, '_, '_, 'i, '_, 's, 'c, '_, '_, '_, '_, W>
{
    pub(crate) fn object(&self) -> ObjectHandle {
        self.plan.object
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged cancellation rejection returns the original reservation"
    )]
    pub(crate) fn cancel(
        mut self,
    ) -> std::result::Result<CatalogCancelled<'i>, RetentionReservationError<Self>> {
        match self.cancel_inner() {
            Ok(()) => {
                self.complete(None)
                    .map_err(RetentionReservationError::Fatal)?;
                self.active = false;
                Ok(CatalogCancelled::new(
                    self.private.take().expect("original private loan"),
                ))
            }
            Err(error) if error.effect == Effect::Unchanged && self.control.pending.is_none() => {
                Err(RetentionReservationError::Before {
                    error,
                    reservation: self,
                })
            }
            Err(error) => Err(RetentionReservationError::Fatal(fatal(
                self.storage,
                error.phase,
                error.cause,
            ))),
        }
    }
    fn cancel_inner(&mut self) -> Result<()> {
        if let Some(admission) = &self.admission {
            admission.statement.reserved(
                self.storage,
                self.control,
                self.plan.ordinal,
                self.plan.object,
                self.reserve,
            )?;
        }
        self.io
            .check::<W>(self.storage, self.pool.bound_core(), &self.plan.pair)?;
        CatalogCreateAccess {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            flush: self.io.original,
        }
        .cancel(
            &mut self.key,
            &self.plan,
            self.reserve,
            self.owner,
            self.private.as_deref_mut().expect("original private loan"),
        )
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged rejection retains the original linear reservation"
    )]
    pub(crate) fn initialize(
        mut self,
    ) -> std::result::Result<CatalogCreated<'i>, RetentionReservationError<Self>> {
        match self.initialize_inner() {
            Ok(object) => {
                self.complete(Some(object))
                    .map_err(RetentionReservationError::Fatal)?;
                self.active = false;
                Ok(CatalogCreated::new(
                    object,
                    self.private.take().expect("original private loan"),
                ))
            }
            Err(error) if error.effect == Effect::Unchanged && self.control.pending.is_none() => {
                Err(RetentionReservationError::Before {
                    error,
                    reservation: self,
                })
            }
            Err(error) => Err(RetentionReservationError::Fatal(fatal(
                self.storage,
                error.phase,
                error.cause,
            ))),
        }
    }

    fn initialize_inner(&mut self) -> Result<ObjectHandle> {
        if let Some(admission) = &self.admission {
            admission.statement.reserved(
                self.storage,
                self.control,
                self.plan.ordinal,
                self.plan.object,
                self.reserve,
            )?;
        }
        self.io
            .check::<W>(self.storage, self.pool.bound_core(), &self.plan.pair)?;
        CatalogCreateAccess {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            flush: self.io.original,
        }
        .initialize(
            &mut self.key,
            &self.plan,
            self.reserve,
            self.owner,
            self.private.as_deref_mut().expect("original private loan"),
            self.definition,
        )
    }
    fn complete(&mut self, object: Option<ObjectHandle>) -> Result<()> {
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::CatalogCreateCompleted,
        );
        // The actual key is already consumed. No failure from this point can
        // return Before, even if a lower cleanup reports Unchanged.
        let result = if let Some(admission) = &mut self.admission {
            let registered = admission
                .statement
                .completed(self.storage, self.control, object);
            let cleanup = admission.finish();
            merge(registered, cleanup)
        } else {
            Ok(())
        };
        result.map_err(|error| fatal(self.storage, error.phase, error.cause))
    }
}

/// Immutable bytes and the original control cut needed after staging. This
/// owns no pin, WAL effect or publication authority; those remain in the
/// original stage/installation receipts. Only preparation constructs it.
pub(in super::super) struct CatalogPayload<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    prefix: WalPrefix,
    observation: Observation,
    slot: Option<usize>,
    frame: &'i [u8],
    frame_digest: [u8; 32],
    after: &'i [[u8; PAGE_BYTES]; 8],
    inventory: Option<&'i [u8; PAGE_BYTES]>,
    control: ControlCut,
}

/// Consuming stage continuation. Canonical physical after order is captured
/// before the preparation arrays end; Allocation's proof-only entry is None.
/// No Clone/Copy, raw constructor or PreparedCatalog reconstruction exists.
pub(super) struct CatalogCommitImage<'i, 'c, 's> {
    payload: CatalogPayload<'i, 'c, 's>,
    after: [Option<&'i [u8; PAGE_BYTES]>; 4],
}
impl<'i, 'c, 's> CatalogCommitImage<'i, 'c, 's> {
    pub(super) fn payload(&self) -> &CatalogPayload<'i, 'c, 's> {
        &self.payload
    }
    pub(super) fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        self.after.get(at).copied().flatten()
    }
    pub(super) fn core(&self) -> &'c PageConcurrency<'s> {
        self.payload.core
    }
    pub(super) fn binding(&self) -> &'s AtomicU8 {
        self.payload.binding
    }
    pub(super) fn fence(&self) -> &'s SessionFence {
        self.payload.fence
    }
    pub(super) fn frame_digest(&self) -> [u8; 32] {
        self.payload.frame_digest
    }
    pub(super) fn observation(&self) -> Observation {
        self.payload.observation
    }
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
    fn capture(control: &control::RetentionLedger<'_, '_, '_>) -> Self {
        Self {
            bank: content_digest(&control.bytes[..BANK_END]),
            jobs: content_digest(&control.bytes[BANK_END..JOB_END]),
            inventory: content_digest(&control.bytes[JOB_END..INVENTORY_END]),
            issued: control.issued_local,
            adopted: control.adopted_local,
        }
    }
    fn matches(self, control: &control::RetentionLedger<'_, '_, '_>) -> bool {
        self.bank == content_digest(&control.bytes[..BANK_END])
            && self.jobs == content_digest(&control.bytes[BANK_END..JOB_END])
            && self.inventory == content_digest(&control.bytes[JOB_END..INVENTORY_END])
            && self.issued == control.issued_local
            && self.adopted == control.adopted_local
    }
}

impl<'i, 'c, 's> PreparedCatalog<'i, 'c, 's> {
    pub(in crate::storage) fn operation(&self) -> Operation {
        self.payload.observation.operation
    }
    pub(in crate::storage) fn observation(&self) -> Observation {
        self.payload.observation
    }
    pub(in crate::storage) fn prefix(&self) -> WalPrefix {
        self.payload.prefix
    }
    pub(in crate::storage) fn owner(&self) -> OwnerIdentity {
        self.owner
    }
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.payload.core
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.payload.binding
    }
    pub(in crate::storage) fn fence(&self) -> &'s SessionFence {
        self.payload.fence
    }
    pub(in crate::storage) fn transaction_slot(&self) -> Option<usize> {
        self.payload.slot
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
        self.payload.frame
    }
    pub(in crate::storage) fn frame_digest(&self) -> [u8; 32] {
        self.payload.frame_digest
    }
    pub(in crate::storage) fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        let role = *self.roles.get(at)?;
        (at < self.count
            && self.payload.observation.order[..usize::from(self.payload.observation.count)]
                .contains(&role))
        .then(|| &self.payload.after[usize::from(role)])
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
    fn add_initialized_pair(&mut self, pair: [TargetRequest; 2]) -> Result<()> {
        if self.operation() != Operation::Forward || self.count != 7 {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        for (side, request) in pair.into_iter().enumerate() {
            let view = crate::storage::page::decode_page(
                &self.payload.after[6 + side],
                request.identity,
                request.kind,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            if view.header().lsn != PageLsn(self.payload.observation.header.lsn)
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
        control: &control::RetentionLedger<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()> {
        check_group(&self.payload, self.owner, owner, pool, control, prefix)
    }
    pub(in super::super) fn confirms_requirement(&self, requirement: WalRequirement<'_>) -> bool {
        self.payload.confirms_requirement(requirement)
    }
    pub(in super::super) fn layout(
        &self,
        profile: crate::storage::control::Profile,
    ) -> Result<semantic_catalog::LayoutView<'i>> {
        self.payload.layout(profile)
    }
    /// Caller has checked Commit and the four-target inventory before Intent.
    /// Extraction itself is infallible and cannot introduce a post-stage retry.
    pub(super) fn into_commit_image(self) -> CatalogCommitImage<'i, 'c, 's> {
        let after = std::array::from_fn(|at| self.after(at));
        CatalogCommitImage {
            payload: self.payload,
            after,
        }
    }
}

impl<'i> CatalogPayload<'i, '_, '_> {
    fn operation(&self) -> Operation {
        self.observation.operation
    }
    fn inventory_after(&self) -> Option<&'i [u8; PAGE_BYTES]> {
        self.inventory
    }
    pub(in super::super) fn confirms_requirement(&self, requirement: WalRequirement<'_>) -> bool {
        confirms_group(self, requirement)
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
    control: &mut control::RetentionLedger<'_, '_, 's>,
    requirement: WalRequirement<'s>,
) -> Result<()> {
    mark_staged_group(&prepared.payload, control, requirement)
}

/// The scope-neutral cut of a prepared existing group (Catalog or Row
/// payload): one check, staging relation, mark and apply prelude for both.
trait GroupPayload<'c, 's> {
    fn core(&self) -> &'c PageConcurrency<'s>;
    fn binding(&self) -> &'s AtomicU8;
    fn fence(&self) -> &'s SessionFence;
    fn prefix(&self) -> WalPrefix;
    fn lsn(&self) -> u64;
    fn frame(&self) -> &[u8];
    fn frame_digest(&self) -> [u8; 32];
    fn cut(&self) -> ControlCut;
}
impl<'c, 's> GroupPayload<'c, 's> for CatalogPayload<'_, 'c, 's> {
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

/// The original owner, bound origin, unchanged prefix/control cut and frame
/// digest, rechecked immediately before the install of either scope.
fn check_group<'c, 's>(
    group: &impl GroupPayload<'c, 's>,
    expected: OwnerIdentity,
    owner: &Owner<'c, 's>,
    pool: &SharedPool<'c, 's, '_, '_>,
    control: &control::RetentionLedger<'_, 'c, 's>,
    prefix: WalPrefix,
) -> Result<()> {
    let actual = owner
        .visibility_identity(group.core())
        .map_err(|error| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: error.effect,
            cause: RetentionCause::Concurrency(error),
        })?;
    if actual != expected
        || !std::ptr::eq(group.core(), pool.bound_core())
        || !pool.uses_storage_origin(group.binding(), group.fence())
    {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::BindingMismatch,
        ));
    }
    if prefix != group.prefix() || control.pending.is_some() || !group.cut().matches(control) {
        return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
    }
    if content_digest(group.frame()) != group.frame_digest() {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::Invariant,
        ));
    }
    Ok(())
}

/// The staged requirement is exactly this group's frame after its prefix.
fn confirms_group<'c, 's>(
    group: &impl GroupPayload<'c, 's>,
    requirement: WalRequirement<'_>,
) -> bool {
    let location = requirement.location();
    let prefix = group.prefix();
    requirement.uses_fence(group.fence())
        && location.stream == prefix.stream
        && location.file == prefix.file
        && location.previous_lsn == prefix.complete_lsn
        && location.lsn == group.lsn()
        && location.start == prefix.complete_offset
        && location.length == group.frame().len()
        && location.start.checked_add(location.length as u64) == Some(location.end)
}

fn mark_staged_group<'c, 'g, 's>(
    group: &impl GroupPayload<'c, 'g>,
    control: &mut control::RetentionLedger<'_, '_, 's>,
    requirement: WalRequirement<'s>,
) -> Result<()> {
    if !confirms_group(group, requirement)
        || control.pending.is_some()
        || !group.cut().matches(control)
    {
        group.fence().fence();
        return Err(RetentionError {
            phase: RetentionPhase::Install,
            effect: Effect::CallerMustResolve,
            cause: RetentionCause::Invariant,
        });
    }
    control.pending = Some(requirement);
    Ok(())
}

/// The control-apply prelude: the original cut is still unchanged and the
/// pending requirement is this group's staged one.
fn check_staged_group<'c, 's>(
    group: &impl GroupPayload<'c, 's>,
    control: &control::RetentionLedger<'_, '_, '_>,
    requirement: WalRequirement<'_>,
) -> Result<()> {
    if !confirms_group(group, requirement)
        || !group.cut().matches(control)
        || control.pending.is_none_or(|pending| {
            !pending.uses_fence(group.fence()) || pending.location() != requirement.location()
        })
    {
        group.fence().fence();
        return Err(RetentionError {
            phase: RetentionPhase::Install,
            effect: Effect::CallerMustResolve,
            cause: RetentionCause::Invariant,
        });
    }
    Ok(())
}

/// Applies only the control part of an already installed actual group. The
/// caller still owns the complete original write/version target set; this
/// helper performs no pool access and cannot publish Q or release a stage.
pub(super) fn catalog_apply(
    prepared: &CatalogPayload<'_, '_, '_>,
    control: &mut control::RetentionLedger<'_, '_, '_>,
    profile: crate::storage::control::Profile,
    requirement: WalRequirement<'_>,
) -> Result<()> {
    check_staged_group(prepared, control, requirement)?;
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

/// Prepay bounded native/RAM progress of one prepared group of either scope
/// through all outstanding credit: WAL file room and LSN for its frame and
/// every future step its successor inventory owes, each changed target's
/// modification counter, and the after Root revision.
fn check_progress_room(
    prefix: WalPrefix,
    (lsn, frame_bytes): (u64, usize),
    (future_wal, future_steps): (u64, u64),
    (roles, stamps): (&[u8], &[Option<FrameStamp>]),
    order: &[u8],
    root: &[u8; PAGE_BYTES],
    profile: crate::storage::control::Profile,
) -> Result<()> {
    if prefix
        .complete_offset
        .checked_add(frame_bytes as u64)
        .and_then(|value| value.checked_add(future_wal))
        .is_none_or(|value| value > crate::storage::wal_io::WAL_FILE_CAP)
        || lsn.checked_add(future_steps).is_none()
    {
        return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
    }
    for (role, stamp) in roles.iter().zip(stamps) {
        if order.contains(role)
            && stamp.is_some_and(|stamp| {
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
    let (_, root) = queue::decode_root_page(root, profile.spec().namespace).map_err(queue_error)?;
    if root.revision().checked_add(future_steps).is_none() {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::Exhausted,
        ));
    }
    Ok(())
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
        // The scope-neutral group helpers forward the owner identity, prefix
        // and their reference arguments one call deeper.
        + size_of::<OwnerIdentity>()
        + size_of::<WalPrefix>()
        + 6 * size_of::<&()>()
        // These are sequential calls; the parent values above remain live.
        + semantic_catalog::layout_source_bytes().max(queue::source_bytes())
}

/// Borrowed successor projection of one group of either scope. The shared
/// encoder is a codec, while this projection retains the group's verified
/// successor rows and the original control source for every other row.
struct ProjectedInventory<'b, 'i, 'a, 'c, 's> {
    control: &'b control::RetentionLedger<'a, 'c, 's>,
    rows: GroupRows<'i>,
    root: &'i [u8; PAGE_BYTES],
    allocator: &'i [u8; PAGE_BYTES],
    jobs: Option<crate::storage::page::PageView<'i>>,
}
/// The control rows a group changes, read from its verified layout: the
/// transaction ATT row (slot and successor descriptor, None when cleared),
/// the successor Aging descriptor when it changes, and the changed Job.
#[derive(Clone, Copy)]
struct GroupRows<'i> {
    transaction: Option<(usize, Option<&'i [u8; 1536]>)>,
    aging: Option<Option<&'i [u8; 1536]>>,
    job_ordinal: Option<usize>,
}
impl crate::storage::plan4_recovery_base::catalog::inventory::InventoryRows
    for ProjectedInventory<'_, '_, '_, '_, '_>
{
    fn transaction(&self, index: usize) -> Option<&[u8; 1536]> {
        if let Some((slot, after)) = self.rows.transaction
            && slot == index
        {
            after
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
        if let Some(after) = self.rows.aging {
            after
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
        if self.rows.job_ordinal == Some(ordinal) {
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
impl<'i> GroupRows<'i> {
    /// A Catalog group: Tx handoffs and RollbackStart own the ATT row, every
    /// other operation the Aging row.
    fn catalog(
        observation: Observation,
        slot: Option<usize>,
        view: semantic_catalog::LayoutView<'i>,
    ) -> Self {
        let operation = observation.operation;
        Self {
            transaction: (operation.handoff() || operation == Operation::RollbackStart)
                .then_some(slot)
                .flatten()
                .map(|slot| {
                    (
                        slot,
                        (!matches!(operation, Operation::Commit | Operation::AbortEnd))
                            .then(|| view.descriptor(0))
                            .flatten(),
                    )
                }),
            aging: (operation != Operation::RollbackStart)
                .then(|| view.descriptor(usize::from(operation.handoff()))),
            job_ordinal: observation.job_ordinal,
        }
    }
}

/// The XOI successor of one verified group of either scope, encoded before
/// its stage. `jobs_after` is the changed Jobs page (with `rows.job_ordinal`).
#[allow(
    clippy::too_many_arguments,
    reason = "borrowed original phase sections"
)]
fn prepare_inventory(
    control: &control::RetentionLedger<'_, '_, '_>,
    profile: crate::storage::control::Profile,
    lsn: u64,
    rows: GroupRows<'_>,
    root: &[u8; PAGE_BYTES],
    allocator: &[u8; PAGE_BYTES],
    jobs_after: Option<&[u8; PAGE_BYTES]>,
    output: &mut [u8; PAGE_BYTES],
) -> Result<(u64, u64)> {
    let jobs = match (rows.job_ordinal, jobs_after) {
        (Some(ordinal), Some(page)) => Some(
            queue::decode_jobs_page(page, profile.spec().namespace, (ordinal / 15) as u8)
                .map_err(queue_error)?,
        ),
        (None, _) => None,
        (Some(_), None) => {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
    };
    let projected = ProjectedInventory {
        control,
        rows,
        root,
        allocator,
        jobs,
    };
    let generation = control
        .source
        .base()
        .map_or(1, |base| base.view().header().generation);
    crate::storage::plan4_recovery_base::catalog::inventory::encode_row_current(
        profile, generation, lsn, &projected, output,
    )
    .map_err(|error| unchanged(RetentionPhase::Prepare, RetentionCause::Base(error)))
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn catalog_create_access(&mut self) -> CatalogCreateAccess<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogCreateAccess {
            storage: self.inner.storage,
            pool: &self.inner.pool,
            control: &mut self.control.ledger,
            history: &self.history,
            visibility: self.catalog_visibility.as_ref(),
            flush: &mut self.flush,
        }
    }

    #[cfg(test)]
    pub(crate) fn reserve_catalog_create<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        actor: TxnRef,
        undo: &Plan4Target,
        definition: &'r crate::api::TableSpec,
        private: &'r mut [u8; PRIVATE_BYTES],
    ) -> Result<ReservedCatalogCreate<'r, 'r, 'r, 'r, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        self.catalog_create_access()
            .reserve(owner, actor, undo, definition, private, None)
            .map_err(CatalogCreateError::error)
    }

    pub(crate) fn catalog_aging_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let (mut writer, _, mut flusher) = self.split();
        writer.catalog_aging_start(owner, private, &mut flusher)
    }

    pub(crate) fn catalog_begin(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let (mut writer, _, mut flusher) = self.split();
        writer.catalog_begin(owner, status, private, &mut flusher)
    }

    pub(crate) fn catalog_rollback_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let (mut writer, _, mut flusher) = self.split();
        writer.catalog_rollback_start(owner, actor, private, &mut flusher)
    }

    pub(crate) fn catalog_abort_end(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let (mut writer, _, mut flusher) = self.split();
        writer.catalog_abort_end(owner, actor, private, &mut flusher)
    }

    fn apply_pair_catalog(
        &mut self,
        key: &TxPairClaimKey<'c, 's>,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        self.catalog_create_access()
            .apply_pair_catalog(key, owner, prepared)
    }

    /// Fault fixtures may retain a preparation to corrupt its source cut. The
    /// production entry prepares only inside the original Commit reservation.
    #[cfg(test)]
    pub(crate) fn prepare_catalog_commit<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_catalog_commit(owner, actor, private)
    }

    fn catalog_preparation(&self) -> CatalogPreparation<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogPreparation {
            storage: self.inner.storage,
            pool: &self.inner.pool,
            control: &self.control.ledger,
            history: &self.history,
            visibility: self.catalog_visibility.as_ref(),
            access: CatalogTargetAccess::Ordinary,
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
    control: &'b control::RetentionLedger<'a, 'c, 's>,
    history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
    access: CatalogTargetAccess<'b, 'c, 's>,
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// Entry checks of an existing-group preparation of either scope: health,
    /// no pending/population, the bound visibility core, an idle original
    /// owner and reader, its identity, the WAL prefix and the whole ledger.
    fn existing_entry(&self, owner: &mut Owner<'c, 's>) -> Result<(OwnerIdentity, WalPrefix)> {
        let storage = self.storage;
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
        Ok((owner_identity, prefix))
    }

    /// Under the original read of either scope's selected targets: copy each
    /// before image into the loan its role names (a proof-only target is
    /// observed in place), prove it equals the canonical replay arena page,
    /// and prove every target's allocator membership from the before loan
    /// `allocator` names.
    fn observe_targets<const N: usize>(
        &self,
        read: &targets::CatalogRead<'_, '_, '_, 'c, 's, '_, '_, N>,
        selected: &[(TargetRequest, u8); N],
        before: &mut [[u8; PAGE_BYTES]; 8],
        arena: &[u8],
        stamps: &mut [Option<FrameStamp>; 9],
        allocator: u8,
    ) -> Result<()> {
        let storage = self.storage;
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
                if image != history::arena_page(arena, target.identity.page.page_no as usize) {
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
            if output != history::arena_page(arena, target.identity.page.page_no as usize) {
                return Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
        }
        let identity = selected
            .iter()
            .find(|(_, role)| *role == allocator)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?
            .0
            .identity;
        let allocator = allocation_page::decode_allocation_page(
            &before[usize::from(allocator)],
            identity,
            storage.profile().spec().namespace,
        )
        .map_err(|error| {
            fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(error),
            )
        })?;
        for (target, _) in selected {
            let page = usize::try_from(target.identity.page.page_no)
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Membership))?;
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
        Ok(())
    }

    fn prepare_catalog_commit<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        health(self.storage, self.pool)?;
        if self.control.pending.is_some() || self.control.population.reserved() {
            return Err(fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        if !visibility.matches_core(self.pool.bound_core()) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let mut terminal = None;
        // Catalog selection skips every Row-scope cell (another actor's).
        for (ordinal, cell) in self.control.jobs().iter().enumerate() {
            let Some(job) =
                queue::decode_catalog_cell(cell, self.storage.profile().spec().namespace)
                    .map_err(queue_error)?
            else {
                continue;
            };
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
            .pool
            .target_request(0)
            .map_err(|error| frame(RetentionPhase::Prepare, error))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let job = target_from_source(
            self.storage,
            Plan4Page::Jobs {
                index: (terminal / 15) as u8,
            },
        )?
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
        let (owner_identity, prefix) = self.existing_entry(owner)?;
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
                    .access
                    .read(self.pool, owner, &requests)
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?;
                let observed = (|| {
                    self.observe_targets(&read, &selected, before, replay.bytes(), &mut stamps, 0)?;
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
            let changed =
                |role| observation.order[..usize::from(observation.count)].contains(&role);
            let rows = GroupRows::catalog(observation, slot, layout);
            let jobs_after = observation.job_ordinal.map(|_| &after[2]);
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
                    observation.header.lsn,
                    rows,
                    if changed(1) { &after[1] } else { &head[1] },
                    if changed(0) { &after[0] } else { &head[0] },
                    jobs_after,
                    &mut tail[0],
                )?
            } else {
                prepare_inventory(
                    self.control,
                    storage.profile(),
                    observation.header.lsn,
                    rows,
                    if changed(1) { &after[1] } else { &before[1] },
                    if changed(0) { &after[0] } else { &before[0] },
                    jobs_after,
                    (&mut spare[..PAGE_BYTES])
                        .try_into()
                        .expect("named inventory page after exact frame"),
                )?
            }
        };
        check_progress_room(
            prefix,
            (observation.header.lsn, observation.frame_bytes),
            (future_wal, future_steps),
            (&roles[..N], &stamps[..N]),
            &observation.order[..usize::from(observation.count)],
            if observation.order[..usize::from(observation.count)].contains(&1) {
                &after[1]
            } else {
                &before[1]
            },
            storage.profile(),
        )?;
        let inventory = if in_before {
            &before[6]
        } else {
            (&spare[..PAGE_BYTES])
                .try_into()
                .expect("named inventory image")
        };
        Ok(PreparedCatalog {
            payload: CatalogPayload {
                core: self.pool.bound_core(),
                binding: &storage.binding,
                fence: storage.session_fence(),
                prefix,
                observation,
                slot,
                frame: bytes,
                frame_digest: content_digest(bytes),
                after,
                inventory: Some(inventory),
                control: ControlCut::capture(self.control),
            },
            owner: owner_identity,
            requests: targets,
            roles,
            count: N,
            stamps,
            before,
        })
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(super) fn prepare_catalog_commit<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_catalog_commit(owner, actor, private)
    }

    fn catalog_preparation(&self) -> CatalogPreparation<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogPreparation {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            access: CatalogTargetAccess::Ordinary,
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
    pub(in super::super) start_health: usize,
    pub(in super::super) start_root: usize,
    pub(in super::super) start_status: usize,
    pub(in super::super) lifecycle: usize,
    /// Row Begin/RollbackStart/AbortEnd under RowProgress (ROW-BEGIN-001).
    pub(in super::super) row_lifecycle: usize,
    pub(in super::super) retirement: usize,
    pub(in super::super) peak: usize,
}

/// Upper estimates of the RowWrite C bodies (S2-0 Row write amendment §4–5,
/// S2W-C01/C02), composed before any Row write effect body exists from the
/// existing census terms at the whole write target set (nine targets). Each
/// phase value here is the C body plus the progress shell; the
/// wrapper/step/Result carriers and the caller share
/// (`ROW_WRITE_CALLER_BYTES`) are added by the Row write census, which checks
/// every phase against CatalogProgressCharge by name. Only the Row write
/// census computes them (`row_write_bodies`); the split census, `peak` and
/// every charge do not. An S2 unit that supplies a body replaces its estimate.
///
/// Recorded constraints for the units that supply the bodies:
/// - `apply` rechecks at the stage by recomputing the draft commitment and
///   comparing the prepared frame/after digests only. It does not run the
///   observer, `validate_live` or the XOI inventory again: with
///   `group_prepare` in its max, apply is 1146 + 7328 + 34404 + 1544 =
///   44422 B, over the charge before any caller share (S2-d).
/// - `validation_lane` rebuilds `RowPreflightContext` from the current
///   frames and the private copies; its decode is not a term yet (S2-b).
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct RowWriteBodies {
    /// Held through every phase: the admission, gate and their Result,
    /// failure and release/reopen carriers (`commit_carrier_bytes`, as the
    /// lifecycle shell) and the two unwind fences.
    pub(in super::super) shell: usize,
    /// `check_catalog_pair_io` for the lease (CREATE's lease check term).
    pub(in super::super) lease_check: usize,
    /// The preflight's target set: requests/roles and before stamps of T.
    pub(in super::super) target_set: usize,
    /// Deep checks: `check_ledger` with the Row admission subset, or the
    /// shared transaction/statement check over the shell scalars. Every
    /// effect step repeats the open/view/source/statement checks.
    pub(in super::super) check: usize,
    /// Preflight's Catalog leaf, anchor and table record decode for the
    /// schema, index-count and RowLeaf checks and the private copies: the
    /// Catalog source census over the shell scalars.
    pub(in super::super) table_census: usize,
    /// One retention LOAD of a nonresident target over the shell scalars.
    pub(in super::super) load: usize,
    /// Free Undo container selection: the two-target allocator/Undo read and
    /// slot0 decode over the shell scalars.
    pub(in super::super) undo_select: usize,
    /// Read-set or write-bit registration in the isolation ledger over the
    /// shell scalars.
    pub(in super::super) register: usize,
    /// Commitment recomputation over the shell scalars: T's requests and
    /// stamps, two hashers and the digests.
    pub(in super::super) commit: usize,
    /// The existing start selection phases (RootJob/Status choice analog).
    pub(in super::super) select: usize,
    /// Row group preparation at nine targets: `validate_live`'s history peak
    /// or C's producer or the observer of A's frame, the XOI inventory, the
    /// assembled group, or `check_ledger`.
    pub(in super::super) group_prepare: usize,
    /// Apply's re-assembly of the prepared group from the cut and the
    /// checked frame/after, without the observer or `validate_live`.
    pub(in super::super) group_reassemble: usize,
    /// The existing-group install at nine targets: stage, page install,
    /// durable wait and the control record.
    pub(in super::super) group_install: usize,
    /// Prepared-group and Observation carriers across prepare and install.
    pub(in super::super) group_carriers: usize,
    /// The Row lifecycle recomputed here from the same repeated terms; S2W-C03
    /// requires it to equal `catalog_live_source_bytes().row_lifecycle`.
    pub(in super::super) lifecycle_check: usize,
    pub(in super::super) preflight: usize,
    pub(in super::super) validation_lane: usize,
    pub(in super::super) reserve: usize,
    pub(in super::super) producer_lane: usize,
    /// The Forward `ReservedRow::prepare`. The CLR task's prepare is S4's own
    /// entry census.
    pub(in super::super) prepare: usize,
    pub(in super::super) apply: usize,
    pub(in super::super) cancel: usize,
    pub(in super::super) finish: usize,
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
    type Reservation<'a, W> = ReservedCatalogCreate<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
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
    let scope_carriers = size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>();
    let replay_carriers = scope_carriers + 3 * size_of::<Result<Observation>>();
    let history_peak = history::source_bytes::<W>().peak;
    let prepare_replay = validation_outer + replay_carriers + history_peak;
    // The shared target observation (`observe_targets`) is a completed call
    // before the producer; both run under the typed read.
    let observe = 8 * size_of::<&()>()
        + size_of::<u8>()
        + 2 * size_of::<Result<()>>()
        + size_of::<crate::storage::allocation_page::AllocationPage<'_>>()
        + size_of::<PageIdentity>()
        + 2 * size_of::<Result<FrameStamp>>();
    let prepare_produce = prepare_outer
        + replay_carriers
        + targets::call_bytes::<9>().max(
            size_of::<targets::CatalogRead<'_, '_, '_, '_, '_, '_, '_, 9>>()
                + observe.max(
                    size_of::<Inputs<'_>>()
                        + size_of::<crate::storage::allocation_page::AllocationPage<'_>>()
                        + size_of::<Result<Observation>>()
                        + semantic_catalog::profile3_source_census().total,
                ),
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
        + crate::storage::plan4_recovery_base::catalog::inventory::row_current_source_bytes()
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
    // check_ledger runs inside the shared entry helper of either scope.
    let ledger = scalar
        + size_of::<[(TargetRequest, u8); 9]>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + 2 * size_of::<&()>()
        + 640
        + 2 * size_of::<crate::storage::obligation::DescriptorSpec>()
        + queue::source_bytes()
            .max(semantic_catalog::profile3_transaction_source_bytes())
            // The Row admission subset is exclusive with the Catalog branch.
            .max(crate::storage::semantic_row::ledger::admission_source_bytes());
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
        + semantic_catalog::profile3_source_census().catalog_image;
    let physical = crate::storage::wal_io::DURABILITY_CALL_BYTES
        + 2 * size_of::<std::result::Result<WalRequirement<'_>, crate::storage::wal_io::WalIoError>>(
        )
        + 2 * size_of::<std::result::Result<DurableWal<'_>, crate::storage::wal_io::WalIoError>>();
    // The consumed receipt exists once in the actual install callee. Returned
    // Observation/requirement values are included; its caller no longer owns a
    // second receipt after the move.
    let ordinary_install = lifecycle::application_carrier_bytes::<W>()
        + visibility::epoch_change_carriers()
        + size_of::<Prepared<'_>>()
        + scalar
        + size_of::<[Option<dirty_frame::DptStamp<'_>>; 9]>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 2 * size_of::<DurableWal<'_>>()
        + 2 * size_of::<InstallGrant<'_, '_, '_>>()
        + 3 * size_of::<Result<Observation>>()
        + targets::call_bytes::<9>()
        + catalog_control_source_bytes()
            .max(physical)
            .max(visibility::epoch_transition_source_bytes());
    let ordinary = (ordinary_caller_source_bytes() + prepare.max(ordinary_install))
        .max(retirement::prepare_caller_source_bytes() + prepare)
        .max(retirement::selection_source_bytes());
    let (start_health, start_root, start_status) = lifecycle::start_selection_source_phases::<W>();
    // The existing CREATE planner also uses the moved first-job view. Its
    // selected terminal/status and input arguments remain across that call.
    let plan = plan.max(
        start_root
            + size_of::<queue::JobView<'_>>()
            + size_of::<StatusSlotRef>()
            + size_of::<TxnRef>()
            + 4 * size_of::<&()>(),
    );
    let lifecycle = ordinary
        .max(lifecycle::quiescence_source_bytes())
        .max(start_health)
        .max(start_root)
        .max(start_status)
        + source::lifecycle_carrier_bytes::<W>()
        + transaction::writable::lifecycle_caller_bytes();
    // CREATE-only forwarding costs. The original common preparation and
    // application views remain counted in their existing lower phases. A
    // moved PreparedCatalog is not a second owned receipt in the wrapper.
    let view = size_of::<CatalogCreateAccess<'_, '_, '_, '_, '_, '_, '_, '_, W>>();
    let reference = size_of::<&()>();
    let factory = 2 * view + 2 * reference;
    let plan_call = factory + 5 * reference + size_of::<TxnRef>() + size_of::<Result<CreatePlan>>();
    let reserve_apply_call = factory + 3 * reference + size_of::<Result<Observation>>();
    let unstaged_return_call = factory
        + 4 * reference
        + size_of::<PageLsn>()
        + size_of::<super::TxPairCancelKind>()
        + size_of::<Option<WalRequirement<'_>>>()
        + size_of::<Result<()>>();
    let lease_check = 12 * reference
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<super::super::super::super::flush_io::IoObservation>();
    let forward_call = lease_check
        + factory
        + 6 * reference
        + size_of::<PageLsn>()
        + size_of::<Result<ObjectHandle>>()
        + 2 * reference; // newly borrowed key/plan closure captures
    let cancel_call =
        lease_check + factory + 5 * reference + size_of::<PageLsn>() + size_of::<Result<()>>();
    let plan = plan + plan_call;
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
        + size_of::<dirty_frame::CatalogPairCandidates<'_, '_, '_>>()
        + scalar;
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
    let forward = (token + prepare + forward_call)
        .max(forward_install + forward_call)
        .max(token_return + scalar);
    let pair_return = scalar
        + size_of::<TxPairAuthority<'_, '_, '_>>()
        + size_of::<super::TxPairCancelPermit<'_, '_, '_, '_>>()
        + size_of::<super::PairIoQuiescence<'_, '_, '_, '_>>()
        + dirty_frame::tx_pair_source_census()
            .max(dirty_frame::tx_pair_existing_source_bytes::<1>());
    let reserve = plan
        .max(reserve_prepare + prepare)
        .max(
            reserve_owned
                + (ordinary_install + reserve_apply_call)
                    .max(dirty_frame::tx_pair_source_census())
                    .max(pair_return + unstaged_return_call),
        )
        .max(token_return + scalar)
        + lease_check
        + 3 * size_of::<create::CatalogPairIoLease<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<create::CatalogPairIoLease<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<(TxPairClaimKey<'_, '_>, PageLsn)>>()
        + 2 * view
        + 12 * reference
        + size_of::<TxnRef>()
        + size_of::<std::result::Result<Reservation<'_, W>, CatalogCreateError<'_>>>()
        + 2 * create::parts_return_bytes::<W>();
    // The small view call has ended before the consuming wrapper constructs
    // its original large Before/Reservation return carrier.
    // This caller is the one original reservation, not an ordinary Begin/Job
    // entry. It has no second request/Job caller packet.
    let cancel = (token
        + cancel_call
        + visibility::epoch_change_carriers()
        + prepare
            .max(ordinary_install)
            .max(pair_return)
            .max(visibility::epoch_transition_source_bytes()))
    .max(token_return + scalar);
    // Admission remains live through every CREATE phase and checked return.
    // Binding checks run sequentially with deep planning/application.
    let shared = shared_create::carrier_bytes();
    let checked = token + transaction::catalog::check_source_bytes();
    let reserve = reserve.max(checked) + shared;
    let forward = forward.max(checked) + shared;
    let cancel = cancel.max(checked) + shared;
    // The Row lifecycle: the same shell, entry/ledger/replay validation and
    // install body with the Row producer, payload and control part.
    let row = lifecycle::row::row_lifecycle_terms();
    let row_prepare = (scalar + row.outer + scope_carriers + history_peak.max(row.produce))
        .max(scalar + row.outer + row.inventory)
        .max(scalar + row.outer + row.assembled)
        .max(ledger);
    let row_install = lifecycle::application_carrier_bytes::<W>()
        + visibility::epoch_change_carriers()
        + scalar
        + size_of::<[Option<dirty_frame::DptStamp<'_>>; 9]>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 2 * size_of::<DurableWal<'_>>()
        + 2 * size_of::<InstallGrant<'_, '_, '_>>()
        + targets::call_bytes::<4>()
        + row
            .control
            .max(physical)
            .max(visibility::epoch_transition_source_bytes());
    let row_lifecycle = (row.caller + row_prepare.max(row_install))
        .max(start_health)
        .max(start_root)
        .max(start_status)
        + source::commit_carrier_bytes()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + lifecycle::entry_carrier_bytes::<W>()
        + row.carriers
        + transaction::writable::row_lifecycle_caller_bytes();
    let retirement = ((retirement::prepare_caller_source_bytes() + prepare)
        .max(retirement::selection_source_bytes())
        + source::retirement_prepare_carrier_bytes())
    .max(source::retirement_source_peak());
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
        start_health,
        start_root,
        start_status,
        lifecycle,
        row_lifecycle,
        retirement,
        peak: prepare
            .max(plan)
            .max(reserve)
            .max(forward)
            .max(cancel)
            .max(ordinary)
            .max(lifecycle)
            .max(row_lifecycle)
            .max(retirement),
    }
}

/// The RowWrite C body estimates (`RowWriteBodies`), for the Row write census
/// only; nothing on the split path calls this. The terms it shares with
/// `catalog_live_source_bytes` (`scalar`, `scope_carriers`, the history peak,
/// `ledger`, `physical`, the start selection and CREATE's lease check) repeat
/// that function's expressions, and the Row lifecycle is recomputed from them
/// as `lifecycle_check`, which S2W-C03 compares with the split census value,
/// so the two copies cannot drift apart unnoticed.
pub(in super::super) fn row_write_bodies<W: WriterNamespace>() -> RowWriteBodies {
    use crate::storage::dirty_frame;
    use std::mem::size_of;
    let scalar = size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 2 * size_of::<&()>() // temporary view receiver and wrapper argument
        + 2 * size_of::<WalPrefix>()
        + size_of::<OwnerIdentity>()
        + 6 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 24 * size_of::<usize>()
        + 20 * size_of::<&[u8]>();
    let scope_carriers = size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>();
    let history_peak = history::source_bytes::<W>().peak;
    let ledger = scalar
        + size_of::<[(TargetRequest, u8); 9]>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + 2 * size_of::<&()>()
        + 640
        + 2 * size_of::<crate::storage::obligation::DescriptorSpec>()
        + queue::source_bytes()
            .max(semantic_catalog::profile3_transaction_source_bytes())
            .max(crate::storage::semantic_row::ledger::admission_source_bytes());
    let physical = crate::storage::wal_io::DURABILITY_CALL_BYTES
        + 2 * size_of::<std::result::Result<WalRequirement<'_>, crate::storage::wal_io::WalIoError>>(
        )
        + 2 * size_of::<std::result::Result<DurableWal<'_>, crate::storage::wal_io::WalIoError>>();
    let (start_health, start_root, start_status) = lifecycle::start_selection_source_phases::<W>();
    let reference = size_of::<&()>();
    let lease_check = 12 * reference
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<super::super::super::super::flush_io::IoObservation>();
    // The existing-group install of a Row group with `call` for its target
    // array and `control` for its control part.
    let install = |call: usize, control: usize| {
        lifecycle::application_carrier_bytes::<W>()
            + visibility::epoch_change_carriers()
            + scalar
            + size_of::<[Option<dirty_frame::DptStamp<'_>>; 9]>()
            + 3 * size_of::<WalRequirement<'_>>()
            + 2 * size_of::<DurableWal<'_>>()
            + 2 * size_of::<InstallGrant<'_, '_, '_>>()
            + call
            + control
                .max(physical)
                .max(visibility::epoch_transition_source_bytes())
    };
    // The Row lifecycle (four targets) from the same terms: the drift check.
    let row = lifecycle::row::row_lifecycle_terms();
    let row_prepare = (scalar + row.outer + scope_carriers + history_peak.max(row.produce))
        .max(scalar + row.outer + row.inventory)
        .max(scalar + row.outer + row.assembled)
        .max(ledger);
    let lifecycle_check = (row.caller
        + row_prepare.max(install(targets::call_bytes::<4>(), row.control)))
    .max(start_health)
    .max(start_root)
    .max(start_status)
        + source::commit_carrier_bytes()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + lifecycle::entry_carrier_bytes::<W>()
        + row.carriers
        + transaction::writable::row_lifecycle_caller_bytes();
    // The Row group over the whole write target set T (≤ 9 targets).
    let group = lifecycle::row::row_group_terms::<9>();
    let shell = source::commit_carrier_bytes()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>();
    let target_set = size_of::<[(TargetRequest, u8); 9]>()
        + size_of::<[Option<FrameStamp>; 9]>()
        + size_of::<Option<usize>>();
    let check = ledger.max(scalar + transaction::catalog::check_source_bytes());
    let table_census = scalar + super::super::read::catalog_source_bytes::<W>();
    let load = scalar
        + super::super::lookup_load::source_bytes::<W>()
        + size_of::<Plan4Target>()
        + 2 * size_of::<Result<Option<RetentionResidencyStep>>>();
    let undo_select = scalar
        + super::super::read::undo_read_source_bytes()
        + 2 * size_of::<crate::storage::undo_ref::UndoRef>()
        + 2 * size_of::<Result<()>>();
    let register = scalar + transaction::catalog_read_source_bytes();
    let commit = scalar
        + size_of::<[TargetRequest; 9]>()
        + size_of::<[Option<FrameStamp>; 9]>()
        + 2 * size_of::<foundation::codec::ContentHasher>()
        + 2 * size_of::<Result<FrameStamp>>()
        + 4 * 32
        + 8 * size_of::<&()>();
    let select = start_health.max(start_root).max(start_status);
    let group_prepare = (scalar
        + group.outer
        + scope_carriers
        + history_peak.max(group.produce).max(group.observer))
    .max(scalar + group.outer + group.inventory)
    .max(scalar + group.outer + group.assembled)
    .max(ledger);
    let group_reassemble = scalar + group.outer + group.assembled;
    let group_install = install(targets::call_bytes::<9>(), group.control);
    let group_carriers = group.carriers;
    // Sequential steps of one call are max-combined; what one call holds
    // throughout is added. Every phase holds the shell.
    let preflight = shell
        + lease_check
        + target_set
        + check
            .max(table_census)
            .max(load)
            .max(undo_select)
            .max(register)
            .max(commit)
            .max(select);
    let lane = shell + commit;
    let validation_lane = lane + 2 * size_of::<RowPreflightContext>();
    let producer_lane = lane + 2 * size_of::<RowProducerContext>();
    let reserve = shell
        + group_carriers
        + check
            .max(commit)
            .max(register)
            .max(group_prepare)
            .max(group_install);
    let prepare = shell + group_carriers + check.max(commit).max(group_prepare);
    let apply = shell
        + group_carriers
        + check
            .max(commit)
            .max(register)
            .max(group_reassemble)
            .max(group_install);
    let cancel = shell
        + group_carriers
        + check
            .max(commit)
            .max(register)
            .max(group_prepare)
            .max(group_install);
    RowWriteBodies {
        shell,
        lease_check,
        target_set,
        check,
        table_census,
        load,
        undo_select,
        register,
        commit,
        select,
        group_prepare,
        group_reassemble,
        group_install,
        group_carriers,
        lifecycle_check,
        preflight,
        validation_lane,
        reserve,
        producer_lane,
        prepare,
        apply,
        cancel,
        finish: shell,
    }
}

/// Commit's caller plus the existing N9 common preparation closure. Its
/// returned PreparedCatalog still owns nine request/stamp cells even for N4.
pub(in super::super) fn commit_prepare_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    let common = ordinary_caller_source_bytes() + catalog_live_source_bytes::<W>().prepare;
    let (health, _, _) = lifecycle::start_selection_source_phases::<W>();
    let select = ordinary_caller_source_bytes()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<queue::JobView<'_>>()
        + queue::source_bytes();
    // View result, writer forward result and the synchronous caller match;
    // the common preparer already accounts for its own return carriers.
    common.max(health).max(select)
        // The private writer forwarder owns another by-value actor until the
        // borrowed view returns; the public actor is charged in every window.
        + size_of::<TxnRef>()
        + 2 * size_of::<Result<PreparedCatalog<'_, '_, '_>>>()
        + 4 * size_of::<&()>()
}

#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Negative capsule fixture: perform the existing actual CREATE through its
    /// private view, without inventing any grant. This is not a product entry.
    pub(in crate::storage) fn catalog_test_forward_beside_capsule(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        undo: &Plan4Target,
        definition: &crate::api::TableSpec,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> ObjectHandle {
        let access = CatalogCreateAccess {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            flush: flusher.control,
        };
        let reserved = access
            .reserve(owner, actor, undo, definition, private, None)
            .map_err(CatalogCreateError::error)
            .unwrap();
        reserved
            .initialize()
            .ok()
            .expect("actual CREATE Forward")
            .into_parts()
            .0
    }
}

#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Actual CREATE plus authentic statement registration. The intervening
    /// legacy body is a fixture bridge, not shared product source admission.
    pub(in crate::storage) fn catalog_test_forward_in_statement(
        &mut self,
        owner: &mut Owner<'c, 's>,
        view: &RowStatementView<'_, '_, 'a, 'c, 's>,
        undo: &Plan4Target,
        definition: &crate::api::TableSpec,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<ObjectHandle> {
        let actor = self.catalog_statement_create_actor(owner, view)?;
        let object = self
            .catalog_test_forward_beside_capsule(owner, actor, undo, definition, private, flusher);
        self.catalog_statement_created(owner, view, object)?;
        Ok(object)
    }
}
