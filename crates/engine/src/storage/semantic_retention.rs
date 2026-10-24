//! Exact N=0 retention groups and unpublished canonical after images.
//! A trusted live/replay caller supplies the selected before state and complete
//! queue census. Success is a byte observation, never a stage/apply capability.
#![allow(dead_code)] // Awaiting the original live retention owner and recovery consumer.

use super::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    control::Profile,
    obligation::{
        self, DescriptorSpec, InventoryClaim, ObligationError, OwnerChain, OwnerState,
        RemainingBudget, SystemProgress, WorkPhase,
    },
    page::{self, FileId, PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind, PageLsn},
    retention_obligation::{self, AGING_START_BYTES, AgingObligationError},
    retention_page::{
        self, EMPTY_PEAK, FINISH_BYTES, JOB_BYTES, JOB_COUNT, JobSlotRef, JobSpec, JobState,
        RetentionError, RootSpec,
    },
    status_home::{self, StatusHome},
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell,
        TxnRef,
    },
    wal_frame::{
        self, FrameExpectation, FrameKind, RedoKind, RedoTarget, WalError, WalFrameHeader,
    },
};
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectId},
};

pub(crate) const HANDOFF_BYTES: usize = 56_712;
pub(crate) const ROLLBACK_BYTES: usize = 3_040;
pub(crate) const BEGIN_ADMISSION_WAL: u64 = 171_576;
const SPAN_START: usize = 72;
const SPAN_END: usize = 16_352;
const REDO_BYTES: usize = 16_344;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionSemanticError {
    InvalidInput,
    UnsupportedOperation,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
    Page(PageError),
    Wal(WalError),
    Retention(RetentionError),
    Aging(AgingObligationError),
    Descriptor(ObligationError),
}
impl From<PageError> for RetentionSemanticError {
    fn from(e: PageError) -> Self {
        Self::Page(e)
    }
}
impl From<WalError> for RetentionSemanticError {
    fn from(e: WalError) -> Self {
        Self::Wal(e)
    }
}
impl From<RetentionError> for RetentionSemanticError {
    fn from(e: RetentionError) -> Self {
        Self::Retention(e)
    }
}
impl From<AgingObligationError> for RetentionSemanticError {
    fn from(e: AgingObligationError) -> Self {
        Self::Aging(e)
    }
}
impl From<ObligationError> for RetentionSemanticError {
    fn from(e: ObligationError) -> Self {
        Self::Descriptor(e)
    }
}
type Result<T> = std::result::Result<T, RetentionSemanticError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionOperation {
    AgingStart,
    Begin,
    AbortEnd,
    Finish,
}
impl RetentionOperation {
    fn handoff(self) -> bool {
        matches!(self, Self::Begin | Self::AbortEnd)
    }
    fn code(self) -> u16 {
        match self {
            Self::AgingStart => 16,
            Self::Begin => 4,
            Self::AbortEnd => 3,
            Self::Finish => 17,
        }
    }
    fn length(self) -> usize {
        match self {
            Self::AgingStart => AGING_START_BYTES,
            Self::Begin | Self::AbortEnd => HANDOFF_BYTES,
            Self::Finish => FINISH_BYTES,
        }
    }
    fn count(self) -> usize {
        if self == Self::AgingStart { 1 } else { 3 }
    }
    fn header_bytes(self) -> usize {
        if self.handoff() { 256 } else { 192 }
    }
    fn descriptor_bytes(self) -> usize {
        if self.handoff() { 3072 } else { 1536 }
    }
    fn native_bytes(self) -> usize {
        if self == Self::AgingStart { 1152 } else { 3904 }
    }
    fn operation_at(self) -> usize {
        160 + self.header_bytes() + self.descriptor_bytes()
    }
    fn native_at(self) -> usize {
        self.operation_at() + 256
    }
    fn redo_at(self) -> usize {
        self.native_at() + self.native_bytes()
    }
}

/// Selected expectations, not values recovered by copying a candidate frame.
/// previous_local is the previous adopted high-water; failed issued IDs may gap.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetentionExpected {
    pub(crate) frame: FrameExpectation,
    pub(crate) profile: Profile,
    pub(crate) actor: Option<TxnRef>,
    pub(crate) status: Option<StatusSlotRef>,
    pub(crate) status_page: Option<PageIdentity>,
    pub(crate) job: Option<JobSlotRef>,
    pub(crate) previous_local: u64,
}
#[derive(Clone, Copy)]
pub(crate) struct RetentionBefore<'a> {
    pub(crate) root: &'a [u8; PAGE_BYTES],
    pub(crate) jobs: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) status: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) sender: Option<&'a [u8; 1536]>,
    pub(crate) aging: Option<&'a [u8; 1536]>,
    pub(crate) job_cells: &'a [[u8; JOB_BYTES]; JOB_COUNT],
}
/// All images are unpublished caller scratch and may change on error.
pub(crate) struct RetentionAfter<'a> {
    pub(crate) root: &'a mut [u8; PAGE_BYTES],
    pub(crate) jobs: Option<&'a mut [u8; PAGE_BYTES]>,
    pub(crate) status: Option<&'a mut [u8; PAGE_BYTES]>,
}
#[derive(Clone, Copy)]
struct AfterView<'a> {
    root: &'a [u8; PAGE_BYTES],
    jobs: Option<&'a [u8; PAGE_BYTES]>,
    status: Option<&'a [u8; PAGE_BYTES]>,
}
impl RetentionAfter<'_> {
    fn view(&self) -> AfterView<'_> {
        AfterView {
            root: self.root,
            jobs: self.jobs.as_deref(),
            status: self.status.as_deref(),
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct RetentionObservation<'a> {
    operation: RetentionOperation,
    header: WalFrameHeader,
    frame: &'a [u8],
}
impl<'a> RetentionObservation<'a> {
    pub(crate) fn operation(self) -> RetentionOperation {
        self.operation
    }
    pub(crate) fn header(self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn bytes(self) -> &'a [u8] {
        self.frame
    }
    pub(crate) fn sender_after(self) -> Option<&'a [u8; 1536]> {
        if self.operation.handoff() {
            Some(self.frame[416..1952].try_into().expect("descriptor"))
        } else {
            None
        }
    }
    pub(crate) fn aging_after(self) -> &'a [u8; 1536] {
        let start = if self.operation.handoff() { 1952 } else { 352 };
        self.frame[start..start + 1536]
            .try_into()
            .expect("descriptor")
    }
}
struct Computed {
    header: WalFrameHeader,
    operation: [u8; 256],
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    retention_page::put(bytes, at, value);
}
fn require(ok: bool, error: RetentionSemanticError) -> Result<()> {
    if ok { Ok(()) } else { Err(error) }
}
fn next(value: u64) -> Result<u64> {
    value
        .checked_add(1)
        .ok_or(RetentionSemanticError::OrderMismatch)
}
fn row(bytes: usize) -> RemainingBudget {
    RemainingBudget {
        steps: 1,
        wal: bytes as u64,
        protected: bytes as u64,
        peak: EMPTY_PEAK,
        ..RemainingBudget::default()
    }
}

/// The exact additive Tx debt; job cleanup debt belongs to Aging, not this row.
pub(crate) fn begin_remaining() -> [RemainingBudget; 5] {
    let mut rows = [RemainingBudget::default(); 5];
    rows[2] = row(HANDOFF_BYTES);
    rows[3] = row(ROLLBACK_BYTES);
    rows
}

fn validate_expected(
    kind: RetentionOperation,
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    a: &RetentionAfter<'_>,
) -> Result<u64> {
    let ns = e.profile.spec().namespace;
    require(
        ns.timeline == 1
            && e.frame.stream.database == ns.database
            && e.frame.stream.timeline == ns.timeline
            && e.frame.stream.writer_epoch != 0,
        RetentionSemanticError::InvalidInput,
    )?;
    if kind == RetentionOperation::AgingStart {
        require(
            e.actor.is_none()
                && e.status.is_none()
                && e.status_page.is_none()
                && e.job.is_none()
                && e.previous_local == 0
                && b.jobs.is_none()
                && b.status.is_none()
                && b.sender.is_none()
                && b.aging.is_none()
                && a.jobs.is_none()
                && a.status.is_none(),
            RetentionSemanticError::InvalidInput,
        )?;
    } else {
        let actor = e.actor.ok_or(RetentionSemanticError::InvalidInput)?;
        let location = e.status.ok_or(RetentionSemanticError::InvalidInput)?;
        let page = e.status_page.ok_or(RetentionSemanticError::InvalidInput)?;
        e.job
            .ok_or(RetentionSemanticError::InvalidInput)?
            .ordinal()?;
        require(
            b.jobs.is_some()
                && b.status.is_some()
                && b.aging.is_some()
                && a.jobs.is_some()
                && a.status.is_some()
                && b.sender.is_some() == (kind == RetentionOperation::AbortEnd),
            RetentionSemanticError::InvalidInput,
        )?;
        require(
            actor.id().database == ns.database
                && actor.id().timeline == ns.timeline
                && actor.id().writer_epoch <= e.frame.stream.writer_epoch
                && location.page() == page.page
                && location.generation() == actor.generation()
                && location.slot() == 0
                && page.database == ns.database
                && page.owner == ObjectId::new(2).expect("Status object")
                && page.owner_generation.get() == 1
                && page.page.file == FileId::new(0x0000_0001_0000_0001).expect("first file")
                && (9..64).contains(&page.page.page_no),
            RetentionSemanticError::IdentityMismatch,
        )?;
        if kind == RetentionOperation::Begin {
            require(
                actor.id().writer_epoch == e.frame.stream.writer_epoch
                    && actor.id().local > e.previous_local,
                RetentionSemanticError::OrderMismatch,
            )?;
        } else {
            require(e.previous_local == 0, RetentionSemanticError::InvalidInput)?;
        }
    }
    next(e.frame.previous_lsn)
}

fn initial_sender(e: RetentionExpected, lsn: u64) -> Result<DescriptorSpec> {
    let status = TransactionStatus::new(
        e.actor.ok_or(RetentionSemanticError::InvalidInput)?,
        TransactionState::Active,
        None,
    )?;
    Ok(DescriptorSpec {
        owner: OwnerState::Transaction(status),
        chain: OwnerChain {
            apply: lsn,
            previous: 0,
            birth: lsn,
            groups: 1,
        },
        profile: retention_obligation::profile_reference(e.profile),
        inventory: InventoryClaim {
            change_lsn: lsn,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        },
        consumed_wal: HANDOFF_BYTES as u64,
        consumed_control: 0,
        remaining: begin_remaining(),
        minimum_wal_lsn: lsn,
        last_mutation_sequence: 0,
    })
}

fn terminal_sender(
    e: RetentionExpected,
    before: &[u8; 1536],
    old: TransactionStatus,
    lsn: u64,
) -> Result<DescriptorSpec> {
    let actor = e.actor.ok_or(RetentionSemanticError::InvalidInput)?;
    let mut spec = obligation::decode_descriptor(
        before,
        ReservationOwner::Transaction(actor),
        retention_obligation::profile_reference(e.profile),
    )?;
    require(
        spec.owner == OwnerState::Transaction(old)
            && old.state() == TransactionState::Aborting(RollbackState::Transaction { next: None })
            && old.undo_head().is_none(),
        RetentionSemanticError::UnsupportedOperation,
    )?;
    require(
        spec.chain.previous == spec.chain.birth,
        RetentionSemanticError::OrderMismatch,
    )?;
    let mut remaining = [RemainingBudget::default(); 5];
    remaining[2] = row(HANDOFF_BYTES);
    require(
        spec.remaining == remaining
            && spec.chain.groups == 2
            && spec.chain.apply <= e.frame.previous_lsn
            && spec.inventory.change_lsn == spec.chain.birth
            && spec.inventory.items == 0
            && spec.inventory.growth_extents == 0
            && spec.inventory.digest == content_digest(&[])
            && spec.minimum_wal_lsn == spec.chain.birth
            && spec.last_mutation_sequence == 0
            && spec.consumed_control == 0
            && spec.consumed_wal == (HANDOFF_BYTES + ROLLBACK_BYTES) as u64,
        RetentionSemanticError::BudgetMismatch,
    )?;
    spec.owner = OwnerState::Transaction(TransactionStatus::new(
        actor,
        TransactionState::Aborted,
        None,
    )?);
    spec.chain.previous = spec.chain.apply;
    spec.chain.apply = lsn;
    spec.chain.groups = 3;
    spec.remaining = [RemainingBudget::default(); 5];
    spec.consumed_wal = spec
        .consumed_wal
        .checked_add(HANDOFF_BYTES as u64)
        .ok_or(RetentionSemanticError::BudgetMismatch)?;
    Ok(spec)
}

fn next_aging(
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    root: RootSpec,
    lsn: u64,
    kind: RetentionOperation,
) -> Result<DescriptorSpec> {
    let reference = root.aging.ok_or(RetentionSemanticError::IdentityMismatch)?;
    let mut spec = if let Some(before) = b.aging {
        retention_obligation::decode(before, e.profile, b.root)?.descriptor()
    } else {
        DescriptorSpec {
            owner: OwnerState::System(SystemProgress {
                reference,
                phase: WorkPhase::Running,
                sequence: root.revision,
                cursor: root.cursor,
                limit: 64,
            }),
            chain: OwnerChain {
                apply: lsn,
                previous: 0,
                birth: lsn,
                groups: 1,
            },
            profile: retention_obligation::profile_reference(e.profile),
            inventory: InventoryClaim {
                change_lsn: lsn,
                items: 0,
                digest: content_digest(&[]),
                growth_extents: 0,
            },
            consumed_wal: AGING_START_BYTES as u64,
            consumed_control: 0,
            remaining: [RemainingBudget::default(); 5],
            minimum_wal_lsn: lsn,
            last_mutation_sequence: 0,
        }
    };
    if kind != RetentionOperation::AgingStart {
        spec.chain.previous = spec.chain.apply;
        spec.chain.apply = lsn;
        spec.chain.groups = next(spec.chain.groups)?;
        if kind == RetentionOperation::Finish {
            spec.consumed_wal = spec
                .consumed_wal
                .checked_add(FINISH_BYTES as u64)
                .ok_or(RetentionSemanticError::BudgetMismatch)?;
        }
    }
    spec.owner = OwnerState::System(SystemProgress {
        reference,
        phase: WorkPhase::Running,
        sequence: root.revision,
        cursor: root.cursor,
        limit: 64,
    });
    spec.remaining =
        retention_page::cleanup_rows((root.leased | root.ready).count_ones() as usize)?;
    Ok(spec)
}

fn compute(
    kind: RetentionOperation,
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    a: &mut RetentionAfter<'_>,
    descriptors: &mut [u8; 3072],
) -> Result<Computed> {
    let lsn = validate_expected(kind, e, b, a)?;
    let ns = e.profile.spec().namespace;
    let (root_header, root_view) = retention_page::decode_root_page(b.root, ns)?;
    retention_page::verify_census(root_view, ns, b.job_cells)?;
    require(
        root_header.lsn.0 <= e.frame.previous_lsn,
        RetentionSemanticError::OrderMismatch,
    )?;
    let old_root = root_view.spec();
    let mut root = old_root;
    let mut operation = [0; 256];
    let mut previous_txn_lsn = 0;
    if kind == RetentionOperation::AgingStart {
        require(
            root.aging.is_none(),
            RetentionSemanticError::UnsupportedOperation,
        )?;
        root.aging = Some(
            SystemWorkRef::new(
                ns,
                SystemWorkKind::Aging,
                1,
                Generation::new(1).expect("one"),
            )
            .map_err(|_| RetentionSemanticError::IdentityMismatch)?,
        );
    } else {
        require(
            root.aging.is_some(),
            RetentionSemanticError::UnsupportedOperation,
        )?;
        let job = e.job.ok_or(RetentionSemanticError::InvalidInput)?;
        let ordinal = job.ordinal()?;
        let index = (job.page().page_no - 2) as u8;
        let before_jobs = b.jobs.ok_or(RetentionSemanticError::InvalidInput)?;
        let jobs = retention_page::decode_jobs_page(before_jobs, ns, index)?;
        require(
            jobs.header().lsn.0 <= e.frame.previous_lsn,
            RetentionSemanticError::OrderMismatch,
        )?;
        for slot in 0..jobs.len() {
            require(
                jobs.cell(slot) == Some(b.job_cells[usize::from(index) * 15 + slot].as_slice()),
                RetentionSemanticError::IdentityMismatch,
            )?;
        }
        let old_job = retention_page::decode_job(&b.job_cells[ordinal], ns)?.spec();
        let actor = e.actor.ok_or(RetentionSemanticError::InvalidInput)?;
        let location = e.status.ok_or(RetentionSemanticError::InvalidInput)?;
        let identity = e.status_page.ok_or(RetentionSemanticError::InvalidInput)?;
        let homes = status_home::decode_status_home_page(
            b.status.ok_or(RetentionSemanticError::InvalidInput)?,
            identity,
            ns,
        )?;
        require(
            homes.len() == 1 && homes.lsn() != 0 && homes.lsn() <= e.frame.previous_lsn,
            RetentionSemanticError::OrderMismatch,
        )?;
        let home = homes.cell(0)?;
        require(
            home.incoming() == 0,
            RetentionSemanticError::UnsupportedOperation,
        )?;
        let bit = 1u64 << ordinal;
        let (new_job, new_status) = match kind {
            RetentionOperation::Begin => {
                let TransactionStatusCell::Free(generation) = home.core() else {
                    return Err(RetentionSemanticError::UnsupportedOperation);
                };
                require(
                    old_job.state == JobState::Free
                        && next(old_job.generation.get())? == job.generation().get()
                        && next(generation.get())? == actor.generation().get()
                        && root.leased.count_ones() < 32,
                    RetentionSemanticError::OrderMismatch,
                )?;
                for cell in b.job_cells {
                    let state = retention_page::decode_job(cell, ns)?.spec().state;
                    if let JobState::Leased { source, status, .. }
                    | JobState::Ready { source, status, .. } = state
                    {
                        require(
                            source.id() != actor.id()
                                && (status.page().file != location.page().file
                                    || status.page().page_no != location.page().page_no),
                            RetentionSemanticError::IdentityMismatch,
                        )?;
                    }
                }
                root.free &= !bit;
                root.leased |= bit;
                root.issued = next(root.issued)?;
                let sender = initial_sender(e, lsn)?;
                obligation::encode_descriptor(&sender, &mut descriptors[..1536])?;
                (
                    JobSpec {
                        generation: job.generation(),
                        state: JobState::Leased {
                            source: actor,
                            status: location,
                            lease_lsn: lsn,
                        },
                    },
                    TransactionStatusCell::Occupied(TransactionStatus::new(
                        actor,
                        TransactionState::Active,
                        None,
                    )?),
                )
            }
            RetentionOperation::AbortEnd => {
                let JobState::Leased {
                    source,
                    status,
                    lease_lsn,
                } = old_job.state
                else {
                    return Err(RetentionSemanticError::UnsupportedOperation);
                };
                require(
                    source == actor && status == location && old_job.generation == job.generation(),
                    RetentionSemanticError::IdentityMismatch,
                )?;
                let TransactionStatusCell::Occupied(old_status) = home.core() else {
                    return Err(RetentionSemanticError::UnsupportedOperation);
                };
                let before = b.sender.ok_or(RetentionSemanticError::InvalidInput)?;
                let sender = terminal_sender(e, before, old_status, lsn)?;
                require(
                    sender.chain.birth == lease_lsn && homes.lsn() >= sender.chain.previous,
                    RetentionSemanticError::OrderMismatch,
                )?;
                previous_txn_lsn = sender.chain.previous;
                obligation::encode_descriptor(&sender, &mut descriptors[..1536])?;
                root.leased &= !bit;
                root.ready |= bit;
                (
                    JobSpec {
                        generation: job.generation(),
                        state: JobState::Ready {
                            source: actor,
                            status: location,
                            lease_lsn,
                            handoff_lsn: lsn,
                            sender_digest: content_digest(before),
                        },
                    },
                    TransactionStatusCell::Occupied(TransactionStatus::new(
                        actor,
                        TransactionState::Aborted,
                        None,
                    )?),
                )
            }
            RetentionOperation::Finish => {
                let JobState::Ready {
                    source,
                    status,
                    handoff_lsn,
                    ..
                } = old_job.state
                else {
                    return Err(RetentionSemanticError::UnsupportedOperation);
                };
                require(
                    source == actor && status == location && old_job.generation == job.generation(),
                    RetentionSemanticError::IdentityMismatch,
                )?;
                require(
                    homes.lsn() >= handoff_lsn,
                    RetentionSemanticError::OrderMismatch,
                )?;
                let expected = TransactionStatus::new(actor, TransactionState::Aborted, None)?;
                require(
                    home.core() == TransactionStatusCell::Occupied(expected),
                    RetentionSemanticError::IdentityMismatch,
                )?;
                root.ready &= !bit;
                root.free |= bit;
                root.completed = next(root.completed)?;
                root.cursor = ((ordinal + 1) % 64) as u64;
                (
                    JobSpec {
                        generation: job.generation(),
                        state: JobState::Free,
                    },
                    TransactionStatusCell::Free(actor.generation()),
                )
            }
            RetentionOperation::AgingStart => unreachable!("separate root-only branch"),
        };
        let mut job_bytes = [0; JOB_BYTES];
        retention_page::encode_job(new_job, ns, &mut job_bytes)?;
        let mut hash = ContentHasher::new();
        for (at, cell) in b.job_cells.iter().enumerate() {
            hash.update(if at == ordinal { &job_bytes } else { cell });
        }
        root.job_digest = hash.digest();
        let mut cells: [&[u8]; 15] = [&[]; 15];
        for (slot, cell) in cells.iter_mut().take(jobs.len()).enumerate() {
            *cell = if slot == usize::from(job.slot()) {
                &job_bytes
            } else {
                jobs.cell(slot).ok_or(RetentionSemanticError::Corrupt)?
            };
        }
        retention_page::encode_jobs_page(
            ns,
            index,
            lsn,
            &cells[..jobs.len()],
            a.jobs
                .as_deref_mut()
                .ok_or(RetentionSemanticError::InvalidInput)?,
        )?;
        let mut status_bytes = [0; 224];
        StatusHome::new(new_status, 0)?.encode(&mut status_bytes)?;
        page::encode_page(
            PageHeader {
                identity,
                kind: PageKind::TransactionStatus,
                level: 0,
                lsn: PageLsn(lsn),
            },
            &[&status_bytes],
            a.status
                .as_deref_mut()
                .ok_or(RetentionSemanticError::InvalidInput)?,
        )?;
    }
    root.revision = next(root.revision)?;
    root.apply = lsn;
    retention_page::encode_root_page(root, ns, a.root)?;
    let aging = next_aging(e, b, root, lsn, kind)?;
    let at = if kind.handoff() { 1536 } else { 0 };
    retention_obligation::encode(&aging, e.profile, a.root, &mut descriptors[at..at + 1536])?;
    if kind.handoff() {
        operation[..4].copy_from_slice(b"XHO1");
        put16(&mut operation, 4, 1);
        put16(&mut operation, 6, kind.code());
        put32(&mut operation, 8, 256);
        e.job
            .ok_or(RetentionSemanticError::InvalidInput)?
            .encode(&mut operation[24..64])?;
        e.status
            .ok_or(RetentionSemanticError::InvalidInput)?
            .encode(&mut operation[64..104])?;
        if kind == RetentionOperation::AbortEnd {
            operation[208..240].copy_from_slice(&retention_page::budget_digest(
                &[RemainingBudget::default(); 5],
            ));
        }
    } else {
        operation[..4].copy_from_slice(b"XRO1");
        put16(&mut operation, 4, 1);
        put16(&mut operation, 6, kind.code());
        put32(&mut operation, 8, 256);
        put64(&mut operation, 16, old_root.revision);
        put64(&mut operation, 24, root.revision);
        if kind == RetentionOperation::Finish {
            e.job
                .ok_or(RetentionSemanticError::InvalidInput)?
                .encode(&mut operation[32..72])?;
            e.status
                .ok_or(RetentionSemanticError::InvalidInput)?
                .encode(&mut operation[72..112])?;
            e.actor
                .ok_or(RetentionSemanticError::InvalidInput)?
                .encode(&mut operation[112..160])?;
        }
        operation[160..192].copy_from_slice(&content_digest(root_view.bytes()));
        operation[192..224].copy_from_slice(&old_root.job_digest);
        put64(&mut operation, 224, kind.count() as u64);
    }
    Ok(Computed {
        header: WalFrameHeader {
            stream: e.frame.stream,
            kind: match kind {
                RetentionOperation::Begin => FrameKind::Begin,
                RetentionOperation::AbortEnd => FrameKind::AbortEnd,
                _ => FrameKind::System,
            },
            lsn,
            previous_lsn: e.frame.previous_lsn,
            txn: if kind.handoff() {
                Some(e.actor.ok_or(RetentionSemanticError::InvalidInput)?.id())
            } else {
                None
            },
            previous_txn_lsn,
        },
        operation,
    })
}

fn owner_bytes(owner: ReservationOwner, output: &mut [u8]) -> Result<()> {
    match owner {
        ReservationOwner::Transaction(actor) => {
            output[0] = 1;
            actor.encode(&mut output[8..56])?;
        }
        ReservationOwner::System(actor) => {
            output[0] = 2;
            actor
                .encode(&mut output[8..56])
                .map_err(|_| RetentionSemanticError::IdentityMismatch)?;
        }
    }
    Ok(())
}
fn group_header(
    kind: RetentionOperation,
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    computed: &Computed,
    output: &mut [u8; 256],
) -> Result<()> {
    output.fill(0);
    let ns = e.profile.spec().namespace;
    let aging = SystemWorkRef::new(
        ns,
        SystemWorkKind::Aging,
        1,
        Generation::new(1).expect("one"),
    )
    .map_err(|_| RetentionSemanticError::IdentityMismatch)?;
    output[..4].copy_from_slice(if kind.handoff() { b"XHG1" } else { b"XSG1" });
    put16(output, 4, 1);
    put16(output, 6, kind.code());
    put16(output, 8, kind.header_bytes() as u16);
    put32(output, 12, (kind.length() - 192) as u32);
    put64(output, 16, computed.header.lsn);
    let prior_aging = if let Some(raw) = b.aging {
        retention_page::get(raw, 72)
    } else {
        0
    };
    if kind.handoff() {
        put64(output, 24, computed.header.previous_txn_lsn);
        put64(output, 32, prior_aging);
        owner_bytes(
            ReservationOwner::Transaction(e.actor.ok_or(RetentionSemanticError::InvalidInput)?),
            &mut output[40..96],
        )?;
        owner_bytes(ReservationOwner::System(aging), &mut output[96..152])?;
        if let Some(raw) = b.sender {
            output[152..184].copy_from_slice(&content_digest(raw));
        }
        output[184..216].copy_from_slice(&content_digest(
            b.aging.ok_or(RetentionSemanticError::InvalidInput)?,
        ));
        for (at, value) in [
            (216, 1536),
            (220, 1536),
            (224, 256),
            (228, 3904),
            (240, 49032),
        ] {
            put32(output, at, value);
        }
        put16(output, 236, 3);
        put16(output, 238, 3);
    } else {
        put64(output, 24, prior_aging);
        owner_bytes(ReservationOwner::System(aging), &mut output[32..88])?;
        if let Some(raw) = b.aging {
            output[88..120].copy_from_slice(&content_digest(raw));
        }
        put32(output, 120, 1536);
        put16(output, 124, kind.count() as u16);
        put16(output, 132, kind.count() as u16);
        put32(output, 136, kind.native_bytes() as u32);
        put32(output, 152, (kind.count() * REDO_BYTES) as u32);
    }
    Ok(())
}

struct Pair<'a> {
    before: &'a [u8; PAGE_BYTES],
    after: &'a [u8; PAGE_BYTES],
    header: PageHeader,
    slot: u16,
    offset: usize,
    body: usize,
    native: u8,
}
fn pair<'a>(
    index: usize,
    e: RetentionExpected,
    b: RetentionBefore<'a>,
    a: AfterView<'a>,
) -> Result<Pair<'a>> {
    let ns = e.profile.spec().namespace;
    let (before, after, slot, offset, body, native, kind) = match index {
        0 => (b.root, a.root, 0, 15840, 512, 8, PageKind::RetentionRoot),
        1 => {
            let job = e.job.ok_or(RetentionSemanticError::InvalidInput)?;
            (
                b.jobs.ok_or(RetentionSemanticError::InvalidInput)?,
                a.jobs.ok_or(RetentionSemanticError::InvalidInput)?,
                job.slot(),
                16352 - 1024 * (usize::from(job.slot()) + 1),
                1024,
                9,
                PageKind::RetentionJobs,
            )
        }
        2 => (
            b.status.ok_or(RetentionSemanticError::InvalidInput)?,
            a.status.ok_or(RetentionSemanticError::InvalidInput)?,
            0,
            16128,
            224,
            5,
            PageKind::TransactionStatus,
        ),
        _ => return Err(RetentionSemanticError::InvalidInput),
    };
    let identity = if index == 2 {
        e.status_page.ok_or(RetentionSemanticError::InvalidInput)?
    } else {
        super::retention_genesis::expected_header(
            ns,
            if index == 0 {
                super::retention_genesis::RetentionGenesisPage::Root
            } else {
                super::retention_genesis::RetentionGenesisPage::Jobs {
                    index: (e
                        .job
                        .ok_or(RetentionSemanticError::InvalidInput)?
                        .page()
                        .page_no
                        - 2) as u8,
                }
            },
        )
        .map_err(|_| RetentionSemanticError::IdentityMismatch)?
        .identity
    };
    Ok(Pair {
        before,
        after,
        header: PageHeader {
            identity,
            kind,
            level: 0,
            lsn: PageLsn(retention_page::get(after, 64)),
        },
        slot,
        offset,
        body,
        native,
    })
}
fn native_header(pair: &Pair<'_>, redo_index: u16) -> [u8; 128] {
    let mut header = [0; 128];
    header[0] = pair.native;
    header[1] = 3;
    put16(&mut header, 2, redo_index);
    put16(&mut header, 4, pair.slot);
    put16(&mut header, 6, pair.slot);
    put16(&mut header, 8, pair.offset as u16);
    put16(&mut header, 10, pair.offset as u16);
    put32(&mut header, 12, pair.body as u32);
    put32(&mut header, 16, pair.body as u32);
    put32(&mut header, 20, (128 + 2 * pair.body) as u32);
    header[24..56].copy_from_slice(&content_digest(
        &pair.before[pair.offset..pair.offset + pair.body],
    ));
    header[56..88].copy_from_slice(&content_digest(
        &pair.after[pair.offset..pair.offset + pair.body],
    ));
    header
}
fn target(pair: &Pair<'_>) -> RedoTarget {
    RedoTarget {
        page: pair.header.identity.page,
        owner: pair.header.identity.owner,
        owner_generation: pair.header.identity.owner_generation,
        kind: pair.header.kind,
        level: 0,
    }
}

fn produce<'a>(
    kind: RetentionOperation,
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    mut a: RetentionAfter<'_>,
    output: &'a mut [u8],
) -> Result<RetentionObservation<'a>> {
    require(
        output.len() == kind.length(),
        RetentionSemanticError::InvalidInput,
    )?;
    let mut descriptors = [0; 3072];
    let computed = compute(kind, e, b, &mut a, &mut descriptors)?;
    let mut group = [0; 256];
    group_header(kind, e, b, &computed, &mut group)?;
    output.fill(0);
    output[160..160 + kind.header_bytes()].copy_from_slice(&group[..kind.header_bytes()]);
    let d = 160 + kind.header_bytes();
    output[d..d + kind.descriptor_bytes()].copy_from_slice(&descriptors[..kind.descriptor_bytes()]);
    output[kind.operation_at()..kind.native_at()].copy_from_slice(&computed.operation);
    let mut at = kind.native_at();
    for index in 0..kind.count() {
        let pair = pair(index, e, b, a.view())?;
        output[at..at + 128].copy_from_slice(&native_header(&pair, index as u16));
        output[at + 128..at + 128 + pair.body]
            .copy_from_slice(&pair.before[pair.offset..pair.offset + pair.body]);
        output[at + 128 + pair.body..at + 128 + 2 * pair.body]
            .copy_from_slice(&pair.after[pair.offset..pair.offset + pair.body]);
        at += 128 + 2 * pair.body;
        let r = kind.redo_at() + index * REDO_BYTES;
        wal_frame::encode_patch_record(
            target(&pair),
            retention_page::get(pair.before, 64),
            SPAN_START as u16,
            &pair.after[SPAN_START..SPAN_END],
            &mut output[r..r + REDO_BYTES],
        )?;
    }
    if kind.handoff() {
        wal_frame::finish_retention_envelope(computed.header, kind.count() as u16, output)?;
    } else {
        wal_frame::finish_semantic_envelope(computed.header, kind.count() as u16, output)?;
    }
    Ok(RetentionObservation {
        operation: kind,
        header: computed.header,
        frame: output,
    })
}

fn observe<'a>(
    kind: RetentionOperation,
    input: &'a [u8],
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    mut a: RetentionAfter<'_>,
) -> Result<RetentionObservation<'a>> {
    require(
        input.len() == kind.length(),
        RetentionSemanticError::Corrupt,
    )?;
    let envelope = if kind.handoff() {
        wal_frame::decode_retention_envelope(input, e.frame)?
    } else {
        wal_frame::decode_semantic_envelope(input, e.frame)?
    };
    let mut descriptors = [0; 3072];
    let computed = compute(kind, e, b, &mut a, &mut descriptors)?;
    require(
        envelope.header == computed.header && envelope.record_count == kind.count() as u16,
        RetentionSemanticError::IdentityMismatch,
    )?;
    let mut group = [0; 256];
    group_header(kind, e, b, &computed, &mut group)?;
    let d = 160 + kind.header_bytes();
    require(
        input[160..d] == group[..kind.header_bytes()]
            && input[d..d + kind.descriptor_bytes()] == descriptors[..kind.descriptor_bytes()]
            && input[kind.operation_at()..kind.native_at()] == computed.operation,
        RetentionSemanticError::Corrupt,
    )?;
    let mut at = kind.native_at();
    for index in 0..kind.count() {
        let pair = pair(index, e, b, a.view())?;
        require(
            input[at..at + 128] == native_header(&pair, index as u16)
                && input[at + 128..at + 128 + pair.body]
                    == pair.before[pair.offset..pair.offset + pair.body]
                && input[at + 128 + pair.body..at + 128 + 2 * pair.body]
                    == pair.after[pair.offset..pair.offset + pair.body],
            RetentionSemanticError::Corrupt,
        )?;
        at += 128 + 2 * pair.body;
        let r = kind.redo_at() + index * REDO_BYTES;
        let redo = wal_frame::decode_redo_record(computed.header, &input[r..r + REDO_BYTES])?;
        require(
            redo.kind == RedoKind::Patch
                && redo.target == target(&pair)
                && redo.prior_lsn == retention_page::get(pair.before, 64)
                && usize::from(redo.offset) == SPAN_START
                && redo.data == &pair.after[SPAN_START..SPAN_END],
            RetentionSemanticError::Corrupt,
        )?;
    }
    Ok(RetentionObservation {
        operation: kind,
        header: computed.header,
        frame: input,
    })
}

macro_rules! entries {
    ($produce:ident,$observe:ident,$kind:ident) => {
        pub(crate) fn $produce<'a>(
            expected: RetentionExpected,
            before: RetentionBefore<'_>,
            after: RetentionAfter<'_>,
            output: &'a mut [u8],
        ) -> Result<RetentionObservation<'a>> {
            produce(RetentionOperation::$kind, expected, before, after, output)
        }
        pub(crate) fn $observe<'a>(
            input: &'a [u8],
            expected: RetentionExpected,
            before: RetentionBefore<'_>,
            after: RetentionAfter<'_>,
        ) -> Result<RetentionObservation<'a>> {
            observe(RetentionOperation::$kind, input, expected, before, after)
        }
    };
}
entries!(produce_aging_start, observe_aging_start, AgingStart);
entries!(produce_begin, observe_begin, Begin);
entries!(produce_abort_end, observe_abort_end, AbortEnd);
entries!(produce_finish, observe_finish, Finish);

/// Source values for the new semantic phase only. Caller maintenance/image/frame
/// loans are separate; the live owner adds all retained state and active pool.
pub(crate) fn source_bytes() -> usize {
    use std::mem::size_of;
    3072 + 2 * 256
        + 1024
        + 224
        + 128
        + 15 * size_of::<&[u8]>()
        + 4 * size_of::<DescriptorSpec>()
        + 4 * size_of::<Result<DescriptorSpec>>()
        + 4 * size_of::<RetentionExpected>()
        + 4 * size_of::<RetentionBefore<'_>>()
        + 3 * size_of::<RetentionAfter<'_>>()
        + 4 * size_of::<RootSpec>()
        + 4 * size_of::<JobSpec>()
        + 4 * size_of::<Computed>()
        + 4 * size_of::<Pair<'_>>()
        + 4 * size_of::<Result<RetentionObservation<'_>>>()
        + 4 * size_of::<ContentHasher>()
        + retention_obligation::source_bytes()
        + 1024
}

#[cfg(test)]
#[path = "semantic_retention_tests.rs"]
mod tests;
