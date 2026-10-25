//! Exclusive first-extent reservations: semantic WAL and authoritative dirty page.
//! D3-ALLOC-RESERVE-001. No payload INIT, home write, reopen or transaction ACK.
use super::super::super::{self as wal, WalPrefix};
use super::DoublewriteStorageSession;
use crate::storage::{
    allocation_page::{self, AllocationEntry, AllocationError, SystemWorkKind, SystemWorkRef},
    control::{ControlError, Profile},
    obligation::{OBLIGATION_BYTES, ObligationError},
    page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
    page_io::PageIoError,
};
use foundation::{error::Effect, identity::Generation, namespace::WriterNamespace};
use std::sync::atomic::Ordering;

#[path = "allocation_operation.rs"]
pub(super) mod operation;
use operation::{Batch, SemanticError};

#[path = "allocation_budget.rs"]
pub(super) mod budget;

#[cfg(test)]
#[path = "allocation_reservation_tests.rs"]
mod tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "allocation_native_tests.rs"]
mod native_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "allocation_resource_tests.rs"]
mod resource_tests;

const OPERATION_BYTES: usize = 81920;
const FRAME_WORK_BYTES: usize = 65536;
const PROTECTED_LIMIT: u64 = 1 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Constructor,
    Read,
    Preflight,
    Prepare,
    Stage,
    Apply,
    Sync,
    Observe,
}
#[derive(Debug)]
pub(crate) enum Cause {
    InvalidInput,
    UnsupportedPlan,
    ActiveReservation,
    NoReservation,
    Exhausted,
    CounterExhausted,
    Capacity,
    Fenced,
    Poisoned,
    Invariant,
    Control(ControlError),
    Allocation(AllocationError),
    PageIo(PageIoError),
    Obligation(ObligationError),
    Wal(wal::WalIoError),
    Semantic(SemanticError),
}
#[derive(Debug)]
pub(crate) struct AllocationSessionError {
    pub(crate) phase: Phase,
    pub(crate) effect: Effect,
    pub(crate) cause: Cause,
}
type Result<T> = std::result::Result<T, AllocationSessionError>;
fn error(phase: Phase, effect: Effect, cause: Cause) -> AllocationSessionError {
    AllocationSessionError {
        phase,
        effect,
        cause,
    }
}
fn reject(cause: Cause) -> AllocationSessionError {
    error(Phase::Preflight, Effect::Unchanged, cause)
}

/// Copy-only diagnostics. A reserved PageRef does not authorize payload access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AllocationObservation {
    pub(crate) active: Option<SystemWorkRef>,
    pub(crate) reserved: usize,
    pub(crate) page_lsn: PageLsn,
    pub(crate) rec_lsn: Option<PageLsn>,
    pub(crate) modification_counter: u64,
    pub(crate) work_high_water: u64,
    pub(crate) protected_bytes: u64,
    pub(crate) wal: WalPrefix,
}

struct AllocationState<'p, 'o> {
    frame: &'p mut [u8; PAGE_BYTES],
    operation: &'o mut [u8; OPERATION_BYTES],
    identity: PageIdentity,
    batch: Option<Batch>,
    descriptor: [u8; OBLIGATION_BYTES],
    rec_lsn: Option<PageLsn>,
    modification_counter: u64,
    work_high_water: u64,
    terminal_pending: bool,
    #[cfg(test)]
    hook: Option<tests::Hook>,
}

/// Retains all four caller borrows and the only installed namespace/WAL owner.
/// Drop only releases Rust ownership; it never syncs, cancels or deletes files.
pub(crate) struct AllocationSession<'m, 'w, 'p, 'o, W: WriterNamespace> {
    installed: DoublewriteStorageSession<'m, 'w, W>,
    state: AllocationState<'p, 'o>,
}

fn health<W: WriterNamespace>(
    installed: &DoublewriteStorageSession<'_, '_, W>,
    phase: Phase,
) -> Result<()> {
    let wal = &installed.fresh.core.wal;
    // Preserve poison rather than letting is_fenced collapse its original cause.
    let cause = if wal.append.is_poisoned() || wal.file.is_poisoned() || wal._writer.is_poisoned() {
        Some(Cause::Poisoned)
    } else if wal.fenced.is_fenced() {
        Some(Cause::Fenced)
    } else {
        None
    };
    if let Some(cause) = cause {
        wal.fenced.fence();
        return Err(error(phase, Effect::CallerMustResolve, cause));
    }
    Ok(())
}
fn fatal<W: WriterNamespace>(
    installed: &DoublewriteStorageSession<'_, '_, W>,
    phase: Phase,
    cause: Cause,
) -> AllocationSessionError {
    installed.fresh.core.wal.fenced.fence();
    error(phase, Effect::CallerMustResolve, cause)
}

impl<'m, 'w, W: WriterNamespace> DoublewriteStorageSession<'m, 'w, W> {
    /// Consume pristine installed storage and read its trusted allocator page.
    /// Every failure consumes/fences the owner; output scratch must be discarded.
    pub(crate) fn into_allocation_session<'p, 'o>(
        self,
        frame: &'p mut [u8; PAGE_BYTES],
        operation: &'o mut [u8; OPERATION_BYTES],
    ) -> Result<AllocationSession<'m, 'w, 'p, 'o, W>> {
        let identity = {
            let _unwind = self.fresh.core.wal.fenced.unwind_guard();
            (|| {
                let fail = |cause| fatal(&self, Phase::Constructor, cause);
                if !budget::fits::<W>() {
                    return Err(fail(Cause::Capacity));
                }
                health(&self, Phase::Constructor)?;
                if self.fresh.core.plan.kind != super::super::BootstrapPlanKind::AllocatorOnly {
                    return Err(fail(Cause::UnsupportedPlan));
                }
                let prefix = self
                    .fresh
                    .core
                    .wal
                    .prefix()
                    .map_err(|e| fail(Cause::Wal(e)))?;
                if self.fresh.core.reader_active.load(Ordering::Acquire)
                    || prefix.complete_lsn != 0
                    || prefix.synced_lsn != 0
                    || prefix.complete_offset != 4096
                    || prefix.synced_offset != 4096
                    || prefix.stream.writer_epoch != 1
                    || self.fresh.core.plan.profile != self._plan.profile
                {
                    return Err(fail(Cause::Invariant));
                }
                {
                    let append = self
                        .fresh
                        .core
                        .wal
                        .state(wal::Phase::Observe)
                        .map_err(|e| fail(Cause::Wal(e)))?;
                    if append.state != wal::SlotState::Idle || append.slot.is_none() {
                        return Err(fail(Cause::Invariant));
                    }
                }
                let profile = self.fresh.core.plan.profile;
                let identity = super::super::fixed_entry()
                    .map_err(|e| fail(Cause::Control(e)))?
                    .metadata_identity(profile.spec().namespace, 0)
                    .map_err(|e| fail(Cause::Control(e)))?;
                let (root, rest) = operation.split_at_mut(PAGE_BYTES);
                let root: &mut [u8; PAGE_BYTES] = root.try_into().expect("fixed root size");
                let scratch: &mut [u8; PAGE_BYTES] = (&mut rest[..PAGE_BYTES])
                    .try_into()
                    .expect("fixed scratch size");
                {
                    let mut reader = self
                        .open_reader(root, scratch)
                        .map_err(|e| fatal(&self, Phase::Read, Cause::PageIo(e)))?;
                    reader
                        .read_page(identity, PageKind::Allocation, 0, frame)
                        .map_err(|e| fatal(&self, Phase::Read, Cause::PageIo(e)))?;
                }
                let page = allocation_page::decode_allocation_page(
                    frame,
                    identity,
                    profile.spec().namespace,
                )
                .map_err(|e| fatal(&self, Phase::Read, Cause::Allocation(e)))?;
                if page.header().lsn != PageLsn(0) {
                    return Err(fail(Cause::Invariant));
                }
                for index in 1..64 {
                    if page.entry(index).map_err(|e| fail(Cause::Allocation(e)))?
                        != AllocationEntry::Free(Generation::new(1).expect("nonzero"))
                    {
                        return Err(fail(Cause::Invariant));
                    }
                }
                Ok(identity)
            })()?
        };
        Ok(AllocationSession {
            installed: self,
            state: AllocationState {
                frame,
                operation,
                identity,
                batch: None,
                descriptor: [0; OBLIGATION_BYTES],
                rec_lsn: None,
                modification_counter: 0,
                work_high_water: 0,
                terminal_pending: false,
                #[cfg(test)]
                hook: None,
            },
        })
    }
}

impl<W: WriterNamespace> AllocationSession<'_, '_, '_, '_, W> {
    pub(crate) fn is_fenced(&self) -> bool {
        self.installed.is_fenced()
    }
    pub(crate) fn observe(&self) -> Result<AllocationObservation> {
        health(&self.installed, Phase::Observe)?;
        let wal = self
            .installed
            .fresh
            .core
            .wal
            .prefix()
            .map_err(|e| error(Phase::Observe, e.effect, Cause::Wal(e)))?;
        self.state
            .observe(self.installed.fresh.core.plan.profile, wal)
            .map_err(|cause| fatal(&self.installed, Phase::Observe, cause))
    }
    pub(crate) fn reserved_page(&self, ordinal: usize) -> Result<Option<PageRef>> {
        self.observe()?;
        let Some(batch) = self.state.batch else {
            return Ok(None);
        };
        let Some(index) = (1..64)
            .filter(|i| batch.mask & (1u64 << i) != 0)
            .nth(ordinal)
        else {
            return Ok(None);
        };
        let page = allocation_page::decode_allocation_page(
            self.state.frame,
            self.state.identity,
            self.installed.fresh.core.plan.profile.spec().namespace,
        )
        .map_err(|e| fatal(&self.installed, Phase::Observe, Cause::Allocation(e)))?;
        let AllocationEntry::Reserved { generation, .. } = page
            .entry(index)
            .map_err(|e| fatal(&self.installed, Phase::Observe, Cause::Allocation(e)))?
        else {
            return Err(fatal(&self.installed, Phase::Observe, Cause::Invariant));
        };
        Ok(Some(PageRef {
            page_no: index as u64,
            incarnation: generation,
            ..self.state.identity.page
        }))
    }
    pub(crate) fn reserve_pages(&mut self, count: usize) -> Result<AllocationObservation> {
        self.state.change(&self.installed, Some(count))
    }
    pub(crate) fn cancel_reserved(&mut self) -> Result<AllocationObservation> {
        self.state.change(&self.installed, None)
    }
}

impl AllocationState<'_, '_> {
    fn protection(&self, prefix: WalPrefix) -> Option<u64> {
        let retained = prefix.complete_offset.checked_sub(4096)?;
        let (terminal, tokens) = match self.batch {
            Some(batch) => (
                if self.terminal_pending {
                    0
                } else {
                    operation::frame_length(batch.count()) as u64
                },
                batch.count() as u64 * PAGE_BYTES as u64,
            ),
            None => (0, 0),
        };
        super::PROTECTED_BYTES
            .checked_add(retained)?
            .checked_add(terminal)?
            .checked_add(tokens)?
            .checked_add(if self.rec_lsn.is_some() {
                PAGE_BYTES as u64
            } else {
                0
            })
    }
    fn observe(
        &self,
        profile: Profile,
        prefix: WalPrefix,
    ) -> std::result::Result<AllocationObservation, Cause> {
        self.observation(profile, prefix, false)
    }

    // Validate the prospective successful observation before releasing tokens.
    // Any error or unwind still leaves the terminal-pending charge owned here.
    fn observation(
        &self,
        profile: Profile,
        prefix: WalPrefix,
        release_terminal: bool,
    ) -> std::result::Result<AllocationObservation, Cause> {
        let page = allocation_page::decode_allocation_page(
            self.frame,
            self.identity,
            profile.spec().namespace,
        )
        .map_err(Cause::Allocation)?;
        if self.terminal_pending != release_terminal
            || (release_terminal && self.batch.is_none())
            || prefix.complete_lsn != prefix.synced_lsn
            || prefix.complete_offset != prefix.synced_offset
            || page.header().lsn.0 != prefix.complete_lsn
        {
            return Err(Cause::Invariant);
        }
        let batch = if release_terminal { None } else { self.batch };
        operation::validate_live(
            &page,
            profile,
            batch,
            &self.descriptor,
            self.work_high_water,
        )
        .map_err(Cause::Semantic)?;
        let retained_tokens = if release_terminal {
            self.batch
                .map_or(0, |b| b.count() as u64 * PAGE_BYTES as u64)
        } else {
            0
        };
        let protected = self
            .protection(prefix)
            .and_then(|p| p.checked_sub(retained_tokens))
            .filter(|p| *p <= PROTECTED_LIMIT)
            .ok_or(Cause::Invariant)?;
        Ok(AllocationObservation {
            active: batch.map(|b| b.owner),
            reserved: batch.map_or(0, |b| b.count()),
            page_lsn: page.header().lsn,
            rec_lsn: self.rec_lsn,
            modification_counter: self.modification_counter,
            work_high_water: self.work_high_water,
            protected_bytes: protected,
            wal: prefix,
        })
    }

    // Finish preflight's page scan and admission arithmetic before the producer
    // and independent validator enter their nested codec calls.
    fn plan_change<W: WriterNamespace>(
        &self,
        installed: &DoublewriteStorageSession<'_, '_, W>,
        count: Option<usize>,
    ) -> Result<(wal::WalPrefix, Batch)> {
        let wal = &installed.fresh.core.wal;
        health(installed, Phase::Preflight)?;
        if count.is_some_and(|n| !(1..=63).contains(&n)) {
            return Err(reject(Cause::InvalidInput));
        }
        if count.is_some() && self.batch.is_some() {
            return Err(reject(Cause::ActiveReservation));
        }
        if count.is_none() && self.batch.is_none() {
            return Err(reject(Cause::NoReservation));
        }
        let prefix = wal
            .prefix()
            .map_err(|e| error(Phase::Preflight, e.effect, Cause::Wal(e)))?;
        self.observe(installed.fresh.core.plan.profile, prefix)
            .map_err(|e| fatal(installed, Phase::Preflight, e))?;
        let page = allocation_page::decode_allocation_page(
            self.frame,
            self.identity,
            installed.fresh.core.plan.profile.spec().namespace,
        )
        .map_err(|e| fatal(installed, Phase::Preflight, Cause::Allocation(e)))?;
        let batch = if let Some(n) = count {
            let mut mask = 0u64;
            for index in 1..64 {
                if let AllocationEntry::Free(g) = page
                    .entry(index)
                    .map_err(|e| fatal(installed, Phase::Preflight, Cause::Allocation(e)))?
                    && g.get() != u64::MAX
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
                installed.fresh.core.plan.profile.spec().namespace,
                SystemWorkKind::AllocationMaintenance,
                id,
                Generation::new(1).expect("nonzero"),
            )
            .map_err(|e| fatal(installed, Phase::Preflight, Cause::Allocation(e)))?;
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
        self.modification_counter
            .checked_add(steps)
            .ok_or_else(|| reject(Cause::CounterExhausted))?;
        let length = operation::frame_length(batch.count());
        let future = prefix
            .complete_offset
            .checked_add(length as u64 * steps)
            .filter(|end| *end <= wal::WAL_FILE_CAP)
            .ok_or_else(|| reject(Cause::Capacity))?;
        let protected = super::PROTECTED_BYTES
            .checked_add(future - 4096)
            .and_then(|v| v.checked_add((batch.count() as u64 + 1) * PAGE_BYTES as u64))
            .filter(|v| *v <= PROTECTED_LIMIT)
            .ok_or_else(|| reject(Cause::Capacity))?;
        let _ = protected; // Admission precedes stage; no charge is returned here.
        Ok((prefix, batch))
    }

    fn change<W: WriterNamespace>(
        &mut self,
        installed: &DoublewriteStorageSession<'_, '_, W>,
        count: Option<usize>,
    ) -> Result<AllocationObservation> {
        let wal = &installed.fresh.core.wal;
        let _unwind = wal.fenced.unwind_guard();
        let (prefix, batch) = self.plan_change(installed, count)?;
        #[cfg(test)]
        tests::cut(self, tests::Point::Snapshot, wal);
        let plan = operation::Plan {
            profile: installed.fresh.core.plan.profile,
            identity: self.identity,
            prefix,
            batch,
            cancel: count.is_none(),
            before_descriptor: if count.is_none() {
                Some(&self.descriptor)
            } else {
                None
            },
        };
        let (frame_work, after_work) = self.operation.split_at_mut(FRAME_WORK_BYTES);
        let after: &mut [u8; PAGE_BYTES] = after_work.try_into().expect("fixed after page");
        let prepared = operation::prepare(plan, self.frame, frame_work, after)
            .map_err(|e| fatal(installed, Phase::Prepare, Cause::Semantic(e)))?;
        let location = prepared
            .stage(wal)
            .map_err(|e| error(Phase::Stage, e.effect, Cause::Wal(e)))?;
        // The copied WAL already owns this obligation, even if application
        // unwinds before the authoritative page and descriptor are installed.
        self.batch = Some(batch);
        self.terminal_pending = count.is_none();
        self.rec_lsn.get_or_insert(PageLsn(location.lsn));
        if count.is_some() {
            self.work_high_water = batch.owner.id();
        }
        #[cfg(test)]
        tests::stage_cut(self.hook.as_ref(), wal);
        // No fallible work from the complete slot copy through live application.
        self.frame.copy_from_slice(after);
        self.descriptor.copy_from_slice(&frame_work[352..1888]);
        self.modification_counter += 1;
        #[cfg(test)]
        tests::cut(self, tests::Point::Apply, wal);
        wal.sync_pending()
            .map_err(|e| fatal(installed, Phase::Sync, Cause::Wal(e)))?;
        #[cfg(test)]
        tests::cut(self, tests::Point::Sync, wal);
        health(installed, Phase::Sync)?;
        let prefix = wal
            .prefix()
            .map_err(|e| fatal(installed, Phase::Sync, Cause::Wal(e)))?;
        let observation = self
            .observation(installed.fresh.core.plan.profile, prefix, count.is_none())
            .map_err(|e| fatal(installed, Phase::Observe, e))?;
        if count.is_none() {
            self.batch = None;
            self.terminal_pending = false;
        }
        Ok(observation)
    }
}
