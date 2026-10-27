//! Private common continuation. No semantic Commit receipt is constructed here.
use super::*;
use crate::storage::disk::{DurableCommit, InstalledCommit, StagedCommit, UnstagedCommit};
use std::time::Duration;

const FENCE_RECHECK: Duration = Duration::from_millis(10);

/// Identity only: its fields and constructors are confined to the original core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstallationKey {
    pub(super) slot: usize,
    pub(super) owner_generation: u64,
    pub(super) installation_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Installation {
    Open,
    Intent(InstallationKey),
    Pending(InstallationKey, u64),
    Draining(InstallationKey, u64),
    Installing(InstallationKey, u64),
    Abandoned(InstallationKey, Option<u64>),
}

pub(super) fn valid_installation(state: &State) -> bool {
    let expected = match state.installation {
        Installation::Open => None,
        Installation::Intent(key) => Some((key, Mode::CommitStage)),
        Installation::Pending(key, lsn) if lsn != 0 => Some((key, Mode::CommitPending)),
        Installation::Draining(key, lsn) if lsn != 0 => Some((key, Mode::CommitDraining)),
        Installation::Installing(key, lsn) if lsn != 0 => Some((key, Mode::CommitInstalling)),
        _ => return false,
    };
    let mut found = 0;
    for (index, slot) in state.slots.iter().enumerate() {
        if matches!(
            slot.mode,
            Mode::CommitStage | Mode::CommitPending | Mode::CommitDraining | Mode::CommitInstalling
        ) {
            found += 1;
            let Some((key, mode)) = expected else {
                return false;
            };
            if key.slot != index
                || key.owner_generation != slot.generation
                || key.installation_generation != state.installation_generation
                || key.installation_generation == 0
                || slot.mode != mode
            {
                return false;
            }
        }
    }
    found == usize::from(expected.is_some())
}

/// The caller retains its page guards on rejection, so it must receive its
/// original version guard and release pages before releasing this guard.
pub(crate) struct PrepareCommitError<'o, 'c, 'f> {
    pub(crate) error: ConcurrencyError,
    pub(crate) version: VersionEditGuard<'o, 'c, 'f>,
}

pub(crate) struct CommitStage<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    guard: Option<VersionEditGuard<'o, 'c, 'f>>,
    key: InstallationKey,
    stage_call_started: bool,
}

impl<'o, 'c, 'f> VersionEditGuard<'o, 'c, 'f> {
    pub(crate) fn prepare_commit(
        self,
    ) -> std::result::Result<CommitStage<'o, 'c, 'f>, PrepareCommitError<'o, 'c, 'f>> {
        let prepare = || -> Result<InstallationKey> {
            let core = self.core;
            let admission = self
                .admission
                .as_ref()
                .ok_or_else(|| core.fatal(Cause::Invariant))?;
            let owner = admission
                .owner
                .as_deref()
                .ok_or_else(|| core.fatal(Cause::Invariant))?;
            let mut state = core.entry_state()?;
            owner.check_mode(&state, Mode::Apply)?;
            if owner.index >= FOREGROUND_OWNERS {
                return Err(unchanged(Cause::InvalidRole));
            }
            if !state.installation_open() {
                return Err(unchanged(Cause::InstallationPending));
            }
            if state.slots[owner.index].read_operation {
                return Err(unchanged(Cause::Busy));
            }
            let generation = state
                .installation_generation
                .checked_add(1)
                .ok_or_else(|| unchanged(Cause::GenerationExhausted))?;
            let key = InstallationKey {
                slot: owner.index,
                owner_generation: owner.generation,
                installation_generation: generation,
            };
            state.installation_generation = generation;
            state.installation = Installation::Intent(key);
            state.slots[owner.index].mode = Mode::CommitStage;
            Ok(key)
        };
        match prepare() {
            Ok(key) => Ok(CommitStage {
                core: self.core,
                guard: Some(self),
                key,
                stage_call_started: false,
            }),
            Err(error) => Err(PrepareCommitError {
                error,
                version: self,
            }),
        }
    }
}

impl<'o, 'c, 'f> CommitStage<'o, 'c, 'f> {
    pub(crate) fn matches_core(&self, core: &PageConcurrency<'_>) -> bool {
        std::ptr::eq(self.core, core)
    }

    pub(crate) fn cancel_unstaged(mut self) -> Result<()> {
        self.release(false)
    }

    /// Mark before entering the original WAL stage call. A panic, lost result
    /// or unrecognized failure then preserves Abandoned rather than cancelling.
    pub(crate) fn stage_call_started(&mut self) -> Result<InstallationKey> {
        if self.stage_call_started || self.guard.is_none() {
            return Err(self.core.fatal(Cause::Invariant));
        }
        let state = self.core.entry_state()?;
        if state.installation != Installation::Intent(self.key) {
            return Err(self.core.fatal(Cause::Invariant));
        }
        self.stage_call_started = true;
        Ok(self.key)
    }

    pub(crate) fn restore_unstaged(&mut self, receipt: UnstagedCommit<'c, 'f>) -> Result<()> {
        if !self.stage_call_started || !receipt.matches(self.core, self.key) {
            return Err(self.core.fatal(Cause::Invariant));
        }
        let state = self.core.entry_state()?;
        if state.installation != Installation::Intent(self.key) {
            return Err(self.core.fatal(Cause::Invariant));
        }
        self.stage_call_started = false;
        Ok(())
    }

    /// Requires the actual marked call and matching sealed semantic receipt.
    pub(crate) fn suspend(
        mut self,
        staged: StagedCommit<'c, 'f>,
    ) -> Result<PendingInstallation<'o, 'c, 'f>> {
        if !self.stage_call_started || !staged.matches(self.core, self.key) {
            return Err(self.core.fatal(Cause::Invariant));
        }
        {
            let mut state = self.core.entry_state()?;
            if state.installation != Installation::Intent(self.key)
                || state.slots[self.key.slot]
                    != (OwnerSlot {
                        generation: self.key.owner_generation,
                        mode: Mode::CommitStage,
                        read_operation: false,
                    })
            {
                return Err(self.core.fatal(Cause::Invariant));
            }
            let count = state
                .active_apply
                .checked_sub(1)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            state.active_apply = count;
            state.slots[self.key.slot].mode = Mode::CommitPending;
            state.installation = Installation::Pending(self.key, staged.lsn());
        }
        let mut guard = self.guard.take().expect("owned Commit stage");
        let mut admission = guard.admission.take().expect("owned Commit admission");
        let owner = admission.owner.take().expect("owned Commit owner");
        drop(guard); // Page guards were returned by the calling pool wrapper.
        self.core.continuation.notify_all();
        Ok(PendingInstallation {
            core: self.core,
            owner: Some(owner),
            key: self.key,
            staged,
        })
    }

    pub(crate) fn abandon(mut self) -> Result<()> {
        self.release(true)
    }

    fn release(&mut self, abandon: bool) -> Result<()> {
        let Some(mut guard) = self.guard.take() else {
            return Ok(());
        };
        if abandon || self.stage_call_started || std::thread::panicking() {
            self.core.fence.fence();
        }
        self.core.fence_unwind();
        // The wrapper must already have released its actual page latches.
        drop(guard.gate.take());
        let mut admission = guard.admission.take().expect("owned Commit admission");
        let owner = admission.owner.take().expect("owned Commit owner");
        let (mut state, failure) = self.core.cleanup_state();
        let result = (|| -> Result<()> {
            if !state.accounting_valid() || state.installation != Installation::Intent(self.key) {
                return Err(self.core.fatal(Cause::Invariant));
            }
            owner.check_mode(&state, Mode::CommitStage)?;
            let count = state
                .active_apply
                .checked_sub(1)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            state.active_apply = count;
            state.slots[self.key.slot].mode = Mode::Idle;
            state.installation = if self.core.fence.is_fenced() {
                Installation::Abandoned(self.key, None)
            } else {
                Installation::Open
            };
            Ok(())
        })();
        drop(state);
        self.core.continuation.notify_all();
        failure.map_or(result, Err)
    }
}
impl Drop for CommitStage<'_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release(false);
    }
}

pub(crate) struct PendingInstallation<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    owner: Option<&'o mut Owner<'c, 'f>>,
    key: InstallationKey,
    staged: StagedCommit<'c, 'f>,
}
impl<'o, 'c, 'f> PendingInstallation<'o, 'c, 'f> {
    pub(crate) fn matches_core(&self, core: &PageConcurrency<'_>) -> bool {
        std::ptr::eq(self.core, core)
    }
    pub(crate) fn begin_drain(
        self,
        durable: &DurableCommit<'c, 'f>,
    ) -> Result<DrainingInstallation<'o, 'c, 'f>> {
        if !self.staged.confirmed_by(durable) {
            return Err(self.core.fatal(Cause::Invariant));
        }
        {
            let mut state = self.core.entry_state()?;
            if state.installation != Installation::Pending(self.key, self.staged.lsn()) {
                return Err(self.core.fatal(Cause::Invariant));
            }
            self.owner
                .as_deref()
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?
                .check_mode(&state, Mode::CommitPending)?;
            state.slots[self.key.slot].mode = Mode::CommitDraining;
            state.installation = Installation::Draining(self.key, self.staged.lsn());
        }
        #[cfg(test)]
        super::catalog_test_support::hit(self.core, super::catalog_test_support::Point::Draining);
        Ok(DrainingInstallation {
            pending: Some(self),
        })
    }
    pub(crate) fn abandon(mut self) -> Result<()> {
        self.release_abandoned()
    }

    fn release_abandoned(&mut self) -> Result<()> {
        let Some(owner) = self.owner.take() else {
            return Ok(());
        };
        self.core.fence.fence();
        let (mut state, failure) = self.core.cleanup_state();
        let expected = match state.installation {
            Installation::Pending(key, lsn) if key == self.key && lsn == self.staged.lsn() => {
                Mode::CommitPending
            }
            Installation::Draining(key, lsn) if key == self.key && lsn == self.staged.lsn() => {
                Mode::CommitDraining
            }
            Installation::Installing(key, lsn) if key == self.key && lsn == self.staged.lsn() => {
                Mode::CommitInstalling
            }
            _ => {
                drop(state);
                return Err(self.core.fatal(Cause::Invariant));
            }
        };
        let result = (|| -> Result<()> {
            if !state.accounting_valid() {
                return Err(self.core.fatal(Cause::Invariant));
            }
            owner.check_mode(&state, expected)?;
            if expected == Mode::CommitInstalling {
                state.active_apply = state
                    .active_apply
                    .checked_sub(1)
                    .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            }
            state.slots[self.key.slot].mode = Mode::Idle;
            state.installation = Installation::Abandoned(self.key, Some(self.staged.lsn()));
            Ok(())
        })();
        drop(state);
        self.core.continuation.notify_all();
        failure.map_or(result, Err)
    }
}
impl Drop for PendingInstallation<'_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release_abandoned();
    }
}

pub(crate) struct DrainingInstallation<'o, 'c, 'f> {
    pending: Option<PendingInstallation<'o, 'c, 'f>>,
}
impl<'o, 'c, 'f> DrainingInstallation<'o, 'c, 'f> {
    pub(crate) fn wait_ready(mut self) -> Result<CommitInstalling<'o, 'c, 'f>> {
        let pending = self.pending.as_ref().expect("owned drain");
        let core = pending.core;
        {
            let mut state = core.entry_state()?;
            #[cfg(test)]
            let mut announced_wait = false;
            loop {
                core.live()?;
                if !state.valid()
                    || state.installation
                        != Installation::Draining(pending.key, pending.staged.lsn())
                {
                    return Err(core.fatal(Cause::Invariant));
                }
                if state.active_apply == 0
                    && state.active_content == 0
                    && !state.slots.iter().any(|slot| slot.read_operation)
                {
                    break;
                }
                #[cfg(test)]
                if !announced_wait {
                    announced_wait = true;
                    drop(state);
                    super::catalog_test_support::hit(
                        core,
                        super::catalog_test_support::Point::DrainWaiting,
                    );
                    state = core.entry_state()?;
                    continue;
                }
                state = match core.continuation.wait_timeout(state, FENCE_RECHECK) {
                    Ok((state, _)) => state,
                    Err(poison) => {
                        let error = core.fatal(Cause::Poisoned);
                        drop(poison.into_inner());
                        return Err(error);
                    }
                };
            }
            state.active_apply = 1;
            state.slots[pending.key.slot].mode = Mode::CommitInstalling;
            state.installation = Installation::Installing(pending.key, pending.staged.lsn());
        }
        let gate = match core.version.try_lock() {
            Ok(gate) => gate,
            Err(TryLockError::WouldBlock) => return Err(core.fatal(Cause::Invariant)),
            Err(TryLockError::Poisoned(error)) => {
                let failure = core.fatal(Cause::Poisoned);
                drop(error.into_inner());
                return Err(failure);
            }
        };
        let result = CommitInstalling {
            gate: Some(gate),
            pending: self.pending.take(),
        };
        core.live()?;
        Ok(result)
    }
    pub(crate) fn abandon(mut self) -> Result<()> {
        self.pending.take().expect("owned drain").abandon()
    }
}

pub(crate) struct CommitInstalling<'o, 'c, 'f> {
    gate: Option<MutexGuard<'c, ()>>,
    pending: Option<PendingInstallation<'o, 'c, 'f>>,
}
impl<'c, 'f> CommitInstalling<'_, 'c, 'f> {
    pub(crate) fn matches_core(&self, core: &PageConcurrency<'_>) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.matches_core(core))
    }
    pub(crate) fn binding(&self) -> &StagedCommit<'c, 'f> {
        &self.pending.as_ref().expect("owned installation").staged
    }
    pub(crate) fn finish(mut self, installed: InstalledCommit<'c, 'f>) -> Result<()> {
        let pending = self.pending.as_mut().expect("owned installation");
        if !pending.staged.completed_by(&installed) {
            return Err(pending.core.fatal(Cause::Invariant));
        }
        // Caller has returned all page latches; Open follows actual gate return.
        drop(self.gate.take());
        {
            let mut state = pending.core.entry_state()?;
            if state.installation != Installation::Installing(pending.key, pending.staged.lsn())
                || state.active_apply != 1
                || state.active_content != 0
                || state.slots.iter().any(|slot| slot.read_operation)
            {
                return Err(pending.core.fatal(Cause::Invariant));
            }
            pending
                .owner
                .as_deref()
                .ok_or_else(|| pending.core.fatal(Cause::Invariant))?
                .check_mode(&state, Mode::CommitInstalling)?;
            state.active_apply = 0;
            state.slots[pending.key.slot].mode = Mode::Idle;
            state.installation = Installation::Open;
        }
        pending.owner.take();
        pending.core.continuation.notify_all();
        self.pending.take();
        Ok(())
    }
    pub(crate) fn abandon(mut self) -> Result<()> {
        let pending = self.pending.as_mut().expect("owned installation");
        pending.core.fence.fence();
        drop(self.gate.take());
        self.pending.take().expect("owned installation").abandon()
    }
}
impl Drop for CommitInstalling<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.as_ref() {
            pending.core.fence.fence();
        }
        drop(self.gate.take());
        drop(self.pending.take());
    }
}

/// Source values in the common continuation. The enclosing pool/semantic
/// producer additionally counts retained pages, plans and its caller returns.
pub(crate) const fn source_bytes() -> usize {
    core_source_bytes() + call_source_bytes()
}

/// Nested continuation values only. The caller must additionally charge the
/// original core once; this function does not establish a shared owner census.
pub(crate) const fn call_source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<PrepareCommitError<'_, '_, '_>>()
        + 2 * size_of::<std::result::Result<CommitStage<'_, '_, '_>, PrepareCommitError<'_, '_, '_>>>(
        )
        + size_of::<CommitStage<'_, '_, '_>>()
        + size_of::<PendingInstallation<'_, '_, '_>>()
        + size_of::<DrainingInstallation<'_, '_, '_>>()
        + size_of::<CommitInstalling<'_, '_, '_>>()
        + 2 * size_of::<Result<PendingInstallation<'_, '_, '_>>>()
        + 2 * size_of::<Result<DrainingInstallation<'_, '_, '_>>>()
        + 2 * size_of::<Result<CommitInstalling<'_, '_, '_>>>()
        + size_of::<std::sync::LockResult<(MutexGuard<'_, State>, std::sync::WaitTimeoutResult)>>()
        + 2 * size_of::<Result<MutexGuard<'_, State>>>()
        + 2 * size_of::<InstallationKey>()
        + 2 * size_of::<ConcurrencyError>()
}

#[cfg(test)]
#[path = "concurrency_commit_tests.rs"]
mod tests;
