//! One actual reserved page becomes the immutable bootstrap Status container.
//! Prepared bytes and both after images are sealed together; this module never
//! installs live pages, invents a role, or grants restart/replay authority.

use super::super::allocation_reservation::{
    budget,
    operation::{
        self, AllocationPopulation, Batch, OperationBudget, SemanticError, TerminalOutcome,
    },
};
use crate::storage::{
    allocation_page::{self, AllocationEntry, ReservationOwner, SystemWorkKind},
    control::{ControlKind, ControlRef, Profile},
    obligation::{self},
    page::{self, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn},
    session_fence::SessionFence,
    status_home::{STATUS_HOME_BYTES, StatusHome, StatusHomePage},
    transaction_record::TransactionStatusCell,
    undo_record,
    wal_frame::{
        self, FrameEnvelope, FrameExpectation, FrameKind, RedoKind, RedoRecordView, RedoTarget,
        WalError, WalFrameHeader,
    },
    wal_io::{self as wal, StatusRole, WalPrefix, WalRequirement, WalRuntime},
};
use std::mem::size_of;
use xadb_foundation::{codec::content_digest, identity::Generation, namespace::WriterNamespace};

type Result<T> = std::result::Result<T, SemanticError>;
pub(super) const FRAME_BYTES: usize = 38000;
const CELL: std::ops::Range<usize> = 10144..16352;
const OLD_CELL: std::ops::Range<usize> = 2272..8480;
const NEW_CELL: std::ops::Range<usize> = 8480..14688;
const STATUS_NATIVE: usize = 14688;
const STATUS_BODY: std::ops::Range<usize> = 14816..15040;
const DELTA: usize = 15040;
const PATCH: std::ops::Range<usize> = 15248..21520;
const INIT: std::ops::Range<usize> = 21520..37968;
const HOME: std::ops::Range<usize> = 16128..16352;

#[path = "fixed_system_operation.rs"]
pub(super) mod fixed;
#[path = "mixed_system_init.rs"]
pub(super) mod mixed;
#[path = "plan4_status_init_operation.rs"]
pub(crate) mod retention;

/// The legacy wrappers can construct only Status. Mixed scope is selected by
/// the separate plan3 role-bearing wrapper, never by a stored native tag.
#[derive(Clone, Copy)]
enum InitScope {
    Status,
    Mixed(mixed::Kind),
}
impl InitScope {
    fn undo(self) -> bool {
        matches!(self, Self::Mixed(mixed::Kind::UndoRoot))
    }
    fn body_bytes(self) -> usize {
        if self.undo() { 432 } else { 224 }
    }
    fn frame_bytes(self) -> usize {
        FRAME_BYTES + self.body_bytes() - 224
    }
    fn home(self) -> std::ops::Range<usize> {
        if self.undo() {
            undo_record::INITIAL_UNDO_CELL
        } else {
            HOME
        }
    }
    fn body(self) -> std::ops::Range<usize> {
        STATUS_BODY.start..STATUS_BODY.end + self.body_bytes() - 224
    }
    fn delta(self) -> usize {
        DELTA + self.body_bytes() - 224
    }
    fn patch(self) -> std::ops::Range<usize> {
        PATCH.start + self.body_bytes() - 224..PATCH.end + self.body_bytes() - 224
    }
    fn init(self) -> std::ops::Range<usize> {
        INIT.start + self.body_bytes() - 224..INIT.end + self.body_bytes() - 224
    }
    fn owner(self) -> u64 {
        if self.undo() { 3 } else { 2 }
    }
    fn kind(self) -> PageKind {
        if self.undo() {
            PageKind::Undo
        } else {
            PageKind::TransactionStatus
        }
    }
    fn native(self) -> u8 {
        if self.undo() { 3 } else { 5 }
    }
    fn budget(self) -> OperationBudget {
        match self {
            Self::Status => OperationBudget::StatusInit114688,
            Self::Mixed(_) => OperationBudget::MixedInit114688,
        }
    }
    fn accepts_population(self, population: AllocationPopulation) -> bool {
        match self {
            Self::Status => matches!(
                population,
                AllocationPopulation::Empty | AllocationPopulation::Status
            ),
            Self::Mixed(_) => population == AllocationPopulation::MixedSystem,
        }
    }
    fn finished(self, profile: &Profile, batch: Batch, lsn: PageLsn) -> obligation::DescriptorSpec {
        match self {
            Self::Status => operation::finished_status_descriptor(profile, batch, lsn),
            Self::Mixed(kind) => operation::finished_mixed_descriptor(profile, batch, lsn, kind),
        }
    }
    fn page_lsn(self, bytes: &[u8; PAGE_BYTES], identity: &PageIdentity) -> Result<PageLsn> {
        if self.undo() {
            undo_record::initial_undo_page_lsn(bytes, *identity)
                .map_err(|_| SemanticError::PageMismatch)
        } else {
            status_lsn(bytes, identity)
        }
    }
    fn encode_page(
        self,
        identity: &PageIdentity,
        lsn: PageLsn,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<()> {
        if self.undo() {
            undo_record::encode_initial_undo_page(*identity, lsn, output)
                .map_err(|_| SemanticError::PageMismatch)
        } else {
            encode_status_image(identity, lsn, output)
        }
    }
}

/// A short borrowed internal context keeps the old pure expectation API fixed.
/// It cannot be returned as a role or installation permit.
struct ScopedExpected<'a, 'p> {
    context: &'a InitExpected<'p>,
    scope: InitScope,
}
impl<'p> std::ops::Deref for ScopedExpected<'_, 'p> {
    type Target = InitExpected<'p>;
    fn deref(&self) -> &Self::Target {
        self.context
    }
}

/// Inputs have live authority only because the upper bound owns the actual role
/// and reservation. Full validation below is still required before preparation.
pub(super) struct InitPlan<'p, 's> {
    pub(super) profile: Profile,
    pub(super) allocator: PageIdentity,
    pub(super) payload: PageIdentity,
    pub(super) prefix: WalPrefix,
    pub(super) batch: Batch,
    pub(super) before_descriptor: &'p [u8; 1536],
    pub(super) role: &'p StatusRole<'s>,
}

/// Canonical byte/geometry context only. This is not a live StatusRole,
/// fence, pin or installation permit and never authorizes stage/apply.
pub(super) struct InitExpected<'p> {
    pub(super) profile: Profile,
    pub(super) allocator: PageIdentity,
    pub(super) payload: PageIdentity,
    pub(super) frame: FrameExpectation,
    pub(super) batch: Batch,
    pub(super) before_descriptor: &'p [u8; 1536],
}
impl<'p> InitPlan<'p, '_> {
    fn expected(&self) -> Result<InitExpected<'p>> {
        require(
            self.role.namespace() == self.profile.spec().namespace
                && self.role.object() == self.payload.owner
                && self.role.generation() == self.payload.owner_generation
                && self.role.kind() == PageKind::TransactionStatus
                && self.role.level() == 0,
            SemanticError::IdentityMismatch,
        )?;
        Ok(InitExpected {
            profile: self.profile,
            allocator: self.allocator,
            payload: self.payload,
            frame: FrameExpectation {
                stream: self.prefix.stream,
                previous_lsn: self.prefix.complete_lsn,
            },
            batch: self.batch,
            before_descriptor: self.before_descriptor,
        })
    }
}

/// No Clone/Copy and no raw constructor. Each produced region is borrowed for
/// the same lifetime, so consuming stage cannot allow an after image to change.
pub(super) struct PreparedInit<'out, 's> {
    bytes: &'out [u8],
    allocator_after: &'out [u8; PAGE_BYTES],
    status_after: &'out [u8; PAGE_BYTES],
    prefix: WalPrefix,
    header: WalFrameHeader,
    fence: &'s SessionFence,
}
impl<'out, 's> PreparedInit<'out, 's> {
    pub(super) fn descriptor_bytes(&self) -> &'out [u8; 1536] {
        self.bytes[352..1888]
            .try_into()
            .expect("sealed descriptor span")
    }
    pub(super) fn allocator_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.allocator_after
    }
    pub(super) fn status_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.status_after
    }
    pub(super) fn stage_required<W: WriterNamespace>(
        self,
        session: &'s WalRuntime<'_, W>,
    ) -> std::result::Result<WalRequirement<'s>, wal::WalIoError> {
        session.prefix()?; // Health precedes an otherwise unchanged origin mismatch.
        if !std::ptr::eq(self.fence, &session.fenced) {
            return Err(wal::unchanged(
                wal::Phase::Stage,
                wal::Cause::IdentityMismatch,
            ));
        }
        session.stage_verified_required(self.prefix, self.header, self.bytes)
    }
}

fn require(ok: bool, error: SemanticError) -> Result<()> {
    if ok { Ok(()) } else { Err(error) }
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
fn get16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("bounded u16"))
}
fn get32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded u32"))
}
fn get64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("bounded u64"))
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn frame_error(error: WalError) -> SemanticError {
    match error {
        WalError::IdentityMismatch => SemanticError::IdentityMismatch,
        WalError::OrderMismatch => SemanticError::OrderMismatch,
        _ => SemanticError::Corrupt,
    }
}
fn profile_ref(profile: &Profile) -> ControlRef {
    ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: profile.spec().generation,
        encoded_len: 4096,
        digest: profile.digest(),
    }
}
fn header(plan: &ScopedExpected<'_, '_>) -> Result<WalFrameHeader> {
    Ok(WalFrameHeader {
        stream: plan.frame.stream,
        kind: FrameKind::System,
        lsn: plan
            .frame
            .previous_lsn
            .checked_add(1)
            .ok_or(SemanticError::OrderMismatch)?,
        previous_lsn: plan.frame.previous_lsn,
        txn: None,
        previous_txn_lsn: 0,
    })
}
fn target(identity: &PageIdentity, kind: PageKind) -> RedoTarget {
    RedoTarget {
        page: identity.page,
        owner: identity.owner,
        owner_generation: identity.owner_generation,
        kind,
        level: 0,
    }
}
fn plan_index(plan: &ScopedExpected<'_, '_>) -> Result<usize> {
    use SemanticError::*;
    let namespace = plan.profile.spec().namespace;
    require(
        plan.allocator.database == namespace.database
            && plan.payload.database == namespace.database
            && plan.payload.owner.get() == plan.scope.owner()
            && plan.payload.owner_generation.get() == 1
            && plan.frame.stream.database == namespace.database
            && plan.frame.stream.timeline == namespace.timeline,
        IdentityMismatch,
    )?;
    require(
        plan.batch.owner.namespace() == namespace
            && plan.batch.owner.kind() == SystemWorkKind::AllocationMaintenance
            && plan.batch.count() == 1
            && plan.batch.mask & 1 == 0,
        StateMismatch,
    )?;
    let index = plan.batch.mask.trailing_zeros() as usize;
    require(
        (1..64).contains(&index)
            && plan.payload.page.file == plan.allocator.page.file
            && plan.allocator.page.page_no.checked_add(index as u64)
                == Some(plan.payload.page.page_no),
        IdentityMismatch,
    )?;
    require(plan.batch.birth == plan.frame.previous_lsn, OrderMismatch)?;
    Ok(index)
}
fn token(plan: &ScopedExpected<'_, '_>) -> Result<[u8; 96]> {
    let mut item = [0; 96];
    item[0] = 1;
    item[1] = 3;
    plan.payload
        .page
        .encode(&mut item[8..32])
        .map_err(|_| SemanticError::InventoryMismatch)?;
    put64(&mut item, 64, 1);
    put64(&mut item, 72, plan.batch.birth);
    Ok(item)
}
fn allocated(plan: &ScopedExpected<'_, '_>) -> AllocationEntry {
    AllocationEntry::Allocated {
        generation: plan.payload.page.incarnation,
        object: plan.payload.owner,
        object_generation: plan.payload.owner_generation,
        kind: plan.scope.kind(),
    }
}

/// Ends before any entry array, Status codec or output write exists.
fn validate_before(
    plan: &ScopedExpected<'_, '_>,
    before: &[u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<()> {
    require(
        plan.scope.accepts_population(population),
        SemanticError::StateMismatch,
    )?;
    plan_index(plan)?;
    let old = allocation_page::decode_allocation_page(
        before,
        plan.allocator,
        plan.profile.spec().namespace,
    )
    .map_err(|_| SemanticError::PageMismatch)?;
    operation::validate_live_in_population(
        &old,
        plan.profile,
        Some(plan.batch),
        plan.before_descriptor,
        plan.batch.owner.id(),
        plan.scope.budget(),
        population,
        TerminalOutcome::Cancelled,
    )?;
    old.lookup_reserved(
        plan.payload.page,
        ReservationOwner::System(plan.batch.owner),
    )
    .map_err(|_| SemanticError::InventoryMismatch)
}

pub(super) fn prepare_init<'out, 's>(
    plan: &InitPlan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    status_after: &'out mut [u8; PAGE_BYTES],
) -> Result<PreparedInit<'out, 's>> {
    prepare_init_in_population(
        plan,
        before,
        output,
        allocator_after,
        status_after,
        AllocationPopulation::Empty,
    )
}

pub(super) fn prepare_init_in_population<'out, 's>(
    plan: &InitPlan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    status_after: &'out mut [u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<PreparedInit<'out, 's>> {
    let expected = plan.expected()?;
    prepare_scoped(
        &ScopedExpected {
            context: &expected,
            scope: InitScope::Status,
        },
        plan.prefix,
        plan.role.fence,
        before,
        output,
        allocator_after,
        status_after,
        population,
    )
}
#[allow(
    clippy::too_many_arguments,
    reason = "short preparation context borrows existing caller loans and authority"
)]
fn prepare_scoped<'out, 's>(
    expected: &ScopedExpected<'_, '_>,
    prefix: WalPrefix,
    fence: &'s SessionFence,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    status_after: &'out mut [u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<PreparedInit<'out, 's>> {
    validate_before(expected, before, population)?;
    let h = header(expected)?;
    let bytes = &mut output[..expected.scope.frame_bytes()];
    bytes.fill(0);
    encode_allocator(expected, before, h.lsn, bytes, allocator_after)?;
    encode_finished_descriptor(
        &expected.profile,
        expected.batch,
        PageLsn(h.lsn),
        &mut bytes[352..1888],
        expected.scope,
    )?;
    // Neither allocation entry array nor DescriptorSpec survives into this helper.
    expected
        .scope
        .encode_page(&expected.payload, PageLsn(h.lsn), status_after)?;
    encode_group(expected, h, before, bytes, allocator_after, status_after)?;
    validate_expected_in_population(
        expected,
        before,
        bytes,
        allocator_after,
        status_after,
        population,
    )?;
    Ok(PreparedInit {
        bytes,
        allocator_after,
        status_after,
        prefix,
        header: h,
        fence,
    })
}
fn encode_allocator(
    plan: &ScopedExpected<'_, '_>,
    before: &[u8; PAGE_BYTES],
    lsn: u64,
    bytes: &mut [u8],
    after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let index = plan_index(plan)?;
    let old = allocation_page::decode_allocation_page(
        before,
        plan.allocator,
        plan.profile.spec().namespace,
    )
    .map_err(|_| SemanticError::PageMismatch)?;
    bytes[OLD_CELL].copy_from_slice(&before[CELL]);
    {
        let mut entries = [AllocationEntry::Free(Generation::new(1).expect("nonzero")); 64];
        for (at, entry) in entries.iter_mut().enumerate() {
            *entry = if at == index {
                allocated(plan)
            } else {
                old.entry(at).map_err(|_| SemanticError::PageMismatch)?
            };
        }
        allocation_page::encode_allocation_cell(
            old.header(),
            plan.profile.spec().namespace,
            &entries,
            &mut bytes[NEW_CELL],
        )
        .map_err(|_| SemanticError::PageMismatch)?;
    }
    allocation_page::encode_allocation_page(
        PageHeader {
            lsn: PageLsn(lsn),
            ..old.header()
        },
        plan.profile.spec().namespace,
        &bytes[NEW_CELL],
        after,
    )
    .map_err(|_| SemanticError::PageMismatch)
}
fn encode_finished_descriptor(
    profile: &Profile,
    batch: Batch,
    lsn: PageLsn,
    output: &mut [u8],
    scope: InitScope,
) -> Result<()> {
    let desc = scope.finished(profile, batch, lsn);
    obligation::encode_descriptor(&desc, output).map_err(|_| SemanticError::Corrupt)
}
pub(super) fn validate_finished(
    profile: Profile,
    batch: Batch,
    raw: &[u8; 1536],
    lsn: PageLsn,
) -> Result<()> {
    validate_finished_scoped(profile, batch, raw, lsn, InitScope::Status)
}
fn validate_finished_scoped(
    profile: Profile,
    batch: Batch,
    raw: &[u8; 1536],
    lsn: PageLsn,
    scope: InitScope,
) -> Result<()> {
    require(
        batch.count() == 1 && batch.mask & 1 == 0 && batch.birth.checked_add(1) == Some(lsn.0),
        SemanticError::OrderMismatch,
    )?;
    let current = obligation::decode_descriptor(
        raw,
        ReservationOwner::System(batch.owner),
        profile_ref(&profile),
    )
    .map_err(|error| {
        if error == obligation::ObligationError::IdentityMismatch {
            SemanticError::IdentityMismatch
        } else {
            SemanticError::Corrupt
        }
    })?;
    let expected = scope.finished(&profile, batch, lsn);
    require(
        current.owner == expected.owner,
        SemanticError::StateMismatch,
    )?;
    require(
        current.chain == expected.chain,
        SemanticError::OrderMismatch,
    )?;
    require(
        current.inventory == expected.inventory,
        SemanticError::InventoryMismatch,
    )?;
    require(current == expected, SemanticError::BudgetMismatch)
}
fn encode_status_image(
    identity: &PageIdentity,
    lsn: PageLsn,
    output: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let mut home = [0; STATUS_HOME_BYTES];
    StatusHome::new(
        TransactionStatusCell::Free(Generation::new(1).expect("nonzero")),
        0,
    )
    .map_err(|_| SemanticError::PageMismatch)?
    .encode(&mut home)
    .map_err(|_| SemanticError::PageMismatch)?;
    page::encode_page(
        PageHeader {
            identity: *identity,
            kind: PageKind::TransactionStatus,
            level: 0,
            lsn,
        },
        &[&home],
        output,
    )
    .map_err(|_| SemanticError::PageMismatch)
}
fn encode_group(
    plan: &ScopedExpected<'_, '_>,
    h: WalFrameHeader,
    before: &[u8; PAGE_BYTES],
    bytes: &mut [u8],
    allocator_after: &[u8; PAGE_BYTES],
    status_after: &[u8; PAGE_BYTES],
) -> Result<()> {
    bytes[160..164].copy_from_slice(b"XSG1");
    put16(bytes, 164, 1);
    put16(bytes, 166, 10);
    put16(bytes, 168, 192);
    put32(bytes, 172, (plan.scope.frame_bytes() - 192) as u32);
    put64(bytes, 176, h.lsn);
    put64(bytes, 184, plan.batch.birth);
    bytes[192] = 2;
    plan.batch
        .owner
        .encode(&mut bytes[200..248])
        .map_err(|_| SemanticError::IdentityMismatch)?;
    bytes[248..280].copy_from_slice(&content_digest(plan.before_descriptor));
    put32(bytes, 280, 1536);
    put16(bytes, 284, 2);
    put16(bytes, 290, 1);
    put16(bytes, 292, 2);
    put32(bytes, 296, (12544 + 128 + plan.scope.body_bytes()) as u32);
    put32(bytes, 308, 208);
    put32(bytes, 312, 22720);
    put16(bytes, 1888, 10);
    put32(bytes, 1892, 256);
    put64(bytes, 2056, 1);
    bytes[2144] = 6;
    bytes[2145] = 3;
    put16(bytes, 2152, 10144);
    put16(bytes, 2154, 10144);
    put32(bytes, 2156, 6208);
    put32(bytes, 2160, 6208);
    put32(bytes, 2164, 12544);
    bytes[2168..2200].copy_from_slice(&content_digest(&before[CELL]));
    bytes[2200..2232].copy_from_slice(&content_digest(&allocator_after[CELL]));
    bytes[STATUS_NATIVE] = plan.scope.native();
    bytes[STATUS_NATIVE + 1] = 2;
    put16(bytes, STATUS_NATIVE + 2, 1);
    put16(bytes, STATUS_NATIVE + 4, u16::MAX);
    put16(bytes, STATUS_NATIVE + 10, plan.scope.home().start as u16);
    put32(bytes, STATUS_NATIVE + 16, plan.scope.body_bytes() as u32);
    put32(
        bytes,
        STATUS_NATIVE + 20,
        (128 + plan.scope.body_bytes()) as u32,
    );
    bytes[STATUS_NATIVE + 56..STATUS_NATIVE + 88]
        .copy_from_slice(&content_digest(&status_after[plan.scope.home()]));
    bytes[plan.scope.body()].copy_from_slice(&status_after[plan.scope.home()]);
    bytes[plan.scope.delta()] = 2;
    bytes[plan.scope.delta() + 1] = 1;
    bytes[plan.scope.delta() + 16..plan.scope.delta() + 112].copy_from_slice(&token(plan)?);
    wal_frame::encode_patch_record(
        target(&plan.allocator, PageKind::Allocation),
        plan.frame.previous_lsn,
        10144,
        &allocator_after[CELL],
        &mut bytes[plan.scope.patch()],
    )
    .map_err(frame_error)?;
    wal_frame::encode_init_record(
        plan.payload.database,
        h.lsn,
        target(&plan.payload, plan.scope.kind()),
        status_after,
        &mut bytes[plan.scope.init()],
    )
    .map_err(frame_error)?;
    wal_frame::finish_semantic_envelope(h, 2, bytes).map_err(frame_error)?;
    Ok(())
}

/// Full independent byte validation; success alone creates no live capability.
/// Header/descriptor, allocator and Status checks return in separate phases.
pub(super) fn validate_init(
    plan: &InitPlan<'_, '_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &[u8; PAGE_BYTES],
    status_after: &[u8; PAGE_BYTES],
) -> Result<()> {
    let expected = plan.expected()?;
    validate_expected(&expected, before, bytes, allocator_after, status_after)
}

pub(super) fn validate_expected(
    plan: &InitExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &[u8; PAGE_BYTES],
    status_after: &[u8; PAGE_BYTES],
) -> Result<()> {
    validate_expected_in_population(
        &ScopedExpected {
            context: plan,
            scope: InitScope::Status,
        },
        before,
        bytes,
        allocator_after,
        status_after,
        AllocationPopulation::Empty,
    )
}
fn validate_expected_in_population(
    plan: &ScopedExpected<'_, '_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &[u8; PAGE_BYTES],
    status_after: &[u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<()> {
    validate_before(plan, before, population)?;
    let h = validate_header(plan, bytes)?;
    validate_finished_scoped(
        plan.profile,
        plan.batch,
        bytes[352..1888].try_into().expect("validated span"),
        PageLsn(h.lsn),
        plan.scope,
    )?;
    validate_allocator_transition(plan, before, bytes, allocator_after, h)?;
    // No allocation Page/descriptor/entry array is retained in this Status phase.
    validate_status_transition(plan, bytes, status_after, h)?;
    Ok(())
}
fn validate_header(plan: &ScopedExpected<'_, '_>, bytes: &[u8]) -> Result<WalFrameHeader> {
    use SemanticError::*;
    require(bytes.len() == plan.scope.frame_bytes(), Corrupt)?;
    let env = wal_frame::decode_semantic_envelope(
        bytes,
        FrameExpectation {
            stream: plan.frame.stream,
            previous_lsn: plan.frame.previous_lsn,
        },
    )
    .map_err(frame_error)?;
    require(env.header == header(plan)?, OrderMismatch)?;
    require(
        env.record_count == 2 && &bytes[160..164] == b"XSG1" && get16(bytes, 164) == 1,
        Corrupt,
    )?;
    require(
        get16(bytes, 166) == 10 && get16(bytes, 1888) == 10,
        UnsupportedOperation,
    )?;
    require(
        get16(bytes, 168) == 192
            && get16(bytes, 170) == 0
            && get32(bytes, 172) == (plan.scope.frame_bytes() - 192) as u32
            && bytes[192] == 2
            && zero(&bytes[193..200])
            && get32(bytes, 280) == 1536
            && get16(bytes, 284) == 2
            && get16(bytes, 286) == 0
            && get16(bytes, 288) == 0
            && get16(bytes, 290) == 1
            && get16(bytes, 292) == 2
            && get16(bytes, 294) == 0
            && get32(bytes, 296) == (12544 + 128 + plan.scope.body_bytes()) as u32
            && get32(bytes, 300) == 0
            && get32(bytes, 304) == 0
            && get32(bytes, 308) == 208
            && get32(bytes, 312) == 22720
            && zero(&bytes[316..352]),
        Corrupt,
    )?;
    require(
        get64(bytes, 176) == env.header.lsn && get64(bytes, 184) == plan.batch.birth,
        OrderMismatch,
    )?;
    require(
        crate::storage::allocation_page::SystemWorkRef::decode(&bytes[200..248])
            .map_err(|_| IdentityMismatch)?
            == plan.batch.owner,
        IdentityMismatch,
    )?;
    require(
        bytes[248..280] == content_digest(plan.before_descriptor),
        StateMismatch,
    )?;
    require(
        get16(bytes, 1890) == 0
            && get32(bytes, 1892) == 256
            && zero(&bytes[1896..2048])
            && get64(bytes, 2048) == 0
            && get64(bytes, 2056) == 1
            && zero(&bytes[2064..2144]),
        StateMismatch,
    )?;
    Ok(env.header)
}
fn validate_allocator_transition(
    plan: &ScopedExpected<'_, '_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    h: WalFrameHeader,
) -> Result<()> {
    use SemanticError::*;
    require(bytes[2144] == 6, UnsupportedNativeBody)?;
    require(
        bytes[2145] == 3
            && get16(bytes, 2146) == 0
            && get16(bytes, 2148) == 0
            && get16(bytes, 2150) == 0
            && get16(bytes, 2152) == 10144
            && get16(bytes, 2154) == 10144
            && get32(bytes, 2156) == 6208
            && get32(bytes, 2160) == 6208
            && get32(bytes, 2164) == 12544
            && zero(&bytes[2232..2272]),
        Corrupt,
    )?;
    require(
        bytes[2168..2200] == content_digest(&bytes[OLD_CELL])
            && bytes[2200..2232] == content_digest(&bytes[NEW_CELL]),
        Corrupt,
    )?;
    require(
        bytes[OLD_CELL] == before[CELL] && bytes[NEW_CELL] == after[CELL],
        PageMismatch,
    )?;
    let old = allocation_page::decode_allocation_page(
        before,
        plan.allocator,
        plan.profile.spec().namespace,
    )
    .map_err(|_| PageMismatch)?;
    let new = allocation_page::decode_allocation_page(
        after,
        plan.allocator,
        plan.profile.spec().namespace,
    )
    .map_err(|_| PageMismatch)?;
    require(
        old.header().lsn.0 == plan.batch.birth && new.header().lsn.0 == h.lsn,
        OrderMismatch,
    )?;
    let selected = plan_index(plan)?;
    for index in 0..64 {
        let expected = if index == selected {
            allocated(plan)
        } else {
            old.entry(index).map_err(|_| PageMismatch)?
        };
        require(
            new.entry(index).map_err(|_| PageMismatch)? == expected,
            PageMismatch,
        )?;
    }
    require(
        bytes[plan.scope.delta()] == 2
            && bytes[plan.scope.delta() + 1] == 1
            && zero(&bytes[plan.scope.delta() + 2..plan.scope.delta() + 16])
            && bytes[plan.scope.delta() + 16..plan.scope.delta() + 112] == token(plan)?
            && zero(&bytes[plan.scope.delta() + 112..plan.scope.delta() + 208]),
        InventoryMismatch,
    )?;
    let redo = wal_frame::decode_redo_record(h, &bytes[plan.scope.patch()]).map_err(frame_error)?;
    require(redo.prior_lsn == plan.batch.birth, OrderMismatch)?;
    require(
        redo.kind == RedoKind::Patch
            && redo.target == target(&plan.allocator, PageKind::Allocation)
            && redo.offset == 10144
            && redo.data == &after[CELL],
        PageMismatch,
    )
}
fn validate_status_transition(
    plan: &ScopedExpected<'_, '_>,
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    h: WalFrameHeader,
) -> Result<()> {
    use SemanticError::*;
    let n = STATUS_NATIVE;
    require(bytes[n] == plan.scope.native(), UnsupportedNativeBody)?;
    require(
        bytes[n + 1] == 2
            && get16(bytes, n + 2) == 1
            && get16(bytes, n + 4) == u16::MAX
            && get16(bytes, n + 6) == 0
            && get16(bytes, n + 8) == 0
            && get16(bytes, n + 10) == plan.scope.home().start as u16
            && get32(bytes, n + 12) == 0
            && get32(bytes, n + 16) == plan.scope.body_bytes() as u32
            && get32(bytes, n + 20) == (128 + plan.scope.body_bytes()) as u32
            && zero(&bytes[n + 24..n + 56])
            && zero(&bytes[n + 88..n + 128]),
        Corrupt,
    )?;
    require(
        bytes[n + 56..n + 88] == content_digest(&bytes[plan.scope.body()]),
        Corrupt,
    )?;
    require(
        bytes[plan.scope.body()] == after[plan.scope.home()],
        PageMismatch,
    )?;
    let redo = wal_frame::decode_redo_record(h, &bytes[plan.scope.init()]).map_err(frame_error)?;
    require(
        redo.kind == RedoKind::Init
            && redo.target == target(&plan.payload, plan.scope.kind())
            && redo.prior_lsn == 0
            && redo.offset == 0
            && redo.data == after,
        PageMismatch,
    )?;
    require(
        plan.scope.page_lsn(after, &plan.payload)? == PageLsn(h.lsn),
        OrderMismatch,
    )
}

/// Output scratch is discardable on error; immutable inputs are never changed.
/// Validating an already-applied/newer before image is deliberately not replay.
pub(super) fn materialize_init(
    plan: &InitPlan<'_, '_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &mut [u8; PAGE_BYTES],
    status_after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let expected = plan.expected()?;
    materialize_analysis(&expected, before, bytes, allocator_after, status_after)
}

pub(super) fn materialize_analysis(
    plan: &InitExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &mut [u8; PAGE_BYTES],
    status_after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    materialize_analysis_in_population(
        plan,
        before,
        bytes,
        allocator_after,
        status_after,
        AllocationPopulation::Empty,
    )
}

/// Discardable canonical analysis; population comes from the recovery owner.
pub(super) fn materialize_analysis_in_population(
    plan: &InitExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &mut [u8; PAGE_BYTES],
    status_after: &mut [u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<()> {
    materialize_scoped(
        &ScopedExpected {
            context: plan,
            scope: InitScope::Status,
        },
        before,
        bytes,
        allocator_after,
        status_after,
        population,
    )
}
fn materialize_scoped(
    plan: &ScopedExpected<'_, '_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &mut [u8; PAGE_BYTES],
    status_after: &mut [u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<()> {
    validate_before(plan, before, population)?;
    let h = validate_header(plan, bytes)?;
    allocator_after.copy_from_slice(before);
    allocator_after[CELL].copy_from_slice(&bytes[NEW_CELL]);
    put64(allocator_after, 64, h.lsn);
    let digest = content_digest(&allocator_after[..page::DIGEST_OFFSET]);
    allocator_after[page::DIGEST_OFFSET..].copy_from_slice(&digest);
    status_after.copy_from_slice(&bytes[plan.scope.init().start + 64..plan.scope.init().end]);
    validate_expected_in_population(
        plan,
        before,
        bytes,
        allocator_after,
        status_after,
        population,
    )
}

/// Reader's Status checksum/page phase ends before canonical Free decoding.
pub(super) fn status_lsn(bytes: &[u8; PAGE_BYTES], identity: &PageIdentity) -> Result<PageLsn> {
    let lsn = status_envelope(bytes, identity)?;
    validate_free_home(&bytes[HOME])?;
    Ok(lsn)
}
pub(super) fn status_envelope(
    bytes: &[u8; PAGE_BYTES],
    identity: &PageIdentity,
) -> Result<PageLsn> {
    let page = page::decode_page(bytes, *identity, PageKind::TransactionStatus)
        .map_err(|_| SemanticError::PageMismatch)?;
    require(
        page.header().level == 0 && page.len() == 1 && page.cell_range(0) == Some(HOME),
        SemanticError::PageMismatch,
    )?;
    Ok(page.header().lsn)
}
fn validate_free_home(bytes: &[u8]) -> Result<()> {
    // This fresh-container reader accepts no Occupied/Undo decode branch.
    require(
        bytes.len() == 224 && bytes[38] == 0,
        SemanticError::PageMismatch,
    )?;
    let home = StatusHome::decode(bytes).map_err(|_| SemanticError::PageMismatch)?;
    require(
        home.core() == TransactionStatusCell::Free(Generation::new(1).expect("nonzero"))
            && home.incoming() == 0,
        SemanticError::PageMismatch,
    )
}
pub(super) fn membership_lsn(
    profile: &Profile,
    allocator: &PageIdentity,
    payload: &PageIdentity,
    role: &StatusRole<'_>,
    bytes: &[u8; PAGE_BYTES],
    population: AllocationPopulation,
) -> Result<PageLsn> {
    use SemanticError::*;
    require(
        role.namespace() == profile.spec().namespace
            && role.kind() == PageKind::TransactionStatus
            && role.level() == 0
            && payload.database == role.namespace().database
            && payload.owner == role.object()
            && payload.owner_generation == role.generation(),
        IdentityMismatch,
    )?;
    membership_scoped(
        profile,
        allocator,
        payload,
        bytes,
        population,
        InitScope::Status,
    )
}

fn membership_scoped(
    profile: &Profile,
    allocator: &PageIdentity,
    payload: &PageIdentity,
    bytes: &[u8; PAGE_BYTES],
    population: AllocationPopulation,
    scope: InitScope,
) -> Result<PageLsn> {
    use SemanticError::*;
    require(scope.accepts_population(population), IdentityMismatch)?;
    require(
        payload.database == profile.spec().namespace.database
            && payload.owner.get() == scope.owner()
            && payload.owner_generation.get() == 1,
        IdentityMismatch,
    )?;
    let page = allocation_page::decode_allocation_page(bytes, *allocator, profile.spec().namespace)
        .map_err(|_| PageMismatch)?;
    require(
        payload.page.file == allocator.page.file && (1..64).contains(&payload.page.page_no),
        IdentityMismatch,
    )?;
    let mut members = 0;
    for index in 1..64 {
        match page.entry(index).map_err(|_| PageMismatch)? {
            AllocationEntry::Free(_) => (),
            AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind,
            } => {
                // Census every Allocated entry; wrong kind/generation is not filtered away.
                require(
                    object_generation.get() == 1
                        && ((object.get() == 2 && kind == PageKind::TransactionStatus)
                            || (population == AllocationPopulation::MixedSystem
                                && object.get() == 3
                                && kind == PageKind::Undo)),
                    IdentityMismatch,
                )?;
                require(generation.get() >= 2, IdentityMismatch)?;
                if allocator.page.page_no.checked_add(index as u64) == Some(payload.page.page_no) {
                    require(
                        generation == payload.page.incarnation
                            && object == payload.owner
                            && kind == scope.kind(),
                        IdentityMismatch,
                    )?;
                    members += 1;
                } else {
                    require(population != AllocationPopulation::Empty, IdentityMismatch)?;
                }
            }
            AllocationEntry::Reserved {
                claimant: ReservationOwner::System(work),
                ..
            } if population != AllocationPopulation::Empty
                && work.namespace() == profile.spec().namespace
                && work.kind() == SystemWorkKind::AllocationMaintenance
                && work.generation().get() == 1 => {}
            _ => return Err(StateMismatch),
        }
    }
    require(members == 1, InventoryMismatch)?;
    Ok(page.header().lsn)
}
/// No descriptor/WAL decoder is nested in this bounded two-page reader helper.
pub(super) fn observe_initialized(
    profile: Profile,
    allocator: PageIdentity,
    payload: PageIdentity,
    role: &StatusRole<'_>,
    allocator_bytes: &[u8; PAGE_BYTES],
    status_bytes: &[u8; PAGE_BYTES],
) -> Result<PageLsn> {
    let allocator_lsn = membership_lsn(
        &profile,
        &allocator,
        &payload,
        role,
        allocator_bytes,
        AllocationPopulation::Empty,
    )?;
    let status_lsn = status_lsn(status_bytes, &payload)?;
    require(
        allocator_lsn == status_lsn && status_lsn.0 != 0,
        SemanticError::OrderMismatch,
    )?;
    Ok(status_lsn)
}

pub(super) fn observe_operational_initialized(
    profile: Profile,
    allocator: PageIdentity,
    payload: PageIdentity,
    role: &StatusRole<'_>,
    allocator_bytes: &[u8; PAGE_BYTES],
    status_bytes: &[u8; PAGE_BYTES],
) -> Result<(PageLsn, PageLsn)> {
    let allocator_lsn = membership_lsn(
        &profile,
        &allocator,
        &payload,
        role,
        allocator_bytes,
        AllocationPopulation::Status,
    )?;
    let status_lsn = status_lsn(status_bytes, &payload)?;
    require(
        allocator_lsn.0 != 0 && status_lsn.0 != 0,
        SemanticError::OrderMismatch,
    )?;
    Ok((allocator_lsn, status_lsn))
}

/// Typed source-value census. Every contributing helper has a bounded phase;
/// these are logical owned values, not a compiler physical-stack measurement.
pub(super) fn status_codec_bytes() -> usize {
    4 * size_of::<StatusHome>()
        + 4 * size_of::<TransactionStatusCell>()
        + 2 * size_of::<StatusHomePage<'static>>()
        + 2 * size_of::<page::PageView<'static>>()
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<PageIdentity>()
        + 4 * size_of::<WalFrameHeader>()
        + 2 * size_of::<RedoRecordView<'static>>()
        + 2 * size_of::<FrameEnvelope<'static>>()
        + 3 * size_of::<Profile>()
        + 4 * size_of::<crate::api::Namespace>()
        + 1184
        + 640
        + budget::HASH
}
pub(super) fn plan_bytes() -> usize {
    budget::PLANS
        .max(
            6 * size_of::<InitPlan<'static, 'static>>()
                + 2 * size_of::<InitExpected<'static>>()
                + 128
                + 2 * size_of::<ScopedExpected<'static, 'static>>()
                + 2 * size_of::<PreparedInit<'static, 'static>>(),
        )
        .max(mixed::plan_bytes())
        .max(fixed::plan_bytes())
}
pub(super) fn temporary_bytes() -> usize {
    budget::TEMPORARIES
        .max(status_codec_bytes())
        .max(mixed::codec_bytes())
}
/// The reader has no descriptor/entry array or Status encoder. During page
/// checksum, the observer borrows Profile and both expected identities; the
/// allocation/page decoders retain two copied identities. One
/// PageView is moved through a helper rather than copied. The 512 carrier pool
/// includes twelve slice/reference slots, eight outer thin references, four
/// Namespace values and three ProfileSpec values; it is a whole-chain allowance,
/// not one per helper. Admission request values must be gone before this phase.
pub(super) fn reader_bytes() -> usize {
    let checksum = budget::HASH
        + size_of::<Profile>()
        + 2 * size_of::<PageIdentity>()
        + size_of::<page::PageView<'static>>()
        + 2 * size_of::<PageHeader>()
        + 64
        + 512
        + 128
        + 128
        + 128;
    let membership = size_of::<Profile>()
        + 2 * size_of::<allocation_page::AllocationPage<'static>>()
        + 8 * size_of::<AllocationEntry>()
        + 512
        + 128
        + 128
        + 128;
    let free_cell = size_of::<Profile>()
        + 4 * size_of::<StatusHome>()
        + 4 * size_of::<TransactionStatusCell>()
        + 2 * size_of::<PageHeader>()
        + 512
        + 128
        + 128
        + 128;
    // The caller retains both expected identities across decode and the two
    // checked slot-generation tokens in its read view. They are additional to
    // the callee inputs and nested decoder copies counted above.
    // Free Undo borrows its432-byte cell; its initial page decoder uses the
    // same two expected identity copies/checksum pool, then checks Generation
    // with scalar/error values bounded by the retained128 pools above.
    checksum
        .max(membership)
        .max(free_cell)
        .max(fixed::request_bytes())
        + size_of::<PageIdentity>()
        + size_of::<crate::storage::dirty_frame::TargetRequest>()
        + 2 * size_of::<u64>()
        + fixed::reader_layer_bytes()
}
pub(super) fn fits() -> bool {
    size_of::<InitPlan<'static, 'static>>() <= 512
        && size_of::<PreparedInit<'static, 'static>>() <= 256
        && size_of::<StatusHome>() <= 256
        && size_of::<TransactionStatusCell>() <= 224
        && size_of::<StatusHomePage<'static>>() <= 128
        && size_of::<page::PageView<'static>>() <= 128
        && size_of::<PageHeader>() <= 96
        && size_of::<PageIdentity>() <= 80
        && size_of::<RedoRecordView<'static>>() <= 128
        && size_of::<FrameEnvelope<'static>>() <= 256
        && size_of::<WalFrameHeader>() <= 128
        && size_of::<Profile>() <= 96
        && size_of::<crate::api::Namespace>() <= 32
        && status_codec_bytes() <= 8192
        && temporary_bytes() <= 16384
        && plan_bytes() <= 4096
        && mixed::plan_bytes() <= plan_bytes()
        && mixed::codec_bytes() <= status_codec_bytes()
        && 12 * size_of::<&[u8]>()
            + 8 * size_of::<&()>()
            + 4 * size_of::<crate::api::Namespace>()
            + 3 * size_of::<crate::storage::control::ProfileSpec>()
            <= 512
        && reader_bytes() <= 3648
}

#[cfg(test)]
#[path = "status_init_operation_tests.rs"]
mod tests;
