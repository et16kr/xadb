//! Atomic original-pool lifecycle for one five-page structural group.
//! The writer's sealed capabilities carry semantics and selected-I/O exclusion.
use super::*;
use crate::storage::wal_io::{
    DurableWal, StructuralAuthority, StructuralCancelPermit, StructuralControlCommit,
    StructuralInstallPermit, StructuralStageRejection,
};

const TARGETS: usize = 5;

/// Original atomic leaves and whole-write continuation. Caller authority,
/// immutable prepared images and I/O lease are counted by their producer.
pub(in crate::storage) fn source_bytes() -> usize {
    use std::mem::size_of;
    let entry = super::super::super::retention_residency::entry_source_phases();
    // rank/lookup follow registry validation. Apply admission, version
    // acquisition and failed-acquisition cleanup are consecutive nested calls.
    let rank_lookup = 2 * size_of::<[u8; 24]>()
        + 2 * size_of::<(u64, u64, u64)>()
        + 2 * size_of::<crate::storage::page::PageRef>()
        + 4 * size_of::<Option<Registration>>()
        + 4 * size_of::<Result<Registration>>()
        + 16 * size_of::<&()>();
    let admission = size_of::<concurrency::ApplyAdmission<'_, '_, '_>>()
        + 2 * size_of::<concurrency::Result<concurrency::ApplyAdmission<'_, '_, '_>>>()
        + size_of::<VersionEditGuard<'_, '_, '_>>()
        + 2 * size_of::<concurrency::Result<VersionEditGuard<'_, '_, '_>>>()
        + 4 * size_of::<MutexGuard<'_, ()>>()
        + 4 * size_of::<concurrency::Result<()>>()
        + 32 * size_of::<&()>()
        + 16 * size_of::<usize>();
    let shared = 2 * size_of::<StructuralClaimRecord>()
        + 2 * size_of::<StructuralClaimKey<'_, '_>>()
        + size_of::<Result<StructuralClaimKey<'_, '_>>>()
        + size_of::<FencedInitRegistry<'_, '_, '_, '_, '_>>()
        + 2 * size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
        + 3 * size_of::<[usize; 2]>()
        + 2 * size_of::<[u64; 2]>()
        + 4 * size_of::<TargetRequest>()
        + 4 * size_of::<Registration>()
        + 3 * size_of::<Option<Registration>>()
        + 3 * size_of::<Result<Registration>>()
        + 4 * size_of::<Result<DirtyObservation>>()
        + 4 * size_of::<Result<FrameStamp>>()
        + 4 * size_of::<Option<FrameStamp>>()
        + 8 * size_of::<Result<()>>()
        + 32 * size_of::<&()>()
        + 24 * size_of::<u64>()
        + entry
            .0
            .max(entry.1)
            .max(entry.2)
            .max(dpt_call_bytes())
            .max(rank_lookup)
            .max(admission)
            .max(pin_release_source_bytes());
    let atomic = 2 * size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
        + size_of::<Option<RwLockWriteGuard<'_, FrameContent<'_>>>>()
        + size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
        + size_of::<StructuralCancelPermit<'_, '_, '_, '_>>()
        + size_of::<CompletedStructural<'_, '_>>()
        + size_of::<[Registration; 2]>()
        + 16 * size_of::<&()>()
        + 16 * size_of::<usize>();
    let write = 2 * size_of::<Result<StructuralWriteTargets<'_, '_, '_, '_, '_, '_, '_>>>()
        + size_of::<StructuralWriteTargets<'_, '_, '_, '_, '_, '_, '_>>()
        + size_of::<StructuralTargets<'_, '_, '_, '_, '_, '_, '_>>()
        + size_of::<Result<StructuralTargets<'_, '_, '_, '_, '_, '_, '_>>>()
        + size_of::<StructuralPins<'_, '_, '_, '_, '_, '_>>()
        + 2 * size_of::<CompletedStructural<'_, '_>>()
        + size_of::<Result<CompletedStructural<'_, '_>>>()
        + size_of::<StructuralControlCommit<'_, '_, '_, '_>>()
        + size_of::<StructuralInstallPermit<'_, '_, '_>>()
        + size_of::<[usize; TARGETS]>()
        + size_of::<[u64; TARGETS]>()
        + 2 * size_of::<DirtyEntry>()
        + 2 * size_of::<DptEntry>()
        + size_of::<Option<DirtyEntry>>()
        + size_of::<[Registration; 2]>()
        + 24 * size_of::<&()>()
        + 16 * size_of::<usize>();
    shared + atomic.max(write)
}

impl StructuralClaimRecord {
    fn init_at(&self, request: TargetRequest) -> Option<usize> {
        self.init[..usize::from(self.init_count)]
            .iter()
            .position(|value| *value == request)
    }
    fn indices(&self) -> [usize; 2] {
        self.indices.map(usize::from)
    }
    fn published(&self, state: &PoolRegistry<'_>) -> bool {
        if self.init_count == 2 {
            state.directory.pair_published()
        } else {
            state.directory.published().is_some()
        }
    }
}

/// Linear cleanup ownership. Exact requests supplement the slot mask; no
/// unrelated target set can borrow this key even when it has the same mask.
pub(in crate::storage) struct StructuralClaimKey<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    identity: StructuralClaimRecord,
    targets: [TargetRequest; TARGETS],
    active: bool,
}
impl StructuralClaimKey<'_, '_> {
    fn matches_pool_in(&self, pool: &SharedPool<'_, '_, '_, '_>, state: &PoolRegistry<'_>) -> bool {
        self.active
            && std::ptr::eq(self.core, pool.core)
            && pool.uses_storage_origin(self.binding, self.fence)
            && state
                .init_claim
                .is_some_and(|slot| slot.identity == InitClaimIdentity::Structural(self.identity))
            && self.identity.valid_in(pool, state)
    }
    fn matches_pool(&self, pool: &SharedPool<'_, '_, '_, '_>) -> Result<bool> {
        let state = pool.entry_registry()?;
        Ok(self.matches_pool_in(pool, &state))
    }
    pub(in crate::storage) fn matches_authority(
        &self,
        authority: &StructuralAuthority<'_, '_, '_>,
    ) -> bool {
        self.active
            && std::ptr::eq(self.core, authority.core())
            && std::ptr::eq(self.binding, authority.binding())
            && std::ptr::eq(self.fence, authority.fence())
            && self.identity.work == authority.work()
            && self.identity.birth == authority.birth()
            && self.identity.init == *authority.init()
            && usize::from(self.identity.init_count) == authority.count()
            && self.targets == *authority.targets()
    }
}
impl Drop for StructuralClaimKey<'_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.fence.fence();
        }
    }
}

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn prepare_structural(
        &self,
        authority: &StructuralAuthority<'_, 'c, 's>,
        selected: &StructuralCandidates<'_, 'c, 's>,
    ) -> Result<StructuralClaimKey<'c, 's>> {
        self.health()?;
        if !std::ptr::eq(selected.anchor, &self.stamp_anchor)
            || !std::ptr::eq(selected.core, self.core)
            || !std::ptr::eq(authority.core(), self.core)
            || !self.uses_storage_origin(selected.binding, selected.fence)
            || !self.uses_storage_origin(authority.binding(), authority.fence())
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        let count = authority.count();
        if !matches!(count, 1 | 2)
            || usize::from(selected.count) != count
            || selected.init != *authority.init()
            || selected.targets != *authority.targets()
        {
            return Err(unchanged(Cause::Shape));
        }
        if selected.requires_flush() {
            return Err(unchanged(Cause::Busy));
        }
        let indices = [
            selected.slots[0].index,
            if count == 2 {
                selected.slots[1].index
            } else {
                0
            },
        ];
        if indices[..count]
            .iter()
            .any(|index| *index == 0 || *index >= self.frames.len())
            || count == 2 && indices[0] == indices[1]
        {
            return Err(unchanged(Cause::Shape));
        }
        let mut first = self.frames[indices[0]]
            .content
            .try_write()
            .map_err(|e| self.load_lock_error(e))?;
        let mut second = if count == 2 {
            Some(
                self.frames[indices[1]]
                    .content
                    .try_write()
                    .map_err(|e| self.load_lock_error(e))?,
            )
        } else {
            None
        };
        let mut registry = FencedInitRegistry {
            held: self.registry.lock(),
            pool: self,
        };
        self.health()?;
        let state = registry.state()?;
        if !self.registry_valid(state) {
            return Err(self.fatal(Cause::Invariant));
        }
        if !state.directory.retention_payloads() {
            return Err(unchanged(Cause::Shape));
        }
        if state.total != 0
            || state.load.is_some()
            || state.claim.is_some()
            || state.init_claim.is_some()
            || state.directory.closed_pair().is_some()
        {
            return Err(unchanged(Cause::Busy));
        }
        let mut target_mask = 0u64;
        for (at, request) in authority.targets().iter().enumerate() {
            if at > 0 && authority.targets()[at - 1].rank() >= request.rank()
                || authority.targets()[..at]
                    .iter()
                    .any(|prior| prior.same_address(request))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            if authority.init()[..count].contains(request) {
                continue;
            }
            let (index, _) = state
                .directory
                .lookup(self.frames, *request)
                .ok_or_else(|| unchanged(Cause::MissingTarget))?;
            if indices[..count].contains(&index) || target_mask & (1 << index) != 0 {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            target_mask |= 1 << index;
        }
        for index in 0..self.frames.len() {
            if state
                .directory
                .resolve(self.frames, index)
                .is_some_and(|entry| {
                    authority.init()[..count]
                        .iter()
                        .any(|new| new.same_address(&entry.request))
                })
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        let mut generations = [0u64; 2];
        for at in 0..count {
            let index = indices[at];
            generations[at] = state
                .directory
                .next_retention_generation(index)
                .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
            let content = if at == 0 {
                &*first
            } else {
                &**second.as_ref().expect("active second slot")
            };
            let observed = if state.directory.resolve(self.frames, index).is_some() {
                Some(self.pair_in(state, index, content)?.frame)
            } else {
                if !content.fresh()
                    || state.entries[index].is_some()
                    || state.requirements[index].is_some()
                {
                    return Err(self.fatal(Cause::Invariant));
                }
                if state.directory.closed_index() == Some(index) {
                    return Err(unchanged(Cause::MissingTarget));
                }
                None
            };
            if observed != selected.slots[at].frame
                || generations[at] != selected.next_generations[at]
            {
                return Err(unchanged(Cause::Stale));
            }
            if content.dirty {
                return Err(unchanged(Cause::Busy));
            }
            target_mask |= 1 << index;
        }
        let identity = StructuralClaimRecord {
            init: *authority.init(),
            work: authority.work(),
            birth: authority.birth(),
            generations,
            indices: indices.map(|v| u8::try_from(v).expect("checked slot")),
            init_count: count as u8,
            target_mask,
        };
        if !identity.shape() || target_mask.count_ones() != TARGETS as u32 || target_mask & 1 == 0 {
            return Err(unchanged(Cause::Shape));
        }
        // Mutation starts here. A failed return after this boundary must fence;
        // the registry guard and linear key never manufacture cancellation.
        if count == 2 {
            registry
                .prechecked_mut()
                .directory
                .prepare_retention_pair(indices, generations);
        } else {
            registry
                .prechecked_mut()
                .directory
                .prepare_retention_slot(indices[0], generations[0]);
        }
        self.init_unpublished.store(count == 1, Ordering::Release);
        for content in std::iter::once(&mut *first).chain(second.as_deref_mut()) {
            content.page_lsn = PageLsn(0);
            content.modification_counter = 0;
            content.rec_lsn = None;
            content.dirty = false;
        }
        registry.prechecked_mut().init_claim = Some(InitClaimSlot {
            identity: InitClaimIdentity::Structural(identity),
            initializer_pins_held: false,
        });
        if !identity.valid_in(self, registry.state()?) {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(StructuralClaimKey {
            core: self.core,
            binding: self.binding,
            fence: self.fence,
            identity,
            targets: *authority.targets(),
            active: true,
        })
    }

    pub(in crate::storage) fn claim_structural_targets<'a, 'b, 'k>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        key: &'k StructuralClaimKey<'c, 's>,
    ) -> Result<StructuralTargets<'a, 'b, 'k, 'c, 's, 'p, 'd>> {
        self.health()?;
        if !owner.check_binding(self.core).map_err(DirtyError::from)?
            || !std::ptr::eq(self.core, key.core)
            || !self.uses_storage_origin(key.binding, key.fence)
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        let mut pins = self.entry_registry()?;
        if !key.matches_pool_in(self, &pins) || key.identity.published(&pins) {
            return Err(unchanged(Cause::Stale));
        }
        let mut indices = [0usize; TARGETS];
        let mut generations = [0u64; TARGETS];
        for (at, request) in key.targets.iter().enumerate() {
            if let Some(init_at) = key.identity.init_at(*request) {
                indices[at] = usize::from(key.identity.indices[init_at]);
                generations[at] = key.identity.generations[init_at];
            } else {
                let (index, registration) = pins
                    .directory
                    .lookup(self.frames, *request)
                    .ok_or_else(|| unchanged(Cause::MissingTarget))?;
                indices[at] = index;
                generations[at] = registration.generation;
            }
        }
        if indices.iter().fold(0u64, |mask, at| mask | (1 << at)) != key.identity.target_mask {
            return Err(unchanged(Cause::Stale));
        }
        if pins
            .init_claim
            .is_none_or(|slot| slot.initializer_pins_held)
            || pins.load.is_some()
            || pins.claim.is_some()
            || indices.iter().any(|index| pins.counts[*index] != 0)
        {
            return Err(unchanged(Cause::Busy));
        }
        if pins.total > MAX_PINS - TARGETS as u16 {
            return Err(unchanged(Cause::Capacity));
        }
        for index in indices {
            pins.counts[index] += 1;
        }
        pins.total += TARGETS as u16;
        pins.init_claim
            .as_mut()
            .expect("checked structural claim")
            .initializer_pins_held = true;
        Ok(StructuralTargets {
            owner: Some(owner),
            pins: Some(StructuralPins {
                pool: self,
                key,
                indices,
                generations,
                active: true,
                _thread: PhantomData,
            }),
        })
    }
}

struct StructuralPins<'b, 'k, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    key: &'k StructuralClaimKey<'c, 's>,
    indices: [usize; TARGETS],
    generations: [u64; TARGETS],
    active: bool,
    _thread: PhantomData<Rc<()>>,
}
impl StructuralPins<'_, '_, '_, '_, '_, '_> {
    fn release(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        self.pool.fence_unwind();
        let (mut pins, poison) = match self.pool.registry.lock() {
            Ok(pins) => (pins, None),
            Err(e) => (e.into_inner(), Some(self.pool.fatal(Cause::Poisoned))),
        };
        let returned = (|| {
            if !self.key.matches_pool_in(self.pool, &pins)
                || !self.pool.pins_valid(&pins)
                || pins
                    .init_claim
                    .is_none_or(|slot| !slot.initializer_pins_held)
            {
                return Err(self.pool.fatal(Cause::Invariant));
            }
            for (at, index) in self.indices.iter().enumerate() {
                if *index >= self.pool.frames.len()
                    || self.indices[..at].contains(index)
                    || pins.counts[*index] == 0
                {
                    return Err(self.pool.fatal(Cause::Invariant));
                }
                if self.key.identity.init_at(self.key.targets[at]).is_none()
                    && self.pool.registration_in(&pins, *index)?
                        != (Registration {
                            request: self.key.targets[at],
                            generation: self.generations[at],
                        })
                {
                    return Err(self.pool.fatal(Cause::Invariant));
                }
            }
            let total = pins
                .total
                .checked_sub(TARGETS as u16)
                .ok_or_else(|| self.pool.fatal(Cause::Invariant))?;
            for index in self.indices {
                pins.counts[index] -= 1;
            }
            pins.total = total;
            pins.init_claim
                .as_mut()
                .expect("checked claim")
                .initializer_pins_held = false;
            Ok(())
        })();
        combine(poison.map_or(Ok(()), Err), returned)
    }
}
impl Drop for StructuralPins<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(in crate::storage) struct StructuralTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    owner: Option<&'a mut Owner<'c, 's>>,
    pins: Option<StructuralPins<'b, 'k, 'c, 's, 'p, 'd>>,
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd> StructuralTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    pub(in crate::storage) fn try_write(
        mut self,
    ) -> Result<StructuralWriteTargets<'a, 'b, 'k, 'c, 's, 'p, 'd>> {
        let mut pins = self.pins.take().expect("owned structural pins");
        let owner = self.owner.take().expect("original owner");
        let version = match owner.try_apply().and_then(|apply| apply.try_version()) {
            Ok(version) => version,
            Err(e) => return combine(Err(e.into()), pins.release()).and_then(|()| unreachable!()),
        };
        let pool = pins.pool;
        let mut write = StructuralWriteTargets {
            guards: [const { None }; TARGETS],
            version: Some(version),
            pins: Some(pins),
            stage_entered: false,
            installed: false,
        };
        for at in 0..TARGETS {
            let index = write.pins().indices[at];
            match pool.frames[index].content.try_write() {
                Ok(guard) => write.guards[at] = Some(guard),
                Err(TryLockError::WouldBlock) => return write.reject(unchanged(Cause::Busy)),
                Err(TryLockError::Poisoned(e)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(e.into_inner());
                    return write.reject(failure);
                }
            }
            if let Err(e) = write.observe(at) {
                return write.reject(e);
            }
        }
        Ok(write)
    }
}

pub(in crate::storage) struct StructuralWriteTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    guards: [Option<RwLockWriteGuard<'b, FrameContent<'d>>>; TARGETS],
    version: Option<VersionEditGuard<'a, 'c, 's>>,
    pins: Option<StructuralPins<'b, 'k, 'c, 's, 'p, 'd>>,
    stage_entered: bool,
    installed: bool,
}
pub(in crate::storage) struct CompletedStructural<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    identity: StructuralClaimRecord,
    requirement: WalRequirement<'s>,
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd> StructuralWriteTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    fn pins(&self) -> &StructuralPins<'b, 'k, 'c, 's, 'p, 'd> {
        self.pins.as_ref().expect("owned structural targets")
    }
    pub(in crate::storage) fn observe(&self, at: usize) -> Result<Option<DirtyObservation>> {
        let pins = self.pins();
        let pool = pins.pool;
        pool.health()?;
        let content = self
            .guards
            .get(at)
            .and_then(Option::as_ref)
            .ok_or_else(|| unchanged(Cause::Shape))?;
        if !pins.key.matches_pool(pool)? {
            return Err(pool.fatal(Cause::Invariant));
        }
        if pins.key.identity.init_at(pins.key.targets[at]).is_some() && !self.installed {
            let registry = pool.entry_registry()?;
            if !content.fresh()
                || registry.entries[pins.indices[at]].is_some()
                || registry.requirements[pins.indices[at]].is_some()
            {
                return Err(pool.fatal(Cause::Invariant));
            }
            Ok(None)
        } else {
            let observed = pool.pair(pins.indices[at], content)?;
            if observed.frame.generation != pins.generations[at]
                || observed.frame.identity != pins.key.targets[at].identity
            {
                return Err(pool.fatal(Cause::Invariant));
            }
            if observed.frame.modification_counter == u64::MAX {
                return Err(unchanged(Cause::CounterExhausted));
            }
            Ok(Some(observed))
        }
    }
    pub(in crate::storage) fn bytes(&self, at: usize) -> Result<&[u8; PAGE_BYTES]> {
        if self.observe(at)?.is_none() {
            return Err(unchanged(Cause::MissingTarget));
        }
        Ok(self.guards[at]
            .as_ref()
            .expect("observed existing target")
            .bytes)
    }
    pub(in crate::storage) fn enter_stage(&mut self) -> Result<()> {
        self.pins().pool.health()?;
        if self.stage_entered || self.installed {
            return Err(self.pins().pool.fatal(Cause::Invariant));
        }
        self.stage_entered = true;
        Ok(())
    }
    pub(in crate::storage) fn accept_stage_rejection(
        &mut self,
        proof: StructuralStageRejection<'_, 'c, 's>,
    ) -> Result<()> {
        if !self.stage_entered || self.installed || !std::ptr::eq(proof.key(), self.pins().key) {
            return Err(self.pins().pool.fatal(Cause::Invariant));
        }
        self.stage_entered = false;
        Ok(())
    }
    pub(in crate::storage) fn install(
        &mut self,
        permit: &StructuralInstallPermit<'_, 'c, 's>,
        mut control: StructuralControlCommit<'_, '_, 'c, 's>,
    ) -> Result<CompletedStructural<'c, 's>> {
        let pool = self.pins().pool;
        let requirement = permit.requirement();
        let lsn = PageLsn(requirement.location().lsn);
        let identity = self.pins().key.identity;
        if !self.stage_entered
            || self.installed
            || !self.pins().key.matches_pool(pool)?
            || !std::ptr::eq(self.pins().key, permit.key())
            || self.pins().key.targets != *permit.targets()
            || !requirement.uses_fence(pool.fence)
            || lsn != identity.birth
            || identity.work.id() != lsn.0
        {
            return Err(pool.fatal(Cause::Invariant));
        }
        let mut registry = FencedInitRegistry {
            held: pool.registry.lock(),
            pool,
        };
        {
            let state = registry.state()?;
            if !pool.dpt_valid(state) || identity.published(state) {
                return Err(pool.fatal(Cause::Invariant));
            }
            for at in 0..TARGETS {
                let index = self.pins().indices[at];
                let request = self.pins().key.targets[at];
                let content = self.guards[at].as_ref().expect("held structural target");
                if let Some(init_at) = identity.init_at(request) {
                    if permit.before()[at].is_some()
                        || !content.fresh()
                        || state.entries[index].is_some()
                        || state.requirements[index].is_some()
                        || self.pins().generations[at] != identity.generations[init_at]
                    {
                        return Err(pool.fatal(Cause::Invariant));
                    }
                } else {
                    let before = permit.before()[at].ok_or_else(|| pool.fatal(Cause::Invariant))?;
                    let expected = before.rec_lsn.map(|rec_lsn| DirtyEntry {
                        identity: before.identity,
                        generation: before.generation,
                        rec_lsn,
                        modification_counter: before.modification_counter,
                    });
                    if pool.stamp_in(state, index, content)? != before
                        || before.identity != request.identity
                        || before.generation != self.pins().generations[at]
                        || !pool.dpt_matches(state, index, expected.as_ref())
                        || state.requirements[index].map(|r| r.location().lsn)
                            != before.rec_lsn.map(|_| before.page_lsn.0)
                        || before.modification_counter.checked_add(1).is_none()
                        || before.page_lsn.0 >= lsn.0
                    {
                        return Err(pool.fatal(Cause::Invariant));
                    }
                }
                if u64::from_le_bytes(permit.after(at)[64..72].try_into().expect("LSN field"))
                    != lsn.0
                {
                    return Err(pool.fatal(Cause::Invariant));
                }
            }
        }
        for at in 0..TARGETS {
            let content = self.guards[at].as_mut().expect("prechecked target");
            content.bytes.copy_from_slice(permit.after(at));
            content.page_lsn = lsn;
            content.modification_counter =
                permit.before()[at].map_or(1, |before| before.modification_counter + 1);
            content.rec_lsn = Some(
                permit.before()[at]
                    .and_then(|before| before.rec_lsn)
                    .unwrap_or(lsn),
            );
            content.dirty = true;
            #[cfg(test)]
            if at == 0 {
                test_support::hit(pool, test_support::InitPoint::StructuralFirstImage);
            }
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::StructuralImages);
        control.apply(self.pins().key, requirement).map_err(|e| {
            pool.fence.fence();
            DirtyError {
                cause: e.cause,
                effect: Effect::CallerMustResolve,
            }
        })?;
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::StructuralControl);
        for at in 0..TARGETS {
            let index = self.pins().indices[at];
            let content = self.guards[at].as_ref().expect("installed target");
            let state = registry.prechecked_mut();
            state.entries[index] = Some(DptEntry::from_checked(&DirtyEntry {
                identity: self.pins().key.targets[at].identity,
                generation: self.pins().generations[at],
                rec_lsn: content.rec_lsn.expect("installed recLSN"),
                modification_counter: content.modification_counter,
            }));
            state.requirements[index] = Some(requirement);
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::StructuralDpt);
        let registrations = std::array::from_fn(|at| Registration {
            request: identity.init[at],
            generation: identity.generations[at],
        });
        let published = if identity.init_count == 2 {
            registry.prechecked_mut().directory.publish_pair(
                registrations,
                #[cfg(test)]
                pool,
            )
        } else {
            registry
                .prechecked_mut()
                .directory
                .publish(registrations[0])
        };
        if !published {
            return Err(pool.fatal(Cause::Invariant));
        }
        pool.init_unpublished.store(false, Ordering::Release);
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::StructuralPublished);
        self.installed = true;
        Ok(CompletedStructural {
            core: pool.core,
            binding: pool.binding,
            identity,
            requirement,
        })
    }
    fn release(&mut self) -> Result<()> {
        let pool = self.pins().pool;
        pool.fence_unwind();
        let original = if self.stage_entered && !self.installed {
            Err(pool.fatal(Cause::Invariant))
        } else {
            Ok(())
        };
        for guard in self.guards.iter_mut().rev() {
            guard.take();
        }
        let version = self.version.take().map_or(Ok(()), |guard| {
            guard.release_checked().map_err(DirtyError::from)
        });
        let pins = self.pins.as_mut().map_or(Ok(()), StructuralPins::release);
        combine(original, combine(version, pins))
    }
    fn reject<T>(mut self, error: DirtyError) -> Result<T> {
        combine(Err(error), self.release()).and_then(|()| unreachable!())
    }
    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}
impl Drop for StructuralWriteTargets<'_, '_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

impl<'s> SharedPool<'_, 's, '_, '_> {
    fn structural_quiescent(
        &self,
        key: &StructuralClaimKey<'_, '_>,
        state: &PoolRegistry<'s>,
    ) -> Result<()> {
        self.health()?;
        if !self.registry_valid(state) || !key.matches_pool_in(self, state) {
            return Err(self.fatal(Cause::Invariant));
        }
        if state.total != 0
            || state.load.is_some()
            || state.claim.is_some()
            || state
                .init_claim
                .is_none_or(|slot| slot.initializer_pins_held)
        {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }
    pub(in crate::storage) fn finish_structural(
        &self,
        key: &mut StructuralClaimKey<'_, '_>,
        completed: CompletedStructural<'_, '_>,
        durable: &DurableWal<'_>,
    ) -> Result<()> {
        let mut registry = FencedInitRegistry {
            held: self.registry.lock(),
            pool: self,
        };
        let state = registry.state()?;
        self.structural_quiescent(key, state)?;
        if !std::ptr::eq(self.core, completed.core)
            || !std::ptr::eq(self.binding, completed.binding)
            || completed.identity != key.identity
            || !completed.requirement.uses_fence(self.fence)
            || !durable.confirms(&completed.requirement)
            || !key.identity.published(state)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        for request in key.targets {
            let (index, _) = state
                .directory
                .lookup(self.frames, request)
                .ok_or_else(|| self.fatal(Cause::Invariant))?;
            if state.requirements[index].map(WalRequirement::location)
                != Some(completed.requirement.location())
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        #[cfg(test)]
        test_support::hit(self, test_support::InitPoint::StructuralFinishChecked);
        let state = registry.prechecked_mut();
        if key.identity.init_count == 2 {
            state.directory.finish_retention_pair();
        } else if !state.directory.finish_retention_slot(
            usize::from(key.identity.indices[0]),
            Registration {
                request: key.identity.init[0],
                generation: key.identity.generations[0],
            },
        ) {
            return Err(self.fatal(Cause::Invariant));
        }
        state.init_claim = None;
        key.active = false;
        Ok(())
    }
    pub(in crate::storage) fn cancel_structural(
        &self,
        key: &mut StructuralClaimKey<'_, '_>,
        permit: StructuralCancelPermit<'_, '_, '_, '_>,
    ) -> Result<()> {
        self.health()?;
        if !key.matches_authority(permit.authority())
            || !permit.io_matches(self.binding, self.fence)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        let allocator = self.frames[0]
            .content
            .try_read()
            .map_err(|e| self.load_lock_error(e))?;
        let indices = key.identity.indices();
        let first = self.frames[indices[0]]
            .content
            .try_write()
            .map_err(|e| self.load_lock_error(e))?;
        let second = if key.identity.init_count == 2 {
            Some(
                self.frames[indices[1]]
                    .content
                    .try_write()
                    .map_err(|e| self.load_lock_error(e))?,
            )
        } else {
            None
        };
        let mut registry = FencedInitRegistry {
            held: self.registry.lock(),
            pool: self,
        };
        let state = registry.state()?;
        self.structural_quiescent(key, state)?;
        if key.identity.published(state)
            || self.pair_in(state, 0, &allocator)?.frame != permit.allocator_before()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        for (at, content) in std::iter::once(&*first)
            .chain(second.as_deref())
            .enumerate()
        {
            if !content.fresh()
                || state.entries[indices[at]].is_some()
                || state.requirements[indices[at]].is_some()
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        let state = registry.prechecked_mut();
        if key.identity.init_count == 2 {
            state.directory.clear_retention_pair(indices);
        } else {
            state.directory.retire_retention(indices[0]);
        }
        self.init_unpublished.store(false, Ordering::Release);
        state.init_claim = None;
        key.active = false;
        Ok(())
    }
}
