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
    /// Enter before the first transient reference or physical read. Pending
    /// Commit permits readers; Draining/Installing reject new operations. This
    /// does not relax try_content's existing Drain exclusion.
    pub(crate) fn try_read_operation(
        &mut self,
        expected: &PageConcurrency<'_>,
    ) -> Result<ReadOperationAdmission<'c, 'f>> {
        let mut state = self.core.entry_state()?;
        if !self.belongs_to(expected) || self.index == FLUSHER_SLOT {
            return Err(unchanged(Cause::InvalidRole));
        }
        self.check_mode(&state, Mode::Idle)?;
        if state.slots[self.index].read_operation {
            return Err(unchanged(Cause::Busy));
        }
        if matches!(
            state.installation,
            Installation::Draining(_, _) | Installation::Installing(_, _)
        ) {
            return Err(unchanged(Cause::InstallationPending));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::concurrency::catalog_test_support::observe;

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
}
