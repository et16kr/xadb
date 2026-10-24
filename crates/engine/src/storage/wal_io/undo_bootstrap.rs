//! Immutable Undo root-container role for a new plan3 namespace.
//! This does not authorize root/chunk INIT, membership or transaction mutation.

use super::{
    BootstrapError, BootstrapPlanKind, Cause, FreshStorageSession, MutableStorageError, Phase,
    Result, StatusRole,
};
use crate::{
    api::Namespace,
    storage::{page::PageKind, session_fence::SessionFence, wal_io::MAX_FRAME_BYTES},
};
use std::mem::size_of;
use xadb_foundation::{
    error::Effect,
    identity::{Generation, ObjectId},
    namespace::WriterNamespace,
};

/// A non-Copy borrow of the installed plan and its original session fence.
/// PageKind::Undo also encodes chunks, but this role's current consumer contract
/// is limited to root containers; kind alone cannot authorize any page content.
pub(crate) struct UndoRole<'s> {
    status: StatusRole<'s>,
}
impl<'s> UndoRole<'s> {
    pub(crate) fn namespace(&self) -> Namespace {
        self.status.namespace()
    }
    pub(crate) fn object(&self) -> ObjectId {
        ObjectId::new(3).expect("fixed nonzero undo object")
    }
    pub(crate) fn generation(&self) -> Generation {
        Generation::new(1).expect("fixed nonzero undo generation")
    }
    pub(crate) fn kind(&self) -> PageKind {
        PageKind::Undo
    }
    pub(crate) fn level(&self) -> u8 {
        0
    }
    pub(crate) fn issued_object_high_water(&self) -> u64 {
        self.status.issued_object_high_water()
    }
    pub(crate) fn uses_fence(&self, fence: &SessionFence) -> bool {
        self.status.uses_fence(fence)
    }
    /// Original fence for the sealed mixed INIT producer; no inner StatusRole
    /// is exposed and this borrow cannot outlive the installed session.
    pub(in crate::storage::wal_io) fn fence(&self) -> &'s SessionFence {
        self.status.fence
    }
}

impl<'m, W: WriterNamespace> FreshStorageSession<'m, W> {
    /// Install plan3's immutable Status2 and Undo3 identities. Failure preserves
    /// completed effects and never returns a live role or fresh-resume permit.
    pub(crate) fn create_undo(
        writer: W,
        profile_id: u32,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        scratch: &mut [[u8; super::PAGE_BYTES]; 3],
    ) -> Result<Self> {
        Self::create_for_kind(
            writer,
            profile_id,
            slot,
            scratch,
            BootstrapPlanKind::UndoBootstrap,
        )
    }

    /// Original poison/fence checks precede the exact plan3 requirement.
    pub(crate) fn undo_role(&self) -> Result<UndoRole<'_>> {
        let status = self.status_role()?;
        if self.plan.kind != BootstrapPlanKind::UndoBootstrap {
            return Err(BootstrapError {
                phase: Phase::Role,
                effect: Effect::Unchanged,
                cause: Cause::UnsupportedPlan,
            });
        }
        Ok(UndoRole { status })
    }
}

// Conservative source-owned call census, not physical stack/RSS. The completed
// StatusRole is retained by UndoRole; count its copies and all return carriers
// in addition to the original health/StatusRole call, without new payload pages.
#[allow(
    clippy::manual_bits,
    reason = "eight source-owned scalar slots, counted in bytes"
)]
pub(super) fn role_bytes(status_role_bytes: usize) -> usize {
    status_role_bytes
        + 2 * size_of::<UndoRole<'_>>()
        + 3 * size_of::<Result<UndoRole<'_>>>()
        + size_of::<BootstrapError>()
        + 8 * size_of::<usize>()
}
pub(super) fn mutable_role_bytes(status_role_bytes: usize) -> usize {
    role_bytes(status_role_bytes)
        + 2 * size_of::<MutableStorageError>()
        + 2 * size_of::<std::result::Result<UndoRole<'_>, MutableStorageError>>()
        + size_of::<BootstrapError>()
        + 12 * size_of::<usize>()
}
