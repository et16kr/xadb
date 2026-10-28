//! The original fixed pool's single Tx CREATE claim and nine-target installer.
//! No allocator, semantic or selected-DWB authority originates in this module.
use super::*;
use crate::storage::{
    transaction_record::TxnRef,
    wal_io::{
        TxPairAuthority, TxPairCancelKind, TxPairCancelPermit, TxPairControlCommit,
        TxPairInstallPermit, TxPairRetirePermit, TxPairStageRejection,
    },
};

const TARGETS: usize = 9;

#[path = "tx_pair_existing.rs"]
mod existing;
pub(in crate::storage) use existing::{
    TxPairExistingRead, TxPairExistingTargets, TxPairExistingWrite,
    source_bytes as tx_pair_existing_source_bytes,
};

#[cfg(test)]
#[path = "tx_pair_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TxPairClaimRecord {
    pair: [TargetRequest; 2],
    actor: TxnRef,
    reserve: PageLsn,
    generations: [u64; 2],
    indices: [u8; 2],
    target_mask: u64,
}
impl TxPairClaimRecord {
    #[cfg(test)]
    pub(super) fn diagnostic_registration(&self, index: usize) -> Option<Registration> {
        self.indices()
            .into_iter()
            .position(|value| value == index)
            .map(|at| Registration {
                request: self.pair[at],
                generation: self.generations[at],
            })
    }
    pub(super) fn blocks_load(&self, index: usize, request: TargetRequest) -> bool {
        (index < MAX_FRAMES && self.target_mask & (1 << index) != 0)
            || self.pair.iter().any(|value| value.same_address(&request))
    }
    pub(super) fn indices(&self) -> [usize; 2] {
        self.indices.map(usize::from)
    }
    pub(super) fn held_pins_valid(&self, pins: &PoolRegistry<'_>) -> bool {
        self.target_mask.count_ones() == TARGETS as u32
            && (0..MAX_FRAMES)
                .all(|index| self.target_mask & (1 << index) == 0 || pins.counts[index] != 0)
    }
    pub(super) fn valid(&self, pool: &SharedPool<'_, '_, '_, '_>) -> Result<bool> {
        let state = pool.entry_registry()?;
        Ok(self.valid_in(pool, &state))
    }
    pub(super) fn valid_in(
        &self,
        pool: &SharedPool<'_, '_, '_, '_>,
        state: &PoolRegistry<'_>,
    ) -> bool {
        state.directory.retention_payloads()
            && state.directory.closed_pair() == Some(self.indices())
            && state.directory.pair_generations() == Some(self.generations)
            && self
                .generations
                .into_iter()
                .all(|generation| generation != 0)
            && self.indices[0] != self.indices[1]
            && self
                .indices()
                .into_iter()
                .all(|index| index != 0 && index < pool.frames.len())
            && pair_shape(self.actor, self.reserve, &self.pair)
            && self.target_mask.count_ones() == TARGETS as u32
            && self.target_mask & 1 != 0
            && self
                .indices()
                .into_iter()
                .all(|index| self.target_mask & (1 << index) != 0)
            && self.existing_mask_valid(pool, state)
            && state
                .directory
                .resolve(pool.frames, 0)
                .is_some_and(|allocator| {
                    allocator.request.identity.database == self.actor.id().database
                        && allocator.request.identity.page.file == self.pair[0].identity.page.file
                })
            && self.indices().into_iter().enumerate().all(|(at, index)| {
                index != 0
                    && index < pool.frames.len()
                    && state
                        .directory
                        .resolve(pool.frames, index)
                        .is_none_or(|registration| {
                            registration.request == self.pair[at]
                                && registration.generation == self.generations[at]
                        })
            })
            && (0..pool.frames.len()).all(|index| {
                self.indices().contains(&index)
                    || state
                        .directory
                        .resolve(pool.frames, index)
                        .is_none_or(|registration| {
                            !self
                                .pair
                                .iter()
                                .any(|request| registration.request.same_address(request))
                        })
            })
    }
    fn pair_at(&self, request: TargetRequest) -> Option<usize> {
        self.pair.iter().position(|candidate| *candidate == request)
    }
    fn existing_mask_valid(
        &self,
        pool: &SharedPool<'_, '_, '_, '_>,
        state: &PoolRegistry<'_>,
    ) -> bool {
        let mut roles = 0u8;
        for index in 0..MAX_FRAMES {
            if self.target_mask & (1 << index) == 0 || self.indices().contains(&index) {
                continue;
            }
            let Some(registration) = state.directory.resolve(pool.frames, index) else {
                return false;
            };
            let Some(role) = existing_role(registration.request) else {
                return false;
            };
            if roles & (1 << role) != 0 {
                return false;
            }
            roles |= 1 << role;
        }
        roles == 127
    }
}

fn pair_shape(actor: TxnRef, reserve: PageLsn, pair: &[TargetRequest; 2]) -> bool {
    reserve.0 != 0
        && pair[0].kind == PageKind::TreeAnchor
        && pair[1].kind == PageKind::RowLeaf
        && pair[0].identity.owner == pair[1].identity.owner
        && pair[0].identity.owner.get() >= 6
        && pair[0].identity.page.file == pair[1].identity.page.file
        && !pair[0].same_address(&pair[1])
        && pair[0].rank() < pair[1].rank()
        && pair.iter().all(|request| {
            request.level == 0
                && request.identity.database == actor.id().database
                && request.identity.owner_generation.get() == 1
                && (9..64).contains(&request.identity.page.page_no)
                && request.identity.page.incarnation.get() >= 2
        })
}

fn existing_role(request: TargetRequest) -> Option<u8> {
    if request.level != 0 || request.identity.owner_generation.get() != 1 {
        return None;
    }
    let (page, owner, kind) = (
        request.identity.page.page_no,
        request.identity.owner.get(),
        request.kind,
    );
    match (page, owner, kind) {
        (0, 1, PageKind::Allocation) => Some(0),
        (1, 4, PageKind::RetentionRoot) => Some(1),
        (2..=6, 4, PageKind::RetentionJobs) => Some(2),
        (7, 5, PageKind::TreeAnchor) => Some(3),
        (8, 5, PageKind::CatalogLeaf) => Some(4),
        (9..=63, 2, PageKind::TransactionStatus) => Some(5),
        (9..=63, 3, PageKind::Undo) => Some(6),
        _ => None,
    }
}

/// Linear cleanup ownership, retained by the same original CREATE reservation.
/// Drop never manufactures Cancel and never hides an incomplete publication.
pub(in crate::storage) struct TxPairClaimKey<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    identity: TxPairClaimRecord,
    active: bool,
}
impl TxPairClaimKey<'_, '_> {
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
    pub(in crate::storage) fn matches_authority(
        &self,
        authority: &TxPairAuthority<'_, '_, '_>,
    ) -> bool {
        self.active
            && std::ptr::eq(self.core, authority.core())
            && std::ptr::eq(self.binding, authority.binding())
            && std::ptr::eq(self.fence, authority.fence())
            && self.identity.actor == authority.actor()
            && self.identity.reserve == authority.reserve()
            && self.identity.pair == *authority.pair()
    }
}
impl Drop for TxPairClaimKey<'_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.fence.fence();
        }
    }
}

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    /// Close both clean victims and install the original claim in one leaf.
    /// Ordinary pins/load/capture arbitrate with this close under the same
    /// registry guard; only authentic subset/whole pins may enter afterward.
    pub(in crate::storage) fn try_prepare_tx_pair(
        &self,
        authority: &TxPairAuthority<'_, 'c, 's>,
        selected: &CatalogPairCandidates<'_, 'c, 's>,
    ) -> Result<TxPairClaimKey<'c, 's>> {
        self.health()?;
        if !std::ptr::eq(selected.anchor, &self.stamp_anchor)
            || !std::ptr::eq(selected.core, self.core)
            || !self.uses_storage_origin(selected.binding, selected.fence)
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if selected.requires_flush() {
            return Err(unchanged(Cause::Busy));
        }
        self.prepare_tx_pair_selected(
            authority,
            [selected.slots[0].index, selected.slots[1].index],
            Some(selected),
        )
    }

    fn prepare_tx_pair_selected(
        &self,
        authority: &TxPairAuthority<'_, 'c, 's>,
        indices: [usize; 2],
        selected: Option<&CatalogPairCandidates<'_, 'c, 's>>,
    ) -> Result<TxPairClaimKey<'c, 's>> {
        self.health()?;
        if !std::ptr::eq(self.core, authority.core())
            || !self.uses_storage_origin(authority.binding(), authority.fence())
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if !pair_shape(authority.actor(), authority.reserve(), authority.pair())
            || indices[0] == indices[1]
            || indices
                .iter()
                .any(|index| *index == 0 || *index >= self.frames.len())
        {
            return Err(unchanged(Cause::Shape));
        }
        let mut first = self.frames[indices[0]]
            .content
            .try_write()
            .map_err(|error| self.load_lock_error(error))?;
        let mut second = self.frames[indices[1]]
            .content
            .try_write()
            .map_err(|error| self.load_lock_error(error))?;
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
        let mut roles = 0u8;
        let mut target_mask = 0u64;
        for request in authority.existing() {
            let role = existing_role(*request).ok_or_else(|| unchanged(Cause::Shape))?;
            if roles & (1 << role) != 0
                || request.identity.database != authority.actor().id().database
                || request.identity.page.file != authority.pair()[0].identity.page.file
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            roles |= 1 << role;
            let (index, _) = state
                .directory
                .lookup(self.frames, *request)
                .ok_or_else(|| unchanged(Cause::MissingTarget))?;
            if indices.contains(&index) || target_mask & (1 << index) != 0 {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            target_mask |= 1 << index;
        }
        for index in 0..self.frames.len() {
            if state
                .directory
                .resolve(self.frames, index)
                .is_some_and(|entry| {
                    authority
                        .pair()
                        .iter()
                        .any(|request| entry.request.same_address(request))
                })
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        let generations = [
            state
                .directory
                .next_retention_generation(indices[0])
                .ok_or_else(|| unchanged(Cause::CounterExhausted))?,
            state
                .directory
                .next_retention_generation(indices[1])
                .ok_or_else(|| unchanged(Cause::CounterExhausted))?,
        ];
        if selected.is_some_and(|value| value.next_generations != generations) {
            return Err(unchanged(Cause::Stale));
        }
        for (at, (index, content)) in [(indices[0], &*first), (indices[1], &*second)]
            .into_iter()
            .enumerate()
        {
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
            if selected.is_some_and(|value| value.slots[at].frame != observed) {
                return Err(unchanged(Cause::Stale));
            }
            if content.dirty {
                return Err(unchanged(Cause::Busy));
            }
        }
        let identity = TxPairClaimRecord {
            pair: *authority.pair(),
            actor: authority.actor(),
            reserve: authority.reserve(),
            generations,
            indices: indices.map(|index| u8::try_from(index).expect("checked slot")),
            target_mask: target_mask | (1 << indices[0]) | (1 << indices[1]),
        };
        // Every Unchanged path ends before this boundary. Directory, fresh
        // metadata and the original claim remain under this one registry guard.
        registry
            .prechecked_mut()
            .directory
            .prepare_retention_pair(indices, generations);
        self.init_unpublished.store(false, Ordering::Release);
        for content in [&mut *first, &mut *second] {
            content.page_lsn = PageLsn(0);
            content.modification_counter = 0;
            content.rec_lsn = None;
            content.dirty = false;
        }
        #[cfg(test)]
        test_support::hit(self, test_support::InitPoint::PairClosedBeforeClaim);
        registry.prechecked_mut().init_claim = Some(InitClaimSlot {
            identity: InitClaimIdentity::TxPair(identity),
            initializer_pins_held: false,
        });
        if !identity.valid_in(self, registry.state()?) {
            return Err(self.fatal(Cause::Invariant));
        }
        #[cfg(test)]
        test_support::hit(self, test_support::InitPoint::PairClaimInstalled);
        Ok(TxPairClaimKey {
            core: self.core,
            binding: self.binding,
            fence: self.fence,
            identity,
            active: true,
        })
    }

    /// Historical exclusive fixtures retain their original call spelling.
    #[cfg(test)]
    pub(in crate::storage) fn prepare_tx_pair(
        &mut self,
        authority: &TxPairAuthority<'_, 'c, 's>,
        indices: [usize; 2],
    ) -> Result<TxPairClaimKey<'c, 's>> {
        self.prepare_tx_pair_selected(authority, indices, None)
    }

    pub(in crate::storage) fn claim_tx_pair_targets<'a, 'b, 'k>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        key: &'k TxPairClaimKey<'c, 's>,
        requests: &'k [TargetRequest; TARGETS],
    ) -> Result<TxPairTargets<'a, 'b, 'k, 'c, 's, 'p, 'd>> {
        self.health()?;
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if !std::ptr::eq(self.core, key.core) || !self.uses_storage_origin(key.binding, key.fence) {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if !key.matches_pool(self)?
            || self.directory_read(|directory| directory.pair_published())?
        {
            return Err(unchanged(Cause::Stale));
        }
        let mut indices = [0; TARGETS];
        let mut generations = [0; TARGETS];
        let mut roles = 0u16;
        for (at, request) in requests.iter().enumerate() {
            if at != 0 && requests[at - 1].rank() >= request.rank()
                || requests[..at]
                    .iter()
                    .any(|prior| prior.same_address(request))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            let role = if let Some(pair_at) = key.identity.pair_at(*request) {
                indices[at] = key.identity.indices()[pair_at];
                generations[at] = key.identity.generations[pair_at];
                7 + pair_at as u8
            } else {
                let role = existing_role(*request).ok_or_else(|| unchanged(Cause::Shape))?;
                let (index, registration) = self
                    .directory_read(|directory| directory.lookup(self.frames, *request))?
                    .ok_or_else(|| unchanged(Cause::MissingTarget))?;
                indices[at] = index;
                generations[at] = registration.generation;
                role
            };
            if roles & (1 << role) != 0 {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            roles |= 1 << role;
        }
        if roles != (1 << TARGETS) - 1 {
            return Err(unchanged(Cause::Shape));
        }
        if indices.iter().fold(0u64, |mask, index| mask | (1 << index)) != key.identity.target_mask
        {
            return Err(unchanged(Cause::Stale));
        }
        {
            let mut pins = self.entry_registry()?;
            if !key.matches_pool_in(self, &pins) {
                return Err(unchanged(Cause::Stale));
            }
            for (at, request) in requests.iter().enumerate() {
                if pins
                    .load
                    .as_ref()
                    .is_some_and(|load| load.blocks(*request, indices[at]))
                {
                    return Err(unchanged(Cause::Busy));
                }
                if key.identity.pair_at(*request).is_none()
                    && pins.directory.lookup(self.frames, *request)
                        != Some((
                            indices[at],
                            Registration {
                                request: *request,
                                generation: generations[at],
                            },
                        ))
                {
                    return Err(unchanged(Cause::Stale));
                }
            }
            if !pins.init_claim.is_some_and(|slot| {
                slot.identity == InitClaimIdentity::TxPair(key.identity)
                    && !slot.initializer_pins_held
            }) {
                return Err(unchanged(Cause::Busy));
            }
            // An authenticated existing subset may already own these pins.
            // Whole Forward and that subset must exclude one another in both
            // directions under the same claim/quota leaf.
            if indices.iter().any(|index| pins.counts[*index] != 0) {
                return Err(unchanged(Cause::Busy));
            }
            if pins.total > MAX_PINS - TARGETS as u16
                || indices
                    .iter()
                    .any(|index| pins.counts[*index] >= MAX_FRAME_PINS)
            {
                return Err(unchanged(Cause::Capacity));
            }
            for index in indices {
                pins.counts[index] += 1;
            }
            pins.total += TARGETS as u16;
            pins.init_claim
                .as_mut()
                .expect("checked pair claim")
                .initializer_pins_held = true;
        }
        Ok(TxPairTargets {
            owner: Some(owner),
            pins: Some(TxPairPins {
                pool: self,
                key,
                requests,
                indices,
                generations,
                active: true,
                _thread: PhantomData,
            }),
        })
    }
}

struct TxPairPins<'b, 'k, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    key: &'k TxPairClaimKey<'c, 's>,
    requests: &'k [TargetRequest; TARGETS],
    indices: [usize; TARGETS],
    generations: [u64; TARGETS],
    active: bool,
    _thread: PhantomData<Rc<()>>,
}
impl TxPairPins<'_, '_, '_, '_, '_, '_> {
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
            if !self.key.matches_pool_in(self.pool, &pins)
                || !self.pool.pins_valid(&pins)
                || !pins.init_claim.is_some_and(|slot| {
                    slot.identity == InitClaimIdentity::TxPair(self.key.identity)
                        && slot.initializer_pins_held
                })
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
                if self.key.identity.pair_at(self.requests[at]).is_none()
                    && self.pool.registration_in(&pins, *index)?
                        != (Registration {
                            request: self.requests[at],
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
                .expect("checked pair claim")
                .initializer_pins_held = false;
            Ok(())
        })();
        combine(poison.map_or(Ok(()), Err), result)
    }
}
impl Drop for TxPairPins<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(in crate::storage) struct TxPairTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    owner: Option<&'a mut Owner<'c, 's>>,
    pins: Option<TxPairPins<'b, 'k, 'c, 's, 'p, 'd>>,
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd> TxPairTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    pub(in crate::storage) fn try_write(
        mut self,
    ) -> Result<TxPairWriteTargets<'a, 'b, 'k, 'c, 's, 'p, 'd>> {
        let mut pins = self.pins.take().expect("owned pair pins");
        let owner = self.owner.take().expect("owned CREATE owner");
        let version = match owner.try_apply().and_then(|apply| apply.try_version()) {
            Ok(version) => version,
            Err(error) => {
                return combine(Err(error.into()), pins.release()).and_then(|()| unreachable!());
            }
        };
        let pool = pins.pool;
        let mut write = TxPairWriteTargets {
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
                Err(TryLockError::Poisoned(error)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(error.into_inner());
                    return write.reject(failure);
                }
            }
            if let Err(error) = write.observe(at) {
                return write.reject(error);
            }
        }
        Ok(write)
    }
    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.owner.take();
        self.pins.as_mut().map_or(Ok(()), TxPairPins::release)
    }
}

pub(in crate::storage) struct TxPairWriteTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    guards: [Option<RwLockWriteGuard<'b, FrameContent<'d>>>; TARGETS],
    version: Option<VersionEditGuard<'a, 'c, 's>>,
    pins: Option<TxPairPins<'b, 'k, 'c, 's, 'p, 'd>>,
    stage_entered: bool,
    installed: bool,
}

/// Produced only after all eight images, control, DPT and the joint gate have
/// been installed. A WAL requirement alone cannot construct this completion.
pub(in crate::storage) struct CompletedTxPair<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    identity: TxPairClaimRecord,
    requirement: WalRequirement<'s>,
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd> TxPairWriteTargets<'a, 'b, 'k, 'c, 's, 'p, 'd> {
    fn pins(&self) -> &TxPairPins<'b, 'k, 'c, 's, 'p, 'd> {
        self.pins.as_ref().expect("owned pair targets")
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
        if pins.key.identity.pair_at(pins.requests[at]).is_some() && !self.installed {
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
                || observed.frame.identity != pins.requests[at].identity
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
    /// Must precede entering the real semantic stage call. This only revokes
    /// ordinary cancellation; it does not issue an install permission.
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
        proof: TxPairStageRejection<'_, 'c, 's>,
    ) -> Result<()> {
        if !self.stage_entered || self.installed || !std::ptr::eq(proof.key(), self.pins().key) {
            return Err(self.pins().pool.fatal(Cause::Invariant));
        }
        self.stage_entered = false;
        Ok(())
    }

    pub(in crate::storage) fn install_forward(
        &mut self,
        permit: &TxPairInstallPermit<'_, 'c, 's>,
        mut control: TxPairControlCommit<'_, '_, 'c, 's>,
    ) -> Result<CompletedTxPair<'c, 's>> {
        let pool = self.pins().pool;
        let requirement = permit.requirement();
        let lsn = PageLsn(requirement.location().lsn);
        let identity = self.pins().key.identity;
        if !self.stage_entered
            || self.installed
            || !self.pins().key.matches_pool(pool)?
            || !std::ptr::eq(self.pins().key, permit.key())
            || self.pins().requests != permit.targets()
            || !requirement.uses_fence(pool.fence)
            || lsn.0 <= identity.reserve.0
            || pool.directory_read(|directory| directory.pair_published())?
        {
            return Err(pool.fatal(Cause::Invariant));
        }
        // Own even the poison guard before inspecting it. During this entire
        // registry scope the pair remains hidden behind its publication gate.
        let mut registry = FencedInitRegistry {
            held: pool.registry.lock(),
            pool,
        };
        {
            let dpt = registry.state()?;
            if !pool.dpt_valid(dpt) {
                return Err(pool.fatal(Cause::Invariant));
            }
            for at in 0..TARGETS {
                let index = self.pins().indices[at];
                let request = self.pins().requests[at];
                let content = self.guards[at].as_ref().expect("held Forward target");
                if let Some(pair_at) = identity.pair_at(request) {
                    if permit.before()[at].is_some()
                        || permit.after()[at].is_none()
                        || !content.fresh()
                        || dpt.entries[index].is_some()
                        || dpt.requirements[index].is_some()
                        || self.pins().generations[at] != identity.generations[pair_at]
                    {
                        return Err(pool.fatal(Cause::Invariant));
                    }
                } else {
                    let before = permit.before()[at].ok_or_else(|| pool.fatal(Cause::Invariant))?;
                    let expected_entry = before.rec_lsn.map(|rec_lsn| DirtyEntry {
                        identity: before.identity,
                        generation: before.generation,
                        rec_lsn,
                        modification_counter: before.modification_counter,
                    });
                    if pool.stamp_in(dpt, index, content)? != before
                        || before.identity != request.identity
                        || before.generation != self.pins().generations[at]
                        || !pool.dpt_matches(dpt, index, expected_entry.as_ref())
                        || dpt.requirements[index].map(|receipt| receipt.location().lsn)
                            != before.rec_lsn.map(|_| before.page_lsn.0)
                        || (existing_role(request) == Some(3)) != permit.after()[at].is_none()
                    {
                        return Err(pool.fatal(Cause::Invariant));
                    }
                    if permit.after()[at].is_some()
                        && (before.modification_counter.checked_add(1).is_none()
                            || before.page_lsn.0 >= lsn.0)
                    {
                        return Err(pool.fatal(Cause::Invariant));
                    }
                }
                if let Some(after) = permit.after()[at]
                    && u64::from_le_bytes(after[64..72].try_into().expect("fixed LSN field"))
                        != lsn.0
                {
                    return Err(pool.fatal(Cause::Invariant));
                }
            }
        }
        for at in 0..TARGETS {
            if let Some(after) = permit.after()[at] {
                let content = self.guards[at].as_mut().expect("prechecked Forward target");
                content.bytes.copy_from_slice(after);
                content.page_lsn = lsn;
            }
        }
        for at in 0..TARGETS {
            if permit.after()[at].is_some() {
                let content = self.guards[at].as_mut().expect("prechecked Forward target");
                content.modification_counter =
                    permit.before()[at].map_or(1, |before| before.modification_counter + 1);
                content.rec_lsn = Some(
                    permit.before()[at]
                        .and_then(|before| before.rec_lsn)
                        .unwrap_or(lsn),
                );
                content.dirty = true;
            }
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::PairImagesInstalled);
        // Actual Tx/Aging descriptors, ATT/inventory and RootJob bank are
        // installed by their sealed original control carrier before publication.
        if let Err(error) = control.apply(self.pins().key, requirement) {
            pool.fence.fence();
            return Err(DirtyError {
                cause: error.cause,
                effect: Effect::CallerMustResolve,
            });
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::PairControlInstalled);
        {
            let dpt = registry.prechecked_mut();
            for at in 0..TARGETS {
                if permit.after()[at].is_some() {
                    let index = self.pins().indices[at];
                    let content = self.guards[at].as_ref().expect("installed Forward target");
                    dpt.entries[index] = Some(DptEntry::from_checked(&DirtyEntry {
                        identity: self.pins().requests[at].identity,
                        generation: self.pins().generations[at],
                        rec_lsn: content.rec_lsn.expect("installed recLSN"),
                        modification_counter: content.modification_counter,
                    }));
                    dpt.requirements[index] = Some(requirement);
                }
            }
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::PairDptInstalled);
        if !registry.prechecked_mut().directory.publish_pair(
            [
                Registration {
                    request: identity.pair[0],
                    generation: identity.generations[0],
                },
                Registration {
                    request: identity.pair[1],
                    generation: identity.generations[1],
                },
            ],
            #[cfg(test)]
            pool,
        ) {
            return Err(pool.fatal(Cause::Invariant));
        }
        #[cfg(test)]
        test_support::hit(pool, test_support::InitPoint::PairGatePublished);
        self.installed = true;
        Ok(CompletedTxPair {
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
        let pins = self.pins.as_mut().map_or(Ok(()), TxPairPins::release);
        combine(original, combine(version, pins))
    }
    fn reject<T>(mut self, error: DirtyError) -> Result<T> {
        combine(Err(error), self.release()).and_then(|()| unreachable!())
    }
    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}
impl Drop for TxPairWriteTargets<'_, '_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

impl<'s> SharedPool<'_, 's, '_, '_> {
    fn tx_pair_quiescent_claim_in(
        &self,
        key: &TxPairClaimKey<'_, '_>,
        state: &PoolRegistry<'s>,
    ) -> Result<()> {
        self.health()?;
        if !self.registry_valid(state) || !key.matches_pool_in(self, state) {
            return Err(self.fatal(Cause::Invariant));
        }
        if state.total != 0
            || state.load.is_some()
            || state.claim.is_some()
            || !state.init_claim.is_some_and(|slot| {
                slot.identity == InitClaimIdentity::TxPair(key.identity)
                    && !slot.initializer_pins_held
            })
        {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }

    pub(in crate::storage) fn finish_tx_pair(
        &self,
        key: &mut TxPairClaimKey<'_, '_>,
        completed: CompletedTxPair<'_, '_>,
    ) -> Result<()> {
        self.health()?;
        let mut registry = FencedInitRegistry {
            held: self.registry.lock(),
            pool: self,
        };
        let state = registry.state()?;
        self.tx_pair_quiescent_claim_in(key, state)?;
        if !std::ptr::eq(self.core, completed.core)
            || !std::ptr::eq(self.binding, completed.binding)
            || completed.identity != key.identity
            || !completed.requirement.uses_fence(self.fence)
            || !state.directory.pair_published()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        for index in key.identity.indices() {
            if state.requirements[index].map(WalRequirement::location)
                != Some(completed.requirement.location())
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        #[cfg(test)]
        test_support::hit(self, test_support::InitPoint::PairFinishChecked);
        let state = registry.prechecked_mut();
        state.directory.finish_retention_pair();
        state.init_claim = None;
        key.active = false;
        Ok(())
    }

    pub(in crate::storage) fn cancel_tx_pair(
        &self,
        key: &mut TxPairClaimKey<'_, '_>,
        permit: TxPairCancelPermit<'_, '_, '_, '_>,
    ) -> Result<()> {
        self.health()?;
        {
            let state = self.entry_registry()?;
            self.tx_pair_quiescent_claim_in(key, &state)?;
        }
        if !key.matches_authority(permit.authority())
            || !permit.io_matches(self.binding, self.fence)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        match (permit.kind(), permit.requirement()) {
            (TxPairCancelKind::Unstaged, None) => (),
            (TxPairCancelKind::Cancelled, Some(requirement))
                if requirement.uses_fence(self.fence)
                    && requirement.location().lsn > key.identity.reserve.0
                    && permit.allocator_after().page_lsn.0 == requirement.location().lsn => {}
            _ => return Err(self.fatal(Cause::Invariant)),
        }
        // Retain the allocator observation until the same leaf returns both
        // fresh slots and retires the claim. Content always precedes registry.
        let allocator = self.frames[0]
            .content
            .try_read()
            .map_err(|error| self.load_lock_error(error))?;
        let indices = key.identity.indices();
        let first = self.frames[indices[0]]
            .content
            .try_write()
            .map_err(|error| self.load_lock_error(error))?;
        let second = self.frames[indices[1]]
            .content
            .try_write()
            .map_err(|error| self.load_lock_error(error))?;
        let mut registry = FencedInitRegistry {
            held: self.registry.lock(),
            pool: self,
        };
        let state = registry.state()?;
        self.tx_pair_quiescent_claim_in(key, state)?;
        if state.directory.pair_published()
            || self.pair_in(state, 0, &allocator)?.frame != permit.allocator_after()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        for (index, content) in [(indices[0], &*first), (indices[1], &*second)] {
            if !content.fresh()
                || state.entries[index].is_some()
                || state.requirements[index].is_some()
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        // All fallible checks precede this single joint retirement; there is
        // no unlock/relock gap between directory return and claim retirement.
        #[cfg(test)]
        test_support::hit(self, test_support::InitPoint::PairCancelChecked);
        let state = registry.prechecked_mut();
        state.directory.clear_retention_pair(indices);
        state.init_claim = None;
        key.active = false;
        Ok(())
    }

    /// Consume actual CLR completion while the original bound is exclusive.
    /// Dirty images may be discarded after logical Free; every selected DWB and
    /// physical capture must already have been retired by the sealed I/O proof.
    pub(in crate::storage) fn retire_tx_pair(
        &self,
        permit: TxPairRetirePermit<'_, '_, '_, '_>,
    ) -> Result<()> {
        self.health()?;
        if !std::ptr::eq(self.core, permit.core())
            || !permit.io_matches(self.binding, self.fence)
            || !pair_shape(permit.actor(), permit.reserve(), permit.pair())
            || !permit.requirement().uses_fence(self.fence)
            || permit.requirement().location().lsn <= permit.reserve().0
            || permit.allocator_after().page_lsn.0 != permit.requirement().location().lsn
            || self
                .directory_read(|directory| directory.closed_pair())?
                .is_some()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        if !self.handoff_quiescent()? {
            return Err(unchanged(Cause::Busy));
        }
        {
            let allocator = self.frames[0]
                .content
                .try_read()
                .map_err(|error| self.load_lock_error(error))?;
            if self.pair(0, &allocator)?.frame != permit.allocator_after() {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        let mut indices = [None; 2];
        let mut guards = [const { None }; 2];
        for at in 0..2 {
            let request = permit.pair()[at];
            // A stale incarnation at the same physical address is not absence.
            for index in 1..self.frames.len() {
                if let Some(registration) = self.resolve_registration(index)?
                    && registration.request.same_address(&request)
                {
                    if registration.request != request || indices[at].is_some() {
                        return Err(self.fatal(Cause::Invariant));
                    }
                    indices[at] = Some(index);
                }
            }
            if let Some(index) = indices[at] {
                let guard = self.frames[index]
                    .content
                    .try_write()
                    .map_err(|error| self.load_lock_error(error))?;
                if Some(self.pair(index, &guard)?.frame) != permit.before()[at] {
                    return Err(self.fatal(Cause::Invariant));
                }
                guards[at] = Some(guard);
            } else if permit.before()[at].is_some() {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        {
            let mut registry = FencedInitRegistry {
                held: self.registry.lock(),
                pool: self,
            };
            if !self.dpt_valid(registry.state()?) || registry.state()?.claim.is_some() {
                return Err(self.fatal(Cause::Invariant));
            }
            for (at, index) in indices.into_iter().enumerate() {
                if let Some(index) = index {
                    let content = guards[at].as_mut().expect("checked resident pair member");
                    content.page_lsn = PageLsn(0);
                    content.modification_counter = 0;
                    content.rec_lsn = None;
                    content.dirty = false;
                    registry.prechecked_mut().entries[index] = None;
                    registry.prechecked_mut().requirements[index] = None;
                    registry.prechecked_mut().directory.retire_retention(index);
                }
            }
        }
        drop(guards);
        Ok(())
    }
}

/// Complete parent values for the three atomic registry leaves. The original
/// entry validation owns its nested directory/claim/lookup packets separately;
/// no leaf calls another registry-locking helper while this parent is live.
fn atomic_pair_source_bytes() -> usize {
    use std::mem::size_of;
    let entry = super::super::retention_residency::entry_source_phases();
    let parent = shared_close_forwarder_bytes()
        + 2 * size_of::<&CatalogPairCandidates<'_, '_, '_>>()
        + size_of::<Option<&CatalogPairCandidates<'_, '_, '_>>>()
        + 2 * size_of::<Option<FrameStamp>>()
        + size_of::<[usize; 2]>()
        + size_of::<FencedInitRegistry<'_, '_, '_, '_, '_>>()
        + 2 * size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
        + size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
        + 2 * size_of::<TxPairClaimRecord>()
        + 2 * size_of::<TxPairClaimKey<'_, '_>>()
        + size_of::<Result<TxPairClaimKey<'_, '_>>>()
        + size_of::<CompletedTxPair<'_, '_>>()
        + size_of::<TxPairCancelPermit<'_, '_, '_, '_>>()
        + 3 * size_of::<[usize; 2]>()
        + 2 * size_of::<[u64; 2]>()
        + 4 * size_of::<TargetRequest>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<Registration>>()
        + 3 * size_of::<Result<DirtyObservation>>()
        + 2 * size_of::<Result<FrameStamp>>()
        + 5 * size_of::<Result<()>>()
        + size_of::<([&(); 16], [usize; 8], [u64; 2], [u8; 4])>();
    parent + entry.0.max(entry.1).max(entry.2).max(dpt_call_bytes())
}

/// The shared typed entry forwards into the original atomic implementation.
/// Count its additional return layer and all three borrowed arguments.
fn shared_close_forwarder_bytes() -> usize {
    std::mem::size_of::<Result<TxPairClaimKey<'_, '_>>>() + 3 * std::mem::size_of::<&()>()
}

/// Explicit source-value census, including return/Result coexistence and both
/// cleanup alternatives. Caller image/descriptor loans are counted separately.
pub(in crate::storage) fn source_census() -> usize {
    use std::mem::size_of;
    let original = shared_close_forwarder_bytes()
        + super::super::retention_residency::pair_source_census()
        + 2 * size_of::<Result<TxPairWriteTargets<'_, '_, '_, '_, '_, '_, '_>>>()
        + size_of::<TxPairWriteTargets<'_, '_, '_, '_, '_, '_, '_>>()
        + size_of::<TxPairPins<'_, '_, '_, '_, '_, '_>>()
        + size_of::<TxPairTargets<'_, '_, '_, '_, '_, '_, '_>>()
        + size_of::<Result<TxPairTargets<'_, '_, '_, '_, '_, '_, '_>>>()
        + 2 * size_of::<TxPairClaimKey<'_, '_>>()
        + size_of::<Result<TxPairClaimKey<'_, '_>>>()
        + size_of::<TxPairAuthority<'_, '_, '_>>()
        + size_of::<TxPairInstallPermit<'_, '_, '_>>()
        + size_of::<TxPairCancelPermit<'_, '_, '_, '_>>()
        + size_of::<TxPairRetirePermit<'_, '_, '_, '_>>()
        + size_of::<CompletedTxPair<'_, '_>>()
        + size_of::<Result<CompletedTxPair<'_, '_>>>()
        + size_of::<FencedInitRegistry<'_, '_, '_, '_, '_>>()
        + 3 * size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
        + size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
        + 3 * size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
        + 5 * size_of::<Result<Registration>>()
        + 3 * size_of::<Result<DirtyObservation>>()
        + 3 * size_of::<Result<FrameStamp>>()
        + 8 * size_of::<Result<()>>()
        + 2 * TARGETS * size_of::<usize>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<TxPairClaimRecord>()
        + dpt_call_bytes()
        + 1024; // bounded borrowed inputs, iterators, role masks and scalar locals
    original.max(atomic_pair_source_bytes())
}
