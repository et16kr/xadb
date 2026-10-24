//! Latest live Status replacement through the original clean-residency gate.
use super::super::super::super::super::{
    super::super::allocation_reservation::operation::AllocationPopulation, operation,
};
use super::super::super::super::operational::load::StatusLoadObservation;
use super::*;
use crate::storage::{dirty_frame::StatusLoadSnapshot, wal_io::StatusLoadPermit};

/// Only a complete live directory/history/final-read check constructs this.
/// The original snapshot and immutable caller loans survive through install.
pub(in super::super::super::super) struct EmptyLoadInstall<'a, 'c, 's> {
    snapshot: &'a StatusLoadSnapshot<'c, 's>,
    index: usize,
    allocator: &'a [u8; PAGE_BYTES],
    bytes: &'a [u8; PAGE_BYTES],
    request: TargetRequest,
    lsn: PageLsn,
}
impl<'a, 'c, 's> EmptyLoadInstall<'a, 'c, 's> {
    pub(in super::super::super::super) fn snapshot(&self) -> &'a StatusLoadSnapshot<'c, 's> {
        self.snapshot
    }
    pub(in super::super::super::super) fn index(&self) -> usize {
        self.index
    }
    pub(in super::super::super::super) fn allocator(&self) -> &'a [u8; PAGE_BYTES] {
        self.allocator
    }
    pub(in super::super::super::super) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }
    pub(in super::super::super::super) fn request(&self) -> TargetRequest {
        self.request
    }
    pub(in super::super::super::super) fn lsn(&self) -> PageLsn {
        self.lsn
    }
}

impl<'s, 'c, W: WriterNamespace> EmptyTransactions<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    #[cfg(test)]
    pub(crate) fn load_test_snapshot(
        &self,
        index: usize,
        bytes: &mut [u8; PAGE_BYTES],
    ) -> FrameStamp {
        dirty_frame::test_support::diagnose(&self.inner.inner.inner.pool, index, bytes).frame
    }
    #[cfg(test)]
    pub(crate) fn load_test_layout() -> (usize, usize, bool) {
        (
            call_bytes::<W>(),
            super::super::retained_bytes::<W>(),
            fits::<W>(),
        )
    }
    /// Replace one clean Status frame with an exact latest nonresident home.
    /// Read errors preserve the old residency; source drift fences the owner.
    pub(crate) fn load_into(
        &mut self,
        owner: &mut Owner<'c, 's>,
        index: usize,
        page: PageRef,
    ) -> Result<StatusLoadObservation> {
        let storage = self.inner.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.health()?;
        if !fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        if !owner
            .check_binding(self.inner.inner.inner.pool.bound_core())
            .map_err(|e| frame(e.into()))?
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?;
        let ledger = ledger(self.bank, storage.profile(), prefix, self.adopted_local)
            .ok_or_else(|| fatal(storage, Cause::Invariant))?;
        let bound = &mut self.inner.inner.inner;
        let snapshot = bound
            .pool
            .prepare_status_load_at(
                index,
                (&mut bound.control.base.operation[65536..81920])
                    .try_into()
                    .expect("allocator loan"),
            )
            .map_err(frame)?;
        bound
            .control
            .base
            .frozen_observation(
                storage.profile(),
                prefix,
                &snapshot.allocator(),
                self.frozen_system_lsn,
            )
            .map_err(|e| fatal(storage, Cause::Allocation(e)))?;
        let allocator_digest = content_digest(bound.control.base.before());
        let request = TargetRequest {
            identity: PageIdentity {
                database: bound.control.role.namespace().database,
                page,
                owner: bound.control.role.object(),
                owner_generation: bound.control.role.generation(),
            },
            kind: PageKind::TransactionStatus,
            level: 0,
        };
        operation::membership_lsn(
            &storage.profile(),
            &snapshot.allocator().identity,
            &request.identity,
            &bound.control.role,
            bound.control.base.before(),
            AllocationPopulation::Status,
        )
        .map_err(|_| unchanged(Cause::Membership))?;
        if bound
            .pool
            .resident_address_present(request)
            .map_err(frame)?
        {
            return Err(unchanged(Cause::AlreadyResident));
        }
        self.check_directory(owner, prefix, None, ledger.occupied)?;
        if self.read_status(owner, request)?.is_some() {
            return Err(fatal(storage, Cause::Invariant));
        }
        let digest = content_digest(self.inner.inner.inner.control.status_after);
        let history = self.scan_history(prefix, request, digest)?;
        // Replay used all scratch loans. Read the selected disk page again;
        // only the latest full image may populate this clean frame.
        if self.read_status(owner, request)?.is_some()
            || content_digest(self.inner.inner.inner.control.status_after) != *history.current()
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        let lsn = operation::status_envelope(
            self.inner.inner.inner.control.status_after,
            &request.identity,
        )
        .map_err(|_| fatal(storage, Cause::Invariant))?;
        if self.allocator_snapshot(owner, prefix)? != snapshot.allocator()
            || content_digest(self.inner.inner.inner.control.base.before()) != allocator_digest
            || storage
                .wal()
                .prefix()
                .map_err(|e| fatal(storage, Cause::Wal(e)))?
                != prefix
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        storage
            .health()
            .map_err(|e| fatal(storage, Cause::Storage(e)))?;
        let bound = &mut self.inner.inner.inner;
        let permit = StatusLoadPermit::from_live(EmptyLoadInstall {
            snapshot: &snapshot,
            index,
            allocator: bound.control.base.before(),
            bytes: bound.control.status_after,
            request,
            lsn,
        });
        let after = bound
            .pool
            .install_status_load(permit)
            .map_err(|e| fatal(storage, Cause::Frame(e)))?;
        Ok(StatusLoadObservation {
            before: snapshot.payload(),
            after,
        })
    }
}

pub(super) fn call_bytes<W: WriterNamespace>() -> usize {
    // The initial snapshot lives through census/replay/read and final install.
    // Existing history call already charges the sequential codec and readers.
    super::history_call_bytes::<W>()
        + 4 * size_of::<StatusLoadSnapshot<'_, '_>>()
        + 4 * size_of::<StatusLoadPermit<'_, '_, '_>>()
        + 4 * size_of::<EmptyLoadInstall<'_, '_, '_>>()
        + 4 * size_of::<Result<StatusLoadObservation>>()
        + 8192
}
pub(super) fn fits<W: WriterNamespace>() -> bool {
    call_bytes::<W>() <= 65536
        && super::super::retained_bytes::<W>() + call_bytes::<W>() <= 196608
        && super::super::retained_bytes::<W>() + call_bytes::<W>() + 65536 <= 262144
        && super::super::handoff_fits::<W>()
}
