//! One closed payload slot, its linear initializer claim, and whole-set install.
//! The upper producer alone seals reserve/stage/completion authority. No I/O.

use super::*;
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    wal_io::{CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit},
};
use std::sync::LockResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InitClaimRecord {
    allocator: TargetRequest,
    payload: TargetRequest,
    work: SystemWorkRef,
    birth: PageLsn,
    ordinal: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InitClaimSlot {
    identity: InitClaimRecord,
    pub(super) initializer_pins_held: bool,
}

impl InitClaimRecord {
    fn from_authority(authority: &InitAuthority<'_, '_>) -> Self {
        Self {
            allocator: authority.allocator(),
            payload: authority.payload(),
            work: authority.work(),
            birth: authority.birth(),
            ordinal: authority.ordinal(),
        }
    }

    fn valid(&self, pool: &SharedPool<'_, '_, '_, '_>) -> bool {
        pool.directory.closed_index() == Some(1)
            && pool.frames.len() == 2
            && pool.resolve_registration(0).is_some_and(|registered| {
                registered.request == self.allocator && registered.generation == 1
            })
            && self.allocator.kind == PageKind::Allocation
            && self.allocator.level == 0
            && self.allocator.identity.page.page_no == 0
            && self.allocator.identity.owner.get() == 1
            && self.allocator.identity.owner_generation.get() == 1
            && self.payload.kind == PageKind::TransactionStatus
            && self.payload.level == 0
            && self.payload.identity.database == self.allocator.identity.database
            && self.payload.identity.page.file == self.allocator.identity.page.file
            && self.payload.identity.page.page_no == 1
            && self.payload.identity.page.incarnation.get() >= 2
            && self.payload.identity.owner.get() == 2
            && self.payload.identity.owner_generation.get() == 1
            && self.work.namespace().database == self.allocator.identity.database
            && self.work.kind() == SystemWorkKind::AllocationMaintenance
            && self.work.generation().get() == 1
            && self.birth.0 != 0
            && self.ordinal == 0
            && self.allocator.rank() < self.payload.rank()
            && pool.directory.published().is_none_or(|published| {
                published.request == self.payload && published.generation == 1
            })
    }
}

/// The sole claim cleanup owner; origin references are external to the Bound.
pub(in crate::storage) struct InitClaimKey<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    identity: InitClaimRecord,
    active: bool,
}
impl InitClaimKey<'_, '_> {
    pub(in crate::storage) fn matches_authority(&self, authority: &InitAuthority<'_, '_>) -> bool {
        self.active
            && std::ptr::eq(self.core, authority.core())
            && std::ptr::eq(self.binding, authority.binding())
            && std::ptr::eq(self.fence, authority.fence())
            && self.identity == InitClaimRecord::from_authority(authority)
    }

    fn matches_pool(&self, pool: &SharedPool<'_, '_, '_, '_>) -> bool {
        self.active
            && std::ptr::eq(self.core, pool.core)
            && std::ptr::eq(self.binding, pool.binding)
            && std::ptr::eq(self.fence, pool.fence)
            && self.identity.valid(pool)
    }
}
impl Drop for InitClaimKey<'_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.fence.fence();
        }
    }
}

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn new_with_closed_init(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>; 2],
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
        if !fits() {
            return Err(unchanged(Cause::Capacity));
        }
        let reject = |cause| {
            fence.fence();
            DirtyError {
                cause,
                effect: Effect::CallerMustResolve,
            }
        };
        let registered = frames[0]
            .registration
            .ok_or_else(|| reject(Cause::Invariant))?;
        if registered.generation != 1
            || registered.request.kind != PageKind::Allocation
            || registered.request.level != 0
            || registered.request.identity.page.page_no != 0
            || frames[1].registration.is_some()
        {
            return Err(reject(Cause::Invariant));
        }
        {
            let content = match frames[1].content.try_read() {
                Ok(content) => content,
                Err(TryLockError::WouldBlock) => return Err(reject(Cause::Invariant)),
                Err(TryLockError::Poisoned(error)) => {
                    let failure = reject(Cause::Poisoned);
                    drop(error.into_inner());
                    return Err(failure);
                }
            };
            if !content.fresh() {
                return Err(reject(Cause::Invariant));
            }
        }
        let mut pool = Self::new(core, fence, binding, frames)?;
        pool.directory.close_status_slot();
        Ok(pool)
    }

    pub(super) fn init_pins_valid(&self, pins: &PinAccounting) -> bool {
        match &pins.init_claim {
            None => true,
            Some(slot) => {
                slot.identity.valid(self)
                    && (!slot.initializer_pins_held || (pins.counts[0] != 0 && pins.counts[1] != 0))
            }
        }
    }

    /// Unpublished lookup must remain lock-free even while C5 holds DPT/content.
    pub(in crate::storage) fn initialized_request(&self) -> Result<Option<TargetRequest>> {
        self.health()?;
        if self.directory.closed_index() != Some(1) || !self.registrations_valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(self
            .resolve_registration(1)
            .map(|registration| registration.request))
    }

    pub(in crate::storage) fn claim_init_targets<'a, 'b>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        authority: &InitAuthority<'c, 's>,
    ) -> Result<InitTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        self.health()?;
        if !std::ptr::eq(authority.core(), self.core)
            || !std::ptr::eq(authority.binding(), self.binding)
            || !std::ptr::eq(authority.fence(), self.fence)
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if self.initialized_request()?.is_some() {
            return Err(unchanged(Cause::AlreadyInitialized));
        }
        let identity = InitClaimRecord::from_authority(authority);
        if !identity.valid(self) {
            return Err(unchanged(Cause::Stale));
        }
        {
            let mut pins = self.entry_pins()?;
            if pins.init_claim.is_some() {
                return Err(unchanged(Cause::Busy));
            }
            if pins.total > MAX_PINS - 2
                || pins.counts[0] >= MAX_FRAME_PINS
                || pins.counts[1] >= MAX_FRAME_PINS
            {
                return Err(unchanged(Cause::Capacity));
            }
            pins.counts[0] += 1;
            pins.counts[1] += 1;
            pins.total += 2;
            pins.init_claim = Some(InitClaimSlot {
                identity,
                initializer_pins_held: true,
            });
        }
        Ok(InitTargets {
            owner: Some(owner),
            pins: Some(InitPinSet {
                pool: self,
                identity,
                active: true,
                retained: false,
                key: Some(InitClaimKey {
                    core: self.core,
                    binding: self.binding,
                    fence: self.fence,
                    identity,
                    active: true,
                }),
                _thread: PhantomData,
            }),
        })
    }

    pub(in crate::storage) fn finish_init_claim(
        &self,
        key: &mut InitClaimKey<'c, 's>,
        completed: &CompletedInitAuthority<'c, 's>,
    ) -> Result<()> {
        self.health()?;
        if !key.matches_pool(self)
            || !completed.matches(key)
            || self.initialized_request()? != Some(key.identity.payload)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        let mut pins = self.entry_pins()?;
        if !pins
            .init_claim
            .as_ref()
            .is_some_and(|slot| slot.identity == key.identity && !slot.initializer_pins_held)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        // Other published readers may hold either/both target pins here.
        pins.init_claim = None;
        key.active = false;
        Ok(())
    }
}

struct InitPinSet<'b, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    identity: InitClaimRecord,
    key: Option<InitClaimKey<'c, 's>>,
    active: bool,
    retained: bool,
    _thread: PhantomData<Rc<()>>,
}
impl InitPinSet<'_, '_, '_, '_, '_> {
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
            if !self.pool.pins_valid(&pins)
                || !pins.init_claim.as_ref().is_some_and(|slot| {
                    slot.identity == self.identity && slot.initializer_pins_held
                })
                || pins.counts[0] == 0
                || pins.counts[1] == 0
                || self.retained == self.key.is_some()
            {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            let total = pins
                .total
                .checked_sub(2)
                .ok_or_else(|| self.pool.fatal(Cause::Invariant))?;
            if let Some(key) = &self.key
                && (!key.matches_pool(self.pool) || key.identity != self.identity)
            {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            pins.counts[0] -= 1;
            pins.counts[1] -= 1;
            pins.total = total;
            if self.retained {
                pins.init_claim
                    .as_mut()
                    .expect("prechecked initializer slot")
                    .initializer_pins_held = false;
            } else {
                pins.init_claim = None;
                self.key
                    .as_mut()
                    .expect("prechecked local claim key")
                    .active = false;
            }
            Ok(())
        })();
        combine(poison.map_or(Ok(()), Err), result)
    }
}
impl Drop for InitPinSet<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(in crate::storage) struct InitTargets<'a, 'b, 'c, 's, 'p, 'd> {
    owner: Option<&'a mut Owner<'c, 's>>,
    pins: Option<InitPinSet<'b, 'c, 's, 'p, 'd>>,
}
impl<'a, 'b, 'c, 's, 'p, 'd> InitTargets<'a, 'b, 'c, 's, 'p, 'd> {
    pub(in crate::storage) fn try_write(
        mut self,
    ) -> Result<InitWriteTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        let mut pins = self.pins.take().expect("owned initializer pins");
        let owner = self.owner.take().expect("owned initializer owner");
        let version = match owner.try_apply().and_then(|apply| apply.try_version()) {
            Ok(version) => version,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let pool = pins.pool;
        let mut result = InitWriteTargets {
            guards: [None, None],
            version: Some(version),
            pins: Some(pins),
            installed: false,
        };
        // Admission already proved allocator rank < payload rank.
        for index in 0..2 {
            match pool.frames[index].content.try_write() {
                Ok(guard) => result.guards[index] = Some(guard),
                Err(TryLockError::WouldBlock) => return result.reject(unchanged(Cause::Busy)),
                Err(TryLockError::Poisoned(error)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(error.into_inner());
                    return result.reject(failure);
                }
            }
        }
        if let Err(error) = result.check_before(None) {
            return result.reject(error);
        }
        Ok(result)
    }

    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.owner.take();
        self.pins.as_mut().map_or(Ok(()), InitPinSet::release)
    }
}

pub(in crate::storage) struct InitWriteTargets<'a, 'b, 'c, 's, 'p, 'd> {
    guards: [Option<RwLockWriteGuard<'b, FrameContent<'d>>>; 2],
    version: Option<VersionEditGuard<'a, 'c, 's>>,
    pins: Option<InitPinSet<'b, 'c, 's, 'p, 'd>>,
    installed: bool,
}
impl<'a, 'b, 'c, 's, 'p, 'd> InitWriteTargets<'a, 'b, 'c, 's, 'p, 'd> {
    fn pool(&self) -> &'b SharedPool<'c, 's, 'p, 'd> {
        self.pins.as_ref().expect("owned initializer pins").pool
    }
    fn check_before(&self, expected: Option<&FrameStamp>) -> Result<FrameStamp> {
        let pool = self.pool();
        pool.health()?;
        if pool.initialized_request()?.is_some() {
            return Err(pool.fatal(Cause::Invariant));
        }
        let allocator = self.guards[0].as_ref().expect("held allocator content");
        let closed = self.guards[1].as_ref().expect("held closed content");
        if !closed.fresh() {
            return Err(pool.fatal(Cause::Invariant));
        }
        let actual = pool.pair(0, allocator)?.frame;
        if expected.is_some_and(|expected| actual != *expected) {
            return Err(unchanged(Cause::Stale));
        }
        if actual.modification_counter == u64::MAX {
            return Err(unchanged(Cause::CounterExhausted));
        }
        let registry = pool.entry_dpt()?;
        if registry.entries[1].is_some() || registry.requirements[1].is_some() {
            return Err(pool.fatal(Cause::Invariant));
        }
        Ok(actual)
    }

    pub(in crate::storage) fn copy_allocator_snapshot(
        &self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<FrameStamp> {
        let stamp = self.check_before(None)?;
        output.copy_from_slice(
            self.guards[0]
                .as_ref()
                .expect("held allocator content")
                .bytes,
        );
        Ok(stamp)
    }

    pub(in crate::storage) fn preflight(&self, allocator_before: &FrameStamp) -> Result<()> {
        self.check_before(Some(allocator_before)).map(|_| ())
    }

    /// Called immediately after actual stage; the caller stores this in O next.
    pub(in crate::storage) fn retain_claim(&mut self) -> InitClaimKey<'c, 's> {
        let pins = self.pins.as_mut().expect("owned initializer pins");
        let key = pins.key.take().expect("sole initializer claim key");
        pins.retained = true;
        key
    }

    pub(in crate::storage) fn prepare_install<'i>(
        &'i mut self,
        permit: &'i InitInstallPermit<'i, 'c, 's>,
        control: InitControlCommit<'i>,
    ) -> Result<PreparedInitSet<'i, 'a, 'b, 'c, 's, 'p, 'd>> {
        let pool = self.pool();
        // Own even PoisonError's guard immediately; no raw-guard take/local.
        let registry = FencedInitRegistry {
            held: pool.dpt.lock(),
            pool,
        };
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::DptGuardOwnedBeforePrecheck);
        let (counter, rec_lsn, registration) = self.check_install(permit, registry.state()?)?;
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::DptPrecheckedBeforePrepared);
        Ok(PreparedInitSet {
            write: self,
            permit,
            control,
            registry,
            counter,
            rec_lsn,
            registration,
            installed: false,
        })
    }

    fn check_install(
        &self,
        permit: &InitInstallPermit<'_, 'c, 's>,
        registry: &DirtyRegistry<'s>,
    ) -> Result<(u64, PageLsn, Registration)> {
        let pool = self.pool();
        pool.health()?;
        let pins = self.pins.as_ref().expect("owned initializer pins");
        let key = permit.claim();
        let before = permit.allocator_before();
        let requirement = permit.requirement();
        let lsn = PageLsn(requirement.location().lsn);
        if !pins.retained
            || pins.key.is_some()
            || !key.matches_pool(pool)
            || key.identity != pins.identity
            || !requirement.uses_fence(pool.fence)
            || !pool.dpt_valid(registry)
            || pool.directory.published().is_some()
            || before.identity != pins.identity.allocator.identity
            || before.generation != 1
            || lsn.0 <= before.page_lsn.0
            || lsn.0 <= pins.identity.birth.0
        {
            return Err(pool.fatal(Cause::Invariant));
        }
        let allocator = self.guards[0].as_ref().expect("held allocator content");
        let payload = self.guards[1].as_ref().expect("held payload content");
        let expected_entry = before.rec_lsn.map(|rec_lsn| DirtyEntry {
            identity: before.identity,
            generation: before.generation,
            rec_lsn,
            modification_counter: before.modification_counter,
        });
        if pool.stamp(0, allocator)? != before
            || !payload.fresh()
            || registry.entries[0] != expected_entry
            || registry.requirements[0].map(|receipt| receipt.location().lsn)
                != before.rec_lsn.map(|_| before.page_lsn.0)
            || registry.entries[1].is_some()
            || registry.requirements[1].is_some()
            || u64::from_le_bytes(
                permit.allocator_after()[64..72]
                    .try_into()
                    .expect("fixed LSN field"),
            ) != lsn.0
            || u64::from_le_bytes(
                permit.status_after()[64..72]
                    .try_into()
                    .expect("fixed LSN field"),
            ) != lsn.0
        {
            return Err(pool.fatal(Cause::Invariant));
        }
        let counter = before
            .modification_counter
            .checked_add(1)
            .ok_or_else(|| pool.fatal(Cause::CounterExhausted))?;
        let rec_lsn = before.rec_lsn.unwrap_or(lsn);
        if rec_lsn.0 == 0 || rec_lsn.0 > lsn.0 {
            return Err(pool.fatal(Cause::Invariant));
        }
        Ok((
            counter,
            rec_lsn,
            Registration {
                request: pins.identity.payload,
                generation: 1,
            },
        ))
    }

    fn release(&mut self) -> Result<()> {
        let pool = self.pool();
        pool.fence_unwind();
        let unfinished = self.pins.as_ref().is_some_and(|pins| pins.retained) && !self.installed;
        let original = if unfinished {
            Err(pool.fatal(Cause::Invariant))
        } else {
            Ok(())
        };
        #[cfg(test)]
        if std::thread::panicking() && self.guards.iter().any(Option::is_some) {
            super::test_support::fenced(pool);
        }
        for guard in self.guards.iter_mut().rev() {
            guard.take();
        }
        #[cfg(test)]
        if self.pins.as_ref().is_some_and(|pins| pins.active) {
            super::test_support::before_release(pool);
        }
        let version = self.version.take().map_or(Ok(()), |version| {
            version.release_checked().map_err(DirtyError::from)
        });
        let pins = self.pins.as_mut().map_or(Ok(()), InitPinSet::release);
        combine(original, combine(version, pins))
    }
    fn reject<T>(mut self, error: DirtyError) -> Result<T> {
        combine(Err(error), self.release()).and_then(|()| unreachable!())
    }
    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}
impl Drop for InitWriteTargets<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

struct FencedInitRegistry<'b, 'c, 's, 'p, 'd> {
    held: LockResult<MutexGuard<'b, DirtyRegistry<'s>>>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
}
impl<'s> FencedInitRegistry<'_, '_, 's, '_, '_> {
    fn state(&self) -> Result<&DirtyRegistry<'s>> {
        match &self.held {
            Ok(guard) => Ok(guard),
            Err(_) => Err(self.pool.fatal(Cause::Poisoned)),
        }
    }
    fn prechecked_mut(&mut self) -> &mut DirtyRegistry<'s> {
        match &mut self.held {
            Ok(guard) => guard,
            Err(_) => unreachable!("registry poison rejected before installation"),
        }
    }
}
impl Drop for FencedInitRegistry<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        self.pool.fence_unwind();
        #[cfg(test)]
        if std::thread::panicking() {
            super::test_support::fenced(self.pool);
        }
        // held (including a poison guard) is destroyed only after this body.
    }
}

pub(in crate::storage) struct PreparedInitSet<'i, 'a, 'b, 'c, 's, 'p, 'd> {
    write: &'i mut InitWriteTargets<'a, 'b, 'c, 's, 'p, 'd>,
    permit: &'i InitInstallPermit<'i, 'c, 's>,
    control: InitControlCommit<'i>,
    registry: FencedInitRegistry<'b, 'c, 's, 'p, 'd>,
    counter: u64,
    rec_lsn: PageLsn,
    registration: Registration,
    installed: bool,
}
impl PreparedInitSet<'_, '_, '_, '_, '_, '_, '_> {
    pub(in crate::storage) fn install(mut self) {
        let pool = self.registry.pool;
        let requirement = self.permit.requirement();
        let lsn = PageLsn(requirement.location().lsn);
        let allocator_identity = self.permit.allocator_before().identity;
        {
            let allocator = self.write.guards[0]
                .as_mut()
                .expect("prechecked allocator guard");
            allocator
                .bytes
                .copy_from_slice(self.permit.allocator_after());
            allocator.page_lsn = lsn;
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::AllocatorImageInstalled);
        {
            let payload = self.write.guards[1]
                .as_mut()
                .expect("prechecked payload guard");
            payload.bytes.copy_from_slice(self.permit.status_after());
            payload.page_lsn = lsn;
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::PayloadImageInstalled);
        {
            let allocator = self.write.guards[0]
                .as_mut()
                .expect("prechecked allocator guard");
            allocator.modification_counter = self.counter;
            allocator.rec_lsn = Some(self.rec_lsn);
            allocator.dirty = true;
            let payload = self.write.guards[1]
                .as_mut()
                .expect("prechecked payload guard");
            payload.modification_counter = 1;
            payload.rec_lsn = Some(lsn);
            payload.dirty = true;
            self.control.install_descriptor();
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::MetadataInstalled);
        {
            let registry = self.registry.prechecked_mut();
            registry.entries[0] = Some(DirtyEntry {
                identity: allocator_identity,
                generation: 1,
                rec_lsn: self.rec_lsn,
                modification_counter: self.counter,
            });
            registry.entries[1] = Some(DirtyEntry {
                identity: self.registration.request.identity,
                generation: 1,
                rec_lsn: lsn,
                modification_counter: 1,
            });
            registry.requirements[0] = Some(requirement);
            registry.requirements[1] = Some(requirement);
            self.control.mark_registry_installed();
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::RegistryInstalled);
        // Sole initializer, unpublished prechecked; fixed closure cannot fail.
        if !pool.directory.publish(self.registration) {
            pool.fatal(Cause::Invariant);
            return;
        }
        self.control.mark_published();
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::Published);
        self.write.installed = true;
        self.installed = true;
    }
}
impl Drop for PreparedInitSet<'_, '_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.installed {
            self.registry.pool.fence.fence();
        }
    }
}

/// The exact previous retained shape is layout-only; no second pool is created.
#[allow(dead_code)]
struct PriorPinAccounting {
    counts: [u16; MAX_FRAMES],
    total: u16,
}
#[allow(dead_code)]
struct PriorPool<'c, 's, 'p, 'd> {
    frames: &'p [DirtyFrame<'d>],
    directory: [Option<Registration>; MAX_FRAMES],
    pins: Mutex<PriorPinAccounting>,
    dpt: Mutex<DirtyRegistry<'s>>,
    core: &'c PageConcurrency<'s>,
    fence: &'s SessionFence,
    binding: &'s AtomicU8,
    stamp_anchor: u8,
    #[cfg(test)]
    fence_pause: Mutex<Option<super::test_support::FencePause>>,
    #[cfg(test)]
    release_pause: Mutex<Option<super::test_support::FencePause>>,
    #[cfg(test)]
    flush_pause: Mutex<
        Option<(
            super::test_support::FlushPoint,
            super::test_support::FencePause,
        )>,
    >,
}
pub(super) fn pool_delta_bytes() -> usize {
    std::mem::size_of::<SharedPool<'_, '_, '_, '_>>()
        .saturating_sub(std::mem::size_of::<PriorPool<'_, '_, '_, '_>>())
}
pub(super) fn fits() -> bool {
    use std::mem::size_of;
    pool_delta_bytes() <= INIT_POOL_DELTA_LIMIT
        && size_of::<InitClaimRecord>() <= 256
        && size_of::<Option<InitClaimSlot>>() <= 256
        && size_of::<InitPinSet<'_, '_, '_, '_, '_>>() <= 512
        && size_of::<VersionEditGuard<'_, '_, '_>>() <= 128
        && size_of::<InitClaimKey<'_, '_>>() <= 256
        && size_of::<Result<InitClaimKey<'_, '_>>>() <= 256
        && size_of::<InitTargets<'_, '_, '_, '_, '_, '_>>() <= 640
        && size_of::<Result<InitTargets<'_, '_, '_, '_, '_, '_>>>() <= 640
        && size_of::<InitWriteTargets<'_, '_, '_, '_, '_, '_>>() <= 640
        && size_of::<Result<InitWriteTargets<'_, '_, '_, '_, '_, '_>>>() <= 640
        && size_of::<PreparedInitSet<'_, '_, '_, '_, '_, '_, '_>>() <= 1024
        && size_of::<Result<PreparedInitSet<'_, '_, '_, '_, '_, '_, '_>>>() <= 1024
        && size_of::<FencedInitRegistry<'_, '_, '_, '_, '_>>() <= 64
}

#[cfg(test)]
pub(in crate::storage) mod test_support {
    use super::*;
    use std::sync::mpsc::{Receiver, SyncSender};
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::storage) enum InitPoint {
        StageOwned,
        DptGuardOwnedBeforePrecheck,
        DptPrecheckedBeforePrepared,
        AllocatorImageInstalled,
        PayloadImageInstalled,
        MetadataInstalled,
        RegistryInstalled,
        Published,
    }
    pub(in crate::storage) fn stage_owned(pool: &SharedPool<'_, '_, '_, '_>) {
        hit(pool, InitPoint::StageOwned);
    }
    pub(in crate::storage) fn install_init_pause(
        pool: &SharedPool<'_, '_, '_, '_>,
        point: InitPoint,
        arrived: SyncSender<()>,
        resume: Receiver<super::super::test_support::Resume>,
    ) {
        *pool.init_pause.lock().unwrap() = Some((
            point,
            super::super::test_support::FencePause { arrived, resume },
        ));
    }
    pub(super) fn hit(pool: &SharedPool<'_, '_, '_, '_>, point: InitPoint) {
        let pause = {
            let mut held = pool
                .init_pause
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if held.as_ref().is_some_and(|(at, _)| *at == point) {
                held.take()
            } else {
                None
            }
        };
        if let Some((_, pause)) = pause {
            pause.arrived.try_send(()).expect("bounded INIT arrival");
            match pause
                .resume
                .recv_timeout(Duration::from_secs(5))
                .expect("bounded INIT release")
            {
                super::super::test_support::Resume::Continue => {}
                super::super::test_support::Resume::Panic => panic!("requested INIT unwind"),
            }
        }
    }

    #[derive(Debug)]
    pub(in crate::storage) struct InitSnapshot {
        pub(in crate::storage) published: Option<TargetRequest>,
        pub(in crate::storage) claim_payload: Option<TargetRequest>,
        pub(in crate::storage) claim_birth: Option<PageLsn>,
        pub(in crate::storage) initializer_pins_held: bool,
        pub(in crate::storage) pins: [u16; MAX_FRAMES],
        pub(in crate::storage) total: u16,
        pub(in crate::storage) page_lsns: [PageLsn; 2],
        pub(in crate::storage) counters: [u64; 2],
        pub(in crate::storage) rec_lsns: [Option<PageLsn>; 2],
        pub(in crate::storage) dirty: [bool; 2],
        pub(in crate::storage) entries: [Option<DirtyEntry>; 2],
        pub(in crate::storage) requirements: [Option<crate::storage::wal_io::WalFrameLocation>; 2],
    }
    /// Quiescent diagnostics only: each lock scope ends before acquiring pins.
    pub(in crate::storage) fn init_diagnose(
        pool: &SharedPool<'_, '_, '_, '_>,
        allocator_output: &mut [u8; PAGE_BYTES],
        status_output: &mut [u8; PAGE_BYTES],
    ) -> InitSnapshot {
        let mut page_lsns = [PageLsn(0); 2];
        let mut counters = [0; 2];
        let mut rec_lsns = [None; 2];
        let mut dirty = [false; 2];
        for (index, output) in [allocator_output, status_output].into_iter().enumerate() {
            let content = pool.frames[index]
                .content
                .read()
                .unwrap_or_else(|error| error.into_inner());
            output.copy_from_slice(content.bytes);
            page_lsns[index] = content.page_lsn;
            counters[index] = content.modification_counter;
            rec_lsns[index] = content.rec_lsn;
            dirty[index] = content.dirty;
        }
        let (entries, requirements) = {
            let registry = pool.dpt.lock().unwrap_or_else(|error| error.into_inner());
            (
                [registry.entries[0], registry.entries[1]],
                [
                    registry.requirements[0].map(WalRequirement::location),
                    registry.requirements[1].map(WalRequirement::location),
                ],
            )
        };
        let pins = pool.pins.lock().unwrap_or_else(|error| error.into_inner());
        InitSnapshot {
            published: pool.directory.published().map(|value| value.request),
            claim_payload: pins.init_claim.map(|slot| slot.identity.payload),
            claim_birth: pins.init_claim.map(|slot| slot.identity.birth),
            initializer_pins_held: pins
                .init_claim
                .is_some_and(|slot| slot.initializer_pins_held),
            pins: pins.counts,
            total: pins.total,
            page_lsns,
            counters,
            rec_lsns,
            dirty,
            entries,
            requirements,
        }
    }
    pub(in crate::storage) fn seed_closed_metadata(
        pool: &SharedPool<'_, '_, '_, '_>,
        lsn: PageLsn,
        counter: u64,
        rec_lsn: Option<PageLsn>,
        dirty: bool,
    ) {
        let mut content = pool.frames[1].content.write().unwrap();
        content.page_lsn = lsn;
        content.modification_counter = counter;
        content.rec_lsn = rec_lsn;
        content.dirty = dirty;
    }
    /// Pre-bind corruption of immutable original registration; never live authority.
    pub(in crate::storage) fn seed_original_registration(
        frame: &mut DirtyFrame<'_>,
        request: TargetRequest,
        generation: u64,
    ) {
        frame.registration = Some(Registration {
            request,
            generation,
        });
    }
    pub(in crate::storage) fn seed_publication(
        pool: &SharedPool<'_, '_, '_, '_>,
        request: TargetRequest,
        generation: u64,
    ) -> bool {
        pool.directory.publish(Registration {
            request,
            generation,
        })
    }
    pub(in crate::storage) fn dpt_busy(pool: &SharedPool<'_, '_, '_, '_>) -> bool {
        matches!(pool.dpt.try_lock(), Err(TryLockError::WouldBlock))
    }
    /// Actual test-only held content guard with a bounded controller; no fake Busy.
    pub(in crate::storage) fn hold_content(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        arrived: SyncSender<()>,
        release: Receiver<()>,
    ) {
        let _guard = pool.frames[index].content.read().unwrap();
        arrived.try_send(()).expect("bounded held-content arrival");
        release
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded held-content release");
    }
    #[derive(Debug, PartialEq, Eq)]
    pub(in crate::storage) struct RawSnapshot {
        pub(in crate::storage) page_lsn: PageLsn,
        pub(in crate::storage) counter: u64,
        pub(in crate::storage) rec_lsn: Option<PageLsn>,
        pub(in crate::storage) dirty: bool,
        pub(in crate::storage) entry: Option<DirtyEntry>,
        pub(in crate::storage) requirement: Option<crate::storage::wal_io::WalFrameLocation>,
        pub(in crate::storage) pins: [u16; MAX_FRAMES],
        pub(in crate::storage) total: u16,
        pub(in crate::storage) claim_present: bool,
        pub(in crate::storage) initializer_pins_held: bool,
        pub(in crate::storage) published: Option<TargetRequest>,
    }
    /// Quiescent raw diagnostics also work while the payload is unpublished.
    pub(in crate::storage) fn snapshot(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        output: &mut [u8; PAGE_BYTES],
    ) -> RawSnapshot {
        let (page_lsn, counter, rec_lsn, dirty) = {
            let content = pool.frames[index]
                .content
                .read()
                .unwrap_or_else(|error| error.into_inner());
            output.copy_from_slice(content.bytes);
            (
                content.page_lsn,
                content.modification_counter,
                content.rec_lsn,
                content.dirty,
            )
        };
        let (entry, requirement) = {
            let registry = pool.dpt.lock().unwrap_or_else(|error| error.into_inner());
            (
                registry.entries[index],
                registry.requirements[index].map(WalRequirement::location),
            )
        };
        let pins = pool.pins.lock().unwrap_or_else(|error| error.into_inner());
        RawSnapshot {
            page_lsn,
            counter,
            rec_lsn,
            dirty,
            entry,
            requirement,
            pins: pins.counts,
            total: pins.total,
            claim_present: pins.init_claim.is_some(),
            initializer_pins_held: pins
                .init_claim
                .is_some_and(|slot| slot.initializer_pins_held),
            published: pool.directory.published().map(|entry| entry.request),
        }
    }
    pub(in crate::storage) fn corrupt_init_claim(pool: &SharedPool<'_, '_, '_, '_>, variant: u8) {
        let mut pins = pool.pins.lock().unwrap();
        let slot = pins
            .init_claim
            .as_mut()
            .expect("existing initializer claim");
        match variant {
            1 => slot.identity.birth = PageLsn(0),
            2 => slot.identity.ordinal = 1,
            3 => slot.initializer_pins_held = !slot.initializer_pins_held,
            _ => panic!("unknown initializer corruption"),
        }
    }
    /// Leaf-only observation remains usable while the installer owns content/DPT.
    pub(in crate::storage) fn pin_state(
        pool: &SharedPool<'_, '_, '_, '_>,
    ) -> ([u16; MAX_FRAMES], u16, bool, bool) {
        let pins = pool.pins.lock().unwrap_or_else(|error| error.into_inner());
        (
            pins.counts,
            pins.total,
            pins.init_claim.is_some(),
            pins.init_claim
                .is_some_and(|slot| slot.initializer_pins_held),
        )
    }
    pub(in crate::storage) fn init_layout() -> [(&'static str, usize); 15] {
        use std::mem::size_of;
        [
            ("pool_delta", pool_delta_bytes()),
            ("claim_record", size_of::<InitClaimRecord>()),
            ("claim_slot", size_of::<Option<InitClaimSlot>>()),
            ("init_pin_set", size_of::<InitPinSet<'_, '_, '_, '_, '_>>()),
            ("version_guard", size_of::<VersionEditGuard<'_, '_, '_>>()),
            ("claim_key", size_of::<InitClaimKey<'_, '_>>()),
            (
                "claim_key_result",
                size_of::<Result<InitClaimKey<'_, '_>>>(),
            ),
            ("targets", size_of::<InitTargets<'_, '_, '_, '_, '_, '_>>()),
            (
                "targets_result",
                size_of::<Result<InitTargets<'_, '_, '_, '_, '_, '_>>>(),
            ),
            (
                "write",
                size_of::<InitWriteTargets<'_, '_, '_, '_, '_, '_>>(),
            ),
            (
                "write_result",
                size_of::<Result<InitWriteTargets<'_, '_, '_, '_, '_, '_>>>(),
            ),
            (
                "prepared",
                size_of::<PreparedInitSet<'_, '_, '_, '_, '_, '_, '_>>(),
            ),
            (
                "prepared_result",
                size_of::<Result<PreparedInitSet<'_, '_, '_, '_, '_, '_, '_>>>(),
            ),
            (
                "fenced_registry",
                size_of::<FencedInitRegistry<'_, '_, '_, '_, '_>>(),
            ),
            ("accounting", size_of::<PinAccounting>()),
        ]
    }
}
