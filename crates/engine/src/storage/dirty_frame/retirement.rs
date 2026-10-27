//! The original target set keeps one continuous version hold from Q/H capture
//! through the exact semantic retirement. No horizon or inner target escapes.
use super::*;
use crate::storage::wal_io::{
    AppliedRetirement, BoundVisibility, HorizonObservation, RetirementIntent, RetirementReject,
    StagedRetirement, UndoPin, VerifiedRetirement, VisibilityCause, VisibilityError,
    VisibilityResult,
};
use crate::transaction::concurrency::VersionIdentity;

fn visibility_error(error: VisibilityError) -> DirtyError {
    let cause = match error.cause {
        VisibilityCause::Fenced => Cause::Fenced,
        VisibilityCause::Poisoned => Cause::Poisoned,
        VisibilityCause::BindingMismatch => Cause::BindingMismatch,
        VisibilityCause::Capacity => Cause::Capacity,
        VisibilityCause::Exhausted => Cause::CounterExhausted,
        VisibilityCause::Busy => Cause::Busy,
        VisibilityCause::Invariant => Cause::Invariant,
        VisibilityCause::Concurrency(cause) => Cause::Concurrency(cause),
    };
    DirtyError {
        cause,
        effect: error.effect,
    }
}

pub(in crate::storage) struct RetirementTargets<'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    visibility: &'v BoundVisibility<'m, 'c, 's>,
    horizon: HorizonObservation<'c, 's>,
    targets: WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N>,
    released: bool,
}

impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> PinnedTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    pub(in crate::storage) fn try_retirement_write<'v, 'm>(
        mut self,
        visibility: &'v BoundVisibility<'m, 'c, 's>,
    ) -> Result<RetirementTargets<'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, N>> {
        let mut pins = self.pins.take().expect("original pin set");
        let owner = self.owner.take().expect("original idle owner");
        if !visibility.matches_core(pins.pool.core) {
            return combine(Err(unchanged(Cause::BindingMismatch)), pins.release())
                .and_then(|()| unreachable!());
        }
        let version = match owner.try_apply().and_then(|apply| apply.try_version()) {
            Ok(version) => version,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let mut targets = WriteTargets {
            guards: [const { None }; N],
            version: Some(version),
            pins: Some(pins),
        };
        // No page/tree/append/registry guard has been acquired. Publication is
        // released inside capture before the snapshot registry is consulted.
        let horizon =
            match visibility.capture_horizon(targets.version.as_ref().expect("original version")) {
                Ok(horizon) => horizon,
                Err(error) => return targets.reject(visibility_error(error)),
            };
        let pool = targets.pins.as_ref().expect("original pins").pool;
        for at in 0..targets.len() {
            let index = targets.pins.as_ref().expect("original pins").indices[at];
            match pool.frames[index].content.try_write() {
                Ok(guard) => targets.guards[at] = Some(guard),
                Err(TryLockError::WouldBlock) => return targets.reject(unchanged(Cause::Busy)),
                Err(TryLockError::Poisoned(error)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(error.into_inner());
                    return targets.reject(failure);
                }
            }
            if let Err(error) = targets
                .pins
                .as_ref()
                .expect("original pins")
                .registration(at)
                .and_then(|_| pool.health())
                .and_then(|()| {
                    pool.pair(
                        index,
                        targets.guards[at].as_ref().expect("acquired page guard"),
                    )
                    .map(|_| ())
                })
            {
                return targets.reject(error);
            }
        }
        Ok(RetirementTargets {
            visibility,
            horizon,
            targets,
            released: false,
        })
    }
}

impl<'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, const N: usize>
    RetirementTargets<'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, N>
{
    pub(in crate::storage) fn version_identity(&self) -> Result<VersionIdentity> {
        let pool = self.targets.pins.as_ref().expect("original pins").pool;
        self.targets
            .version
            .as_ref()
            .expect("original version")
            .visibility_identity(pool.core)
            .map_err(DirtyError::from)
    }
    /// Scalar observations are not a retirement capability.
    pub(in crate::storage) fn q_and_horizon(&self) -> (u64, u64) {
        (self.horizon.published(), self.horizon.horizon())
    }
    pub(in crate::storage) fn observe(&self, at: usize) -> Result<DirtyObservation> {
        let pins = self.targets.pins.as_ref().expect("original pins");
        pins.pool.health()?;
        let guard = self
            .targets
            .guards
            .get(at)
            .and_then(Option::as_ref)
            .ok_or_else(|| unchanged(Cause::Shape))?;
        pins.registration(at)?;
        pins.pool.pair(pins.indices[at], guard)
    }
    pub(in crate::storage) fn bytes(&self, at: usize) -> Result<&[u8; PAGE_BYTES]> {
        self.observe(at)?;
        Ok(self.targets.guards[at]
            .as_ref()
            .expect("observed page")
            .bytes)
    }
    pub(in crate::storage) fn preflight_dpt(
        &self,
        at: usize,
        expected: &FrameStamp,
    ) -> Result<DptStamp<'b>> {
        self.targets.preflight_dpt(at, expected)
    }
    /// The original target pins are the entire allowed physical population at
    /// CLR admission. The writer also borrows the original flush owner.
    pub(in crate::storage) fn preflight_catalog_pair(
        &self,
        pair: &[TargetRequest; 2],
    ) -> Result<()> {
        let pins = self.targets.pins.as_ref().expect("original pins");
        let state = pins.pool.entry_registry()?;
        if state.total as usize != pins.len
            || state.init_claim.is_some()
            || state.claim.is_some()
            || state.load.is_some()
            || state.directory.closed_pair().is_some()
        {
            return Err(unchanged(Cause::Busy));
        }
        for request in pair {
            let at = (0..pins.len)
                .find(|at| {
                    state
                        .directory
                        .resolve(pins.pool.frames, pins.indices[*at])
                        .is_some_and(|registration| registration.request == *request)
                })
                .ok_or_else(|| pins.pool.fatal(Cause::Invariant))?;
            let index = pins.indices[at];
            if state.counts[index] != 1 {
                return Err(unchanged(Cause::Busy));
            }
        }
        Ok(())
    }
    #[allow(
        clippy::result_large_err,
        reason = "rejection returns the original linear UndoPin without heap allocation"
    )]
    pub(in crate::storage) fn begin<'t>(
        &'t mut self,
        verified: VerifiedRetirement<'c, 's>,
        current: Option<UndoPin<'v, 'm, 'c, 's>>,
    ) -> std::result::Result<
        RetirementApply<'t, 'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, N>,
        RetirementReject<'v, 'm, 'c, 's>,
    > {
        let intent = self.visibility.begin_retirement(
            &self.horizon,
            self.targets.version.as_ref().expect("original version"),
            verified,
            current,
        )?;
        Ok(RetirementApply {
            targets: self,
            intent: Some(intent),
        })
    }
    fn release(&mut self) -> Result<()> {
        if self.released {
            return Ok(());
        }
        self.released = true;
        let checked = self
            .visibility
            .check_retirement_released()
            .map_err(visibility_error);
        combine(checked, self.targets.release())
    }
    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}

impl<const N: usize> Drop for RetirementTargets<'_, '_, '_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Keeps intent and original target borrow together. Releasing the targets or
/// issuing a second retirement while this scope is live is not expressible.
pub(in crate::storage) struct RetirementApply<'t, 'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    targets: &'t mut RetirementTargets<'v, 'm, 'a, 'b, 'c, 's, 'p, 'd, N>,
    intent: Option<RetirementIntent<'v, 'm, 'c, 's>>,
}
impl<'v, 'm, 'c, 's, const N: usize> RetirementApply<'_, 'v, 'm, '_, '_, 'c, 's, '_, '_, N> {
    pub(in crate::storage) fn stage_call_started(&mut self) -> VisibilityResult<()> {
        self.intent
            .as_mut()
            .expect("owned retirement intent")
            .stage_call_started()
    }
    pub(in crate::storage) fn reject_catalog_stage(
        &mut self,
        receipt: crate::storage::wal_io::CatalogRetirementRejected<'_, '_, 'c, 's>,
    ) -> VisibilityResult<()> {
        self.intent
            .as_mut()
            .expect("owned retirement intent")
            .reject_catalog_stage(receipt)
    }
    pub(in crate::storage) fn bind_staged(
        &mut self,
        staged: StagedRetirement<'c, 's>,
    ) -> VisibilityResult<()> {
        self.intent
            .as_mut()
            .expect("owned retirement intent")
            .bind_staged(staged)
    }
    pub(in crate::storage) fn install_page(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.intent
            .as_ref()
            .expect("owned retirement intent")
            .confirms_requirement(&permit.requirement())
            .map_err(visibility_error)?;
        self.targets.targets.install_page(at, permit)
    }
    pub(in crate::storage) fn install_metadata(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.intent
            .as_ref()
            .expect("owned retirement intent")
            .confirms_requirement(&permit.requirement())
            .map_err(visibility_error)?;
        self.targets.targets.install_metadata(at, permit)
    }
    pub(in crate::storage) fn install_dpt(
        &self,
        stamp: DptStamp<'_>,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.intent
            .as_ref()
            .expect("owned retirement intent")
            .confirms_requirement(&permit.requirement())
            .map_err(visibility_error)?;
        self.targets.targets.install_dpt(stamp, permit)
    }
    pub(in crate::storage) fn cancel_unstaged(
        mut self,
    ) -> VisibilityResult<Option<UndoPin<'v, 'm, 'c, 's>>> {
        self.intent
            .take()
            .expect("owned retirement intent")
            .cancel_unstaged()
    }
    pub(in crate::storage) fn finish_applied(
        mut self,
        applied: AppliedRetirement<'c, 's>,
    ) -> VisibilityResult<()> {
        self.intent
            .take()
            .expect("owned retirement intent")
            .finish_applied(applied)
    }
}

pub(in crate::storage) const fn retirement_source_bytes<const N: usize>() -> usize {
    use std::mem::size_of;
    type Targets<'a, const N: usize> = RetirementTargets<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Apply<'a, const N: usize> = RetirementApply<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, N>;
    target_set_call_bytes::<N>()
        + size_of::<Targets<'_, N>>()
        + 2 * size_of::<Result<Targets<'_, N>>>()
        + size_of::<HorizonObservation<'_, '_>>()
        + 2 * size_of::<VisibilityResult<HorizonObservation<'_, '_>>>()
        + size_of::<Apply<'_, N>>()
        + 2 * size_of::<std::result::Result<Apply<'_, N>, RetirementReject<'_, '_, '_, '_>>>()
        + 2 * size_of::<VisibilityResult<()>>()
        + 2 * size_of::<VisibilityError>()
        + 2 * size_of::<VersionIdentity>()
        + 128
}
