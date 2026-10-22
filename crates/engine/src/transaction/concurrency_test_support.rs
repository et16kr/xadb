//! Private cfg(test)-only observations and SIMULATED Commit ownership transitions.
//! No WAL bytes are staged, synced or certified by this module.

use super::*;
use std::{
    sync::mpsc::{Receiver, SyncSender},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InstallationKey {
    pub(super) slot: usize,
    pub(super) owner_generation: u64,
    pub(super) installation_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Installation {
    Open,
    Intent(InstallationKey),
    Pending(InstallationKey, u64),
    Installing(InstallationKey, u64),
    Abandoned(InstallationKey, Option<u64>),
}

pub(super) fn valid_installation(state: &State) -> bool {
    let expected = match state.installation {
        Installation::Open => None,
        Installation::Intent(key) => Some((key, Mode::CommitStage)),
        Installation::Pending(key, lsn) if lsn != 0 => Some((key, Mode::CommitPending)),
        Installation::Installing(key, lsn) if lsn != 0 => Some((key, Mode::CommitInstalling)),
        // Abandoned is never healthy; new entry must first observe the shared fence.
        _ => return false,
    };
    let mut found = 0;
    for (index, slot) in state.slots.iter().enumerate() {
        if matches!(
            slot.mode,
            Mode::CommitStage | Mode::CommitPending | Mode::CommitInstalling
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PausePoint {
    AfterVersionPrecheck,
    AfterVersionLock,
    FencedBeforeRelease,
    BeforeCleanupLock,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Resume {
    Continue,
    Panic,
}
struct Pause {
    arrived: SyncSender<()>,
    release: Receiver<Resume>,
}
#[derive(Default)]
pub(super) struct Hooks {
    after_precheck: Option<Pause>,
    after_lock: Option<Pause>,
    fenced: Option<Pause>,
    before_cleanup: Option<Pause>,
}
impl Hooks {
    fn at(&mut self, point: PausePoint) -> &mut Option<Pause> {
        match point {
            PausePoint::AfterVersionPrecheck => &mut self.after_precheck,
            PausePoint::AfterVersionLock => &mut self.after_lock,
            PausePoint::FencedBeforeRelease => &mut self.fenced,
            PausePoint::BeforeCleanupLock => &mut self.before_cleanup,
        }
    }
}

pub(super) fn install_pause(
    core: &PageConcurrency<'_>,
    point: PausePoint,
    arrived: SyncSender<()>,
    release: Receiver<Resume>,
) {
    let mut hooks = core.hooks.lock().unwrap();
    assert!(hooks.at(point).is_none(), "pause already installed");
    *hooks.at(point) = Some(Pause { arrived, release });
}

pub(super) fn hit_pause(core: &PageConcurrency<'_>, point: PausePoint) {
    let pause = core.hooks.lock().unwrap().at(point).take();
    if let Some(pause) = pause {
        pause
            .arrived
            .try_send(())
            .expect("pause arrival channel must have capacity");
        match pause
            .release
            .recv_timeout(Duration::from_secs(5))
            .expect("pause release watchdog")
        {
            Resume::Continue => {}
            Resume::Panic => panic!("test-only pause panic"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Observation {
    pub(super) slots: [OwnerSlot; OWNER_SLOTS],
    pub(super) active_apply: u16,
    pub(super) active_content: u16,
    pub(super) installation: Installation,
    pub(super) installation_generation: u64,
    pub(super) fenced: bool,
}

pub(super) fn observe(core: &PageConcurrency<'_>) -> Observation {
    let state = core
        .state
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    Observation {
        slots: state.slots,
        active_apply: state.active_apply,
        active_content: state.active_content,
        installation: state.installation,
        installation_generation: state.installation_generation,
        fenced: core.fence.is_fenced(),
    }
}

pub(super) fn owner_identity(owner: &Owner<'_, '_>) -> (usize, u64) {
    (owner.index, owner.generation)
}

pub(super) fn set_slot(core: &PageConcurrency<'_>, index: usize, generation: u64, mode: Mode) {
    core.state.lock().unwrap().slots[index] = OwnerSlot { generation, mode };
}

pub(super) fn set_counts(core: &PageConcurrency<'_>, apply: u16, content: u16) {
    let mut state = core.state.lock().unwrap();
    state.active_apply = apply;
    state.active_content = content;
}

pub(super) fn set_installation_generation(core: &PageConcurrency<'_>, generation: u64) {
    core.state.lock().unwrap().installation_generation = generation;
}

pub(super) fn hold_state<'c>(core: &'c PageConcurrency<'_>) -> MutexGuard<'c, State> {
    core.state.lock().unwrap()
}

pub(super) fn version_busy(core: &PageConcurrency<'_>) -> bool {
    matches!(core.version.try_lock(), Err(TryLockError::WouldBlock))
}

pub(super) fn poison_state(core: &PageConcurrency<'_>) {
    let _ = std::panic::catch_unwind(|| {
        let _guard = core.state.lock().unwrap();
        panic!("test-only state poison");
    });
}

pub(super) fn poison_version(core: &PageConcurrency<'_>) {
    let _ = std::panic::catch_unwind(|| {
        let _guard = core.version.lock().unwrap();
        panic!("test-only version poison");
    });
}

pub(super) struct SimulatedCommitStage<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    guard: Option<VersionEditGuard<'o, 'c, 'f>>,
    key: InstallationKey,
    attempted: bool,
}

pub(super) fn simulated_commit<'o, 'c, 'f>(
    guard: VersionEditGuard<'o, 'c, 'f>,
) -> Result<SimulatedCommitStage<'o, 'c, 'f>> {
    let admission = guard.admission.as_ref().unwrap();
    let owner = admission.owner.as_deref().unwrap();
    let core = owner.core;
    let key = {
        let mut state = core.entry_state()?;
        owner.check_mode(&state, Mode::Apply)?;
        if owner.index >= FOREGROUND_OWNERS {
            return Err(unchanged(Cause::InvalidRole));
        }
        if !state.installation_open() {
            return Err(unchanged(Cause::InstallationPending));
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
        key
    };
    Ok(SimulatedCommitStage {
        core,
        guard: Some(guard),
        key,
        attempted: false,
    })
}

impl<'o, 'c, 'f> SimulatedCommitStage<'o, 'c, 'f> {
    pub(super) fn mark_attempted(&mut self) {
        self.attempted = true;
    }

    pub(super) fn cancel_unattempted(self) -> Result<()> {
        if self.attempted {
            return Err(self.core.fatal(Cause::Invariant));
        }
        Ok(())
    }

    /// SIMULATED lower-layer proof of no staged bytes, never a production receipt.
    pub(super) fn reject_unchanged(mut self) {
        self.attempted = false;
    }

    pub(super) fn complete(mut self, lsn: u64) -> Result<SimulatedInstallation<'o, 'c, 'f>> {
        if !self.attempted || lsn == 0 {
            return Err(self.core.fatal(Cause::Invariant));
        }
        {
            let mut state = self.core.entry_state()?;
            if state.installation != Installation::Intent(self.key) {
                return Err(self.core.fatal(Cause::Invariant));
            }
            let count = state
                .active_apply
                .checked_sub(1)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            state.installation = Installation::Pending(self.key, lsn);
            state.slots[self.key.slot].mode = Mode::CommitPending;
            state.active_apply = count;
        }
        let mut guard = self.guard.take().unwrap();
        let mut admission = guard.admission.take().unwrap();
        let owner = admission.owner.take().unwrap();
        drop(guard); // Actual gate release follows the pending ownership handoff.
        Ok(SimulatedInstallation {
            core: self.core,
            owner: Some(owner),
            key: self.key,
            lsn,
        })
    }
}

impl Drop for SimulatedCommitStage<'_, '_, '_> {
    fn drop(&mut self) {
        if self.guard.is_none() {
            return;
        }
        let abandoned = self.attempted || std::thread::panicking() || self.core.fence.is_fenced();
        if abandoned {
            self.core.fence.fence();
        }
        self.core.fence_unwind();
        {
            let (mut state, _) = self.core.cleanup_state();
            let abandoned = abandoned || self.core.fence.is_fenced();
            if !state.accounting_valid()
                || state.installation != Installation::Intent(self.key)
                || state.slots[self.key.slot]
                    != (OwnerSlot {
                        generation: self.key.owner_generation,
                        mode: Mode::CommitStage,
                    })
            {
                self.core.fence.fence();
            } else {
                state.installation = if abandoned {
                    Installation::Abandoned(self.key, None)
                } else {
                    Installation::Open
                };
                state.slots[self.key.slot].mode = Mode::Apply;
            }
        }
        drop(self.guard.take());
    }
}

pub(super) struct SimulatedInstallation<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    owner: Option<&'o mut Owner<'c, 'f>>,
    key: InstallationKey,
    lsn: u64,
}

impl<'o, 'c, 'f> SimulatedInstallation<'o, 'c, 'f> {
    pub(super) fn key(&self) -> InstallationKey {
        self.key
    }

    pub(super) fn try_install(
        self,
    ) -> std::result::Result<SimulatedInstallGuard<'o, 'c, 'f>, (ConcurrencyError, Self)> {
        let core = self.core;
        self.try_install_on(core)
    }

    /// Wrong-core use fences the token's origin and retains its pending owner.
    pub(super) fn try_install_on(
        self,
        destination: &PageConcurrency<'_>,
    ) -> std::result::Result<SimulatedInstallGuard<'o, 'c, 'f>, (ConcurrencyError, Self)> {
        if !std::ptr::eq(self.core, destination) {
            let error = self.core.fatal(Cause::Invariant);
            return Err((error, self));
        }
        let check = || -> Result<()> {
            let state = self.core.entry_state()?;
            if state.installation != Installation::Pending(self.key, self.lsn) {
                return Err(self.core.fatal(Cause::Invariant));
            }
            Ok(())
        };
        if let Err(error) = check() {
            return Err((error, self));
        }
        let gate = match self.core.version.try_lock() {
            Ok(gate) => gate,
            Err(TryLockError::WouldBlock) => {
                let error = self
                    .core
                    .live()
                    .err()
                    .unwrap_or_else(|| unchanged(Cause::Busy));
                return Err((error, self));
            }
            Err(TryLockError::Poisoned(_)) => {
                let error = self.core.fatal(Cause::Poisoned);
                return Err((error, self));
            }
        };
        let change = || -> Result<()> {
            let mut state = self.core.entry_state()?;
            if state.installation != Installation::Pending(self.key, self.lsn) {
                return Err(self.core.fatal(Cause::Invariant));
            }
            let count = state
                .active_apply
                .checked_add(1)
                .filter(|count| *count <= 33)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            state.active_apply = count;
            state.slots[self.key.slot].mode = Mode::CommitInstalling;
            state.installation = Installation::Installing(self.key, self.lsn);
            Ok(())
        };
        if let Err(error) = change() {
            drop(gate);
            return Err((error, self));
        }
        Ok(SimulatedInstallGuard {
            gate: Some(gate),
            pending: Some(self),
        })
    }
}

impl Drop for SimulatedInstallation<'_, '_, '_> {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        self.core.fence.fence();
        self.core.fence_unwind();
        let (mut state, _) = self.core.cleanup_state();
        if state.accounting_valid()
            && state.installation == Installation::Pending(self.key, self.lsn)
            && owner.check_mode(&state, Mode::CommitPending).is_ok()
        {
            state.installation = Installation::Abandoned(self.key, Some(self.lsn));
            state.slots[self.key.slot].mode = Mode::Idle;
        }
    }
}

pub(super) struct SimulatedInstallGuard<'o, 'c, 'f> {
    gate: Option<MutexGuard<'c, ()>>,
    pending: Option<SimulatedInstallation<'o, 'c, 'f>>,
}

impl SimulatedInstallGuard<'_, '_, '_> {
    /// SIMULATED installation receipt; no actual status/DPT/terminal effect occurs.
    pub(super) fn complete(mut self) -> Result<()> {
        let pending = self.pending.as_mut().unwrap();
        {
            let mut state = pending.core.entry_state()?;
            if state.installation != Installation::Installing(pending.key, pending.lsn) {
                return Err(pending.core.fatal(Cause::Invariant));
            }
            let count = state
                .active_apply
                .checked_sub(1)
                .ok_or_else(|| pending.core.fatal(Cause::Invariant))?;
            state.active_apply = count;
            state.slots[pending.key.slot].mode = Mode::Idle;
            state.installation = Installation::Open;
        }
        pending.owner.take();
        drop(self.gate.take());
        drop(self.pending.take());
        Ok(())
    }
}

impl Drop for SimulatedInstallGuard<'_, '_, '_> {
    fn drop(&mut self) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        pending.core.fence.fence();
        pending.core.fence_unwind();
        drop(self.gate.take());
        {
            let (mut state, _) = pending.core.cleanup_state();
            if state.accounting_valid()
                && state.installation == Installation::Installing(pending.key, pending.lsn)
                && state.slots[pending.key.slot]
                    == (OwnerSlot {
                        generation: pending.key.owner_generation,
                        mode: Mode::CommitInstalling,
                    })
                && let Some(count) = state.active_apply.checked_sub(1)
            {
                state.active_apply = count;
                state.slots[pending.key.slot].mode = Mode::Idle;
                state.installation = Installation::Abandoned(pending.key, Some(pending.lsn));
                pending.owner.take();
            }
        }
        drop(self.pending.take());
    }
}
