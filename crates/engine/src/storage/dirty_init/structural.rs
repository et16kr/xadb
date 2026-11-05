//! Metadata for the original pool's atomic Row-tree structural group.
//! Semantic authority remains sealed by the original writer.
use super::*;

#[path = "structural/live.rs"]
mod live;
pub(in crate::storage) use live::source_bytes as structural_effect_pool_source_bytes;
pub(in crate::storage) use live::{
    CompletedStructural, StructuralClaimKey, StructuralWriteTargets,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StructuralClaimRecord {
    init: [TargetRequest; 2],
    work: SystemWorkRef,
    birth: PageLsn,
    generations: [u64; 2],
    indices: [u8; 2],
    init_count: u8,
    target_mask: u64,
}

impl StructuralClaimRecord {
    fn count(&self) -> Option<usize> {
        matches!(self.init_count, 1 | 2).then_some(usize::from(self.init_count))
    }

    pub(super) fn owns_index(&self, index: usize) -> bool {
        self.count().is_some_and(|count| {
            self.indices[..count]
                .iter()
                .any(|value| usize::from(*value) == index)
        })
    }

    pub(super) fn blocks_load(&self, index: usize, request: TargetRequest) -> bool {
        let Some(count) = self.count() else {
            return true;
        };
        (index < MAX_FRAMES && self.target_mask & (1 << index) != 0)
            || self.init[..count]
                .iter()
                .any(|value| value.same_address(&request))
    }

    #[cfg(test)]
    pub(super) fn diagnostic_registration(&self, index: usize) -> Option<Registration> {
        self.indices[..self.count()?]
            .iter()
            .position(|value| usize::from(*value) == index)
            .map(|at| Registration {
                request: self.init[at],
                generation: self.generations[at],
            })
    }

    fn shape(&self) -> bool {
        let Some(count) = self.count() else {
            return false;
        };
        let right = &self.init[0];
        self.birth.0 != 0
            && self.work.id() == self.birth.0
            && self.work.kind() == SystemWorkKind::StructuralChange
            && right.level < 15
            && right.kind
                == if right.level == 0 {
                    PageKind::RowLeaf
                } else {
                    PageKind::RowInternal
                }
            && self.init[..count].iter().all(|request| {
                request.identity.database == self.work.namespace().database
                    && request.identity.page.file == right.identity.page.file
                    && (9..64).contains(&request.identity.page.page_no)
                    && request.identity.page.incarnation.get() >= 2
                    && request.identity.owner == right.identity.owner
                    && request.identity.owner.get() >= 6
                    && request.identity.owner_generation.get() == 1
            })
            && if count == 1 {
                self.init[1] == self.init[0] && self.generations[1] == 0 && self.indices[1] == 0
            } else {
                self.init[1].kind == PageKind::RowInternal
                    && self.init[1].level == right.level + 1
                    && !right.same_address(&self.init[1])
                    && right.rank() < self.init[1].rank()
                    && self.indices[0] != self.indices[1]
            }
    }

    pub(super) fn held_pins_valid(&self, state: &PoolRegistry<'_>) -> bool {
        self.target_mask.count_ones() == 5
            && (0..MAX_FRAMES)
                .all(|index| self.target_mask & (1 << index) == 0 || state.counts[index] != 0)
    }

    pub(super) fn valid_in(
        &self,
        pool: &SharedPool<'_, '_, '_, '_>,
        state: &PoolRegistry<'_>,
    ) -> bool {
        if !self.shape() || !state.directory.retention_payloads() {
            return false;
        }
        let count = usize::from(self.init_count); // shape checked before slicing/indexing
        if self.target_mask.count_ones() != 5
            || self.target_mask & 1 == 0
            || (pool.frames.len()..MAX_FRAMES).any(|index| self.target_mask & (1 << index) != 0)
            || self.indices[..count].iter().enumerate().any(|(at, index)| {
                *index == 0
                    || usize::from(*index) >= pool.frames.len()
                    || self.generations[at] == 0
                    || self.target_mask & (1 << index) == 0
            })
        {
            return false;
        }
        let closed_matches = if count == 1 {
            state.directory.closed_pair().is_none()
                && state.directory.closed_index() == Some(usize::from(self.indices[0]))
                && state.directory.closed_generation() == Some(self.generations[0])
        } else {
            state.directory.closed_index().is_none()
                && state.directory.closed_pair() == Some(self.indices.map(usize::from))
                && state.directory.pair_generations() == Some(self.generations)
        };
        if !closed_matches {
            return false;
        }
        let mut roles = 0u8;
        for index in 0..pool.frames.len() {
            let registered = state.directory.resolve(pool.frames, index);
            if let Some(at) = self.indices[..count]
                .iter()
                .position(|value| usize::from(*value) == index)
            {
                if registered.is_some_and(|value| {
                    value.request != self.init[at] || value.generation != self.generations[at]
                }) {
                    return false;
                }
                continue;
            }
            if registered.is_some_and(|value| {
                self.init[..count]
                    .iter()
                    .any(|new| new.same_address(&value.request))
            }) {
                return false;
            }
            if self.target_mask & (1 << index) == 0 {
                continue;
            }
            let Some(registered) = registered else {
                return false;
            };
            let request = &registered.request;
            if request.identity.database != self.work.namespace().database
                || request.identity.page.file != self.init[0].identity.page.file
                || request.identity.owner_generation.get() != 1
            {
                return false;
            }
            let role = if index == 0 {
                if request.kind != PageKind::Allocation
                    || request.level != 0
                    || request.identity.owner.get() != 1
                    || request.identity.page.page_no != 0
                    || request.identity.page.incarnation.get() != 1
                    || registered.generation != 1
                {
                    return false;
                }
                1
            } else {
                if request.identity.owner != self.init[0].identity.owner
                    || !(9..64).contains(&request.identity.page.page_no)
                    || request.identity.page.incarnation.get() < 2
                {
                    return false;
                }
                match (request.kind, request.level) {
                    (PageKind::TreeAnchor, 0) => 2,
                    (kind, level) if kind == self.init[0].kind && level == self.init[0].level => 4,
                    (PageKind::RowInternal, level)
                        if count == 1 && level == self.init[0].level + 1 =>
                    {
                        8
                    }
                    _ => return false,
                }
            };
            if roles & role != 0 {
                return false;
            }
            roles |= role;
        }
        roles == if count == 1 { 15 } else { 7 }
    }
}

/// Nested claim validation borrows the record. Directory resolution packets,
/// shape/rank copies and slice/loop carriers are charged explicitly; no new
/// full record or page image is copied onto a common entry path.
pub(in crate::storage) fn source_observation() -> usize {
    use std::mem::size_of;
    2 * size_of::<SystemWorkRef>()
        + 2 * size_of::<crate::api::Namespace>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<Registration>>()
        + 2 * size_of::<Option<usize>>()
        + size_of::<[usize; 2]>()
        + size_of::<Option<[usize; 2]>>()
        + size_of::<[u64; 2]>()
        + size_of::<Option<[u64; 2]>>()
        + 2 * size_of::<(u64, u64, u64)>()
        + size_of::<[u8; 24]>()
        + 3 * size_of::<[u8; 8]>()
        + 2 * size_of::<crate::storage::page::PageRef>()
        + 3 * size_of::<std::result::Result<(), crate::storage::page::PageError>>()
        + 2 * size_of::<std::slice::Iter<'_, TargetRequest>>()
        + 2 * size_of::<std::slice::Iter<'_, u8>>()
        + size_of::<std::iter::Enumerate<std::slice::Iter<'_, u8>>>()
        + 2 * size_of::<std::ops::Range<usize>>()
        + super::super::registration::retention_source_observation()
}

#[cfg(test)]
#[path = "structural/tests.rs"]
mod tests;
