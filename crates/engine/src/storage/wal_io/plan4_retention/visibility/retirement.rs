use super::{authority::ReferenceTarget, *};

/// Rejection returns the exact traversal token, including a foreign token. No
/// caller-supplied actor/count is subtracted from a registry census.
pub(in crate::storage) struct RetirementReject<'v, 'a, 'c, 's> {
    pub(in crate::storage) error: VisibilityError,
    pub(in crate::storage) current: Option<UndoPin<'v, 'a, 'c, 's>>,
}

/// Kept inside the original retirement target wrapper until explicit cancel or
/// actual complete installation. Drop preserves the marker and fences.
pub(in crate::storage) struct RetirementIntent<'v, 'a, 'c, 's> {
    pub(super) visibility: &'v BoundVisibility<'a, 'c, 's>,
    pub(super) generation: u64,
    pub(super) scope: RetirementScope,
    pub(super) target: ReferenceTarget,
    pub(super) group_digest: [u8; 32],
    pub(super) requirement: Option<WalRequirement<'s>>,
    current: Option<UndoPin<'v, 'a, 'c, 's>>,
    active: bool,
}

impl<'a, 'c, 's> BoundVisibility<'a, 'c, 's> {
    /// Only the same-gate, actual semantic Before supplies VerifiedRetirement.
    /// The calling target wrapper retains the original gate through application.
    #[allow(
        clippy::result_large_err,
        reason = "rejection returns the original linear UndoPin without heap allocation"
    )]
    pub(in crate::storage) fn begin_retirement<'v>(
        &'v self,
        horizon: &HorizonObservation<'c, 's>,
        version: &VersionEditGuard<'_, 'c, 's>,
        verified: VerifiedRetirement<'c, 's>,
        mut current: Option<UndoPin<'v, 'a, 'c, 's>>,
    ) -> Result<RetirementIntent<'v, 'a, 'c, 's>, RetirementReject<'v, 'a, 'c, 's>> {
        let checked = (|| -> VisibilityResult<u64> {
            self.check_horizon(horizon, version)?;
            if !self.origin.same(&verified.origin) || verified.version != horizon.version {
                return Err(unchanged(VisibilityCause::BindingMismatch));
            }
            match (
                verified.scope,
                verified.target,
                verified.committed_q,
                current.as_ref(),
            ) {
                (RetirementScope::MissingPrune, ReferenceTarget::Undo(_), Some(q), None)
                    if q != 0 =>
                {
                    if q > horizon.published || q > horizon.horizon {
                        return Err(unchanged(VisibilityCause::Busy));
                    }
                }
                (RetirementScope::MissingClr, ReferenceTarget::Undo(_), None, Some(pin)) => {
                    if !pin.pin.visibility.same_registry(self)
                        || pin.pin.target != verified.target
                        || pin.pin.owner != horizon.version.owner()
                        || !pin.is_rollback_for(verified.actor)
                    {
                        return Err(unchanged(VisibilityCause::BindingMismatch));
                    }
                }
                (RetirementScope::UndoFree, ReferenceTarget::Undo(_), None, None)
                | (RetirementScope::TerminalFree, ReferenceTarget::Transaction(_), None, None) => {}
                _ => return Err(self.origin.fatal(VisibilityCause::Invariant)),
            }
            let mut bytes = self.entry()?;
            if retirement_row(&bytes[..])[0] != 0 {
                return Err(unchanged(VisibilityCause::Busy));
            }
            if let Some(pin) = current.as_ref() {
                pin.pin.check(&bytes[..], 2)?;
            }
            if pins::count(&bytes[..], verified.target) != usize::from(current.is_some()) {
                return Err(unchanged(VisibilityCause::Busy));
            }
            let generation = u64_at(&bytes[..], 8)
                .checked_add(1)
                .ok_or_else(|| unchanged(VisibilityCause::Exhausted))?;
            // All checks are complete. The zero census, own-pin transfer and
            // retiring-key publication are one registry critical section.
            if let Some(pin) = current.as_ref() {
                reference_mut(&mut bytes[..], pin.pin.row)[0] = pins::TRANSFERRED;
            }
            put_u64(&mut bytes[..], 8, generation);
            let marker = retirement_mut(&mut bytes[..]);
            marker.fill(0);
            marker[0] = 1;
            marker[1] = verified.target.kind();
            marker[2] = verified.scope as u8;
            put_u64(marker, 8, generation);
            put_u64(
                marker,
                16,
                current.as_ref().map_or(u64::MAX, |pin| pin.pin.row as u64),
            );
            put_u64(
                marker,
                24,
                current.as_ref().map_or(0, |pin| pin.pin.generation),
            );
            marker[32..64].copy_from_slice(&verified.group_digest);
            verified.target.encode(&mut marker[64..112]);
            Ok(generation)
        })();
        match checked {
            Ok(generation) => {
                if let Some(pin) = current.as_mut() {
                    pin.pin.active = false;
                }
                Ok(RetirementIntent {
                    visibility: self,
                    generation,
                    scope: verified.scope,
                    target: verified.target,
                    group_digest: verified.group_digest,
                    requirement: None,
                    current,
                    active: true,
                })
            }
            Err(error) => Err(RetirementReject { error, current }),
        }
    }
}

impl<'v, 'a, 'c, 's> RetirementIntent<'v, 'a, 'c, 's> {
    fn check(&self, bytes: &[u8], state: u8) -> VisibilityResult<()> {
        let marker = retirement_row(bytes);
        if !self.active
            || marker[0] != state
            || marker[1] != self.target.kind()
            || marker[2] != self.scope as u8
            || u64_at(marker, 8) != self.generation
            || marker[32..64] != self.group_digest
            || ReferenceTarget::decode(marker[1], &marker[64..112]).ok() != Some(self.target)
            || u64_at(marker, 16)
                != self
                    .current
                    .as_ref()
                    .map_or(u64::MAX, |pin| pin.pin.row as u64)
            || u64_at(marker, 24) != self.current.as_ref().map_or(0, |pin| pin.pin.generation)
        {
            return Err(self.visibility.origin.fatal(VisibilityCause::Invariant));
        }
        if let Some(pin) = self.current.as_ref() {
            pin.pin.check(bytes, pins::TRANSFERRED)?;
        }
        Ok(())
    }
    /// Set immediately before entering the original semantic WAL stage call.
    /// This is not the physical first-write marker and never claims durability.
    pub(in crate::storage) fn stage_call_started(&mut self) -> VisibilityResult<()> {
        let mut bytes = self.visibility.entry()?;
        self.check(&bytes[..], 1)?;
        retirement_mut(&mut bytes[..])[0] = 2;
        Ok(())
    }
    /// Bind the completed actual semantic stage before accepting any dirty
    /// permit. Its physical interval alone cannot construct this receipt.
    pub(in crate::storage) fn bind_staged(
        &mut self,
        staged: StagedRetirement<'c, 's>,
    ) -> VisibilityResult<()> {
        if !self.visibility.origin.same(&staged.origin)
            || staged.generation != self.generation
            || staged.scope != self.scope
            || staged.target != self.target
            || staged.group_digest != self.group_digest
            || !staged.requirement.uses_fence(self.visibility.origin.fence)
            || self.requirement.is_some()
        {
            return Err(self
                .visibility
                .origin
                .fatal(VisibilityCause::BindingMismatch));
        }
        let mut bytes = self.visibility.entry()?;
        self.check(&bytes[..], 2)?;
        retirement_mut(&mut bytes[..])[0] = 3;
        self.requirement = Some(staged.requirement);
        Ok(())
    }
    pub(in crate::storage) fn confirms_requirement(
        &self,
        requirement: &WalRequirement<'_>,
    ) -> VisibilityResult<()> {
        let bytes = self.visibility.entry()?;
        self.check(&bytes[..], 3)?;
        if !requirement.uses_fence(self.visibility.origin.fence)
            || self
                .requirement
                .is_none_or(|expected| expected.location() != requirement.location())
        {
            return Err(self
                .visibility
                .origin
                .fatal(VisibilityCause::BindingMismatch));
        }
        Ok(())
    }
    pub(in crate::storage) fn cancel_unstaged(
        mut self,
    ) -> VisibilityResult<Option<UndoPin<'v, 'a, 'c, 's>>> {
        {
            let mut bytes = self.visibility.entry()?;
            self.check(&bytes[..], 1)?;
            if let Some(pin) = self.current.as_ref() {
                reference_mut(&mut bytes[..], pin.pin.row)[0] = 2;
            }
            retirement_mut(&mut bytes[..]).fill(0);
        }
        if let Some(pin) = self.current.as_mut() {
            pin.pin.active = true;
        }
        self.active = false;
        Ok(self.current.take())
    }
    pub(in crate::storage) fn finish_applied(
        mut self,
        applied: AppliedRetirement<'c, 's>,
    ) -> VisibilityResult<()> {
        if !self.visibility.origin.same(&applied.origin)
            || applied.generation != self.generation
            || applied.scope != self.scope
            || applied.target != self.target
            || applied.group_digest != self.group_digest
            || !applied.requirement.uses_fence(self.visibility.origin.fence)
            || self
                .requirement
                .is_none_or(|expected| expected.location() != applied.requirement.location())
        {
            return Err(self
                .visibility
                .origin
                .fatal(VisibilityCause::BindingMismatch));
        }
        {
            let mut bytes = self.visibility.entry()?;
            self.check(&bytes[..], 3)?;
            if let Some(pin) = self.current.as_ref() {
                pins::clear_target(reference_mut(&mut bytes[..], pin.pin.row));
            }
            retirement_mut(&mut bytes[..]).fill(0);
        }
        self.active = false;
        Ok(())
    }
}
impl Drop for RetirementIntent<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.visibility.origin.fence.fence();
        }
    }
}

pub(super) fn validate(bytes: &[u8]) -> Result<(), ()> {
    let marker = retirement_row(bytes);
    if marker[0] == 0 {
        return if marker.iter().all(|v| *v == 0) {
            Ok(())
        } else {
            Err(())
        };
    }
    if !matches!(marker[0], 1..=3)
        || marker[3..8]
            .iter()
            .chain(marker[112..128].iter())
            .any(|v| *v != 0)
        || u64_at(marker, 8) == 0
        || u64_at(marker, 8) != u64_at(bytes, 8)
    {
        return Err(());
    }
    let scope = RetirementScope::decode(marker[2]).ok_or(())?;
    let target = ReferenceTarget::decode(marker[1], &marker[64..112])?;
    if matches!(scope, RetirementScope::TerminalFree)
        != matches!(target, ReferenceTarget::Transaction(_))
        || pins::count(bytes, target) != 0
    {
        return Err(());
    }
    if scope == RetirementScope::MissingClr {
        let index = usize::try_from(u64_at(marker, 16)).map_err(|_| ())?;
        if index >= REFERENCES {
            return Err(());
        }
        let pin = reference_row(bytes, index);
        if pin[0] != pins::TRANSFERRED || u64_at(pin, 8) != u64_at(marker, 24) {
            return Err(());
        }
    } else if u64_at(marker, 16) != u64::MAX || u64_at(marker, 24) != 0 {
        return Err(());
    }
    Ok(())
}
