//! Sealed bridge from the actual CREATE producer and held original targets.
//! Parsing a frame or supplying a raw reference cannot call these factories.
use super::super::create_pair::catalog_live::{
    PreparedCatalog,
    retirement::{CatalogRetirementApplied, CatalogRetirementRejected, CatalogRetirementStaged},
};
use super::authority::ReferenceTarget;
use super::*;
use crate::storage::{
    dirty_frame::{ReadTargets, RetirementTargets},
    page::{PageKind, decode_page},
    retention_page::catalog,
    semantic_catalog::Operation,
    status_home::StatusHome,
    transaction_record::{
        RollbackState, TransactionState, TransactionStatus, TransactionStatusCell,
    },
};

fn origin<'c, 's>(prepared: &PreparedCatalog<'_, 'c, 's>) -> Origin<'c, 's> {
    Origin {
        core: prepared.core(),
        binding: prepared.binding(),
        fence: prepared.fence(),
    }
}
fn status(prepared: &PreparedCatalog<'_, '_, '_>) -> VisibilityResult<TransactionStatus> {
    let at = prepared
        .targets()
        .iter()
        .position(|request| request.kind == PageKind::TransactionStatus)
        .ok_or_else(|| origin(prepared).fatal(VisibilityCause::Invariant))?;
    let page = decode_page(
        prepared
            .before(at)
            .ok_or_else(|| origin(prepared).fatal(VisibilityCause::Invariant))?,
        prepared.targets()[at].identity,
        PageKind::TransactionStatus,
    )
    .map_err(|_| origin(prepared).fatal(VisibilityCause::Invariant))?;
    let home = StatusHome::decode(
        page.cell(0)
            .ok_or_else(|| origin(prepared).fatal(VisibilityCause::Invariant))?,
    )
    .map_err(|_| origin(prepared).fatal(VisibilityCause::Invariant))?;
    match home.core() {
        TransactionStatusCell::Occupied(status)
            if Some(status.reference()) == prepared.observation().actor =>
        {
            Ok(status)
        }
        _ => Err(origin(prepared).fatal(VisibilityCause::Invariant)),
    }
}
fn fields(
    prepared: &PreparedCatalog<'_, '_, '_>,
) -> VisibilityResult<(RetirementScope, ReferenceTarget, TxnRef, Option<u64>)> {
    let source = origin(prepared);
    source.health()?;
    let actor = prepared
        .observation()
        .actor
        .ok_or_else(|| source.fatal(VisibilityCause::Invariant))?;
    if prepared.operation() == Operation::TerminalFinish {
        return Ok((
            RetirementScope::TerminalFree,
            ReferenceTarget::Transaction(actor),
            actor,
            None,
        ));
    }
    let ordinal = prepared
        .observation()
        .job_ordinal
        .ok_or_else(|| source.fatal(VisibilityCause::Invariant))?;
    let namespace = crate::api::Namespace {
        database: actor.id().database,
        timeline: actor.id().timeline,
    };
    let page = catalog::decode_jobs_page(
        prepared
            .before_role(2)
            .ok_or_else(|| source.fatal(VisibilityCause::Invariant))?,
        namespace,
        (ordinal / 15) as u8,
    )
    .map_err(|_| source.fatal(VisibilityCause::Invariant))?;
    let job = catalog::decode_job(
        page.cell(ordinal % 15)
            .ok_or_else(|| source.fatal(VisibilityCause::Invariant))?,
        namespace,
    )
    .map_err(|_| source.fatal(VisibilityCause::Invariant))?;
    if job.source() != Some(actor) {
        return Err(source.fatal(VisibilityCause::Invariant));
    }
    let target = ReferenceTarget::Undo(
        job.undo()
            .ok_or_else(|| source.fatal(VisibilityCause::Invariant))?,
    );
    let (scope, q) = match prepared.operation() {
        Operation::Clr => (RetirementScope::MissingClr, None),
        Operation::RootFree => (RetirementScope::UndoFree, None),
        Operation::Prune => match status(prepared)?.state() {
            TransactionState::Committed(q) => (RetirementScope::MissingPrune, Some(q.get())),
            _ => return Err(source.fatal(VisibilityCause::Invariant)),
        },
        _ => return Err(source.fatal(VisibilityCause::Invariant)),
    };
    Ok((scope, target, actor, q))
}

impl<'a, 'c, 's> BoundVisibility<'a, 'c, 's> {
    pub(in super::super) fn verify_catalog_retirement<const N: usize>(
        &self,
        prepared: &PreparedCatalog<'_, 'c, 's>,
        targets: &RetirementTargets<'_, '_, '_, '_, 'c, 's, '_, '_, N>,
    ) -> super::super::Result<VerifiedRetirement<'c, 's>> {
        use super::super::{RetentionCause, RetentionError, RetentionPhase, frame};
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
        if version.owner() != prepared.owner() || prepared.targets().len() != N {
            return Err(convert(unchanged(VisibilityCause::BindingMismatch)));
        }
        for at in 0..N {
            let actual = targets
                .observe(at)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?
                .frame;
            let bytes = targets
                .bytes(at)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            if Some(actual) != prepared.stamp(at)
                || prepared.before(at).is_some_and(|before| bytes != before)
            {
                return Err(convert(unchanged(VisibilityCause::Busy)));
            }
        }
        let (scope, target, actor, committed_q) = fields(prepared).map_err(convert)?;
        Ok(VerifiedRetirement {
            origin: self.origin,
            version,
            scope,
            target,
            actor,
            committed_q,
            group_digest: prepared.frame_digest(),
        })
    }

    pub(in super::super) fn catalog_rollback_source<'g, const N: usize>(
        &self,
        prepared: &PreparedCatalog<'_, 'c, 's>,
        read: &'g ReadTargets<'_, '_, 'c, 's, '_, '_, N>,
    ) -> super::super::Result<RollbackSource<'g, 'c, 's>> {
        use super::super::{RetentionCause, RetentionError, RetentionPhase, frame};
        let convert = |error: VisibilityError| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: error.effect,
            cause: RetentionCause::Visibility(error),
        };
        if !self.origin.same(&origin(prepared)) || prepared.operation() != Operation::Clr {
            return Err(convert(unchanged(VisibilityCause::BindingMismatch)));
        }
        let at = prepared
            .targets()
            .iter()
            .position(|request| request.kind == PageKind::TransactionStatus)
            .ok_or_else(|| convert(self.origin.fatal(VisibilityCause::Invariant)))?;
        if Some(
            read.observe(at)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?
                .frame,
        ) != prepared.stamp(at)
            || Some(
                read.bytes(at)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
            ) != prepared.before(at)
        {
            return Err(convert(unchanged(VisibilityCause::Busy)));
        }
        let (_, target, actor, _) = fields(prepared).map_err(convert)?;
        let ReferenceTarget::Undo(current) = target else {
            return Err(convert(self.origin.fatal(VisibilityCause::Invariant)));
        };
        if !matches!(status(prepared).map_err(convert)?.state(), TransactionState::Aborting(RollbackState::Transaction { next: Some(next) }) if next == current)
        {
            return Err(convert(self.origin.fatal(VisibilityCause::Invariant)));
        }
        Ok(RollbackSource {
            origin: self.origin,
            actor,
            current,
            source: std::marker::PhantomData,
        })
    }

    fn catalog_marker(
        &self,
        prepared: &PreparedCatalog<'_, 'c, 's>,
        state: u8,
    ) -> VisibilityResult<(u64, RetirementScope, ReferenceTarget)> {
        if !self.origin.same(&origin(prepared)) {
            return Err(self.origin.fatal(VisibilityCause::BindingMismatch));
        }
        let (scope, target, _, _) = fields(prepared)?;
        let bytes = self.entry()?;
        let marker = retirement_row(&bytes[..]);
        if marker[0] != state
            || marker[2] != scope as u8
            || marker[32..64] != prepared.frame_digest()
            || ReferenceTarget::decode(marker[1], &marker[64..112]).ok() != Some(target)
        {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        Ok((u64_at(marker, 8), scope, target))
    }
    pub(in super::super) fn staged_catalog_retirement(
        &self,
        receipt: &CatalogRetirementStaged<'_, '_, 'c, 's>,
    ) -> VisibilityResult<StagedRetirement<'c, 's>> {
        if !receipt
            .prepared()
            .confirms_requirement(receipt.requirement())
        {
            return Err(self.origin.fatal(VisibilityCause::BindingMismatch));
        }
        let (generation, scope, target) = self.catalog_marker(receipt.prepared(), 2)?;
        Ok(StagedRetirement {
            origin: self.origin,
            generation,
            scope,
            target,
            group_digest: receipt.prepared().frame_digest(),
            requirement: receipt.requirement(),
        })
    }
    pub(in super::super) fn applied_catalog_retirement(
        &self,
        receipt: &CatalogRetirementApplied<'_, '_, 'c, 's>,
    ) -> VisibilityResult<AppliedRetirement<'c, 's>> {
        let staged = receipt.staged();
        if !staged.prepared().confirms_requirement(staged.requirement()) {
            return Err(self.origin.fatal(VisibilityCause::BindingMismatch));
        }
        let (generation, scope, target) = self.catalog_marker(staged.prepared(), 3)?;
        Ok(AppliedRetirement {
            origin: self.origin,
            generation,
            scope,
            target,
            group_digest: staged.prepared().frame_digest(),
            requirement: staged.requirement(),
        })
    }
}

impl RetirementIntent<'_, '_, '_, '_> {
    /// Only the original stage call's exact Unchanged branch issues receipt.
    pub(in crate::storage) fn reject_catalog_stage(
        &mut self,
        receipt: CatalogRetirementRejected<'_, '_, '_, '_>,
    ) -> VisibilityResult<()> {
        let prepared = receipt.prepared();
        if !self.visibility.origin.same(&origin(prepared))
            || self.group_digest != prepared.frame_digest()
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
