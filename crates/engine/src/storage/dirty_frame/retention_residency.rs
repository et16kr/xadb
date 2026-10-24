//! Live plan4 residency in the original directory and DPT. The old genesis
//! mode remains clean-only; this mode consumes separately sealed history proof.
use super::*;
use crate::storage::wal_io::{RetentionLoadPermit, WalRequirement};

/// New live calls keep the original pool backing; scratch images remain loans.
pub(in crate::storage) fn source_census() -> usize {
    use std::mem::size_of;
    // Conservatively sum the candidate and load scopes instead of relying on
    // optimizer liveness or machine stack allocation.
    8 * size_of::<Result<()>>()
        + 4 * size_of::<Result<Registration>>()
        + 4 * size_of::<Result<DirtyObservation>>()
        + 6 * size_of::<Result<Option<FrameStamp>>>()
        + 3 * size_of::<Option<StatusCandidate>>()
        + 2 * size_of::<Result<StatusCandidate>>()
        + 2 * size_of::<StatusLoadSnapshot<'_, '_>>()
        + size_of::<Result<StatusLoadSnapshot<'_, '_>>>()
        + size_of::<RetentionLoadPermit<'_, '_, '_>>()
        + 6 * size_of::<TargetRequest>()
        + 4 * size_of::<Result<FrameStamp>>()
        + 2 * size_of::<MutexGuard<'_, PinAccounting>>()
        + 4 * size_of::<MutexGuard<'_, DirtyRegistry<'_>>>()
        + 3 * size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
        + size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + size_of::<
            std::result::Result<
                crate::storage::page::PageView<'_>,
                crate::storage::page::PageError,
            >,
        >()
        + 2048 // Bounded hash/envelope locals, indices, borrowed inputs and bools.
}

impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    pub(in crate::storage) fn activate_plan4_retention(&mut self) -> Result<()> {
        self.plan4_quiescent()?;
        if !self.directory.activate_retention(self.frames) {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(())
    }

    pub(in crate::storage) fn retention_payloads(&self) -> bool {
        self.directory.retention_payloads()
    }

    pub(in crate::storage) fn check_retention_reader(&self, owner: &Owner<'c, 's>) -> Result<()> {
        self.health()?;
        if !self.retention_payloads() {
            return Err(unchanged(Cause::Shape));
        }
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        Ok(())
    }

    fn retention_quiescent(&self) -> Result<()> {
        self.health()?;
        if !self.retention_payloads() {
            return Err(unchanged(Cause::Shape));
        }
        if !self.handoff_quiescent()? {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }

    fn retention_request(&self, request: TargetRequest) -> bool {
        self.directory.accepts_retention_payload(request)
            && self.resolve_registration(0).is_some_and(|allocator| {
                request.identity.database == allocator.request.identity.database
                    && request.identity.page.file == allocator.request.identity.page.file
            })
    }

    pub(in crate::storage) fn retention_allocator_snapshot(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<FrameStamp> {
        self.retention_quiescent()?;
        let content = self.frames[0]
            .content
            .try_read()
            .map_err(|e| self.load_lock_error(e))?;
        let pair = self.pair(0, &content)?;
        output.copy_from_slice(content.bytes);
        Ok(pair.frame)
    }

    /// Required addresses are protected from replacement but not exempt from
    /// validation. A full scan precedes selection, including after a resident hit.
    pub(in crate::storage) fn retention_candidate(
        &mut self,
        request: TargetRequest,
        start: usize,
        required: &[TargetRequest],
    ) -> Result<StatusCandidate> {
        self.retention_quiescent()?;
        if start == 0 || required.len() > 4 || !self.retention_request(request) {
            return Err(unchanged(Cause::Shape));
        }
        let allocator = self.registration(0)?.request;
        for (index, target) in required.iter().enumerate() {
            if *target != allocator && !self.retention_request(*target) {
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
                .map_err(|e| self.load_lock_error(e))?;
            self.pair(0, &content)?;
        }
        let count = self.frames.len() - 1;
        let start = (start - 1) % count;
        let mut resident = None;
        let mut clean: Option<StatusCandidate> = None;
        let mut dirty: Option<StatusCandidate> = None;
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
            let registered = self.resolve_registration(index).map(|entry| entry.request);
            let frame = if let Some(actual) = registered {
                let frame = self.pair(index, &content)?.frame;
                if actual.same_address(&request) {
                    if actual != request {
                        return Err(unchanged(Cause::Stale));
                    }
                    resident = Some(StatusCandidate {
                        kind: StatusCandidateKind::Resident,
                        index,
                        frame: Some(frame),
                        next: 1 + index % count,
                    });
                }
                for wanted in required {
                    if actual.same_address(wanted) && actual != *wanted {
                        return Err(unchanged(Cause::Stale));
                    }
                }
                Some(frame)
            } else {
                let dpt = self.entry_dpt()?;
                if !content.fresh()
                    || dpt.entries[index].is_some()
                    || dpt.requirements[index].is_some()
                {
                    return Err(self.fatal(Cause::Invariant));
                }
                None
            };
            let keep = registered.is_some_and(|actual| required.contains(&actual));
            outside |= !keep;
            if keep || self.directory.next_retention_generation(index).is_none() {
                continue;
            }
            let candidate = StatusCandidate {
                kind: if content.dirty {
                    StatusCandidateKind::Flush
                } else {
                    StatusCandidateKind::Load
                },
                index,
                frame,
                next: 1 + index % count,
            };
            if content.dirty {
                let rec = frame
                    .and_then(|value| value.rec_lsn)
                    .ok_or_else(|| self.fatal(Cause::Invariant))?;
                if dirty.is_none_or(|prior| {
                    rec.0
                        < prior
                            .frame
                            .expect("dirty candidate")
                            .rec_lsn
                            .expect("dirty recLSN")
                            .0
                }) {
                    dirty = Some(candidate);
                }
            } else if clean.is_none_or(|prior| prior.frame.is_some() && frame.is_none()) {
                clean = Some(candidate);
            }
        }
        self.health()?;
        if busy {
            return Err(unchanged(Cause::Busy));
        }
        resident.or(clean).or(dirty).ok_or_else(|| {
            unchanged(if outside {
                Cause::CounterExhausted
            } else {
                Cause::Busy
            })
        })
    }

    /// Exclusive clean-slot reservation before the actual allocation operation.
    /// The original initializer claim remains the only shared publication path.
    pub(in crate::storage) fn prepare_retention_slot(
        &mut self,
        index: usize,
    ) -> Result<Option<FrameStamp>> {
        self.retention_quiescent()?;
        if index == 0 || index >= self.frames.len() {
            return Err(unchanged(Cause::Shape));
        }
        let generation = self
            .directory
            .next_retention_generation(index)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
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
        self.directory.prepare_retention_slot(index, generation);
        content.page_lsn = PageLsn(0);
        content.modification_counter = 0;
        content.rec_lsn = None;
        content.dirty = false;
        Ok(before)
    }

    pub(in crate::storage) fn prepare_retention_load_at(
        &mut self,
        index: usize,
        allocator_bytes: &mut [u8; PAGE_BYTES],
    ) -> Result<StatusLoadSnapshot<'c, 's>> {
        self.retention_quiescent()?;
        if index == 0 || index >= self.frames.len() {
            return Err(unchanged(Cause::Shape));
        }
        let generation = self
            .directory
            .next_retention_generation(index)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        let allocator = {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|e| self.load_lock_error(e))?;
            let pair = self.pair(0, &content)?;
            allocator_bytes.copy_from_slice(content.bytes);
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

    pub(in crate::storage) fn install_retention_load(
        &mut self,
        permit: RetentionLoadPermit<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.retention_quiescent()?;
        let snapshot = permit.snapshot();
        if permit.index() != snapshot.index
            || snapshot.index == 0
            || snapshot.index >= self.frames.len()
            || !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.directory.next_retention_generation(snapshot.index) != Some(snapshot.generation)
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
        let request = permit.request();
        if !self.retention_request(request) || self.directory.address_present(request) {
            return Err(self.fatal(Cause::Invariant));
        }
        // The permit owns the independent canonical-image proof. Its checked
        // header LSN is an observation; no requirement is fabricated for load.
        let header =
            super::super::page::decode_page(permit.bytes(), request.identity, request.kind)
                .map_err(|_| self.fatal(Cause::Invariant))?;
        if header.header().level != request.level {
            return Err(self.fatal(Cause::Invariant));
        }
        let stamp = FrameStamp {
            identity: request.identity,
            generation: snapshot.generation,
            page_lsn: header.header().lsn,
            modification_counter: u64::from(header.header().lsn.0 != 0),
            rec_lsn: None,
        };
        self.health()?;
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = stamp.page_lsn;
        content.modification_counter = stamp.modification_counter;
        content.rec_lsn = None;
        content.dirty = false;
        self.directory.replace_retention(
            snapshot.index,
            Registration {
                request,
                generation: stamp.generation,
            },
        );
        Ok(stamp)
    }
}
