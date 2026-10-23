//! One bounded directory with a publication cell per caller frame.
//! Only exclusive pool mutation may issue generations or replace a cell. Shared
//! INIT publishes into its preclaimed generation without adding a lookup lock.
//! General multi-address loading and repeated INIT remain separate consumers.

use super::{DirtyFrame, MAX_FRAMES, PageKind, Registration, TargetRequest};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fixed,
    AllocatorReloadable,
}

struct Slot {
    generation: u64,
    request: OnceLock<TargetRequest>,
}
impl Slot {
    const fn empty() -> Self {
        Self {
            generation: 0,
            request: OnceLock::new(),
        }
    }

    fn registration(&self) -> Option<Registration> {
        self.request.get().copied().map(|request| Registration {
            request,
            generation: self.generation,
        })
    }

    // A failed generation check cannot initialize the publication cell. OnceLock
    // provides shared single publication, not address or initializer authority.
    fn publish(&self, registration: Registration) -> bool {
        self.generation != 0
            && self.generation == registration.generation
            && *self.request.get_or_init(|| registration.request) == registration.request
    }

    fn replace(&mut self, registration: Registration) {
        self.request.take();
        self.generation = registration.generation;
        // Exclusive access makes this assignment infallible.
        let _ = self.request.set(registration.request);
    }
}

pub(super) struct RegistrationDirectory {
    mode: Mode,
    entries: [Slot; MAX_FRAMES],
    closed_index: Option<usize>,
    resident: u64,
    pub(super) eviction_epoch: u64,
}
impl RegistrationDirectory {
    pub(super) fn from_bootstrap(frames: &[DirtyFrame<'_>]) -> Option<Self> {
        let mut entries = [const { Slot::empty() }; MAX_FRAMES];
        for (index, frame) in frames.iter().enumerate() {
            let entry = entries.get_mut(index)?;
            if let Some(registration) = frame.registration
                && (registration.generation != 1
                    || frames[..index]
                        .iter()
                        .filter_map(|frame| frame.registration)
                        .any(|prior| prior.request.same_address(&registration.request)))
            {
                return None;
            }
            if let Some(registration) = frame.registration {
                entry.replace(registration);
            }
        }
        Some(Self {
            mode: Mode::Fixed,
            entries,
            closed_index: None,
            resident: 0,
            eviction_epoch: 0,
        })
    }

    pub(super) fn close_status_slot(&mut self) {
        self.closed_index = Some(1);
        self.entries[1].generation = 1;
    }
    pub(super) fn closed_index(&self) -> Option<usize> {
        self.closed_index
    }
    pub(super) fn published(&self) -> Option<Registration> {
        self.closed_index
            .and_then(|index| self.entries[index].registration())
    }
    // The exclusive INIT writer prechecks None before stage. Equality also
    // protects the final publication from silently accepting another value.
    pub(super) fn publish(&self, registration: Registration) -> bool {
        self.closed_index
            .is_some_and(|index| self.entries[index].publish(registration))
    }

    pub(super) fn resolve(&self, frames: &[DirtyFrame<'_>], index: usize) -> Option<Registration> {
        let frame = frames.get(index)?;
        let original = self.entries.get(index)?.registration();
        if self.closed_index == Some(index) {
            if frame.registration.is_some() {
                return None;
            }
            original.filter(|value| value.generation != 0)
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
            || self.entries[1..]
                .iter()
                .any(|slot| slot.registration().is_some())
            || !self.entries[0].registration().is_some_and(|entry| {
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
        self.entries[index].generation = generation;
    }
    pub(super) fn can_renew_allocator(&self, index: usize, previous: Registration) -> bool {
        self.mode == Mode::AllocatorReloadable
            && self.allocator_index() == Some(index)
            && self.entries.get(index).and_then(Slot::registration) == Some(previous)
            && previous.generation != 0
            && previous.generation != u64::MAX
    }
    pub(super) fn renew_allocator(&mut self, index: usize, previous: Registration) -> Option<u64> {
        if !self.can_renew_allocator(index, previous) {
            return None;
        }
        let generation = previous.generation.checked_add(1)?;
        self.entries[index].generation = generation;
        Some(generation)
    }

    pub(super) fn evict_allocator(&mut self, index: usize, previous: Registration) -> Option<u64> {
        if self.allocator_index() != Some(index)
            || self.entries[index].registration() != Some(previous)
        {
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
        let generation = entry.generation.checked_add(1)?;
        let registration = Registration {
            request,
            generation,
        };
        entry.replace(registration);
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
            && self.entries.get(index).and_then(Slot::registration) == Some(registration)
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
                && self.entries.iter().enumerate().all(|(index, slot)| {
                    match slot.registration() {
                        Some(entry) => {
                            index < frames.len()
                                && entry.generation != 0
                                && entry.request == initial.request
                        }
                        None => slot.generation == 0 && self.resident & (1u64 << index) == 0,
                    }
                });
        }
        if self.closed_index.is_none() && self.published().is_some() {
            return false;
        }
        if let Some(index) = self.closed_index {
            if index != 1
                || frames.len() != 2
                || self.entries[index].generation != 1
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
        self.entries.iter().enumerate().all(|(index, slot)| {
            if self.closed_index == Some(index) {
                return true;
            }
            match (slot.registration(), frames.get(index)) {
                (Some(entry), Some(_)) => {
                    (entry.generation == 1 || self.mode == Mode::AllocatorReloadable)
                        && self.resolve(frames, index) == Some(entry)
                }
                (None, Some(frame)) => slot.generation == 0 && frame.registration.is_none(),
                (None, None) => slot.generation == 0,
                (Some(_), None) => false,
            }
        })
    }
}

pub(super) fn fits() -> bool {
    use std::mem::size_of;
    size_of::<Slot>() <= size_of::<Option<Registration>>() + 8
        && size_of::<RegistrationDirectory>() <= MAX_FRAMES * size_of::<Slot>() + 48
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page_io::test_support::expected;

    fn request(page_no: u64) -> TargetRequest {
        let mut identity = expected();
        identity.page.page_no = page_no;
        TargetRequest {
            identity,
            kind: PageKind::RowLeaf,
            level: 0,
        }
    }

    #[test]
    fn slot_wrong_generation_does_not_publish() {
        let mut slot = Slot::empty();
        let target = request(1);
        for generation in [0, 1, u64::MAX] {
            assert!(!slot.publish(Registration {
                request: target,
                generation
            }));
            assert!(slot.registration().is_none());
        }
        slot.generation = 7;
        assert!(!slot.publish(Registration {
            request: target,
            generation: 6
        }));
        assert!(slot.registration().is_none());
        let current = Registration {
            request: target,
            generation: 7,
        };
        assert!(slot.publish(current));
        assert!(!slot.publish(Registration {
            request: request(2),
            ..current
        }));
        assert_eq!(slot.registration(), Some(current));
    }

    #[test]
    fn slot_concurrent_publication_has_one_exact_winner() {
        let mut slot = Slot::empty();
        slot.generation = 9;
        let barrier = std::sync::Barrier::new(3);
        let winners = std::thread::scope(|scope| {
            let shared = &slot;
            let barrier = &barrier;
            let first = scope.spawn(move || {
                barrier.wait();
                shared.publish(Registration {
                    request: request(1),
                    generation: 9,
                })
            });
            let second = scope.spawn(move || {
                barrier.wait();
                shared.publish(Registration {
                    request: request(2),
                    generation: 9,
                })
            });
            barrier.wait();
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(winners.0, winners.1);
        let winner = slot.registration().unwrap();
        assert_eq!(winner.request, request(if winners.0 { 1 } else { 2 }));
        assert!(slot.publish(winner));
        assert!(!slot.publish(Registration {
            generation: 10,
            ..winner
        }));
    }

    #[test]
    fn slot_exclusive_replacement_keeps_cells_independent() {
        let mut slots = [const { Slot::empty() }; 2];
        let old = Registration {
            request: request(1),
            generation: 3,
        };
        slots[0].replace(old);
        assert_eq!(slots[0].registration(), Some(old));
        assert_eq!(slots[1].generation, 0);
        assert!(slots[1].registration().is_none());
        let next = Registration {
            request: request(2),
            generation: 4,
        };
        slots[0].replace(next);
        assert_eq!(slots[0].registration(), Some(next));
        assert!(!slots[0].publish(old));
        assert!(slots[1].registration().is_none());
    }
}
