//! DEV3 3.10.1.a.1: bounded operation admission and one version-edit gate.
//! No page latch, snapshot, FIFO turn, WAL stage or Commit installation API.
//! The separate test-support module simulates the future installation boundary.
#![allow(dead_code)] // Awaiting separately reviewed page/transaction consumers.

use crate::storage::session_fence::SessionFence;
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{Mutex, MutexGuard, TryLockError},
};
use xadb_foundation::error::Effect;

#[cfg(test)]
#[path = "concurrency_test_support.rs"]
pub(super) mod test_support;

pub(crate) const FOREGROUND_OWNERS: usize = 32;
pub(crate) const OWNER_SLOTS: usize = FOREGROUND_OWNERS + 2;
const MAINTENANCE_SLOT: usize = FOREGROUND_OWNERS;
const FLUSHER_SLOT: usize = FOREGROUND_OWNERS + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerRole {
    Foreground,
    Maintenance,
    DedicatedFlusher,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Fenced,
    Poisoned,
    Invariant,
    Capacity,
    GenerationExhausted,
    InvalidRole,
    InstallationPending,
    Busy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConcurrencyError {
    pub(crate) cause: Cause,
    pub(crate) effect: Effect,
}
pub(crate) type Result<T> = std::result::Result<T, ConcurrencyError>;

fn unchanged(cause: Cause) -> ConcurrencyError {
    ConcurrencyError {
        cause,
        effect: Effect::Unchanged,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Free,
    Idle,
    Apply,
    Content,
    Retired,
    #[cfg(test)]
    CommitStage,
    #[cfg(test)]
    CommitPending,
    #[cfg(test)]
    CommitInstalling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnerSlot {
    generation: u64,
    mode: Mode,
}
impl OwnerSlot {
    const EMPTY: Self = Self {
        generation: 0,
        mode: Mode::Free,
    };
}

struct State {
    slots: [OwnerSlot; OWNER_SLOTS],
    active_apply: u16,
    active_content: u16,
    #[cfg(test)]
    installation: test_support::Installation,
    #[cfg(test)]
    installation_generation: u64,
}
impl State {
    const fn new() -> Self {
        Self {
            slots: [OwnerSlot::EMPTY; OWNER_SLOTS],
            active_apply: 0,
            active_content: 0,
            #[cfg(test)]
            installation: test_support::Installation::Open,
            #[cfg(test)]
            installation_generation: 0,
        }
    }

    /// Bounded census; do not clone the state or allocate an auxiliary owner map.
    fn accounting_valid(&self) -> bool {
        let mut apply = 0;
        let mut content = 0;
        for (index, slot) in self.slots.iter().enumerate() {
            match slot.mode {
                Mode::Free if slot.generation < u64::MAX => {}
                Mode::Retired if slot.generation == u64::MAX => {}
                Mode::Idle if slot.generation != 0 => {}
                Mode::Apply if slot.generation != 0 && index != FLUSHER_SLOT => apply += 1,
                Mode::Content if slot.generation != 0 => content += 1,
                #[cfg(test)]
                Mode::CommitStage | Mode::CommitInstalling
                    if slot.generation != 0 && index < FOREGROUND_OWNERS =>
                {
                    apply += 1;
                }
                #[cfg(test)]
                Mode::CommitPending if slot.generation != 0 && index < FOREGROUND_OWNERS => {}
                _ => return false,
            }
        }
        if apply != self.active_apply
            || content != self.active_content
            || apply > 33
            || apply + content > OWNER_SLOTS as u16
        {
            return false;
        }
        true
    }

    fn valid(&self) -> bool {
        if !self.accounting_valid() {
            return false;
        }
        #[cfg(test)]
        if !test_support::valid_installation(self) {
            return false;
        }
        true
    }

    fn installation_open(&self) -> bool {
        #[cfg(test)]
        {
            self.installation == test_support::Installation::Open
        }
        #[cfg(not(test))]
        {
            true
        }
    }
}

/// Borrow one storage session's permanent fence. Assembly supplies exactly one
/// core per session; no self-reference, namespace reopening or fence reset occurs.
pub(crate) struct PageConcurrency<'f> {
    fence: &'f SessionFence,
    state: Mutex<State>,
    version: Mutex<()>,
    #[cfg(test)]
    hooks: Mutex<test_support::Hooks>,
}

impl<'f> PageConcurrency<'f> {
    #[cfg(test)]
    pub(crate) fn diagnostic_version_busy(&self) -> bool {
        matches!(self.version.try_lock(), Err(TryLockError::WouldBlock))
    }

    /// Assembly checks the original fence by identity; a matching value is not
    /// another session's authority. This neither admits work nor resets a fence.
    pub(crate) fn uses_fence(&self, fence: &SessionFence) -> bool {
        std::ptr::eq(self.fence, fence)
    }

    /// Constructor owns no heap, page, file or WAL space. Concrete control-size
    /// ceilings do not bound external callers' stacks or establish global 64 KiB admission.
    pub(crate) fn new(fence: &'f SessionFence) -> Result<Self> {
        if fence.is_fenced() {
            return Err(ConcurrencyError {
                cause: Cause::Fenced,
                effect: Effect::CallerMustResolve,
            });
        }
        if std::mem::size_of::<Self>() > 2048
            || std::mem::size_of::<Owner<'_, '_>>() > 64
            || std::mem::size_of::<ApplyAdmission<'_, '_, '_>>() > 80
            || std::mem::size_of::<ContentAdmission<'_, '_, '_>>() > 80
            || std::mem::size_of::<VersionEditGuard<'_, '_, '_>>() > 384
        {
            return Err(unchanged(Cause::Capacity));
        }
        Ok(Self {
            fence,
            state: Mutex::new(State::new()),
            version: Mutex::new(()),
            #[cfg(test)]
            hooks: Mutex::new(test_support::Hooks::default()),
        })
    }

    fn fatal(&self, cause: Cause) -> ConcurrencyError {
        self.fence.fence();
        ConcurrencyError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }

    fn live(&self) -> Result<()> {
        if self.fence.is_fenced() {
            Err(ConcurrencyError {
                cause: Cause::Fenced,
                effect: Effect::CallerMustResolve,
            })
        } else {
            Ok(())
        }
    }

    /// Blocking leaf mutex; no caller callback, I/O or upper lock acquisition.
    /// Returned poison remains set. The second fence check follows lock acquisition.
    fn entry_state(&self) -> Result<MutexGuard<'_, State>> {
        self.live()?;
        let state = self.state.lock().map_err(|_| self.fatal(Cause::Poisoned))?;
        self.live()?;
        if !state.valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(state)
    }

    fn cleanup_state(&self) -> (MutexGuard<'_, State>, Option<ConcurrencyError>) {
        #[cfg(test)]
        test_support::hit_pause(self, test_support::PausePoint::BeforeCleanupLock);
        // Cleanup must still return known ownership after a fence. Preserve the
        // first fatal observation while acquiring bookkeeping access.
        let mut failure = self.live().err();
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let error = self.fatal(Cause::Poisoned);
                failure = failure.or(Some(error));
                // Never clear poison or admit service from recovered bookkeeping.
                poison.into_inner()
            }
        };
        failure = failure.or_else(|| self.live().err());
        (state, failure)
    }

    fn fence_unwind(&self) {
        if std::thread::panicking() {
            self.fence.fence();
            #[cfg(test)]
            test_support::hit_pause(self, test_support::PausePoint::FencedBeforeRelease);
        }
    }

    /// Reserve one role's fixed slot. Pending installation does not prohibit
    /// owning an Idle handle or entering content; it prohibits apply admission.
    /// Capacity/generation rejection changes no slot or counter.
    pub(crate) fn try_owner(&self, role: OwnerRole) -> Result<Owner<'_, 'f>> {
        let mut state = self.entry_state()?;
        let (start, end) = match role {
            OwnerRole::Foreground => (0, FOREGROUND_OWNERS),
            OwnerRole::Maintenance => (MAINTENANCE_SLOT, MAINTENANCE_SLOT + 1),
            OwnerRole::DedicatedFlusher => (FLUSHER_SLOT, FLUSHER_SLOT + 1),
        };
        let mut occupied = false;
        for index in start..end {
            let slot = &mut state.slots[index];
            if slot.mode == Mode::Free {
                let generation = slot
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| self.fatal(Cause::Invariant))?;
                *slot = OwnerSlot {
                    generation,
                    mode: Mode::Idle,
                };
                return Ok(Owner {
                    core: self,
                    index,
                    generation,
                    _thread: PhantomData,
                });
            }
            occupied |= slot.mode != Mode::Retired;
        }
        Err(unchanged(if occupied {
            Cause::Capacity
        } else {
            Cause::GenerationExhausted
        }))
    }
}

/// Non-transferable slot authority. Children borrow it exclusively; dropping a
/// healthy Idle owner frees or retires its exact generation without resetting it.
pub(crate) struct Owner<'c, 'f> {
    core: &'c PageConcurrency<'f>,
    index: usize,
    generation: u64,
    _thread: PhantomData<Rc<()>>,
}

impl<'c, 'f> Owner<'c, 'f> {
    /// Distinct cores may borrow the same fence, so consumers bind this exact
    /// core before charging pins or entering an admission section.
    pub(crate) fn belongs_to(&self, core: &PageConcurrency<'_>) -> bool {
        std::ptr::eq(self.core, core)
    }

    pub(crate) fn check_idle(&self) -> Result<()> {
        let state = self.core.entry_state()?;
        self.check_mode(&state, Mode::Idle)
    }

    /// Known session/state failure wins; a foreign healthy core is rejected
    /// before checking this owner's mode or altering either core's accounting.
    pub(crate) fn check_binding(&self, expected: &PageConcurrency<'_>) -> Result<bool> {
        let state = self.core.entry_state()?;
        if !self.belongs_to(expected) {
            return Ok(false);
        }
        self.check_mode(&state, Mode::Idle)?;
        Ok(true)
    }

    fn check_mode(&self, state: &State, mode: Mode) -> Result<()> {
        if state.slots[self.index]
            != (OwnerSlot {
                generation: self.generation,
                mode,
            })
        {
            return Err(self.core.fatal(Cause::Invariant));
        }
        Ok(())
    }

    /// No pin or latch is acquired. Errors retain this Idle owner. The sole
    /// flusher role cannot apply; expected installation exclusion is Unchanged.
    pub(crate) fn try_apply(&mut self) -> Result<ApplyAdmission<'_, 'c, 'f>> {
        {
            let mut state = self.core.entry_state()?;
            self.check_mode(&state, Mode::Idle)?;
            if self.index == FLUSHER_SLOT {
                return Err(unchanged(Cause::InvalidRole));
            }
            if !state.installation_open() {
                return Err(unchanged(Cause::InstallationPending));
            }
            let count = state
                .active_apply
                .checked_add(1)
                .filter(|count| *count <= 33)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            state.active_apply = count;
            state.slots[self.index].mode = Mode::Apply;
        }
        Ok(ApplyAdmission {
            core: self.core,
            owner: Some(self),
        })
    }

    /// Admission for a bounded existing-image content section, including during
    /// pending installation. The eventual consumer must acquire its actual pin/
    /// content latch separately and perform no I/O or admission wait in this scope.
    pub(crate) fn try_content(&mut self) -> Result<ContentAdmission<'_, 'c, 'f>> {
        {
            let mut state = self.core.entry_state()?;
            self.check_mode(&state, Mode::Idle)?;
            let count = state
                .active_content
                .checked_add(1)
                .filter(|count| usize::from(*count) <= OWNER_SLOTS)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            state.active_content = count;
            state.slots[self.index].mode = Mode::Content;
        }
        Ok(ContentAdmission {
            core: self.core,
            owner: Some(self),
        })
    }

    fn release_admission(&self, mode: Mode) -> Result<()> {
        self.core.fence_unwind();
        let (mut state, failure) = self.core.cleanup_state();
        let cleanup = (|| -> Result<()> {
            if !state.accounting_valid() {
                return Err(self.core.fatal(Cause::Invariant));
            }
            self.check_mode(&state, mode)?;
            let counter = match mode {
                Mode::Apply => &mut state.active_apply,
                Mode::Content => &mut state.active_content,
                _ => return Err(self.core.fatal(Cause::Invariant)),
            };
            let count = counter
                .checked_sub(1)
                .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
            *counter = count;
            state.slots[self.index].mode = Mode::Idle;
            Ok(())
        })();
        match failure {
            Some(error) => Err(error),
            None => cleanup,
        }
    }
}

impl Drop for Owner<'_, '_> {
    fn drop(&mut self) {
        self.core.fence_unwind();
        let (mut state, _) = self.core.cleanup_state();
        if !state.accounting_valid() {
            self.core.fence.fence();
            return;
        }
        // Forgotten live children are corruption, not permission to erase their
        // admission. Preserve the evidence and permanently fence this instance.
        if self.check_mode(&state, Mode::Idle).is_err() {
            return;
        }
        state.slots[self.index].mode = if self.generation == u64::MAX {
            Mode::Retired
        } else {
            Mode::Free
        };
    }
}

pub(crate) struct ApplyAdmission<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    owner: Option<&'o mut Owner<'c, 'f>>,
}

impl<'o, 'c, 'f> ApplyAdmission<'o, 'c, 'f> {
    /// Consume admission. Every error releases local state/gate locks before
    /// explicit cleanup; a fatal cleanup observation upgrades expected rejection.
    pub(crate) fn try_version(self) -> Result<VersionEditGuard<'o, 'c, 'f>> {
        let mut guard = VersionEditGuard {
            core: self.core,
            gate: None,
            admission: Some(self),
        };
        match guard.acquire_version() {
            Ok(()) => Ok(guard),
            Err(error) => Err(guard.reject(error)),
        }
    }

    fn release_checked(mut self) -> Result<()> {
        match self.owner.take() {
            Some(owner) => owner.release_admission(Mode::Apply),
            None => Err(self.core.fatal(Cause::Invariant)),
        }
    }
}

impl Drop for ApplyAdmission<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            let _ = owner.release_admission(Mode::Apply);
        }
    }
}

pub(crate) struct ContentAdmission<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    owner: Option<&'o mut Owner<'c, 'f>>,
}
impl ContentAdmission<'_, '_, '_> {
    /// Consumers release actual content latches first, then retain a cleanup
    /// failure instead of returning an earlier healthy Busy rejection.
    pub(crate) fn release_checked(mut self) -> Result<()> {
        match self.owner.take() {
            Some(owner) => owner.release_admission(Mode::Content),
            None => Err(self.core.fatal(Cause::Invariant)),
        }
    }
}
impl Drop for ContentAdmission<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            let _ = owner.release_admission(Mode::Content);
        }
    }
}

/// Owns the actual exclusive version mutex and the counted application scope.
/// Drop fences unwind before exposing the gate, then releases admission exactly once.
pub(crate) struct VersionEditGuard<'o, 'c, 'f> {
    core: &'c PageConcurrency<'f>,
    gate: Option<MutexGuard<'c, ()>>,
    admission: Option<ApplyAdmission<'o, 'c, 'f>>,
}
impl<'o, 'c, 'f> VersionEditGuard<'o, 'c, 'f> {
    /// Called after consumer content guards are released. The gate is returned
    /// before the leaf admission state is acquired for checked cleanup.
    pub(crate) fn release_checked(mut self) -> Result<()> {
        self.core.fence_unwind();
        drop(self.gate.take());
        match self.admission.take() {
            Some(admission) => admission.release_checked(),
            None => Err(self.core.fatal(Cause::Invariant)),
        }
    }

    fn acquire_version(&mut self) -> Result<()> {
        let admission = self
            .admission
            .as_ref()
            .ok_or_else(|| self.core.fatal(Cause::Invariant))?;
        let core: &'c PageConcurrency<'f> = admission.core;
        let owner = admission
            .owner
            .as_deref()
            .ok_or_else(|| core.fatal(Cause::Invariant))?;
        {
            let state = core.entry_state()?;
            owner.check_mode(&state, Mode::Apply)?;
            if !state.installation_open() {
                return Err(unchanged(Cause::InstallationPending));
            }
        }
        #[cfg(test)]
        test_support::hit_pause(core, test_support::PausePoint::AfterVersionPrecheck);
        self.gate = Some(match core.version.try_lock() {
            Ok(gate) => gate,
            Err(TryLockError::WouldBlock) => {
                core.live()?;
                return Err(unchanged(Cause::Busy));
            }
            Err(TryLockError::Poisoned(_)) => return Err(core.fatal(Cause::Poisoned)),
        });
        // The wrapper owns the gate before a post-acquisition hook or check can
        // unwind, so construction failures fence before the actual gate release.
        #[cfg(test)]
        test_support::hit_pause(core, test_support::PausePoint::AfterVersionLock);
        {
            let state = core.entry_state()?;
            owner.check_mode(&state, Mode::Apply)?;
            if !state.installation_open() {
                return Err(unchanged(Cause::InstallationPending));
            }
        }
        Ok(())
    }

    fn reject(mut self, original: ConcurrencyError) -> ConcurrencyError {
        drop(self.gate.take());
        let cleanup = self.admission.take().map(ApplyAdmission::release_checked);
        if original.effect == Effect::CallerMustResolve {
            original
        } else {
            match cleanup {
                Some(Err(error)) => error,
                Some(Ok(())) => original,
                None => self.core.fatal(Cause::Invariant),
            }
        }
    }
}
impl Drop for VersionEditGuard<'_, '_, '_> {
    fn drop(&mut self) {
        self.core.fence_unwind();
        drop(self.gate.take());
        drop(self.admission.take());
    }
}

#[cfg(test)]
#[path = "concurrency_tests.rs"]
mod tests;
