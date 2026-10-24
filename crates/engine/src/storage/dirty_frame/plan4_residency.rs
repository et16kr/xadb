//! Genesis-only replacement in the original directory. No INIT or dirty permit.
use super::*;
use crate::storage::wal_io::Plan4LoadPermit;

pub(in crate::storage) struct CoreCensus {
    pub(in crate::storage) load_nested: usize,
    pub(in crate::storage) candidate_nested: usize,
    pub(in crate::storage) pool: usize,
    pub(in crate::storage) directory: usize,
    pub(in crate::storage) mode: usize,
}

/// Bounded source values below the owner. Image loans and retained pool are
/// separate; these helpers hold no page array or namespace/file owner.
pub(in crate::storage) fn source_census() -> CoreCensus {
    use std::mem::size_of;
    let common = 4 * size_of::<Result<Registration>>()
        + 2 * size_of::<Result<DirtyObservation>>()
        + 2 * size_of::<Result<Option<FrameStamp>>>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<MutexGuard<'_, PinAccounting>>()
        + 2 * size_of::<MutexGuard<'_, DirtyRegistry<'_>>>()
        + 2 * size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
        + size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
        + 256; // Bounded borrowed arguments, indices, iterators and bools.
    CoreCensus {
        load_nested: common
            + 2 * size_of::<Plan4LoadSnapshot<'_, '_>>()
            + size_of::<Result<Plan4LoadSnapshot<'_, '_>>>()
            + size_of::<Plan4LoadPermit<'_, '_, '_>>()
            + 2 * size_of::<TargetRequest>()
            + 2 * size_of::<Result<FrameStamp>>(),
        candidate_nested: common
            + 3 * size_of::<Option<StatusCandidate>>()
            + size_of::<Result<StatusCandidate>>()
            + 4 * size_of::<TargetRequest>()
            + size_of::<Option<FrameStamp>>(),
        pool: size_of::<SharedPool<'_, '_, '_, '_>>(),
        directory: size_of::<RegistrationDirectory>(),
        mode: registration::plan4_mode_bytes(),
    }
}

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn new_plan4(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p [DirtyFrame<'d>],
    ) -> Result<Self> {
        if !(4..=MAX_FRAMES).contains(&frames.len()) {
            return Err(unchanged(Cause::Shape));
        }
        let mut pool = Self::new(core, fence, binding, frames)?;
        if !pool.directory.open_plan4(frames) {
            return Err(pool.fatal(Cause::Invariant));
        }
        pool.plan4_quiescent()?;
        Ok(pool)
    }

    pub(in crate::storage) fn plan4_payloads(&self) -> bool {
        self.directory.plan4_payloads()
    }

    pub(in crate::storage) fn check_plan4_reader(&self, owner: &Owner<'c, 's>) -> Result<()> {
        self.health()?;
        if !self.plan4_payloads() {
            return Err(unchanged(Cause::Shape));
        }
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        Ok(())
    }

    fn plan4_lock_error<T>(&self, source: TryLockError<T>) -> DirtyError {
        match source {
            TryLockError::WouldBlock => unchanged(Cause::Busy),
            TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
        }
    }

    fn plan4_admission(&self) -> Result<()> {
        self.health()?;
        if !self.plan4_payloads() {
            return Err(unchanged(Cause::Shape));
        }
        if !self.handoff_quiescent()? {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }

    // Check clean-only metadata BEFORE pair/identity/candidate/generation logic.
    // An unexpected dirty outsider must not be hidden by an exact resident hit.
    fn plan4_content(
        &self,
        index: usize,
        content: &FrameContent<'_>,
    ) -> Result<Option<FrameStamp>> {
        if !content.fresh() {
            return Err(self.fatal(Cause::Invariant));
        }
        if self.resolve_registration(index).is_some() {
            Ok(Some(self.pair(index, content)?.frame))
        } else {
            let dpt = self.entry_dpt()?;
            if index == 0 || dpt.entries[index].is_some() || dpt.requirements[index].is_some() {
                return Err(self.fatal(Cause::Invariant));
            }
            Ok(None)
        }
    }

    fn plan4_quiescent(&self) -> Result<()> {
        self.plan4_admission()?;
        let mut busy = false;
        for index in 0..self.frames.len() {
            let content = match self.frames[index].content.try_read() {
                Ok(content) => content,
                Err(TryLockError::WouldBlock) => {
                    busy = true;
                    continue;
                }
                Err(TryLockError::Poisoned(_)) => return Err(self.fatal(Cause::Poisoned)),
            };
            self.plan4_content(index, &content)?;
        }
        self.health()?;
        if busy {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }

    fn plan4_request(&self, request: TargetRequest) -> bool {
        self.directory.accepts_plan4_payload(request)
            && self.resolve_registration(0).is_some_and(|allocator| {
                request.identity.database == allocator.request.identity.database
                    && request.identity.page.file == allocator.request.identity.page.file
            })
    }

    pub(in crate::storage) fn plan4_allocator_snapshot(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<FrameStamp> {
        self.plan4_quiescent()?;
        let content = self.frames[0]
            .content
            .try_read()
            .map_err(|e| self.plan4_lock_error(e))?;
        let observed = self
            .plan4_content(0, &content)?
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        output.copy_from_slice(content.bytes);
        Ok(observed)
    }

    pub(in crate::storage) fn prepare_plan4_load_at(
        &mut self,
        index: usize,
        allocator_bytes: &mut [u8; PAGE_BYTES],
    ) -> Result<Plan4LoadSnapshot<'c, 's>> {
        self.plan4_quiescent()?;
        if index == 0 || index >= self.frames.len() {
            return Err(unchanged(Cause::Shape));
        }
        let generation = self
            .directory
            .next_plan4_generation(index)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        let allocator = {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.plan4_lock_error(e))?;
            self.plan4_content(0, &content)?;
            let pair = self.pair(0, &content)?;
            allocator_bytes.copy_from_slice(content.bytes);
            pair
        };
        let payload = {
            let content = self.frames[index]
                .content
                .try_read()
                .map_err(|e| self.plan4_lock_error(e))?;
            self.plan4_content(index, &content)?
        };
        self.health()?;
        Ok(Plan4LoadSnapshot {
            core: self.core,
            binding: self.binding,
            allocator,
            requirement: None,
            payload,
            generation,
            index,
        })
    }

    /// Finite exclusive scan. Required addresses are kept but never exempt from
    /// validation. No pin, registration or generation changes during this call.
    pub(in crate::storage) fn plan4_candidate(
        &mut self,
        request: TargetRequest,
        start: usize,
        required: &[TargetRequest],
    ) -> Result<StatusCandidate> {
        self.plan4_admission()?;
        if start == 0 || required.len() > 4 || !self.plan4_request(request) {
            return Err(unchanged(Cause::Shape));
        }
        let allocator = self.registration(0)?.request;
        for (index, target) in required.iter().enumerate() {
            if *target != allocator && !self.plan4_request(*target) {
                return Err(unchanged(Cause::Shape));
            }
            if required[..index]
                .iter()
                .any(|prior| prior.same_address(target))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.plan4_lock_error(e))?;
            self.plan4_content(0, &content)?;
        }
        let count = self.frames.len() - 1;
        let start = (start - 1) % count;
        let mut resident = None;
        let mut clean: Option<StatusCandidate> = None;
        let mut busy = false;
        let mut outside = false;
        for offset in 0..count {
            let index = 1 + (start + offset) % count;
            let content = match self.frames[index].content.try_read() {
                Ok(content) => content,
                Err(TryLockError::WouldBlock) => {
                    busy = true;
                    continue;
                }
                Err(TryLockError::Poisoned(_)) => return Err(self.fatal(Cause::Poisoned)),
            };
            let frame = self.plan4_content(index, &content)?;
            let registered = self.resolve_registration(index).map(|entry| entry.request);
            if let Some(actual) = registered {
                if actual.same_address(&request) {
                    if actual != request {
                        return Err(unchanged(Cause::Stale));
                    }
                    resident = Some(StatusCandidate {
                        kind: StatusCandidateKind::Resident,
                        index,
                        frame,
                        next: 1 + index % count,
                    });
                }
                for wanted in required {
                    if actual.same_address(wanted) && actual != *wanted {
                        return Err(unchanged(Cause::Stale));
                    }
                }
            }
            let keep = registered.is_some_and(|actual| required.contains(&actual));
            outside |= !keep;
            // Preserve resident pages while a reusable empty slot remains. Keep
            // the first candidate in each class without shortening the scan.
            if !keep
                && clean.is_none_or(|candidate| candidate.frame.is_some() && frame.is_none())
                && self.directory.next_plan4_generation(index).is_some()
            {
                clean = Some(StatusCandidate {
                    kind: StatusCandidateKind::Load,
                    index,
                    frame,
                    next: 1 + index % count,
                });
            }
        }
        self.health()?;
        if busy {
            return Err(unchanged(Cause::Busy));
        }
        resident.or(clean).ok_or_else(|| {
            unchanged(if outside {
                Cause::CounterExhausted
            } else {
                Cause::Busy
            })
        })
    }

    pub(in crate::storage) fn install_plan4_load(
        &mut self,
        permit: Plan4LoadPermit<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.plan4_quiescent()?;
        let snapshot = permit.snapshot();
        if permit.index() != snapshot.index
            || snapshot.index == 0
            || snapshot.index >= self.frames.len()
            || !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.directory.next_plan4_generation(snapshot.index) != Some(snapshot.generation)
            || snapshot.requirement.is_some()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.plan4_lock_error(e))?;
            self.plan4_content(0, &content)?;
            if self.pair(0, &content)? != snapshot.allocator || content.bytes != permit.allocator()
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        let mut content = self.frames[snapshot.index]
            .content
            .try_write()
            .map_err(|e| self.plan4_lock_error(e))?;
        if self.plan4_content(snapshot.index, &content)? != snapshot.payload {
            return Err(self.fatal(Cause::Invariant));
        }
        let request = permit.request();
        if !self.plan4_request(request) || self.directory.address_present(request) {
            return Err(self.fatal(Cause::Invariant));
        }
        self.health()?;
        let stamp = FrameStamp {
            identity: request.identity,
            generation: snapshot.generation,
            page_lsn: PageLsn(0),
            modification_counter: 0,
            rec_lsn: None,
        };
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = PageLsn(0);
        content.modification_counter = 0;
        content.rec_lsn = None;
        content.dirty = false;
        self.directory.replace_plan4(
            snapshot.index,
            Registration {
                request,
                generation: stamp.generation,
            },
        );
        Ok(stamp)
    }
}

#[cfg(test)]
pub(in crate::storage) mod test_support {
    use super::*;
    pub(in crate::storage) fn seed_dirty(pool: &SharedPool<'_, '_, '_, '_>, index: usize) {
        pool.frames[index].content.write().unwrap().dirty = true;
    }
    pub(in crate::storage) fn seed_generation(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        index: usize,
        generation: u64,
    ) {
        assert!(pool.plan4_payloads() && index > 0 && generation != 0);
        let mut registration = pool.resolve_registration(index).unwrap();
        registration.generation = generation;
        pool.directory.replace_plan4(index, registration);
    }
}
