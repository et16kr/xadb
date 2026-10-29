//! First actual writable capsule: preprovisioned Status, empty statements and
//! checked empty abort. General Row/CREATE mutation and own reads follow.
use super::*;
use crate::storage::{
    allocation_page::ReservationOwner,
    obligation::{self, OwnerState},
    retention_page::catalog3::{self as queue, JobState},
    semantic_catalog::{Observation, Operation},
    transaction_record::{RollbackState, TransactionState},
};

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The Status must have been provisioned by the original exclusive INIT.
    /// This additive internal entry makes the original flusher loan explicit;
    /// it accepts neither a raw actor nor a caller-selected snapshot.
    pub(in crate::storage) fn begin_catalog_transaction(
        &mut self,
        owner: &mut Owner<'c, 's>,
        isolation: IsolationLevel,
        status: &Plan4Target,
        private: &mut [u8; 425_984],
        flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowTransaction<'b, 'a, 'c, 's>> {
        let _unwind = self.storage.session_fence().unwind_guard();
        let visibility = self.transaction_entry(owner)?;
        let mut transaction = self.reserve_transaction(owner, isolation, visibility)?;
        let observation = match self.catalog_begin(owner, status, private, flusher) {
            Ok(observation) => observation,
            Err(error) => {
                if error.effect != Effect::Unchanged {
                    transaction.lease.fence.fence();
                }
                return merge(Err(error), transaction.close());
            }
        };
        // Begin has installed an obligation. Every subsequent failure is fatal,
        // including source/gate cleanup and the binding of the capsule itself.
        let bound = (|| {
            let actor = observation
                .actor
                .ok_or_else(|| fail(transaction.lease.fence, RetentionCause::Invariant))?;
            // Own the issued identity before any later fallible validation.
            transaction.lease.actor = Some(actor);
            if observation.operation != Operation::Begin || !self.empty_actor_active(actor)? {
                return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
            }
            let mut state = entry(self.isolation, transaction.lease.fence)?;
            check_slot(
                state.bytes,
                transaction.lease.slot,
                transaction.lease.generation,
                transaction.lease.fence,
            )?;
            let target = row_mut(state.bytes, transaction.lease.slot);
            if target[2] != 0 || target[32..80].iter().any(|&b| b != 0) {
                return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
            }
            actor
                .encode(&mut target[32..80])
                .map_err(|_| fail(transaction.lease.fence, RetentionCause::Invariant))?;
            target[2] = 1;
            Ok(())
        })();
        if bound.is_err() {
            return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
        }
        Ok(transaction)
    }

    /// Observe only the original live ATT and its original terminal Job. This
    /// is a preflight, not a receipt or a constructor of a transaction capsule.
    pub(super) fn empty_actor_active(&self, actor: TxnRef) -> Result<bool> {
        let fence = self.storage.session_fence();
        let bad = || fail(fence, RetentionCause::Invariant);
        let prefix = self.storage.wal().prefix().map_err(|_| bad())?;
        self.control
            .check_ledger(self.storage.profile(), prefix)
            .map_err(|_| bad())?;
        let slot = self.control.slot(actor).ok_or_else(bad)?;
        let descriptor = obligation::decode_descriptor(
            self.control.row(slot)[..1536]
                .try_into()
                .expect("ATT descriptor"),
            ReservationOwner::Transaction(actor),
            crate::storage::retention_obligation::profile_reference(self.storage.profile()),
        )
        .map_err(|_| bad())?;
        let OwnerState::Transaction(transaction) = descriptor.owner else {
            return Err(bad());
        };
        if transaction.reference() != actor {
            return Err(bad());
        }
        // No root, including a merely Reserved or Compensated root, may be
        // abandoned by this empty-only operation.
        for bytes in self.control.jobs() {
            let job = queue::decode_job(bytes, self.storage.profile().spec().namespace)
                .map_err(|_| bad())?;
            if job.source() == Some(actor) && job.state().is_root() {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
        }
        if transaction.undo_head().is_some() {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        match transaction.state() {
            TransactionState::Active => Ok(true),
            TransactionState::Aborting(RollbackState::Transaction { next: None }) => Ok(false),
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            )),
        }
    }

    /// Consume an empty writer through the actual RollbackStart and AbortEnd.
    /// A pre-effect rejection returns the same capsule; after RollbackStart an
    /// Unchanged rejection retains MustAbort and can retry this operation only.
    #[allow(
        clippy::result_large_err,
        reason = "same capsule is charged to its original 512-byte lease"
    )]
    pub(in crate::storage) fn abort_empty_transaction<'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut transaction: RowTransaction<'v, 'a, 'c, 's>,
        private: &mut [u8; 425_984],
        flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> std::result::Result<(), TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>> {
        let _unwind = self.storage.session_fence().unwind_guard();
        let checked = (|| {
            self.transaction_entry(owner)?;
            if !std::ptr::eq(self.isolation, transaction.lease.ledger)
                || !self
                    .visibility
                    .is_some_and(|v| std::ptr::eq(v, transaction.lease.visibility))
            {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::BindingMismatch,
                ));
            }
            if !matches!(transaction.phase.get(), Phase::Active | Phase::MustAbort) {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::WrongState,
                ));
            }
            let actor = transaction
                .lease
                .actor
                .ok_or_else(|| unchanged(RetentionPhase::Finalize, RetentionCause::WrongState))?;
            {
                let state = entry(self.isolation, transaction.lease.fence)?;
                transaction.lease.check(state.bytes)?;
                let target = row(state.bytes, transaction.lease.slot);
                let expected = if transaction.phase.get() == Phase::Active {
                    1
                } else {
                    2
                };
                if target[0] != expected {
                    return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
                }
                if read64(target, 80) != 0 {
                    return Err(unchanged(RetentionPhase::Finalize, RetentionCause::Busy));
                }
            }
            if self.empty_actor_active(actor)? {
                if transaction.phase.get() != Phase::Active {
                    return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
                }
                self.catalog_rollback_start(owner, actor, private, flusher)?;
            }
            transaction.set_abort_phase(Phase::MustAbort)?;
            #[cfg(test)]
            crate::transaction::concurrency::catalog_test_support::hit(
                self.pool.bound_core(),
                crate::transaction::concurrency::catalog_test_support::Point::EmptyRollbackCompleted,
            );
            let observation = self.catalog_abort_end(owner, actor, private, flusher)?;
            // No-ATT alone is insufficient. This exact successful AbortEnd and
            // actual TerminalReady/outcome=Aborted must agree before releasing
            // the snapshot. Physical TerminalFinish remains Aging's obligation.
            if observation.operation != Operation::AbortEnd
                || observation.actor != Some(actor)
                || self.control.slot(actor).is_some()
            {
                return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
            }
            let mut terminals = 0;
            for bytes in self.control.jobs() {
                let job = queue::decode_job(bytes, self.storage.profile().spec().namespace)
                    .map_err(|_| fail(transaction.lease.fence, RetentionCause::Invariant))?;
                if job.source() == Some(actor) {
                    if job.state() != JobState::TerminalReady
                        || job.outcome() != 2
                        || job.status() != observation.status
                        || job.commit_q() != 0
                    {
                        return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
                    }
                    terminals += 1;
                }
            }
            if terminals != 1 {
                return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
            }
            transaction.set_abort_phase(Phase::Aborted)?;
            Ok(())
        })();
        match checked {
            Err(error) if error.effect == Effect::Unchanged => {
                if transaction.phase.get() == Phase::MustAbort {
                    Err(TransactionFinishError::RollbackRequired { error, transaction })
                } else {
                    Err(TransactionFinishError::Before { error, transaction })
                }
            }
            Err(error) => {
                transaction.lease.fence.fence();
                Err(TransactionFinishError::Fatal(error))
            }
            Ok(()) => transaction.close().map_err(TransactionFinishError::Fatal),
        }
    }
}

/// The long-lived capsule and its return carrier are already charged by the
/// 512-byte lease. Count this new wrapper's other values through the original
/// synchronous lifecycle call, without last-use/optimizer discounts.
pub(in super::super) fn lifecycle_caller_bytes() -> usize {
    use std::mem::size_of;
    (12 * size_of::<&()>() // public arguments plus closure capture references
        + size_of::<IsolationLevel>()
        + 2 * size_of::<TxnRef>()
        + 2 * size_of::<Result<Observation>>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>())
    .max(super::catalog::terminal_caller_bytes())
}

pub(in super::super) fn source_bytes() -> usize {
    use std::mem::size_of;
    lifecycle_caller_bytes()
        + super::source_bytes()
        + 2 * size_of::<obligation::DescriptorSpec>()
        + 2 * size_of::<Result<obligation::DescriptorSpec>>()
        + 3 * size_of::<queue::JobView<'_>>()
        + 2 * size_of::<Result<queue::JobView<'_>>>()
        + 2 * size_of::<crate::storage::wal_io::WalPrefix>()
        + 2 * size_of::<Result<crate::storage::wal_io::WalPrefix>>()
        + 2 * size_of::<Result<(u64, u64)>>()
        + size_of::<[usize; 6]>()
        + 640 // original check_ledger aggregate inventory, retained during scans
        + 2 * size_of::<[u8; 48]>()
        + obligation::descriptor_source_bytes()
            .max(crate::storage::semantic_catalog::profile3_transaction_source_bytes())
            .max(queue::source_bytes())
}

#[cfg(test)]
impl RowTransaction<'_, '_, '_, '_> {
    pub(in crate::storage) fn test_write_identity(&self) -> (Option<TxnRef>, usize, u64, u8, u64) {
        let state = self.lease.ledger.lock().unwrap();
        (
            self.lease.actor,
            self.lease.slot,
            self.lease.generation,
            row(state.bytes, self.lease.slot)[0],
            read64(state.bytes, 16),
        )
    }
    pub(in crate::storage) fn test_corrupt_readonly_phase(&self, phase: u8) {
        assert!(self.lease.actor.is_none());
        assert!(matches!(phase, 2..=4));
        let mut state = self.lease.ledger.lock().unwrap();
        row_mut(state.bytes, self.lease.slot)[0] = phase;
    }
    pub(in crate::storage) fn test_corrupt_write_identity(&self, offset: usize) {
        assert!(matches!(offset, 0 | 16 | 32));
        let mut state = self.lease.ledger.lock().unwrap();
        row_mut(state.bytes, self.lease.slot)[offset] ^= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writable_capsule_and_wrappers_stay_within_original_limits() {
        fn check<W: WriterNamespace>() {
            let live = resources::source_census::<W>();
            let phases = create_pair::catalog_live::catalog_live_source_bytes::<W>();
            let handles = super::super::handle_census();
            println!(
                "WRITABLE_SOURCE handles={handles:?} wrapper={} helper={} lifecycle={} retirement={} peak={}",
                lifecycle_caller_bytes(),
                source_bytes(),
                phases.lifecycle,
                phases.retirement,
                live.peak
            );
            assert!(handles.0 <= HANDLE_BYTES && handles.1 <= HANDLE_BYTES);
            assert!(
                phases.lifecycle
                    >= super::super::super::source::lifecycle_carrier_bytes::<W>()
                        + lifecycle_caller_bytes()
            );
            assert!(live.peak <= 65536);
        }
        check::<crate::storage::wal_io::plan4_test_support::Writer>();
        check::<super::super::super::super::native::Writer>();
    }
}
