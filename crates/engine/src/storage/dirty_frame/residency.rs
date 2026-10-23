//! Same-address clean allocator epoch renewal under an exclusive pool borrow.
//! Source I/O/semantic checks belong to the consuming storage owner; only its
//! sealed loaded-image permit can install a new resident generation here.
use super::*;
use crate::storage::wal_io::ReloadInstallPermit;

pub(in crate::storage) struct ReloadSnapshot<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    registration: Registration,
    stamp: FrameStamp,
    index: usize,
    epoch: u64,
}
pub(in crate::storage) struct AllocatorLoadClaim<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    index: usize,
    registration: Registration,
    epoch: u64,
}
impl ReloadSnapshot<'_, '_> {
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.registration.request
    }
    pub(in crate::storage) fn stamp(&self) -> FrameStamp {
        self.stamp
    }
}
impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    fn reload_quiescent(&self) -> Result<()> {
        {
            let pins = self.entry_pins()?;
            if pins.total != 0 || pins.init_claim.is_some() {
                return Err(unchanged(Cause::Busy));
            }
        }
        let dpt = self.entry_dpt()?;
        if dpt.claim.is_some()
            || dpt.entries.iter().any(Option::is_some)
            || dpt.requirements.iter().any(Option::is_some)
        {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }
    pub(in crate::storage) fn enable_allocator_reload(&mut self) -> Result<()> {
        self.health()?;
        self.reload_quiescent()?;
        {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|error| match error {
                    TryLockError::WouldBlock => unchanged(Cause::Busy),
                    TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
                })?;
            self.pair(0, &content)?;
        }
        if !self.directory.enable_allocator_reload(self.frames) {
            return Err(unchanged(Cause::Shape));
        }
        Ok(())
    }
    /// No new generation is published. The output is caller scratch only.
    pub(in crate::storage) fn prepare_allocator_reload(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<ReloadSnapshot<'c, 's>> {
        self.prepare_allocator_snapshot(output, true)
    }
    pub(in crate::storage) fn prepare_allocator_eviction(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<ReloadSnapshot<'c, 's>> {
        self.prepare_allocator_snapshot(output, false)
    }
    fn prepare_allocator_snapshot(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
        renew: bool,
    ) -> Result<ReloadSnapshot<'c, 's>> {
        self.health()?;
        self.reload_quiescent()?;
        let index = self
            .directory
            .allocator_index()
            .ok_or_else(|| unchanged(Cause::MissingTarget))?;
        let registration = self.registration(index)?;
        if (renew && !self.directory.can_renew_allocator(index, registration))
            || (!renew && self.directory.eviction_epoch == u64::MAX)
        {
            return Err(unchanged(Cause::CounterExhausted));
        }
        let content = match self.frames[index].content.try_read() {
            Ok(content) => content,
            Err(TryLockError::WouldBlock) => return Err(unchanged(Cause::Busy)),
            Err(TryLockError::Poisoned(error)) => {
                let failure = self.fatal(Cause::Poisoned);
                drop(error.into_inner());
                return Err(failure);
            }
        };
        let before = self.pair(index, &content)?;
        if content.dirty || before.entry.is_some() || before.frame.rec_lsn.is_some() {
            return Err(unchanged(Cause::Busy));
        }
        output.copy_from_slice(content.bytes);
        drop(content);
        self.health()?;
        Ok(ReloadSnapshot {
            core: self.core,
            binding: self.binding,
            registration,
            stamp: before.frame,
            index,
            epoch: self.directory.eviction_epoch,
        })
    }
    /// All fallible checks precede renewal/copy. An unwind fences the original
    /// storage; no old-generation completion may then service the new epoch.
    pub(in crate::storage) fn install_allocator_reload(
        &mut self,
        permit: ReloadInstallPermit<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.health()?;
        let snapshot = permit.snapshot();
        if !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.registration(snapshot.index)? != snapshot.registration
            || self.directory.eviction_epoch != snapshot.epoch
            || !self
                .directory
                .can_renew_allocator(snapshot.index, snapshot.registration)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        self.reload_quiescent()?;
        let mut content = match self.frames[snapshot.index].content.try_write() {
            Ok(content) => content,
            Err(TryLockError::WouldBlock) => return Err(unchanged(Cause::Busy)),
            Err(TryLockError::Poisoned(error)) => {
                let failure = self.fatal(Cause::Poisoned);
                drop(error.into_inner());
                return Err(failure);
            }
        };
        if self.pair(snapshot.index, &content)?.frame != snapshot.stamp || content.dirty {
            return Err(self.fatal(Cause::Invariant));
        }
        self.health()?;
        let Some(generation) = self
            .directory
            .renew_allocator(snapshot.index, snapshot.registration)
        else {
            return Err(self.fatal(Cause::Invariant));
        };
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = snapshot.stamp.page_lsn;
        content.modification_counter = u64::from(content.page_lsn.0 != 0);
        content.dirty = false;
        content.rec_lsn = None;
        Ok(FrameStamp {
            generation,
            modification_counter: content.modification_counter,
            ..snapshot.stamp
        })
    }

    pub(in crate::storage) fn evict_allocator_frame(
        &mut self,
        mut snapshot: ReloadSnapshot<'c, 's>,
    ) -> Result<ReloadSnapshot<'c, 's>> {
        self.health()?;
        self.reload_quiescent()?;
        if !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.directory.eviction_epoch != snapshot.epoch
            || self.registration(snapshot.index)? != snapshot.registration
        {
            return Err(self.fatal(Cause::Invariant));
        }
        {
            let content = self.frames[snapshot.index]
                .content
                .try_read()
                .map_err(|error| match error {
                    TryLockError::WouldBlock => unchanged(Cause::Busy),
                    TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
                })?;
            if self.pair(snapshot.index, &content)?.frame != snapshot.stamp || content.dirty {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        snapshot.epoch = self
            .directory
            .evict_allocator(snapshot.index, snapshot.registration)
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        Ok(snapshot)
    }

    pub(in crate::storage) fn claim_allocator_load(
        &mut self,
        snapshot: &ReloadSnapshot<'c, 's>,
        index: usize,
    ) -> Result<AllocatorLoadClaim<'c, 's>> {
        self.health()?;
        self.reload_quiescent()?;
        if !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.directory.eviction_epoch != snapshot.epoch
            || self.directory.allocator_index().is_some()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        if index >= self.frames.len() {
            return Err(unchanged(Cause::Shape));
        }
        // Vacant frames may retain old bytes. They must retain no dirty effect.
        {
            let content = self.frames[index]
                .content
                .try_read()
                .map_err(|error| match error {
                    TryLockError::WouldBlock => unchanged(Cause::Busy),
                    TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
                })?;
            if !content.valid() || content.dirty || content.rec_lsn.is_some() {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        let registration = self
            .directory
            .claim_allocator_load(index, snapshot.request())
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        Ok(AllocatorLoadClaim {
            core: self.core,
            binding: self.binding,
            index,
            registration,
            epoch: snapshot.epoch,
        })
    }

    pub(in crate::storage) fn install_evicted_allocator(
        &mut self,
        claim: AllocatorLoadClaim<'c, 's>,
        permit: ReloadInstallPermit<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.health()?;
        self.reload_quiescent()?;
        let snapshot = permit.snapshot();
        if !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || !std::ptr::eq(self.core, claim.core)
            || !std::ptr::eq(self.binding, claim.binding)
            || claim.epoch != snapshot.epoch
            || claim.registration.request != snapshot.request()
            || !self
                .directory
                .matches_load(claim.index, claim.registration, claim.epoch)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        let mut content =
            self.frames[claim.index]
                .content
                .try_write()
                .map_err(|error| match error {
                    TryLockError::WouldBlock => unchanged(Cause::Busy),
                    TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
                })?;
        if !content.valid() || content.dirty || content.rec_lsn.is_some() {
            return Err(self.fatal(Cause::Invariant));
        }
        self.health()?;
        // All fallible checks precede bytes and publication. The exclusive pool
        // borrow keeps the newly installed frame inaccessible until return.
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = snapshot.stamp.page_lsn;
        content.modification_counter = u64::from(content.page_lsn.0 != 0);
        content.dirty = false;
        content.rec_lsn = None;
        self.directory.publish_load(claim.index);
        Ok(FrameStamp {
            generation: claim.registration.generation,
            modification_counter: content.modification_counter,
            ..snapshot.stamp
        })
    }
}

#[cfg(test)]
pub(in crate::storage) mod test_support {
    use super::*;
    pub(in crate::storage) fn replace_claim_binding<'s>(
        claim: &mut AllocatorLoadClaim<'_, 's>,
        binding: &'s AtomicU8,
    ) {
        claim.binding = binding;
    }
    pub(in crate::storage) fn seed_generation(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        generation: u64,
    ) {
        pool.directory.test_seed_reload_generation(generation);
    }
    pub(in crate::storage) fn resident_slot(pool: &SharedPool<'_, '_, '_, '_>) -> Option<usize> {
        pool.directory.allocator_index()
    }
    pub(in crate::storage) fn seed_eviction_epoch(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        epoch: u64,
    ) {
        pool.directory.eviction_epoch = epoch;
    }
}
