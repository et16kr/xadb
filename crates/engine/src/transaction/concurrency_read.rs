//! Whole read-operation lifetime in the original admission slots. This tracks
//! finite cleanup across page access and I/O; it does not reserve source bytes.
use super::*;

/// Kept by the physical read adapter while it reborrows its original Owner.
/// There is no Clone or public constructor. A forgotten ticket leaves the slot
/// occupied; Owner cleanup fences instead of freeing that generation.
pub(crate) struct ReadOperationAdmission<'c, 'f> {
    core: Option<&'c PageConcurrency<'f>>,
    owner: OwnerIdentity,
}

impl<'c, 'f> Owner<'c, 'f> {
    /// Observe admission priority without claiming a slot. A subsequent entry
    /// must check again; this grants no right across a concurrent Drain.
    pub(crate) fn check_read_operation(&self, expected: &PageConcurrency<'_>) -> Result<()> {
        let state = self.core.entry_state()?;
        self.check_read_entry(expected, &state)
    }

    fn check_read_entry(&self, expected: &PageConcurrency<'_>, state: &State) -> Result<()> {
        if !self.belongs_to(expected) || self.index == FLUSHER_SLOT {
            return Err(unchanged(Cause::InvalidRole));
        }
        self.check_mode(state, Mode::Idle)?;
        if state.slots[self.index].read_operation {
            return Err(unchanged(Cause::Busy));
        }
        if state.read_gate.is_some()
            || matches!(
                state.installation,
                Installation::Draining(_, _) | Installation::Installing(_, _)
            )
        {
            return Err(unchanged(Cause::InstallationPending));
        }
        Ok(())
    }
    /// Enter before the first transient reference or physical read. Pending
    /// Commit permits readers; a terminal read gate and Draining/Installing
    /// reject new operations. This does not relax try_content's Drain exclusion.
    pub(crate) fn try_read_operation(
        &mut self,
        expected: &PageConcurrency<'_>,
    ) -> Result<ReadOperationAdmission<'c, 'f>> {
        let mut state = self.core.entry_state()?;
        self.check_read_entry(expected, &state)?;
        state.slots[self.index].read_operation = true;
        Ok(ReadOperationAdmission {
            core: Some(self.core),
            owner: OwnerIdentity {
                index: self.index as u8,
                generation: self.generation,
            },
        })
    }
}

impl ReadOperationAdmission<'_, '_> {
    /// Consume the ticket after the source loan has been closed. Exact original
    /// core/OwnerIdentity remains authoritative; Drop is inert after this call.
    pub(crate) fn finish_checked(mut self) -> Result<()> {
        self.release_checked()
    }

    /// Return after all page guards, lookup claims and transient refs. Cleanup
    /// remains possible after a fence/poison, but never reports those as success.
    /// Repeated close is inert so the adapter's explicit close and Drop agree.
    pub(crate) fn release_checked(&mut self) -> Result<()> {
        let Some(core) = self.core.take() else {
            return Ok(());
        };
        core.fence_unwind();
        let (mut state, failure) = core.cleanup_state();
        let cleanup = (|| {
            if !state.accounting_valid() {
                return Err(core.fatal(Cause::Invariant));
            }
            let slot = &mut state.slots[self.owner.index()];
            if slot.generation != self.owner.generation()
                || slot.mode != Mode::Idle
                || !slot.read_operation
            {
                return Err(core.fatal(Cause::Invariant));
            }
            slot.read_operation = false;
            Ok(())
        })();
        drop(state);
        core.continuation.notify_all();
        match failure {
            Some(error) => Err(error),
            None => cleanup,
        }
    }
}

impl Drop for ReadOperationAdmission<'_, '_> {
    fn drop(&mut self) {
        let _ = self.release_checked();
    }
}

/// Terminal progress: close new read operations, then wait only for those
/// already admitted. Each admitted operation returns its source loan before its
/// slot flag, so an empty flag set proves no reader still holds a short charge.
/// Readers never wait on the terminal, so the bounded wait is the same finite
/// dependency as Drain. No Clone or public constructor; Drop reopens reads.
pub(crate) struct TerminalReadGate<'c, 'f> {
    core: Option<&'c PageConcurrency<'f>>,
    owner: OwnerIdentity,
}

impl<'c, 'f> Owner<'c, 'f> {
    /// The terminal owner holds no page guard, pin or version gate here. Busy
    /// and installation exclusion return Unchanged before the gate is closed.
    pub(crate) fn close_reads_for_terminal(
        &mut self,
        expected: &PageConcurrency<'_>,
    ) -> Result<TerminalReadGate<'c, 'f>> {
        self.close_reads(expected, FOREGROUND_OWNERS)
    }

    /// Retirement also belongs to the original maintenance owner. Commit keeps
    /// its foreground-only entry; neither entry admits the dedicated flusher.
    pub(crate) fn close_reads_for_retirement(
        &mut self,
        expected: &PageConcurrency<'_>,
    ) -> Result<TerminalReadGate<'c, 'f>> {
        self.close_reads(expected, FLUSHER_SLOT)
    }

    fn close_reads(
        &mut self,
        expected: &PageConcurrency<'_>,
        owner_limit: usize,
    ) -> Result<TerminalReadGate<'c, 'f>> {
        let owner = {
            let mut state = self.core.entry_state()?;
            if !self.belongs_to(expected) || self.index >= owner_limit {
                return Err(unchanged(Cause::InvalidRole));
            }
            self.check_mode(&state, Mode::Idle)?;
            if state.slots[self.index].read_operation || state.read_gate.is_some() {
                return Err(unchanged(Cause::Busy));
            }
            if !state.installation_open() {
                return Err(unchanged(Cause::InstallationPending));
            }
            let owner = OwnerIdentity {
                index: self.index as u8,
                generation: self.generation,
            };
            state.read_gate = Some(owner.index);
            owner
        };
        let gate = TerminalReadGate {
            core: Some(self.core),
            owner,
        };
        gate.wait_admitted_reads()?;
        Ok(gate)
    }
}

impl TerminalReadGate<'_, '_> {
    /// Exact core identity for the consumer's own ledger binding check.
    pub(crate) fn belongs_to(&self, core: &PageConcurrency<'_>) -> bool {
        self.core.is_some_and(|own| std::ptr::eq(own, core))
    }

    fn matches(&self, state: &State) -> bool {
        state.read_gate == Some(self.owner.index)
            && state.slots[self.owner.index()].generation == self.owner.generation
    }

    fn wait_admitted_reads(&self) -> Result<()> {
        let core = self.core.expect("owned terminal read gate");
        let mut state = core.entry_state()?;
        #[cfg(test)]
        let mut announced_wait = false;
        loop {
            core.live()?;
            if !state.valid() || !self.matches(&state) {
                return Err(core.fatal(Cause::Invariant));
            }
            if !state.slots.iter().any(|slot| slot.read_operation) {
                return Ok(());
            }
            #[cfg(test)]
            if !announced_wait {
                announced_wait = true;
                drop(state);
                super::catalog_test_support::hit(
                    core,
                    super::catalog_test_support::Point::ReadGateWaiting,
                );
                state = core.entry_state()?;
                continue;
            }
            state = match core
                .continuation
                .wait_timeout(state, super::commit::FENCE_RECHECK)
            {
                Ok((state, _)) => state,
                Err(poison) => {
                    let error = core.fatal(Cause::Poisoned);
                    drop(poison.into_inner());
                    return Err(error);
                }
            };
        }
    }

    /// Reopen after the terminal's foreground-exclusive phase. Cleanup still
    /// clears this exact gate after a fence, but never reports that as success.
    pub(crate) fn reopen_checked(mut self) -> Result<()> {
        self.release_checked()
    }

    fn release_checked(&mut self) -> Result<()> {
        let Some(core) = self.core.take() else {
            return Ok(());
        };
        core.fence_unwind();
        let (mut state, failure) = core.cleanup_state();
        let cleanup = if self.matches(&state) {
            state.read_gate = None;
            if state.accounting_valid() {
                Ok(())
            } else {
                Err(core.fatal(Cause::Invariant))
            }
        } else {
            Err(core.fatal(Cause::Invariant))
        };
        drop(state);
        core.continuation.notify_all();
        match failure {
            Some(error) => Err(error),
            None => cleanup,
        }
    }
}

impl Drop for TerminalReadGate<'_, '_> {
    fn drop(&mut self) {
        let _ = self.release_checked();
    }
}

/// Close/wait and reopen temporaries of the gate; the caller separately counts
/// the retained gate value. The mutex guard borrows State; nothing is copied.
/// State::valid/accounting_valid run under the guard on entry, in the wait
/// loop and in cleanup, so their larger closure is included once.
pub(crate) const fn terminal_gate_source_bytes() -> usize {
    use std::mem::size_of;
    let accounting = size_of::<std::slice::Iter<'_, OwnerSlot>>()
        + size_of::<std::iter::Enumerate<std::slice::Iter<'_, OwnerSlot>>>()
        + 2 * size_of::<(usize, &OwnerSlot)>()
        + 5 * size_of::<&()>()
        + 2 * size_of::<u16>()
        + size_of::<[usize; 8]>()
        + 8 * size_of::<bool>();
    let installation = 2 * size_of::<Installation>()
        + 2 * size_of::<Option<(InstallationKey, Mode)>>()
        + 2 * size_of::<InstallationKey>()
        + 2 * size_of::<Mode>()
        + 2 * size_of::<std::iter::Enumerate<std::slice::Iter<'_, OwnerSlot>>>()
        + size_of::<(usize, &OwnerSlot)>()
        + size_of::<[usize; 8]>()
        + 6 * size_of::<&()>()
        + 8 * size_of::<bool>();
    let validation = if accounting > installation {
        accounting
    } else {
        installation
    };
    validation
        + 2 * size_of::<TerminalReadGate<'_, '_>>()
        + 2 * size_of::<Result<TerminalReadGate<'_, '_>>>()
        + 4 * size_of::<MutexGuard<'_, State>>()
        + 2 * size_of::<Result<MutexGuard<'_, State>>>()
        + 2 * size_of::<std::sync::LockResult<(MutexGuard<'_, State>, std::sync::WaitTimeoutResult)>>(
        )
        + size_of::<std::slice::Iter<'_, OwnerSlot>>()
        + 2 * size_of::<Option<u8>>()
        + 2 * size_of::<OwnerIdentity>()
        + 2 * size_of::<Option<ConcurrencyError>>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<ConcurrencyError>()
        + size_of::<std::time::Duration>()
        + 10 * size_of::<&()>()
        // The two public entries retain their borrows while the shared close
        // helper holds its role limit. Count that extra frame explicitly.
        + 5 * size_of::<usize>()
        + 8 * size_of::<bool>()
}

/// Entry/cleanup temporaries; the caller separately counts its retained ticket.
/// The mutex guard borrows State; no slot array or new manager is copied.
pub(crate) const fn source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<ReadOperationAdmission<'_, '_>>()
        + 2 * size_of::<Result<ReadOperationAdmission<'_, '_>>>()
        + 2 * size_of::<MutexGuard<'_, State>>()
        + 2 * size_of::<Result<MutexGuard<'_, State>>>()
        + size_of::<OwnerIdentity>()
        + 4 * size_of::<ConcurrencyError>()
        + 8 * size_of::<&()>()
        + 2 * size_of::<usize>()
}

/// Observation only: same-core Owner slots, guardless waiting, guarded work,
/// and guardless return carriers. This is not wired into source admission.
/// The DedicatedFlusher also reaches entry_state before InvalidRole rejection.
#[cfg(test)]
pub(crate) fn entry_source_observation() -> (usize, usize, usize, usize) {
    use std::mem::size_of;
    // cleanup's self/core Option/extracted core/cleanup_state/live arguments;
    // its first failure remains while waiting. Entry has fewer arguments.
    let waiting = 5 * size_of::<&()>()
        + 2 * size_of::<Option<ConcurrencyError>>()
        + 4 * size_of::<Result<()>>();
    let locks = 4 * size_of::<MutexGuard<'_, State>>()
        + 2 * size_of::<std::sync::LockResult<MutexGuard<'_, State>>>()
        + 2 * size_of::<Result<MutexGuard<'_, State>>>()
        + 2 * size_of::<std::sync::PoisonError<MutexGuard<'_, State>>>();
    let accounting = size_of::<std::slice::Iter<'_, OwnerSlot>>()
        + size_of::<std::iter::Enumerate<std::slice::Iter<'_, OwnerSlot>>>()
        + 2 * size_of::<(usize, &OwnerSlot)>()
        + 5 * size_of::<&()>()
        + 2 * size_of::<u16>()
        + size_of::<[usize; 8]>()
        + 8 * size_of::<bool>();
    // Include the copied match input, expected tuple, extracted key/mode and
    // returned/extracted iteration values. No State/slot array is copied.
    let installation = 2 * size_of::<Installation>()
        + 2 * size_of::<Option<(InstallationKey, Mode)>>()
        + 2 * size_of::<InstallationKey>()
        + 2 * size_of::<Mode>()
        + 2 * size_of::<std::iter::Enumerate<std::slice::Iter<'_, OwnerSlot>>>()
        + size_of::<(usize, &OwnerSlot)>()
        + size_of::<[usize; 8]>()
        + 6 * size_of::<&()>()
        + 8 * size_of::<bool>();
    let guarded = locks
        + 6 * size_of::<&()>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<Option<ConcurrencyError>>()
        + 2 * size_of::<OwnerIdentity>() // cleanup index()/generation() value receivers
        + accounting.max(installation);
    // Entry's ticket/identity and complete returned Result remain per caller,
    // even after the guard is gone. Do not classify them as serialized work.
    let returning = size_of::<ReadOperationAdmission<'_, '_>>()
        + 2 * size_of::<Result<ReadOperationAdmission<'_, '_>>>()
        + size_of::<OwnerIdentity>()
        + 4 * size_of::<ConcurrencyError>()
        + 3 * size_of::<&()>();
    (OWNER_SLOTS, waiting, guarded, returning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::concurrency::catalog_test_support::observe;

    #[test]
    fn read_operation_precheck_has_no_claim_and_entry_checks_again() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        owner.check_read_operation(&core).unwrap();
        assert_eq!(observe(&core).active_read_operations, 0);
        let mut operation = owner.try_read_operation(&core).unwrap();
        assert_eq!(
            owner.check_read_operation(&core).unwrap_err().cause,
            Cause::Busy
        );
        operation.release_checked().unwrap();
        owner.check_read_operation(&core).unwrap();
        fence.fence();
        assert_eq!(
            owner.try_read_operation(&core).err().unwrap().cause,
            Cause::Fenced
        );
        assert_eq!(observe(&core).active_read_operations, 0);
    }

    #[test]
    fn read_operation_keeps_slot_across_content_and_rejects_apply() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut operation = owner.try_read_operation(&core).unwrap();
        assert_eq!(observe(&core).active_read_operations, 1);
        assert_eq!(
            owner.try_read_operation(&core).err().unwrap().cause,
            Cause::Busy
        );
        assert_eq!(owner.try_apply().err().unwrap().cause, Cause::Busy);
        let content = owner.try_content().unwrap();
        let active = observe(&core);
        assert_eq!(
            (active.active_read_operations, active.active_content),
            (1, 1)
        );
        content.release_checked().unwrap();
        assert_eq!(observe(&core).active_read_operations, 1);
        operation.release_checked().unwrap();
        operation.release_checked().unwrap();
        assert_eq!(observe(&core).active_read_operations, 0);
        owner.try_apply().unwrap().release_checked().unwrap();
        drop(owner.try_read_operation(&core).unwrap());
        drop(owner);
        let mut next = core.try_owner(OwnerRole::Foreground).unwrap();
        let next_operation = next.try_read_operation(&core).unwrap();
        operation.release_checked().unwrap();
        assert_eq!(
            observe(&core).active_read_operations,
            1,
            "old close cannot clear the reused slot"
        );
        drop(next_operation);
        println!(
            "READ_OPERATION_LAYOUT core={} slot={} ticket={} closure={}",
            std::mem::size_of::<PageConcurrency<'_>>(),
            std::mem::size_of::<OwnerSlot>(),
            std::mem::size_of::<ReadOperationAdmission<'_, '_>>(),
            source_bytes()
        );
        assert!(!fence.is_fenced());
    }

    #[test]
    fn read_operation_rejects_foreign_core_and_flusher_without_effects() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let other = PageConcurrency::new(&fence).unwrap();
        for role in [OwnerRole::Foreground, OwnerRole::DedicatedFlusher] {
            let mut owner = core.try_owner(role).unwrap();
            let expected = if role == OwnerRole::Foreground {
                &other
            } else {
                &core
            };
            let error = owner.try_read_operation(expected).err().unwrap();
            assert_eq!(
                (error.cause, error.effect),
                (Cause::InvalidRole, Effect::Unchanged)
            );
            assert_eq!(observe(&core).active_read_operations, 0);
            assert_eq!(observe(&other).active_read_operations, 0);
            owner.check_idle().unwrap();
        }
        let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
        drop(maintenance.try_read_operation(&core).unwrap());
        assert!(!fence.is_fenced());
    }

    #[test]
    fn read_operation_forget_and_premature_release_preserve_ownership() {
        for forget in [true, false] {
            let fence = SessionFence::new();
            let core = PageConcurrency::new(&fence).unwrap();
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut operation = owner.try_read_operation(&core).unwrap();
            if forget {
                std::mem::forget(operation);
                drop(owner);
            } else {
                let content = owner.try_content().unwrap();
                let error = operation.release_checked().unwrap_err();
                assert_eq!(error.effect, Effect::CallerMustResolve);
                drop(content);
            }
            assert!(fence.is_fenced());
            assert_eq!(observe(&core).active_read_operations, 1);
            assert_eq!(
                core.try_owner(OwnerRole::Foreground).err().unwrap().cause,
                Cause::Fenced
            );
        }
    }

    #[test]
    fn read_operation_cleanup_returns_known_slot_after_poison_or_fence() {
        for poison in [false, true] {
            let fence = SessionFence::new();
            let core = PageConcurrency::new(&fence).unwrap();
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut operation = owner.try_read_operation(&core).unwrap();
            if poison {
                test_support::poison_state(&core);
            } else {
                fence.fence();
            }
            let error = operation.release_checked().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(fence.is_fenced());
            assert_eq!(observe(&core).active_read_operations, 0);
            assert_eq!(core.state.is_poisoned(), poison);
        }
    }

    #[test]
    fn read_operation_unwind_fences_before_returning_slot() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _operation = owner.try_read_operation(&core).unwrap();
                panic!("bounded operation unwind");
            }))
            .is_err()
        );
        assert!(fence.is_fenced());
        assert_eq!(observe(&core).active_read_operations, 0);
    }

    #[test]
    fn terminal_gate_closes_new_reads_and_waits_for_admitted_reads() {
        use crate::transaction::concurrency::catalog_test_support::{Point, Resume, install};
        use std::{sync::mpsc::sync_channel, time::Duration};
        const WAIT: Duration = Duration::from_secs(5);
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut reader = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
        let operation = reader.try_read_operation(&core).unwrap();
        let (entered, arrived) = sync_channel(1);
        let (resume, release) = sync_channel(1);
        install(&core, Point::ReadGateWaiting, entered, release);
        let (closed_tx, closed) = sync_channel(1);
        let (reopen_tx, reopen) = sync_channel::<()>(1);
        std::thread::scope(|scope| {
            let core_ref = &core;
            let terminal = scope.spawn(move || {
                let mut owner = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                let gate = owner.close_reads_for_terminal(core_ref).unwrap();
                closed_tx.send(()).unwrap();
                reopen.recv_timeout(WAIT).unwrap();
                gate.reopen_checked().unwrap();
                owner.check_idle().unwrap();
            });
            arrived.recv_timeout(WAIT).unwrap();
            let waiting = observe(&core);
            assert!(waiting.read_gate_closed);
            assert_eq!(waiting.active_read_operations, 1);
            for error in [
                late.check_read_operation(&core).unwrap_err(),
                late.try_read_operation(&core).err().unwrap(),
            ] {
                assert_eq!(
                    (error.cause, error.effect),
                    (Cause::InstallationPending, Effect::Unchanged)
                );
            }
            resume.send(Resume::Continue).unwrap();
            // The admitted operation is never cancelled to make progress.
            assert!(closed.recv_timeout(Duration::from_millis(50)).is_err());
            assert!(observe(&core).read_gate_closed);
            drop(operation);
            closed.recv_timeout(WAIT).unwrap();
            let closed_state = observe(&core);
            assert!(closed_state.read_gate_closed);
            assert_eq!(closed_state.active_read_operations, 0);
            assert_eq!(
                late.try_read_operation(&core).err().unwrap().cause,
                Cause::InstallationPending
            );
            reopen_tx.send(()).unwrap();
            terminal.join().unwrap();
        });
        assert!(!observe(&core).read_gate_closed);
        drop(late.try_read_operation(&core).unwrap());
        reader.check_idle().unwrap();
        println!(
            "TERMINAL_READ_GATE core={} gate={} closure={}",
            std::mem::size_of::<PageConcurrency<'_>>(),
            std::mem::size_of::<TerminalReadGate<'_, '_>>(),
            terminal_gate_source_bytes()
        );
        assert!(!fence.is_fenced());
    }

    #[test]
    fn retirement_gate_preserves_maintenance_and_rejects_flusher_and_foreign() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let foreign = PageConcurrency::new(&fence).unwrap();
        let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        for error in [
            maintenance.close_reads_for_terminal(&core).err().unwrap(),
            maintenance
                .close_reads_for_retirement(&foreign)
                .err()
                .unwrap(),
            flusher.close_reads_for_retirement(&core).err().unwrap(),
        ] {
            assert_eq!(
                (error.cause, error.effect),
                (Cause::InvalidRole, Effect::Unchanged)
            );
        }
        let gate = maintenance.close_reads_for_retirement(&core).unwrap();
        let mut reader = core.try_owner(OwnerRole::Foreground).unwrap();
        assert_eq!(
            reader.try_read_operation(&core).err().unwrap().cause,
            Cause::InstallationPending
        );
        gate.reopen_checked().unwrap();
        reader
            .try_read_operation(&core)
            .unwrap()
            .finish_checked()
            .unwrap();
        maintenance.check_idle().unwrap();
        flusher.check_idle().unwrap();
        assert!(!fence.is_fenced());
    }

    #[test]
    fn terminal_gate_rejections_drop_and_fence_cleanup() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let foreign = PageConcurrency::new(&fence).unwrap();
        let mut terminal = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
        // An owner's own admitted read cannot be waited for by itself.
        let own = terminal.try_read_operation(&core).unwrap();
        assert_eq!(
            terminal
                .close_reads_for_terminal(&core)
                .err()
                .unwrap()
                .cause,
            Cause::Busy
        );
        drop(own);
        for error in [
            maintenance.close_reads_for_terminal(&core).err().unwrap(),
            terminal.close_reads_for_terminal(&foreign).err().unwrap(),
        ] {
            assert_eq!(
                (error.cause, error.effect),
                (Cause::InvalidRole, Effect::Unchanged)
            );
        }
        assert!(!observe(&core).read_gate_closed);
        // One terminal gate at a time; Drop reopens without an explicit close.
        let gate = terminal.close_reads_for_terminal(&core).unwrap();
        assert_eq!(
            other.close_reads_for_terminal(&core).err().unwrap().cause,
            Cause::Busy
        );
        drop(gate);
        assert!(!observe(&core).read_gate_closed);
        let operation = other.try_read_operation(&core).unwrap();
        // A fence during the wait is returned and the exact gate is cleared.
        std::thread::scope(|scope| {
            let (core_ref, fence_ref) = (&core, &fence);
            let fencing = scope.spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while !observe(core_ref).read_gate_closed {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::yield_now();
                }
                fence_ref.fence();
            });
            let error = terminal.close_reads_for_terminal(&core).err().unwrap();
            assert_eq!(
                (error.cause, error.effect),
                (Cause::Fenced, Effect::CallerMustResolve)
            );
            fencing.join().unwrap();
        });
        let fenced = observe(&core);
        assert!(!fenced.read_gate_closed);
        assert_eq!(fenced.active_read_operations, 1);
        drop(operation);
        assert_eq!(observe(&core).active_read_operations, 0);
        assert!(observe(&foreign).fenced);
    }

    #[test]
    fn owner_drop_with_live_terminal_gate_fences_and_keeps_slot() {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut terminal = core.try_owner(OwnerRole::Foreground).unwrap();
        let gate = terminal.close_reads_for_terminal(&core).unwrap();
        // A forgotten gate is a live child: the slot is kept, never freed.
        drop(terminal);
        assert!(fence.is_fenced());
        assert!(observe(&core).read_gate_closed);
        // The exact gate still matches its kept slot and is cleared on Drop.
        drop(gate);
        assert!(!observe(&core).read_gate_closed);
    }
}
