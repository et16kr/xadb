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
    OperationalStatus,
    ResidentStatus,
    ResidentMixed,
    Plan4Genesis,
    Plan4Retention,
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
// The old expansion keeps its exact closed-slot predicate. Only live retention
// admits the original single closed initializer slot.
macro_rules! plan4_closed_invalid {
    (genesis, $directory:ident, $frames:ident) => {
        $directory.closed_index.is_some()
    };
    (retention, $directory:ident, $frames:ident) => {
        $directory.closed_index.is_some_and(|index| {
            index == 0 || index >= $frames.len() || $directory.entries[index].generation == 0
        })
    };
}
// Two fixed modes share registration geometry; accepted payloads and INIT differ.
macro_rules! plan4_directory_validator {
    ($method:ident, $accept:ident, $mode:ident) => {
        fn $method(&self, frames: &[DirtyFrame<'_>]) -> bool {
            if !(4..=MAX_FRAMES).contains(&frames.len())
                || plan4_closed_invalid!($mode, self, frames)
                || self.resident != 0
                || self.eviction_epoch != 0
            {
                return false;
            }
            let Some(allocator) = self.entries[0].registration() else {
                return false;
            };
            if allocator.generation != 1
                || frames[0].registration != Some(allocator)
                || allocator.request.kind != PageKind::Allocation
                || allocator.request.level != 0
                || allocator.request.identity.page.page_no != 0
                || allocator.request.identity.page.incarnation.get() != 1
                || allocator.request.identity.owner.get() != 1
                || allocator.request.identity.owner_generation.get() != 1
            {
                return false;
            }
            self.entries.iter().enumerate().all(|(index, slot)| {
                if index >= frames.len() {
                    return slot.generation == 0 && slot.registration().is_none();
                }
                if index == 0 {
                    return true;
                }
                if frames[index].registration.is_some() {
                    return false;
                }
                let Some(entry) = slot.registration() else {
                    return true;
                };
                entry.generation != 0
                    && self.$accept(entry.request)
                    && entry.request.identity.database == allocator.request.identity.database
                    && entry.request.identity.page.file == allocator.request.identity.page.file
                    && !self.entries[..index]
                        .iter()
                        .filter_map(Slot::registration)
                        .any(|prior| prior.request.same_address(&entry.request))
            })
        }
    };
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
    pub(super) fn operational_status(&self) -> bool {
        matches!(self.mode, Mode::OperationalStatus | Mode::ResidentStatus)
    }
    pub(super) fn resident_status(&self) -> bool {
        self.mode == Mode::ResidentStatus
    }
    pub(super) fn resident_mixed(&self) -> bool {
        self.mode == Mode::ResidentMixed
    }
    pub(super) fn resident_payloads(&self) -> bool {
        matches!(self.mode, Mode::ResidentStatus | Mode::ResidentMixed)
    }
    pub(super) fn operational_payloads(&self) -> bool {
        self.operational_status() || self.resident_mixed()
    }
    pub(super) fn plan4_payloads(&self) -> bool {
        self.mode == Mode::Plan4Genesis
    }
    /// This predicate is deliberately separate from Status/Mixed eligibility.
    pub(super) fn accepts_plan4_payload(&self, request: TargetRequest) -> bool {
        self.plan4_payloads()
            && request.level == 0
            && request.identity.page.incarnation.get() == 1
            && request.identity.owner_generation.get() == 1
            && match request.identity.page.page_no {
                1 => request.kind == PageKind::RetentionRoot && request.identity.owner.get() == 4,
                2..=6 => {
                    request.kind == PageKind::RetentionJobs && request.identity.owner.get() == 4
                }
                7 => request.kind == PageKind::TreeAnchor && request.identity.owner.get() == 5,
                8 => request.kind == PageKind::CatalogLeaf && request.identity.owner.get() == 5,
                _ => false,
            }
    }
    pub(super) fn retention_payloads(&self) -> bool {
        self.mode == Mode::Plan4Retention
    }
    pub(super) fn accepts_retention_payload(&self, request: TargetRequest) -> bool {
        self.retention_payloads()
            && request.level == 0
            && request.identity.owner_generation.get() == 1
            && match request.identity.page.page_no {
                1 => {
                    request.identity.page.incarnation.get() == 1
                        && request.kind == PageKind::RetentionRoot
                        && request.identity.owner.get() == 4
                }
                2..=6 => {
                    request.identity.page.incarnation.get() == 1
                        && request.kind == PageKind::RetentionJobs
                        && request.identity.owner.get() == 4
                }
                7 => {
                    request.identity.page.incarnation.get() == 1
                        && request.kind == PageKind::TreeAnchor
                        && request.identity.owner.get() == 5
                }
                8 => {
                    request.identity.page.incarnation.get() == 1
                        && request.kind == PageKind::CatalogLeaf
                        && request.identity.owner.get() == 5
                }
                9..=63 => {
                    request.identity.page.incarnation.get() >= 2
                        && matches!(
                            (request.kind, request.identity.owner.get()),
                            (PageKind::TransactionStatus, 2) | (PageKind::Undo, 3)
                        )
                }
                _ => false,
            }
    }
    pub(super) fn activate_retention(&mut self, frames: &[DirtyFrame<'_>]) -> bool {
        if !self.plan4_payloads() || !self.valid_plan4(frames) {
            return false;
        }
        self.mode = Mode::Plan4Retention;
        true
    }
    pub(super) fn next_retention_generation(&self, index: usize) -> Option<u64> {
        self.retention_payloads().then_some(())?;
        (index != 0).then_some(())?;
        self.entries.get(index)?.generation.checked_add(1)
    }
    pub(super) fn replace_retention(&mut self, index: usize, registration: Registration) {
        self.entries[index].replace(registration);
    }
    pub(super) fn prepare_retention_slot(&mut self, index: usize, generation: u64) {
        self.entries[index].request.take();
        self.entries[index].generation = generation;
        self.closed_index = Some(index);
    }
    pub(super) fn open_plan4(&mut self, frames: &[DirtyFrame<'_>]) -> bool {
        if self.mode != Mode::Fixed
            || self.closed_index.is_some()
            || !(4..=MAX_FRAMES).contains(&frames.len())
            || !self.valid(frames)
            || self.entries[1..]
                .iter()
                .any(|slot| slot.registration().is_some())
        {
            return false;
        }
        self.mode = Mode::Plan4Genesis;
        self.valid(frames)
    }
    pub(super) fn next_plan4_generation(&self, index: usize) -> Option<u64> {
        self.plan4_payloads().then_some(())?;
        (index != 0).then_some(())?;
        self.entries.get(index)?.generation.checked_add(1)
    }
    pub(super) fn replace_plan4(&mut self, index: usize, registration: Registration) {
        self.entries[index].replace(registration);
    }
    pub(super) fn accepts_payload(&self, request: TargetRequest) -> bool {
        request.level == 0
            && request.identity.owner_generation.get() == 1
            && ((request.kind == PageKind::TransactionStatus && request.identity.owner.get() == 2)
                || (self.resident_mixed()
                    && request.kind == PageKind::Undo
                    && request.identity.owner.get() == 3))
    }
    pub(super) fn open_resident_mixed(&mut self) {
        self.mode = Mode::ResidentMixed;
        self.close_status_slot();
    }
    pub(super) fn init_generations(&self, frames: &[DirtyFrame<'_>]) -> Option<[u64; 2]> {
        let index = self.closed_index?;
        if !self.valid(frames) {
            return None;
        }
        Some([
            self.resolve(frames, 0)?.generation,
            self.entries[index].generation,
        ])
    }
    pub(super) fn open_resident_status(&mut self) {
        self.mode = Mode::ResidentStatus;
        self.close_status_slot();
    }
    pub(super) fn next_resident_generation(&self, index: usize) -> Option<u64> {
        self.resident_payloads().then_some(())?;
        self.entries.get(index)?.generation.checked_add(1)
    }
    pub(super) fn prepare_resident_slot(&mut self, index: usize, generation: u64) {
        self.entries[index].request.take();
        self.entries[index].generation = generation;
        self.closed_index = Some(index);
    }
    pub(super) fn replace_resident_status(&mut self, index: usize, registration: Registration) {
        self.entries[index].replace(registration);
    }
    pub(super) fn address_present(&self, request: TargetRequest) -> bool {
        self.entries
            .iter()
            .filter_map(Slot::registration)
            .any(|entry| entry.request.same_address(&request))
    }
    pub(super) fn enable_status_operation(&mut self, frames: &[DirtyFrame<'_>]) -> bool {
        if self.mode != Mode::Fixed || self.closed_index != Some(1) || !self.valid(frames) {
            return false;
        }
        self.mode = Mode::OperationalStatus;
        true
    }
    pub(super) fn next_status_generation(&self) -> Option<u64> {
        self.operational_status().then_some(())?;
        self.entries[1].generation.checked_add(1)
    }
    #[cfg(test)]
    pub(super) fn seed_status_generation(&mut self, generation: u64) {
        assert!(self.operational_status() && generation != 0);
        self.entries[1].generation = generation;
    }
    #[cfg(test)]
    pub(super) fn seed_resident_generation(&mut self, index: usize, generation: u64) {
        assert!(self.resident_payloads() && index > 0 && generation != 0);
        self.entries[index].generation = generation;
    }
    pub(super) fn prepare_next_status(&mut self, generation: u64) {
        // Exclusive caller already checked the next generation and all claims.
        self.entries[1].request.take();
        self.entries[1].generation = generation;
    }
    pub(super) fn replace_status(&mut self, registration: Registration) {
        // The exclusive Status loader checked mode, next generation and origin.
        self.entries[1].replace(registration);
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
        if self.closed_index == Some(index)
            || ((self.resident_payloads() || self.plan4_payloads() || self.retention_payloads())
                && index != 0)
        {
            if frame.registration.is_some() {
                return None;
            }
            original.filter(|value| value.generation != 0)
        } else {
            original.filter(|value| {
                value.generation != 0
                    && match self.mode {
                        Mode::Fixed
                        | Mode::OperationalStatus
                        | Mode::ResidentStatus
                        | Mode::ResidentMixed
                        | Mode::Plan4Genesis
                        | Mode::Plan4Retention => frame.registration == Some(*value),
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
        if self.plan4_payloads() {
            return self.valid_plan4(frames);
        }
        if self.retention_payloads() {
            return self.valid_retention(frames);
        }
        if self.resident_payloads() {
            let Some(selected) = self.closed_index else {
                return false;
            };
            if !(2..=MAX_FRAMES).contains(&frames.len())
                || !(1..frames.len()).contains(&selected)
                || self.entries[selected].generation == 0
                || self.resident != 0
            {
                return false;
            }
            let Some(allocator) = self.entries[0].registration() else {
                return false;
            };
            if allocator.generation != 1
                || frames[0].registration != Some(allocator)
                || allocator.request.kind != PageKind::Allocation
                || allocator.request.level != 0
                || allocator.request.identity.page.page_no != 0
            {
                return false;
            }
            return self.entries.iter().enumerate().all(|(index, slot)| {
                if index >= frames.len() {
                    return slot.generation == 0 && slot.registration().is_none();
                }
                if index == 0 {
                    return true;
                }
                if frames[index].registration.is_some() {
                    return false;
                }
                let Some(entry) = slot.registration() else {
                    return true;
                };
                entry.generation != 0
                    && self.accepts_payload(entry.request)
                    && entry.request.identity.database == allocator.request.identity.database
                    && entry.request.identity.page.file == allocator.request.identity.page.file
                    && (1..64).contains(&entry.request.identity.page.page_no)
                    && entry.request.identity.page.incarnation.get() >= 2
                    && !self.entries[..index]
                        .iter()
                        .filter_map(Slot::registration)
                        .any(|prior| prior.request.same_address(&entry.request))
            });
        }
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
                || self.entries[index].generation == 0
                || (!self.operational_status() && self.entries[index].generation != 1)
                || frames[index].registration.is_some()
            {
                return false;
            }
            if let Some(published) = self.published() {
                let Some(allocator) = self.resolve(frames, 0) else {
                    return false;
                };
                if (!self.operational_status() && published.generation != 1)
                    || published.request.kind != PageKind::TransactionStatus
                    || published.request.level != 0
                    || published.request.identity.database != allocator.request.identity.database
                    || published.request.identity.page.file != allocator.request.identity.page.file
                    || if self.operational_status() {
                        !(1..64).contains(&published.request.identity.page.page_no)
                    } else {
                        published.request.identity.page.page_no != 1
                    }
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

    plan4_directory_validator!(valid_plan4, accepts_plan4_payload, genesis);
    plan4_directory_validator!(valid_retention, accepts_retention_payload, retention);
}

pub(super) fn fits() -> bool {
    use std::mem::size_of;
    size_of::<Slot>() <= size_of::<Option<Registration>>() + 8
        && size_of::<RegistrationDirectory>() <= MAX_FRAMES * size_of::<Slot>() + 48
}

pub(super) fn plan4_mode_bytes() -> usize {
    std::mem::size_of::<Mode>()
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
