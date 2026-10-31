//! Live rollback holds the original mutation port, gate, source admission and
//! disjoint flusher. CLR candidates borrow the same rollback and private loan.
use super::super::create_pair::catalog_live::row_preflight::rollback::{
    RollbackCandidate, RollbackSelection,
};
use super::super::source::{OperationClass, OperationSourceAdmission};
use super::super::visibility::{ReferenceLease, UndoPin};
use super::*;
use crate::storage::{
    semantic_row::{Operation, status::StatementBoundary},
    transaction_record::{RollbackState, TransactionState},
};
use crate::transaction::concurrency::TerminalReadGate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RollbackScope {
    Statement,
    Transaction,
}
struct RollbackRefs<'b, 'a, 'c, 's> {
    current: Option<UndoPin<'b, 'a, 'c, 's>>,
    lease: ReferenceLease<'b, 'a, 'c, 's>,
}
pub(crate) struct RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
where
    W: WriterNamespace,
{
    mutation: &'r mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'r mut Owner<'c, 's>,
    transaction: &'r RowTransaction<'v, 'a, 'c, 's>,
    statement: Option<&'r RowStatementView<'t, 'v, 'a, 'c, 's>>,
    private: Option<&'i mut [u8; 425_984]>,
    flusher: &'r mut MaintenancePort<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    flush_owner: &'r mut Owner<'c, 's>,
    actor: TxnRef,
    boundary: Option<StatementBoundary>,
    scope: RollbackScope,
    refs: Option<RollbackRefs<'b, 'a, 'c, 's>>,
    source: Option<OperationSourceAdmission<'b, 'a, 'c, 's>>,
    gate: Option<TerminalReadGate<'c, 's>>,
    active: bool,
}
#[allow(
    clippy::large_enum_variant,
    reason = "the fixed caller census accounts for the inline task, without heap allocation"
)]
pub(crate) enum RollbackStep<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
where
    W: WriterNamespace,
{
    Candidate(RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>),
    FlushRequired { target: Plan4Target },
    Complete,
}
pub(crate) struct RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
where
    W: WriterNamespace,
{
    rollback: &'x mut RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    candidate: RollbackCandidate,
}
pub(crate) struct PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
where
    W: WriterNamespace,
{
    task: RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    checked: RowPreparedCut,
}
pub(crate) struct RowRollbackDone<'i> {
    private: &'i mut [u8; 425_984],
    scope: RollbackScope,
}
impl<'i> RowRollbackDone<'i> {
    pub(in crate::storage) fn into_parts(self) -> (RollbackScope, &'i mut [u8; 425_984]) {
        (self.scope, self.private)
    }
}
impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[allow(
        clippy::too_many_arguments,
        reason = "the original mutation, capsule, private loan and disjoint flusher authority remain explicit"
    )]
    pub(in crate::storage) fn start_row_rollback<'r, 't, 'v, 'i>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        transaction: &'r RowTransaction<'v, 'a, 'c, 's>,
        statement: Option<&'r RowStatementView<'t, 'v, 'a, 'c, 's>>,
        scope: RollbackScope,
        private: &'i mut [u8; 425_984],
        flusher: &'r mut MaintenancePort<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
        flush_owner: &'r mut Owner<'c, 's>,
    ) -> Result<RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.transaction_entry(owner)?;
        self.transaction_origin_matches(transaction)?;
        let (actor, boundary) = transaction.rollback_entry(statement, scope)?;
        if !std::ptr::eq(flusher.pool, self.pool) || !std::ptr::eq(flusher.storage, self.storage) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        flusher
            .control
            .check_catalog_pair_io::<W>(&self.storage.binding, self.storage.session_fence())?;
        let gate = owner
            .close_reads_for_terminal(self.pool.bound_core())
            .map_err(|error| RetentionError {
                phase: RetentionPhase::Prepare,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })?;
        let source = match OperationSourceAdmission::progress(
            self.isolation,
            &self.commit_charges,
            &self.progress_charge,
            &gate,
            OperationClass::RowWrite,
        ) {
            Ok(source) => source,
            Err(error) => {
                return merge(
                    Err(RetentionError::from(error)),
                    gate.reopen_checked().map_err(|e| RetentionError {
                        phase: RetentionPhase::Finalize,
                        cause: RetentionCause::Concurrency(e),
                        effect: e.effect,
                    }),
                );
            }
        };
        let mut rollback = RowRollback {
            mutation: self,
            owner,
            transaction,
            statement,
            private: Some(private),
            flusher,
            flush_owner,
            actor,
            boundary,
            scope,
            refs: None,
            source: Some(source),
            gate: Some(gate),
            active: false,
        };
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            rollback.mutation.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::RowPreflightResident,
        );
        rollback.warm()?;
        // All pre-effect checks precede the actual RollbackStart. The original
        // empty/full body is deliberately not used to authenticate a live mark.
        rollback
            .mutation
            .row_statement_head(actor, RetentionPhase::Prepare)?;
        let visibility = rollback.mutation.visibility.ok_or_else(unsupported)?;
        let lease = visibility
            .reserve_references(rollback.owner)
            .map_err(vis_error)?;
        rollback.refs = Some(RollbackRefs {
            current: None,
            lease,
        });
        let observation = rollback.mutation.row_rollback_control(
            rollback.owner,
            actor,
            boundary,
            Operation::RollbackStart,
            rollback.private.as_deref_mut().expect("loan"),
        )?;
        rollback.active = true;
        if observation.operation != Operation::RollbackStart || observation.actor != Some(actor) {
            return Err(rollback.fatal(RetentionCause::Invariant));
        }
        rollback
            .refresh_pin()
            .map_err(|e| rollback.fatal(e.cause))?;
        Ok(rollback)
    }
}
fn vis_error(error: super::super::visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Prepare,
        cause: RetentionCause::Visibility(error),
        effect: error.effect,
    }
}
impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn fatal(&self, cause: RetentionCause) -> RetentionError {
        super::super::fatal(self.mutation.storage, RetentionPhase::Install, cause)
    }
    fn check_admitted(&mut self) -> Result<()> {
        if self.source.is_none() || self.gate.is_none() || (self.active && self.refs.is_none()) {
            return Err(self.fatal(RetentionCause::Invariant));
        }
        self.flusher.control.check_catalog_pair_io::<W>(
            &self.mutation.storage.binding,
            self.mutation.storage.session_fence(),
        )
    }
    fn release_refs(&mut self) -> Result<()> {
        if let Some(mut refs) = self.refs.take() {
            if let Some(pin) = refs.current.take() {
                pin.release_checked().map_err(vis_error)?;
            }
            refs.lease.release_checked().map_err(vis_error)?;
        }
        Ok(())
    }
    fn refresh_pin(&mut self) -> Result<()> {
        let target = self.mutation.row_rollback_status_target(self.actor)?;
        let refs = self.refs.as_mut().expect("reference lease");
        // H is captured from the post-CLR Status latch before U is returned.
        let next =
            refs.lease
                .capture_row_head(self.mutation.pool, self.owner, target, self.actor)?;
        if let Some(current) = refs.current.take() {
            current.release_checked().map_err(vis_error)?;
        }
        refs.current = next;
        Ok(())
    }
    fn release_source(&mut self) -> Result<()> {
        let source = self
            .source
            .take()
            .map_or(Ok(()), |s| s.finish_checked().map_err(RetentionError::from));
        let gate = self.gate.take().map_or(Ok(()), |g| {
            g.reopen_checked().map_err(|e| RetentionError {
                phase: RetentionPhase::Finalize,
                cause: RetentionCause::Concurrency(e),
                effect: e.effect,
            })
        });
        merge(source, gate)
    }
    fn admit(&mut self) -> Result<()> {
        let gate = self
            .owner
            .close_reads_for_terminal(self.mutation.pool.bound_core())
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                cause: RetentionCause::Concurrency(e),
                effect: e.effect,
            })?;
        let source = OperationSourceAdmission::progress(
            self.mutation.isolation,
            &self.mutation.commit_charges,
            &self.mutation.progress_charge,
            &gate,
            OperationClass::RowWrite,
        )
        .map_err(RetentionError::from)?;
        self.source = Some(source);
        self.gate = Some(gate);
        Ok(())
    }
    /// Disjoint continuation for the original flusher. No task can be borrowed
    /// here. Release pins/admission/gate before I/O, then re-admit and reselect.
    pub(in crate::storage) fn flush(&mut self, target: &Plan4Target) -> Result<()> {
        self.release_refs().map_err(|e| self.fatal(e.cause))?;
        self.release_source().map_err(|e| self.fatal(e.cause))?;
        let result = (|| {
            #[cfg(test)]
            if TEST_FLUSH_REFUSAL.with(|flag| flag.replace(false)) {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
            }
            if self.flusher.phase() == RetentionFlushPhase::Ready {
                self.flusher.try_flush(self.flush_owner, target)?;
            }
            while self.flusher.phase() != RetentionFlushPhase::Ready {
                self.flusher.continue_flush(self.flush_owner)?;
                std::thread::yield_now();
            }
            Ok(())
        })();
        self.admit().map_err(|e| self.fatal(e.cause))?;
        if self.active {
            let visibility = self.mutation.visibility.ok_or_else(unsupported)?;
            self.refs = Some(RollbackRefs {
                current: None,
                lease: visibility
                    .reserve_references(self.owner)
                    .map_err(vis_error)?,
            });
        }
        result
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_flush_refusal(&self) {
        TEST_FLUSH_REFUSAL.with(|flag| assert!(!flag.replace(true)));
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_root(&self) -> Plan4Target {
        self.mutation.row_rollback_test_root()
    }
    fn warm(&mut self) -> Result<()> {
        loop {
            match self.mutation.row_rollback_warm(
                self.owner,
                self.actor,
                self.private.as_deref_mut().expect("loan"),
            )? {
                Some(target) => self.flush(&target)?,
                None => return Ok(()),
            }
        }
    }
    pub(in crate::storage) fn next<'x>(
        &'x mut self,
    ) -> Result<RollbackStep<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.check_admitted()?;
        self.transaction
            .rollback_entry(self.statement, self.scope)?;
        let mark = self.boundary.and_then(|s| s.mark);
        let tx = self.mutation.row_rollback_state(self.actor)?;
        let expected = match self.scope {
            RollbackScope::Statement => RollbackState::Statement {
                target: mark,
                next: tx.undo_head(),
            },
            RollbackScope::Transaction => RollbackState::Transaction {
                next: tx.undo_head(),
            },
        };
        if tx.state() != TransactionState::Aborting(expected) {
            return Err(self.fatal(RetentionCause::Invariant));
        }
        match self.mutation.row_rollback_select(
            self.owner,
            self.actor,
            mark,
            self.private.as_deref_mut().expect("loan"),
        )? {
            RollbackSelection::Flush(target) => Ok(RollbackStep::FlushRequired { target }),
            RollbackSelection::Complete => Ok(RollbackStep::Complete),
            RollbackSelection::Candidate(candidate) => {
                if self
                    .refs
                    .as_ref()
                    .and_then(|r| r.current.as_ref())
                    .is_none()
                {
                    self.refresh_pin()?;
                }
                if self
                    .refs
                    .as_ref()
                    .and_then(|r| r.current.as_ref())
                    .map(|p| p.reference())
                    != Some(candidate.issued.undo)
                {
                    return Err(self.fatal(RetentionCause::Invariant));
                }
                Ok(RollbackStep::Candidate(RowRollbackTask {
                    rollback: self,
                    candidate,
                }))
            }
        }
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original rollback"
    )]
    pub(in crate::storage) fn finish(
        mut self,
    ) -> std::result::Result<RowRollbackDone<'i>, RowOwnedError<Self>> {
        let result = (|| {
            self.check_admitted()?;
            self.transaction
                .rollback_entry(self.statement, self.scope)?;
            let tx = self.mutation.row_rollback_state(self.actor)?;
            let mark = self.boundary.and_then(|s| s.mark);
            let expected = match self.scope {
                RollbackScope::Statement => RollbackState::Statement {
                    target: mark,
                    next: mark,
                },
                RollbackScope::Transaction => RollbackState::Transaction { next: None },
            };
            if tx.undo_head() != mark || tx.state() != TransactionState::Aborting(expected) {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::WrongState,
                ));
            }
            self.warm()?;
            let operation = match self.scope {
                RollbackScope::Statement => Operation::StatementFinish,
                RollbackScope::Transaction => Operation::AbortEnd,
            };
            let observation = self.mutation.row_rollback_control(
                self.owner,
                self.actor,
                self.boundary,
                operation,
                self.private.as_deref_mut().expect("loan"),
            )?;
            if observation.operation != operation || observation.actor != Some(self.actor) {
                return Err(self.fatal(RetentionCause::Invariant));
            }
            if self.scope == RollbackScope::Statement {
                self.mutation
                    .row_statement_head(self.actor, RetentionPhase::Finalize)
                    .map_err(|e| self.fatal(e.cause))?;
            } else {
                self.mutation.row_aborted_terminal(self.actor)?;
            }
            self.transaction
                .rollback_completed(self.statement, self.scope)
                .map_err(|e| self.fatal(e.cause))?;
            self.active = false;
            self.release_refs().map_err(|e| self.fatal(e.cause))?;
            self.release_source().map_err(|e| self.fatal(e.cause))?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(RowRollbackDone {
                scope: self.scope,
                private: self.private.take().expect("loan"),
            }),
            Err(error) if error.effect == Effect::Unchanged => {
                Err(RowOwnedError::Before { error, owned: self })
            }
            Err(error) => Err(RowOwnedError::Fatal(error)),
        }
    }
}
impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Drop
    for RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn drop(&mut self) {
        if self.active {
            self.mutation.storage.session_fence().fence();
        }
    }
}
impl<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self) -> Result<RowProducerLane<'l>> {
        let r = &mut self.rollback;
        let c = &self.candidate;
        let private = r.private.as_deref_mut().expect("loan");
        let context = r.mutation.row_producer_context(
            r.owner,
            r.statement,
            (c.table, &c.draft, c.issued),
            private,
        )?;
        let control = r.mutation.row_control_observation(r.actor)?;
        let (frame, rest) = private.split_at_mut(RowLoanLayout::FRAME.end);
        let (before, rest) = rest.split_at_mut(range_len(RowLoanLayout::BEFORE));
        let (after, rest) = rest.split_at_mut(range_len(RowLoanLayout::AFTER));
        let (candidate, rest) = rest.split_at_mut(range_len(RowLoanLayout::CANDIDATE));
        let (schema, rest) = rest.split_at_mut(range_len(RowLoanLayout::SCHEMA));
        let scratch = &mut rest[RowLoanLayout::A_SCRATCH.start - RowLoanLayout::XOI.start..];
        scratch.fill(0);
        Ok(RowProducerLane {
            context,
            control,
            before: before
                .as_chunks::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("nine before"),
            schema: (&*schema).try_into().expect("schema"),
            frame: frame.try_into().expect("frame"),
            after: after
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("five after"),
            candidate: candidate.try_into().expect("candidate"),
            scratch: scratch.try_into().expect("A scratch"),
        })
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original task"
    )]
    pub(in crate::storage) fn prepare(
        mut self,
        proposal: RowProposal,
    ) -> std::result::Result<
        PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RowOwnedError<Self>,
    > {
        let r = &mut self.rollback;
        let c = &self.candidate;
        let result = r.mutation.row_prepare_clr(
            r.owner,
            c.table,
            &c.draft,
            c.issued,
            &proposal,
            r.private.as_deref_mut().expect("loan"),
        );
        match result {
            Ok(checked) => Ok(PreparedRollbackTask {
                task: self,
                checked,
            }),
            Err(error) if error.effect == Effect::Unchanged => {
                Err(RowOwnedError::Before { error, owned: self })
            }
            Err(error) => Err(RowOwnedError::Fatal(error)),
        }
    }
}
impl<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original prepared task"
    )]
    pub(in crate::storage) fn apply(
        mut self,
    ) -> std::result::Result<RowMutationObservation, RowOwnedError<Self>> {
        let r = &mut self.task.rollback;
        let c = &self.task.candidate;
        let result = r.mutation.row_apply_clr(
            r.owner,
            c.table,
            &c.draft,
            c.issued,
            &self.checked,
            r.private.as_deref_mut().expect("loan"),
        );
        match result {
            Ok(()) => {
                let tx = r
                    .mutation
                    .row_rollback_state(r.actor)
                    .map_err(|e| RowOwnedError::Fatal(r.fatal(e.cause)))?;
                if tx.undo_head() != c.next.map(|l| l.reference) {
                    return Err(RowOwnedError::Fatal(r.fatal(RetentionCause::Invariant)));
                }
                r.refresh_pin()
                    .map_err(|e| RowOwnedError::Fatal(r.fatal(e.cause)))?;
                Ok(RowMutationObservation {
                    row: c.issued.row,
                    undo: c.issued.undo,
                    operation: RowOperation::Clr,
                })
            }
            Err(error) if error.effect == Effect::Unchanged => {
                Err(RowOwnedError::Before { error, owned: self })
            }
            Err(error) => Err(RowOwnedError::Fatal(error)),
        }
    }
    pub(in crate::storage) fn reopen(
        self,
    ) -> RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        self.task
    }
}

#[cfg(test)]
#[path = "rollback_census_tests.rs"]
mod census;

#[cfg(test)]
thread_local! {static TEST_FLUSH_REFUSAL:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
