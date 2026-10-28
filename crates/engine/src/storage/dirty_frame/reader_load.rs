//! One original-pool clean-victim claim. No source, WAL or semantic authority.
use super::*;
use crate::storage::wal_io::VerifiedLookupLoad;
use crate::transaction::concurrency::OwnerIdentity;

/// Copied correlation identity. Only an actual installed source can authorize
/// the image; neither this identity nor its getters can create that authority.
#[derive(Clone, Copy)]
pub(in crate::storage) struct LoadClaimIdentity<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    owner: OwnerIdentity,
    attempt: u64,
    index: usize,
    generation: u64,
    request: TargetRequest,
}
impl std::fmt::Debug for LoadClaimIdentity<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadClaimIdentity")
            .field("owner", &self.owner)
            .field("attempt", &self.attempt)
            .field("index", &self.index)
            .field("generation", &self.generation)
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}
impl LoadClaimIdentity<'_, '_> {
    pub(in crate::storage) fn owner(self) -> OwnerIdentity {
        self.owner
    }
    pub(in crate::storage) fn attempt(self) -> u64 {
        self.attempt
    }
    pub(in crate::storage) fn index(self) -> usize {
        self.index
    }
    pub(in crate::storage) fn generation(self) -> u64 {
        self.generation
    }
    pub(in crate::storage) fn request(self) -> TargetRequest {
        self.request
    }
    pub(in crate::storage) fn matches_origin(
        self,
        core: &PageConcurrency<'_>,
        binding: &AtomicU8,
    ) -> bool {
        std::ptr::eq(self.core, core) && std::ptr::eq(self.binding, binding)
    }
    pub(in crate::storage) fn same(self, other: Self) -> bool {
        self.matches_origin(other.core, other.binding)
            && self.owner == other.owner
            && self.attempt == other.attempt
            && self.index == other.index
            && self.generation == other.generation
            && self.request == other.request
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LoadClaimRecord {
    owner: OwnerIdentity,
    attempt: u64,
    index: usize,
    generation: u64,
    request: TargetRequest,
    previous: Option<FrameStamp>,
    previous_kind: Option<(PageKind, u8)>,
}
impl LoadClaimRecord {
    pub(super) fn index(&self) -> usize {
        self.index
    }
    pub(super) fn blocks(&self, request: TargetRequest, index: usize) -> bool {
        index == self.index || request.same_address(&self.request)
    }
    pub(super) fn wants(&self, request: TargetRequest) -> bool {
        request.same_address(&self.request)
    }
    pub(super) fn valid(
        &self,
        pool: &SharedPool<'_, '_, '_, '_>,
        state: &PoolRegistry<'_>,
    ) -> bool {
        self.attempt != 0
            && self.attempt == state.last_load_attempt
            && self.owner.generation() != 0
            && self.owner.index() < concurrency::OWNER_SLOTS
            && self.index != 0
            && self.index < pool.frames.len()
            && state.directory.retention_payloads()
            && state.directory.next_retention_generation(self.index) == Some(self.generation)
            && state.directory.accepts_retention_payload(self.request)
            && state
                .directory
                .resolve(pool.frames, 0)
                .is_some_and(|allocator| {
                    self.request.identity.database == allocator.request.identity.database
                        && self.request.identity.page.file == allocator.request.identity.page.file
                })
            && !state.directory.address_present(self.request)
            && state.counts[self.index] == 1
            && state.entries[self.index].is_none()
            && state.requirements[self.index].is_none()
            && state
                .claim
                .as_ref()
                .is_none_or(|claim| claim.index() != self.index)
            && state
                .init_claim
                .as_ref()
                .is_none_or(|claim| !claim.blocks_load(self.index, self.request))
            && match (
                state.directory.resolve(pool.frames, self.index),
                self.previous,
            ) {
                (None, None) => self.previous_kind.is_none(),
                (Some(registration), Some(previous)) => {
                    self.previous_kind
                        == Some((registration.request.kind, registration.request.level))
                        && registration.request.identity == previous.identity
                        && registration.generation == previous.generation
                        && previous.rec_lsn.is_none()
                }
                _ => false,
            }
    }
    fn matches(&self, identity: LoadClaimIdentity<'_, '_>) -> bool {
        self.owner == identity.owner
            && self.attempt == identity.attempt
            && self.index == identity.index
            && self.generation == identity.generation
            && self.request == identity.request
    }
}

pub(in crate::storage) enum LoadClaimStep<'a, 'b, 'c, 's, 'p, 'd> {
    Resident(StatusCandidate),
    FlushRequired(StatusCandidate),
    Claimed(RetentionLoadClaim<'a, 'b, 'c, 's, 'p, 'd>),
}
/// Owner remains borrowed for the caller's actual operation. Every copy/install
/// phase reborrows it briefly; no content admission or leaf guard survives I/O.
pub(in crate::storage) struct RetentionLoadClaim<'a, 'b, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    owner: &'a mut Owner<'c, 's>,
    identity: LoadClaimIdentity<'c, 's>,
    active: bool,
}
impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn claim_retention_load<'a, 'b>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        request: TargetRequest,
        start: usize,
        protected: &[TargetRequest],
    ) -> Result<LoadClaimStep<'a, 'b, 'c, 's, 'p, 'd>> {
        self.check_retention_reader(owner)?;
        let owner_id = owner
            .visibility_identity(self.core)
            .map_err(DirtyError::from)?;
        if start == 0 || protected.len() > 9 || !self.retention_request(request)? {
            return Err(unchanged(Cause::Shape));
        }
        let allocator = self.target_request(0)?;
        for (at, value) in protected.iter().enumerate() {
            if *value != allocator && !self.retention_request(*value)? {
                return Err(unchanged(Cause::Shape));
            }
            if protected[..at]
                .iter()
                .any(|prior| prior.same_address(value))
            {
                return Err(unchanged(Cause::DuplicateTarget));
            }
        }
        let admission = owner.try_content().map_err(DirtyError::from)?;
        let selected = self.select_load(owner_id, request, start, protected);
        let released = admission.release_checked().map_err(DirtyError::from);
        let selected = match (selected, released) {
            (Err(error), _) if error.effect != Effect::Unchanged => return Err(error),
            (_, Err(error)) => return Err(error),
            (result, Ok(())) => result?,
        };
        match selected {
            Selection::Resident(candidate) => Ok(LoadClaimStep::Resident(candidate)),
            Selection::Flush(candidate) => Ok(LoadClaimStep::FlushRequired(candidate)),
            Selection::Claim(record) => Ok(LoadClaimStep::Claimed(RetentionLoadClaim {
                pool: self,
                owner,
                identity: LoadClaimIdentity {
                    core: self.core,
                    binding: self.binding,
                    owner: record.owner,
                    attempt: record.attempt,
                    index: record.index,
                    generation: record.generation,
                    request: record.request,
                },
                active: true,
            })),
        }
    }
    fn load_eligible(
        &self,
        state: &PoolRegistry<'s>,
        index: usize,
        request: TargetRequest,
    ) -> bool {
        state.counts[index] == 0
            && state.claim.is_none_or(|claim| {
                claim.index() != index && !claim.request().same_address(&request)
            })
            && state
                .init_claim
                .is_none_or(|claim| !claim.blocks_load(index, request))
            && (state.directory.closed_index() != Some(index)
                || state.directory.published().is_some())
            && state
                .directory
                .closed_pair()
                .is_none_or(|pair| !pair.contains(&index))
    }
    fn select_load(
        &self,
        owner: OwnerIdentity,
        request: TargetRequest,
        start: usize,
        protected: &[TargetRequest],
    ) -> Result<Selection> {
        let count = self.frames.len() - 1;
        let start = (start - 1) % count;
        let mut clean = None;
        let mut dirty = None;
        let mut blocked = false;
        let mut exhausted = false;
        let mut resident = None;
        {
            let state = self.entry_registry()?;
            if state
                .init_claim
                .as_ref()
                .is_some_and(init::InitClaimSlot::blocks_ordinary)
            {
                return Err(unchanged(Cause::Busy));
            }
            for index in 1..self.frames.len() {
                if let Some(registration) = state.directory.resolve(self.frames, index)
                    && registration.request.same_address(&request)
                {
                    if registration.request != request {
                        return Err(unchanged(Cause::Stale));
                    }
                    if state
                        .load
                        .as_ref()
                        .is_some_and(|load| load.blocks(request, index))
                    {
                        return Err(unchanged(Cause::Busy));
                    }
                    resident = Some(index);
                    break;
                }
            }
            if resident.is_none() && state.load.is_some() {
                return Err(unchanged(Cause::Busy));
            }
            if resident.is_none()
                && state
                    .init_claim
                    .is_some_and(|claim| claim.blocks_load(MAX_FRAMES, request))
            {
                return Err(unchanged(Cause::Busy));
            }
        }
        if let Some(index) = resident {
            #[cfg(test)]
            init::test_support::hit(self, init::test_support::InitPoint::LoadResidentObserved);
            let content = self.frames[index]
                .content
                .try_read()
                .map_err(|error| self.load_lock_error(error))?;
            let state = self.entry_registry()?;
            if state
                .init_claim
                .as_ref()
                .is_some_and(init::InitClaimSlot::blocks_ordinary)
            {
                return Err(unchanged(Cause::Busy));
            }
            if state
                .load
                .as_ref()
                .is_some_and(|load| load.blocks(request, index))
            {
                return Err(unchanged(Cause::Busy));
            }
            if state
                .directory
                .resolve(self.frames, index)
                .is_none_or(|entry| entry.request != request)
            {
                return Err(unchanged(Cause::Stale));
            }
            let frame = self.pair_in(&state, index, &content)?.frame;
            return Ok(Selection::Resident(StatusCandidate {
                kind: StatusCandidateKind::Resident,
                index,
                frame: Some(frame),
                next: 1 + index % count,
            }));
        }
        // Each candidate is inspected under content->leaf. No unrelated pin or
        // inaccessible content forces a global quiescence requirement.
        for offset in 0..count {
            let index = 1 + (start + offset) % count;
            let content = match self.frames[index].content.try_read() {
                Ok(value) => value,
                Err(TryLockError::WouldBlock) => {
                    blocked = true;
                    continue;
                }
                Err(TryLockError::Poisoned(_)) => return Err(self.fatal(Cause::Poisoned)),
            };
            let state = self.entry_registry()?;
            if state
                .init_claim
                .as_ref()
                .is_some_and(init::InitClaimSlot::blocks_ordinary)
            {
                return Err(unchanged(Cause::Busy));
            }
            if state.load.is_some() {
                return Err(unchanged(Cause::Busy));
            }
            let registration = state.directory.resolve(self.frames, index);
            if registration.is_some_and(|value| {
                protected
                    .iter()
                    .any(|kept| value.request.same_address(kept))
            }) {
                continue;
            }
            if !self.load_eligible(&state, index, request) {
                blocked = true;
                continue;
            }
            let frame = self.load_frame_in(&state, index, &content)?;
            if state.directory.next_retention_generation(index).is_none() {
                exhausted = true;
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
                if dirty.is_none() {
                    dirty = Some(candidate);
                }
            } else if clean
                .is_none_or(|old: StatusCandidate| old.frame.is_some() && frame.is_none())
            {
                clean = Some(candidate);
            }
        }
        let Some(candidate) = clean else {
            #[cfg(test)]
            if dirty.is_some() {
                init::test_support::hit(self, init::test_support::InitPoint::LoadFlushSelected);
            }
            // The scan's content/registry guards have ended. A dirty candidate
            // is only a hint, but its return must still arbitrate with a pair
            // close that won after the final scan leaf.
            let state = self.entry_registry()?;
            if state
                .init_claim
                .as_ref()
                .is_some_and(init::InitClaimSlot::blocks_ordinary)
            {
                return Err(unchanged(Cause::Busy));
            }
            return dirty.map(Selection::Flush).ok_or_else(|| {
                unchanged(if exhausted && !blocked {
                    Cause::CounterExhausted
                } else {
                    Cause::Busy
                })
            });
        };
        #[cfg(test)]
        init::test_support::hit(self, init::test_support::InitPoint::LoadCleanSelected);
        let content = self.frames[candidate.index]
            .content
            .try_read()
            .map_err(|e| self.load_lock_error(e))?;
        let mut state = self.entry_registry()?;
        if state
            .init_claim
            .as_ref()
            .is_some_and(init::InitClaimSlot::blocks_ordinary)
        {
            return Err(unchanged(Cause::Busy));
        }
        if state.load.is_some()
            || !self.load_eligible(&state, candidate.index, request)
            || state.directory.address_present(request)
        {
            return Err(unchanged(Cause::Busy));
        }
        let previous = self.load_frame_in(&state, candidate.index, &content)?;
        if previous != candidate.frame || content.dirty {
            return Err(unchanged(Cause::Stale));
        }
        if state.total == MAX_PINS {
            return Err(unchanged(Cause::Capacity));
        }
        let generation = state
            .directory
            .next_retention_generation(candidate.index)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        let attempt = state
            .last_load_attempt
            .checked_add(1)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))?;
        let record = LoadClaimRecord {
            owner,
            attempt,
            index: candidate.index,
            generation,
            request,
            previous,
            previous_kind: state
                .directory
                .resolve(self.frames, candidate.index)
                .map(|registration| (registration.request.kind, registration.request.level)),
        };
        state.last_load_attempt = attempt;
        state.load = Some(record);
        state.counts[candidate.index] = 1;
        state.total += 1;
        Ok(Selection::Claim(record))
    }
    fn load_frame_in(
        &self,
        state: &PoolRegistry<'s>,
        index: usize,
        content: &FrameContent<'_>,
    ) -> Result<Option<FrameStamp>> {
        if state.directory.resolve(self.frames, index).is_some() {
            Ok(Some(self.pair_in(state, index, content)?.frame))
        } else if content.fresh()
            && state.entries[index].is_none()
            && state.requirements[index].is_none()
        {
            Ok(None)
        } else {
            Err(self.fatal(Cause::Invariant))
        }
    }
    /// Last action of the private resolver, independently from replay checkout
    /// audit. A forgotten inner keeps its exact marker and quota and fences.
    pub(in crate::storage) fn audit_load_released(
        &self,
        owner: OwnerIdentity,
        expected_attempt: Option<u64>,
    ) -> Result<()> {
        let state = self
            .registry
            .lock()
            .map_err(|_| self.fatal(Cause::Poisoned))?;
        if !self.registry_valid(&state)
            || state.load.is_some_and(|claim| claim.owner == owner)
            || expected_attempt
                .is_some_and(|attempt| attempt == 0 || attempt > state.last_load_attempt)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        self.health()
    }
}
enum Selection {
    Resident(StatusCandidate),
    Flush(StatusCandidate),
    Claim(LoadClaimRecord),
}

impl<'c, 's> RetentionLoadClaim<'_, '_, 'c, 's, '_, '_> {
    pub(in crate::storage) fn identity(&self) -> LoadClaimIdentity<'c, 's> {
        self.identity
    }
    fn record<'r>(&self, state: &'r PoolRegistry<'s>) -> Result<&'r LoadClaimRecord> {
        if !self.active
            || !self
                .identity
                .matches_origin(self.pool.core, self.pool.binding)
        {
            return Err(self.pool.fatal(Cause::Invariant));
        }
        state
            .load
            .as_ref()
            .filter(|value| value.matches(self.identity))
            .ok_or_else(|| self.pool.fatal(Cause::Invariant))
    }
    pub(in crate::storage) fn copy_allocator(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<FrameStamp> {
        self.pool.health()?;
        {
            let state = self.pool.entry_registry()?;
            self.record(&state)?;
        }
        // Short original pin/content authority, no owner escapes this method.
        let request = self.pool.target_request(0)?;
        let read = self
            .pool
            .reserve_targets(self.owner, &[request])?
            .try_read()?;
        let copied = read.copy_snapshot(0, output);
        let released = read.release_checked();
        match (copied, released) {
            (Err(error), _) if error.effect != Effect::Unchanged => Err(error),
            (_, Err(error)) => Err(error),
            (result, Ok(())) => result,
        }
    }
    pub(in crate::storage) fn capture_victim(&mut self) -> Result<Option<FrameStamp>> {
        let pool = self.pool;
        let identity = self.identity;
        let active = self.active;
        let admission = self.owner.try_content().map_err(DirtyError::from)?;
        let observed = (|| {
            let content = pool.frames[identity.index]
                .content
                .try_read()
                .map_err(|e| pool.load_lock_error(e))?;
            let state = pool.entry_registry()?;
            let record = state
                .load
                .as_ref()
                .filter(|record| active && record.matches(identity))
                .ok_or_else(|| pool.fatal(Cause::Invariant))?;
            let actual = pool.load_frame_in(&state, record.index, &content)?;
            if actual != record.previous || content.dirty {
                return Err(pool.fatal(Cause::Invariant));
            }
            Ok(actual)
        })();
        let released = admission.release_checked().map_err(DirtyError::from);
        match (observed, released) {
            (Err(error), _) if error.effect != Effect::Unchanged => Err(error),
            (_, Err(error)) => Err(error),
            (result, Ok(())) => result,
        }
    }
    pub(in crate::storage) fn install(
        &mut self,
        permit: &VerifiedLookupLoad<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.pool.health()?;
        if !self.identity.same(permit.identity()) || permit.request() != self.identity.request {
            return Err(self.pool.fatal(Cause::Invariant));
        }
        let pool = self.pool;
        let identity = self.identity;
        let active = self.active;
        let admission = self.owner.try_content().map_err(DirtyError::from)?;
        let installed = (|| {
            // Allocator has the lowest physical rank. Both guards precede leaf.
            let allocator = pool.frames[0]
                .content
                .try_read()
                .map_err(|e| pool.load_lock_error(e))?;
            let mut content = pool.frames[identity.index]
                .content
                .try_write()
                .map_err(|e| pool.load_lock_error(e))?;
            let mut state = pool.entry_registry()?;
            let record = state
                .load
                .as_ref()
                .filter(|value| value.matches(identity))
                .ok_or_else(|| pool.fatal(Cause::Invariant))?;
            if !active
                || !record.valid(pool, &state)
                || pool.load_frame_in(&state, identity.index, &content)? != record.previous
                || content.dirty
            {
                return Err(pool.fatal(Cause::Invariant));
            }
            pool.pair_in(&state, 0, &allocator)?;
            if !permit.accepts_current_allocator(allocator.bytes) {
                return Err(pool.fatal(Cause::Invariant));
            }
            let page = super::super::page::decode_page(
                permit.bytes(),
                identity.request.identity,
                identity.request.kind,
            )
            .map_err(|_| pool.fatal(Cause::Invariant))?;
            if page.header().level != identity.request.level {
                return Err(pool.fatal(Cause::Invariant));
            }
            let stamp = FrameStamp {
                identity: identity.request.identity,
                generation: identity.generation,
                page_lsn: page.header().lsn,
                modification_counter: u64::from(page.header().lsn.0 != 0),
                rec_lsn: None,
            };
            // The unwind guard fences before the mutex is poisoned. There are
            // no fallible steps after the first byte, and no rollback-on-Drop.
            let mutation = LoadMutation(pool);
            content.bytes.copy_from_slice(permit.bytes());
            content.page_lsn = stamp.page_lsn;
            content.modification_counter = stamp.modification_counter;
            content.rec_lsn = None;
            content.dirty = false;
            state.directory.replace_retention(
                identity.index,
                Registration {
                    request: identity.request,
                    generation: identity.generation,
                },
            );
            state.load = None;
            state.counts[identity.index] = 0;
            state.total -= 1;
            drop(mutation);
            Ok(stamp)
        })();
        // Guard scopes above end before the original content admission returns.
        let released = admission.release_checked().map_err(DirtyError::from);
        if installed.is_ok() {
            self.active = false;
        }
        match (installed, released) {
            (Err(error), _) if error.effect != Effect::Unchanged => Err(error),
            (_, Err(error)) => Err(error),
            (result, Ok(())) => result,
        }
    }
    pub(in crate::storage) fn cancel(mut self) -> Result<()> {
        let pool = self.pool;
        let mut state = pool.entry_registry()?;
        self.record(&state)?;
        state.load = None;
        state.counts[self.identity.index] = 0;
        state.total -= 1;
        self.active = false;
        Ok(())
    }
}
impl Drop for RetentionLoadClaim<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.pool.fence.fence();
        }
    }
}
struct LoadMutation<'a, 'c, 's, 'p, 'd>(&'a SharedPool<'c, 's, 'p, 'd>);
impl Drop for LoadMutation<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        self.0.fence_unwind();
    }
}

/// Layout-only predecessor used to expose the exact retained migration delta.
/// It is never instantiated and never introduces another pool or registry.
#[allow(dead_code)]
struct PreLoadPool<'c, 's, 'p, 'd> {
    frames: &'p [DirtyFrame<'d>],
    directory: registration::PreLoadDirectory,
    pins: Mutex<PinAccounting>,
    dpt: Mutex<DirtyRegistry<'s>>,
    core: &'c PageConcurrency<'s>,
    fence: &'s SessionFence,
    binding: &'s AtomicU8,
    stamp_anchor: u8,
    #[cfg(test)]
    fence_pause: Mutex<Option<test_support::FencePause>>,
    #[cfg(test)]
    release_pause: Mutex<Option<test_support::FencePause>>,
    #[cfg(test)]
    flush_pause: Mutex<Option<(test_support::FlushPoint, test_support::FencePause)>>,
    #[cfg(test)]
    init_pause: Mutex<Option<(init::test_support::InitPoint, test_support::FencePause)>>,
}
pub(in crate::storage) struct LoadSourceCensus {
    pub(in crate::storage) pool: usize,
    pub(in crate::storage) previous_pool: usize,
    pub(in crate::storage) registry: usize,
    pub(in crate::storage) previous_registry_parts: usize,
    pub(in crate::storage) claim_record: usize,
    pub(in crate::storage) identity: usize,
    pub(in crate::storage) token: usize,
    pub(in crate::storage) step_result: usize,
    pub(in crate::storage) selection_nested: usize,
    pub(in crate::storage) install_nested: usize,
    pub(in crate::storage) copy_nested: usize,
    pub(in crate::storage) cancellation_nested: usize,
}
/// Source values, independent of optimizer lifetimes. Caller includes the
/// actual outer/read/replay/source-cut carriers and original loan backing once.
/// Nested figures exclude the retained pool and caller's active claim token.
pub(in crate::storage) fn source_census() -> LoadSourceCensus {
    use std::mem::size_of;
    let leaf = init::ORDINARY_PAIR_GATE_BYTES
        + 2 * size_of::<Result<MutexGuard<'_, PoolRegistry<'_>>>>()
        + size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<Result<Registration>>()
        + 3 * size_of::<Option<Registration>>()
        + 2 * size_of::<Result<DirtyObservation>>()
        + 256; // bounded validation indices, references, bools, fixed iterators
    let content = size_of::<ContentAdmission<'_, '_, '_>>()
        + size_of::<std::result::Result<ContentAdmission<'_, '_, '_>, concurrency::ConcurrencyError>>(
        );
    LoadSourceCensus {
        pool: size_of::<SharedPool<'_, '_, '_, '_>>(),
        previous_pool: size_of::<PreLoadPool<'_, '_, '_, '_>>(),
        registry: size_of::<Mutex<PoolRegistry<'_>>>(),
        previous_registry_parts: size_of::<registration::PreLoadDirectory>()
            + size_of::<Mutex<PinAccounting>>()
            + size_of::<Mutex<DirtyRegistry<'_>>>(),
        claim_record: size_of::<LoadClaimRecord>(),
        identity: size_of::<LoadClaimIdentity<'_, '_>>(),
        token: size_of::<RetentionLoadClaim<'_, '_, '_, '_, '_, '_>>(),
        step_result: size_of::<Result<LoadClaimStep<'_, '_, '_, '_, '_, '_>>>(),
        // The final no-clean return gate is sequential with candidate scans
        // and final claim installation; leaf includes its guard and Results.
        selection_nested: leaf
            + content
            + 2 * size_of::<Result<Selection>>()
            + size_of::<Selection>()
            + 2 * size_of::<LoadClaimRecord>()
            + 3 * size_of::<Option<StatusCandidate>>()
            + size_of::<StatusCandidate>()
            + 3 * size_of::<Result<Option<FrameStamp>>>()
            + size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
            + size_of::<LoadClaimStep<'_, '_, '_, '_, '_, '_>>()
            + size_of::<Result<LoadClaimStep<'_, '_, '_, '_, '_, '_>>>(),
        install_nested: leaf
            + content
            + size_of::<LoadClaimIdentity<'_, '_>>()
            + 3 * size_of::<Result<FrameStamp>>()
            + size_of::<FrameStamp>()
            + size_of::<RwLockReadGuard<'_, FrameContent<'_>>>()
            + size_of::<RwLockWriteGuard<'_, FrameContent<'_>>>()
            + size_of::<crate::storage::page::PageView<'_>>()
            + size_of::<
                std::result::Result<
                    crate::storage::page::PageView<'_>,
                    crate::storage::page::PageError,
                >,
            >()
            + size_of::<LoadMutation<'_, '_, '_, '_, '_>>(),
        copy_nested: leaf
            + target_set_call_bytes::<2>()
            + 3 * size_of::<Result<FrameStamp>>()
            + size_of::<TargetRequest>(),
        cancellation_nested: leaf
            + size_of::<Result<&LoadClaimRecord>>()
            + size_of::<Option<u64>>()
            + size_of::<OwnerIdentity>(),
    }
}
