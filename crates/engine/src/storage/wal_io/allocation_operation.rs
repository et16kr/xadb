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
use xadb_foundation::{
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
}
impl OperationBudget {
    pub(in super::super) const fn memory(self) -> u64 {
        match self {
            Self::Exclusive81920 => 81920,
            Self::Dirty98304 => 98304,
            Self::StatusInit114688 => 114688,
        }
    }

    pub(in super::super) fn accepts_count(self, count: usize) -> bool {
        (1..=63).contains(&count) && (self != Self::StatusInit114688 || count == 1)
    }

    pub(in super::super) fn terminal_wal(self, count: usize) -> u64 {
        if self == Self::StatusInit114688 {
            38000
        } else {
            frame_length(count) as u64
        }
    }

    pub(in super::super) const fn terminal_frames(self) -> u64 {
        if matches!(self, Self::StatusInit114688) {
            2
        } else {
            1
        }
    }

    pub(in super::super) fn terminal_protected(self, count: usize) -> u64 {
        self.terminal_wal(count)
            + if self == Self::StatusInit114688 {
                2
            } else {
                count as u64
            } * PAGE_BYTES as u64
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
    let mut observed_mask = 0;
    for i in 1..64 {
        match page.entry(i).map_err(|_| SemanticError::PageMismatch)? {
            AllocationEntry::Free(_) => (),
            AllocationEntry::Reserved { claimant, .. }
                if batch.is_some_and(|b| claimant == ReservationOwner::System(b.owner)) =>
            {
                observed_mask |= 1u64 << i
            }
            _ => return Err(SemanticError::PageMismatch),
        }
    }
    require(
        observed_mask == batch.map_or(0, |b| b.mask),
        SemanticError::InventoryMismatch,
    )?;
    if let Some(batch) = batch {
        validate_reserved(page, &profile, &batch, raw, high_water, budget)
    } else if high_water == 0 {
        require(
            zero(raw) && page.header().lsn == PageLsn(0),
            SemanticError::StateMismatch,
        )
    } else {
        validate_finished(page, &profile, raw, high_water, budget)
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
    require(
        current
            == descriptor(
                *profile,
                batch,
                page.header().lsn.0,
                true,
                content_digest(&[]),
                budget,
            ),
        SemanticError::StateMismatch,
    )
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
        wal: &wal::GenesisWalSession<'_, W>,
    ) -> std::result::Result<wal::WalFrameLocation, wal::WalIoError> {
        wal.stage_verified(self.prefix, self.header, self.bytes)
    }

    pub(in super::super) fn stage_required<'s, W: WriterNamespace>(
        self,
        wal: &'s wal::GenesisWalSession<'_, W>,
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
    // End the producer's array/descriptor stack frame before full validation.
    // Only this wrapper can turn the independently checked bytes into authority.
    let h = encode_prepared(plan, before, output, after, budget)?;
    let raw = &output[..frame_length(plan.batch.count())];
    validate_with_budget(plan, before, raw, after, budget)?;
    Ok(PreparedAllocation {
        bytes: raw,
        prefix: plan.prefix,
        header: h,
    })
}

fn encode_prepared(
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
        page.header().lsn.0 == plan.prefix.complete_lsn,
        SemanticError::OrderMismatch,
    )?;
    let raw = &mut output[..length];
    raw.fill(0);
    let g = 160;
    raw[g..g + 4].copy_from_slice(b"XSG1");
    put16(raw, g + 4, 1);
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
    require(
        budget.accepts_count(plan.batch.count()),
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
    validate_transition(&plan, before, bytes, after, header, &descriptor, budget)
}

fn validate_header(plan: &AnalysisExpected<'_>, bytes: &[u8]) -> Result<WalFrameHeader> {
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
            && get16(bytes, 164) == 1
            && get16(bytes, 168) == 192
            && get16(bytes, 170) == 0
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

fn validate_transition(
    plan: &AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    h: WalFrameHeader,
    desc: &DescriptorSpec,
    budget: OperationBudget,
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
        old.header().lsn.0 == plan.frame.previous_lsn && new.header().lsn.0 == h.lsn,
        OrderMismatch,
    )?;
    require(
        bytes[OLD_CELL] == before[CELL] && bytes[NEW_CELL] == after[CELL],
        PageMismatch,
    )?;
    if plan.cancel {
        validate_live_with_budget(
            &old,
            plan.profile,
            Some(plan.batch),
            plan.before_descriptor.ok_or(StateMismatch)?,
            plan.batch.owner.id(),
            budget,
        )?;
    } else {
        require(
            plan.before_descriptor.is_none() && plan.batch.birth == h.lsn,
            StateMismatch,
        )?;
    }
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

/// Discardable RAM analysis output, never a live apply/stage permit. Physical
/// mixed home pages must not be supplied as this canonical before image.
pub(in super::super) fn materialize_analysis(
    expected: AnalysisExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &mut [u8; PAGE_BYTES],
    budget: OperationBudget,
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
    validate_analysis_with_budget(expected, before, bytes, after, budget)
}
