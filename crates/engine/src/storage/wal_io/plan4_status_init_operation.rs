//! Plan4's adjacent reserve-one -> Status INIT terminal. The current allocator
//! population is checked by the same base bridge; all INIT bytes/descriptor/
//! redo validation reuse the original Status implementation without extending
//! its old population or role authority.
use super::*;
use foundation::identity::ObjectId;
use operation::retention::{
    RetentionAllocationExpected, RetentionAllocationPlan, RetentionAllocationState,
    RetentionReservation, retained_reservation,
};

pub(crate) const RETENTION_INIT_BYTES: usize = FRAME_BYTES;

fn context<'p>(
    expected: &RetentionAllocationExpected<'p>,
    before: &[u8; PAGE_BYTES],
) -> Result<InitExpected<'p>> {
    let reservation = retained_reservation(expected, before)?;
    let context = InitExpected {
        profile: expected.profile,
        allocator: expected.allocator,
        payload: PageIdentity {
            database: expected.profile.spec().namespace.database,
            page: reservation.page,
            owner: ObjectId::new(2).expect("fixed Status object"),
            owner_generation: Generation::new(1).expect("fixed generation"),
        },
        frame: expected.frame,
        batch: Batch {
            owner: reservation.owner,
            mask: 1u64 << reservation.page.page_no,
            birth: reservation.birth,
        },
        before_descriptor: expected
            .before_descriptor
            .ok_or(SemanticError::StateMismatch)?,
    };
    plan_index(&ScopedExpected {
        context: &context,
        scope: InitScope::Status,
    })?;
    Ok(context)
}

/// The original owner supplies the fence and consumes this sealed preparation
/// through its original WAL runtime. These borrowed byte outputs confer no
/// installation, pool membership or recovered-owner authority on their own.
pub(crate) struct PreparedRetentionInit<'out, 's> {
    bytes: &'out [u8],
    allocator_after: &'out [u8; PAGE_BYTES],
    status_after: &'out [u8; PAGE_BYTES],
    prefix: WalPrefix,
    header: WalFrameHeader,
    state: RetentionAllocationState,
    fence: &'s SessionFence,
}
impl<'out, 's> PreparedRetentionInit<'out, 's> {
    pub(crate) fn header(&self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn frame(&self) -> &'out [u8] {
        self.bytes
    }
    pub(crate) fn allocator_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.allocator_after
    }
    pub(crate) fn status_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.status_after
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
        session: &'s WalRuntime<'_, W>,
    ) -> std::result::Result<WalRequirement<'s>, wal::WalIoError> {
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

fn state(context: &InitExpected<'_>) -> RetentionAllocationState {
    RetentionAllocationState {
        high_water: context.batch.owner.id(),
        terminal: 2,
        reserved: None,
    }
}

pub(crate) fn prepare_retention_init<'out, 's>(
    plan: &RetentionAllocationPlan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    status_after: &'out mut [u8; PAGE_BYTES],
) -> Result<PreparedRetentionInit<'out, 's>> {
    let expected = plan.expected();
    let context = context(&expected, before)?;
    let scoped = ScopedExpected {
        context: &context,
        scope: InitScope::Status,
    };
    let h = header(&scoped)?;
    let bytes = &mut output[..RETENTION_INIT_BYTES];
    bytes.fill(0);
    encode_allocator(&scoped, before, h.lsn, bytes, allocator_after)?;
    encode_finished_descriptor(
        &context.profile,
        context.batch,
        PageLsn(h.lsn),
        &mut bytes[352..1888],
        InitScope::Status,
    )?;
    InitScope::Status.encode_page(&context.payload, PageLsn(h.lsn), status_after)?;
    encode_group(&scoped, h, before, bytes, allocator_after, status_after)?;
    validate_preflighted(&scoped, before, bytes, allocator_after, status_after)?;
    Ok(PreparedRetentionInit {
        bytes,
        allocator_after,
        status_after,
        prefix: plan.prefix,
        header: h,
        state: state(&context),
        fence: plan.fence,
    })
}

fn validate_preflighted(
    expected: &ScopedExpected<'_, '_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &[u8; PAGE_BYTES],
    status_after: &[u8; PAGE_BYTES],
) -> Result<()> {
    let h = validate_header(expected, bytes)?;
    validate_finished_scoped(
        expected.profile,
        expected.batch,
        bytes[352..1888]
            .try_into()
            .expect("validated header length"),
        PageLsn(h.lsn),
        InitScope::Status,
    )?;
    validate_allocator_transition(expected, before, bytes, allocator_after, h)?;
    validate_status_transition(expected, bytes, status_after, h)
}

pub(crate) fn validate_retention_init(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &[u8; PAGE_BYTES],
    status_after: &[u8; PAGE_BYTES],
) -> Result<()> {
    let context = context(expected, before)?;
    validate_preflighted(
        &ScopedExpected {
            context: &context,
            scope: InitScope::Status,
        },
        before,
        bytes,
        allocator_after,
        status_after,
    )
}

/// Materialization observes a complete original frame into discardable caller
/// pages. It accepts only the immediately preceding reservation; neither newer
/// physical homes nor a reconstructed scalar state can authorize live apply.
pub(crate) fn materialize_retention_init(
    expected: &RetentionAllocationExpected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &mut [u8; PAGE_BYTES],
    status_after: &mut [u8; PAGE_BYTES],
) -> Result<RetentionAllocationState> {
    let context = context(expected, before)?;
    let scoped = ScopedExpected {
        context: &context,
        scope: InitScope::Status,
    };
    let h = validate_header(&scoped, bytes)?;
    {
        let old = allocation_page::decode_allocation_page(
            before,
            expected.allocator,
            expected.profile.spec().namespace,
        )
        .map_err(|_| SemanticError::PageMismatch)?;
        allocation_page::encode_allocation_page(
            PageHeader {
                lsn: PageLsn(h.lsn),
                ..old.header()
            },
            expected.profile.spec().namespace,
            &bytes[NEW_CELL],
            allocator_after,
        )
        .map_err(|_| SemanticError::PageMismatch)?;
    }
    status_after.copy_from_slice(&bytes[INIT.start + 64..INIT.end]);
    validate_preflighted(&scoped, before, bytes, allocator_after, status_after)?;
    Ok(state(&context))
}

/// New-only actual forwarding/return carriers plus the original bounded INIT
/// closure. Caller frame/after-page loans and the live owner's retained values
/// must be charged by that caller; this is not an independent capacity permit.
pub(crate) fn retention_init_source_bytes() -> usize {
    operation::retention::retention_allocation_source_bytes().max(temporary_bytes() + plan_bytes())
        + 2 * size_of::<RetentionAllocationPlan<'_, '_>>()
        + 3 * size_of::<RetentionAllocationExpected<'_>>()
        + 4 * size_of::<InitExpected<'_>>()
        + 4 * size_of::<ScopedExpected<'_, '_>>()
        + 2 * size_of::<RetentionReservation>()
        + 3 * size_of::<Result<InitExpected<'_>>>()
        + 2 * size_of::<PreparedRetentionInit<'_, '_>>()
        + 2 * size_of::<Result<PreparedRetentionInit<'_, '_>>>()
        + 3 * size_of::<RetentionAllocationState>()
        + 3 * size_of::<Result<RetentionAllocationState>>()
        + 512
}
