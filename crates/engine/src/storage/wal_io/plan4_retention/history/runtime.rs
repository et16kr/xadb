//! Shared access to the original replay loan and installed source receipt.
//! Mutex guards end before any WAL/File read. Neither copied identity nor a
//! checkout grants page, source-selection or INIT authority.
use super::*;
use crate::transaction::concurrency::{OwnerIdentity, PageConcurrency};
use std::sync::{Condvar, Mutex, MutexGuard};

#[derive(Clone, Copy, PartialEq, Eq)]
struct ReplayKey {
    owner: OwnerIdentity,
    attempt: u64,
}
struct ReplayState<'a> {
    available: Option<&'a mut [u8; RETENTION_REPLAY_BYTES]>,
    active: Option<ReplayKey>,
    last_attempt: u64,
}
#[derive(Clone, Copy)]
struct InstalledSource<'s> {
    source: HistorySource<'s>,
    seed: WalPrefix,
    requirement: Option<WalRequirement<'s>>,
}
impl InstalledSource<'_> {
    fn lsn(self) -> u64 {
        self.requirement
            .map_or(self.seed.complete_lsn, |r| r.location().lsn)
    }
    fn end(self) -> u64 {
        self.requirement
            .map_or(self.seed.complete_offset, |r| r.location().end)
    }
}

pub(in super::super) struct RuntimeHistory<'a, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    completion: &'s Condvar,
    installed: Mutex<InstalledSource<'s>>,
    replay: Mutex<ReplayState<'a>>,
}
impl<'a, 'c, 's> RuntimeHistory<'a, 'c, 's> {
    /// Called only by the actual READY/recovered conversion after its original
    /// source and seed-prefix checks. No raw caller-facing constructor exists.
    pub(in super::super) fn new<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, '_, W>,
        core: &'c PageConcurrency<'s>,
        source: HistorySource<'s>,
        seed: WalPrefix,
        replay: &'a mut [u8; RETENTION_REPLAY_BYTES],
    ) -> Self {
        Self {
            core,
            binding: &storage.binding,
            fence: storage.session_fence(),
            completion: &storage.wal().completion,
            installed: Mutex::new(InstalledSource {
                source,
                seed,
                requirement: None,
            }),
            replay: Mutex::new(ReplayState {
                available: Some(replay),
                active: None,
                last_attempt: 0,
            }),
        }
    }
    fn fail(&self, cause: RetentionCause) -> RetentionError {
        self.fence.fence();
        self.completion.notify_all();
        RetentionError {
            phase: RetentionPhase::Source,
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
    fn healthy(&self) -> Result<()> {
        if self.replay.is_poisoned() || self.installed.is_poisoned() {
            return Err(self.fail(RetentionCause::Poisoned));
        }
        if self.fence.is_fenced() {
            return Err(self.fail(RetentionCause::Fenced));
        }
        Ok(())
    }
    fn replay_state(&self) -> Result<MutexGuard<'_, ReplayState<'a>>> {
        self.healthy()?;
        let state = self.replay.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => unchanged(RetentionPhase::Source, RetentionCause::Busy),
            TryLockError::Poisoned(_) => self.fail(RetentionCause::Poisoned),
        })?;
        self.healthy()?;
        if state.available.is_some() != state.active.is_none()
            || state
                .active
                .is_some_and(|key| key.attempt == 0 || key.attempt != state.last_attempt)
        {
            return Err(self.fail(RetentionCause::Invariant));
        }
        Ok(state)
    }
    fn source_state(&self) -> Result<MutexGuard<'_, InstalledSource<'s>>> {
        self.healthy()?;
        let state = self.installed.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => unchanged(RetentionPhase::Source, RetentionCause::Busy),
            TryLockError::Poisoned(_) => self.fail(RetentionCause::Poisoned),
        })?;
        self.healthy()?;
        Ok(state)
    }
    /// Only actual installer completion calls this with its retained authentic
    /// requirement. A stage or a copied current WAL prefix is insufficient.
    pub(in super::super) fn installed(&self, requirement: WalRequirement<'s>) -> Result<()> {
        self.healthy()?;
        let mut current = self
            .installed
            .lock()
            .map_err(|_| self.fail(RetentionCause::Poisoned))?;
        self.healthy()?;
        let next = requirement.location();
        if !requirement.uses_fence(self.fence)
            || next.stream != current.seed.stream
            || next.file != current.seed.file
            || next.previous_lsn != current.lsn()
            || next.start != current.end()
            || next.lsn <= next.previous_lsn
            || next.end <= next.start
        {
            return Err(self.fail(RetentionCause::Invariant));
        }
        current.requirement = Some(requirement);
        Ok(())
    }
    pub(in super::super) fn source_cut(&self) -> Result<LookupSourceCut<'_, 'a, 'c, 's>> {
        let current = *self.source_state()?;
        Ok(LookupSourceCut {
            runtime: self,
            current,
        })
    }
    pub(in super::super) fn replay_scope<'r>(
        &'r self,
        owner: &Owner<'c, 's>,
    ) -> Result<ReplayScope<'r, 'a, 'c, 's>> {
        self.healthy()?;
        let identity = owner
            .visibility_identity(self.core)
            .map_err(|error| RetentionError {
                phase: RetentionPhase::Source,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })?;
        Ok(ReplayScope {
            runtime: self,
            owner: identity,
            attempt: None,
            completed: false,
        })
    }
    /// Exclusive original owner paths cannot coexist with a split Reader.
    /// A forgotten checkout is still detected rather than silently recreated.
    pub(in super::super) fn exclusive(&mut self) -> Result<&mut [u8; RETENTION_REPLAY_BYTES]> {
        self.healthy()?;
        let state = match self.replay.get_mut() {
            Ok(state) => state,
            Err(_) => {
                self.fence.fence();
                self.completion.notify_all();
                return Err(RetentionError {
                    phase: RetentionPhase::Source,
                    cause: RetentionCause::Poisoned,
                    effect: Effect::CallerMustResolve,
                });
            }
        };
        if state.active.is_some() || state.available.is_none() {
            self.fence.fence();
            self.completion.notify_all();
            return Err(RetentionError {
                phase: RetentionPhase::Source,
                cause: RetentionCause::Invariant,
                effect: Effect::CallerMustResolve,
            });
        }
        Ok(state
            .available
            .as_deref_mut()
            .expect("checked original loan"))
    }
    pub(in super::super) fn quiescent(&self) -> Result<()> {
        let state = self.replay_state()?;
        if state.active.is_some() {
            return Err(unchanged(RetentionPhase::Source, RetentionCause::Busy));
        }
        Ok(())
    }
}

impl Drop for RuntimeHistory<'_, '_, '_> {
    fn drop(&mut self) {
        let valid = self
            .replay
            .get_mut()
            .is_ok_and(|state| state.active.is_none() && state.available.is_some());
        if !valid || self.installed.is_poisoned() {
            self.fence.fence();
            self.completion.notify_all();
        }
    }
}

/// Observation of the original last installed interval, never a raw-LSN grant.
pub(in super::super) struct LookupSourceCut<'r, 'a, 'c, 's> {
    runtime: &'r RuntimeHistory<'a, 'c, 's>,
    current: InstalledSource<'s>,
}
impl<'s> LookupSourceCut<'_, '_, '_, 's> {
    pub(in super::super) fn source(&self) -> HistorySource<'s> {
        self.current.source
    }
    pub(in super::super) fn seed(&self) -> WalPrefix {
        self.current.seed
    }
    pub(in super::super) fn requirement(&self) -> Option<WalRequirement<'s>> {
        self.current.requirement
    }
    pub(in super::super) fn check_origin<W: WriterNamespace>(
        &self,
        storage: &MutableStorageOwner<'_, '_, W>,
    ) -> Result<()> {
        self.runtime.healthy()?;
        if !std::ptr::eq(self.runtime.binding, &storage.binding)
            || !std::ptr::eq(self.runtime.fence, storage.session_fence())
        {
            return Err(self.runtime.fail(RetentionCause::BindingMismatch));
        }
        storage.health().map_err(|error| {
            fatal(
                storage,
                RetentionPhase::Source,
                RetentionCause::Storage(error),
            )
        })
    }
    pub(in super::super) fn ensure_durable<W: WriterNamespace>(
        &self,
        storage: &'s MutableStorageOwner<'_, '_, W>,
    ) -> Result<Option<DurableWal<'s>>> {
        self.check_origin(storage)?;
        self.current
            .requirement
            .map(|requirement| {
                match storage
                    .wal()
                    .ensure_durable(&requirement)
                    .map_err(|error| {
                        fatal(storage, RetentionPhase::Source, RetentionCause::Wal(error))
                    })? {
                    wal::EnsureDurable::AlreadySynced(durable)
                    | wal::EnsureDurable::SyncedHere(durable) => Ok(durable),
                    wal::EnsureDurable::AwaitWalOwner => {
                        Err(unchanged(RetentionPhase::Source, RetentionCause::Busy))
                    }
                }
            })
            .transpose()
    }
    /// Normal progress of another installer is a retry, not corruption. The
    /// loader separately proves its exact target stayed protected by the claim.
    pub(in super::super) fn unchanged(&self) -> Result<()> {
        let current = self.runtime.source_state()?;
        if current.requirement.map(|r| r.location())
            != self.current.requirement.map(|r| r.location())
        {
            return Err(unchanged(RetentionPhase::Source, RetentionCause::Busy));
        }
        Ok(())
    }
}

/// The actual resolver retains this audit outside the inner checkout token.
/// Its attempt is independent of the pool load claim's attempt counter.
pub(in super::super) struct ReplayScope<'r, 'a, 'c, 's> {
    runtime: &'r RuntimeHistory<'a, 'c, 's>,
    owner: OwnerIdentity,
    attempt: Option<u64>,
    completed: bool,
}
impl<'r, 'a, 'c, 's> ReplayScope<'r, 'a, 'c, 's> {
    pub(in super::super) fn checkout(&mut self) -> Result<ReplayLoan<'r, 'a, 'c, 's>> {
        let mut state = self.runtime.replay_state()?;
        if let Some(active) = state.active {
            if active.owner == self.owner {
                return Err(self.runtime.fail(RetentionCause::Invariant));
            }
            return Err(unchanged(RetentionPhase::Source, RetentionCause::Busy));
        }
        let attempt = state
            .last_attempt
            .checked_add(1)
            .ok_or_else(|| unchanged(RetentionPhase::Source, RetentionCause::Exhausted))?;
        let key = ReplayKey {
            owner: self.owner,
            attempt,
        };
        let bytes = state.available.take().expect("available iff no active key");
        state.active = Some(key);
        state.last_attempt = attempt;
        self.attempt = Some(attempt);
        drop(state);
        Ok(ReplayLoan {
            runtime: self.runtime,
            key,
            bytes: Some(bytes),
        })
    }
    fn audit(&self) -> Result<()> {
        let state = self
            .runtime
            .replay
            .lock()
            .map_err(|_| self.runtime.fail(RetentionCause::Poisoned))?;
        if state.available.is_some() != state.active.is_none()
            || state.active.is_some_and(|key| {
                key.owner == self.owner || key.attempt == 0 || key.attempt != state.last_attempt
            })
        {
            return Err(self.runtime.fail(RetentionCause::Invariant));
        }
        self.runtime.healthy()
    }
    pub(in super::super) fn attempt(&self) -> Option<u64> {
        self.attempt
    }
    pub(in super::super) fn finish_checked(mut self) -> Result<()> {
        let result = self.audit();
        self.completed = true;
        result
    }
}
impl Drop for ReplayScope<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.completed && self.audit().is_err() {
            self.runtime.fence.fence();
            self.runtime.completion.notify_all();
        }
    }
}

pub(in super::super) struct ReplayLoan<'r, 'a, 'c, 's> {
    runtime: &'r RuntimeHistory<'a, 'c, 's>,
    key: ReplayKey,
    bytes: Option<&'a mut [u8; RETENTION_REPLAY_BYTES]>,
}
impl ReplayLoan<'_, '_, '_, '_> {
    pub(in super::super) fn bytes(&self) -> &[u8; RETENTION_REPLAY_BYTES] {
        self.bytes.as_deref().expect("live checkout")
    }
    pub(in super::super) fn bytes_mut(&mut self) -> &mut [u8; RETENTION_REPLAY_BYTES] {
        self.bytes.as_deref_mut().expect("live checkout")
    }
    fn return_inner(&mut self) -> Result<()> {
        let mut state = match self.runtime.replay.lock() {
            Ok(state) => state,
            Err(error) => {
                self.runtime.fence.fence();
                self.runtime.completion.notify_all();
                error.into_inner()
            }
        };
        if state.active != Some(self.key)
            || self.key.attempt != state.last_attempt
            || state.available.is_some()
            || self.bytes.is_none()
        {
            return Err(self.runtime.fail(RetentionCause::Invariant));
        }
        state.available = self.bytes.take();
        state.active = None;
        if self.runtime.fence.is_fenced() {
            return Err(self.runtime.fail(RetentionCause::Fenced));
        }
        Ok(())
    }
    pub(in super::super) fn return_checked(mut self) -> Result<()> {
        self.return_inner()
    }
}
impl Drop for ReplayLoan<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.bytes.is_some() {
            self.runtime.fence.fence();
            self.runtime.completion.notify_all();
            let _ = self.return_inner();
        }
    }
}

pub(in super::super) fn source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<RuntimeHistory<'_, '_, '_>>()
        + 2 * size_of::<InstalledSource<'_>>()
        + 2 * size_of::<ReplayState<'_>>()
        + 2 * size_of::<LookupSourceCut<'_, '_, '_, '_>>()
        + 2 * size_of::<ReplayScope<'_, '_, '_, '_>>()
        + 2 * size_of::<ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<ReplayLoan<'_, '_, '_, '_>>>()
        + 2 * size_of::<MutexGuard<'_, ReplayState<'_>>>()
        + 2 * size_of::<MutexGuard<'_, InstalledSource<'_>>>()
        + 4 * size_of::<Result<()>>()
        + 16 * size_of::<u64>()
}

/// Steady-state method locals only. The caller separately retains the original
/// RuntimeHistory, its cut/scope/loan, and any checked-result alternatives.
/// These methods never construct another RuntimeHistory or copy ReplayState.
/// `durability` excludes the actual WAL call, which its caller must add.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct LookupSourceCensus {
    pub(in super::super) source: usize,
    pub(in super::super) checkout: usize,
    pub(in super::super) returning: usize,
    pub(in super::super) durability: usize,
    pub(in super::super) peak: usize,
}

pub(in super::super) fn lookup_source_census() -> LookupSourceCensus {
    use std::mem::size_of;
    type Cut<'a> = LookupSourceCut<'a, 'a, 'a, 'a>;
    type Scope<'a> = ReplayScope<'a, 'a, 'a, 'a>;
    type Loan<'a> = ReplayLoan<'a, 'a, 'a, 'a>;
    type SourceGuard<'a> = MutexGuard<'a, InstalledSource<'a>>;
    type ReplayGuard<'a> = MutexGuard<'a, ReplayState<'a>>;
    let common = 4 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 8 * size_of::<&[u8]>()
        + 8 * size_of::<u64>();
    // source_state copies no payload. source_cut copies InstalledSource once;
    // the guard has ended before the returned Cut is constructed. unchanged
    // retains one guard while comparing two copied optional intervals.
    // check_origin adds only the health/binding references and translated
    // health Result/error carriers already retained in common.
    let source_read = size_of::<InstalledSource<'_>>()
        + 2 * size_of::<SourceGuard<'_>>()
        + size_of::<Result<SourceGuard<'_>>>();
    let source_return =
        size_of::<InstalledSource<'_>>() + size_of::<Cut<'_>>() + size_of::<Result<Cut<'_>>>();
    let source_check = 2 * size_of::<SourceGuard<'_>>()
        + size_of::<Result<SourceGuard<'_>>>()
        + 2 * size_of::<Option<WalRequirement<'_>>>()
        + 2 * size_of::<Option<WalFrameLocation>>();
    let source = common + source_read.max(source_return).max(source_check);
    let scope = size_of::<OwnerIdentity>()
        + size_of::<crate::transaction::concurrency::Result<OwnerIdentity>>()
        + size_of::<Scope<'_>>()
        + size_of::<Result<Scope<'_>>>();
    // checkout explicitly drops the registry guard before constructing Loan.
    // Neither the mutex's ReplayState nor its original backing is moved out.
    let checkout_locked = 2 * size_of::<ReplayGuard<'_>>()
        + size_of::<Result<ReplayGuard<'_>>>()
        + 2 * size_of::<ReplayKey>()
        + size_of::<Option<ReplayKey>>()
        + size_of::<Option<&mut [u8; RETENTION_REPLAY_BYTES]>>();
    let checkout_return =
        size_of::<ReplayKey>() + size_of::<Loan<'_>>() + size_of::<Result<Loan<'_>>>();
    let checkout = common + scope.max(checkout_locked).max(checkout_return);
    // return_checked moves the one original Loan, rather than cloning it.
    // audit/return/quiescent are separate leaf calls. Poison into_inner retains
    // a guard, not an owned ReplayState; key/Option comparisons are real copies.
    let returning = common
        + 2 * size_of::<ReplayGuard<'_>>()
        + 2 * size_of::<Result<ReplayGuard<'_>>>()
        + 2 * size_of::<ReplayKey>()
        + 2 * size_of::<Option<ReplayKey>>()
        + size_of::<Result<()>>();
    // ensure_durable's map/transpose can retain these real enum/result values.
    // It runs before replay checkout in lookup. The WAL implementation's own
    // call closure is intentionally absent and must be added at that phase.
    let durability = common
        + size_of::<WalRequirement<'_>>()
        + size_of::<Option<WalRequirement<'_>>>()
        + size_of::<wal::EnsureDurable<'_>>()
        + size_of::<std::result::Result<wal::EnsureDurable<'_>, wal::WalIoError>>()
        + size_of::<DurableWal<'_>>()
        + size_of::<Result<DurableWal<'_>>>()
        + size_of::<Option<Result<DurableWal<'_>>>>()
        + size_of::<Result<Option<DurableWal<'_>>>>();
    LookupSourceCensus {
        source,
        checkout,
        returning,
        durability,
        peak: source.max(checkout).max(returning).max(durability),
    }
}

/// One conservative steady-state leaf bound. The lookup caller adds the actual
/// WAL durability closure to this bound, separately from canonical replay.
pub(in super::super) fn lookup_source_bytes() -> usize {
    lookup_source_census().peak
}
