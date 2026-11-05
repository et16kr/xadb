//! Copy-only one/two-victim selection for the original structural claim.
//! No allocator, INIT, content replacement or WAL authority originates here.
#![allow(
    dead_code,
    reason = "original Structural claim consumes this selection next"
)]
use super::*;

pub(in crate::storage) struct StructuralCandidates<'b, 'c, 's> {
    pub(in super::super) anchor: &'b u8,
    pub(in super::super) core: &'c PageConcurrency<'s>,
    pub(in super::super) binding: &'s AtomicU8,
    pub(in super::super) fence: &'s SessionFence,
    pub(in super::super) targets: [TargetRequest; 5],
    pub(in super::super) init: [TargetRequest; 2],
    pub(in super::super) count: u8,
    pub(in super::super) slots: [StatusCandidate; 2],
    pub(in super::super) next_generations: [u64; 2],
}
impl StructuralCandidates<'_, '_, '_> {
    pub(in crate::storage) fn requires_flush(&self) -> bool {
        self.slots[..usize::from(self.count)]
            .iter()
            .any(|slot| slot.kind == StatusCandidateKind::Flush)
    }
}

impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    pub(in crate::storage) fn structural_candidates(
        &self,
        init: &[TargetRequest],
        start: usize,
        targets: &[TargetRequest; 5],
    ) -> Result<StructuralCandidates<'_, 'c, 's>> {
        self.health()?;
        if !matches!(init.len(), 1 | 2) || start == 0 {
            return Err(unchanged(Cause::Shape));
        }
        for (at, request) in targets.iter().enumerate() {
            if at > 0 && targets[at - 1].rank() >= request.rank()
                || targets[..at]
                    .iter()
                    .any(|prior| prior.same_address(request))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        for (at, request) in init.iter().enumerate() {
            if !targets.contains(request)
                || init[..at].iter().any(|prior| prior.same_address(request))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        let first = self.retention_candidate_at(Some(init[0]), start, targets, 5, None)?;
        if first.kind == StatusCandidateKind::Resident {
            return Err(unchanged(Cause::AlreadyInitialized));
        }
        let mut slots = [first; 2];
        if init.len() == 2 {
            slots[1] = self.retention_candidate_at(
                Some(init[1]),
                first.next,
                targets,
                5,
                Some(first.index),
            )?;
            if slots[1].kind == StatusCandidateKind::Resident {
                return Err(unchanged(Cause::AlreadyInitialized));
            }
        }
        self.freeze_structural_candidates(init, targets, slots)
    }

    fn freeze_structural_candidates(
        &self,
        init: &[TargetRequest],
        targets: &[TargetRequest; 5],
        slots: [StatusCandidate; 2],
    ) -> Result<StructuralCandidates<'_, 'c, 's>> {
        // Declaration order releases registry, second, first on every exit.
        let first = self.frames[slots[0].index]
            .content
            .try_read()
            .map_err(|error| self.load_lock_error(error))?;
        let second = if init.len() == 2 {
            Some(
                self.frames[slots[1].index]
                    .content
                    .try_read()
                    .map_err(|error| self.load_lock_error(error))?,
            )
        } else {
            None
        };
        let guards = [Some(&first), second.as_ref()];
        let state = self.entry_registry()?;
        if state.total != 0
            || state.load.is_some()
            || state.claim.is_some()
            || state.init_claim.is_some()
            || state.directory.closed_pair().is_some()
        {
            return Err(unchanged(Cause::Busy));
        }
        let mut next_generations = [0; 2];
        for at in 0..init.len() {
            let index = slots[at].index;
            let content = guards[at].as_ref().expect("active selected slot");
            let current = if let Some(registration) = state.directory.resolve(self.frames, index) {
                if targets
                    .iter()
                    .any(|target| target.same_address(&registration.request))
                {
                    return Err(unchanged(Cause::Stale));
                }
                Some(self.pair_in(&state, index, content)?.frame)
            } else if content.fresh()
                && state.entries[index].is_none()
                && state.requirements[index].is_none()
            {
                None
            } else {
                return Err(self.fatal(Cause::Invariant));
            };
            if current != slots[at].frame {
                return Err(unchanged(Cause::Stale));
            }
            next_generations[at] = state
                .directory
                .next_retention_generation(index)
                .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        }
        Ok(StructuralCandidates {
            anchor: &self.stamp_anchor,
            core: self.core,
            binding: self.binding,
            fence: self.fence,
            targets: *targets,
            init: [init[0], *init.get(1).unwrap_or(&init[0])],
            count: init.len() as u8,
            slots,
            next_generations,
        })
    }
}

pub(in crate::storage) fn structural_selection_source_bytes() -> usize {
    use std::mem::size_of;
    super::source_census()
        + 3 * size_of::<StructuralCandidates<'_, '_, '_>>()
        + 2 * size_of::<Result<StructuralCandidates<'_, '_, '_>>>()
        + 2 * size_of::<StatusCandidate>()
        + 2 * size_of::<Result<StatusCandidate>>()
        + size_of::<[StatusCandidate; 2]>()
        + size_of::<[Option<RwLockReadGuard<'_, FrameContent<'_>>>; 2]>()
        + size_of::<[Option<&RwLockReadGuard<'_, FrameContent<'_>>>; 2]>()
        + size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<FrameStamp>>()
        + size_of::<Result<DirtyObservation>>()
        + size_of::<[u64; 2]>()
        + 16 * size_of::<&[u8]>()
        + 16 * size_of::<usize>()
}

#[cfg(test)]
#[path = "structural_residency_tests.rs"]
mod tests;
