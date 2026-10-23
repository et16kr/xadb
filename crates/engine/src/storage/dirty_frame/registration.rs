//! One bounded lookup for bootstrap registrations and the closed INIT publication.
//! Copy snapshots carry identity across pin/content/flush boundaries. The
//! consuming allocator mode renews or evicts/reloads the same address, retaining
//! slot-generation tombstones. General page loading/reuse remain separate.

use super::{DirtyFrame, MAX_FRAMES, PageKind, Registration, TargetRequest};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fixed,
    AllocatorReloadable,
}

pub(super) struct RegistrationDirectory {
    mode: Mode,
    entries: [Option<Registration>; MAX_FRAMES],
    closed_index: Option<usize>,
    pub(super) publication: OnceLock<Registration>,
    resident: u64,
    pub(super) eviction_epoch: u64,
}
impl RegistrationDirectory {
    pub(super) fn from_bootstrap(frames: &[DirtyFrame<'_>]) -> Option<Self> {
        let mut entries = [None; MAX_FRAMES];
        for (index, frame) in frames.iter().enumerate() {
            let entry = entries.get_mut(index)?;
            *entry = frame.registration;
            if let Some(registration) = frame.registration
                && (registration.generation != 1
                    || frames[..index]
                        .iter()
                        .filter_map(|frame| frame.registration)
                        .any(|prior| prior.request.same_address(&registration.request)))
            {
                return None;
            }
        }
        Some(Self {
            mode: Mode::Fixed,
            entries,
            closed_index: None,
            publication: OnceLock::new(),
            resident: 0,
            eviction_epoch: 0,
        })
    }

    pub(super) fn close_status_slot(&mut self) {
        self.closed_index = Some(1);
    }
    pub(super) fn closed_index(&self) -> Option<usize> {
        self.closed_index
    }
    pub(super) fn published(&self) -> Option<Registration> {
        self.publication.get().copied()
    }
    // The exclusive INIT writer prechecks None before stage. Equality also
    // protects the final publication from silently accepting another value.
    pub(super) fn publish(&self, registration: Registration) -> bool {
        *self.publication.get_or_init(|| registration) == registration
    }

    pub(super) fn resolve(&self, frames: &[DirtyFrame<'_>], index: usize) -> Option<Registration> {
        let frame = frames.get(index)?;
        let original = *self.entries.get(index)?;
        if self.closed_index == Some(index) {
            if original.is_some() || frame.registration.is_some() {
                return None;
            }
            self.published().filter(|value| value.generation != 0)
        } else {
            original.filter(|value| {
                value.generation != 0
                    && match self.mode {
                        Mode::Fixed => frame.registration == Some(*value),
                        Mode::AllocatorReloadable => self.resident & (1u64 << index) != 0,
                    }
            })
        }
    }

    pub(super) fn lookup(
        &self,
        frames: &[DirtyFrame<'_>],
        request: TargetRequest,
    ) -> Option<(usize, Registration)> {
        (0..frames.len()).find_map(|index| {
            self.resolve(frames, index)
                .filter(|entry| entry.request == request)
                .map(|entry| (index, entry))
        })
    }

    pub(super) fn enable_allocator_reload(&mut self, frames: &[DirtyFrame<'_>]) -> bool {
        if self.mode != Mode::Fixed
            || !self.valid(frames)
            || self.closed_index.is_some()
            || self.published().is_some()
            || self.entries[1..].iter().any(Option::is_some)
            || !self.entries[0].is_some_and(|entry| {
                entry.request.kind == PageKind::Allocation
                    && entry.request.level == 0
                    && entry.request.identity.page.page_no == 0
            })
        {
            return false;
        }
        self.mode = Mode::AllocatorReloadable;
        self.resident = 1;
        true
    }
    pub(super) fn allocator_index(&self) -> Option<usize> {
        (self.mode == Mode::AllocatorReloadable && self.resident.count_ones() == 1)
            .then(|| self.resident.trailing_zeros() as usize)
    }
    #[cfg(test)]
    pub(super) fn test_seed_reload_generation(&mut self, generation: u64) {
        assert!(self.mode == Mode::AllocatorReloadable);
        let index = self.allocator_index().unwrap_or(0);
        self.entries[index].as_mut().unwrap().generation = generation;
    }
    pub(super) fn can_renew_allocator(&self, index: usize, previous: Registration) -> bool {
        self.mode == Mode::AllocatorReloadable
            && self.allocator_index() == Some(index)
            && self.entries.get(index) == Some(&Some(previous))
            && previous.generation != 0
            && previous.generation != u64::MAX
    }
    pub(super) fn renew_allocator(&mut self, index: usize, previous: Registration) -> Option<u64> {
        if !self.can_renew_allocator(index, previous) {
            return None;
        }
        let generation = previous.generation.checked_add(1)?;
        self.entries[index] = Some(Registration {
            generation,
            ..previous
        });
        Some(generation)
    }

    pub(super) fn evict_allocator(&mut self, index: usize, previous: Registration) -> Option<u64> {
        if self.allocator_index() != Some(index) || self.entries[index] != Some(previous) {
            return None;
        }
        let epoch = self.eviction_epoch.checked_add(1)?;
        self.resident = 0;
        self.eviction_epoch = epoch;
        Some(epoch)
    }

    /// An inactive entry is a generation tombstone, never an address authority.
    /// The exclusive eviction guard owns the load claim; every issued generation
    /// remains consumed even if reading fails before publication.
    pub(super) fn claim_allocator_load(
        &mut self,
        index: usize,
        request: TargetRequest,
    ) -> Option<Registration> {
        if self.mode != Mode::AllocatorReloadable || self.resident != 0 {
            return None;
        }
        let entry = self.entries.get_mut(index)?;
        let generation = entry.map_or(Some(1), |prior| prior.generation.checked_add(1))?;
        let registration = Registration {
            request,
            generation,
        };
        *entry = Some(registration);
        Some(registration)
    }

    pub(super) fn matches_load(
        &self,
        index: usize,
        registration: Registration,
        epoch: u64,
    ) -> bool {
        self.mode == Mode::AllocatorReloadable
            && self.resident == 0
            && self.eviction_epoch == epoch
            && self.entries.get(index) == Some(&Some(registration))
    }
    pub(super) fn publish_load(&mut self, index: usize) {
        // Caller validates the exact claim and all content before copying.
        self.resident = 1u64 << index;
    }

    /// Fixed-adapter provenance, including its generation-one restriction.
    /// This is not a future movable-directory or page reuse invariant.
    pub(super) fn valid(&self, frames: &[DirtyFrame<'_>]) -> bool {
        if self.mode == Mode::AllocatorReloadable {
            let Some(initial) = frames.first().and_then(|frame| frame.registration) else {
                return false;
            };
            return self.closed_index.is_none()
                && self.published().is_none()
                && initial.generation == 1
                && initial.request.kind == PageKind::Allocation
                && initial.request.level == 0
                && initial.request.identity.page.page_no == 0
                && frames[1..].iter().all(|frame| frame.registration.is_none())
                && self.resident.count_ones() <= 1
                && (self.resident == 0 || self.resident.trailing_zeros() < frames.len() as u32)
                && self
                    .entries
                    .iter()
                    .enumerate()
                    .all(|(index, entry)| match entry {
                        Some(entry) => {
                            index < frames.len()
                                && entry.generation != 0
                                && entry.request == initial.request
                        }
                        None => self.resident & (1u64 << index) == 0,
                    });
        }
        if self.closed_index.is_none() && self.published().is_some() {
            return false;
        }
        if let Some(index) = self.closed_index {
            if index != 1
                || frames.len() != 2
                || self.entries[index].is_some()
                || frames[index].registration.is_some()
            {
                return false;
            }
            if let Some(published) = self.published() {
                let Some(allocator) = self.resolve(frames, 0) else {
                    return false;
                };
                if published.generation != 1
                    || published.request.kind != PageKind::TransactionStatus
                    || published.request.level != 0
                    || published.request.identity.database != allocator.request.identity.database
                    || published.request.identity.page.file != allocator.request.identity.page.file
                    || published.request.identity.page.page_no != 1
                    || published.request.identity.page.incarnation.get() < 2
                    || published.request.identity.owner.get() != 2
                    || published.request.identity.owner_generation.get() != 1
                {
                    return false;
                }
            }
        }
        self.entries.iter().enumerate().all(|(index, entry)| {
            if self.closed_index == Some(index) {
                return entry.is_none();
            }
            match (entry, frames.get(index)) {
                (Some(entry), Some(_)) => {
                    (entry.generation == 1 || self.mode == Mode::AllocatorReloadable)
                        && self.resolve(frames, index) == Some(*entry)
                }
                (None, Some(frame)) => frame.registration.is_none(),
                (None, None) => true,
                (Some(_), None) => false,
            }
        })
    }
}
