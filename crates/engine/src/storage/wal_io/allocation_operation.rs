//! Private producer and complete before/after validation for one allocator owner.
//! A successful envelope alone cannot construct PreparedAllocation.
use super::wal;
use crate::storage::{
    allocation_page::{self, AllocationEntry, AllocationPage, ReservationOwner, SystemWorkRef},
    control::{ControlKind, ControlRef, Profile},
    obligation::{
        self, DescriptorSpec, InventoryClaim, OwnerChain, OwnerState, RemainingBudget,
        ResourcePeak, SystemProgress, WorkPhase,
    },
    page::{PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
    wal_frame::{
        self, FrameExpectation, FrameKind, RedoKind, RedoTarget, WalError, WalFrameHeader,
    },
};
use foundation::{
    codec::{ContentHasher, content_digest},
    identity::Generation,
    namespace::WriterNamespace,
};

const CELL: std::ops::Range<usize> = 10144..16352;
const OLD_CELL: std::ops::Range<usize> = 2272..8480;
const NEW_CELL: std::ops::Range<usize> = 8480..14688;
const DELTAS: usize = 14688;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticError {
    Corrupt,
    UnsupportedOperation,
    UnsupportedNativeBody,
    IdentityMismatch,
    OrderMismatch,
    StateMismatch,
    PageMismatch,
    InventoryMismatch,
    BudgetMismatch,
}
type Result<T> = std::result::Result<T, SemanticError>;

/// Trusted caller workspace contract, never selected from serialized bytes.
/// The dirty consumer keeps a separate before image while producing its frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum OperationBudget {
    Exclusive81920,
    Dirty98304,
    StatusInit114688,
    MixedInit114688,
}
impl OperationBudget {
    pub(in super::super) const fn memory(self) -> u64 {
        match self {
            Self::Exclusive81920 => 81920,
            Self::Dirty98304 => 98304,
            Self::StatusInit114688 | Self::MixedInit114688 => 114688,
        }
    }

    pub(in super::super) fn accepts_count(self, count: usize) -> bool {
        (1..=63).contains(&count)
            && (!matches!(self, Self::StatusInit114688 | Self::MixedInit114688) || count == 1)
    }

    pub(in super::super) fn terminal_wal(self, count: usize) -> u64 {
        match self {
            Self::StatusInit114688 => 38000,
            Self::MixedInit114688 => 38208,
            _ => frame_length(count) as u64,
        }
    }

    pub(in super::super) const fn terminal_frames(self) -> u64 {
        if matches!(self, Self::StatusInit114688 | Self::MixedInit114688) {
            2
        } else {
            1
        }
    }

    pub(in super::super) fn terminal_protected(self, count: usize) -> u64 {
        self.terminal_wal(count)
            + if matches!(self, Self::StatusInit114688 | Self::MixedInit114688) {
                2
            } else {
                count as u64
            } * PAGE_BYTES as u64
    }
}

/// Trusted owner scope, never inferred from a descriptor or its budget.
/// Status means the immutable plan2 role (object2, generation1, Status pages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum AllocationPopulation {
    Empty,
    Status,
    MixedSystem,
}
impl AllocationPopulation {
    fn accepts_budget(self, budget: OperationBudget) -> bool {
        match self {
            Self::Empty => budget != OperationBudget::MixedInit114688,
            Self::Status => budget == OperationBudget::StatusInit114688,
            Self::MixedSystem => budget == OperationBudget::MixedInit114688,
        }
    }
}

/// With no active batch, identifies the terminal descriptor retained by the
/// live owner. A successful reserve resets it to Cancelled; active-batch
/// validation uses the exact Reserved descriptor independently of this tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum TerminalOutcome {
    Cancelled,
    InitializedStatus,
    InitializedMixedStatus,
    InitializedUndoRoot,
}

/// Closed payload class for the fixed plan3 roles, never a wire discriminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum MixedInitKind {
    Status,
    UndoRoot,
}
impl MixedInitKind {
    pub(in super::super) fn terminal(self) -> TerminalOutcome {
        match self {
            Self::Status => TerminalOutcome::InitializedMixedStatus,
            Self::UndoRoot => TerminalOutcome::InitializedUndoRoot,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Batch {
    pub(in super::super) owner: SystemWorkRef,
    pub(in super::super) mask: u64,
    pub(in super::super) birth: u64,
}
impl Batch {
    pub(in super::super) fn count(self) -> usize {
        self.mask.count_ones() as usize
    }
}
#[derive(Clone, Copy)]
pub(in super::super) struct Plan<'a> {
    pub(in super::super) profile: Profile,
    pub(in super::super) identity: PageIdentity,
    pub(in super::super) prefix: wal::WalPrefix,
    pub(in super::super) batch: Batch,
    pub(in super::super) cancel: bool,
    pub(in super::super) before_descriptor: Option<&'a [u8]>,
}
/// Pure semantic context. Its before image is either the live fixed page or
/// a canonical page derived from authenticated genesis and prior accepted WAL.
/// No synced-prefix, session, stage or physical-home authority is represented.
#[derive(Clone, Copy)]
pub(in super::super) struct AnalysisExpected<'a> {
    pub(in super::super) profile: Profile,
    pub(in super::super) identity: PageIdentity,
    pub(in super::super) frame: FrameExpectation,
    pub(in super::super) batch: Batch,
    pub(in super::super) cancel: bool,
    pub(in super::super) before_descriptor: Option<&'a [u8]>,
}
impl<'a> From<Plan<'a>> for AnalysisExpected<'a> {
    fn from(plan: Plan<'a>) -> Self {
        Self {
            profile: plan.profile,
            identity: plan.identity,
            frame: FrameExpectation {
                stream: plan.prefix.stream,
                previous_lsn: plan.prefix.complete_lsn,
            },
            batch: plan.batch,
            cancel: plan.cancel,
            before_descriptor: plan.before_descriptor,
        }
    }
}
fn analysis_header(expected: &AnalysisExpected<'_>) -> Result<WalFrameHeader> {
    Ok(WalFrameHeader {
        stream: expected.frame.stream,
        kind: FrameKind::System,
        lsn: expected
            .frame
            .previous_lsn
            .checked_add(1)
            .ok_or(SemanticError::OrderMismatch)?,
        previous_lsn: expected.frame.previous_lsn,
        txn: None,
        previous_txn_lsn: 0,
    })
}

pub(in super::super) fn frame_length(n: usize) -> usize {
    20992 + 208 * n
}
fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn get16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().expect("bounded u16"))
}
fn get32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().expect("bounded u32"))
}
fn get64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().expect("bounded u64"))
}
fn zero(b: &[u8]) -> bool {
    b.iter().all(|b| *b == 0)
}
fn require(ok: bool, error: SemanticError) -> Result<()> {
    if ok { Ok(()) } else { Err(error) }
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
fn descriptor_error(error: obligation::ObligationError) -> SemanticError {
    match error {
        obligation::ObligationError::IdentityMismatch => SemanticError::IdentityMismatch,
        _ => SemanticError::Corrupt,
    }
}
fn frame_error(error: WalError) -> SemanticError {
    match error {
        WalError::IdentityMismatch => SemanticError::IdentityMismatch,
        WalError::OrderMismatch => SemanticError::OrderMismatch,
        _ => SemanticError::Corrupt,
    }
}
fn header(plan: Plan<'_>) -> Result<WalFrameHeader> {
    let lsn = plan
        .prefix
        .complete_lsn
        .checked_add(1)
        .ok_or(SemanticError::OrderMismatch)?;
    Ok(WalFrameHeader {
        stream: plan.prefix.stream,
        kind: FrameKind::System,
        lsn,
        previous_lsn: plan.prefix.complete_lsn,
        txn: None,
        previous_txn_lsn: 0,
    })
}
fn item(reference: PageRef, lsn: u64) -> Result<[u8; 96]> {
    let mut out = [0; 96];
    out[0] = 1;
    out[1] = 3;
    reference
        .encode(&mut out[8..32])
        .map_err(|_| SemanticError::InventoryMismatch)?;
    put64(&mut out, 64, 1);
    put64(&mut out, 72, lsn);
    Ok(out)
}
fn item_for(
    page: &AllocationPage<'_>,
    batch: Batch,
    index: usize,
    cancel: bool,
) -> Result<[u8; 96]> {
    let generation = match page.entry(index).map_err(|_| SemanticError::PageMismatch)? {
        AllocationEntry::Free(g) if !cancel => {
            Generation::new(g.get().checked_add(1).ok_or(SemanticError::PageMismatch)?)
                .map_err(|_| SemanticError::PageMismatch)?
        }
        AllocationEntry::Reserved {
            generation,
            claimant,
        } if cancel && claimant == ReservationOwner::System(batch.owner) => generation,
        _ => return Err(SemanticError::PageMismatch),
    };
    item(
        PageRef {
            page_no: index as u64,
            incarnation: generation,
            ..page.header().identity.page
        },
        batch.birth,
    )
}
fn item_digest(page: &AllocationPage<'_>, batch: Batch, cancel: bool) -> Result<[u8; 32]> {
    let mut hash = ContentHasher::new();
    for i in 1..64 {
        if batch.mask & (1u64 << i) != 0 {
            hash.update(&item_for(page, batch, i, cancel)?);
        }
    }
    Ok(hash.digest())
}
fn descriptor(
    profile: Profile,
    batch: Batch,
    lsn: u64,
    cancel: bool,
    digest: [u8; 32],
    budget: OperationBudget,
) -> DescriptorSpec {
    let n = batch.count() as u64;
    let length = frame_length(n as usize) as u64;
    let mut remaining = [RemainingBudget::default(); 5];
    if !cancel {
        remaining[2] = RemainingBudget {
            steps: 1,
            wal: budget.terminal_wal(batch.count()),
            pages: n,
            peak: ResourcePeak {
                frames: budget.terminal_frames(),
                memory: budget.memory(),
                files: 1,
                ..ResourcePeak::default()
            },
            protected: budget.terminal_protected(batch.count()),
            ..RemainingBudget::default()
        };
    }
    DescriptorSpec {
        owner: OwnerState::System(SystemProgress {
            reference: batch.owner,
            phase: if cancel {
                WorkPhase::Finished
            } else {
                WorkPhase::Prepared
            },
            sequence: 1,
            cursor: if cancel { n } else { 0 },
            limit: n,
        }),
        chain: OwnerChain {
            apply: lsn,
            previous: if cancel { batch.birth } else { 0 },
            birth: batch.birth,
            groups: if cancel { 2 } else { 1 },
        },
        profile: profile_ref(profile),
        inventory: InventoryClaim {
            change_lsn: lsn,
            items: if cancel { 0 } else { n },
            digest: if cancel { content_digest(&[]) } else { digest },
            growth_extents: 0,
        },
        consumed_wal: length * if cancel { 2 } else { 1 },
        consumed_control: 0,
        remaining,
        minimum_wal_lsn: if cancel { 0 } else { batch.birth },
        last_mutation_sequence: 0,
    }
}

pub(in super::super) fn validate_live(
    page: &AllocationPage<'_>,
    profile: Profile,
    batch: Option<Batch>,
    raw: &[u8],
    high_water: u64,
) -> Result<()> {
    validate_live_with_budget(
        page,
        profile,
        batch,
        raw,
        high_water,
        OperationBudget::Exclusive81920,
    )
}

pub(in super::super) fn validate_live_with_budget(
    page: &AllocationPage<'_>,
    profile: Profile,
    batch: Option<Batch>,
    raw: &[u8],
    high_water: u64,
    budget: OperationBudget,
) -> Result<()> {
    validate_live_in_population(
        page,
        profile,
        batch,
        raw,
        high_water,
        budget,
        AllocationPopulation::Empty,
        TerminalOutcome::Cancelled,
    )
}

pub(in super::super) fn validate_population(
    page: &AllocationPage<'_>,
    batch: Option<Batch>,
    population: AllocationPopulation,
) -> Result<usize> {
    let counts = population_census(page, batch, population)?;
    Ok(counts[0] + counts[1])
}

// Two bounded scalar counters retain compatible terminal membership while the
// same full 1..63 scan validates every entry, including the last one.
fn population_census(
    page: &AllocationPage<'_>,
    batch: Option<Batch>,
    population: AllocationPopulation,
) -> Result<[usize; 2]> {
    let mut observed_mask = 0;
    let mut allocated = [0; 2];
    for i in 1..64 {
        match page.entry(i).map_err(|_| SemanticError::PageMismatch)? {
            AllocationEntry::Free(_) => (),
            AllocationEntry::Reserved { claimant, .. }
                if batch.is_some_and(|b| claimant == ReservationOwner::System(b.owner)) =>
            {
                observed_mask |= 1u64 << i
            }
            AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind,
            } if generation.get() >= 2
                && object_generation.get() == 1
                && match population {
                    AllocationPopulation::Empty => false,
                    AllocationPopulation::Status => {
                        object.get() == 2 && kind == PageKind::TransactionStatus
                    }
                    AllocationPopulation::MixedSystem => {
                        (object.get() == 2 && kind == PageKind::TransactionStatus)
                            || (object.get() == 3 && kind == PageKind::Undo)
                    }
                } =>
            {
                allocated[usize::from(kind == PageKind::Undo)] += 1;
            }
            _ => return Err(SemanticError::PageMismatch),
        }
    }
    require(
        observed_mask == batch.map_or(0, |b| b.mask),
        SemanticError::InventoryMismatch,
    )?;
    Ok(allocated)
}

#[allow(
    clippy::too_many_arguments,
    reason = "trusted scope and outcome remain independent of serialized budget"
)]
pub(in super::super) fn validate_live_in_population(
    page: &AllocationPage<'_>,
    profile: Profile,
    batch: Option<Batch>,
    raw: &[u8],
    high_water: u64,
    budget: OperationBudget,
    population: AllocationPopulation,
    terminal: TerminalOutcome,
) -> Result<()> {
    require(
        population.accepts_budget(budget),
        SemanticError::StateMismatch,
    )?;
    let members = population_census(page, batch, population)?;
    let allocated = members[0] + members[1];
    if let Some(batch) = batch {
        validate_reserved(page, &profile, &batch, raw, high_water, budget)
    } else if high_water == 0 {
        require(
            zero(raw)
                && page.header().lsn == PageLsn(0)
                && allocated == 0
                && terminal == TerminalOutcome::Cancelled,
            SemanticError::StateMismatch,
        )
    } else {
        require(
            match terminal {
                TerminalOutcome::Cancelled => true,
                TerminalOutcome::InitializedStatus => {
                    population == AllocationPopulation::Status && allocated != 0
                }
                TerminalOutcome::InitializedMixedStatus => {
                    population == AllocationPopulation::MixedSystem && members[0] != 0
                }
                TerminalOutcome::InitializedUndoRoot => {
                    population == AllocationPopulation::MixedSystem && members[1] != 0
                }
            },
            SemanticError::StateMismatch,
        )?;
        validate_finished(page, &profile, raw, high_water, budget, terminal)
    }
}

fn validate_reserved(
    page: &AllocationPage<'_>,
    profile: &Profile,
    batch: &Batch,
    raw: &[u8],
    high_water: u64,
    budget: OperationBudget,
) -> Result<()> {
    require(
        batch.mask & 1 == 0
            && budget.accepts_count(batch.count())
            && batch.owner.id() == high_water,
        SemanticError::StateMismatch,
    )?;
    let current = obligation::decode_descriptor(
        raw,
        ReservationOwner::System(batch.owner),
        profile_ref(*profile),
    )
    .map_err(descriptor_error)?;
    require(
        current
            == descriptor(
                *profile,
                *batch,
                batch.birth,
                false,
                item_digest(page, *batch, true)?,
                budget,
            ),
        SemanticError::StateMismatch,
    )?;
    require(
        page.header().lsn.0 == batch.birth,
        SemanticError::OrderMismatch,
    )
}

fn validate_finished(
    page: &AllocationPage<'_>,
    profile: &Profile,
    raw: &[u8],
    high_water: u64,
    budget: OperationBudget,
    terminal: TerminalOutcome,
) -> Result<()> {
    let owner = SystemWorkRef::new(
        profile.spec().namespace,
        allocation_page::SystemWorkKind::AllocationMaintenance,
        high_water,
        Generation::new(1).expect("nonzero"),
    )
    .map_err(|_| SemanticError::IdentityMismatch)?;
    let current =
        obligation::decode_descriptor(raw, ReservationOwner::System(owner), profile_ref(*profile))
            .map_err(descriptor_error)?;
    let OwnerState::System(progress) = current.owner else {
        return Err(SemanticError::StateMismatch);
    };
    require(
        (1..=63).contains(&progress.limit) && budget.accepts_count(progress.limit as usize),
        SemanticError::StateMismatch,
    )?;
    // Only cardinality enters Finished; no token remains in this descriptor.
    let mask = u64::MAX >> (63 - progress.limit) & !1;
    let batch = Batch {
        owner,
        mask,
        birth: page
            .header()
            .lsn
            .0
            .checked_sub(1)
            .ok_or(SemanticError::OrderMismatch)?,
    };
    let expected = match terminal {
        TerminalOutcome::Cancelled => descriptor(
            *profile,
            batch,
            page.header().lsn.0,
            true,
            content_digest(&[]),
            budget,
        ),
        TerminalOutcome::InitializedStatus => {
            require(progress.limit == 1, SemanticError::StateMismatch)?;
            finished_status_descriptor(profile, batch, page.header().lsn)
        }
        TerminalOutcome::InitializedMixedStatus | TerminalOutcome::InitializedUndoRoot => {
            require(progress.limit == 1, SemanticError::StateMismatch)?;
            finished_mixed_descriptor(
                profile,
                batch,
                page.header().lsn,
                if terminal == TerminalOutcome::InitializedMixedStatus {
                    MixedInitKind::Status
                } else {
                    MixedInitKind::UndoRoot
                },
            )
        }
    };
    require(current == expected, SemanticError::StateMismatch)
}

/// Exact plan4 population followed by the original count-one descriptor
/// algebra. This byte observation does not establish historical INIT/WAL or
/// reservation-restoration authority. Old population dispatch is unchanged.
pub(crate) fn validate_plan4_base_allocation(
    bytes: &[u8; PAGE_BYTES],
    profile: Profile,
    raw: Option<&[u8; 1536]>,
    high_water: u64,
    terminal: u8,
) -> Result<()> {
    validate_plan4_population(bytes, profile, raw, high_water, terminal, false)
}

/// Exact v2 plan4 system population. This observation never issues a role or
/// a restored reservation; the caller supplies the explicitly selected scope.
pub(crate) fn validate_plan4_system_base_allocation(
    bytes: &[u8; PAGE_BYTES],
    profile: Profile,
    raw: Option<&[u8; 1536]>,
    high_water: u64,
    terminal: u8,
) -> Result<()> {
    validate_plan4_population(bytes, profile, raw, high_water, terminal, true)
}

fn validate_plan4_population(
    bytes: &[u8; PAGE_BYTES],
    profile: Profile,
    raw: Option<&[u8; 1536]>,
    high_water: u64,
    terminal: u8,
    system: bool,
) -> Result<()> {
    let budget = if system {
        OperationBudget::MixedInit114688
    } else {
        OperationBudget::StatusInit114688
    };
    let namespace = profile.spec().namespace;
    require(namespace.timeline == 1, SemanticError::IdentityMismatch)?;
    let fixed = super::super::super::fixed_entry().map_err(|_| SemanticError::IdentityMismatch)?;
    let identity = fixed
        .metadata_identity(namespace, 0)
        .map_err(|_| SemanticError::IdentityMismatch)?;
    let page = allocation_page::decode_allocation_page(bytes, identity, namespace)
        .map_err(|_| SemanticError::PageMismatch)?;
    require(page.header().level == 0, SemanticError::PageMismatch)?;
    let mut batch = None;
    let mut statuses = 0;
    let mut undos = 0;
    for ordinal in 0..64 {
        let entry = page
            .entry(ordinal)
            .map_err(|_| SemanticError::PageMismatch)?;
        if ordinal < 9 {
            let (expected_object, expected_kind) = match ordinal {
                0 => (1, PageKind::Allocation),
                1 => (4, PageKind::RetentionRoot),
                2..=6 => (4, PageKind::RetentionJobs),
                7 => (5, PageKind::TreeAnchor),
                8 => (5, PageKind::CatalogLeaf),
                _ => unreachable!("bounded fixed genesis role"),
            };
            require(
                matches!(entry, AllocationEntry::Allocated {
                    generation, object, object_generation, kind,
                } if generation.get() == 1 && object.get() == expected_object
                    && object_generation.get() == 1 && kind == expected_kind),
                SemanticError::PageMismatch,
            )?;
            continue;
        }
        match entry {
            AllocationEntry::Free(generation) => {
                require(
                    high_water != 0 || generation.get() == 1,
                    SemanticError::StateMismatch,
                )?;
            }
            AllocationEntry::Reserved {
                generation,
                claimant: ReservationOwner::System(owner),
            } if generation.get() >= 2
                && owner.namespace() == namespace
                && owner.kind() == allocation_page::SystemWorkKind::AllocationMaintenance
                && owner.id() == high_water
                && owner.generation().get() == 1
                && batch.is_none() =>
            {
                batch = Some(Batch {
                    owner,
                    mask: 1u64 << ordinal,
                    birth: page.header().lsn.0,
                });
            }
            AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind,
            } if generation.get() >= 2
                && object.get() == 2
                && object_generation.get() == 1
                && kind == PageKind::TransactionStatus =>
            {
                statuses += 1;
            }
            AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind,
            } if system
                && generation.get() >= 2
                && object.get() == 3
                && object_generation.get() == 1
                && kind == PageKind::Undo =>
            {
                undos += 1;
            }
            _ => return Err(SemanticError::PageMismatch),
        }
    }
    if let Some(batch) = batch {
        require(
            high_water != 0 && terminal == 0,
            SemanticError::StateMismatch,
        )?;
        validate_reserved(
            &page,
            &profile,
            &batch,
            raw.ok_or(SemanticError::StateMismatch)?,
            high_water,
            budget,
        )
    } else if high_water == 0 {
        require(
            raw.is_none()
                && terminal == 0
                && statuses == 0
                && undos == 0
                && page.header().lsn.0 == 0,
            SemanticError::StateMismatch,
        )
    } else {
        let outcome = match terminal {
            1 => TerminalOutcome::Cancelled,
            2 if statuses != 0 => {
                if system {
                    TerminalOutcome::InitializedMixedStatus
                } else {
                    TerminalOutcome::InitializedStatus
                }
            }
            3 if system && undos != 0 => TerminalOutcome::InitializedUndoRoot,
            _ => return Err(SemanticError::StateMismatch),
        };
        validate_finished(
            &page,
            &profile,
            raw.ok_or(SemanticError::StateMismatch)?,
            high_water,
            budget,
            outcome,
        )
    }
}

/// New-only forwarding/population values plus the unchanged original codec and
/// descriptor bound. No caller page or bank array is copied into this closure.
pub(crate) fn plan4_base_allocation_source_bytes() -> usize {
    use std::mem::size_of;
    super::budget::TEMPORARIES
        + 2 * size_of::<AllocationPage<'_>>()
        + 2 * size_of::<std::result::Result<AllocationPage<'_>, allocation_page::AllocationError>>()
        + 2 * size_of::<Profile>()
        + 2 * size_of::<crate::storage::control::FileMapEntry>()
        + 2 * size_of::<PageIdentity>()
        + 2 * size_of::<Batch>()
        + 2 * size_of::<Option<Batch>>()
        + 2 * size_of::<AllocationEntry>()
        + 2 * size_of::<std::result::Result<AllocationEntry, allocation_page::AllocationError>>()
        + 4 * size_of::<Result<()>>()
        + 24 * size_of::<usize>()
        + 12 * size_of::<&[u8]>()
}

/// Same bounded full-population observer plus its explicit scope/counter values.
pub(crate) fn plan4_system_base_allocation_source_bytes() -> usize {
    plan4_base_allocation_source_bytes()
        + 4 * std::mem::size_of::<OperationBudget>()
        + 8 * std::mem::size_of::<usize>()
}

/// Canonical terminal for reserve-one + Status INIT. Shared by the producer and
/// later live-owner validation; it has no storage, stage or recovery authority.
pub(in super::super) fn finished_status_descriptor(
    profile: &Profile,
    batch: Batch,
    lsn: PageLsn,
) -> DescriptorSpec {
    let mut result = descriptor(
        *profile,
        batch,
        lsn.0,
        true,
        content_digest(&[]),
        OperationBudget::StatusInit114688,
    );
    result.consumed_wal = frame_length(1) as u64 + 38000;
    result
}

/// The mixed reservation protects the larger Undo terminal. A finished owner
/// records only the actual emitted terminal bytes and retains no remainder.
pub(in super::super) fn finished_mixed_descriptor(
    profile: &Profile,
    batch: Batch,
    lsn: PageLsn,
    kind: MixedInitKind,
) -> DescriptorSpec {
    let actual = match kind {
        MixedInitKind::Status => 38000,
        MixedInitKind::UndoRoot => 38208,
    };
    let mut result = descriptor(
        *profile,
        batch,
        lsn.0,
        true,
        content_digest(&[]),
        OperationBudget::MixedInit114688,
    );
    result.consumed_wal = frame_length(1) as u64 + actual;
    result
}

/// The bytes and header are borrowed only after full transition validation.
/// Its fields and constructor are private to this module.
pub(in super::super) struct PreparedAllocation<'a> {
    bytes: &'a [u8],
    prefix: wal::WalPrefix,
    header: WalFrameHeader,
}
impl<'a> PreparedAllocation<'a> {
    pub(in super::super) fn descriptor_bytes(&self) -> &'a [u8] {
        &self.bytes[352..1888]
    }

    pub(in super::super) fn stage<W: WriterNamespace>(
        self,
        wal: &wal::WalRuntime<'_, W>,
    ) -> std::result::Result<wal::WalFrameLocation, wal::WalIoError> {
        wal.stage_verified(self.prefix, self.header, self.bytes)
    }

    pub(in super::super) fn stage_required<'s, W: WriterNamespace>(
        self,
        wal: &'s wal::WalRuntime<'_, W>,
    ) -> std::result::Result<wal::WalRequirement<'s>, wal::WalIoError> {
        wal.stage_verified_required(self.prefix, self.header, self.bytes)
    }
}

pub(in super::super) fn prepare<'a>(
    plan: Plan<'_>,
    before: &[u8; PAGE_BYTES],
    output: &'a mut [u8],
    after: &mut [u8; PAGE_BYTES],
) -> Result<PreparedAllocation<'a>> {
    prepare_with_budget(plan, before, output, after, OperationBudget::Exclusive81920)
}

pub(in super::super) fn prepare_with_budget<'a>(
    plan: Plan<'_>,
    before: &[u8; PAGE_BYTES],
    output: &'a mut [u8],
    after: &mut [u8; PAGE_BYTES],
    budget: OperationBudget,
) -> Result<PreparedAllocation<'a>> {
    prepare_in_population(
        plan,
        before,
        output,
        after,
        budget,
        AllocationPopulation::Empty,
    )
}

pub(in super::super) fn prepare_in_population<'a>(
    plan: Plan<'_>,
    before: &[u8; PAGE_BYTES],
    output: &'a mut [u8],
    after: &mut [u8; PAGE_BYTES],
    budget: OperationBudget,
    population: AllocationPopulation,
) -> Result<PreparedAllocation<'a>> {
    // End the producer's array/descriptor stack frame before full validation.
    // Only this wrapper can turn the independently checked bytes into authority.
    require(
        population.accepts_budget(budget),
        SemanticError::StateMismatch,
    )?;
    let h = encode_prepared(plan, before, output, after, budget)?;
    let raw = &output[..frame_length(plan.batch.count())];
    validate_analysis_in_population(
        AnalysisExpected::from(plan),
        before,
        raw,
        after,
        budget,
        population,
    )?;
    Ok(PreparedAllocation {
        bytes: raw,
        prefix: plan.prefix,
        header: h,
    })
}

// Fixed legacy/plan4 source expansions preserve the old call graph. Only
// plan4 reserve may follow unrelated groups without modifying the allocator.
macro_rules! allocation_before_lsn {
    (legacy, $actual:expr, $previous:expr) => {
        $actual == $previous
    };
    (retention, $actual:expr, $previous:expr) => {
        $actual <= $previous
    };
}
macro_rules! allocation_group_version {
    (legacy) => {
        1
    };
    (retention) => {
        2
    };
}
macro_rules! allocation_group_flags {
    (legacy) => {
        0
    };
    (retention) => {
        1
    };
}
macro_rules! allocation_encoder {
    ($name:ident, $policy:ident) => {
        fn $name(
            plan: Plan<'_>,
            before: &[u8; PAGE_BYTES],
            output: &mut [u8],
            after: &mut [u8; PAGE_BYTES],
            budget: OperationBudget,
        ) -> Result<WalFrameHeader> {
            let h = header(plan)?;
            let n = plan.batch.count();
            require(
                budget.accepts_count(n)
                    && plan.batch.mask & 1 == 0
                    && plan.cancel == plan.before_descriptor.is_some(),
                SemanticError::StateMismatch,
            )?;
            let length = frame_length(n);
            require(output.len() >= length, SemanticError::Corrupt)?;
            let page = allocation_page::decode_allocation_page(
                before,
                plan.identity,
                plan.profile.spec().namespace,
            )
            .map_err(|_| SemanticError::PageMismatch)?;
            require(
                allocation_before_lsn!($policy, page.header().lsn.0, plan.prefix.complete_lsn),
                SemanticError::OrderMismatch,
            )?;
            let raw = &mut output[..length];
            raw.fill(0);
            let g = 160;
            raw[g..g + 4].copy_from_slice(b"XSG1");
            put16(raw, g + 4, allocation_group_version!($policy));
            put16(raw, g + 10, allocation_group_flags!($policy));
            put16(raw, g + 6, if plan.cancel { 10 } else { 8 });
            put16(raw, g + 8, 192);
            put32(raw, g + 12, (length - 192) as u32);
            put64(raw, g + 16, h.lsn);
            put64(raw, g + 24, if plan.cancel { plan.batch.birth } else { 0 });
            raw[g + 32] = 2;
            plan.batch
                .owner
                .encode(&mut raw[g + 40..g + 88])
                .map_err(|_| SemanticError::IdentityMismatch)?;
            if let Some(before) = plan.before_descriptor {
                raw[g + 88..g + 120].copy_from_slice(&content_digest(before));
            }
            put32(raw, g + 120, 1536);
            put16(raw, g + 124, 1);
            put16(raw, g + 130, n as u16);
            put16(raw, g + 132, 1);
            put32(raw, g + 136, 12544);
            put32(raw, g + 148, 208 * n as u32);
            put32(raw, g + 152, 6272);
            put16(raw, 1888, if plan.cancel { 10 } else { 8 });
            put32(raw, 1892, 256);
            put64(raw, 2056, if plan.cancel { n as u64 } else { 0 });
            raw[2144] = 6;
            raw[2145] = 3;
            put16(raw, 2152, 10144);
            put16(raw, 2154, 10144);
            put32(raw, 2156, 6208);
            put32(raw, 2160, 6208);
            put32(raw, 2164, 12544);
            raw[OLD_CELL].copy_from_slice(&before[CELL]);
            {
                let mut entries = [AllocationEntry::Free(Generation::new(1).expect("nonzero")); 64];
                for (i, entry) in entries.iter_mut().enumerate() {
                    *entry = page.entry(i).map_err(|_| SemanticError::PageMismatch)?;
                    if plan.batch.mask & (1u64 << i) != 0 {
                        *entry = changed_entry(*entry, plan.batch, plan.cancel)?;
                    }
                }
                allocation_page::encode_allocation_cell(
                    page.header(),
                    plan.profile.spec().namespace,
                    &entries,
                    &mut raw[NEW_CELL],
                )
                .map_err(|_| SemanticError::PageMismatch)?;
            }
            let old_digest = content_digest(&raw[OLD_CELL]);
            let new_digest = content_digest(&raw[NEW_CELL]);
            raw[2168..2200].copy_from_slice(&old_digest);
            raw[2200..2232].copy_from_slice(&new_digest);
            allocation_page::encode_allocation_page(
                PageHeader {
                    lsn: PageLsn(h.lsn),
                    ..page.header()
                },
                plan.profile.spec().namespace,
                &raw[NEW_CELL],
                after,
            )
            .map_err(|_| SemanticError::PageMismatch)?;
            let mut digest = ContentHasher::new();
            let mut index = 0;
            for i in 1..64 {
                if plan.batch.mask & (1u64 << i) == 0 {
                    continue;
                }
                let it = item_for(&page, plan.batch, i, plan.cancel)?;
                digest.update(&it);
                let d = DELTAS + 208 * index;
                raw[d] = if plan.cancel { 3 } else { 1 };
                raw[d + 1] = if plan.cancel { 1 } else { 2 };
                let start = d + if plan.cancel { 16 } else { 112 };
                raw[start..start + 96].copy_from_slice(&it);
                index += 1;
            }
            let spec = descriptor(
                plan.profile,
                plan.batch,
                h.lsn,
                plan.cancel,
                digest.digest(),
                budget,
            );
            obligation::encode_descriptor(&spec, &mut raw[352..1888]).map_err(descriptor_error)?;
            let redo = DELTAS + 208 * n;
            let (source, target) = raw.split_at_mut(redo);
            wal_frame::encode_patch_record(
                RedoTarget {
                    page: plan.identity.page,
                    owner: plan.identity.owner,
                    owner_generation: plan.identity.owner_generation,
                    kind: PageKind::Allocation,
                    level: 0,
                },
                page.header().lsn.0,
                10144,
                &source[NEW_CELL],
                &mut target[..6272],
            )
            .map_err(frame_error)?;
            wal_frame::finish_semantic_envelope(h, 1, raw).map_err(frame_error)?;
            Ok(h)
        }
    };
}
allocation_encoder!(encode_prepared, legacy);
allocation_encoder!(encode_retention_prepared, retention);

fn changed_entry(old: AllocationEntry, batch: Batch, cancel: bool) -> Result<AllocationEntry> {
    match old {
        AllocationEntry::Free(g) if !cancel => Ok(AllocationEntry::Reserved {
            generation: Generation::new(g.get().checked_add(1).ok_or(SemanticError::PageMismatch)?)
                .map_err(|_| SemanticError::PageMismatch)?,
            claimant: ReservationOwner::System(batch.owner),
        }),
        AllocationEntry::Reserved {
            generation,
            claimant,
        } if cancel && claimant == ReservationOwner::System(batch.owner) => {
            Ok(AllocationEntry::Free(generation))
        }
        _ => Err(SemanticError::PageMismatch),
    }
}

// Full validation also serves the private raw-negative seam. It does not itself
// publish an apply capability; prepare alone owns the immutable final borrow.
#[cfg(test)]
pub(in super::super) fn validate(
    plan: Plan<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
) -> Result<()> {
    validate_with_budget(plan, before, bytes, after, OperationBudget::Exclusive81920)
}

pub(in super::super) fn validate_with_budget(
    plan: Plan<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    budget: OperationBudget,
) -> Result<()> {
    let expected = AnalysisExpected::from(plan);
    validate_analysis_with_budget(expected, before, bytes, after, budget)
}

pub(in super::super) fn validate_analysis_with_budget(
    plan: AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    budget: OperationBudget,
) -> Result<()> {
    validate_analysis_in_population(
        plan,
        before,
        bytes,
        after,
        budget,
        AllocationPopulation::Empty,
    )
}

fn validate_analysis_in_population(
    plan: AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    budget: OperationBudget,
    population: AllocationPopulation,
) -> Result<()> {
    require(
        budget.accepts_count(plan.batch.count()) && population.accepts_budget(budget),
        SemanticError::StateMismatch,
    )?;
    // Preserve error precedence while retiring header/descriptor decoder locals
    // before native/page/inventory validation enters its nested codec calls.
    let header = validate_header(&plan, bytes)?;
    let descriptor = obligation::decode_descriptor(
        &bytes[352..1888],
        ReservationOwner::System(plan.batch.owner),
        profile_ref(plan.profile),
    )
    .map_err(descriptor_error)?;
    validate_descriptor(&plan, bytes, header, &descriptor)?;
    validate_transition(
        &plan,
        before,
        bytes,
        after,
        header,
        &descriptor,
        budget,
        population,
    )
}

fn validate_header(plan: &AnalysisExpected<'_>, bytes: &[u8]) -> Result<WalFrameHeader> {
    validate_header_scoped(plan, bytes, false)
}
fn validate_header_scoped(
    plan: &AnalysisExpected<'_>,
    bytes: &[u8],
    system: bool,
) -> Result<WalFrameHeader> {
    use SemanticError::*;
    let n = plan.batch.count();
    require(
        (1..=63).contains(&n) && plan.batch.mask & 1 == 0,
        StateMismatch,
    )?;
    let env = wal_frame::decode_semantic_envelope(
        bytes,
        FrameExpectation {
            stream: plan.frame.stream,
            previous_lsn: plan.frame.previous_lsn,
        },
    )
    .map_err(frame_error)?;
    require(bytes.len() == frame_length(n), Corrupt)?;
    let h = analysis_header(plan)?;
    require(env.header == h && env.record_count == 1, OrderMismatch)?;
    let op = if plan.cancel { 10 } else { 8 };
    require(
        get16(bytes, 166) == op && get16(bytes, 1888) == op,
        UnsupportedOperation,
    )?;
    require(
        &bytes[160..164] == b"XSG1"
            && get16(bytes, 164) == if system { 2 } else { 1 }
            && get16(bytes, 168) == 192
            && get16(bytes, 170) == u16::from(system)
            && get32(bytes, 172) as usize == bytes.len() - 192
            && get32(bytes, 280) == 1536
            && get16(bytes, 284) == 1
            && get16(bytes, 286) == 0
            && get16(bytes, 288) == 0
            && get16(bytes, 290) as usize == n
            && get16(bytes, 292) == 1
            && get16(bytes, 294) == 0
            && get32(bytes, 296) == 12544
            && get32(bytes, 300) == 0
            && get32(bytes, 304) == 0
            && get32(bytes, 308) as usize == 208 * n
            && get32(bytes, 312) == 6272
            && zero(&bytes[316..352])
            && zero(&bytes[193..200]),
        Corrupt,
    )?;
    match bytes[192] {
        2 => (),
        1 => {
            crate::storage::transaction_record::TxnRef::decode(&bytes[200..248])
                .map_err(|_| Corrupt)?;
            return Err(IdentityMismatch);
        }
        _ => return Err(Corrupt),
    }
    let owner = SystemWorkRef::decode(&bytes[200..248]).map_err(|_| Corrupt)?;
    require(owner == plan.batch.owner, IdentityMismatch)?;
    let previous = if plan.cancel { plan.batch.birth } else { 0 };
    require(
        get64(bytes, 176) == h.lsn && get64(bytes, 184) == previous,
        OrderMismatch,
    )?;
    let old_sha = plan.before_descriptor.map_or([0; 32], content_digest);
    require(bytes[248..280] == old_sha, OrderMismatch)?;
    Ok(h)
}

fn validate_descriptor(
    plan: &AnalysisExpected<'_>,
    bytes: &[u8],
    h: WalFrameHeader,
    desc: &DescriptorSpec,
) -> Result<()> {
    use SemanticError::*;
    let n = plan.batch.count();
    let previous = if plan.cancel { plan.batch.birth } else { 0 };
    let expected_chain = OwnerChain {
        apply: h.lsn,
        previous,
        birth: plan.batch.birth,
        groups: if plan.cancel { 2 } else { 1 },
    };
    require(desc.chain == expected_chain, OrderMismatch)?;
    let expected_progress = OwnerState::System(SystemProgress {
        reference: plan.batch.owner,
        phase: if plan.cancel {
            WorkPhase::Finished
        } else {
            WorkPhase::Prepared
        },
        sequence: 1,
        cursor: if plan.cancel { n as u64 } else { 0 },
        limit: n as u64,
    });
    require(desc.owner == expected_progress, StateMismatch)?;
    require(
        get16(bytes, 1890) == 0
            && get32(bytes, 1892) == 256
            && zero(&bytes[1896..2048])
            && zero(&bytes[2064..2144]),
        Corrupt,
    )?;
    require(
        get64(bytes, 2048) == 0 && get64(bytes, 2056) == if plan.cancel { n as u64 } else { 0 },
        StateMismatch,
    )?;
    Ok(())
}

macro_rules! allocation_population_check {
    (legacy, $old:ident, $plan:ident, $budget:ident, $population:ident, $h:ident, $state_error:ident) => {
        validate_population(&$old, $plan.cancel.then_some($plan.batch), $population)?;
        if $plan.cancel {
            validate_live_in_population(
                &$old,
                $plan.profile,
                Some($plan.batch),
                $plan.before_descriptor.ok_or($state_error)?,
                $plan.batch.owner.id(),
                $budget,
                $population,
                TerminalOutcome::Cancelled,
            )?;
        } else {
            require(
                $plan.before_descriptor.is_none() && $plan.batch.birth == $h.lsn,
                $state_error,
            )?;
        }
    };
    (retention, $old:ident, $plan:ident, $budget:ident, $population:ident, $h:ident, $state_error:ident) => {
        // The closed caller already checked the entire plan4 population and
        // actual current descriptor before entering this common transition.
        if !$plan.cancel {
            require(
                $plan.before_descriptor.is_none() && $plan.batch.birth == $h.lsn,
                SemanticError::StateMismatch,
            )?;
        }
    };
}
macro_rules! allocation_transition {
    ($name:ident, $policy:ident, $population:ident, ($($population_args:tt)*)) => {
#[allow(
    clippy::too_many_arguments,
    reason = "trusted population is separate from WAL semantic context"
)]
fn $name(
    plan: &AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    h: WalFrameHeader,
    desc: &DescriptorSpec,
    budget: OperationBudget,
    $($population_args)*
) -> Result<()> {
    use SemanticError::*;
    let n = plan.batch.count();
    require(bytes[2144] == 6, UnsupportedNativeBody)?;
    require(
        bytes[2145] == 3
            && get16(bytes, 2146) == 0
            && get32(bytes, 2156) == 6208
            && get32(bytes, 2160) == 6208
            && get32(bytes, 2164) == 12544
            && zero(&bytes[2232..2272]),
        Corrupt,
    )?;
    require(
        get16(bytes, 2148) == 0
            && get16(bytes, 2150) == 0
            && get16(bytes, 2152) == 10144
            && get16(bytes, 2154) == 10144,
        PageMismatch,
    )?;
    require(
        bytes[2168..2200] == content_digest(&bytes[OLD_CELL])
            && bytes[2200..2232] == content_digest(&bytes[NEW_CELL]),
        Corrupt,
    )?;
    let old = allocation_page::decode_allocation_page(
        before,
        plan.identity,
        plan.profile.spec().namespace,
    )
    .map_err(|_| PageMismatch)?;
    let new = allocation_page::decode_allocation_page(
        after,
        plan.identity,
        plan.profile.spec().namespace,
    )
    .map_err(|_| PageMismatch)?;
    require(
        allocation_before_lsn!($policy, old.header().lsn.0, plan.frame.previous_lsn) && new.header().lsn.0 == h.lsn,
        OrderMismatch,
    )?;
    require(
        bytes[OLD_CELL] == before[CELL] && bytes[NEW_CELL] == after[CELL],
        PageMismatch,
    )?;
    allocation_population_check!($policy, old, plan, budget, $population, h, StateMismatch);
    let mut selected = 0;
    for i in 0..64 {
        let entry = old.entry(i).map_err(|_| PageMismatch)?;
        if i > 0 && !plan.cancel {
            let eligible =
                matches!(entry,AllocationEntry::Free(g) if g.get()!=u64::MAX) && selected < n;
            require(
                eligible == (plan.batch.mask & (1u64 << i) != 0),
                PageMismatch,
            )?;
            if eligible {
                selected += 1;
            }
        }
        let expected = if plan.batch.mask & (1u64 << i) != 0 {
            changed_entry(entry, plan.batch, plan.cancel)?
        } else {
            entry
        };
        require(
            new.entry(i).map_err(|_| PageMismatch)? == expected,
            PageMismatch,
        )?;
    }
    let redo_start = DELTAS + 208 * n;
    let redo = wal_frame::decode_redo_record(h, &bytes[redo_start..redo_start + 6272])
        .map_err(|_| PageMismatch)?;
    require(redo.prior_lsn == old.header().lsn.0, OrderMismatch)?;
    require(
        redo.kind == RedoKind::Patch
            && redo.target
                == RedoTarget {
                    page: plan.identity.page,
                    owner: plan.identity.owner,
                    owner_generation: plan.identity.owner_generation,
                    kind: PageKind::Allocation,
                    level: 0,
                }
            && redo.offset == 10144
            && redo.data == &bytes[NEW_CELL],
        PageMismatch,
    )?;
    let mut digest = ContentHasher::new();
    let mut ordinal = 0;
    for i in 1..64 {
        if plan.batch.mask & (1u64 << i) == 0 {
            continue;
        }
        let wanted = item_for(&old, plan.batch, i, plan.cancel)?;
        digest.update(&wanted);
        let d = DELTAS + 208 * ordinal;
        require(
            bytes[d] == if plan.cancel { 3 } else { 1 }
                && bytes[d + 1] == if plan.cancel { 1 } else { 2 }
                && zero(&bytes[d + 2..d + 16]),
            InventoryMismatch,
        )?;
        let (old_item, new_item) = (&bytes[d + 16..d + 112], &bytes[d + 112..d + 208]);
        require(
            if plan.cancel {
                old_item == wanted && zero(new_item)
            } else {
                zero(old_item) && new_item == wanted
            },
            InventoryMismatch,
        )?;
        ordinal += 1;
    }
    let expected = descriptor(
        plan.profile,
        plan.batch,
        h.lsn,
        plan.cancel,
        digest.digest(),
        budget,
    );
    require(desc.inventory == expected.inventory, InventoryMismatch)?;
    require(
        desc.remaining == expected.remaining
            && desc.consumed_wal == expected.consumed_wal
            && desc.consumed_control == 0
            && desc.minimum_wal_lsn == expected.minimum_wal_lsn
            && desc.last_mutation_sequence == 0,
        BudgetMismatch,
    )?;
    Ok(())
}

    };
}
allocation_transition!(validate_transition, legacy, population, (population: AllocationPopulation,));
allocation_transition!(validate_retention_transition, retention, population, ());

/// Discardable RAM analysis output, never a live apply/stage permit. Physical
/// mixed home pages must not be supplied as this canonical before image.
pub(in super::super) fn materialize_analysis(
    expected: AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &mut [u8; PAGE_BYTES],
    budget: OperationBudget,
) -> Result<()> {
    materialize_analysis_in_population(
        expected,
        before,
        bytes,
        after,
        budget,
        AllocationPopulation::Empty,
    )
}

/// Canonical recovery scope is supplied by its owner, never serialized input.
pub(in super::super) fn materialize_analysis_in_population(
    expected: AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &mut [u8; PAGE_BYTES],
    budget: OperationBudget,
    population: AllocationPopulation,
) -> Result<()> {
    let header = validate_header(&expected, bytes)?;
    let old = allocation_page::decode_allocation_page(
        before,
        expected.identity,
        expected.profile.spec().namespace,
    )
    .map_err(|_| SemanticError::PageMismatch)?;
    allocation_page::encode_allocation_page(
        PageHeader {
            lsn: PageLsn(header.lsn),
            ..old.header()
        },
        expected.profile.spec().namespace,
        &bytes[NEW_CELL],
        after,
    )
    .map_err(|_| SemanticError::PageMismatch)?;
    validate_analysis_in_population(expected, before, bytes, after, budget, population)
}

#[path = "plan4_allocation_operation.rs"]
pub(crate) mod retention;
