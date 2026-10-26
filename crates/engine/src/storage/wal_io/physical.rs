//! Physical append, exact-interval durability and the original WAL drainer.
//! The runtime and its resources remain private in the parent module. This
//! implementation consumes validated immutable bytes and has no Catalog/Row
//! operation, candidate or terminal-receipt dependency.

use super::{
    AppendState, COMPLETION_RECHECK, CONTROL_BYTES, Cause, DurableWal, EnsureDurable, IoFence,
    MAX_FRAME_BYTES, Phase, Result, SlotState, WAL_FILE_CAP, WalFrameLocation, WalIoError,
    WalPrefix, WalRequirement, WalRuntime, WalSyncObservation, attempted, interval_end, unchanged,
};
use crate::storage::{
    page::FileId,
    wal_frame::{self, FrameExpectation, WalError},
};
use foundation::{
    io::{sync, write_all_at},
    namespace::WriterNamespace,
};
use std::sync::{MutexGuard, atomic::Ordering};

#[cfg(test)]
use super::durability_test_support;

impl<'m, W: WriterNamespace> WalRuntime<'m, W> {
    /// Physical observation only, including after an I/O fence. The original
    /// stage-issued exact interval is required; no caller LSN or semantic grant.
    pub(super) fn first_write_started(&self, requirement: &WalRequirement<'_>) -> Result<bool> {
        if !requirement.uses_fence(&self.fenced) {
            return Err(unchanged(Phase::Observe, Cause::IdentityMismatch));
        }
        let state = self.append.lock().map_err(|_| {
            self.fenced.fence();
            self.completion.notify_all();
            attempted(Phase::Observe, Cause::Poisoned)
        })?;
        // Do not call state()/live(): the post-failure distinction is useful
        // precisely when the session is fenced. Structural validation remains.
        self.requirement_ready(&state, requirement)?;
        let observed = self.first_write_lsn.load(Ordering::Acquire);
        if observed > state.prefix.complete_lsn {
            return Err(self.requirement_invariant());
        }
        Ok(observed >= requirement.exact.lsn)
    }

    /// Poison is observable without acquiring a possibly held mutex.
    pub(super) fn is_fenced(&self) -> bool {
        if self.append.is_poisoned() || self.file.is_poisoned() || self._writer.is_poisoned() {
            self.fenced.fence();
            self.completion.notify_all();
        }
        self.fenced.is_fenced()
    }

    pub(super) fn state(&self, phase: Phase) -> Result<MutexGuard<'_, AppendState<'m>>> {
        if self.is_fenced() {
            return Err(attempted(phase, Cause::Fenced));
        }
        let state = self.append.lock().map_err(|_| {
            self.fenced.fence();
            self.completion.notify_all();
            attempted(phase, Cause::Poisoned)
        })?;
        if self.is_fenced() {
            return Err(attempted(phase, Cause::Fenced));
        }
        Ok(state)
    }

    pub(super) fn prefix(&self) -> Result<WalPrefix> {
        let _unwind = self.fenced.unwind_guard();
        Ok(self.state(Phase::Observe)?.prefix)
    }

    /// Validate outside the append gate, then copy only against the same snapshot.
    pub(super) fn try_stage(&self, bytes: &[u8]) -> Result<WalFrameLocation> {
        #[cfg(test)]
        {
            self.stage_inner(bytes, None, || {})
        }
        #[cfg(not(test))]
        {
            self.stage_inner(bytes, None)
        }
    }

    // The instance-owned test pause is absent from the production signature.
    pub(super) fn stage_inner(
        &self,
        bytes: &[u8],
        mapping: Option<(FileId, u64)>,
        #[cfg(test)] after_snapshot: impl FnOnce(),
    ) -> Result<WalFrameLocation> {
        let _unwind = self.fenced.unwind_guard();
        let snapshot = {
            let state = self.state(Phase::Stage)?;
            if state.state != SlotState::Idle {
                return Err(unchanged(Phase::Stage, Cause::Busy));
            }
            state.prefix
        };
        #[cfg(test)]
        after_snapshot();
        let view = wal_frame::decode_frame(
            bytes,
            FrameExpectation {
                stream: snapshot.stream,
                previous_lsn: snapshot.complete_lsn,
            },
        )
        .map_err(|e| unchanged(Phase::Stage, Cause::Frame(e)))?;
        if let Some((file, pages)) = mapping {
            for record in view.redo_records() {
                if record.target.page.file != file || record.target.page.page_no >= pages {
                    return Err(unchanged(Phase::Stage, Cause::MappingMismatch));
                }
            }
        }
        self.stage_verified(snapshot, view.header(), bytes)
    }

    // Callers must own validated immutable bytes. The allocation consumer keeps
    // this behind its private PreparedAllocation, never a raw semantic API.
    pub(super) fn stage_verified(
        &self,
        snapshot: WalPrefix,
        header: wal_frame::WalFrameHeader,
        bytes: &[u8],
    ) -> Result<WalFrameLocation> {
        let mut state = self.state(Phase::Stage)?;
        if state.state != SlotState::Idle {
            return Err(unchanged(Phase::Stage, Cause::Busy));
        }
        if state.prefix != snapshot {
            return Err(unchanged(
                Phase::Stage,
                Cause::Frame(WalError::OrderMismatch),
            ));
        }
        let end = interval_end(snapshot.complete_offset, bytes.len())
            .map_err(|e| unchanged(Phase::Stage, e))?;
        if end > WAL_FILE_CAP {
            return Err(unchanged(Phase::Stage, Cause::Capacity));
        }
        let frame = WalFrameLocation {
            stream: header.stream,
            file: snapshot.file,
            previous_lsn: header.previous_lsn,
            lsn: header.lsn,
            start: snapshot.complete_offset,
            length: bytes.len(),
            end,
        };
        let slot = state.slot.as_mut().ok_or_else(|| {
            self.fenced.fence();
            attempted(Phase::Stage, Cause::Poisoned)
        })?;
        slot[..bytes.len()].copy_from_slice(bytes);
        state.prefix.complete_lsn = frame.lsn;
        state.prefix.complete_offset = frame.end;
        state.state = SlotState::Complete(frame);
        Ok(frame)
    }

    pub(super) fn stage_verified_required<'s>(
        &'s self,
        snapshot: WalPrefix,
        header: wal_frame::WalFrameHeader,
        bytes: &[u8],
    ) -> Result<WalRequirement<'s>> {
        let exact = self.stage_verified(snapshot, header, bytes)?;
        // No fallible operation or callback follows the actual complete stage.
        Ok(WalRequirement {
            origin: &self.fenced,
            exact,
        })
    }

    pub(super) fn requirement_invariant(&self) -> WalIoError {
        self.fenced.fence();
        self.completion.notify_all();
        attempted(Phase::Observe, Cause::Poisoned)
    }

    // Must run under the same append guard as a subsequent claim or wait.
    fn requirement_ready(
        &self,
        state: &AppendState<'_>,
        requirement: &WalRequirement<'_>,
    ) -> Result<bool> {
        if !requirement.uses_fence(&self.fenced) {
            return Err(unchanged(Phase::Observe, Cause::IdentityMismatch));
        }
        let frame = requirement.exact;
        let prefix = state.prefix;
        let pending_valid = |pending: WalFrameLocation| {
            pending.stream == prefix.stream
                && pending.file == prefix.file
                && pending.previous_lsn == prefix.synced_lsn
                && pending.lsn > pending.previous_lsn
                && pending.start == prefix.synced_offset
                && pending.lsn == prefix.complete_lsn
                && pending.end == prefix.complete_offset
                && (192..=MAX_FRAME_BYTES).contains(&pending.length)
                && interval_end(pending.start, pending.length).ok() == Some(pending.end)
        };
        let census = prefix.synced_offset >= CONTROL_BYTES as u64
            && prefix.synced_offset <= prefix.complete_offset
            && prefix.synced_lsn <= prefix.complete_lsn
            && prefix.complete_offset <= WAL_FILE_CAP
            && match state.state {
                SlotState::Idle => {
                    state.slot.is_some()
                        && prefix.synced_lsn == prefix.complete_lsn
                        && prefix.synced_offset == prefix.complete_offset
                }
                SlotState::Complete(pending) => state.slot.is_some() && pending_valid(pending),
                SlotState::IoOwned(pending) => state.slot.is_none() && pending_valid(pending),
            };
        let authentic_shape = frame.stream == prefix.stream
            && frame.file == prefix.file
            && frame.lsn > frame.previous_lsn
            && frame.start >= CONTROL_BYTES as u64
            && (192..=MAX_FRAME_BYTES).contains(&frame.length)
            && interval_end(frame.start, frame.length).ok() == Some(frame.end)
            && frame.end <= prefix.complete_offset
            && frame.lsn <= prefix.complete_lsn;
        let lsn_ready = frame.lsn <= prefix.synced_lsn;
        let bytes_ready = frame.end <= prefix.synced_offset;
        if !census || !authentic_shape || lsn_ready != bytes_ready {
            return Err(self.requirement_invariant());
        }
        if !lsn_ready
            && !matches!(state.state, SlotState::Complete(pending) | SlotState::IoOwned(pending) if pending == frame)
        {
            return Err(self.requirement_invariant());
        }
        Ok(lsn_ready)
    }

    /// Inspect and claim one exact requirement without waiting for an I/O owner.
    pub(super) fn ensure_durable<'s>(
        &self,
        requirement: &WalRequirement<'s>,
    ) -> Result<EnsureDurable<'s>> {
        let _unwind = self.fenced.unwind_guard();
        let (slot, frame) = {
            let mut state = self.state(Phase::Observe)?;
            if self.requirement_ready(&state, requirement)? {
                return Ok(EnsureDurable::AlreadySynced(DurableWal {
                    requirement: *requirement,
                    prefix: state.prefix,
                }));
            }
            match state.state {
                SlotState::IoOwned(_) => return Ok(EnsureDurable::AwaitWalOwner),
                SlotState::Complete(frame) => {
                    let slot = state
                        .slot
                        .take()
                        .ok_or_else(|| self.requirement_invariant())?;
                    state.state = SlotState::IoOwned(frame);
                    (slot, frame)
                }
                SlotState::Idle => return Err(self.requirement_invariant()),
            }
        };
        let observation = self.sync_claim(slot, frame)?;
        Ok(EnsureDurable::SyncedHere(DurableWal {
            requirement: *requirement,
            prefix: observation.prefix,
        }))
    }

    /// Only the dirty writer waits here, after releasing every page/pin guard.
    pub(super) fn wait_durable<'s>(
        &self,
        requirement: &WalRequirement<'s>,
    ) -> Result<DurableWal<'s>> {
        let _unwind = self.fenced.unwind_guard();
        match self.ensure_durable(requirement)? {
            EnsureDurable::AlreadySynced(proof) | EnsureDurable::SyncedHere(proof) => {
                return Ok(proof);
            }
            EnsureDurable::AwaitWalOwner => {}
        }
        #[cfg(test)]
        durability_test_support::before_wait(self);
        let mut state = self.state(Phase::Observe)?;
        // A completion between ensure and wait is observed under this guard.
        // Spurious wakeups repeat the predicate, never write/sync.
        while !self.requirement_ready(&state, requirement)? {
            if self.is_fenced() {
                return Err(attempted(Phase::Observe, Cause::Fenced));
            }
            #[cfg(test)]
            self.durability_waits
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (next, _) = self
                .completion
                .wait_timeout(state, COMPLETION_RECHECK)
                .map_err(|_| {
                    self.fenced.fence();
                    self.completion.notify_all();
                    attempted(Phase::Observe, Cause::Poisoned)
                })?;
            state = next;
            if self.is_fenced() {
                return Err(attempted(Phase::Observe, Cause::Fenced));
            }
        }
        if self.is_fenced() {
            return Err(attempted(Phase::Observe, Cause::Fenced));
        }
        Ok(DurableWal {
            requirement: *requirement,
            prefix: state.prefix,
        })
    }

    /// Move the single protected interval to I/O ownership before any write.
    /// No append or namespace mutex is held during file I/O or file-lock waiting.
    pub(super) fn sync_pending(&self) -> Result<WalSyncObservation> {
        let _unwind = self.fenced.unwind_guard();
        let (slot, frame) = {
            let mut state = self.state(Phase::Write)?;
            let frame = match state.state {
                SlotState::Idle => return Err(unchanged(Phase::Write, Cause::NoPending)),
                SlotState::IoOwned(_) => return Err(unchanged(Phase::Write, Cause::Busy)),
                SlotState::Complete(frame) => frame,
            };
            let slot = state.slot.take().ok_or_else(|| {
                self.fenced.fence();
                attempted(Phase::Write, Cause::Poisoned)
            })?;
            state.state = SlotState::IoOwned(frame);
            (slot, frame)
        };
        self.sync_claim(slot, frame)
    }

    fn sync_claim(
        &self,
        slot: &'m mut [u8; MAX_FRAME_BYTES],
        frame: WalFrameLocation,
    ) -> Result<WalSyncObservation> {
        // If any subsequent path fails, the interval stays protected in IoOwned
        // and the borrowed slot cannot be issued again through this session.
        let mut claim = IoFence {
            fenced: &self.fenced,
            completion: &self.completion,
            armed: true,
        };
        // This exact staged nonempty interval also proves foundation::io::range
        // cannot reject between the marker and the first FileIo::write_at call.
        if !(192..=MAX_FRAME_BYTES).contains(&frame.length)
            || interval_end(frame.start, frame.length).ok() != Some(frame.end)
            || frame.start < CONTROL_BYTES as u64
            || frame.end > WAL_FILE_CAP
            || frame.lsn <= frame.previous_lsn
        {
            return Err(attempted(Phase::Write, Cause::Poisoned));
        }
        {
            let mut file = self
                .file
                .lock()
                .map_err(|_| attempted(Phase::Write, Cause::Poisoned))?;
            if self.is_fenced() {
                return Err(attempted(Phase::Write, Cause::Fenced));
            }
            let previous = self.first_write_lsn.load(Ordering::Acquire);
            if previous >= frame.lsn
                || self
                    .first_write_lsn
                    .compare_exchange(previous, frame.lsn, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                return Err(attempted(Phase::Write, Cause::Poisoned));
            }
            // No callback, fallible check, lock or fence recheck may be inserted
            // between this monotone marker and the nonempty exact write call.
            write_all_at(&mut *file, &slot[..frame.length], frame.start)
                .map_err(|e| attempted(Phase::Write, Cause::Io(e)))?;
            sync(&mut *file).map_err(|e| attempted(Phase::Sync, Cause::Io(e)))?;
        }
        // File ownership is released before reacquiring the append gate.
        let mut state = self.state(Phase::Publish)?;
        if state.state != SlotState::IoOwned(frame)
            || state.slot.is_some()
            || state.prefix.complete_lsn != frame.lsn
            || state.prefix.complete_offset != frame.end
        {
            return Err(attempted(Phase::Publish, Cause::Poisoned));
        }
        state.prefix.synced_lsn = frame.lsn;
        state.prefix.synced_offset = frame.end;
        state.slot = Some(slot);
        state.state = SlotState::Idle;
        let observation = WalSyncObservation {
            frame,
            prefix: state.prefix,
        };
        claim.armed = false;
        self.completion.notify_all();
        Ok(observation)
    }
}
