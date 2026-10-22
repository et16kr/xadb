//! Fixed authoritative dirty frames and bounded pin/content ownership.
//! No load, replacement, flush, capture, Commit or raw semantic-stage authority.
#![allow(dead_code)] // The separately reviewed allocator consumer is being connected.

use super::{
    page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn},
    session_fence::SessionFence,
    wal_io::DirtyInstallPermit,
};
use crate::transaction::concurrency::{
    self, ContentAdmission, Owner, PageConcurrency, VersionEditGuard,
};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError},
};
use xadb_foundation::error::Effect;

pub(crate) const MAX_FRAMES: usize = 64;
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
    fn valid(&self) -> bool {
        if self.dirty {
            self.modification_counter != 0
                && self
                    .rec_lsn
                    .is_some_and(|lsn| lsn.0 != 0 && lsn.0 <= self.page_lsn.0)
        } else {
            self.modification_counter == 0 && self.rec_lsn.is_none() && self.page_lsn.0 == 0
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
        if !content.valid() || content.dirty {
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
}

/// Fixed directory and leaf registries; the exact core cannot be caller-selected.
pub(crate) struct SharedPool<'c, 's, 'p, 'd> {
    frames: &'p [DirtyFrame<'d>],
    directory: [Option<Registration>; MAX_FRAMES],
    pins: Mutex<PinAccounting>,
    dpt: Mutex<[Option<DirtyEntry>; MAX_FRAMES]>,
    core: &'c PageConcurrency<'s>,
    fence: &'s SessionFence,
    stamp_anchor: u8,
    #[cfg(test)]
    fence_pause: Mutex<Option<test_support::FencePause>>,
    #[cfg(test)]
    release_pause: Mutex<Option<test_support::FencePause>>,
}
impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn new(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
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
        let mut directory = [None; MAX_FRAMES];
        for (index, frame) in frames.iter().enumerate() {
            if let Some(registration) = frame.registration {
                if registration.generation != 1
                    || directory[..index]
                        .iter()
                        .flatten()
                        .any(|prior: &Registration| {
                            prior.request.same_address(&registration.request)
                        })
                {
                    fence.fence();
                    return Err(DirtyError {
                        cause: Cause::Invariant,
                        effect: Effect::CallerMustResolve,
                    });
                }
                directory[index] = Some(registration);
            }
        }
        Ok(Self {
            frames,
            directory,
            pins: Mutex::new(PinAccounting {
                counts: [0; MAX_FRAMES],
                total: 0,
            }),
            dpt: Mutex::new([None; MAX_FRAMES]),
            core,
            fence,
            stamp_anchor: 0,
            #[cfg(test)]
            fence_pause: Mutex::new(None),
            #[cfg(test)]
            release_pause: Mutex::new(None),
        })
    }

    pub(in crate::storage) fn bound_core(&self) -> &'c PageConcurrency<'s> {
        self.core
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
        let mut sum = 0u16;
        for (index, count) in pins.counts.iter().enumerate() {
            if *count > MAX_FRAME_PINS || (self.directory[index].is_none() && *count != 0) {
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
    fn dpt_valid(&self, dpt: &[Option<DirtyEntry>; MAX_FRAMES]) -> bool {
        dpt.iter().enumerate().all(|(index, entry)| match entry {
            None => true,
            Some(entry) => self.directory[index].as_ref().is_some_and(|registration| {
                entry.identity == registration.request.identity
                    && entry.generation == registration.generation
                    && entry.rec_lsn.0 != 0
                    && entry.modification_counter != 0
            }),
        })
    }
    fn entry_dpt(&self) -> Result<MutexGuard<'_, [Option<DirtyEntry>; MAX_FRAMES]>> {
        self.health()?;
        let dpt = self.dpt.lock().map_err(|_| self.fatal(Cause::Poisoned))?;
        self.health()?;
        if !self.dpt_valid(&dpt) {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(dpt)
    }
    fn registration(&self, index: usize) -> Result<&Registration> {
        let registration = self
            .directory
            .get(index)
            .and_then(Option::as_ref)
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        if registration.generation != 1
            || self
                .frames
                .get(index)
                .and_then(|frame| frame.registration.as_ref())
                != Some(registration)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(registration)
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
        if dpt[index] != expected {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(DirtyObservation {
            frame,
            entry: expected,
        })
    }

    pub(crate) fn reserve_targets<'a, 'b>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        requests: &[TargetRequest],
    ) -> Result<PinnedTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        self.health()?;
        // Health precedes identity; a different core's mode is not ours to check.
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if requests.is_empty() || requests.len() > MAX_TARGETS {
            return Err(unchanged(Cause::Shape));
        }
        if requests.len() == 2 && requests[0].same_address(&requests[1]) {
            return Err(unchanged(Cause::DuplicateTarget));
        }
        let mut indices = [0; MAX_TARGETS];
        for (at, request) in requests.iter().enumerate() {
            indices[at] = self
                .directory
                .iter()
                .position(|entry| {
                    entry
                        .as_ref()
                        .is_some_and(|entry| entry.request == *request)
                })
                .ok_or_else(|| unchanged(Cause::MissingTarget))?;
            self.registration(indices[at])?;
        }
        if requests.len() == 2 && requests[0].rank() > requests[1].rank() {
            indices.swap(0, 1);
        }
        {
            let mut pins = self.entry_pins()?;
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
}

struct PinSet<'b, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    indices: [usize; MAX_TARGETS],
    len: usize,
    active: bool,
    _thread: PhantomData<Rc<()>>,
}
impl PinSet<'_, '_, '_, '_, '_> {
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
            if !self.pool.pins_valid(&pins) || !(1..=MAX_TARGETS).contains(&self.len) {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            for index in &self.indices[..self.len] {
                self.pool.registration(*index)?;
                if pins.counts[*index] == 0 {
                    return Err(self.pool.fatal(Cause::Invariant));
                }
            }
            if self.len == 2 && self.indices[0] == self.indices[1] {
                return Err(self.pool.fatal(Cause::Invariant));
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
impl Drop for PinSet<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(crate) struct PinnedTargets<'a, 'b, 'c, 's, 'p, 'd> {
    owner: Option<&'a mut Owner<'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd>>,
}
impl<'a, 'b, 'c, 's, 'p, 'd> PinnedTargets<'a, 'b, 'c, 's, 'p, 'd> {
    pub(crate) fn try_read(mut self) -> Result<ReadTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        let mut pins = self.pins.take().expect("owned pin set");
        let owner = self.owner.take().expect("owned idle owner");
        let admission = match owner.try_content() {
            Ok(admission) => admission,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let mut result = ReadTargets {
            guards: [None, None],
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
            if let Err(error) = pool.health().and_then(|()| {
                pool.pair(index, result.guards[at].as_ref().expect("acquired guard"))
                    .map(|_| ())
            }) {
                return result.reject(error);
            }
        }
        Ok(result)
    }

    pub(crate) fn try_write(mut self) -> Result<WriteTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        let mut pins = self.pins.take().expect("owned pin set");
        let owner = self.owner.take().expect("owned idle owner");
        let version = match owner.try_apply().and_then(|apply| apply.try_version()) {
            Ok(version) => version,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let mut result = WriteTargets {
            guards: [None, None],
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
            if let Err(error) = pool.health().and_then(|()| {
                pool.pair(index, result.guards[at].as_ref().expect("acquired guard"))
                    .map(|_| ())
            }) {
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

pub(crate) struct ReadTargets<'a, 'b, 'c, 's, 'p, 'd> {
    guards: [Option<RwLockReadGuard<'b, FrameContent<'d>>>; MAX_TARGETS],
    admission: Option<ContentAdmission<'a, 'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd>>,
}
impl ReadTargets<'_, '_, '_, '_, '_, '_> {
    fn len(&self) -> usize {
        self.pins.as_ref().map_or(0, |pins| pins.len)
    }
    pub(crate) fn observe(&self, at: usize) -> Result<DirtyObservation> {
        let pins = self.pins.as_ref().expect("live read targets");
        pins.pool.health()?;
        let guard = self
            .guards
            .get(at)
            .and_then(Option::as_ref)
            .ok_or_else(|| unchanged(Cause::Shape))?;
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
impl Drop for ReadTargets<'_, '_, '_, '_, '_, '_> {
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

pub(crate) struct WriteTargets<'a, 'b, 'c, 's, 'p, 'd> {
    guards: [Option<RwLockWriteGuard<'b, FrameContent<'d>>>; MAX_TARGETS],
    version: Option<VersionEditGuard<'a, 'c, 's>>,
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd>>,
}
impl<'a, 'b, 'c, 's, 'p, 'd> WriteTargets<'a, 'b, 'c, 's, 'p, 'd> {
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
    fn check_permit(&self, at: usize, permit: &DirtyInstallPermit<'_, '_, '_>) -> Result<()> {
        let pins = self.pins.as_ref().expect("live write targets");
        pins.pool.health()?;
        if at >= pins.len {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        let registration = pins.pool.registration(pins.indices[at])?;
        if !permit.matches_core(pins.pool.core)
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
        permit: &DirtyInstallPermit<'_, '_, '_>,
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
        permit: &DirtyInstallPermit<'_, '_, '_>,
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
        permit: &DirtyInstallPermit<'_, '_, '_>,
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
        if dpt[stamp.index] != expected {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        dpt[stamp.index] = Some(DirtyEntry {
            identity: actual.identity,
            generation: actual.generation,
            rec_lsn: permit.first_rec_lsn(),
            modification_counter: actual.modification_counter,
        });
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
impl Drop for WriteTargets<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(in crate::storage) fn fits() -> bool {
    use std::mem::size_of;
    size_of::<DirtyFrame<'_>>() <= 256
        && size_of::<Option<Registration>>() <= 128
        && size_of::<Option<DirtyEntry>>() <= 128
        && size_of::<PinAccounting>() <= 256
        && size_of::<PinSet<'_, '_, '_, '_, '_>>() <= 64
        && size_of::<ReadTargets<'_, '_, '_, '_, '_, '_>>() <= 256
        && size_of::<WriteTargets<'_, '_, '_, '_, '_, '_>>() <= 256
        && size_of::<DptStamp<'_>>() <= 128
        && size_of::<FrameStamp>() <= 112
        && size_of::<DirtyObservation>() <= 192
        && size_of::<Result<DirtyObservation>>() <= 192
        && size_of::<Result<FrameStamp>>() <= 112
        && size_of::<Option<DirtyEntry>>() <= 96
        && size_of::<Result<&Registration>>() <= 32
        && size_of::<DirtyError>() <= 32
        && size_of::<Result<()>>() <= 32
        && size_of::<MutexGuard<'_, PinAccounting>>() <= 32
        && size_of::<std::sync::LockResult<MutexGuard<'_, PinAccounting>>>() <= 32
        && size_of::<RwLockReadGuard<'_, FrameContent<'_>>>() <= 32
        && size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>() <= 32
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::mpsc::{Receiver, SyncSender};
    use std::time::Duration;

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
            registration_result: size_of::<Result<&Registration>>(),
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
        arrived: SyncSender<()>,
        resume: Receiver<Resume>,
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
        let registration = pool.frames[index].registration.unwrap();
        output.copy_from_slice(content.bytes);
        let entry = pool.dpt.lock().unwrap_or_else(|error| error.into_inner())[index];
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
        pool.dpt.lock().unwrap()[index] = entry;
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
