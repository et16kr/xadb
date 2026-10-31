//! First Catalog-writing statement and consuming transaction completion. The
//! underlying Catalog format still has one statement/operation; no Row mark or
//! arbitrary transaction history is manufactured here.
use super::*;
use crate::storage::{
    allocation_page::ReservationOwner,
    obligation::{self, OwnerState},
    retention_page::catalog3::{self as queue, JobState},
    semantic_catalog::{Observation, Operation},
    transaction_record::{RollbackState, TransactionState},
};
use foundation::identity::ObjectHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActorStage {
    EmptyActive,
    EmptyAborting,
    Active(usize),
    Rollback(usize),
    Compensated(usize),
}

/// Observation of the actual completed Catalog Commit, not a WAL authority.
pub(crate) struct CatalogCommit {
    q: u64,
}
impl CatalogCommit {
    pub(in crate::storage) fn q(&self) -> u64 {
        self.q
    }
}

fn actor_stage<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    control: &control::RetentionLedger<'_, '_, '_>,
    actor: TxnRef,
) -> Result<ActorStage> {
    let bad = || fail(storage.session_fence(), RetentionCause::Invariant);
    let prefix = storage.wal().prefix().map_err(|_| bad())?;
    control
        .check_ledger(storage.profile(), prefix)
        .map_err(|_| bad())?;
    let slot = control.slot(actor).ok_or_else(bad)?;
    let descriptor = obligation::decode_descriptor(
        control.row(slot)[..1536]
            .try_into()
            .expect("ATT descriptor"),
        ReservationOwner::Transaction(actor),
        crate::storage::retention_obligation::profile_reference(storage.profile()),
    )
    .map_err(|_| bad())?;
    let OwnerState::Transaction(transaction) = descriptor.owner else {
        return Err(bad());
    };
    if transaction.reference() != actor {
        return Err(bad());
    }
    let mut root = None;
    let mut terminals = 0;
    // Catalog selection skips every Row-scope cell (another actor's).
    for (ordinal, bytes) in control.jobs().iter().enumerate() {
        let Some(job) = queue::decode_catalog_cell(bytes, storage.profile().spec().namespace)
            .map_err(|_| bad())?
        else {
            continue;
        };
        if job.source() != Some(actor) {
            continue;
        }
        if job.state().is_terminal() {
            if job.state() != JobState::TerminalLeased {
                return Err(bad());
            }
            terminals += 1;
        } else if root.replace((ordinal, job)).is_some() {
            return Err(bad());
        }
    }
    if terminals != 1 {
        return Err(bad());
    }
    let Some((ordinal, root)) = root else {
        if transaction.undo_head().is_some() || descriptor.last_mutation_sequence != 0 {
            return Err(bad());
        }
        return match transaction.state() {
            TransactionState::Active => Ok(ActorStage::EmptyActive),
            TransactionState::Aborting(RollbackState::Transaction { next: None }) => {
                Ok(ActorStage::EmptyAborting)
            }
            _ => Err(bad()),
        };
    };
    if root.state() == JobState::RootReserved {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::Unsupported,
        ));
    }
    if root.statement() != 1 || root.mutation() != 1 || descriptor.last_mutation_sequence != 1 {
        return Err(bad());
    }
    match (root.state(), transaction.state()) {
        (JobState::RootRetained, TransactionState::Active)
            if transaction.undo_head() == root.undo() =>
        {
            Ok(ActorStage::Active(ordinal))
        }
        (
            JobState::RootRetained,
            TransactionState::Aborting(RollbackState::Transaction { next }),
        ) if next == root.undo() && transaction.undo_head() == next => {
            Ok(ActorStage::Rollback(ordinal))
        }
        (
            JobState::RootReleasingCompensated,
            TransactionState::Aborting(RollbackState::Transaction { next: None }),
        ) if transaction.undo_head().is_none() => Ok(ActorStage::Compensated(ordinal)),
        _ => Err(bad()),
    }
}

/// Sealed reference to the original live statement. Scalars never substitute
/// for its slot/generation, snapshot, credit or transaction ownership.
pub(in super::super) struct CreateStatementBinding<'r, 't, 'v, 'a, 'c, 's> {
    view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
}
impl<'r, 't, 'v, 'a, 'c, 's> CreateStatementBinding<'r, 't, 'v, 'a, 'c, 's> {
    pub(in super::super) fn admit<W: WriterNamespace>(
        writer: &MutationPort<'_, 'a, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    ) -> Result<Self> {
        writer.transaction_entry(owner)?;
        writer.transaction_matches(view.transaction)?;
        view.check()?;
        let actor = view
            .transaction
            .lease
            .actor
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        if view.sequence != 1
            || view.revision.get() != 0
            || writer.catalog_actor_stage(actor)? != ActorStage::EmptyActive
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        let binding = Self { view };
        binding.check()?;
        Ok(binding)
    }
    pub(in super::super) fn actor(&self) -> TxnRef {
        self.view
            .transaction
            .lease
            .actor
            .expect("authenticated writable statement")
    }
    /// Conservative dependency begins immediately before the actual planner:
    /// even a later planner rejection cannot erase its Catalog observation.
    pub(in super::super) fn planning(&self) -> Result<()> {
        self.view.record_catalog_read()
    }
    fn check(&self) -> Result<()> {
        self.view.check()?;
        let lease = &self.view.transaction.lease;
        let state = entry(lease.ledger, lease.fence)?;
        lease.check(state.bytes)?;
        if row(state.bytes, lease.slot)[1] > 1 {
            return Err(fail(lease.fence, RetentionCause::Invariant));
        }
        drop(state);
        if self.view.sequence != 1 || self.view.revision.get() != 0 {
            return Err(fail(
                self.view.transaction.lease.fence,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
    pub(in super::super) fn reserved<W: WriterNamespace>(
        &self,
        storage: &MutableStorageOwner<'_, '_, W>,
        control: &control::RetentionLedger<'_, '_, '_>,
        ordinal: usize,
        object: ObjectHandle,
        reserve: PageLsn,
    ) -> Result<()> {
        self.check()?;
        let bad = || fail(self.view.transaction.lease.fence, RetentionCause::Invariant);
        let prefix = storage.wal().prefix().map_err(|_| bad())?;
        control
            .check_ledger(storage.profile(), prefix)
            .map_err(|_| bad())?;
        let job = queue::decode_job(
            control.jobs().get(ordinal).ok_or_else(bad)?,
            storage.profile().spec().namespace,
        )
        .map_err(|_| bad())?;
        if job.state() != JobState::RootReserved
            || job.source() != Some(self.actor())
            || job.object() != Some(object)
            || job.lease_lsn() != reserve.0
        {
            return Err(bad());
        }
        let lease = &self.view.transaction.lease;
        let mut state = entry(lease.ledger, lease.fence)?;
        lease.check(state.bytes)?;
        row_mut(state.bytes, lease.slot)[3] |= 3;
        Ok(())
    }
    pub(in super::super) fn completed<W: WriterNamespace>(
        &self,
        storage: &MutableStorageOwner<'_, '_, W>,
        control: &control::RetentionLedger<'_, '_, '_>,
        object: Option<ObjectHandle>,
    ) -> Result<()> {
        self.check()?;
        let bad = || fail(self.view.transaction.lease.fence, RetentionCause::Invariant);
        match (object, actor_stage(storage, control, self.actor())?) {
            (Some(object), ActorStage::Active(ordinal)) => {
                let job =
                    queue::decode_job(&control.jobs()[ordinal], storage.profile().spec().namespace)
                        .map_err(|_| bad())?;
                if job.object() != Some(object) {
                    return Err(bad());
                }
                self.view.revision.set(1);
                Ok(())
            }
            (None, ActorStage::EmptyActive) => Ok(()),
            _ => Err(bad()),
        }
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Only bounded scalar results leave this checked phase. No decoded owner,
    /// Job view or guard survives into a separately admitted WAL operation.
    pub(super) fn catalog_transaction_check<T>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        check: impl FnOnce(&mut Self, &mut Owner<'c, 's>) -> Result<T>,
    ) -> Result<T> {
        self.transaction_check(owner, source::OperationClass::CatalogProgress, check)
    }

    /// The same checked phase under either fixed progress class. Both reserve
    /// the one CatalogProgressCharge (`OperationSourceAdmission::progress`);
    /// the class names the capsule kind whose check runs inside.
    pub(super) fn transaction_check<T>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        class: source::OperationClass,
        check: impl FnOnce(&mut Self, &mut Owner<'c, 's>) -> Result<T>,
    ) -> Result<T> {
        let _unwind = self.storage.session_fence().unwind_guard();
        self.storage.health().map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Storage(e),
            )
        })?;
        self.pool
            .check_retention_entry_health()
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        if !self
            .pool
            .uses_storage_origin(&self.storage.binding, self.storage.session_fence())
        {
            return Err(fail(
                self.storage.session_fence(),
                RetentionCause::BindingMismatch,
            ));
        }
        let gate = owner
            .close_reads_for_retirement(self.pool.bound_core())
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::Concurrency(e),
                effect: e.effect,
            })?;
        let reopen = |gate: crate::transaction::concurrency::TerminalReadGate<'c, 's>| {
            gate.reopen_checked().map_err(|e| RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::Concurrency(e),
                effect: e.effect,
            })
        };
        let source = match source::OperationSourceAdmission::progress(
            self.isolation,
            &self.commit_charges,
            &self.progress_charge,
            &gate,
            class,
        ) {
            Ok(source) => source,
            Err(error) => return merge(Err(RetentionError::from(error)), reopen(gate)),
        };
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::TransactionChecking,
        );
        let result = check(self, owner);
        let cleanup = source.finish_checked().map_err(RetentionError::from);
        merge(merge(result, cleanup), reopen(gate))
    }

    fn catalog_actor_stage(&self, actor: TxnRef) -> Result<ActorStage> {
        actor_stage(self.storage, self.control, actor)
    }

    /// C-only preflight for the actual CREATE caller. The current wire format
    /// can attach its single mutation only to the first actual empty statement.
    pub(in super::super) fn catalog_statement_create_actor(
        &mut self,
        owner: &mut Owner<'c, 's>,
        view: &RowStatementView<'_, '_, 'a, 'c, 's>,
    ) -> Result<TxnRef> {
        self.catalog_transaction_check(owner, |writer, owner| {
            writer.transaction_entry(owner)?;
            writer.transaction_matches(view.transaction)?;
            view.check()?;
            let actor =
                view.transaction.lease.actor.ok_or_else(|| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::WrongState)
                })?;
            if view.sequence != 1
                || view.revision.get() != 0
                || writer.catalog_actor_stage(actor)? != ActorStage::EmptyActive
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
            Ok(actor)
        })
    }

    /// Called only after the original CREATE initialize returned this object.
    /// Actual control bytes are rechecked before recording the observation.
    pub(in super::super) fn catalog_statement_created(
        &mut self,
        owner: &mut Owner<'c, 's>,
        view: &RowStatementView<'_, '_, 'a, 'c, 's>,
        object: ObjectHandle,
    ) -> Result<()> {
        let result = self.catalog_transaction_check(owner, |writer, owner| {
            writer.transaction_entry(owner)?;
            writer.transaction_matches(view.transaction)?;
            view.check()?;
            let actor =
                view.transaction.lease.actor.ok_or_else(|| {
                    fail(writer.storage.session_fence(), RetentionCause::Invariant)
                })?;
            let ActorStage::Active(ordinal) = writer.catalog_actor_stage(actor)? else {
                return Err(fail(
                    writer.storage.session_fence(),
                    RetentionCause::Invariant,
                ));
            };
            let job = queue::decode_job(
                &writer.control.jobs()[ordinal],
                writer.storage.profile().spec().namespace,
            )
            .map_err(|_| fail(writer.storage.session_fence(), RetentionCause::Invariant))?;
            if view.sequence != 1 || view.revision.get() != 0 || job.object() != Some(object) {
                return Err(fail(
                    writer.storage.session_fence(),
                    RetentionCause::Invariant,
                ));
            }
            let lease = &view.transaction.lease;
            let mut state = entry(lease.ledger, lease.fence)?;
            lease.check(state.bytes)?;
            row_mut(state.bytes, lease.slot)[3] |= 3;
            view.revision.set(1);
            Ok(())
        });
        // CREATE has already installed its durable obligation. Capacity or a
        // cleanup error here is not a healthy pre-effect retry of initialize.
        result.map_err(|error| fail(self.storage.session_fence(), error.cause))
    }

    pub(super) fn check_catalog_statement_finish(
        &self,
        view: &RowStatementView<'_, '_, 'a, 'c, 's>,
    ) -> Result<()> {
        let actor = view.transaction.lease.actor.expect("writable dispatch");
        if view.revision.get() == 0 {
            return if self.empty_actor_active(actor, CATALOG_KIND)? {
                Ok(())
            } else {
                Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::WrongState,
                ))
            };
        }
        if view.sequence != 1
            || view.revision.get() != 1
            || !matches!(self.catalog_actor_stage(actor)?, ActorStage::Active(_))
        {
            return Err(fail(
                self.storage.session_fence(),
                RetentionCause::Invariant,
            ));
        }
        let state = entry(self.isolation, view.transaction.lease.fence)?;
        if row(state.bytes, view.transaction.lease.slot)[3] != 3 {
            return Err(fail(
                self.storage.session_fence(),
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }

    fn catalog_capsule(&self, transaction: &RowTransaction<'_, 'a, 'c, 's>) -> Result<TxnRef> {
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
        let state = entry(self.isolation, transaction.lease.fence)?;
        transaction.lease.check(state.bytes)?;
        let target = row(state.bytes, transaction.lease.slot);
        let phase = if transaction.phase.get() == Phase::Active {
            1
        } else {
            2
        };
        if target[0] != phase || target[1] > 1 || target[88..].iter().any(|&b| b != 0) {
            return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
        }
        if read64(target, 80) != 0 {
            return Err(unchanged(RetentionPhase::Finalize, RetentionCause::Busy));
        }
        transaction
            .lease
            .actor
            .ok_or_else(|| unchanged(RetentionPhase::Finalize, RetentionCause::WrongState))
    }

    fn catalog_terminal(
        &self,
        observation: &Observation,
        actor: TxnRef,
        committed: bool,
    ) -> Result<u64> {
        let bad = || fail(self.storage.session_fence(), RetentionCause::Invariant);
        if observation.operation
            != if committed {
                Operation::Commit
            } else {
                Operation::AbortEnd
            }
            || observation.actor != Some(actor)
            || self.control.slot(actor).is_some()
        {
            return Err(bad());
        }
        let mut terminals = 0;
        let mut roots = 0;
        let mut q = 0;
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let Some(job) =
                queue::decode_catalog_cell(bytes, self.storage.profile().spec().namespace)
                    .map_err(|_| bad())?
            else {
                continue;
            };
            if job.source() != Some(actor) {
                continue;
            }
            if job.state() == JobState::TerminalReady {
                if Some(ordinal) != observation.job_ordinal
                    || job.status() != observation.status
                    || job.adoption_lsn() != observation.header.lsn
                    || job.outcome() != if committed { 1 } else { 2 }
                {
                    return Err(bad());
                }
                terminals += 1;
                q = job.commit_q();
            } else if job.state()
                == if committed {
                    JobState::RootRetained
                } else {
                    JobState::RootReleasingCompensated
                }
            {
                roots += 1;
                if job.status() != observation.status || job.statement() != 1 || job.mutation() != 1
                {
                    return Err(bad());
                }
            } else {
                return Err(bad());
            }
        }
        if terminals != 1 || roots > 1 || (committed && roots != 1) {
            return Err(bad());
        }
        if committed {
            if q == 0
                || self
                    .visibility
                    .ok_or_else(bad)?
                    .isolation_frontier()
                    .map_err(vis)?
                    != q
            {
                return Err(bad());
            }
        } else if q != 0 {
            return Err(bad());
        }
        Ok(q)
    }

    #[allow(
        clippy::result_large_err,
        reason = "same capsule and return are charged by its original512-byte lease"
    )]
    pub(in crate::storage) fn commit_catalog_transaction<'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut transaction: RowTransaction<'v, 'a, 'c, 's>,
        private: &mut [u8; 425_984],
    ) -> std::result::Result<CatalogCommit, TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>>
    {
        let _unwind = self.storage.session_fence().unwind_guard();
        // A Row capsule is refused by kind before the gate, source or scan.
        if transaction.row_scope() {
            return transaction_error(
                transaction,
                unchanged(RetentionPhase::Finalize, RetentionCause::Unsupported),
            );
        }
        let checked = self.catalog_transaction_check(owner, |writer, owner| {
            writer.transaction_entry(owner)?;
            let actor = writer.catalog_capsule(&transaction)?;
            if transaction.phase.get() != Phase::Active {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::WrongState,
                ));
            }
            if !matches!(writer.catalog_actor_stage(actor)?, ActorStage::Active(_)) {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::Unsupported,
                ));
            }
            let (mode, q, mask, catalog) = {
                let state = entry(writer.isolation, transaction.lease.fence)?;
                let target = row(state.bytes, transaction.lease.slot);
                (
                    target[1],
                    read64(target, 24),
                    read32(target, 4) | read32(target, 8),
                    target[3],
                )
            };
            if catalog != 3 {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::Unsupported,
                ));
            }
            if mode == 0
                && !transaction
                    .lease
                    .visibility
                    .validate_isolation(q, mask, true)
                    .map_err(vis)?
            {
                transaction.set_abort_phase(Phase::MustAbort)?;
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::SerializationConflict,
                ));
            }
            Ok(actor)
        });
        let actor = match checked {
            Ok(actor) => actor,
            Err(error) => return transaction_error(transaction, error),
        };
        // The same exclusive control borrow spans validation through publication.
        let observation = match self.commit_group::<PreparedCatalog>(owner, actor, private) {
            Ok(observation) => observation,
            Err(error) => return transaction_error(transaction, error),
        };
        let completed = self.catalog_transaction_check(owner, |writer, owner| {
            writer.transaction_entry(owner)?;
            let q = writer.catalog_terminal(&observation, actor, true)?;
            transaction.set_abort_phase(Phase::Committed)?;
            transaction.close()?;
            Ok(CatalogCommit { q })
        });
        completed.map_err(|error| {
            TransactionFinishError::Fatal(fail(self.storage.session_fence(), error.cause))
        })
    }

    #[allow(
        clippy::result_large_err,
        reason = "same capsule and return are charged by its original512-byte lease"
    )]
    pub(in crate::storage) fn abort_catalog_transaction<'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut transaction: RowTransaction<'v, 'a, 'c, 's>,
        private: &mut [u8; 425_984],
        flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> std::result::Result<(), TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>> {
        let _unwind = self.storage.session_fence().unwind_guard();
        // A Row capsule is refused by kind before the gate, source or scan.
        if transaction.row_scope() {
            return transaction_error(
                transaction,
                unchanged(RetentionPhase::Finalize, RetentionCause::Unsupported),
            );
        }
        let actor = match self.catalog_transaction_check(owner, |writer, owner| {
            writer.transaction_entry(owner)?;
            writer.catalog_capsule(&transaction)
        }) {
            Ok(actor) => actor,
            Err(error) => return transaction_error(transaction, error),
        };
        loop {
            let selected = self.catalog_transaction_check(owner, |writer, owner| {
                writer.transaction_entry(owner)?;
                writer.catalog_capsule(&transaction)?;
                writer.catalog_actor_stage(actor)
            });
            let selected = match selected {
                Ok(selected) => selected,
                Err(error) => return transaction_error(transaction, error),
            };
            match selected {
                ActorStage::EmptyActive | ActorStage::Active(_) => {
                    if let Err(error) = self.catalog_rollback_start(owner, actor, private, flusher)
                    {
                        return transaction_error(transaction, error);
                    }
                    // This actual effect precedes any fallible reacquisition.
                    if let Err(error) = transaction.set_abort_phase(Phase::MustAbort) {
                        return transaction_error(transaction, error);
                    }
                    #[cfg(test)]
                    crate::transaction::concurrency::catalog_test_support::hit(self.pool.bound_core(), crate::transaction::concurrency::catalog_test_support::Point::CatalogRollbackCompleted);
                }
                ActorStage::Rollback(ordinal) => {
                    if transaction.phase.get() != Phase::MustAbort {
                        return transaction_error(
                            transaction,
                            fail(self.storage.session_fence(), RetentionCause::Invariant),
                        );
                    }
                    if let Err(error) =
                        self.retire_catalog(owner, ordinal, Operation::Clr, private, flusher)
                    {
                        return transaction_error(transaction, error);
                    }
                    #[cfg(test)]
                    crate::transaction::concurrency::catalog_test_support::hit(self.pool.bound_core(), crate::transaction::concurrency::catalog_test_support::Point::CatalogClrCompleted);
                }
                ActorStage::EmptyAborting | ActorStage::Compensated(_) => break,
            }
        }
        if transaction.phase.get() != Phase::MustAbort {
            return transaction_error(
                transaction,
                fail(self.storage.session_fence(), RetentionCause::Invariant),
            );
        }
        let observation = match self.catalog_abort_end(owner, actor, private, flusher) {
            Ok(observation) => observation,
            Err(error) => return transaction_error(transaction, error),
        };
        self.catalog_transaction_check(owner, |writer, owner| {
            writer.transaction_entry(owner)?;
            writer.catalog_terminal(&observation, actor, false)?;
            transaction.set_abort_phase(Phase::Aborted)?;
            transaction.close()
        })
        .map_err(|error| {
            TransactionFinishError::Fatal(fail(self.storage.session_fence(), error.cause))
        })
    }
}

#[allow(
    clippy::result_large_err,
    reason = "same original capsule is returned inline"
)]
pub(super) fn transaction_error<'v, 'a, 'c, 's, T>(
    transaction: RowTransaction<'v, 'a, 'c, 's>,
    error: RetentionError,
) -> std::result::Result<T, TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>> {
    if error.effect != Effect::Unchanged {
        transaction.lease.fence.fence();
        Err(TransactionFinishError::Fatal(error))
    } else if transaction.phase.get() == Phase::MustAbort {
        Err(TransactionFinishError::RollbackRequired { error, transaction })
    } else {
        Err(TransactionFinishError::Before { error, transaction })
    }
}

/// Values retained by the new consuming caller across a lower WAL operation.
/// The capsule/error return stays in its original long512-byte reservation.
pub(in super::super) fn terminal_caller_bytes() -> usize {
    use std::mem::size_of;
    5 * size_of::<&()>()
        + size_of::<TxnRef>()
        + size_of::<ActorStage>()
        + 2 * size_of::<Result<Observation>>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
}

/// The CLR arm has no Observation binding: its if-let consumes the lower
/// Result before the next checked loop. Commit and AbortEnd retain an
/// Observation for terminal validation in distinct branches, so their larger
/// return census must not be stacked onto this mutually exclusive CLR arm.
pub(in super::super) fn rollback_caller_bytes() -> usize {
    use std::mem::size_of;
    5 * size_of::<&()>()
        + size_of::<TxnRef>()
        + size_of::<ActorStage>()
        + size_of::<usize>() // ordinal destructured in the upper CLR arm
        + 2 * size_of::<Result<ActorStage>>()
        + size_of::<Result<Observation>>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
}

pub(in super::super) fn check_source_bytes() -> usize {
    use std::mem::size_of;
    // Existing bounded descriptor/Job/ledger scans plus the new helper and its
    // retained gate/admission, closure captures and scalar/result returns.
    writable::source_bytes()
        + size_of::<&()>()
        + size_of::<Result<ActorStage>>()
        + source::commit_carrier_bytes()
        + size_of::<source::OperationClass>() // transaction_check's class
        // finish_statement's actor and kind dispatch around the check.
        + size_of::<(TxnRef, bool)>()
        + 14 * size_of::<&()>()
        + 2 * size_of::<Result<ActorStage>>()
        + 2 * size_of::<Result<TxnRef>>()
        + size_of::<Observation>()
        + 2 * size_of::<ObjectHandle>()
        + 2 * size_of::<CatalogCommit>()
        + 4 * size_of::<usize>()
}

#[cfg(test)]
impl RowTransaction<'_, '_, '_, '_> {
    pub(in crate::storage) fn test_flip_catalog_mode(&self) {
        let mut state = self.lease.ledger.lock().unwrap();
        row_mut(state.bytes, self.lease.slot)[1] ^= 1;
    }
    pub(in crate::storage) fn test_corrupt_catalog_completion(&self, mark: bool) {
        let mut state = self.lease.ledger.lock().unwrap();
        let target = row_mut(state.bytes, self.lease.slot);
        if mark {
            target[88] = 1;
        } else {
            target[1] = 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consuming_catalog_caller_and_checks_fit_original_limits() {
        fn check<W: WriterNamespace>() {
            let live = resources::source_census::<W>();
            let charges = source::CatalogProgressCharge::new::<W>();
            let handles = super::super::handle_census();
            // Admission uses a fixed maximum selected during split.
            assert!(source::progress_test_bytes(&charges) >= check_source_bytes());
            assert!(handles.0 <= HANDLE_BYTES && handles.1 <= HANDLE_BYTES);
            println!(
                "CATALOG_CAPSULE_SOURCE caller={} clr_caller={} check={} progress={} handles={handles:?} peak={}",
                terminal_caller_bytes(),
                rollback_caller_bytes(),
                check_source_bytes(),
                source::progress_test_bytes(&charges),
                live.peak
            );
            assert!(live.peak <= 65536);
        }
        check::<crate::storage::wal_io::plan4_test_support::Writer>();
        check::<super::super::super::super::native::Writer>();
    }
}
