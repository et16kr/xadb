//! One bounded directory with a publication cell per caller frame.
//! The original PoolRegistry leaf owns this directory with pins and DPT.
//! Exclusive handoff or a checked per-target LOAD claim may replace a cell.
//! INIT retains its preclaimed generations and joint pair publication gate.

use super::{DirtyFrame, MAX_FRAMES, PageKind, Registration, TargetRequest};
use std::sync::atomic::{AtomicBool, Ordering};

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
    request: Option<TargetRequest>,
}

/// Observation-only closure for the retention directory, also a conservative
/// bound for its resolve/address_present leaves. Counts returned and extracted
/// registrations as well as allocator/current/prior in the nested predicate.
#[cfg(test)]
pub(super) fn retention_source_observation() -> usize {
    use std::mem::size_of;
    4 * size_of::<Registration>()
        + 4 * size_of::<Option<Registration>>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<Option<TargetRequest>>()
        + 3 * size_of::<Option<[usize; 2]>>()
        + 2 * size_of::<[usize; 2]>()
        + 2 * size_of::<std::array::IntoIter<usize, 2>>()
        + 2 * size_of::<std::iter::Enumerate<std::slice::Iter<'_, Slot>>>()
        + 2 * size_of::<std::slice::Iter<'_, Slot>>()
        + 4 * size_of::<(usize, &Slot)>()
}
impl Slot {
    const fn empty() -> Self {
        Self {
            generation: 0,
            request: None,
        }
    }

    fn registration(&self) -> Option<Registration> {
        self.request.map(|request| Registration {
            request,
            generation: self.generation,
        })
    }

    // The original registry leaf supplies exclusive access. Generation and
    // exact prior identity are checked before changing the publication cell.
    fn publish(&mut self, registration: Registration) -> bool {
        if self.generation == 0 || self.generation != registration.generation {
            return false;
        }
        match self.request {
            Some(request) => request == registration.request,
            None => {
                self.request = Some(registration.request);
                true
            }
        }
    }

    fn replace(&mut self, registration: Registration) {
        self.generation = registration.generation;
        // Exclusive access makes this assignment infallible.
        self.request = Some(registration.request);
    }
}

pub(super) struct RegistrationDirectory {
    mode: Mode,
    entries: [Slot; MAX_FRAMES],
    closed_index: Option<usize>,
    // A pair is hidden until both publication cells and the whole Forward
    // installation are complete. The gate, not either cell, publishes it.
    closed_pair: Option<[u8; 2]>,
    pair_published: AtomicBool,
    resident: u64,
    pub(super) eviction_epoch: u64,
}

/// Exact pre-LOAD directory layout for historical source census only. These
/// types are never constructed; current admission always uses the live type.
#[allow(dead_code)]
struct PreLoadSlot {
    generation: u64,
    request: std::sync::OnceLock<TargetRequest>,
}
#[allow(dead_code)]
pub(super) struct PreLoadDirectory {
    mode: Mode,
    entries: [PreLoadSlot; MAX_FRAMES],
    closed_index: Option<usize>,
    closed_pair: Option<[u8; 2]>,
    pair_published: AtomicBool,
    resident: u64,
    eviction_epoch: u64,
}
// The old expansion keeps its exact closed-slot predicate. Only live retention
// admits the original single closed initializer slot.
macro_rules! plan4_closed_invalid {
    (genesis, $directory:ident, $frames:ident) => {
        $directory.closed_index.is_some() || $directory.closed_pair.is_some()
    };
    (retention, $directory:ident, $frames:ident) => {
        !$directory.closed_retention_valid($frames)
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
            closed_pair: None,
            pair_published: AtomicBool::new(false),
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
    pub(super) fn closed_pair(&self) -> Option<[usize; 2]> {
        self.closed_pair.map(|indices| indices.map(usize::from))
    }
    pub(super) fn pair_published(&self) -> bool {
        self.closed_pair.is_some() && self.pair_published.load(Ordering::Acquire)
    }
    fn closed_retention_valid(&self, frames: &[DirtyFrame<'_>]) -> bool {
        let valid = |index: usize| {
            index != 0 && index < frames.len() && self.entries[index].generation != 0
        };
        if let Some(pair) = self.closed_pair() {
            self.closed_index.is_none()
                && pair[0] != pair[1]
                && pair.into_iter().all(valid)
                && (!self.pair_published()
                    || pair
                        .into_iter()
                        .all(|index| self.entries[index].registration().is_some()))
        } else {
            !self.pair_published.load(Ordering::Relaxed) && self.closed_index.is_none_or(valid)
        }
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
                            (PageKind::TransactionStatus, 2)
                                | (PageKind::Undo, 3)
                                | (PageKind::TreeAnchor | PageKind::RowLeaf, 6..)
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

    /// Exclusive caller has checked both clean frames and next generations.
    /// This consumes generations even if the later semantic reserve is cancelled.
    pub(super) fn prepare_retention_pair(&mut self, indices: [usize; 2], generations: [u64; 2]) {
        debug_assert!(self.closed_pair.is_none());
        self.closed_index = None;
        self.pair_published.store(false, Ordering::Relaxed);
        for (index, generation) in indices.into_iter().zip(generations) {
            self.entries[index].request.take();
            self.entries[index].generation = generation;
        }
        self.closed_pair =
            Some(indices.map(|index| u8::try_from(index).expect("bounded pool index")));
    }

    pub(super) fn pair_generations(&self) -> Option<[u64; 2]> {
        Some(
            self.closed_pair()?
                .map(|index| self.entries[index].generation),
        )
    }

    /// No lookup can observe the first cell while the second is unpublished.
    /// A failed second publication leaves both hidden; the installer fences.
    pub(super) fn publish_pair(
        &mut self,
        registrations: [Registration; 2],
        #[cfg(test)] pool: &super::SharedPool<'_, '_, '_, '_>,
    ) -> bool {
        let Some(indices) = self.closed_pair() else {
            return false;
        };
        if self.pair_published()
            || registrations[0]
                .request
                .same_address(&registrations[1].request)
        {
            return false;
        }
        for (index, registration) in indices.into_iter().zip(registrations) {
            if self.entries[index].generation != registration.generation
                || self.entries[index].registration().is_some()
            {
                return false;
            }
        }
        #[allow(
            clippy::unused_enumerate_index,
            reason = "the ordinal selects the first-registration fault cut in test builds"
        )]
        for (_at, (index, registration)) in indices.into_iter().zip(registrations).enumerate() {
            if !self.entries[index].publish(registration) {
                return false;
            }
            #[cfg(test)]
            if _at == 0 {
                super::init::test_support::hit(
                    pool,
                    super::init::test_support::InitPoint::PairFirstRegistered,
                );
            }
        }
        #[cfg(test)]
        super::init::test_support::hit(
            pool,
            super::init::test_support::InitPoint::PairBothRegisteredBeforeGate,
        );
        self.pair_published.store(true, Ordering::Release);
        true
    }

    /// Original bound is exclusive; all pins/claims and selected DWB have been
    /// resolved by the sealed consumer. Generation tombstones remain consumed.
    pub(super) fn clear_retention_pair(&mut self, indices: [usize; 2]) {
        for index in indices {
            self.entries[index].request.take();
        }
        self.closed_pair = None;
        self.pair_published.store(false, Ordering::Relaxed);
    }

    /// After a complete Forward the registrations remain ordinary residents.
    pub(super) fn finish_retention_pair(&mut self) {
        debug_assert!(self.pair_published());
        self.closed_pair = None;
        self.pair_published.store(false, Ordering::Relaxed);
    }

    pub(super) fn retire_retention(&mut self, index: usize) {
        self.entries[index].request.take();
        if self.closed_index == Some(index) {
            self.closed_index = None;
        }
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
    pub(super) fn publish(&mut self, registration: Registration) -> bool {
        self.closed_index
            .is_some_and(|index| self.entries[index].publish(registration))
    }

    pub(super) fn resolve(&self, frames: &[DirtyFrame<'_>], index: usize) -> Option<Registration> {
        let frame = frames.get(index)?;
        if self.closed_pair().is_some_and(|pair| pair.contains(&index)) && !self.pair_published() {
            return None;
        }
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
        if self.closed_pair.is_some() || self.pair_published.load(Ordering::Relaxed) {
            return false;
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
    fn original_registry_serializes_exact_publication() {
        use super::super::{PageConcurrency, SessionFence, SharedPool};
        use foundation::identity::{Generation, ObjectId};
        use std::sync::atomic::AtomicU8;

        // Metadata-only publication uses the actual original registry leaf.
        // It creates no INIT/WAL/image installation or semantic capability.
        let mut backing = [[0; super::super::PAGE_BYTES]; 2];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut allocator = request(0);
        allocator.kind = PageKind::Allocation;
        frames[0].register(allocator).unwrap();
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let binding = AtomicU8::new(2);
        let pool = SharedPool::new(&core, &fence, &binding, &frames).unwrap();
        pool.directory_edit(RegistrationDirectory::close_status_slot)
            .unwrap();
        let mut target = request(1);
        target.kind = PageKind::TransactionStatus;
        target.identity.owner = ObjectId::new(2).unwrap();
        target.identity.owner_generation = Generation::new(1).unwrap();
        target.identity.page.incarnation = Generation::new(2).unwrap();
        let first = Registration {
            request: target,
            generation: 1,
        };
        target.identity.page.incarnation = Generation::new(3).unwrap();
        let second = Registration {
            request: target,
            generation: 1,
        };
        let barrier = std::sync::Barrier::new(3);
        let winners = std::thread::scope(|scope| {
            let shared = &pool;
            let barrier = &barrier;
            let first = scope.spawn(move || {
                barrier.wait();
                shared.entry_registry().unwrap().directory.publish(first)
            });
            let second = scope.spawn(move || {
                barrier.wait();
                shared.entry_registry().unwrap().directory.publish(second)
            });
            barrier.wait();
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(winners.0, winners.1);
        let mut state = pool.entry_registry().unwrap();
        let winner = state.directory.published().unwrap();
        assert_eq!(winner, if winners.0 { first } else { second });
        assert!(state.directory.publish(winner));
        assert!(!state.directory.publish(Registration {
            generation: 2,
            ..winner
        }));
        assert_eq!(state.directory.published(), Some(winner));
        assert_eq!(state.total, 0);
        assert!(state.init_claim.is_none());
        assert!(!fence.is_fenced());
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
