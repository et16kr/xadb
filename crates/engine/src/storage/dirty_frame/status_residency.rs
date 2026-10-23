//! Exclusive Status replacement with a preserved allocator and old residency.
//! A storage-owner permit is issued only after selected home I/O and semantics.
use super::*;
use crate::storage::wal_io::{StatusLoadPermit, WalFrameLocation};

pub(in crate::storage) struct StatusLoadSnapshot<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    allocator: DirtyObservation,
    requirement: Option<WalFrameLocation>,
    payload: Option<FrameStamp>,
    generation: u64,
    index: usize,
}
impl StatusLoadSnapshot<'_, '_> {
    pub(in crate::storage) fn allocator(&self) -> FrameStamp {
        self.allocator.frame
    }
    pub(in crate::storage) fn payload(&self) -> Option<FrameStamp> {
        self.payload
    }
}
impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    fn status_load_quiescent(&self) -> Result<()> {
        self.health()?;
        if !self.directory.operational_status()
            || (!self.directory.resident_status() && self.frames.len() != 2)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        {
            let pins = self.entry_pins()?;
            if pins.total != 0 || pins.init_claim.is_some() {
                return Err(unchanged(Cause::Busy));
            }
        }
        // entry_dpt validates origins and entry/requirement presence. The two
        // content pairs below distinguish valid dirty state from corruption.
        if self.entry_dpt()?.claim.is_some() {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }
    fn status_payload(
        &self,
        index: usize,
        content: &FrameContent<'_>,
    ) -> Result<Option<FrameStamp>> {
        let stamp = if self.resolve_registration(index).is_some() {
            Some(self.pair(index, content)?.frame)
        } else {
            let dpt = self.entry_dpt()?;
            if !content.fresh() || dpt.entries[index].is_some() || dpt.requirements[index].is_some()
            {
                return Err(self.fatal(Cause::Invariant));
            }
            None
        };
        if content.dirty {
            return Err(unchanged(Cause::Busy));
        }
        Ok(stamp)
    }
    fn status_generation(&self, index: usize) -> Option<u64> {
        if index == 0 || index >= self.frames.len() {
            return None;
        }
        if self.directory.resident_status() {
            self.directory.next_resident_generation(index)
        } else if index == 1 {
            self.directory.next_status_generation()
        } else {
            None
        }
    }
    pub(in crate::storage) fn resident_address_present(
        &self,
        request: TargetRequest,
    ) -> Result<bool> {
        self.health()?;
        if !self.registrations_valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(self.directory.address_present(request))
    }
    pub(in crate::storage) fn resident_request(
        &self,
        request: TargetRequest,
    ) -> Result<Option<TargetRequest>> {
        self.health()?;
        if !self.registrations_valid() {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(self
            .directory
            .lookup(self.frames, request)
            .map(|(_, entry)| entry.request))
    }
    /// Exclusive, clean-only eviction. Other dirty pages keep their DPT entries.
    pub(in crate::storage) fn prepare_resident_slot(
        &mut self,
        index: usize,
    ) -> Result<Option<FrameStamp>> {
        self.status_load_quiescent()?;
        if !self.directory.resident_status() || index == 0 || index >= self.frames.len() {
            return Err(unchanged(Cause::Shape));
        }
        let generation = self
            .status_generation(index)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        // Validate allocator metadata too, even though no new page image is read.
        {
            let allocator = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.load_lock_error(e))?;
            self.pair(0, &allocator)?;
        }
        let mut content = self.frames[index]
            .content
            .try_write()
            .map_err(|e| self.load_lock_error(e))?;
        let before = self.status_payload(index, &content)?;
        if before.is_none() && self.directory.closed_index() == Some(index) {
            return Err(unchanged(Cause::MissingTarget));
        }
        self.health()?;
        self.directory.prepare_resident_slot(index, generation);
        content.page_lsn = PageLsn(0);
        content.modification_counter = 0;
        content.rec_lsn = None;
        content.dirty = false;
        Ok(before)
    }
    /// Copies the actual current allocator before any semantic observation. No
    /// lock, pin, generation or payload mutation survives this preparation.
    pub(in crate::storage) fn prepare_status_load(
        &mut self,
        before: &mut [u8; PAGE_BYTES],
    ) -> Result<StatusLoadSnapshot<'c, 's>> {
        self.prepare_status_load_at(1, before)
    }
    pub(in crate::storage) fn prepare_status_load_at(
        &mut self,
        index: usize,
        before: &mut [u8; PAGE_BYTES],
    ) -> Result<StatusLoadSnapshot<'c, 's>> {
        self.status_load_quiescent()?;
        if index == 0
            || index >= self.frames.len()
            || (!self.directory.resident_status() && index != 1)
        {
            return Err(unchanged(Cause::Shape));
        }
        let generation = self
            .status_generation(index)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        let allocator = {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.load_lock_error(e))?;
            let pair = self.pair(0, &content)?;
            before.copy_from_slice(content.bytes);
            pair
        };
        let payload = {
            let content = self.frames[index]
                .content
                .try_read()
                .map_err(|e| self.load_lock_error(e))?;
            self.status_payload(index, &content)?
        };
        let requirement = self.entry_dpt()?.requirements[0].map(WalRequirement::location);
        self.health()?;
        Ok(StatusLoadSnapshot {
            core: self.core,
            binding: self.binding,
            allocator,
            requirement,
            payload,
            generation,
            index,
        })
    }
    fn load_lock_error<T>(&self, error: TryLockError<T>) -> DirtyError {
        match error {
            TryLockError::WouldBlock => unchanged(Cause::Busy),
            TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
        }
    }
    /// Publication is infallible after every origin, byte, metadata and receipt
    /// comparison. Exclusive access prevents readers seeing the copy in flight.
    pub(in crate::storage) fn install_status_load(
        &mut self,
        permit: StatusLoadPermit<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.status_load_quiescent()?;
        let snapshot = permit.snapshot();
        if permit.index() != snapshot.index
            || !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.status_generation(snapshot.index) != Some(snapshot.generation)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.load_lock_error(e))?;
            if self.pair(0, &content)? != snapshot.allocator
                || content.bytes != permit.allocator()
                || self.entry_dpt()?.requirements[0].map(WalRequirement::location)
                    != snapshot.requirement
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        let mut content = self.frames[snapshot.index]
            .content
            .try_write()
            .map_err(|e| self.load_lock_error(e))?;
        if self.status_payload(snapshot.index, &content)? != snapshot.payload {
            return Err(self.fatal(Cause::Invariant));
        }
        self.health()?;
        let request = permit.request();
        let stamp = FrameStamp {
            identity: request.identity,
            generation: snapshot.generation,
            page_lsn: permit.lsn(),
            modification_counter: 1,
            rec_lsn: None,
        };
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = stamp.page_lsn;
        content.modification_counter = 1;
        content.rec_lsn = None;
        content.dirty = false;
        let registration = Registration {
            request,
            generation: stamp.generation,
        };
        if self.directory.resident_status() {
            self.directory
                .replace_resident_status(snapshot.index, registration);
        } else {
            self.directory.replace_status(registration);
        }
        Ok(stamp)
    }
}

#[cfg(test)]
pub(in crate::storage) mod test_support {
    use super::*;
    pub(in crate::storage) fn seed_generation(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        index: usize,
        generation: u64,
    ) {
        pool.directory.seed_resident_generation(index, generation);
    }
    pub(in crate::storage) fn edit_content(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        edit: impl FnOnce(&mut [u8; PAGE_BYTES]),
    ) {
        edit(pool.frames[index].content.write().unwrap().bytes);
    }
    pub(in crate::storage) fn hold_write(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        f: impl FnOnce(),
    ) {
        let _held = pool.frames[index].content.write().unwrap();
        f();
    }
    pub(in crate::storage) fn slot(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
    ) -> Option<(TargetRequest, u64)> {
        pool.resolve_registration(index)
            .map(|r| (r.request, r.generation))
    }
    pub(in crate::storage) fn corrupt_index(
        snapshot: &mut StatusLoadSnapshot<'_, '_>,
        index: usize,
    ) {
        snapshot.index = index;
    }
    pub(in crate::storage) fn generation(pool: &SharedPool<'_, '_, '_, '_>) -> u64 {
        pool.directory.init_generations(pool.frames).unwrap()[1]
    }
    pub(in crate::storage) fn stale(snapshot: &mut StatusLoadSnapshot<'_, '_>) {
        snapshot.generation -= 1;
    }
    pub(in crate::storage) fn foreign<'s>(
        snapshot: &mut StatusLoadSnapshot<'_, 's>,
        binding: &'s AtomicU8,
    ) {
        snapshot.binding = binding;
    }
    pub(in crate::storage) fn corrupt_requirement(pool: &SharedPool<'_, '_, '_, '_>) {
        pool.dpt.lock().unwrap().requirements[0] = None;
    }
    pub(in crate::storage) fn corrupt_allocator(pool: &SharedPool<'_, '_, '_, '_>) {
        pool.frames[0].content.write().unwrap().bytes[128] ^= 1;
    }
}
