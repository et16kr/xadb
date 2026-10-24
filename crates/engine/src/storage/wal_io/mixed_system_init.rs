//! Closed plan3 Status/Free-Undo INIT preparation over the original core.
//! This prepares sealed bytes/images, not allocation, shared-pool installation
//! or recovery authority. The live owner must supply its actual reservation.

use super::{
    AllocationPopulation, Batch, FrameExpectation, InitExpected, InitScope, PAGE_BYTES,
    PageIdentity, PageKind, PreparedInit, Profile, Result, ScopedExpected, SemanticError,
    WalPrefix, WalRequirement, WalRuntime, materialize_scoped, prepare_scoped, require,
    validate_expected_in_population,
};
use crate::storage::wal_io::{UndoRole, WalIoError};
use xadb_foundation::namespace::WriterNamespace;

pub(in super::super) use super::operation::MixedInitKind as Kind;

pub(in super::super) struct Plan<'p, 's> {
    pub(in super::super) profile: Profile,
    pub(in super::super) allocator: PageIdentity,
    pub(in super::super) payload: PageIdentity,
    pub(in super::super) prefix: WalPrefix,
    pub(in super::super) batch: Batch,
    pub(in super::super) before_descriptor: &'p [u8; 1536],
    pub(in super::super) role: &'p UndoRole<'s>,
    pub(in super::super) kind: Kind,
}

/// Trusted canonical geometry for pure observation/materialization. The private
/// inner scope cannot be passed to the old Status observer or become a permit.
pub(in super::super) struct Expected<'p> {
    inner: InitExpected<'p>,
    kind: Kind,
}
impl<'p> Expected<'p> {
    pub(in super::super) fn new(
        profile: Profile,
        allocator: PageIdentity,
        payload: PageIdentity,
        frame: FrameExpectation,
        batch: Batch,
        before_descriptor: &'p [u8; 1536],
        kind: Kind,
    ) -> Self {
        Self {
            inner: InitExpected {
                profile,
                allocator,
                payload,
                frame,
                batch,
                before_descriptor,
            },
            kind,
        }
    }
    fn scoped(&self) -> ScopedExpected<'_, 'p> {
        ScopedExpected {
            context: &self.inner,
            scope: InitScope::Mixed(self.kind),
        }
    }
}
impl<'p> Plan<'p, '_> {
    fn expected(&self) -> Result<Expected<'p>> {
        require(
            self.role.namespace() == self.profile.spec().namespace
                && self.role.object().get() == 3
                && self.role.generation().get() == 1
                && self.role.kind() == PageKind::Undo
                && self.role.level() == 0
                && self.role.issued_object_high_water() == 3,
            SemanticError::IdentityMismatch,
        )?;
        Ok(Expected::new(
            self.profile,
            self.allocator,
            self.payload,
            FrameExpectation {
                stream: self.prefix.stream,
                previous_lsn: self.prefix.complete_lsn,
            },
            self.batch,
            self.before_descriptor,
            self.kind,
        ))
    }
}

/// No Copy/Clone, public fields or conversion to the old Status prepared type.
/// Stage consumes the exact immutable output/image loans and original fence.
pub(in super::super) struct Prepared<'out, 's> {
    inner: PreparedInit<'out, 's>,
}
impl<'out, 's> Prepared<'out, 's> {
    pub(in super::super) fn descriptor_bytes(&self) -> &'out [u8; 1536] {
        self.inner.descriptor_bytes()
    }
    pub(in super::super) fn allocator_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.inner.allocator_after()
    }
    pub(in super::super) fn payload_after(&self) -> &'out [u8; PAGE_BYTES] {
        self.inner.status_after()
    }
    pub(in super::super) fn stage_required<W: WriterNamespace>(
        self,
        session: &'s WalRuntime<'_, W>,
    ) -> std::result::Result<WalRequirement<'s>, WalIoError> {
        self.inner.stage_required(session)
    }
}

/// Errors leave immutable inputs unchanged; output scratch is discardable.
pub(in super::super) fn prepare<'out, 's>(
    plan: &Plan<'_, 's>,
    before: &[u8; PAGE_BYTES],
    output: &'out mut [u8; 65536],
    allocator_after: &'out mut [u8; PAGE_BYTES],
    payload_after: &'out mut [u8; PAGE_BYTES],
) -> Result<Prepared<'out, 's>> {
    let expected = plan.expected()?;
    prepare_scoped(
        &expected.scoped(),
        plan.prefix,
        plan.role.fence(),
        before,
        output,
        allocator_after,
        payload_after,
        AllocationPopulation::MixedSystem,
    )
    .map(|inner| Prepared { inner })
}

pub(in super::super) fn validate(
    expected: &Expected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &[u8; PAGE_BYTES],
    payload_after: &[u8; PAGE_BYTES],
) -> Result<()> {
    validate_expected_in_population(
        &expected.scoped(),
        before,
        bytes,
        allocator_after,
        payload_after,
        AllocationPopulation::MixedSystem,
    )
}

/// Pure canonical materialization; no bootstrap selection or replay permit.
pub(in super::super) fn materialize(
    expected: &Expected<'_>,
    before: &[u8; PAGE_BYTES],
    bytes: &[u8],
    allocator_after: &mut [u8; PAGE_BYTES],
    payload_after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    materialize_scoped(
        &expected.scoped(),
        before,
        bytes,
        allocator_after,
        payload_after,
        AllocationPopulation::MixedSystem,
    )
}

/// Conservative source-value phases for the new borrowed wrapper. The largest
/// common descriptor path is unchanged: closed kind removes nested large
/// Result<DescriptorSpec> carriers. Values here are not physical stack or RSS.
pub(in super::super) fn plan_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<Plan<'static, 'static>>()
        + 3 * size_of::<Expected<'static>>()
        + 2 * size_of::<Result<Expected<'static>>>()
        + 2 * size_of::<Prepared<'static, 'static>>()
        + 2 * size_of::<Result<Prepared<'static, 'static>>>()
        + 4 * size_of::<ScopedExpected<'static, 'static>>()
        + 128
}
pub(in super::super) fn codec_bytes() -> usize {
    use crate::storage::page::{PageHeader, PageView};
    use std::mem::size_of;
    use xadb_foundation::identity::Generation;
    // Same retained WAL/redo/profile metadata as status_codec_bytes, with the
    // Status record/cell representations replaced by the one Free Undo body.
    432 + 4 * size_of::<Generation>()
        + 2 * size_of::<PageView<'static>>()
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<PageIdentity>()
        + 4 * size_of::<super::WalFrameHeader>()
        + 2 * size_of::<super::RedoRecordView<'static>>()
        + 2 * size_of::<super::FrameEnvelope<'static>>()
        + 3 * size_of::<Profile>()
        + 4 * size_of::<crate::api::Namespace>()
        + 1184
        + 640
        + super::budget::HASH
}
