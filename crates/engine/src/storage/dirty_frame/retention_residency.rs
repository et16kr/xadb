//! Live plan4 residency in the original directory and DPT. The old genesis
//! mode remains clean-only; this mode consumes separately sealed history proof.
use super::*;
use crate::storage::wal_io::{RetentionLoadPermit, WalRequirement};

#[cfg(test)]
#[path = "retention_residency_tests.rs"]
mod tests;

/// Observation of retention_payloads/check_retention_reader. The registry's
/// guard does not escape either API; later Owner checking uses the core leaf.
/// Columns are guardless waiting, guarded work, guardless returned values.
#[cfg(test)]
pub(in crate::storage) fn entry_source_observation() -> (usize, usize, usize) {
    use crate::storage::{
        allocation_page::SystemWorkRef,
        page::{PageError, PageRef},
        transaction_record::TxnRef,
        wal_io::WalFrameLocation,
    };
    use crate::{api::Namespace, transaction::TxnId};
    use std::{mem::size_of, ops::Range};
    type Guard<'a> = MutexGuard<'a, PoolRegistry<'a>>;
    let waiting = 8 * size_of::<&()>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<Result<bool>>()
        + 2 * size_of::<DirtyError>();
    let locks = 4 * size_of::<Guard<'_>>()
        + 2 * size_of::<std::sync::LockResult<Guard<'_>>>()
        + 2 * size_of::<Result<Guard<'_>>>()
        + 2 * size_of::<std::sync::PoisonError<Guard<'_>>>();
    let directory = registration::retention_source_observation();
    let rank = 2 * size_of::<(u64, u64, u64)>()
        + size_of::<[u8; 24]>()
        + 3 * size_of::<[u8; 8]>()
        + 2 * size_of::<PageRef>()
        + 3 * size_of::<std::result::Result<(), PageError>>();
    // System's borrowed claim remains while init_generations calls valid on
    // this same directory. Keep the nested directory closure in this branch.
    let system_init = 2 * size_of::<SystemWorkRef>()
        + size_of::<Namespace>()
        + rank
        + 2 * size_of::<Option<[u64; 2]>>()
        + size_of::<[u64; 2]>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<Registration>>()
        + 2 * size_of::<Range<usize>>()
        + directory;
    let pair_init = 2 * size_of::<TxnRef>()
        + 2 * size_of::<TxnId>()
        + size_of::<PageLsn>()
        + size_of::<[u64; 2]>()
        + 2 * size_of::<Option<[u64; 2]>>()
        + 3 * size_of::<[usize; 2]>()
        + 2 * size_of::<Option<[usize; 2]>>()
        + size_of::<std::array::IntoIter<u64, 2>>()
        + 2 * size_of::<std::array::IntoIter<usize, 2>>()
        + size_of::<std::iter::Enumerate<std::array::IntoIter<usize, 2>>>()
        + 2 * size_of::<std::slice::Iter<'_, TargetRequest>>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<Registration>>()
        + size_of::<TargetRequest>()
        + size_of::<(u64, u64, PageKind)>()
        + 2 * size_of::<Range<usize>>()
        + rank
        + directory;
    // Both the load and its flush/INIT conflict inputs remain borrowed under
    // this same registry guard. Full original claims are retained in the pool.
    let load = 2 * size_of::<concurrency::OwnerIdentity>()
        + 2 * size_of::<Option<&flush::Claim>>()
        + 2 * size_of::<&flush::Claim>()
        + 2 * size_of::<Option<&init::InitClaimSlot>>()
        + 2 * size_of::<&init::InitClaimSlot>()
        + 2 * size_of::<TargetRequest>()
        + size_of::<(Option<Registration>, Option<FrameStamp>)>()
        + size_of::<Registration>()
        + size_of::<FrameStamp>()
        + 2 * size_of::<Option<(PageKind, u8)>>()
        + size_of::<std::slice::Iter<'_, TargetRequest>>()
        + directory;
    let flush = 2 * size_of::<Option<FrameStamp>>()
        + 2 * size_of::<FrameStamp>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<Registration>>()
        + 2 * size_of::<Option<PageLsn>>()
        + directory;
    let dpt = 2 * size_of::<Option<WalRequirement<'_>>>()
        + 2 * size_of::<WalRequirement<'_>>()
        + 2 * size_of::<WalFrameLocation>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Option<Registration>>()
        + size_of::<std::iter::Enumerate<std::slice::Iter<'_, Option<DptEntry>>>>()
        + 2 * size_of::<(usize, &Option<DptEntry>)>()
        + directory;
    let pin_count = size_of::<std::iter::Enumerate<std::slice::Iter<'_, u16>>>()
        + 2 * size_of::<(usize, &u16)>()
        + size_of::<Option<u16>>()
        + size_of::<u16>()
        + size_of::<Option<&init::InitClaimSlot>>()
        + size_of::<Option<&reader_load::LoadClaimRecord>>()
        + size_of::<[usize; 2]>()
        + directory;
    // At most13 nested helpers: check -> retention_payloads -> directory_read
    // -> entry -> registry_valid -> pins -> init -> valid_in -> init_generations
    // -> directory.valid -> valid_retention -> Slot.registration -> map.
    // Each has <=64B scalar/ref/closure arguments beyond explicit packets.
    let auxiliary = 13 * size_of::<(&(), &(), &(), &[u8], usize, u64, [bool; 4])>()
        + 2 * size_of::<Range<usize>>()
        + 2 * size_of::<std::slice::Iter<'_, u8>>()
        + size_of::<std::ops::RangeInclusive<usize>>();
    let guarded = locks
        + auxiliary
        + directory
            .max(system_init)
            .max(pair_init)
            .max(load)
            .max(flush)
            .max(dpt)
            .max(pin_count);
    let returning = 4 * size_of::<Result<()>>()
        + 4 * size_of::<Result<bool>>()
        + 4 * size_of::<DirtyError>()
        + 2 * size_of::<concurrency::ConcurrencyError>()
        + 6 * size_of::<&()>();
    (waiting, guarded, returning)
}

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
        + size_of::<Option<TargetRequest>>()
        + size_of::<Result<[StatusCandidate; 2]>>()
        + 4 * size_of::<Result<FrameStamp>>()
        + 2 * size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
        + 4 * size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
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

pub(super) fn pair_source_census() -> usize {
    use std::mem::size_of;
    source_census()
        + size_of::<StatusCandidate>()
        + 2 * size_of::<Result<[StatusCandidate; 2]>>()
        + size_of::<[StatusCandidate; 2]>()
        + 2 * size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
        + 3 * size_of::<Result<[u64; 2]>>()
        + size_of::<[u64; 2]>()
        + 2 * size_of::<Result<Option<FrameStamp>>>()
        + 256 // extra borrowed inputs, selected indices and checked-generation locals
}

impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    pub(in crate::storage) fn activate_plan4_retention(&mut self) -> Result<()> {
        self.plan4_quiescent()?;
        if !self.directory_edit(|directory| directory.activate_retention(self.frames))? {
            return Err(self.fatal(Cause::Invariant));
        }
        Ok(())
    }

    pub(in crate::storage) fn retention_payloads(&self) -> Result<bool> {
        self.directory_read(|directory| directory.retention_payloads())
    }

    pub(in crate::storage) fn check_retention_reader(&self, owner: &Owner<'c, 's>) -> Result<()> {
        self.health()?;
        if !self.retention_payloads()? {
            return Err(unchanged(Cause::Shape));
        }
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        Ok(())
    }

    fn retention_quiescent(&self) -> Result<()> {
        self.health()?;
        if !self.retention_payloads()? {
            return Err(unchanged(Cause::Shape));
        }
        if !self.handoff_quiescent()? {
            return Err(unchanged(Cause::Busy));
        }
        if self
            .directory_read(|directory| directory.closed_pair())?
            .is_some()
        {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }

    pub(super) fn retention_request(&self, request: TargetRequest) -> Result<bool> {
        let state = self.entry_registry()?;
        Ok({
            state.directory.accepts_retention_payload(request)
                && state
                    .directory
                    .resolve(self.frames, 0)
                    .is_some_and(|allocator| {
                        request.identity.database == allocator.request.identity.database
                            && request.identity.page.file == allocator.request.identity.page.file
                    })
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
        // CREATE protects seven existing pages; recovered CLR additionally
        // protects both initialized pair pages before retiring them.
        self.retention_candidate_at(Some(request), start, required, 9, None)
    }

    /// Copy-only preflight of the two victims a later CREATE reservation would
    /// select, before the pair identities exist. A dirty victim is returned to
    /// the caller's original flusher; nothing is reserved or initialized here.
    pub(in crate::storage) fn retention_pair_room(
        &mut self,
        start: usize,
        required: &[TargetRequest; 7],
    ) -> Result<[StatusCandidate; 2]> {
        self.health()?;
        let first = self.retention_candidate_at(None, start, required, 7, None)?;
        let second =
            self.retention_candidate_at(None, first.next, required, 7, Some(first.index))?;
        Ok([first, second])
    }

    /// Copy-only selection for the actual two-page CREATE reservation. No
    /// directory generation is consumed until both selected clean slots close.
    pub(in crate::storage) fn retention_pair_candidates(
        &mut self,
        pair: &[TargetRequest; 2],
        start: usize,
        required: &[TargetRequest; 7],
    ) -> Result<[StatusCandidate; 2]> {
        self.health()?;
        if pair[0].same_address(&pair[1]) {
            return Err(unchanged(Cause::DuplicateTarget));
        }
        let first = self.retention_candidate_at(Some(pair[0]), start, required, 7, None)?;
        if first.kind == StatusCandidateKind::Resident {
            return Err(unchanged(Cause::AlreadyInitialized));
        }
        let second =
            self.retention_candidate_at(Some(pair[1]), first.next, required, 7, Some(first.index))?;
        if second.kind == StatusCandidateKind::Resident {
            return Err(unchanged(Cause::AlreadyInitialized));
        }
        Ok([first, second])
    }

    /// Without a request no address can be Resident; only victims are chosen.
    fn retention_candidate_at(
        &mut self,
        request: Option<TargetRequest>,
        start: usize,
        required: &[TargetRequest],
        required_limit: usize,
        excluded: Option<usize>,
    ) -> Result<StatusCandidate> {
        self.retention_quiescent()?;
        if start == 0 || required.len() > required_limit {
            return Err(unchanged(Cause::Shape));
        }
        if let Some(request) = request
            && !self.retention_request(request)?
        {
            return Err(unchanged(Cause::Shape));
        }
        let allocator = self.registration(0)?.request;
        for (index, target) in required.iter().enumerate() {
            if *target != allocator && !self.retention_request(*target)? {
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
            let registered = self.resolve_registration(index)?.map(|entry| entry.request);
            let frame = if let Some(actual) = registered {
                let frame = self.pair(index, &content)?.frame;
                if let Some(request) = request
                    && actual.same_address(&request)
                {
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
                let dpt = self.entry_registry()?;
                if !content.fresh()
                    || dpt.entries[index].is_some()
                    || dpt.requirements[index].is_some()
                {
                    return Err(self.fatal(Cause::Invariant));
                }
                None
            };
            let keep = excluded == Some(index)
                || registered.is_some_and(|actual| required.contains(&actual));
            outside |= !keep;
            if keep
                || self
                    .directory_read(|directory| directory.next_retention_generation(index))?
                    .is_none()
            {
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
            .directory_read(|directory| directory.next_retention_generation(index))?
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
        if before.is_none()
            && self.directory_read(|directory| directory.closed_index())? == Some(index)
        {
            return Err(unchanged(Cause::MissingTarget));
        }
        self.health()?;
        self.directory_edit(|directory| directory.prepare_retention_slot(index, generation))?;
        self.init_unpublished.store(true, Ordering::Release);
        content.page_lsn = PageLsn(0);
        content.modification_counter = 0;
        content.rec_lsn = None;
        content.dirty = false;
        Ok(before)
    }

    /// Close two clean slots together, protecting all seven already resident
    /// Forward targets. This reserves RAM generations only; a raw slot plan is
    /// never an allocator reservation or permission to initialize either page.
    pub(super) fn prepare_retention_pair_slots(
        &mut self,
        indices: [usize; 2],
        required: &[TargetRequest; 7],
    ) -> Result<[u64; 2]> {
        self.retention_quiescent()?;
        if self
            .directory_read(|directory| directory.closed_pair())?
            .is_some()
            || indices[0] == indices[1]
            || indices
                .iter()
                .any(|index| *index == 0 || *index >= self.frames.len())
        {
            return Err(unchanged(Cause::Shape));
        }
        for (at, request) in required.iter().enumerate() {
            if required[..at]
                .iter()
                .any(|prior| prior.same_address(request))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
            let (index, _) = self
                .directory_read(|directory| directory.lookup(self.frames, *request))?
                .ok_or_else(|| unchanged(Cause::MissingTarget))?;
            if indices.contains(&index) {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        let generations = [
            self.directory_read(|directory| directory.next_retention_generation(indices[0]))?
                .ok_or_else(|| unchanged(Cause::CounterExhausted))?,
            self.directory_read(|directory| directory.next_retention_generation(indices[1]))?
                .ok_or_else(|| unchanged(Cause::CounterExhausted))?,
        ];
        let mut first = self.frames[indices[0]]
            .content
            .try_write()
            .map_err(|error| self.load_lock_error(error))?;
        let mut second = self.frames[indices[1]]
            .content
            .try_write()
            .map_err(|error| self.load_lock_error(error))?;
        for (index, content) in [(indices[0], &*first), (indices[1], &*second)] {
            if self.status_payload(index, content)?.is_none()
                && self.directory_read(|directory| directory.closed_index())? == Some(index)
            {
                return Err(unchanged(Cause::MissingTarget));
            }
        }
        self.health()?;
        self.directory_edit(|directory| directory.prepare_retention_pair(indices, generations))?;
        self.init_unpublished.store(false, Ordering::Release);
        for content in [&mut *first, &mut *second] {
            content.page_lsn = PageLsn(0);
            content.modification_counter = 0;
            content.rec_lsn = None;
            content.dirty = false;
        }
        Ok(generations)
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
            .directory_read(|directory| directory.next_retention_generation(index))?
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
        let requirement = self.entry_registry()?.requirements[0].map(WalRequirement::location);
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
            || self
                .directory_read(|directory| directory.next_retention_generation(snapshot.index))?
                != Some(snapshot.generation)
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
                || self.entry_registry()?.requirements[0].map(WalRequirement::location)
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
        if !self.retention_request(request)?
            || self.directory_read(|directory| directory.address_present(request))?
        {
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
        let mut registry = self.entry_registry()?;
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = stamp.page_lsn;
        content.modification_counter = stamp.modification_counter;
        content.rec_lsn = None;
        content.dirty = false;
        registry.directory.replace_retention(
            snapshot.index,
            Registration {
                request,
                generation: stamp.generation,
            },
        );
        if registry.directory.closed_index() == Some(snapshot.index) {
            self.init_unpublished.store(false, Ordering::Release);
        }
        Ok(stamp)
    }
}
