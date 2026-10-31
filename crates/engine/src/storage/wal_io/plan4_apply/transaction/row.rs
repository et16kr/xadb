//! Consuming Row Commit: the original mutable ledger remains borrowed from
//! isolation validation through durable installation, publication and cleanup.
use super::super::create_pair::catalog_live::lifecycle::row::{
    PreparedRowGroup, RowCommitSummary as Observation,
};
use super::*;
use crate::storage::retention_page::row::{self as job3, JobScope, JobState};

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[allow(
        clippy::result_large_err,
        reason = "same capsule returns in its original 512-byte lease"
    )]
    pub(in crate::storage) fn commit_row<'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut transaction: RowTransaction<'v, 'a, 'c, 's>,
        private: &mut [u8; 425_984],
    ) -> std::result::Result<
        super::super::row::RowCommitObservation,
        TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>,
    > {
        let _unwind = self.storage.session_fence().unwind_guard();
        // Scope is checked before any read gate, source admission or scan.
        if !transaction.row_scope() {
            return catalog::transaction_error(
                transaction,
                unchanged(RetentionPhase::Finalize, RetentionCause::Unsupported),
            );
        }
        let checked = self.transaction_check(
            owner,
            source::OperationClass::RowProgress,
            |writer, owner| {
                writer.transaction_entry(owner)?;
                writer.transaction_matches(&transaction)?;
                let (actor, mode, q, mask, catalog) = {
                    let state = entry(writer.isolation, transaction.lease.fence)?;
                    transaction.lease.check(state.bytes)?;
                    let target = row(state.bytes, transaction.lease.slot);
                    if read64(target, 80) != 0 {
                        return Err(unchanged(RetentionPhase::Finalize, RetentionCause::Busy));
                    }
                    if target[MARK].iter().any(|&b| b != 0) || target[1] > 1 || target[3] & !1 != 0
                    {
                        return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
                    }
                    (
                        transaction.lease.actor.ok_or_else(|| {
                            fail(transaction.lease.fence, RetentionCause::Invariant)
                        })?,
                        target[1],
                        read64(target, 24),
                        read32(target, 4) | read32(target, 8),
                        target[3] != 0,
                    )
                };
                writer.row_statement_head(actor, RetentionPhase::Finalize)?;
                if mode == 0
                    && !transaction
                        .lease
                        .visibility
                        .validate_isolation(q, mask, catalog)
                        .map_err(vis)?
                {
                    transaction.set_abort_phase(Phase::MustAbort)?;
                    return Err(unchanged(
                        RetentionPhase::Finalize,
                        RetentionCause::SerializationConflict,
                    ));
                }
                Ok(actor)
            },
        );
        let actor = match checked {
            Ok(actor) => actor,
            Err(error) => return catalog::transaction_error(transaction, error),
        };
        let observation = match self.commit_group::<PreparedRowGroup>(owner, actor, private) {
            Ok(observation) => observation,
            Err(error) => return catalog::transaction_error(transaction, error),
        };
        let completed = self.transaction_check(
            owner,
            source::OperationClass::RowProgress,
            |writer, owner| {
                writer.transaction_entry(owner)?;
                let q = writer.row_committed_terminal(&observation, actor)?;
                transaction.set_abort_phase(Phase::Committed)?;
                transaction.close()?;
                Ok(super::super::row::RowCommitObservation { actor, q })
            },
        );
        completed.map_err(|error| {
            TransactionFinishError::Fatal(fail(self.storage.session_fence(), error.cause))
        })
    }

    /// Scalar evidence of the exact installed Commit. ATT absence alone never
    /// closes a capsule, and no physical cleanup is hidden in this observation.
    fn row_committed_terminal(&self, observation: &Observation<'_>, actor: TxnRef) -> Result<u64> {
        let bad = || fail(self.storage.session_fence(), RetentionCause::Invariant);
        let expected_jobs = job3::decode_jobs_page(
            observation.jobs,
            self.storage.profile().spec().namespace,
            (observation.terminal / 15) as u8,
        )
        .map_err(|_| bad())?;
        let expected_cell = expected_jobs
            .cell(observation.terminal % 15)
            .ok_or_else(bad)?;
        let expected = job3::decode_job(expected_cell, self.storage.profile().spec().namespace)
            .map_err(|_| bad())?;
        if expected.source() != Some(actor)
            || expected.scope() != Some(JobScope::Row)
            || expected.state() != JobState::TerminalReady
            || expected.status().is_none()
            || self.control.slot(actor).is_some()
        {
            return Err(bad());
        }
        let prefix = self.storage.wal().prefix().map_err(|_| bad())?;
        self.control
            .check_ledger(self.storage.profile(), prefix)
            .map_err(|_| bad())?;
        let mut terminals = 0;
        let mut q = 0;
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let job = job3::decode_job(bytes, self.storage.profile().spec().namespace)
                .map_err(|_| bad())?;
            if job.source() != Some(actor) {
                continue;
            }
            if job.scope() != Some(JobScope::Row) || job.status() != expected.status() {
                return Err(bad());
            }
            match job.state() {
                JobState::TerminalReady => {
                    if ordinal != observation.terminal
                        || bytes.as_slice() != expected_cell
                        || job.outcome() != 1
                        || job.adoption_lsn() != observation.lsn
                    {
                        return Err(bad());
                    }
                    terminals += 1;
                    q = job.commit_q();
                }
                JobState::RootRetained | JobState::RootReleasingCompensated => {}
                _ => return Err(bad()),
            }
        }
        if terminals != 1
            || q == 0
            || self
                .visibility
                .ok_or_else(bad)?
                .isolation_frontier()
                .map_err(vis)?
                != q
        {
            return Err(bad());
        }
        Ok(q)
    }
}

/// The capsule remains in its original 512-byte long lease. These are the
/// caller's short values across the common Commit, including both precheck
/// returns (frame-static), forwarding borrows and the Row observation return.
#[cfg(test)]
pub(in super::super) fn terminal_caller_bytes() -> usize {
    use std::mem::size_of;
    8 * size_of::<&()>()
        + size_of::<TxnRef>()
        + 2 * size_of::<Result<TxnRef>>()
        + 2 * size_of::<Result<Observation<'_>>>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
}

/// Precheck and completion are sequential RowProgress closures. The existing
/// writable census includes entry, lease, descriptor/ledger and Job scans.
/// Count the Row closure captures, state tuple and cleanup returns separately.
#[cfg(test)]
pub(in super::super) fn check_source_bytes() -> usize {
    use std::mem::size_of;
    writable::source_bytes()
        + source::commit_carrier_bytes()
        + terminal_caller_bytes()
        + size_of::<source::OperationClass>()
        + 16 * size_of::<&()>()
        + size_of::<(TxnRef, u8, u64, u32, bool)>()
        + size_of::<Observation<'_>>()
        + 2 * size_of::<Result<super::super::row::RowCommitObservation>>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<job3::JobView<'_>>()
        + 2 * size_of::<Result<job3::JobView<'_>>>()
        + 4 * size_of::<usize>()
        + visibility::row_commit_source_bytes()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<Result<crate::storage::page::PageView<'_>>>()
        + 4 * size_of::<&[u8]>()
}

#[cfg(test)]
impl RowTransaction<'_, '_, '_, '_> {
    pub(in crate::storage) fn test_commit_reserved_once(&self) -> writable::TestOnce {
        writable::test_root_reserved_once()
    }
}
