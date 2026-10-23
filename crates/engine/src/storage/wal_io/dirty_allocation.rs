//! First real dirty-frame consumer: one allocator page, reserve and cancel.
//! Preparation owns caller workspace; complete stage installs a retained live
//! obligation before any hook or fallible operation. I/O starts after all guards.

use super::super::allocation_reservation::{
    AllocationObservation,
    operation::{self, Batch, OperationBudget, SemanticError},
};
use super::{MutableStorageError, MutableStorageOwner};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry, AllocationError, SystemWorkKind, SystemWorkRef},
        control::Profile,
        dirty_frame::{DirtyError, FrameStamp, SharedPool, TargetRequest},
        page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
        wal_io::{WalFrameLocation, WalIoError, WalPrefix, WalRequirement},
    },
    transaction::concurrency::{Owner, PageConcurrency},
};
use xadb_foundation::{error::Effect, identity::Generation, namespace::WriterNamespace};

pub(super) const OPERATION_BYTES: usize = 98304;
const FRAME_WORK_BYTES: usize = 65536;
const BEFORE_END: usize = FRAME_WORK_BYTES + PAGE_BYTES;
const PROTECTED_LIMIT: u64 = 1 << 30;

#[cfg(test)]
#[path = "dirty_allocation_test_support.rs"]
mod test_support;

#[cfg(test)]
pub(super) use test_support::BeforeSyncPause;

#[cfg(test)]
#[path = "dirty_allocation_native_tests.rs"]
mod native_tests;

#[cfg(test)]
#[path = "dirty_allocation_tests.rs"]
mod tests;

#[cfg(test)]
macro_rules! cut {
    ($control:expr, $point:ident) => {
        test_support::hit(
            &$control.hook,
            test_support::Point::$point,
            test_support::capture(
                &$control.descriptor,
                $control.batch,
                $control.work_high_water,
                $control.terminal_pending,
                $control.installation.as_ref(),
            ),
        );
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Snapshot,
    Preflight,
    Prepare,
    Revalidate,
    Stage,
    Apply,
    Release,
    Sync,
    Observe,
}

#[derive(Debug)]
pub(crate) enum Cause {
    Storage(MutableStorageError),
    Frame(DirtyError),
    Wal(WalIoError),
    Semantic(SemanticError),
    Allocation(AllocationError),
    InvalidInput,
    ActiveReservation,
    NoReservation,
    Exhausted,
    CounterExhausted,
    Capacity,
    Invariant,
}

#[derive(Debug)]
pub(crate) struct DirtyAllocationError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, DirtyAllocationError>;

fn error(phase: Phase, effect: Effect, cause: Cause) -> DirtyAllocationError {
    DirtyAllocationError {
        phase,
        effect,
        cause,
    }
}
fn reject(cause: Cause) -> DirtyAllocationError {
    error(Phase::Preflight, Effect::Unchanged, cause)
}
fn frame_error(phase: Phase, error_value: DirtyError) -> DirtyAllocationError {
    error(phase, error_value.effect, Cause::Frame(error_value))
}
fn merge_cleanup<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Err(original), _) if original.effect == Effect::CallerMustResolve => Err(original),
        (_, Err(cleanup)) => Err(cleanup),
        (result, Ok(())) => result,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InstallPhase {
    StageOwned,
    PageInstalled,
    DescriptorInstalled,
    DptInstalled,
}

/// Not Clone/Copy: only complete stage constructs this live control obligation.
/// Fenced cleanup retains it, including after all temporary page guards end.
pub(super) struct InstallObligation<'s> {
    requirement: WalRequirement<'s>,
    identity: PageIdentity,
    generation: u64,
    first_rec_lsn: PageLsn,
    before_counter: u64,
    after_counter: u64,
    cancel: bool,
    phase: InstallPhase,
}

/// Test-only isolated retained obligation. It requires the authentic receipt
/// of a completed stage; production callers cannot manufacture this state.
#[cfg(test)]
pub(super) fn test_retained_installation<'s>(
    requirement: WalRequirement<'s>,
    stamp: FrameStamp,
) -> InstallObligation<'s> {
    InstallObligation {
        requirement,
        identity: stamp.identity,
        generation: stamp.generation,
        first_rec_lsn: PageLsn(requirement.location().lsn),
        before_counter: stamp.modification_counter - 1,
        after_counter: stamp.modification_counter,
        cancel: false,
        phase: InstallPhase::DptInstalled,
    }
}

/// A sealed image from the validated producer and its actual complete stage.
/// Only this module constructs it. It borrows caller workspace immutably, never
/// lives inside WriterControl, and cannot replace the retained obligation.
pub(in crate::storage) struct InstallPermit<'a, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    stamp: FrameStamp,
    requirement: WalRequirement<'s>,
    after_counter: u64,
    after: &'a [u8; PAGE_BYTES],
}
impl<'s> InstallPermit<'_, '_, 's> {
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn location(&self) -> WalFrameLocation {
        self.requirement.location()
    }
    pub(in crate::storage) fn matches_core(&self, core: &PageConcurrency<'_>) -> bool {
        std::ptr::eq(self.core, core)
    }
    pub(in crate::storage) fn after(&self) -> &[u8; PAGE_BYTES] {
        self.after
    }
    pub(in crate::storage) fn identity(&self) -> PageIdentity {
        self.stamp.identity
    }
    pub(in crate::storage) fn generation(&self) -> u64 {
        self.stamp.generation
    }
    pub(in crate::storage) fn before_counter(&self) -> u64 {
        self.stamp.modification_counter
    }
    pub(in crate::storage) fn after_counter(&self) -> u64 {
        self.after_counter
    }
    pub(in crate::storage) fn page_lsn(&self) -> PageLsn {
        PageLsn(self.location().lsn)
    }
    pub(in crate::storage) fn prior_lsn(&self) -> PageLsn {
        self.stamp.page_lsn
    }
    pub(in crate::storage) fn first_rec_lsn(&self) -> PageLsn {
        self.stamp.rec_lsn.unwrap_or(self.page_lsn())
    }
}

pub(super) fn typed_fits() -> bool {
    std::mem::size_of::<InstallObligation<'_>>() <= 192
        && std::mem::size_of::<Option<InstallObligation<'_>>>() <= 192
        && std::mem::size_of::<InstallPermit<'_, '_, '_>>() <= 384
        && std::mem::size_of::<Result<FrameStamp>>() <= 192
        && base_delta() <= 64
}

pub(super) struct WriterControl<'s, 'o> {
    pub(super) operation: &'o mut [u8; OPERATION_BYTES],
    pub(super) identity: PageIdentity,
    pub(super) descriptor: [u8; 1536],
    pub(super) batch: Option<Batch>,
    pub(super) work_high_water: u64,
    pub(super) terminal_pending: bool,
    pub(super) installation: Option<InstallObligation<'s>>,
    budget: OperationBudget,
    #[cfg(test)]
    hook: Option<std::sync::Arc<test_support::Hook>>,
}

/// Layout only: no second control value is constructed or kept alive.
/// Preserve the exact pre-Status fields (including the diagnostic-only hook)
/// so both plan1 and plan2 account for the shared trusted-budget field growth.
#[allow(dead_code)]
struct BaselineWriterControl<'s, 'o> {
    operation: &'o mut [u8; OPERATION_BYTES],
    identity: PageIdentity,
    descriptor: [u8; 1536],
    batch: Option<Batch>,
    work_high_water: u64,
    terminal_pending: bool,
    installation: Option<InstallObligation<'s>>,
    #[cfg(test)]
    hook: Option<std::sync::Arc<test_support::Hook>>,
}
pub(super) fn baseline_control_bytes() -> usize {
    std::mem::size_of::<BaselineWriterControl<'_, '_>>()
}
pub(super) fn base_delta() -> usize {
    std::mem::size_of::<WriterControl<'_, '_>>().saturating_sub(baseline_control_bytes())
}
impl<'s, 'o> WriterControl<'s, 'o> {
    pub(super) fn new(operation: &'o mut [u8; OPERATION_BYTES], identity: PageIdentity) -> Self {
        Self {
            operation,
            identity,
            descriptor: [0; 1536],
            batch: None,
            work_high_water: 0,
            terminal_pending: false,
            installation: None,
            budget: OperationBudget::Dirty98304,
            #[cfg(test)]
            hook: None,
        }
    }

    /// Only the plan2 bound wraps this control with the remaining16KiB image.
    /// The ordinary writer is never exposed by that bound after INIT.
    pub(super) fn new_status(
        operation: &'o mut [u8; OPERATION_BYTES],
        identity: PageIdentity,
    ) -> Self {
        Self {
            operation,
            identity,
            descriptor: [0; 1536],
            batch: None,
            work_high_water: 0,
            terminal_pending: false,
            installation: None,
            budget: OperationBudget::StatusInit114688,
            #[cfg(test)]
            hook: None,
        }
    }

    fn request(&self) -> TargetRequest {
        TargetRequest {
            identity: self.identity,
            kind: PageKind::Allocation,
            level: 0,
        }
    }

    fn before(&self) -> &[u8; PAGE_BYTES] {
        self.operation[FRAME_WORK_BYTES..BEFORE_END]
            .try_into()
            .expect("fixed before image")
    }

    pub(super) fn observation(
        &self,
        profile: Profile,
        prefix: WalPrefix,
        stamp: &FrameStamp,
        release_terminal: bool,
    ) -> std::result::Result<AllocationObservation, Cause> {
        let page = allocation_page::decode_allocation_page(
            self.before(),
            self.identity,
            profile.spec().namespace,
        )
        .map_err(Cause::Allocation)?;
        if self.terminal_pending != release_terminal
            || (release_terminal && self.batch.is_none())
            || prefix.complete_lsn != prefix.synced_lsn
            || prefix.complete_offset != prefix.synced_offset
            || page.header().lsn != stamp.page_lsn
            || stamp.page_lsn.0 != prefix.complete_lsn
        {
            return Err(Cause::Invariant);
        }
        let batch = if release_terminal { None } else { self.batch };
        operation::validate_live_with_budget(
            &page,
            profile,
            batch,
            &self.descriptor,
            self.work_high_water,
            self.budget,
        )
        .map_err(Cause::Semantic)?;
        let retained = prefix
            .complete_offset
            .checked_sub(4096)
            .ok_or(Cause::Invariant)?;
        let (terminal, tokens) = match batch {
            Some(batch) => (
                self.budget.terminal_wal(batch.count()),
                batch.count() as u64 * PAGE_BYTES as u64,
            ),
            None => (0, 0),
        };
        let protected = super::super::PROTECTED_BYTES
            .checked_add(retained)
            .and_then(|v| v.checked_add(terminal))
            .and_then(|v| v.checked_add(tokens))
            .and_then(|v| {
                v.checked_add(if stamp.rec_lsn.is_some() {
                    PAGE_BYTES as u64
                } else {
                    0
                })
            })
            .filter(|v| *v <= PROTECTED_LIMIT)
            .ok_or(Cause::Invariant)?;
        Ok(AllocationObservation {
            active: batch.map(|b| b.owner),
            reserved: batch.map_or(0, |b| b.count()),
            page_lsn: stamp.page_lsn,
            rec_lsn: stamp.rec_lsn,
            modification_counter: stamp.modification_counter,
            work_high_water: self.work_high_water,
            protected_bytes: protected,
            wal: prefix,
        })
    }

    fn plan_change(
        &self,
        profile: Profile,
        prefix: WalPrefix,
        stamp: &FrameStamp,
        count: Option<usize>,
    ) -> Result<Batch> {
        if count.is_some_and(|n| !self.budget.accepts_count(n)) {
            return Err(reject(Cause::InvalidInput));
        }
        if count.is_some() && self.batch.is_some() {
            return Err(reject(Cause::ActiveReservation));
        }
        if count.is_none() && self.batch.is_none() {
            return Err(reject(Cause::NoReservation));
        }
        // The caller upgrades these live-state failures through its common fence.
        self.observation(profile, prefix, stamp, false)
            .map_err(|cause| error(Phase::Preflight, Effect::CallerMustResolve, cause))?;
        let page = allocation_page::decode_allocation_page(
            self.before(),
            self.identity,
            profile.spec().namespace,
        )
        .map_err(|e| {
            error(
                Phase::Preflight,
                Effect::CallerMustResolve,
                Cause::Allocation(e),
            )
        })?;
        let batch = if let Some(n) = count {
            let mut mask = 0u64;
            for index in 1..64 {
                if let AllocationEntry::Free(g) = page.entry(index).map_err(|e| {
                    error(
                        Phase::Preflight,
                        Effect::CallerMustResolve,
                        Cause::Allocation(e),
                    )
                })? && g.get() != u64::MAX
                    && (mask.count_ones() as usize) < n
                {
                    mask |= 1u64 << index;
                }
            }
            if mask.count_ones() as usize != n {
                return Err(reject(Cause::Exhausted));
            }
            let id = self
                .work_high_water
                .checked_add(1)
                .ok_or_else(|| reject(Cause::CounterExhausted))?;
            let owner = SystemWorkRef::new(
                profile.spec().namespace,
                SystemWorkKind::AllocationMaintenance,
                id,
                Generation::new(1).expect("nonzero"),
            )
            .map_err(|e| {
                error(
                    Phase::Preflight,
                    Effect::CallerMustResolve,
                    Cause::Allocation(e),
                )
            })?;
            Batch {
                owner,
                mask,
                birth: prefix
                    .complete_lsn
                    .checked_add(1)
                    .ok_or_else(|| reject(Cause::CounterExhausted))?,
            }
        } else {
            self.batch.expect("active batch checked")
        };
        let steps = if count.is_some() { 2 } else { 1 };
        prefix
            .complete_lsn
            .checked_add(steps)
            .ok_or_else(|| reject(Cause::CounterExhausted))?;
        stamp
            .modification_counter
            .checked_add(steps)
            .ok_or_else(|| reject(Cause::CounterExhausted))?;
        let additional_wal = if count.is_some() {
            (operation::frame_length(batch.count()) as u64)
                .checked_add(self.budget.terminal_wal(batch.count()))
                .ok_or_else(|| reject(Cause::Capacity))?
        } else {
            operation::frame_length(batch.count()) as u64
        };
        let future = prefix
            .complete_offset
            .checked_add(additional_wal)
            .filter(|end| *end <= crate::storage::wal_io::WAL_FILE_CAP)
            .ok_or_else(|| reject(Cause::Capacity))?;
        super::super::PROTECTED_BYTES
            .checked_add(future - 4096)
            .and_then(|v| v.checked_add((batch.count() as u64 + 1) * PAGE_BYTES as u64))
            .filter(|v| *v <= PROTECTED_LIMIT)
            .ok_or_else(|| reject(Cause::Capacity))?;
        Ok(batch)
    }
}

pub(crate) struct AllocationWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    pub(super) control: &'b mut WriterControl<'s, 'o>,
    pub(super) pool: &'b SharedPool<'c, 's, 'p, 'd>,
    pub(super) storage: &'s MutableStorageOwner<'m, 'w, W>,
}

impl<'s, 'c, W: WriterNamespace> AllocationWriter<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    fn fatal(&self, phase: Phase, cause: Cause) -> DirtyAllocationError {
        self.storage.session_fence().fence();
        error(phase, Effect::CallerMustResolve, cause)
    }

    fn health(&self, phase: Phase) -> Result<()> {
        self.storage
            .health()
            .map_err(|e| error(phase, e.effect, Cause::Storage(e)))
    }

    fn snapshot(&mut self, owner: &mut Owner<'c, 's>) -> Result<FrameStamp> {
        self.health(Phase::Snapshot)?;
        let request = [self.control.request()];
        let read = self
            .pool
            .reserve_targets(owner, &request)
            .map_err(|e| frame_error(Phase::Snapshot, e))?
            .try_read()
            .map_err(|e| frame_error(Phase::Snapshot, e))?;
        let before = (&mut self.control.operation[FRAME_WORK_BYTES..BEFORE_END])
            .try_into()
            .expect("fixed before image");
        let snapshot = read
            .copy_snapshot(0, before)
            .map_err(|e| frame_error(Phase::Snapshot, e));
        merge_cleanup(
            snapshot,
            read.release_checked()
                .map_err(|e| frame_error(Phase::Release, e)),
        )
    }

    pub(crate) fn observe(&mut self, owner: &mut Owner<'c, 's>) -> Result<AllocationObservation> {
        let stamp = self.snapshot(owner)?;
        if self.control.installation.is_some() {
            return Err(self.fatal(Phase::Observe, Cause::Invariant));
        }
        let prefix = self
            .storage
            .wal()
            .prefix()
            .map_err(|e| error(Phase::Observe, e.effect, Cause::Wal(e)))?;
        self.control
            .observation(self.storage.profile(), prefix, &stamp, false)
            .map_err(|e| self.fatal(Phase::Observe, e))
    }

    pub(crate) fn reserved_page(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
    ) -> Result<Option<PageRef>> {
        self.observe(owner)?;
        let Some(batch) = self.control.batch else {
            return Ok(None);
        };
        let Some(index) = (1..64)
            .filter(|i| batch.mask & (1u64 << i) != 0)
            .nth(ordinal)
        else {
            return Ok(None);
        };
        let page = allocation_page::decode_allocation_page(
            self.control.before(),
            self.control.identity,
            self.storage.profile().spec().namespace,
        )
        .map_err(|e| self.fatal(Phase::Observe, Cause::Allocation(e)))?;
        let AllocationEntry::Reserved { generation, .. } = page
            .entry(index)
            .map_err(|e| self.fatal(Phase::Observe, Cause::Allocation(e)))?
        else {
            return Err(self.fatal(Phase::Observe, Cause::Invariant));
        };
        Ok(Some(PageRef {
            page_no: index as u64,
            incarnation: generation,
            ..self.control.identity.page
        }))
    }

    pub(crate) fn reserve_pages(
        &mut self,
        owner: &mut Owner<'c, 's>,
        count: usize,
    ) -> Result<AllocationObservation> {
        self.change(owner, Some(count))
    }
    pub(crate) fn cancel_reserved(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<AllocationObservation> {
        self.change(owner, None)
    }

    fn change(
        &mut self,
        owner: &mut Owner<'c, 's>,
        count: Option<usize>,
    ) -> Result<AllocationObservation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let stamp = self.snapshot(owner)?;
        if self.control.installation.is_some() {
            return Err(self.fatal(Phase::Preflight, Cause::Invariant));
        }
        #[cfg(test)]
        cut!(self.control, Snapshot);
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| error(Phase::Preflight, e.effect, Cause::Wal(e)))?;
        let batch = self
            .control
            .plan_change(storage.profile(), prefix, &stamp, count)
            .inspect_err(|e| {
                if e.effect == Effect::CallerMustResolve {
                    storage.session_fence().fence();
                }
            })?;
        let request = [self.control.request()];
        let next_counter = stamp.modification_counter + 1; // preflight checked terminal headroom
        let control = &mut *self.control;
        let plan = operation::Plan {
            profile: storage.profile(),
            identity: control.identity,
            prefix,
            batch,
            cancel: count.is_none(),
            before_descriptor: count.is_none().then_some(&control.descriptor),
        };
        let (frame_work, rest) = control.operation.split_at_mut(FRAME_WORK_BYTES);
        let (before, after) = rest.split_at_mut(PAGE_BYTES);
        let before: &[u8; PAGE_BYTES] = (&*before).try_into().expect("fixed before image");
        let after: &mut [u8; PAGE_BYTES] = after.try_into().expect("fixed after image");
        let prepared =
            operation::prepare_with_budget(plan, before, frame_work, after, control.budget)
                .map_err(|e| {
                    storage.session_fence().fence();
                    error(
                        Phase::Prepare,
                        Effect::CallerMustResolve,
                        Cause::Semantic(e),
                    )
                })?;
        let after_descriptor = prepared.descriptor_bytes();
        let mut write = self
            .pool
            .reserve_targets(owner, &request)
            .map_err(|e| frame_error(Phase::Revalidate, e))?
            .try_write()
            .map_err(|e| frame_error(Phase::Revalidate, e))?;
        let installed = (|| -> Result<()> {
            write
                .matches_stamp(0, &stamp)
                .map_err(|e| frame_error(Phase::Revalidate, e))?;
            let dpt = write
                .preflight_dpt(0, &stamp)
                .map_err(|e| frame_error(Phase::Revalidate, e))?;
            let requirement = prepared
                .stage_required(storage.wal())
                .map_err(|e| error(Phase::Stage, e.effect, Cause::Wal(e)))?;
            let location = requirement.location();
            // S1: no fallible call or hook between complete slot and retained ownership.
            let first_rec_lsn = stamp.rec_lsn.unwrap_or(PageLsn(location.lsn));
            control.batch = Some(batch);
            control.terminal_pending = count.is_none();
            if count.is_some() {
                control.work_high_water = batch.owner.id();
            }
            control.installation = Some(InstallObligation {
                requirement,
                identity: stamp.identity,
                generation: stamp.generation,
                first_rec_lsn,
                before_counter: stamp.modification_counter,
                after_counter: next_counter,
                cancel: count.is_none(),
                phase: InstallPhase::StageOwned,
            });
            let permit = InstallPermit {
                core: self.pool.bound_core(),
                stamp,
                requirement,
                after_counter: next_counter,
                after,
            };
            #[cfg(test)]
            cut!(control, AfterStageOwned);
            // S2: page bytes/LSN, while descriptor/counter/recLSN/DPT remain before.
            write
                .install_page(0, &permit)
                .map_err(|e| frame_error(Phase::Apply, e))?;
            control.installation.as_mut().expect("stage-owned").phase = InstallPhase::PageInstalled;
            #[cfg(test)]
            cut!(control, AfterPageInstalled);
            // S3: this entire group has no callback/hook between its fields.
            write
                .install_metadata(0, &permit)
                .map_err(|e| frame_error(Phase::Apply, e))?;
            control.descriptor.copy_from_slice(after_descriptor);
            control.installation.as_mut().expect("stage-owned").phase =
                InstallPhase::DescriptorInstalled;
            #[cfg(test)]
            cut!(control, AfterDescriptorInstalled);
            // S4: the fixed registry slot was checked before append and its leaf
            // mutex was released. A newly discovered failure is fatal.
            write
                .install_dpt(dpt, &permit)
                .map_err(|e| frame_error(Phase::Apply, e))?;
            control.installation.as_mut().expect("stage-owned").phase = InstallPhase::DptInstalled;
            #[cfg(test)]
            cut!(control, AfterDptInstalled);
            Ok(())
        })();
        let installed = installed.map_err(|mut e| {
            if control.installation.is_some() {
                storage.session_fence().fence();
                e.effect = Effect::CallerMustResolve;
            }
            e
        });
        merge_cleanup(
            installed,
            write
                .release_checked()
                .map_err(|e| frame_error(Phase::Release, e)),
        )?;
        // No pins, admission or page/version guards survive this boundary.
        #[cfg(test)]
        cut!(control, BeforeSync);
        let requirement = self
            .control
            .installation
            .as_ref()
            .expect("stage-owned")
            .requirement;
        storage.wal().wait_durable(&requirement).map_err(|e| {
            storage.session_fence().fence();
            error(Phase::Sync, Effect::CallerMustResolve, Cause::Wal(e))
        })?;
        self.health(Phase::Sync)?;
        // The change is already durable. Even a transient snapshot conflict
        // must fence the retained obligation, never report the call unchanged.
        let final_stamp = self
            .snapshot(owner)
            .map_err(|e| self.fatal(e.phase, e.cause))?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| self.fatal(Phase::Observe, Cause::Wal(e)))?;
        let observation = self
            .control
            .observation(storage.profile(), prefix, &final_stamp, count.is_none())
            .map_err(|e| self.fatal(Phase::Observe, e))?;
        if count.is_none() {
            self.control.batch = None;
            self.control.terminal_pending = false;
        }
        self.control.installation = None;
        Ok(observation)
    }
}
