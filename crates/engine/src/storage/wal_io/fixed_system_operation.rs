//! Internal core entry points selected by an actual retained bootstrap role.
//! Old Status and mixed public producers retain their separate typed APIs.

use super::*;
use crate::storage::{dirty_frame::TargetRequest, wal_io::bootstrap::FixedSystemRole};

pub(in super::super) struct Plan<'p, 's> {
    pub(in super::super) profile: Profile,
    pub(in super::super) allocator: PageIdentity,
    pub(in super::super) payload: PageIdentity,
    pub(in super::super) prefix: WalPrefix,
    pub(in super::super) batch: Batch,
    pub(in super::super) before_descriptor: &'p [u8; 1536],
    pub(in super::super) role: &'p FixedSystemRole<'s>,
    pub(in super::super) kind: mixed::Kind,
    pub(in super::super) population: AllocationPopulation,
}

fn scope(
    role: &FixedSystemRole<'_>,
    kind: mixed::Kind,
    population: AllocationPopulation,
) -> Result<InitScope> {
    if role.mixed() {
        require(
            population == AllocationPopulation::MixedSystem,
            SemanticError::IdentityMismatch,
        )?;
        Ok(InitScope::Mixed(kind))
    } else {
        require(
            kind == mixed::Kind::Status
                && matches!(
                    population,
                    AllocationPopulation::Empty | AllocationPopulation::Status
                ),
            SemanticError::IdentityMismatch,
        )?;
        Ok(InitScope::Status)
    }
}

pub(in super::super) fn request(
    role: &FixedSystemRole<'_>,
    page: page::PageRef,
    kind: mixed::Kind,
) -> Result<TargetRequest> {
    let population = if role.mixed() {
        AllocationPopulation::MixedSystem
    } else {
        AllocationPopulation::Status
    };
    let scope = scope(role, kind, population)?;
    Ok(TargetRequest {
        identity: PageIdentity {
            database: role.namespace().database,
            page,
            owner: xadb_foundation::identity::ObjectId::new(scope.owner()).expect("fixed object"),
            owner_generation: Generation::new(1).expect("fixed generation"),
        },
        kind: scope.kind(),
        level: 0,
    })
}

pub(in super::super) struct FixedPrepared<'out, 's> {
    inner: PreparedInit<'out, 's>,
}
impl<'out, 's> FixedPrepared<'out, 's> {
    pub(in super::super) fn descriptor_bytes(&self) -> &'out [u8; 1536] {
        self.inner.descriptor_bytes()
    }
    pub(in super::super) fn allocator_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.inner.allocator_after()
    }
    pub(in super::super) fn status_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.inner.status_after()
    }
    pub(in super::super) fn stage_required<W: WriterNamespace>(
        self,
        session: &'s GenesisWalSession<'_, W>,
    ) -> std::result::Result<WalRequirement<'s>, wal::WalIoError> {
        self.inner.stage_required(session)
    }
}

pub(in super::super) fn prepare<'out, 's>(
    plan: &Plan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    payload_after: &'out mut [u8; PAGE_BYTES],
) -> Result<FixedPrepared<'out, 's>> {
    prepare_inner(plan, before, output, allocator_after, payload_after)
        .map(|inner| FixedPrepared { inner })
}

// All owned Expected/ScopedExpected values end before the public wrapper maps
// the core result into its separate sealed FixedPrepared type.
fn prepare_inner<'out, 's>(
    plan: &Plan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    payload_after: &'out mut [u8; PAGE_BYTES],
) -> Result<PreparedInit<'out, 's>> {
    require(
        plan.role.namespace() == plan.profile.spec().namespace,
        SemanticError::IdentityMismatch,
    )?;
    let selected = scope(plan.role, plan.kind, plan.population)?;
    let expected = InitExpected {
        profile: plan.profile,
        allocator: plan.allocator,
        payload: plan.payload,
        frame: FrameExpectation {
            stream: plan.prefix.stream,
            previous_lsn: plan.prefix.complete_lsn,
        },
        batch: plan.batch,
        before_descriptor: plan.before_descriptor,
    };
    prepare_scoped(
        &ScopedExpected {
            context: &expected,
            scope: selected,
        },
        plan.prefix,
        plan.role.fence(),
        before,
        output,
        allocator_after,
        payload_after,
        plan.population,
    )
}

pub(in super::super) fn membership(
    profile: &Profile,
    allocator: &PageIdentity,
    payload: &PageIdentity,
    role: &FixedSystemRole<'_>,
    bytes: &[u8; PAGE_BYTES],
    population: AllocationPopulation,
    kind: mixed::Kind,
) -> Result<PageLsn> {
    require(
        role.namespace() == profile.spec().namespace,
        SemanticError::IdentityMismatch,
    )?;
    membership_scoped(
        profile,
        allocator,
        payload,
        bytes,
        population,
        scope(role, kind, population)?,
    )
}

pub(in super::super) fn observe(
    profile: &Profile,
    allocator: &PageIdentity,
    payload: &TargetRequest,
    role: &FixedSystemRole<'_>,
    allocator_bytes: &[u8; PAGE_BYTES],
    payload_bytes: &[u8; PAGE_BYTES],
    operational: bool,
) -> Result<(PageLsn, PageLsn)> {
    let kind = if role.mixed() && payload.kind == PageKind::Undo {
        mixed::Kind::UndoRoot
    } else {
        mixed::Kind::Status
    };
    require(
        request(role, payload.identity.page, kind)? == *payload,
        SemanticError::IdentityMismatch,
    )?;
    let population = if role.mixed() {
        AllocationPopulation::MixedSystem
    } else if operational {
        AllocationPopulation::Status
    } else {
        AllocationPopulation::Empty
    };
    let allocator_lsn = membership(
        profile,
        allocator,
        &payload.identity,
        role,
        allocator_bytes,
        population,
        kind,
    )?;
    let payload_lsn = scope(role, kind, population)?.page_lsn(payload_bytes, &payload.identity)?;
    require(
        allocator_lsn.0 != 0 && payload_lsn.0 != 0 && (operational || allocator_lsn == payload_lsn),
        SemanticError::OrderMismatch,
    )?;
    Ok((allocator_lsn, payload_lsn))
}

pub(in super::super) fn finished(
    profile: Profile,
    batch: Batch,
    bytes: &[u8; 1536],
    lsn: PageLsn,
    role: &FixedSystemRole<'_>,
    kind: mixed::Kind,
    population: AllocationPopulation,
) -> Result<TerminalOutcome> {
    validate_finished_scoped(profile, batch, bytes, lsn, scope(role, kind, population)?)?;
    Ok(if role.mixed() {
        kind.terminal()
    } else {
        TerminalOutcome::InitializedStatus
    })
}

pub(in super::super) fn plan_bytes() -> usize {
    let retained = 6 * size_of::<Plan<'static, 'static>>() + 128;
    let inner = retained
        + 2 * size_of::<InitExpected<'static>>()
        + 2 * size_of::<ScopedExpected<'static, 'static>>()
        + 2 * size_of::<PreparedInit<'static, 'static>>()
        + 2 * size_of::<Result<PreparedInit<'static, 'static>>>();
    let mapping = retained
        + 2 * size_of::<PreparedInit<'static, 'static>>()
        + 2 * size_of::<Result<PreparedInit<'static, 'static>>>()
        + 2 * size_of::<FixedPrepared<'static, 'static>>()
        + 2 * size_of::<Result<FixedPrepared<'static, 'static>>>();
    inner.max(mapping)
}

/// No checksum/entry decoder overlaps the closed request/scope comparison.
pub(in super::super) fn request_bytes() -> usize {
    2 * size_of::<TargetRequest>()
        + 2 * size_of::<Result<TargetRequest>>()
        + 2 * size_of::<PageIdentity>()
        + 2 * size_of::<page::PageRef>()
        + 2 * size_of::<Profile>()
        + 2 * size_of::<crate::storage::control::ProfileSpec>()
        + 4 * size_of::<crate::api::Namespace>()
        + 2 * size_of::<Result<InitScope>>()
        + size_of::<(
            &FixedSystemRole<'_>,
            mixed::Kind,
            AllocationPopulation,
            InitScope,
        )>()
        + 128
}
pub(in super::super) fn reader_layer_bytes() -> usize {
    size_of::<(
        &Profile,
        &PageIdentity,
        &TargetRequest,
        InitScope,
        mixed::Kind,
        AllocationPopulation,
    )>() + 2 * size_of::<Result<InitScope>>()
}
