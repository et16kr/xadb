//! Authoritative dirty frames and bounded pin/content ownership.
//! Bounded capture/finalize consumes stage-issued WAL and retired-batch permits.
//! Allocator residency consumes sealed selected-home permits; general page load,
//! replacement, Commit and raw semantic-stage authority remain separate.
#![allow(dead_code)] // The separately reviewed allocator consumer is being connected.

use super::{
    page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn},
    session_fence::SessionFence,
    wal_io::{DirtyInstallPermit, WalRequirement},
};
use crate::transaction::concurrency::{
    self, ContentAdmission, Owner, PageConcurrency, VersionEditGuard,
};
use foundation::error::Effect;
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{
        Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError,
        atomic::AtomicU8,
    },
};

mod commit;
mod plan4_residency;
#[allow(unused_imports)] // The actual terminal producer will consume these opaque carriers.
pub(in crate::storage) use commit::{
    CommitInstallTargets, CommitStageTargets, PendingCommitTargets,
};
mod recovered_plan4;
mod registration;
mod retention_residency;
pub(in crate::storage) use plan4_residency::source_census as plan4_core_census;
#[cfg(test)]
pub(in crate::storage) use plan4_residency::test_support as plan4_load_test_support;
pub(in crate::storage) use retention_residency::source_census as retention_core_census;
pub(in crate::storage) use status_residency::StatusLoadSnapshot as Plan4LoadSnapshot;
mod residency;
mod status_pressure;
mod status_residency;
#[cfg(test)]
pub(in crate::storage) use residency::test_support as residency_test_support;
pub(in crate::storage) use residency::{AllocatorLoadClaim, ReloadSnapshot};
#[cfg(test)]
pub(in crate::storage) use status_pressure::test_support as pressure_test_support;
pub(in crate::storage) use status_pressure::{StatusCandidate, StatusCandidateKind};
pub(in crate::storage) use status_residency::StatusLoadSnapshot;
#[cfg(test)]
pub(in crate::storage) use status_residency::test_support as status_load_test_support;
#[cfg(test)]
mod registration_tests;
#[cfg(test)]
mod target_set_tests;
use registration::RegistrationDirectory;

#[path = "dirty_flush.rs"]
mod flush;
pub(in crate::storage) use flush::{CapturedPage, FinalizedPage};
#[path = "dirty_init.rs"]
mod init;
pub(in crate::storage) use init::{InitClaimKey, InitWriteTargets};

pub(crate) const MAX_FRAMES: usize = 64;
/// Prior closed-INIT metadata allowance plus at most eight bytes per slot for
/// shared publication. Component and whole-operation limits remain unchanged.
pub(in crate::storage) const INIT_POOL_DELTA_LIMIT: usize = 512 + MAX_FRAMES * 8;
/// Source-value scratch for Copy registration snapshots and checked target views.
pub(in crate::storage) const CALL_BYTES: usize = 3072;
pub(crate) const MAX_TARGETS: usize = 2;
const MAX_FRAME_PINS: u16 = 34;
const MAX_PINS: u16 = 68;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Fenced,
    Poisoned,
    Invariant,
    BindingMismatch,
    Shape,
    Capacity,
    MissingTarget,
    DuplicateTarget,
    Busy,
    Stale,
    CounterExhausted,
    AlreadyInitialized,
    Concurrency(concurrency::Cause),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyError {
    pub(crate) cause: Cause,
    pub(crate) effect: Effect,
}
pub(crate) type Result<T> = std::result::Result<T, DirtyError>;

fn unchanged(cause: Cause) -> DirtyError {
    DirtyError {
        cause,
        effect: Effect::Unchanged,
    }
}
impl From<concurrency::ConcurrencyError> for DirtyError {
    fn from(error: concurrency::ConcurrencyError) -> Self {
        let cause = match error.cause {
            concurrency::Cause::Fenced => Cause::Fenced,
            concurrency::Cause::Poisoned => Cause::Poisoned,
            concurrency::Cause::Invariant => Cause::Invariant,
            concurrency::Cause::Busy => Cause::Busy,
            concurrency::Cause::Capacity => Cause::Capacity,
            cause => Cause::Concurrency(cause),
        };
        Self {
            cause,
            effect: error.effect,
        }
    }
}

// Continue all cleanup, preserving the first fatal rather than its later fence.
fn combine(first: Result<()>, next: Result<()>) -> Result<()> {
    match (first, next) {
        (Err(error), _) if error.effect != Effect::Unchanged => Err(error),
        (_, Err(error)) if error.effect != Effect::Unchanged => Err(error),
        (Err(error), _) => Err(error),
        (_, result) => result,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TargetRequest {
    pub(crate) identity: PageIdentity,
    pub(crate) kind: PageKind,
    pub(crate) level: u8,
}
impl TargetRequest {
    fn same_address(&self, other: &Self) -> bool {
        self.identity.database == other.identity.database
            && self.identity.page.file == other.identity.page.file
            && self.identity.page.page_no == other.identity.page.page_no
    }
    fn rank(&self) -> (u64, u64, u64) {
        let mut bytes = [0; 24];
        // The fixed PageRef encoding is infallible for this exact-size output.
        self.identity
            .page
            .encode(&mut bytes)
            .expect("fixed PageRef output");
        (
            u64::from_le_bytes(bytes[..8].try_into().expect("fixed file id")),
            self.identity.page.page_no,
            self.identity.page.incarnation.get(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Registration {
    request: TargetRequest,
    generation: u64,
}
struct FrameContent<'d> {
    bytes: &'d mut [u8; PAGE_BYTES],
    page_lsn: PageLsn,
    modification_counter: u64,
    rec_lsn: Option<PageLsn>,
    dirty: bool,
}
impl FrameContent<'_> {
    fn fresh(&self) -> bool {
        !self.dirty
            && self.page_lsn.0 == 0
            && self.modification_counter == 0
            && self.rec_lsn.is_none()
    }
    fn valid(&self) -> bool {
        if self.dirty {
            self.modification_counter != 0
                && self
                    .rec_lsn
                    .is_some_and(|lsn| lsn.0 != 0 && lsn.0 <= self.page_lsn.0)
        } else {
            self.rec_lsn.is_none()
                && ((self.modification_counter == 0 && self.page_lsn.0 == 0)
                    || (self.modification_counter != 0 && self.page_lsn.0 != 0))
        }
    }
}

/// The page remains caller-owned; constructing this wrapper never copies it.
pub(crate) struct DirtyFrame<'d> {
    registration: Option<Registration>,
    content: RwLock<FrameContent<'d>>,
}
impl<'d> DirtyFrame<'d> {
    pub(crate) fn new(bytes: &'d mut [u8; PAGE_BYTES]) -> Self {
        Self {
            registration: None,
            content: RwLock::new(FrameContent {
                bytes,
                page_lsn: PageLsn(0),
                modification_counter: 0,
                rec_lsn: None,
                dirty: false,
            }),
        }
    }

    /// Exclusive binding preflight. Never repairs a previous registration.
    pub(in crate::storage) fn check_empty(&mut self) -> Result<()> {
        if self.registration.is_some() {
            return Err(unchanged(Cause::Invariant));
        }
        let content = self
            .content
            .get_mut()
            .map_err(|_| unchanged(Cause::Poisoned))?;
        if !content.fresh() {
            return Err(unchanged(Cause::Invariant));
        }
        Ok(())
    }

    /// Only the claimed storage binder may perform initial readback here.
    pub(in crate::storage) fn initial_payload(&mut self) -> Result<&mut [u8; PAGE_BYTES]> {
        self.check_empty()?;
        Ok(self
            .content
            .get_mut()
            .map_err(|_| unchanged(Cause::Poisoned))?
            .bytes)
    }

    pub(in crate::storage) fn register(&mut self, request: TargetRequest) -> Result<()> {
        self.check_empty()?;
        self.registration = Some(Registration {
            request,
            generation: 1,
        });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameStamp {
    pub(crate) identity: PageIdentity,
    pub(crate) generation: u64,
    pub(crate) page_lsn: PageLsn,
    pub(crate) modification_counter: u64,
    pub(crate) rec_lsn: Option<PageLsn>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyEntry {
    pub(crate) identity: PageIdentity,
    pub(crate) generation: u64,
    pub(crate) rec_lsn: PageLsn,
    pub(crate) modification_counter: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyObservation {
    pub(crate) frame: FrameStamp,
    pub(crate) entry: Option<DirtyEntry>,
}
struct PinAccounting {
    counts: [u16; MAX_FRAMES],
    total: u16,
    init_claim: Option<init::InitClaimSlot>,
}

// The receipt and DPT entry are installed/removed together under content→registry.
// Keeping the authentic receipt here outlives the writer's completed obligation.
struct DirtyRegistry<'s> {
    entries: [Option<DirtyEntry>; MAX_FRAMES],
    requirements: [Option<WalRequirement<'s>>; MAX_FRAMES],
    claim: Option<flush::Claim>,
    last_attempt: u64,
}

/// Bounded directory and leaf registries; the exact core cannot be caller-selected.
pub(crate) struct SharedPool<'c, 's, 'p, 'd> {
    frames: &'p [DirtyFrame<'d>],
    directory: RegistrationDirectory,
    pins: Mutex<PinAccounting>,
    dpt: Mutex<DirtyRegistry<'s>>,
    core: &'c PageConcurrency<'s>,
    fence: &'s SessionFence,
    binding: &'s AtomicU8,
    stamp_anchor: u8,
    #[cfg(test)]
    fence_pause: Mutex<Option<test_support::FencePause>>,
    #[cfg(test)]
    release_pause: Mutex<Option<test_support::FencePause>>,
    #[cfg(test)]
    flush_pause: Mutex<Option<(test_support::FlushPoint, test_support::FencePause)>>,
    #[cfg(test)]
    init_pause: Mutex<Option<(init::test_support::InitPoint, test_support::FencePause)>>,
}
impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn new(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>],
    ) -> Result<Self> {
        if fence.is_fenced() {
            return Err(DirtyError {
                cause: Cause::Fenced,
                effect: Effect::CallerMustResolve,
            });
        }
        if !core.uses_fence(fence) {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if frames.is_empty() || frames.len() > MAX_FRAMES {
            return Err(unchanged(Cause::Shape));
        }
        let directory = RegistrationDirectory::from_bootstrap(frames).ok_or_else(|| {
            fence.fence();
            DirtyError {
                cause: Cause::Invariant,
                effect: Effect::CallerMustResolve,
            }
        })?;
        Ok(Self {
            frames,
            directory,
            pins: Mutex::new(PinAccounting {
                counts: [0; MAX_FRAMES],
                total: 0,
                init_claim: None,
            }),
            dpt: Mutex::new(DirtyRegistry {
                entries: [None; MAX_FRAMES],
                requirements: [None; MAX_FRAMES],
                claim: None,
                last_attempt: 0,
            }),
            core,
            fence,
            binding,
            stamp_anchor: 0,
            #[cfg(test)]
            fence_pause: Mutex::new(None),
            #[cfg(test)]
            release_pause: Mutex::new(None),
            #[cfg(test)]
            flush_pause: Mutex::new(None),
            #[cfg(test)]
            init_pause: Mutex::new(None),
        })
    }

    pub(in crate::storage) fn bound_core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }

    /// Read-only origin comparison; this cannot create a binding or a claim.
    pub(in crate::storage) fn uses_storage_origin(
        &self,
        binding: &AtomicU8,
        fence: &SessionFence,
    ) -> bool {
        std::ptr::eq(self.binding, binding) && std::ptr::eq(self.fence, fence)
    }

    fn fatal(&self, cause: Cause) -> DirtyError {
        self.fence.fence();
        DirtyError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
    fn health(&self) -> Result<()> {
        if self.fence.is_fenced() {
            Err(self.fatal(Cause::Fenced))
        } else {
            Ok(())
        }
    }
    fn fence_unwind(&self) {
        if std::thread::panicking() {
            self.fence.fence();
        }
    }
    fn pins_valid(&self, pins: &PinAccounting) -> bool {
        if !self.registrations_valid() || !self.init_pins_valid(pins) {
            return false;
        }
        let mut sum = 0u16;
        for (index, count) in pins.counts.iter().enumerate() {
            let initialized = self.resolve_registration(index).is_some();
            let initializer_pin = pins.init_claim.as_ref().is_some_and(|slot| {
                slot.initializer_pins_held && self.directory.closed_index() == Some(index)
            });
            if *count > MAX_FRAME_PINS || (!initialized && *count != u16::from(initializer_pin)) {
                return false;
            }
            let Some(next) = sum.checked_add(*count) else {
                return false;
            };
            sum = next;
        }
        sum == pins.total && sum <= MAX_PINS
    }
    fn entry_pins(&self) -> Result<MutexGuard<'_, PinAccounting>> {
        self.health()?;
        let pins = self.pins.lock().map_err(|_| self.fatal(Cause::Poisoned))?;
        self.health()?;
        if !self.pins_valid(&pins) {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(pins)
    }
    fn dpt_valid(&self, dpt: &DirtyRegistry<'s>) -> bool {
        self.registrations_valid()
            && dpt
                .claim
                .as_ref()
                .is_none_or(|claim| claim.valid(self, dpt.last_attempt))
            && dpt.entries.iter().enumerate().all(|(index, entry)| {
                match (entry, dpt.requirements[index]) {
                    (None, None) => true,
                    (Some(entry), Some(requirement)) => self
                        .resolve_registration(index)
                        .is_some_and(|registration| {
                            entry.identity == registration.request.identity
                                && entry.generation == registration.generation
                                && entry.rec_lsn.0 != 0
                                && entry.modification_counter != 0
                                && requirement.uses_fence(self.fence)
                                && requirement.location().lsn >= entry.rec_lsn.0
                        }),
                    _ => false,
                }
            })
    }
    fn entry_dpt(&self) -> Result<MutexGuard<'_, DirtyRegistry<'s>>> {
        self.health()?;
        let dpt = self.dpt.lock().map_err(|_| self.fatal(Cause::Poisoned))?;
        self.health()?;
        if !self.dpt_valid(&dpt) {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(dpt)
    }
    /// Copy snapshot only; callers need a checked pin for content authority.
    fn resolve_registration(&self, index: usize) -> Option<Registration> {
        self.directory.resolve(self.frames, index)
    }
    fn registrations_valid(&self) -> bool {
        self.directory.valid(self.frames)
    }
    fn registration(&self, index: usize) -> Result<Registration> {
        self.resolve_registration(index)
            .ok_or_else(|| self.fatal(Cause::Invariant))
    }
    /// A copied request grants no pin, content or mutation authority.
    pub(in crate::storage) fn target_request(&self, index: usize) -> Result<TargetRequest> {
        self.health()?;
        if !self.registrations_valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(self.registration(index)?.request)
    }
    fn stamp(&self, index: usize, content: &FrameContent<'_>) -> Result<FrameStamp> {
        let registration = self.registration(index)?;
        if !content.valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(FrameStamp {
            identity: registration.request.identity,
            generation: registration.generation,
            page_lsn: content.page_lsn,
            modification_counter: content.modification_counter,
            rec_lsn: content.rec_lsn,
        })
    }
    fn pair(&self, index: usize, content: &FrameContent<'_>) -> Result<DirtyObservation> {
        let frame = self.stamp(index, content)?;
        let dpt = self.entry_dpt()?;
        let expected = frame.rec_lsn.map(|rec_lsn| DirtyEntry {
            identity: frame.identity,
            generation: frame.generation,
            rec_lsn,
            modification_counter: frame.modification_counter,
        });
        if dpt.entries[index] != expected
            || dpt.requirements[index].map(|value| value.location().lsn)
                != frame.rec_lsn.map(|_| frame.page_lsn.0)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(DirtyObservation {
            frame,
            entry: expected,
        })
    }

    /// Existing one/two-target consumers retain the same carrier layout.
    pub(crate) fn reserve_targets<'a, 'b>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        requests: &[TargetRequest],
    ) -> Result<PinnedTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        self.reserve_target_capacity::<MAX_TARGETS>(owner, requests)
    }

    /// Reserve the exact whole set before any content acquisition. N is a
    /// storage capacity, never permission to exceed the original pin quota.
    pub(crate) fn reserve_target_set<'a, 'b, const N: usize>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        requests: &[TargetRequest; N],
    ) -> Result<PinnedTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        self.reserve_target_capacity::<N>(owner, requests)
    }

    fn reserve_target_capacity<'a, 'b, const N: usize>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        requests: &[TargetRequest],
    ) -> Result<PinnedTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        const { assert!(N >= 1 && N <= MAX_FRAMES) }; // target-set capacity assertion
        self.health()?;
        // Health precedes identity; a different core's mode is not ours to check.
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if !self.registrations_valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        if requests.is_empty() || requests.len() > N {
            return Err(unchanged(Cause::Shape));
        }
        for (at, request) in requests.iter().enumerate() {
            if requests[..at]
                .iter()
                .any(|prior| prior.same_address(request))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        let mut indices = [0; N];
        let mut generations = [0; N];
        {
            let mut pins = self.entry_pins()?;
            for (at, request) in requests.iter().enumerate() {
                let (index, registration) = self
                    .directory
                    .lookup(self.frames, *request)
                    .ok_or_else(|| unchanged(Cause::MissingTarget))?;
                // Bounded insertion sort needs only the token arrays. The
                // directory cannot change while this shared pool is borrowed.
                let mut position = at;
                while position != 0
                    && self.registration(indices[position - 1])?.request.rank() > request.rank()
                {
                    indices[position] = indices[position - 1];
                    generations[position] = generations[position - 1];
                    position -= 1;
                }
                indices[position] = index;
                generations[position] = registration.generation;
            }
            if pins.total + requests.len() as u16 > MAX_PINS
                || indices[..requests.len()]
                    .iter()
                    .any(|index| pins.counts[*index] == MAX_FRAME_PINS)
            {
                return Err(unchanged(Cause::Capacity));
            }
            for index in &indices[..requests.len()] {
                pins.counts[*index] += 1;
            }
            pins.total += requests.len() as u16;
        }
        Ok(PinnedTargets {
            owner: Some(owner),
            pins: Some(PinSet {
                pool: self,
                indices,
                generations,
                len: requests.len(),
                active: true,
                _thread: PhantomData,
            }),
        })
    }

    pub(in crate::storage) fn has_pins(&self) -> Result<bool> {
        let pins = self.pins.lock().map_err(|_| self.fatal(Cause::Poisoned))?;
        if !self.pins_valid(&pins) {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(pins.total != 0)
    }

    /// Inspect retained leaf obligations before an exclusive owner handoff.
    /// The caller must exclude new pool users for the entire transition; this
    /// observation itself is not an admission or pin capability. Dirty entries
    /// and their WAL requirements may remain, but no pin/INIT/flush claim may.
    pub(in crate::storage) fn handoff_quiescent(&self) -> Result<bool> {
        {
            let pins = self.entry_pins()?;
            if pins.total != 0 || pins.init_claim.is_some() {
                return Ok(false);
            }
        }
        Ok(self.entry_dpt()?.claim.is_none())
    }
}

struct PinSet<'b, 'c, 's, 'p, 'd, const N: usize = MAX_TARGETS> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    indices: [usize; N],
    generations: [u64; N],
    len: usize,
    active: bool,
    _thread: PhantomData<Rc<()>>,
}
impl<const N: usize> PinSet<'_, '_, '_, '_, '_, N> {
    fn registration(&self, at: usize) -> Result<Registration> {
        if at >= self.len {
            return Err(unchanged(Cause::Shape));
        }
        if !self.pool.registrations_valid() {
            return Err(self.pool.fatal(Cause::Invariant));
        }
        let expected = self.generations[at];
        let actual = self.pool.registration(self.indices[at])?;
        if expected == 0 || actual.generation != expected {
            return Err(self.pool.fatal(Cause::Invariant));
        }
        Ok(actual)
    }
    fn release(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        self.pool.fence_unwind();
        let (mut pins, poison) = match self.pool.pins.lock() {
            Ok(pins) => (pins, None),
            Err(error) => (error.into_inner(), Some(self.pool.fatal(Cause::Poisoned))),
        };
        let result = (|| {
            if !self.pool.pins_valid(&pins) || !(1..=N).contains(&self.len) {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            for (at, index) in self.indices[..self.len].iter().enumerate() {
                self.registration(at)?;
                if pins.counts[*index] == 0 {
                    return Err(self.pool.fatal(Cause::Invariant));
                }
            }
            for at in 0..self.len {
                if self.indices[..at].contains(&self.indices[at]) {
                    return Err(self.pool.fatal(Cause::Invariant));
                }
            }
            let total = pins
                .total
                .checked_sub(self.len as u16)
                .ok_or_else(|| self.pool.fatal(Cause::Invariant))?;
            for index in &self.indices[..self.len] {
                pins.counts[*index] -= 1;
            }
            pins.total = total;
            Ok(())
        })();
        combine(poison.map_or(Ok(()), Err), result)
    }
}
impl<const N: usize> Drop for PinSet<'_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(crate) struct PinnedTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize = MAX_TARGETS> {
    owner: Option<&'a mut Owner<'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd, N>>,
}
impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> PinnedTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    pub(crate) fn try_read(mut self) -> Result<ReadTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        let mut pins = self.pins.take().expect("owned pin set");
        let owner = self.owner.take().expect("owned idle owner");
        let admission = match owner.try_content() {
            Ok(admission) => admission,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let mut result = ReadTargets {
            guards: [const { None }; N],
            admission: Some(admission),
            pins: Some(pins),
        };
        let pool = result.pins.as_ref().expect("owned pins").pool;
        for at in 0..result.len() {
            let index = result.pins.as_ref().expect("owned pins").indices[at];
            match pool.frames[index].content.try_read() {
                Ok(guard) => result.guards[at] = Some(guard),
                Err(TryLockError::WouldBlock) => return result.reject(unchanged(Cause::Busy)),
                Err(TryLockError::Poisoned(error)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(error.into_inner());
                    return result.reject(failure);
                }
            }
            if let Err(error) = result
                .pins
                .as_ref()
                .expect("owned pins")
                .registration(at)
                .and_then(|_| pool.health())
                .and_then(|()| {
                    pool.pair(index, result.guards[at].as_ref().expect("acquired guard"))
                        .map(|_| ())
                })
            {
                return result.reject(error);
            }
        }
        Ok(result)
    }

    pub(crate) fn try_write(mut self) -> Result<WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        let mut pins = self.pins.take().expect("owned pin set");
        let owner = self.owner.take().expect("owned idle owner");
        let version = match owner.try_apply().and_then(|apply| apply.try_version()) {
            Ok(version) => version,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let mut result = WriteTargets {
            guards: [const { None }; N],
            version: Some(version),
            pins: Some(pins),
        };
        let pool = result.pins.as_ref().expect("owned pins").pool;
        for at in 0..result.len() {
            let index = result.pins.as_ref().expect("owned pins").indices[at];
            match pool.frames[index].content.try_write() {
                Ok(guard) => result.guards[at] = Some(guard),
                Err(TryLockError::WouldBlock) => return result.reject(unchanged(Cause::Busy)),
                Err(TryLockError::Poisoned(error)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(error.into_inner());
                    return result.reject(failure);
                }
            }
            if let Err(error) = result
                .pins
                .as_ref()
                .expect("owned pins")
                .registration(at)
                .and_then(|_| pool.health())
                .and_then(|()| {
                    pool.pair(index, result.guards[at].as_ref().expect("acquired guard"))
                        .map(|_| ())
                })
            {
                return result.reject(error);
            }
        }
        Ok(result)
    }
    pub(crate) fn release_checked(mut self) -> Result<()> {
        self.owner.take();
        self.pins.as_mut().map_or(Ok(()), PinSet::release)
    }
}

pub(crate) struct ReadTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize = MAX_TARGETS> {
    guards: [Option<RwLockReadGuard<'b, FrameContent<'d>>>; N],
    admission: Option<ContentAdmission<'a, 'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd, N>>,
}
impl<const N: usize> ReadTargets<'_, '_, '_, '_, '_, '_, N> {
    fn len(&self) -> usize {
        self.pins.as_ref().map_or(0, |pins| pins.len)
    }
    pub(in crate::storage) fn target_request(&self, at: usize) -> Result<TargetRequest> {
        let pins = self.pins.as_ref().expect("live read targets");
        if at >= pins.len {
            return Err(unchanged(Cause::Shape));
        }
        pins.pool.health()?;
        Ok(pins.registration(at)?.request)
    }
    pub(crate) fn observe(&self, at: usize) -> Result<DirtyObservation> {
        let pins = self.pins.as_ref().expect("live read targets");
        pins.pool.health()?;
        let guard = self
            .guards
            .get(at)
            .and_then(Option::as_ref)
            .ok_or_else(|| unchanged(Cause::Shape))?;
        pins.registration(at)?;
        pins.pool.pair(pins.indices[at], guard)
    }
    pub(crate) fn bytes(&self, at: usize) -> Result<&[u8; PAGE_BYTES]> {
        self.observe(at)?;
        Ok(self.guards[at].as_ref().expect("observed guard").bytes)
    }
    pub(crate) fn copy_snapshot(
        &self,
        at: usize,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<FrameStamp> {
        let observation = self.observe(at)?;
        output.copy_from_slice(self.guards[at].as_ref().expect("observed guard").bytes);
        Ok(observation.frame)
    }
    fn release(&mut self) -> Result<()> {
        if let Some(pins) = &self.pins {
            pins.pool.fence_unwind();
            #[cfg(test)]
            if std::thread::panicking() && self.guards.iter().any(Option::is_some) {
                test_support::fenced(pins.pool);
            }
        }
        for guard in self.guards.iter_mut().rev() {
            guard.take();
        }
        #[cfg(test)]
        if let Some(pins) = &self.pins
            && pins.active
        {
            test_support::before_release(pins.pool);
        }
        let admission = self.admission.take().map_or(Ok(()), |admission| {
            admission.release_checked().map_err(DirtyError::from)
        });
        let pins = self.pins.as_mut().map_or(Ok(()), PinSet::release);
        combine(admission, pins)
    }
    fn reject<T>(mut self, error: DirtyError) -> Result<T> {
        combine(Err(error), self.release()).and_then(|()| unreachable!())
    }
    pub(crate) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}
impl<const N: usize> Drop for ReadTargets<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Private capability bound to the stable pool; it contains no held leaf guard.
pub(crate) struct DptStamp<'b> {
    anchor: &'b u8,
    index: usize,
    generation: u64,
    counter: u64,
    rec_lsn: Option<PageLsn>,
}

pub(crate) struct WriteTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize = MAX_TARGETS> {
    guards: [Option<RwLockWriteGuard<'b, FrameContent<'d>>>; N],
    version: Option<VersionEditGuard<'a, 'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd, N>>,
}
impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    fn len(&self) -> usize {
        self.pins.as_ref().map_or(0, |pins| pins.len)
    }
    pub(crate) fn matches_stamp(&self, at: usize, expected: &FrameStamp) -> Result<()> {
        let pins = self.pins.as_ref().expect("live write targets");
        pins.pool.health()?;
        let guard = self
            .guards
            .get(at)
            .and_then(Option::as_ref)
            .ok_or_else(|| unchanged(Cause::Shape))?;
        pins.registration(at)?;
        let actual = pins.pool.pair(pins.indices[at], guard)?.frame;
        if actual != *expected {
            return Err(unchanged(Cause::Stale));
        }
        if actual.modification_counter == u64::MAX {
            return Err(unchanged(Cause::CounterExhausted));
        }
        Ok(())
    }
    pub(crate) fn preflight_dpt(&self, at: usize, expected: &FrameStamp) -> Result<DptStamp<'b>> {
        self.matches_stamp(at, expected)?;
        let pins = self.pins.as_ref().expect("live write targets");
        Ok(DptStamp {
            anchor: &pins.pool.stamp_anchor,
            index: pins.indices[at],
            generation: expected.generation,
            counter: expected.modification_counter,
            rec_lsn: expected.rec_lsn,
        })
    }
    fn check_permit(&self, at: usize, permit: &DirtyInstallPermit<'_, 'c, 's>) -> Result<()> {
        let pins = self.pins.as_ref().expect("live write targets");
        pins.pool.health()?;
        if at >= pins.len {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        let registration = pins.registration(at)?;
        if !permit.matches_core(pins.pool.core)
            || !permit.requirement().uses_fence(pins.pool.fence)
            || permit.identity() != registration.request.identity
            || permit.generation() != registration.generation
            || permit.before_counter().checked_add(1) != Some(permit.after_counter())
            || permit.page_lsn().0 <= permit.prior_lsn().0
            || permit.first_rec_lsn().0 == 0
            || permit.first_rec_lsn().0 > permit.page_lsn().0
        {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        Ok(())
    }

    /// Only the actual stage producer can seal this immutable after image.
    pub(in crate::storage) fn install_page(
        &mut self,
        at: usize,
        permit: &DirtyInstallPermit<'_, 'c, 's>,
    ) -> Result<()> {
        self.check_permit(at, permit)?;
        let pins = self.pins.as_ref().expect("live write targets");
        let guard = self.guards[at].as_mut().expect("held write guard");
        if !guard.valid()
            || guard.page_lsn != permit.prior_lsn()
            || guard.modification_counter != permit.before_counter()
            || guard
                .rec_lsn
                .is_some_and(|rec| rec != permit.first_rec_lsn())
        {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        guard.bytes.copy_from_slice(permit.after());
        guard.page_lsn = permit.page_lsn();
        Ok(())
    }

    pub(in crate::storage) fn install_metadata(
        &mut self,
        at: usize,
        permit: &DirtyInstallPermit<'_, 'c, 's>,
    ) -> Result<()> {
        self.check_permit(at, permit)?;
        let pins = self.pins.as_ref().expect("live write targets");
        let guard = self.guards[at].as_mut().expect("held write guard");
        if guard.page_lsn != permit.page_lsn()
            || guard.modification_counter != permit.before_counter()
            || guard
                .rec_lsn
                .is_some_and(|old| old != permit.first_rec_lsn())
            || (guard.rec_lsn.is_none() && permit.first_rec_lsn() != permit.page_lsn())
        {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        guard.modification_counter = permit.after_counter();
        guard.dirty = true;
        guard.rec_lsn = Some(permit.first_rec_lsn());
        Ok(())
    }

    pub(in crate::storage) fn install_dpt(
        &self,
        stamp: DptStamp<'_>,
        permit: &DirtyInstallPermit<'_, 'c, 's>,
    ) -> Result<()> {
        let pins = self.pins.as_ref().expect("live write targets");
        if !std::ptr::eq(stamp.anchor, &pins.pool.stamp_anchor) {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        let at = pins.indices[..pins.len]
            .iter()
            .position(|index| *index == stamp.index)
            .ok_or_else(|| pins.pool.fatal(Cause::Invariant))?;
        self.check_permit(at, permit)?;
        let content = self.guards[at].as_ref().expect("held write guard");
        let actual = pins.pool.stamp(stamp.index, content)?;
        if actual.generation != stamp.generation
            || stamp.counter != permit.before_counter()
            || actual.modification_counter != permit.after_counter()
            || actual.page_lsn != permit.page_lsn()
            || actual.rec_lsn != Some(permit.first_rec_lsn())
            || actual.rec_lsn != Some(stamp.rec_lsn.unwrap_or(actual.page_lsn))
        {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        let mut dpt = pins.pool.entry_dpt()?;
        let expected = stamp.rec_lsn.map(|rec_lsn| DirtyEntry {
            identity: actual.identity,
            generation: stamp.generation,
            rec_lsn,
            modification_counter: stamp.counter,
        });
        if dpt.entries[stamp.index] != expected
            || dpt.requirements[stamp.index].map(|value| value.location().lsn)
                != stamp.rec_lsn.map(|_| permit.prior_lsn().0)
        {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        dpt.entries[stamp.index] = Some(DirtyEntry {
            identity: actual.identity,
            generation: actual.generation,
            rec_lsn: permit.first_rec_lsn(),
            modification_counter: actual.modification_counter,
        });
        dpt.requirements[stamp.index] = Some(permit.requirement());
        Ok(())
    }
    fn release(&mut self) -> Result<()> {
        if let Some(pins) = &self.pins {
            pins.pool.fence_unwind();
            #[cfg(test)]
            if std::thread::panicking() && self.guards.iter().any(Option::is_some) {
                test_support::fenced(pins.pool);
            }
        }
        for guard in self.guards.iter_mut().rev() {
            guard.take();
        }
        #[cfg(test)]
        if let Some(pins) = &self.pins
            && pins.active
        {
            test_support::before_release(pins.pool);
        }
        let version = self.version.take().map_or(Ok(()), |version| {
            version.release_checked().map_err(DirtyError::from)
        });
        let pins = self.pins.as_mut().map_or(Ok(()), PinSet::release);
        combine(version, pins)
    }
    fn reject<T>(mut self, error: DirtyError) -> Result<T> {
        combine(Err(error), self.release()).and_then(|()| unreachable!())
    }
    pub(crate) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}
impl<const N: usize> Drop for WriteTargets<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Source-value bound for one target reservation/acquisition/cleanup chain.
/// Retained groups and concurrently in-flight calls require separate admission.
pub(in crate::storage) const fn target_set_call_bytes<const N: usize>() -> usize {
    use std::mem::size_of;
    let read = size_of::<ReadTargets<'_, '_, '_, '_, '_, '_, N>>();
    let write = size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, N>>();
    let largest = if read > write { read } else { write };
    let read_result = size_of::<Result<ReadTargets<'_, '_, '_, '_, '_, '_, N>>>();
    let write_result = size_of::<Result<WriteTargets<'_, '_, '_, '_, '_, '_, N>>>();
    let result = if read_result > write_result {
        read_result
    } else {
        write_result
    };
    2 * result
        + largest
        + size_of::<PinSet<'_, '_, '_, '_, '_, N>>()
        + size_of::<PinnedTargets<'_, '_, '_, '_, '_, '_, N>>()
        + size_of::<Result<PinnedTargets<'_, '_, '_, '_, '_, '_, N>>>()
        + 6 * size_of::<Result<Registration>>()
        + 2 * size_of::<DirtyObservation>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<MutexGuard<'_, PinAccounting>>()
        + 32 * 8
        + 64
}
const _: () = assert!(target_set_call_bytes::<2>() <= CALL_BYTES);
const _: () = assert!(target_set_call_bytes::<64>() <= 16384);

pub(in crate::storage) fn fits() -> bool {
    use std::mem::size_of;
    size_of::<DirtyFrame<'_>>() <= 256
        && size_of::<Option<Registration>>() <= 128
        && size_of::<Option<DirtyEntry>>() <= 128
        && size_of::<PinAccounting>() <= 512
        && size_of::<PinSet<'_, '_, '_, '_, '_>>() <= 64
        && size_of::<ReadTargets<'_, '_, '_, '_, '_, '_>>() <= 256
        && size_of::<WriteTargets<'_, '_, '_, '_, '_, '_>>() <= 256
        && size_of::<DptStamp<'_>>() <= 128
        && size_of::<FrameStamp>() <= 112
        && size_of::<DirtyObservation>() <= 192
        && size_of::<Result<DirtyObservation>>() <= 192
        && size_of::<Result<FrameStamp>>() <= 112
        && size_of::<Option<DirtyEntry>>() <= 96
        && size_of::<Result<Registration>>() <= 128
        && size_of::<DirtyError>() <= 32
        && size_of::<Result<()>>() <= 32
        && size_of::<MutexGuard<'_, PinAccounting>>() <= 32
        && size_of::<std::sync::LockResult<MutexGuard<'_, PinAccounting>>>() <= 32
        && size_of::<RwLockReadGuard<'_, FrameContent<'_>>>() <= 32
        && size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>() <= 32
        && size_of::<WalRequirement<'_>>() <= 128
        && size_of::<Option<WalRequirement<'_>>>() <= 128
        && size_of::<DirtyRegistry<'_>>() <= 16384
        && registration::fits()
        && flush::fits()
        && init::fits()
}

pub(in crate::storage) fn init_pool_delta_bytes() -> usize {
    init::pool_delta_bytes()
}
pub(in crate::storage) fn init_fits() -> bool {
    fits()
}
#[cfg(test)]
pub(in crate::storage) use init::test_support as init_test_support;

/// Increment beyond the previously reserved entries-only DPT. Constructor
/// accounting uses this actual target-layout delta, including alignment.
pub(in crate::storage) fn flush_registry_bytes() -> usize {
    std::mem::size_of::<Mutex<DirtyRegistry<'_>>>()
        - std::mem::size_of::<Mutex<[Option<DirtyEntry>; MAX_FRAMES]>>()
        + std::mem::size_of::<&AtomicU8>()
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(in crate::storage) use super::flush::test_support::{
        FlushSnapshot, actual_requirement, corrupt_flush_claim, flush_diagnose,
        replace_requirement, seed_last_attempt, seed_uncaptured_claim, stale_capture_generation,
    };
    use super::*;
    use std::sync::mpsc::{Receiver, SyncSender};
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum FlushPoint {
        Claimed,
        Copied,
        Finalized,
    }

    pub(crate) fn install_flush_pause(
        pool: &SharedPool<'_, '_, '_, '_>,
        point: FlushPoint,
        arrived: SyncSender<()>,
        resume: Receiver<Resume>,
    ) {
        *pool.flush_pause.lock().unwrap() = Some((point, FencePause { arrived, resume }));
    }
    fn flush_pause(pool: &SharedPool<'_, '_, '_, '_>, point: FlushPoint) {
        let pause = {
            let mut pause = pool
                .flush_pause
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if pause.as_ref().is_some_and(|(at, _)| *at == point) {
                pause.take()
            } else {
                None
            }
        };
        if let Some((_, pause)) = pause {
            pause.arrived.try_send(()).expect("bounded flush arrival");
            match pause
                .resume
                .recv_timeout(Duration::from_secs(5))
                .expect("bounded flush release")
            {
                Resume::Continue => {}
                Resume::Panic => panic!("requested flush unwind"),
            }
        }
    }
    pub(super) fn flush_claimed(pool: &SharedPool<'_, '_, '_, '_>) {
        flush_pause(pool, FlushPoint::Claimed);
    }
    pub(super) fn flush_copied(pool: &SharedPool<'_, '_, '_, '_>) {
        flush_pause(pool, FlushPoint::Copied);
    }
    pub(super) fn flush_finalized(pool: &SharedPool<'_, '_, '_, '_>) {
        flush_pause(pool, FlushPoint::Finalized);
    }

    /// Actual target layouts only; tests own every independent ceiling.
    #[derive(Debug)]
    pub(crate) struct Layout {
        pub(crate) frame: usize,
        pub(crate) registration_option: usize,
        pub(crate) entry_option: usize,
        pub(crate) accounting: usize,
        pub(crate) pin_set: usize,
        pub(crate) read_targets: usize,
        pub(crate) write_targets: usize,
        pub(crate) dpt_stamp: usize,
        pub(crate) frame_stamp: usize,
        pub(crate) frame_stamp_result: usize,
        pub(crate) observation: usize,
        pub(crate) observation_result: usize,
        pub(crate) error: usize,
        pub(crate) unit_result: usize,
        pub(crate) registration_result: usize,
        pub(crate) pin_guard: usize,
        pub(crate) pin_lock_result: usize,
        pub(crate) read_guard: usize,
        pub(crate) write_guard: usize,
        pub(crate) pool: usize,
    }
    pub(crate) fn layout() -> Layout {
        use std::mem::size_of;
        Layout {
            frame: size_of::<DirtyFrame<'_>>(),
            registration_option: size_of::<Option<Registration>>(),
            entry_option: size_of::<Option<DirtyEntry>>(),
            accounting: size_of::<PinAccounting>(),
            pin_set: size_of::<PinSet<'_, '_, '_, '_, '_>>(),
            read_targets: size_of::<ReadTargets<'_, '_, '_, '_, '_, '_>>(),
            write_targets: size_of::<WriteTargets<'_, '_, '_, '_, '_, '_>>(),
            dpt_stamp: size_of::<DptStamp<'_>>(),
            frame_stamp: size_of::<FrameStamp>(),
            frame_stamp_result: size_of::<Result<FrameStamp>>(),
            observation: size_of::<DirtyObservation>(),
            observation_result: size_of::<Result<DirtyObservation>>(),
            error: size_of::<DirtyError>(),
            unit_result: size_of::<Result<()>>(),
            registration_result: size_of::<Result<Registration>>(),
            pin_guard: size_of::<MutexGuard<'_, PinAccounting>>(),
            pin_lock_result: size_of::<std::sync::LockResult<MutexGuard<'_, PinAccounting>>>(),
            read_guard: size_of::<RwLockReadGuard<'_, FrameContent<'_>>>(),
            write_guard: size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>(),
            pool: size_of::<SharedPool<'_, '_, '_, '_>>(),
        }
    }

    pub(crate) enum Resume {
        Continue,
        Panic,
    }
    pub(super) struct FencePause {
        pub(super) arrived: SyncSender<()>,
        pub(super) resume: Receiver<Resume>,
    }
    pub(crate) fn install_fence_pause(
        pool: &SharedPool<'_, '_, '_, '_>,
        arrived: SyncSender<()>,
        resume: Receiver<Resume>,
    ) {
        *pool.fence_pause.lock().unwrap() = Some(FencePause { arrived, resume });
    }
    pub(super) fn fenced(pool: &SharedPool<'_, '_, '_, '_>) {
        let hook = pool
            .fence_pause
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(hook) = hook {
            hook.arrived.try_send(()).expect("bounded fence arrival");
            match hook
                .resume
                .recv_timeout(Duration::from_secs(5))
                .expect("bounded fence release")
            {
                Resume::Continue => {}
                Resume::Panic => panic!("injected fence cut"),
            }
        }
    }

    pub(crate) fn install_release_pause(
        pool: &SharedPool<'_, '_, '_, '_>,
        arrived: SyncSender<()>,
        resume: Receiver<Resume>,
    ) {
        *pool.release_pause.lock().unwrap() = Some(FencePause { arrived, resume });
    }
    pub(super) fn before_release(pool: &SharedPool<'_, '_, '_, '_>) {
        let hook = pool
            .release_pause
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(hook) = hook {
            hook.arrived.try_send(()).expect("bounded cleanup arrival");
            match hook
                .resume
                .recv_timeout(Duration::from_secs(5))
                .expect("bounded cleanup release")
            {
                Resume::Continue => {}
                Resume::Panic => panic!("injected cleanup cut"),
            }
        }
    }
    pub(crate) fn content_busy(pool: &SharedPool<'_, '_, '_, '_>, index: usize) -> bool {
        match pool.frames[index].content.try_write() {
            Ok(guard) => {
                drop(guard);
                false
            }
            Err(TryLockError::WouldBlock) => true,
            Err(TryLockError::Poisoned(error)) => {
                drop(error.into_inner());
                false
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct Snapshot {
        pub(crate) frame: FrameStamp,
        pub(crate) dirty: bool,
        pub(crate) entry: Option<DirtyEntry>,
        pub(crate) pins: [u16; MAX_FRAMES],
        pub(crate) total: u16,
    }
    /// Poison recovery is diagnostic only; it does not clear poison or service fence.
    pub(crate) fn diagnose(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        output: &mut [u8; PAGE_BYTES],
    ) -> Snapshot {
        let content = pool.frames[index]
            .content
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let registration = pool.resolve_registration(index).unwrap();
        output.copy_from_slice(content.bytes);
        let entry = pool
            .dpt
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries[index];
        let pins = pool.pins.lock().unwrap_or_else(|error| error.into_inner());
        Snapshot {
            frame: FrameStamp {
                identity: registration.request.identity,
                generation: registration.generation,
                page_lsn: content.page_lsn,
                modification_counter: content.modification_counter,
                rec_lsn: content.rec_lsn,
            },
            dirty: content.dirty,
            entry,
            pins: pins.counts,
            total: pins.total,
        }
    }
    pub(crate) fn seed_counter(pool: &SharedPool<'_, '_, '_, '_>, index: usize, counter: u64) {
        pool.frames[index]
            .content
            .write()
            .unwrap()
            .modification_counter = counter;
    }
    pub(crate) fn seed_dpt(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        entry: Option<DirtyEntry>,
    ) {
        pool.dpt.lock().unwrap().entries[index] = entry;
    }
    pub(crate) fn seed_pins(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        count: u16,
        total: u16,
    ) {
        let mut pins = pool.pins.lock().unwrap();
        pins.counts[index] = count;
        pins.total = total;
    }
    pub(crate) fn seed_generation(frame: &mut DirtyFrame<'_>, generation: u64) {
        frame.registration.as_mut().unwrap().generation = generation;
    }
    pub(crate) fn register(frame: &mut DirtyFrame<'_>, request: TargetRequest) {
        frame.register(request).unwrap();
    }
    pub(crate) fn poison_content(pool: &SharedPool<'_, '_, '_, '_>, index: usize) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = pool.frames[index].content.write().unwrap();
            panic!("injected content poison");
        }));
    }
    pub(crate) fn poison_dpt(pool: &SharedPool<'_, '_, '_, '_>) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = pool.dpt.lock().unwrap();
            panic!("injected DPT poison");
        }));
    }
    pub(crate) fn poison_pins(pool: &SharedPool<'_, '_, '_, '_>) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = pool.pins.lock().unwrap();
            panic!("injected pin poison");
        }));
    }
}
