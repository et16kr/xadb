//! Closed plan4 reserve/cancel preparation on the original allocator algebra.
//! Reserve may follow unrelated WAL groups; its allocator prior LSN remains
//! the actual before image's LSN. A reservation's terminal is strictly next.
use super::*;
use crate::storage::session_fence::SessionFence;

pub(crate) const RETENTION_RESERVE_BYTES: usize = 21_200;

pub(crate) struct RetentionAllocationPlan<'p, 's> {
    pub(crate) profile: Profile,
    pub(crate) allocator: PageIdentity,
    pub(crate) prefix: wal::WalPrefix,
    pub(crate) high_water: u64,
    /// None/active0, Cancelled1, InitializedStatus2, InitializedUndoRoot3.
    pub(crate) terminal: u8,
    pub(crate) before_descriptor: Option<&'p [u8; 1536]>,
    /// Last accepted System INIT; current graph/provenance is checked by the caller.
    pub(crate) initialized: Option<(PageRef, PageKind, u64)>,
    pub(crate) fence: &'s SessionFence,
}
pub(crate) struct RetentionAllocationExpected<'p> {
    pub(crate) profile: Profile,
    pub(crate) allocator: PageIdentity,
    pub(crate) frame: FrameExpectation,
    pub(crate) high_water: u64,
    pub(crate) terminal: u8,
    pub(crate) before_descriptor: Option<&'p [u8; 1536]>,
    /// Last accepted System INIT; current graph/provenance is checked by the caller.
    pub(crate) initialized: Option<(PageRef, PageKind, u64)>,
}
impl<'p> RetentionAllocationPlan<'p, '_> {
    pub(crate) fn expected(&self) -> RetentionAllocationExpected<'p> {
        RetentionAllocationExpected {
            profile: self.profile,
            allocator: self.allocator,
            frame: FrameExpectation {
                stream: self.prefix.stream,
                previous_lsn: self.prefix.complete_lsn,
            },
            high_water: self.high_water,
            terminal: self.terminal,
            before_descriptor: self.before_descriptor,
            initialized: self.initialized,
        }
    }
}

/// Scalar observation only. The live linear reservation also borrows its
/// original owner and prevents every other append until initialize/cancel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetentionReservation {
    pub(crate) owner: SystemWorkRef,
    pub(crate) page: PageRef,
    pub(crate) birth: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetentionAllocationState {
    pub(crate) high_water: u64,
    pub(crate) terminal: u8,
    pub(crate) reserved: Option<RetentionReservation>,
}

fn analyze<'p>(
    expected: &RetentionAllocationExpected<'p>,
    before: &[u8; PAGE_BYTES],
    cancel: bool,
) -> Result<(AnalysisExpected<'p>, RetentionReservation)> {
    let namespace = expected.profile.spec().namespace;
    require(
        expected.frame.stream.database == namespace.database
            && expected.frame.stream.timeline == namespace.timeline,
        SemanticError::IdentityMismatch,
    )?;
    if expected.profile.format() == 2 {
        validate_catalog_base_allocation(
            before,
            expected.profile,
            expected.before_descriptor,
            expected.high_water,
            expected.terminal,
            expected.initialized,
        )?;
    } else {
        validate_plan4_system_base_allocation(
            before,
            expected.profile,
            expected.before_descriptor,
            expected.high_water,
            expected.terminal,
        )?;
    }
    let page = allocation_page::decode_allocation_page(before, expected.allocator, namespace)
        .map_err(|_| SemanticError::PageMismatch)?;
    require(
        page.header().lsn.0 <= expected.frame.previous_lsn,
        SemanticError::OrderMismatch,
    )?;
    let lsn = expected
        .frame
        .previous_lsn
        .checked_add(1)
        .ok_or(SemanticError::OrderMismatch)?;
    let mut selected = None;
    for index in 9..64 {
        match page.entry(index).map_err(|_| SemanticError::PageMismatch)? {
            AllocationEntry::Reserved {
                generation,
                claimant: ReservationOwner::System(owner),
            } => {
                require(cancel && selected.is_none(), SemanticError::StateMismatch)?;
                selected = Some((index, generation, owner, page.header().lsn.0));
            }
            AllocationEntry::Free(generation)
                if !cancel && selected.is_none() && generation.get() != u64::MAX =>
            {
                let owner = SystemWorkRef::new(
                    namespace,
                    allocation_page::SystemWorkKind::AllocationMaintenance,
                    expected
                        .high_water
                        .checked_add(1)
                        .ok_or(SemanticError::OrderMismatch)?,
                    Generation::new(1).expect("fixed generation"),
                )
                .map_err(|_| SemanticError::IdentityMismatch)?;
                selected = Some((
                    index,
                    Generation::new(generation.get() + 1).expect("checked next generation"),
                    owner,
                    lsn,
                ));
            }
            _ => (),
        }
    }
    let (index, generation, owner, birth) = selected.ok_or(SemanticError::StateMismatch)?;
    if cancel {
        require(
            expected.terminal == 0 && birth == expected.frame.previous_lsn,
            SemanticError::OrderMismatch,
        )?;
    }
    let reservation = RetentionReservation {
        owner,
        page: PageRef {
            page_no: index as u64,
            incarnation: generation,
            ..expected.allocator.page
        },
        birth,
    };
    Ok((
        AnalysisExpected {
            profile: expected.profile,
            identity: expected.allocator,
            frame: expected.frame,
            batch: Batch {
                owner,
                mask: 1u64 << index,
                birth,
            },
            cancel,
            before_descriptor: if cancel {
                Some(
                    expected
                        .before_descriptor
                        .ok_or(SemanticError::StateMismatch)?,
                )
            } else {
                None
            },
        },
        reservation,
    ))
}

pub(crate) fn retained_reservation(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
) -> Result<RetentionReservation> {
    analyze(expected, before, true).map(|(_, reservation)| reservation)
}

pub(crate) struct PreparedRetentionAllocation<'out, 's> {
    bytes: &'out [u8],
    allocator_after: &'out [u8; PAGE_BYTES],
    prefix: wal::WalPrefix,
    header: WalFrameHeader,
    state: RetentionAllocationState,
    fence: &'s SessionFence,
}
impl<'out, 's> PreparedRetentionAllocation<'out, 's> {
    pub(crate) fn header(&self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn frame(&self) -> &'out [u8] {
        self.bytes
    }
    pub(crate) fn allocator_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.allocator_after
    }
    pub(crate) fn descriptor_bytes(&self) -> &'out [u8; 1536] {
        self.bytes[352..1888]
            .try_into()
            .expect("validated descriptor span")
    }
    pub(crate) fn state(&self) -> RetentionAllocationState {
        self.state
    }
    pub(crate) fn stage_required<W: WriterNamespace>(
        self,
        session: &'s wal::WalRuntime<'_, W>,
    ) -> std::result::Result<wal::WalRequirement<'s>, wal::WalIoError> {
        session.prefix()?;
        if !std::ptr::eq(self.fence, &session.fenced) {
            return Err(wal::unchanged(
                wal::Phase::Stage,
                wal::Cause::IdentityMismatch,
            ));
        }
        session.stage_verified_required(self.prefix, self.header, self.bytes)
    }
}

fn state(reservation: RetentionReservation, cancel: bool) -> RetentionAllocationState {
    RetentionAllocationState {
        high_water: reservation.owner.id(),
        terminal: u8::from(cancel),
        reserved: (!cancel).then_some(reservation),
    }
}

fn prepare<'out, 's>(
    plan: &RetentionAllocationPlan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    after: &'out mut [u8; PAGE_BYTES],
    cancel: bool,
) -> Result<PreparedRetentionAllocation<'out, 's>> {
    let expected = plan.expected();
    let (analysis, reservation) = analyze(&expected, before, cancel)?;
    let header = encode_retention_prepared(
        Plan {
            profile: plan.profile,
            identity: plan.allocator,
            prefix: plan.prefix,
            batch: analysis.batch,
            cancel,
            before_descriptor: analysis.before_descriptor,
        },
        before,
        output,
        after,
        OperationBudget::MixedInit114688,
    )?;
    let bytes = &output[..RETENTION_RESERVE_BYTES];
    validate(&expected, before, bytes, after, cancel)?;
    Ok(PreparedRetentionAllocation {
        bytes,
        allocator_after: after,
        prefix: plan.prefix,
        header,
        state: state(reservation, cancel),
        fence: plan.fence,
    })
}

pub(crate) fn prepare_retention_reserve<'out, 's>(
    plan: &RetentionAllocationPlan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    after: &'out mut [u8; PAGE_BYTES],
) -> Result<PreparedRetentionAllocation<'out, 's>> {
    prepare(plan, before, output, after, false)
}
pub(crate) fn prepare_retention_cancel<'out, 's>(
    plan: &RetentionAllocationPlan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    after: &'out mut [u8; PAGE_BYTES],
) -> Result<PreparedRetentionAllocation<'out, 's>> {
    prepare(plan, before, output, after, true)
}

fn validate(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
    cancel: bool,
) -> Result<RetentionAllocationState> {
    let (analysis, reservation) = analyze(expected, before, cancel)?;
    let h = validate_header_scoped(&analysis, bytes, true)?;
    let descriptor = obligation::decode_descriptor(
        &bytes[352..1888],
        ReservationOwner::System(analysis.batch.owner),
        profile_ref(expected.profile),
    )
    .map_err(descriptor_error)?;
    validate_descriptor(&analysis, bytes, h, &descriptor)?;
    validate_retention_transition(
        &analysis,
        before,
        bytes,
        after,
        h,
        &descriptor,
        OperationBudget::MixedInit114688,
    )?;
    Ok(state(reservation, cancel))
}
pub(crate) fn validate_retention_reserve(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
) -> Result<()> {
    validate(expected, before, bytes, after, false).map(|_| ())
}
pub(crate) fn validate_retention_cancel(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &[u8; PAGE_BYTES],
) -> Result<()> {
    validate(expected, before, bytes, after, true).map(|_| ())
}

fn materialize(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &mut [u8; PAGE_BYTES],
    cancel: bool,
) -> Result<RetentionAllocationState> {
    {
        let (analysis, _) = analyze(expected, before, cancel)?;
        let header = validate_header_scoped(&analysis, bytes, true)?;
        let old = allocation_page::decode_allocation_page(
            before,
            expected.allocator,
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
    }
    validate(expected, before, bytes, after, cancel)
}
pub(crate) fn materialize_retention_reserve(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &mut [u8; PAGE_BYTES],
) -> Result<RetentionAllocationState> {
    materialize(expected, before, bytes, after, false)
}
pub(crate) fn materialize_retention_cancel(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    after: &mut [u8; PAGE_BYTES],
) -> Result<RetentionAllocationState> {
    materialize(expected, before, bytes, after, true)
}

pub(crate) fn retention_allocation_source_bytes() -> usize {
    use std::mem::size_of;
    // The full-population preflight returns before the encoder or transition
    // retains its descriptor. These are separate nested peaks, not concurrent
    // descriptor/entry closures. All new outer/result values are added below.
    super::plan4_system_base_allocation_source_bytes()
        .max(super::super::budget::PLANS + super::super::budget::TEMPORARIES)
        + 4 * size_of::<RetentionAllocationPlan<'_, '_>>()
        + 6 * size_of::<RetentionAllocationExpected<'_>>()
        + 4 * size_of::<RetentionReservation>()
        + 4 * size_of::<RetentionAllocationState>()
        + 4 * size_of::<Result<RetentionAllocationState>>()
        + 2 * size_of::<PreparedRetentionAllocation<'_, '_>>()
        + 2 * size_of::<Result<PreparedRetentionAllocation<'_, '_>>>()
        + 2 * size_of::<Result<(AnalysisExpected<'_>, RetentionReservation)>>()
        + 3 * size_of::<Result<RetentionReservation>>()
        + 1024
}

/// Actual replay materialization, excluding live producer Plan/Prepared return
/// alternatives. The first analyze/encode scope has ended before validate
/// analyzes the same immutable predecessor again; no check is omitted.
pub(crate) fn retention_materialize_source_bytes() -> usize {
    use std::mem::size_of;
    super::plan4_system_base_allocation_source_bytes().max(super::retention_readonly_source_bytes())
        + 2 * size_of::<RetentionAllocationExpected<'_>>()
        + 3 * size_of::<AnalysisExpected<'_>>()
        + 3 * size_of::<Result<(AnalysisExpected<'_>, RetentionReservation)>>()
        + 3 * size_of::<RetentionReservation>()
        + 2 * size_of::<Result<RetentionReservation>>()
        + 2 * size_of::<RetentionAllocationState>()
        + 2 * size_of::<Result<RetentionAllocationState>>()
        + 2 * size_of::<AllocationPage<'_>>()
        + 2 * size_of::<WalFrameHeader>()
        + 16 * size_of::<&[u8]>()
        + 16 * size_of::<u64>()
        + 4 * size_of::<Result<()>>()
        + 512
}

/// INIT context calls only analyze -> map(reservation). It does not materialize
/// a reserve/cancel frame or return their State/Header. Keep all pair/extracted
/// reservation and expected handoff carriers; the caller counts InitExpected.
pub(crate) fn retained_reservation_source_bytes() -> usize {
    use std::mem::size_of;
    super::plan4_system_base_allocation_source_bytes()
        + 2 * size_of::<RetentionAllocationExpected<'_>>()
        + 3 * size_of::<AnalysisExpected<'_>>()
        + 3 * size_of::<Result<(AnalysisExpected<'_>, RetentionReservation)>>()
        + 3 * size_of::<RetentionReservation>()
        + 2 * size_of::<Result<RetentionReservation>>()
        + 2 * size_of::<AllocationPage<'_>>()
        + 16 * size_of::<&[u8]>() + 16 * size_of::<u64>()
        + 4 * size_of::<Result<()>>()
        // Namespace, selected Option/tuple and destructured tuple, PageRef,
        // SystemWorkRef construction/Result and generation/error values.
        + 512
}
