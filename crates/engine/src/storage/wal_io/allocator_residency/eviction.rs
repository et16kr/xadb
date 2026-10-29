//! Exclusive nonresident interval of the existing allocator owner. Dropping an
//! unfinished guard abandons volatile authority and fences; it performs no I/O.
use super::*;

pub(crate) struct EvictedAllocator<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    operational: &'a mut OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    snapshot: ReloadSnapshot<'c, 's>,
    prefix: wal::WalPrefix,
    complete: bool,
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Remove the clean allocator's registration, retaining its descriptor and
    /// all persistent obligations. The returned exclusive guard must load it
    /// before release; dropping it unfinished fences the original session.
    ///
    /// Ordinary pre-effect refusal leaves registration unchanged. A leaked
    /// guard leaves no resident target, so subsequent split views cannot use it.
    pub(crate) fn evict_allocator(
        &mut self,
    ) -> Result<EvictedAllocator<'_, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let original = self.inner.storage;
        let _unwind = original.session_fence().unwind_guard();
        original.health().map_err(storage)?;
        if !super::fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        if self.inner.writer.installation.is_some()
            || self.inner.writer.terminal_pending
            || !self.inner.flush.ready_for_reload()
        {
            return Err(unchanged(Cause::Busy));
        }
        // This copy is made only once. Retry may overwrite `loaded`, never prior.
        let snapshot = self
            .inner
            .pool
            .prepare_allocator_eviction(
                (&mut self.inner.writer.operation[32768..49152])
                    .try_into()
                    .expect("fixed immutable prior"),
            )
            .map_err(frame)?;
        // WriterControl's existing decoder reads its before slot. Seal that
        // slot from the just-copied current frame, never from earlier scratch.
        self.inner.writer.operation.copy_within(32768..49152, 65536);
        let prefix = original
            .wal()
            .prefix()
            .map_err(|error| self.fatal(Cause::Wal(error)))?;
        self.inner
            .writer
            .observation(original.profile(), prefix, &snapshot.stamp(), false)
            .map_err(|error| self.fatal(Cause::Allocation(error)))?;
        let snapshot = self
            .inner
            .pool
            .evict_allocator_frame(snapshot)
            .map_err(frame)?;
        Ok(EvictedAllocator {
            operational: self,
            snapshot,
            prefix,
            complete: false,
        })
    }
}

impl<W: WriterNamespace> EvictedAllocator<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Read the selected durable home into an empty caller frame. A real load
    /// attempt consumes that slot's generation even on an ordinary read error.
    /// Such an error retains this guard and its immutable before image for retry.
    /// Bounds/counter refusals precede the claim and all I/O. After success a
    /// second call is rejected; release the guard to regain split views.
    pub(crate) fn load_allocator(&mut self, slot: usize) -> Result<ReloadObservation> {
        let original = self.operational.inner.storage;
        let _unwind = original.session_fence().unwind_guard();
        original.health().map_err(storage)?;
        if self.complete {
            return Err(unchanged(Cause::Busy));
        }
        let claim = self
            .operational
            .inner
            .pool
            .claim_allocator_load(&self.snapshot, slot)
            .map_err(frame)?;
        let request = self.snapshot.request();
        let loaded = (|| -> std::result::Result<bool, PageIoError> {
            let (work, rest) = self.operational.inner.writer.operation.split_at_mut(65536);
            let (root, work) = work.split_at_mut(PAGE_BYTES);
            let (scratch, prior) = work.split_at_mut(PAGE_BYTES);
            let after: &mut [u8; PAGE_BYTES] = (&mut rest[..PAGE_BYTES])
                .try_into()
                .expect("fixed load page");
            {
                let mut reader = original.core.open_reader(
                    root.try_into().expect("fixed root"),
                    scratch.try_into().expect("fixed scratch"),
                )?;
                reader.read_page(request.identity, request.kind, request.level, after)?;
            }
            Ok(&prior[..PAGE_BYTES] == after)
        })()
        .map_err(|error| self.operational.read_error(error))?;
        if !loaded {
            return Err(self.operational.fatal(Cause::HomeMismatch));
        }
        original.health().map_err(storage)?;
        let current = original
            .wal()
            .prefix()
            .map_err(|error| self.operational.fatal(Cause::Wal(error)))?;
        if current != self.prefix {
            return Err(self.operational.fatal(Cause::Invariant));
        }
        let before = self.snapshot.stamp();
        self.operational
            .inner
            .writer
            .observation(original.profile(), current, &before, false)
            .map_err(|error| self.operational.fatal(Cause::Allocation(error)))?;
        let permit = ReloadInstallPermit {
            snapshot: &self.snapshot,
            bytes: (&self.operational.inner.writer.operation[65536..65536 + PAGE_BYTES])
                .try_into()
                .expect("verified load page"),
        };
        let after = self
            .operational
            .inner
            .pool
            .install_evicted_allocator(claim, permit)
            .map_err(frame)?;
        self.complete = true;
        Ok(ReloadObservation { before, after })
    }
}

impl<W: WriterNamespace> Drop for EvictedAllocator<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        if !self.complete || std::thread::panicking() {
            self.operational.inner.storage.session_fence().fence();
        }
    }
}

pub(super) fn fits<W: WriterNamespace>() -> bool {
    // Outer guard/snapshot/claim/returned stamps coexist; reader/codec peaks
    // remain in the original 64KiB reload accounting, without another page.
    let guard = size_of::<EvictedAllocator<'_, '_, '_, '_, '_, '_, '_, '_, W>>();
    let claim = size_of::<dirty_frame::AllocatorLoadClaim<'_, '_>>();
    guard <= 512
        && claim <= 256
        && 2 * guard
            + 2 * claim
            + 4 * size_of::<ReloadSnapshot<'_, '_>>()
            + 4 * size_of::<FrameStamp>()
            + 2 * size_of::<Result<ReloadObservation>>()
            + 2 * size_of::<ReloadInstallPermit<'_, '_, '_>>()
            + 2 * size_of::<super::super::super::allocation_reservation::AllocationObservation>()
            + 512
            <= 4096
}

#[cfg(test)]
#[path = "eviction/tests.rs"]
mod tests;
