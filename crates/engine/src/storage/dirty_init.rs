//! One closed payload slot, its linear initializer claim, and whole-set install.
//! The upper producer alone seals reserve/stage/completion authority. No I/O.

use super::*;
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    wal_io::{CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit},
};
use std::sync::LockResult;

#[path = "dirty_init/tx_pair.rs"]
mod tx_pair;
pub(in crate::storage) use tx_pair::source_census as tx_pair_source_census;
pub(in crate::storage) use tx_pair::{CompletedTxPair, TxPairClaimKey, TxPairWriteTargets};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InitClaimRecord {
    allocator: TargetRequest,
    payload: TargetRequest,
    work: SystemWorkRef,
    birth: PageLsn,
    ordinal: u16,
    generations: [u64; 2],
    payload_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitClaimIdentity {
    System(InitClaimRecord),
    TxPair(tx_pair::TxPairClaimRecord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InitClaimSlot {
    identity: InitClaimIdentity,
    pub(super) initializer_pins_held: bool,
}
impl InitClaimSlot {
    #[cfg(test)]
    pub(super) fn diagnostic_registration(&self, index: usize) -> Option<Registration> {
        match &self.identity {
            InitClaimIdentity::System(record) if index == 0 => Some(Registration {
                request: record.allocator,
                generation: record.generations[0],
            }),
            InitClaimIdentity::System(record) if index == record.payload_index => {
                Some(Registration {
                    request: record.payload,
                    generation: record.generations[1],
                })
            }
            InitClaimIdentity::System(_) => None,
            InitClaimIdentity::TxPair(record) => record.diagnostic_registration(index),
        }
    }
    fn system(&self) -> Option<&InitClaimRecord> {
        match &self.identity {
            InitClaimIdentity::System(record) => Some(record),
            InitClaimIdentity::TxPair(_) => None,
        }
    }
    #[cfg(test)]
    fn system_mut(&mut self) -> &mut InitClaimRecord {
        match &mut self.identity {
            InitClaimIdentity::System(record) => record,
            InitClaimIdentity::TxPair(_) => panic!("expected original System claim"),
        }
    }
    fn matches_system(&self, identity: &InitClaimRecord) -> bool {
        self.system() == Some(identity)
    }
    pub(super) fn blocks_load(&self, index: usize, request: TargetRequest) -> bool {
        match &self.identity {
            InitClaimIdentity::System(record) => {
                index == 0 || index == record.payload_index || request.same_address(&record.payload)
            }
            InitClaimIdentity::TxPair(record) => record.blocks_load(index, request),
        }
    }
    pub(super) fn owns_unpublished_pin(&self, index: usize) -> bool {
        self.initializer_pins_held
            && match &self.identity {
                InitClaimIdentity::System(record) => record.payload_index == index,
                InitClaimIdentity::TxPair(record) => record.indices().contains(&index),
            }
    }
}

impl InitClaimRecord {
    fn from_authority(authority: &InitAuthority<'_, '_>) -> Self {
        Self {
            allocator: authority.allocator(),
            payload: authority.payload(),
            work: authority.work(),
            birth: authority.birth(),
            ordinal: authority.ordinal(),
            generations: authority.generations(),
            payload_index: authority.payload_index(),
        }
    }

    fn valid(&self, pool: &SharedPool<'_, '_, '_, '_>) -> Result<bool> {
        let state = pool.entry_registry()?;
        Ok(self.valid_in(pool, &state))
    }
    fn valid_in(&self, pool: &SharedPool<'_, '_, '_, '_>, state: &PoolRegistry<'_>) -> bool {
        state.directory.closed_index() == Some(self.payload_index)
            && self.payload_index > 0
            && self.payload_index < pool.frames.len()
            && (pool.frames.len() == 2
                || state.directory.resident_payloads()
                || state.directory.retention_payloads())
            && state.directory.init_generations(pool.frames) == Some(self.generations)
            && state
                .directory
                .resolve(pool.frames, 0)
                .is_some_and(|registered| {
                    registered.request == self.allocator
                        && registered.generation == self.generations[0]
                })
            && self.allocator.kind == PageKind::Allocation
            && self.allocator.level == 0
            && self.allocator.identity.page.page_no == 0
            && self.allocator.identity.owner.get() == 1
            && self.allocator.identity.owner_generation.get() == 1
            && (if state.directory.retention_payloads() {
                state.directory.accepts_retention_payload(self.payload)
                    && matches!(
                        self.payload.kind,
                        PageKind::TransactionStatus | PageKind::Undo
                    )
            } else {
                state.directory.accepts_payload(self.payload)
            })
            && self.payload.level == 0
            && self.payload.identity.database == self.allocator.identity.database
            && self.payload.identity.page.file == self.allocator.identity.page.file
            && (if state.directory.retention_payloads() {
                (9..64).contains(&self.payload.identity.page.page_no)
            } else if state.directory.operational_payloads() {
                (1..64).contains(&self.payload.identity.page.page_no)
            } else {
                self.payload.identity.page.page_no == 1
            })
            && self.payload.identity.page.incarnation.get() >= 2
            && self.payload.identity.owner_generation.get() == 1
            && self.work.namespace().database == self.allocator.identity.database
            && self.work.kind() == SystemWorkKind::AllocationMaintenance
            && self.work.generation().get() == 1
            && self.birth.0 != 0
            && self.ordinal == 0
            && self.allocator.rank() < self.payload.rank()
            && (0..pool.frames.len())
                .filter(|index| *index != self.payload_index)
                .all(|index| {
                    state
                        .directory
                        .resolve(pool.frames, index)
                        .is_none_or(|entry| !entry.request.same_address(&self.payload))
                })
            && state.directory.published().is_none_or(|published| {
                published.request == self.payload && published.generation == self.generations[1]
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

    fn matches_pool(&self, pool: &SharedPool<'_, '_, '_, '_>) -> Result<bool> {
        let state = pool.entry_registry()?;
        Ok(self.matches_pool_in(pool, &state))
    }
    fn matches_pool_in(&self, pool: &SharedPool<'_, '_, '_, '_>, state: &PoolRegistry<'_>) -> bool {
        self.active
            && std::ptr::eq(self.core, pool.core)
            && std::ptr::eq(self.binding, pool.binding)
            && std::ptr::eq(self.fence, pool.fence)
            && self.identity.valid_in(pool, state)
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
    pub(in crate::storage) fn operational_status(&self) -> Result<bool> {
        self.directory_read(|directory| directory.operational_status())
    }
    pub(in crate::storage) fn operational_payloads(&self) -> Result<bool> {
        self.directory_read(|directory| directory.operational_payloads())
    }
    pub(in crate::storage) fn mixed_payloads(&self) -> Result<bool> {
        self.directory_read(|directory| directory.resident_mixed())
    }
    pub(in crate::storage) fn init_payload_index(&self) -> Result<usize> {
        self.health()?;
        if !self.registrations_valid()? {
            return Err(self.fatal(Cause::Invariant));
        }
        self.directory_read(|directory| directory.closed_index())?
            .ok_or_else(|| self.fatal(Cause::Invariant))
    }
    /// Absence is normal before a live caller prepares its first closed slot.
    /// Reservation and publication still require `init_payload_index`.
    pub(in crate::storage) fn prepared_init_payload_index(&self) -> Result<Option<usize>> {
        self.health()?;
        if !self.registrations_valid()? {
            return Err(self.fatal(Cause::Invariant));
        }
        self.directory_read(|directory| directory.closed_index())
    }
    pub(in crate::storage) fn init_generations(&self) -> Result<[u64; 2]> {
        self.health()?;
        self.directory_read(|directory| directory.init_generations(self.frames))?
            .ok_or_else(|| self.fatal(Cause::Invariant))
    }
    pub(in crate::storage) fn enable_status_operation(&mut self) -> Result<()> {
        self.health()?;
        if self.frames.len() != 2
            || self.directory_read(|directory| directory.closed_index())? != Some(1)
        {
            return Err(unchanged(Cause::Shape));
        }
        self.reload_quiescent()?;
        for index in 0..2 {
            let content = self.frames[index]
                .content
                .try_read()
                .map_err(|error| match error {
                    TryLockError::WouldBlock => unchanged(Cause::Busy),
                    TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
                })?;
            if index == 0
                || self
                    .directory_read(|directory| directory.published())?
                    .is_some()
            {
                self.pair(index, &content)?;
            } else if !content.fresh() {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        if !self.directory_edit(|directory| directory.enable_status_operation(self.frames))? {
            return Err(unchanged(Cause::Shape));
        }
        Ok(())
    }
    /// Retire a clean resident Status before the next reservation. Bytes remain
    /// caller storage, but no lookup can authorize them after publication clears.
    pub(in crate::storage) fn prepare_next_status(&mut self) -> Result<FrameStamp> {
        self.health()?;
        self.reload_quiescent()?;
        if self.initialized_request()?.is_none() {
            return Err(unchanged(Cause::MissingTarget));
        }
        let generation = self
            .directory_read(|directory| directory.next_status_generation())?
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        let mut content = self.frames[1]
            .content
            .try_write()
            .map_err(|error| match error {
                TryLockError::WouldBlock => unchanged(Cause::Busy),
                TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
            })?;
        let before = self.pair(1, &content)?;
        if content.dirty || before.entry.is_some() || before.frame.rec_lsn.is_some() {
            return Err(unchanged(Cause::Busy));
        }
        self.health()?;
        self.directory_edit(|directory| directory.prepare_next_status(generation))?;
        self.init_unpublished.store(true, Ordering::Release);
        content.page_lsn = PageLsn(0);
        content.modification_counter = 0;
        content.rec_lsn = None;
        content.dirty = false;
        Ok(before.frame)
    }
    pub(in crate::storage) fn new_with_closed_init(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>; 2],
    ) -> Result<Self> {
        Self::new_status_slots(core, fence, binding, frames, false)
    }
    pub(in crate::storage) fn new_status_slots(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>],
        resident: bool,
    ) -> Result<Self> {
        Self::new_system_slots(core, fence, binding, frames, resident, false)
    }
    pub(in crate::storage) fn new_mixed_slots(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>],
    ) -> Result<Self> {
        Self::new_system_slots(core, fence, binding, frames, true, true)
    }
    fn new_system_slots(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>],
        resident: bool,
        mixed: bool,
    ) -> Result<Self> {
        if !(2..=MAX_FRAMES).contains(&frames.len()) || (!resident && frames.len() != 2) {
            return Err(unchanged(Cause::Shape));
        }
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
            || frames[1..].iter().any(|frame| frame.registration.is_some())
        {
            return Err(reject(Cause::Invariant));
        }
        for frame in &frames[1..] {
            let content = match frame.content.try_read() {
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
        let pool = Self::new(core, fence, binding, frames)?;
        if mixed {
            pool.directory_edit(|directory| directory.open_resident_mixed())?;
        } else if resident {
            pool.directory_edit(|directory| directory.open_resident_status())?;
        } else {
            pool.directory_edit(|directory| directory.close_status_slot())?;
        }
        pool.init_unpublished.store(true, Ordering::Release);
        Ok(pool)
    }

    pub(super) fn init_pins_valid(&self, pins: &PoolRegistry<'_>) -> bool {
        match &pins.init_claim {
            None => true,
            Some(slot) => match &slot.identity {
                InitClaimIdentity::System(identity) => {
                    identity.valid_in(self, pins)
                        && (!slot.initializer_pins_held
                            || (pins.counts[0] != 0 && pins.counts[identity.payload_index] != 0))
                }
                InitClaimIdentity::TxPair(identity) => {
                    identity.valid_in(self, pins)
                        && (!slot.initializer_pins_held || identity.held_pins_valid(pins))
                }
            },
        }
    }

    /// Unpublished lookup must remain lock-free even while C5 holds DPT/content.
    pub(in crate::storage) fn initialized_request(&self) -> Result<Option<TargetRequest>> {
        self.health()?;
        if self.init_unpublished.load(Ordering::Acquire) {
            return Ok(None);
        }
        let state = self.try_registry()?;
        let index = state
            .directory
            .closed_index()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        Ok(state
            .directory
            .resolve(self.frames, index)
            .map(|entry| entry.request))
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
        if !identity.valid(self)? {
            return Err(unchanged(Cause::Stale));
        }
        {
            let mut pins = self.entry_registry()?;
            if pins.init_claim.is_some()
                || pins
                    .load
                    .as_ref()
                    .is_some_and(|load| load.blocks(identity.payload, identity.payload_index))
            {
                return Err(unchanged(Cause::Busy));
            }
            if !identity.valid_in(self, &pins) {
                return Err(unchanged(Cause::Stale));
            }
            if pins.total > MAX_PINS - 2
                || pins.counts[0] >= MAX_FRAME_PINS
                || pins.counts[identity.payload_index] >= MAX_FRAME_PINS
            {
                return Err(unchanged(Cause::Capacity));
            }
            pins.counts[0] += 1;
            pins.counts[identity.payload_index] += 1;
            pins.total += 2;
            pins.init_claim = Some(InitClaimSlot {
                identity: InitClaimIdentity::System(identity),
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
        if !key.matches_pool(self)?
            || !completed.matches(key)
            || self.initialized_request()? != Some(key.identity.payload)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        let mut pins = self.entry_registry()?;
        if !pins
            .init_claim
            .as_ref()
            .is_some_and(|slot| slot.matches_system(&key.identity) && !slot.initializer_pins_held)
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
        let (mut pins, poison) = match self.pool.registry.lock() {
            Ok(pins) => (pins, None),
            Err(error) => (error.into_inner(), Some(self.pool.fatal(Cause::Poisoned))),
        };
        let result = (|| {
            if !self.pool.pins_valid(&pins)
                || !pins.init_claim.as_ref().is_some_and(|slot| {
                    slot.matches_system(&self.identity) && slot.initializer_pins_held
                })
                || pins.counts[0] == 0
                || pins.counts[self.identity.payload_index] == 0
                || self.retained == self.key.is_some()
            {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            let total = pins
                .total
                .checked_sub(2)
                .ok_or_else(|| self.pool.fatal(Cause::Invariant))?;
            if let Some(key) = &self.key
                && (!key.matches_pool_in(self.pool, &pins) || key.identity != self.identity)
            {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            pins.counts[0] -= 1;
            pins.counts[self.identity.payload_index] -= 1;
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
        let indices = [0, pins.identity.payload_index];
        let mut result = InitWriteTargets {
            guards: [None, None],
            version: Some(version),
            pins: Some(pins),
            installed: false,
        };
        // Admission already proved allocator rank < payload rank.
        for (index, frame_index) in indices.into_iter().enumerate() {
            match pool.frames[frame_index].content.try_write() {
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
        let pins = self.pins.as_ref().expect("owned initializer pins");
        let key = pins
            .key
            .as_ref()
            .ok_or_else(|| pool.fatal(Cause::Invariant))?;
        if !pins.identity.valid(pool)?
            || !key.matches_pool(pool)?
            || key.identity != pins.identity
            || pins.retained
        {
            return Err(pool.fatal(Cause::Invariant));
        }
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
        let registry = pool.entry_registry()?;
        if registry.entries[pins.identity.payload_index].is_some()
            || registry.requirements[pins.identity.payload_index].is_some()
        {
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
            held: pool.registry.lock(),
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
        registry: &PoolRegistry<'s>,
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
            || !key.matches_pool_in(pool, registry)
            || key.identity != pins.identity
            || !requirement.uses_fence(pool.fence)
            || !pool.dpt_valid(registry)
            || registry.directory.published().is_some()
            || before.identity != pins.identity.allocator.identity
            || before.generation != pins.identity.generations[0]
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
        if pool.stamp_in(registry, 0, allocator)? != before
            || !payload.fresh()
            || !pool.dpt_matches(registry, 0, expected_entry.as_ref())
            || registry.requirements[0].map(|receipt| receipt.location().lsn)
                != before.rec_lsn.map(|_| before.page_lsn.0)
            || registry.entries[pins.identity.payload_index].is_some()
            || registry.requirements[pins.identity.payload_index].is_some()
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
                generation: pins.identity.generations[1],
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
    held: LockResult<MutexGuard<'b, PoolRegistry<'s>>>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
}
impl<'s> FencedInitRegistry<'_, '_, 's, '_, '_> {
    fn state(&self) -> Result<&PoolRegistry<'s>> {
        match &self.held {
            Ok(guard) => Ok(guard),
            Err(_) => Err(self.pool.fatal(Cause::Poisoned)),
        }
    }
    fn prechecked_mut(&mut self) -> &mut PoolRegistry<'s> {
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
            let index = self
                .write
                .pins
                .as_ref()
                .expect("prechecked initializer pins")
                .identity
                .payload_index;
            let registry = self.registry.prechecked_mut();
            registry.entries[0] = Some(DptEntry::from_checked(&DirtyEntry {
                identity: allocator_identity,
                generation: self.permit.allocator_before().generation,
                rec_lsn: self.rec_lsn,
                modification_counter: self.counter,
            }));
            registry.entries[index] = Some(DptEntry::from_checked(&DirtyEntry {
                identity: self.registration.request.identity,
                generation: self.registration.generation,
                rec_lsn: lsn,
                modification_counter: 1,
            }));
            registry.requirements[0] = Some(requirement);
            registry.requirements[index] = Some(requirement);
            self.control.mark_registry_installed();
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::RegistryInstalled);
        // Sole initializer, unpublished prechecked; fixed closure cannot fail.
        if !self
            .registry
            .prechecked_mut()
            .directory
            .publish(self.registration)
        {
            pool.fatal(Cause::Invariant);
            return;
        }
        self.control.mark_published();
        pool.init_unpublished.store(false, Ordering::Release);
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
        PairClosedBeforeClaim,
        PairClaimInstalled,
        PairFinishChecked,
        PairCancelChecked,
        PairImagesInstalled,
        PairControlInstalled,
        PairDptInstalled,
        PairFirstRegistered,
        PairBothRegisteredBeforeGate,
        PairGatePublished,
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
    pub(in crate::storage::dirty_frame) fn hit(
        pool: &SharedPool<'_, '_, '_, '_>,
        point: InitPoint,
    ) {
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
            let registry = pool
                .registry
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            (
                [
                    pool.diagnose_dpt(&registry, 0),
                    pool.diagnose_dpt(&registry, 1),
                ],
                [
                    registry.requirements[0].map(WalRequirement::location),
                    registry.requirements[1].map(WalRequirement::location),
                ],
            )
        };
        let pins = pool
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        InitSnapshot {
            published: pins.directory.published().map(|value| value.request),
            claim_payload: pins
                .init_claim
                .and_then(|slot| slot.system().map(|record| record.payload)),
            claim_birth: pins
                .init_claim
                .and_then(|slot| slot.system().map(|record| record.birth)),
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
        let published = pool
            .registry
            .lock()
            .unwrap()
            .directory
            .publish(Registration {
                request,
                generation,
            });
        if published {
            pool.init_unpublished.store(false, Ordering::Release);
        }
        published
    }
    pub(in crate::storage) fn dpt_busy(pool: &SharedPool<'_, '_, '_, '_>) -> bool {
        matches!(pool.registry.try_lock(), Err(TryLockError::WouldBlock))
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
            let registry = pool
                .registry
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            (
                pool.diagnose_dpt(&registry, index),
                registry.requirements[index].map(WalRequirement::location),
            )
        };
        let pins = pool
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
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
            published: pins.directory.published().map(|entry| entry.request),
        }
    }
    /// Registry-only observation, including fenced close/return diagnostics.
    pub(in crate::storage) fn pair_close_state(
        pool: &SharedPool<'_, '_, '_, '_>,
    ) -> (Option<[usize; 2]>, Option<[u64; 2]>, bool, u16) {
        let state = pool.registry.lock().unwrap_or_else(|e| e.into_inner());
        (
            state.directory.closed_pair(),
            state.directory.pair_generations(),
            state.init_claim.is_some(),
            state.total,
        )
    }

    pub(in crate::storage) fn seed_status_generation(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        generation: u64,
    ) {
        pool.registry
            .lock()
            .unwrap()
            .directory
            .seed_status_generation(generation);
    }
    pub(in crate::storage) fn corrupt_local_init_generation(
        write: &mut InitWriteTargets<'_, '_, '_, '_, '_, '_>,
    ) {
        let pins = write.pins.as_mut().unwrap();
        pins.identity.generations[1] = pins.identity.generations[1].wrapping_add(1);
    }
    pub(in crate::storage) fn corrupt_local_init_index(
        write: &mut InitWriteTargets<'_, '_, '_, '_, '_, '_>,
        index: usize,
    ) {
        write.pins.as_mut().unwrap().identity.payload_index = index;
    }
    pub(in crate::storage) fn corrupt_init_key_index(key: &mut InitClaimKey<'_, '_>, index: usize) {
        key.identity.payload_index = index;
    }
    pub(in crate::storage) fn corrupt_init_key_generation(key: &mut InitClaimKey<'_, '_>) {
        key.identity.generations[1] = key.identity.generations[1].wrapping_add(1);
    }
    pub(in crate::storage) fn corrupt_init_claim(pool: &SharedPool<'_, '_, '_, '_>, variant: u8) {
        let mut pins = pool.registry.lock().unwrap();
        let slot = pins
            .init_claim
            .as_mut()
            .expect("existing initializer claim");
        match variant {
            1 => slot.system_mut().birth = PageLsn(0),
            2 => slot.system_mut().ordinal = 1,
            3 => slot.initializer_pins_held = !slot.initializer_pins_held,
            4 => {
                slot.system_mut().generations[1] =
                    slot.system().expect("System claim").generations[1].wrapping_add(1)
            }
            _ => panic!("unknown initializer corruption"),
        }
    }
    /// Leaf-only observation remains usable while the installer owns content/DPT.
    pub(in crate::storage) fn pin_state(
        pool: &SharedPool<'_, '_, '_, '_>,
    ) -> ([u16; MAX_FRAMES], u16, bool, bool) {
        let pins = pool
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (
            pins.counts,
            pins.total,
            pins.init_claim.is_some(),
            pins.init_claim
                .is_some_and(|slot| slot.initializer_pins_held),
        )
    }
    /// Nonblocking observer for the installer-held unified leaf at C6.
    pub(in crate::storage) fn try_pin_state(
        pool: &SharedPool<'_, '_, '_, '_>,
    ) -> Result<([u16; MAX_FRAMES], u16, bool, bool)> {
        let pins = pool.try_registry()?;
        Ok((
            pins.counts,
            pins.total,
            pins.init_claim.is_some(),
            pins.init_claim
                .is_some_and(|slot| slot.initializer_pins_held),
        ))
    }
    /// Test-only damage of an already registered original page. This cannot
    /// publish, register or construct a replacement payload/claim.
    pub(in crate::storage) fn corrupt_retention_content(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        offset: usize,
        value: u8,
    ) {
        pool.frames[index].content.write().unwrap().bytes[offset] = value;
    }
    /// Scalar-only observer: never copies the retained pin counts or pages.
    pub(in crate::storage) fn claim_state(pool: &SharedPool<'_, '_, '_, '_>) -> (bool, u16) {
        let pins = pool
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (pins.init_claim.is_some(), pins.total)
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
