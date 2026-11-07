//! Sealed bridge from C's live Row cleanup producer (reclamation
//! `#live-row-retirement`) and the held original Row targets. Parsing a frame
//! or supplying a raw reference cannot call these factories: the verified
//! Before needs the held `RetirementTargets`, and the stage/apply receipts
//! are built only by the live Row retirement install.
use super::super::create_pair::catalog_live::lifecycle::row::{
    PreparedRowGroup,
    retire::{RowRetirementApplied, RowRetirementRejected, RowRetirementStaged},
};
use super::authority::ReferenceTarget;
use super::*;
use crate::storage::{
    dirty_frame::RetirementTargets,
    page::{PAGE_BYTES, PageKind, decode_page},
    retention_page::row::{self as job3, JobScope},
    semantic_row::Operation,
    status_home::StatusHome,
    transaction_record::{TransactionState, TransactionStatusCell},
};

fn origin<'c, 's, const T: usize>(prepared: &PreparedRowGroup<'_, 'c, 's, T>) -> Origin<'c, 's> {
    let (core, binding, fence) = prepared.retirement_origin();
    Origin {
        core,
        binding,
        fence,
    }
}

/// The retirement scope of a retiring live Row cleanup operation. DeletePrune
/// and UpdatePrune drop the old value U holds, so both need no snapshot
/// that reads it (`ValuePrune`, dev3-row-update.md#retirement). Cancel,
/// ReleaseRole and every other Row group retire no reference target. The
/// match names every operation, so a new one cannot fall to no scope.
fn scope(operation: Operation) -> Option<RetirementScope> {
    match operation {
        Operation::Prune => Some(RetirementScope::MissingPrune),
        Operation::DeletePrune | Operation::UpdatePrune => Some(RetirementScope::ValuePrune),
        Operation::RootFree => Some(RetirementScope::UndoFree),
        Operation::TerminalFinish => Some(RetirementScope::TerminalFree),
        Operation::Begin
        | Operation::Forward
        | Operation::RollbackStart
        | Operation::Clr
        | Operation::StatementFinish
        | Operation::Commit
        | Operation::AbortEnd
        | Operation::DeleteForward
        | Operation::DeleteClr
        | Operation::UpdateForward
        | Operation::Reserve
        | Operation::Cancel
        | Operation::UpdateClr
        | Operation::AgingStart
        | Operation::ReleaseRole
        // A chain return moves chunk pages to the anchor's retired list;
        // no reader reaches them (dev3-row-overflow.md#no-reader-race).
        | Operation::ChunkWrite
        | Operation::OverflowCancel
        | Operation::ChainRetire => None,
    }
}

/// The held target of group role `role`.
fn held<'t, const T: usize, const N: usize>(
    prepared: &PreparedRowGroup<'_, '_, '_, T>,
    targets: &'t RetirementTargets<'_, '_, '_, '_, '_, '_, '_, '_, N>,
    role: u8,
) -> super::super::Result<(usize, &'t [u8; PAGE_BYTES])> {
    use super::super::{RetentionCause, RetentionPhase, frame, unchanged as refusal};
    let at = (0..N)
        .find(|at| prepared.retirement_role(*at) == Some(role))
        .ok_or_else(|| refusal(RetentionPhase::Prepare, RetentionCause::Invariant))?;
    let bytes = targets
        .bytes(at)
        .map_err(|e| frame(RetentionPhase::Prepare, e))?;
    Ok((at, bytes))
}

impl<'a, 'c, 's> BoundVisibility<'a, 'c, 's> {
    /// The verified semantic Before of a retiring live Row group: every held
    /// original target still carries its prepared stamp under the same
    /// version hold, the held Job names the group's actor, and the target
    /// comes from that Job (and, for Prune, the held Status's committed Q).
    /// Prune names the head U (`MissingPrune`, its Q), DeletePrune and
    /// UpdatePrune the head U (`ValuePrune`, its Q), RootFree U (`UndoFree`), TerminalFinish the
    /// actor (`TerminalFree`).
    pub(in super::super) fn verify_row_retirement<const T: usize, const N: usize>(
        &self,
        prepared: &PreparedRowGroup<'_, 'c, 's, T>,
        targets: &RetirementTargets<'_, '_, '_, '_, 'c, 's, '_, '_, N>,
    ) -> super::super::Result<VerifiedRetirement<'c, 's>> {
        use super::super::{RetentionCause, RetentionError, RetentionPhase, frame};
        use crate::storage::semantic_row::transition as row_group;
        let convert = |error: VisibilityError| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: error.effect,
            cause: RetentionCause::Visibility(error),
        };
        if !self.origin.same(&origin(prepared)) {
            return Err(convert(unchanged(VisibilityCause::BindingMismatch)));
        }
        let version = targets
            .version_identity()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if version.owner() != prepared.retirement_owner()
            || prepared.retirement_targets().len() != N
        {
            return Err(convert(unchanged(VisibilityCause::BindingMismatch)));
        }
        for at in 0..N {
            let actual = targets
                .observe(at)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?
                .frame;
            if Some(actual) != prepared.retirement_stamp(at) {
                return Err(convert(unchanged(VisibilityCause::Busy)));
            }
        }
        let bad = || convert(self.origin.fatal(VisibilityCause::Invariant));
        let observation = prepared.retirement_observation();
        let scope = scope(observation.operation).ok_or_else(bad)?;
        let actor = observation.actor.ok_or_else(bad)?;
        // Every scope reads its actor (and RootFree/Prune their U) back
        // from the held Job: the Terminal Job for TerminalFinish, the root
        // Job otherwise.
        let ordinal = observation.job_ordinal.ok_or_else(bad)?;
        let namespace = crate::api::Namespace {
            database: actor.id().database,
            timeline: actor.id().timeline,
        };
        let (_, bytes) = held(prepared, targets, row_group::JOB as u8)?;
        let page =
            job3::decode_jobs_page(bytes, namespace, (ordinal / 15) as u8).map_err(|_| bad())?;
        let job = job3::decode_job(page.cell(ordinal % 15).ok_or_else(bad)?, namespace)
            .map_err(|_| bad())?;
        if job.scope() != Some(JobScope::Row) || job.source() != Some(actor) {
            return Err(bad());
        }
        let (target, committed_q) = if scope == RetirementScope::TerminalFree {
            (ReferenceTarget::Transaction(actor), None)
        } else {
            let undo = ReferenceTarget::Undo(job.undo().ok_or_else(bad)?);
            if matches!(
                scope,
                RetirementScope::MissingPrune | RetirementScope::ValuePrune
            ) {
                let (at, bytes) = held(prepared, targets, row_group::STATUS as u8)?;
                let page = decode_page(
                    bytes,
                    prepared.retirement_targets()[at].identity,
                    PageKind::TransactionStatus,
                )
                .map_err(|_| bad())?;
                let home = StatusHome::decode(page.cell(0).ok_or_else(bad)?).map_err(|_| bad())?;
                match home.core() {
                    TransactionStatusCell::Occupied(status) if status.reference() == actor => {
                        match status.state() {
                            TransactionState::Committed(q) => (undo, Some(q.get())),
                            _ => return Err(bad()),
                        }
                    }
                    _ => return Err(bad()),
                }
            } else {
                (undo, None)
            }
        };
        Ok(VerifiedRetirement {
            origin: self.origin,
            version,
            scope,
            target,
            actor,
            committed_q,
            group_digest: prepared.retirement_digest(),
        })
    }

    /// The published marker of this group at `state`, its generation and
    /// its scope/target. The marker's digest is the prepared frame's, its
    /// scope the operation's, and a TerminalFree target the group's actor.
    fn row_marker<const T: usize>(
        &self,
        prepared: &PreparedRowGroup<'_, 'c, 's, T>,
        state: u8,
    ) -> VisibilityResult<(u64, RetirementScope, ReferenceTarget)> {
        if !self.origin.same(&origin(prepared)) {
            return Err(self.origin.fatal(VisibilityCause::BindingMismatch));
        }
        let observation = prepared.retirement_observation();
        let (Some(scope), Some(actor)) = (scope(observation.operation), observation.actor) else {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        };
        let bytes = self.entry()?;
        let marker = retirement_row(&bytes[..]);
        if marker[0] != state
            || marker[2] != scope as u8
            || marker[32..64] != prepared.retirement_digest()
        {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        let target = ReferenceTarget::decode(marker[1], &marker[64..112])
            .map_err(|_| self.origin.fatal(VisibilityCause::Invariant))?;
        let named = match (scope, target) {
            (RetirementScope::TerminalFree, ReferenceTarget::Transaction(named)) => named == actor,
            (RetirementScope::TerminalFree, _) | (_, ReferenceTarget::Transaction(_)) => false,
            (_, ReferenceTarget::Undo(_)) => true,
        };
        if !named {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        Ok((u64_at(marker, 8), scope, target))
    }
    pub(in super::super) fn staged_row_retirement(
        &self,
        receipt: &RowRetirementStaged<'_, '_, 'c, 's>,
    ) -> VisibilityResult<StagedRetirement<'c, 's>> {
        if !receipt
            .prepared()
            .confirms_retirement_requirement(receipt.requirement())
        {
            return Err(self.origin.fatal(VisibilityCause::BindingMismatch));
        }
        let (generation, scope, target) = self.row_marker(receipt.prepared(), 2)?;
        Ok(StagedRetirement {
            origin: self.origin,
            generation,
            scope,
            target,
            group_digest: receipt.prepared().retirement_digest(),
            requirement: receipt.requirement(),
        })
    }
    pub(in super::super) fn applied_row_retirement(
        &self,
        receipt: &RowRetirementApplied<'_, '_, 'c, 's>,
    ) -> VisibilityResult<AppliedRetirement<'c, 's>> {
        let staged = receipt.staged();
        if !staged
            .prepared()
            .confirms_retirement_requirement(staged.requirement())
        {
            return Err(self.origin.fatal(VisibilityCause::BindingMismatch));
        }
        let (generation, scope, target) = self.row_marker(staged.prepared(), 3)?;
        Ok(AppliedRetirement {
            origin: self.origin,
            generation,
            scope,
            target,
            group_digest: staged.prepared().retirement_digest(),
            requirement: staged.requirement(),
        })
    }
}

impl RetirementIntent<'_, '_, '_, '_> {
    /// Only the original stage call's exact Unchanged branch issues receipt.
    pub(in crate::storage) fn reject_row_stage(
        &mut self,
        receipt: RowRetirementRejected<'_, '_, '_, '_>,
    ) -> VisibilityResult<()> {
        let prepared = receipt.prepared();
        if !self.visibility.origin.same(&origin(prepared))
            || self.group_digest != prepared.retirement_digest()
            || self.requirement.is_some()
        {
            return Err(self
                .visibility
                .origin
                .fatal(VisibilityCause::BindingMismatch));
        }
        let mut bytes = self.visibility.entry()?;
        let marker = retirement_row(&bytes[..]);
        if marker[0] != 2
            || u64_at(marker, 8) != self.generation
            || marker[32..64] != self.group_digest
        {
            return Err(self.visibility.origin.fatal(VisibilityCause::Invariant));
        }
        retirement_mut(&mut bytes[..])[0] = 1;
        Ok(())
    }
}
