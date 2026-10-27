//! One claimed existing image, then retirement-authorized conditional finalize.
//! This module does not perform I/O and cannot mint stage or retirement permits.

use super::*;
use crate::storage::wal_io::FlushFinalizePermit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Claim {
    attempt: u64,
    index: usize,
    registration: Registration,
    captured: Option<FrameStamp>,
}
impl Claim {
    pub(super) fn index(&self) -> usize {
        self.index
    }
    pub(super) fn request(&self) -> TargetRequest {
        self.registration.request
    }
    pub(super) fn valid(
        &self,
        pool: &SharedPool<'_, '_, '_, '_>,
        state: &PoolRegistry<'_>,
    ) -> bool {
        self.attempt != 0
            && self.attempt == state.last_attempt
            && state.directory.resolve(pool.frames, self.index) == Some(self.registration)
            && self.registration.generation != 0
            && self.captured.is_none_or(|stamp| {
                stamp.identity == self.registration.request.identity
                    && stamp.generation == self.registration.generation
                    && stamp.modification_counter != 0
                    && stamp
                        .rec_lsn
                        .is_some_and(|rec| rec.0 != 0 && rec.0 <= stamp.page_lsn.0)
            })
    }
}

/// Only a coherent copy under the actual content latch constructs this seal.
/// Its stable origin is outside the movable BoundAllocation value.
pub(in crate::storage) struct CapturedPage<'c, 's> {
    binding: &'s AtomicU8,
    core: &'c PageConcurrency<'s>,
    request: TargetRequest,
    index: usize,
    attempt: u64,
    stamp: FrameStamp,
    requirement: WalRequirement<'s>,
}
impl std::fmt::Debug for CapturedPage<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedPage")
            .field("attempt", &self.attempt)
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}
impl<'s> CapturedPage<'_, 's> {
    pub(in crate::storage) fn uses_core(&self, core: &PageConcurrency<'s>) -> bool {
        std::ptr::eq(self.core, core)
    }

    pub(in crate::storage) fn stamp(&self) -> FrameStamp {
        self.stamp
    }
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.request
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    pub(in crate::storage) fn attempt(&self) -> u64 {
        self.attempt
    }
}

/// A point-in-time result. A writer may redirty after the content latch ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) struct FinalizedPage {
    pub(in crate::storage) frame: FrameStamp,
    pub(in crate::storage) cleaned: bool,
}

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn check_flusher(&self, owner: &Owner<'c, 's>) -> Result<()> {
        self.health()?;
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        owner.require_flusher().map_err(DirtyError::from)
    }

    /// Stored-state-only admission. No pin, image copy, checksum or I/O.
    pub(in crate::storage) fn flush_preflight(&self) -> Result<()> {
        let registry = self.entry_registry()?;
        if registry.claim.is_some() {
            return Err(unchanged(Cause::Busy));
        }
        registry
            .last_attempt
            .checked_add(1)
            .ok_or_else(|| unchanged(Cause::Capacity))?;
        Ok(())
    }

    /// Registers the physical-address claim before taking any capture pin/latch.
    /// None is a coherent Clean observation, with checked claim return and I/O0.
    pub(in crate::storage) fn capture_flush(
        &self,
        owner: &mut Owner<'c, 's>,
        request: TargetRequest,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<Option<CapturedPage<'c, 's>>> {
        self.check_flusher(owner)?;
        let (index, registration, attempt) = {
            let mut registry = self.entry_registry()?;
            if registry.claim.is_some()
                || registry
                    .load
                    .as_ref()
                    .is_some_and(|load| load.wants(request))
            {
                return Err(unchanged(Cause::Busy));
            }
            let (index, registration) = registry
                .directory
                .lookup(self.frames, request)
                .ok_or_else(|| unchanged(Cause::MissingTarget))?;
            if registry
                .load
                .as_ref()
                .is_some_and(|load| load.blocks(request, index))
            {
                return Err(unchanged(Cause::Busy));
            }
            let attempt = registry
                .last_attempt
                .checked_add(1)
                .ok_or_else(|| unchanged(Cause::Capacity))?;
            registry.last_attempt = attempt;
            registry.claim = Some(Claim {
                attempt,
                index,
                registration,
                captured: None,
            });
            (index, registration, attempt)
        };
        let mut pending = CaptureAttempt {
            pool: self,
            attempt,
            active: true,
        };
        #[cfg(test)]
        super::test_support::flush_claimed(self);
        let read = match self
            .reserve_targets(owner, &[request])
            .and_then(PinnedTargets::try_read)
        {
            Ok(read) => read,
            Err(error) if error.effect == Effect::Unchanged => {
                return combine(Err(error), pending.cancel()).and_then(|()| unreachable!());
            }
            Err(error) => return Err(error),
        };
        let copied = (|| -> Result<Option<CapturedPage<'c, 's>>> {
            let stamp = read.copy_snapshot(0, output)?;
            if stamp.rec_lsn.is_none() {
                return Ok(None);
            }
            let mut registry = self.entry_registry()?;
            let requirement =
                registry.requirements[index].ok_or_else(|| self.fatal(Cause::Invariant))?;
            if requirement.location().lsn != stamp.page_lsn.0 || !requirement.uses_fence(self.fence)
            {
                return Err(self.fatal(Cause::Invariant));
            }
            let claim = registry
                .claim
                .as_mut()
                .ok_or_else(|| self.fatal(Cause::Invariant))?;
            if claim.attempt != attempt
                || claim.index != index
                || claim.registration != registration
                || claim.captured.is_some()
            {
                return Err(self.fatal(Cause::Invariant));
            }
            claim.captured = Some(stamp);
            Ok(Some(CapturedPage {
                binding: self.binding,
                core: self.core,
                request,
                index,
                attempt,
                stamp,
                requirement,
            }))
        })();
        #[cfg(test)]
        if copied.as_ref().is_ok_and(Option::is_some) {
            super::test_support::flush_copied(self);
        }
        let cleanup = read.release_checked();
        let captured = match (copied, cleanup) {
            (Err(error), _) if error.effect != Effect::Unchanged => return Err(error),
            (_, Err(error)) => return Err(error),
            (result, Ok(())) => result?,
        };
        if captured.is_none() {
            pending.cancel()?;
        } else {
            self.health()?;
            pending.active = false;
        }
        Ok(captured)
    }

    fn claim_matches(&self, registry: &PoolRegistry<'s>, capture: &CapturedPage<'c, 's>) -> bool {
        std::ptr::eq(capture.binding, self.binding)
            && std::ptr::eq(capture.core, self.core)
            && capture.requirement.uses_fence(self.fence)
            && registry.claim.is_some_and(|claim| {
                claim.attempt == capture.attempt
                    && claim.index == capture.index
                    && claim.registration.request == capture.request
                    && claim.registration.generation == capture.stamp.generation
                    && claim.captured == Some(capture.stamp)
            })
    }

    pub(in crate::storage) fn validate_capture(
        &self,
        capture: &CapturedPage<'c, 's>,
    ) -> Result<()> {
        let registry = self.entry_registry()?;
        if !self.claim_matches(&registry, capture) {
            return Err(self.fatal(Cause::Invariant));
        }
        let registration = self.registration_in(&registry, capture.index)?;
        if registration.request != capture.request
            || registration.generation != capture.stamp.generation
        {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(())
    }

    /// Retirement is already durable. None retains the claim for RAM-only retry.
    /// The final claim return occurs after all content/admission/pin cleanup.
    pub(in crate::storage) fn finalize_flush(
        &self,
        owner: &mut Owner<'c, 's>,
        capture: &CapturedPage<'c, 's>,
        permit: &FlushFinalizePermit<'_, 'c, 's>,
    ) -> Result<Option<FinalizedPage>> {
        self.check_flusher(owner)?;
        self.validate_capture(capture)?;
        if !permit.matches(capture) {
            return Err(self.fatal(Cause::Invariant));
        }
        let pinned = match self.reserve_targets(owner, &[capture.request]) {
            Ok(pinned) => pinned,
            Err(error) if retryable(error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut write = match FinalizeContent::acquire(pinned) {
            Ok(write) => write,
            Err(error) if retryable(error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let finalized = (|| -> Result<FinalizedPage> {
            let content = write.guard.as_mut().expect("held finalize content");
            let actual = self.stamp(capture.index, content)?;
            let mut registry = self.entry_registry()?;
            let expected = actual.rec_lsn.map(|rec_lsn| DirtyEntry {
                identity: actual.identity,
                generation: actual.generation,
                rec_lsn,
                modification_counter: actual.modification_counter,
            });
            if !self.claim_matches(&registry, capture)
                || !self.dpt_matches(&registry, capture.index, expected.as_ref())
                || registry.requirements[capture.index].map(|value| value.location().lsn)
                    != actual.rec_lsn.map(|_| actual.page_lsn.0)
                || actual.identity != capture.stamp.identity
                || actual.generation != capture.stamp.generation
                || actual.rec_lsn != capture.stamp.rec_lsn
                || actual.modification_counter < capture.stamp.modification_counter
                || actual.page_lsn.0 < capture.stamp.page_lsn.0
                || !content.dirty
            {
                return Err(self.fatal(Cause::Invariant));
            }
            let cleaned = actual.modification_counter == capture.stamp.modification_counter;
            if (cleaned && actual != capture.stamp)
                || (!cleaned && actual.page_lsn.0 <= capture.stamp.page_lsn.0)
            {
                return Err(self.fatal(Cause::Invariant));
            }
            // No fallible operation inside this coupled state change.
            if cleaned {
                content.dirty = false;
                content.rec_lsn = None;
                registry.entries[capture.index] = None;
                registry.requirements[capture.index] = None;
            }
            Ok(FinalizedPage {
                frame: FrameStamp {
                    rec_lsn: if cleaned { None } else { actual.rec_lsn },
                    ..actual
                },
                cleaned,
            })
        })();
        #[cfg(test)]
        if finalized.is_ok() {
            super::test_support::flush_finalized(self);
        }
        let cleanup = write.release();
        let finalized = match (finalized, cleanup) {
            (Err(error), _) if error.effect != Effect::Unchanged => return Err(error),
            (_, Err(error)) => return Err(error),
            (result, Ok(())) => result?,
        };
        let mut registry = self.entry_registry()?;
        if !self.claim_matches(&registry, capture) {
            return Err(self.fatal(Cause::Invariant));
        }
        registry.claim = None;
        Ok(Some(finalized))
    }
}

fn retryable(error: DirtyError) -> bool {
    error.effect == Effect::Unchanged
        && matches!(
            error.cause,
            Cause::Busy
                | Cause::Capacity
                | Cause::Concurrency(concurrency::Cause::InstallationPending)
        )
}

struct CaptureAttempt<'b, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    attempt: u64,
    active: bool,
}
impl CaptureAttempt<'_, '_, '_, '_, '_> {
    fn cancel(&mut self) -> Result<()> {
        let mut registry = self.pool.entry_registry()?;
        if !registry
            .claim
            .is_some_and(|claim| claim.attempt == self.attempt && claim.captured.is_none())
        {
            return Err(self.pool.fatal(Cause::Invariant));
        }
        registry.claim = None;
        self.active = false;
        Ok(())
    }
}
impl Drop for CaptureAttempt<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.pool.fence.fence();
        }
    }
}

struct FinalizeContent<'a, 'b, 'c, 's, 'p, 'd> {
    guard: Option<RwLockWriteGuard<'b, FrameContent<'d>>>,
    admission: Option<ContentAdmission<'a, 'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd>>,
}
impl<'a, 'b, 'c, 's, 'p, 'd> FinalizeContent<'a, 'b, 'c, 's, 'p, 'd> {
    fn acquire(mut pinned: PinnedTargets<'a, 'b, 'c, 's, 'p, 'd>) -> Result<Self> {
        let mut pins = pinned.pins.take().expect("owned finalize pins");
        let owner = pinned.owner.take().expect("owned idle flusher");
        if pins.len != 1 {
            let error = pins.pool.fatal(Cause::Invariant);
            return combine(Err(error), pins.release()).and_then(|()| unreachable!());
        }
        let admission = match owner.try_content() {
            Ok(admission) => admission,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let pool = pins.pool;
        let index = pins.indices[0];
        let mut result = Self {
            guard: None,
            admission: Some(admission),
            pins: Some(pins),
        };
        match pool.frames[index].content.try_write() {
            Ok(guard) => result.guard = Some(guard),
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
            .expect("owned finalize pins")
            .registration(0)
            .and_then(|_| pool.pair(index, result.guard.as_ref().expect("held finalize content")))
        {
            return result.reject(error);
        }
        Ok(result)
    }
    fn release(&mut self) -> Result<()> {
        if let Some(pins) = &self.pins {
            pins.pool.fence_unwind();
            #[cfg(test)]
            if std::thread::panicking() && self.guard.is_some() {
                super::test_support::fenced(pins.pool);
            }
        }
        self.guard.take();
        #[cfg(test)]
        if let Some(pins) = &self.pins
            && pins.active
        {
            super::test_support::before_release(pins.pool);
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
}
impl Drop for FinalizeContent<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(super) fn fits() -> bool {
    use std::mem::size_of;
    size_of::<Claim>() <= 256
        && size_of::<CapturedPage<'_, '_>>() <= 384
        && size_of::<Option<CapturedPage<'_, '_>>>() <= 384
        && size_of::<Result<Option<CapturedPage<'_, '_>>>>() <= 512
        && size_of::<FinalizeContent<'_, '_, '_, '_, '_, '_>>() <= 192
        && size_of::<Result<FinalizeContent<'_, '_, '_, '_, '_, '_>>>() <= 256
        && size_of::<CaptureAttempt<'_, '_, '_, '_, '_>>() <= 64
        && size_of::<FinalizedPage>() <= 128
        && size_of::<Result<Option<FinalizedPage>>>() <= 192
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    use crate::storage::wal_io::WalFrameLocation;

    pub(in crate::storage) fn acquire_finalize(
        pinned: PinnedTargets<'_, '_, '_, '_, '_, '_>,
    ) -> Result<()> {
        FinalizeContent::acquire(pinned)
            .map(|mut content| content.release())
            .and_then(|result| result)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct ClaimSnapshot {
        pub(crate) attempt: u64,
        pub(crate) index: usize,
        pub(crate) identity: crate::storage::page::PageIdentity,
        pub(crate) generation: u64,
        pub(crate) captured: Option<FrameStamp>,
    }
    #[derive(Debug)]
    pub(crate) struct FlushSnapshot {
        pub(crate) claim: Option<ClaimSnapshot>,
        pub(crate) last_attempt: u64,
        pub(crate) requirements: [Option<WalFrameLocation>; MAX_FRAMES],
    }
    pub(crate) fn flush_diagnose(pool: &SharedPool<'_, '_, '_, '_>) -> FlushSnapshot {
        let registry = pool
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        FlushSnapshot {
            claim: registry.claim.map(|claim| ClaimSnapshot {
                attempt: claim.attempt,
                index: claim.index,
                identity: claim.registration.request.identity,
                generation: claim.registration.generation,
                captured: claim.captured,
            }),
            last_attempt: registry.last_attempt,
            requirements: registry
                .requirements
                .map(|value| value.map(|value| value.location())),
        }
    }
    pub(crate) fn seed_last_attempt(pool: &SharedPool<'_, '_, '_, '_>, attempt: u64) {
        pool.registry.lock().unwrap().last_attempt = attempt;
    }
    /// Isolated orphan of the genuine pre-capture claim shape, with no pin or
    /// dirty entry. This is a test-only quiescence witness, never I/O authority.
    pub(crate) fn seed_uncaptured_claim(pool: &SharedPool<'_, '_, '_, '_>) {
        let registration = pool.resolve_registration(0).unwrap().unwrap();
        let mut registry = pool.registry.lock().unwrap();
        assert!(registry.claim.is_none());
        registry.last_attempt = registry.last_attempt.checked_add(1).unwrap();
        registry.claim = Some(Claim {
            attempt: registry.last_attempt,
            index: 0,
            registration,
            captured: None,
        });
    }
    pub(in crate::storage) fn actual_requirement<'s>(
        pool: &SharedPool<'_, 's, '_, '_>,
        index: usize,
    ) -> Option<WalRequirement<'s>> {
        pool.registry.lock().unwrap().requirements[index]
    }
    /// Requires a receipt issued by a real stage; cannot manufacture provenance.
    pub(in crate::storage) fn replace_requirement<'s>(
        pool: &SharedPool<'_, 's, '_, '_>,
        index: usize,
        actual: WalRequirement<'s>,
    ) {
        pool.registry.lock().unwrap().requirements[index] = Some(actual);
    }
    pub(in crate::storage) fn stale_capture_generation(capture: &mut CapturedPage<'_, '_>) {
        capture.stamp.generation += 1;
    }
    /// Six frozen trusted-state corruptions, applied only to a real claim.
    pub(crate) fn corrupt_flush_claim(pool: &SharedPool<'_, '_, '_, '_>, variant: u8) {
        let mut registry = pool.registry.lock().unwrap();
        let last = registry.last_attempt;
        let claim = registry.claim.as_mut().expect("actual active claim");
        match variant {
            1 => claim.attempt = 0,
            2 => claim.attempt = last.checked_add(1).expect("finite test attempt"),
            3 => claim.index = MAX_FRAMES,
            4 => claim.registration.generation = 2,
            5 => {
                claim
                    .captured
                    .as_mut()
                    .expect("actual captured claim")
                    .identity
                    .owner = foundation::identity::ObjectId::new(2).unwrap()
            }
            6 => {
                claim
                    .captured
                    .as_mut()
                    .expect("actual captured claim")
                    .rec_lsn = Some(PageLsn(0))
            }
            _ => panic!("unknown frozen claim corruption"),
        }
    }
}
