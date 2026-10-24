//! One-shot conversion and binding of installed storage to actual dirty frames.
//! Initial readback occurs only for the atomic claim winner, before views exist.

use super::super::super::{self as wal, GenesisWalSession};
use super::{DoublewriteStorageSession, allocation_reservation::budget};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry, AllocationError},
        control::{ControlError, Profile},
        dirty_frame::{
            self, DirtyError, DirtyFrame, DirtyObservation, ReadTargets, SharedPool, TargetRequest,
        },
        page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn},
        page_io::{PageFileReader, PageIoError},
        session_fence::SessionFence,
    },
    transaction::concurrency::{Owner, PageConcurrency},
};
use std::{
    mem::size_of,
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
};
use xadb_foundation::{error::Effect, identity::Generation, namespace::WriterNamespace};

#[path = "dirty_allocation.rs"]
mod dirty_allocation;
pub(in crate::storage) use dirty_allocation::InstallPermit as DirtyInstallPermit;
pub(crate) use dirty_allocation::{AllocationWriter, DirtyAllocationError};
use dirty_allocation::{OPERATION_BYTES, WriterControl};

#[path = "recovery_prefix.rs"]
mod recovery_prefix;
#[path = "status_init.rs"]
mod status_init;
#[path = "status_init_operation.rs"]
mod status_init_operation;
pub(crate) use recovery_prefix::{
    Cause as RecoveryPrefixCause, ClosedRecovery, Phase as RecoveryPrefixPhase,
    ProducerMode as RecoveryProducerMode, RecoveryAcquireError, RecoveryCause, RecoveryError,
    RecoveryFailure, RecoveryInspection, RecoveryPhase, RecoveryPrefix, RecoveryPrefixError,
    RecoveryPrefixObservation,
};
pub(crate) use status_init::{
    BeforeEffectError, BoundStatusBootstrap, BoundStatusWithFlush, EMPTY_TRANSACTION_BANK_BYTES,
    EmptyTransactionSnapshot, EmptyTransactions, InitAttemptError, InitializedStatusObservation,
    IntoEmptyTransactionsError, IntoFlushError, IntoOperationalStatusError, OperationalStatus,
    OperationalStatusReader, OperationalStatusWriter, ReadStatusSet, ReservedStatus,
    ResidentReadAdmission, ResidentStatus, ResidentStatusBinding, ResidentStatusReader,
    StatusBootstrapWriter, StatusFlushError, StatusFlusher, StatusInitError, StatusReadGuard,
    StatusReader,
};
pub(in crate::storage) use status_init::{
    CompletedInitAuthority, InitAuthority, InitControlCommit, InitInstallPermit,
    StatusFlushAuthority,
};

#[path = "allocator_flush.rs"]
mod allocator_flush;
#[path = "flush_io.rs"]
mod flush_io;
use allocator_flush::FlushControl;
pub(in crate::storage) use allocator_flush::FlushFinalizePermit;
pub(crate) use allocator_flush::{AllocatorFlushError, AllocatorFlusher, FlushProgress};

mod allocator_residency;
pub(in crate::storage) use allocator_residency::ReloadInstallPermit;
pub(crate) use allocator_residency::{
    IntoOperationalError, OperationalAllocation, ReloadError, ReloadObservation,
};

const AVAILABLE: u8 = 0;
const CLAIMED: u8 = 1;
const BOUND: u8 = 2;
const ABANDONED: u8 = 3;

#[cfg(test)]
#[path = "status_bootstrap_tests.rs"]
mod status_bootstrap_tests;

#[derive(Debug)]
pub(crate) enum Cause {
    Fenced,
    Poisoned,
    Invariant,
    BindingMismatch,
    AlreadyBound,
    Shape,
    Capacity,
    UnsupportedPlan,
    Bootstrap(super::super::BootstrapError),
    Frame(DirtyError),
    Wal(wal::WalIoError),
    PageIo(PageIoError),
    Allocation(AllocationError),
    Control(ControlError),
}
#[derive(Debug)]
pub(crate) struct MutableStorageError {
    pub(crate) cause: Cause,
    pub(crate) effect: Effect,
}
pub(crate) type Result<T> = std::result::Result<T, MutableStorageError>;
fn unchanged(cause: Cause) -> MutableStorageError {
    MutableStorageError {
        cause,
        effect: Effect::Unchanged,
    }
}

pub(crate) struct MutableStorageOwner<'m, 'w, W: WriterNamespace> {
    fresh: super::FreshStorageSession<'m, W>,
    plan: super::InstallPlan,
    workspace: Mutex<Option<&'w mut [u8; super::WORKSPACE_BYTES]>>,
    binding: AtomicU8,
}
impl<'m, 'w, W: WriterNamespace> DoublewriteStorageSession<'m, 'w, W> {
    /// An infallible move: original health, reader claim and fence are preserved.
    pub(crate) fn into_mutable_storage(self) -> MutableStorageOwner<'m, 'w, W> {
        let DoublewriteStorageSession {
            fresh,
            _workspace,
            _plan,
        } = self;
        MutableStorageOwner {
            fresh,
            plan: _plan,
            workspace: Mutex::new(Some(_workspace)),
            binding: AtomicU8::new(AVAILABLE),
        }
    }
}
impl<'m, 'w, W: WriterNamespace> MutableStorageOwner<'m, 'w, W> {
    pub(crate) fn session_fence(&self) -> &SessionFence {
        &self.fresh.wal.fenced
    }
    fn fatal(&self, cause: Cause) -> MutableStorageError {
        self.session_fence().fence();
        MutableStorageError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
    pub(super) fn health(&self) -> Result<()> {
        let wal = self.wal();
        if wal.append.is_poisoned()
            || wal.file.is_poisoned()
            || wal._writer.is_poisoned()
            || self.workspace.is_poisoned()
        {
            return Err(self.fatal(Cause::Poisoned));
        }
        if self.session_fence().is_fenced() {
            return Err(self.fatal(Cause::Fenced));
        }
        Ok(())
    }
    pub(super) fn profile(&self) -> Profile {
        self.fresh.plan.profile
    }
    pub(super) fn wal(&self) -> &GenesisWalSession<'m, W> {
        &self.fresh.wal
    }

    /// Borrow the original immutable bootstrap role without consuming binding.
    /// Mutable workspace health stays typed; the fresh error is preserved whole.
    pub(crate) fn status_role(&self) -> Result<super::super::StatusRole<'_>> {
        self.health()?;
        self.fresh
            .status_role()
            .map_err(|error| MutableStorageError {
                effect: error.effect,
                cause: Cause::Bootstrap(error),
            })
    }

    /// Borrow plan3's Undo role only after the full mutable owner is healthy.
    pub(crate) fn undo_role(&self) -> Result<crate::storage::wal_io::UndoRole<'_>> {
        self.health()?;
        self.fresh.undo_role().map_err(|error| MutableStorageError {
            effect: error.effect,
            cause: Cause::Bootstrap(error),
        })
    }

    /// Shape/resource rejection precedes the atomic claim and every readback.
    /// Claimed failure is permanently abandoned; caller scratch is then invalid.
    pub(crate) fn bind_allocator<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        core: &'c PageConcurrency<'s>,
        frames: &'p mut [DirtyFrame<'d>],
        operation: &'o mut [u8; OPERATION_BYTES],
    ) -> Result<BoundAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        self.health()?;
        if self.fresh.plan.kind != super::super::BootstrapPlanKind::AllocatorOnly {
            return Err(unchanged(Cause::UnsupportedPlan));
        }
        if !core.uses_fence(self.session_fence()) {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if frames.is_empty() || frames.len() > dirty_frame::MAX_FRAMES {
            return Err(unchanged(Cause::Shape));
        }
        if !fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        for frame in frames.iter_mut() {
            frame
                .check_empty()
                .map_err(|error| self.fatal(Cause::Frame(error)))?;
        }
        self.health()?;
        match self
            .binding
            .compare_exchange(AVAILABLE, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(CLAIMED | BOUND) => {
                self.health()?;
                return Err(unchanged(Cause::AlreadyBound));
            }
            Err(ABANDONED) => return Err(self.fatal(Cause::Fenced)),
            Err(_) => return Err(self.fatal(Cause::Invariant)),
        }
        let mut claim = BindingClaim {
            storage: self,
            complete: false,
        };
        // This phase ends before constructing the large directory/DPT arrays.
        let identity = self.read_allocator(&mut frames[0], operation)?;
        frames[0]
            .register(TargetRequest {
                identity,
                kind: PageKind::Allocation,
                level: 0,
            })
            .map_err(|error| self.fatal(Cause::Frame(error)))?;
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| self.fatal(Cause::Poisoned))?
            .take()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        let pool = SharedPool::new(core, self.session_fence(), &self.binding, frames)
            .map_err(|error| self.fatal(Cause::Frame(error)))?;
        let bound = BoundAllocation {
            storage: self,
            pool,
            writer: WriterControl::new(operation, identity),
            flush: FlushControl::new(
                flush_io::FlushIo::new(
                    self.plan.selected,
                    workspace,
                    &self.binding,
                    self.session_fence(),
                ),
                identity,
            ),
        };
        self.health()?;
        self.binding.store(BOUND, Ordering::Release);
        claim.complete = true;
        Ok(bound)
    }

    fn read_allocator(
        &self,
        frame: &mut DirtyFrame<'_>,
        operation: &mut [u8; OPERATION_BYTES],
    ) -> Result<PageIdentity> {
        self.health()?;
        let prefix = self
            .wal()
            .prefix()
            .map_err(|error| self.fatal(Cause::Wal(error)))?;
        if self.fresh.reader_active.load(Ordering::Acquire)
            || prefix.complete_lsn != 0
            || prefix.synced_lsn != 0
            || prefix.complete_offset != 4096
            || prefix.synced_offset != 4096
            || prefix.stream.writer_epoch != 1
            || self.profile() != self.plan.profile
        {
            return Err(self.fatal(Cause::Invariant));
        }
        {
            let append = self
                .wal()
                .state(wal::Phase::Observe)
                .map_err(|error| self.fatal(Cause::Wal(error)))?;
            if append.state != wal::SlotState::Idle || append.slot.is_none() {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        let profile = self.profile();
        let identity = super::super::fixed_entry()
            .map_err(|error| self.fatal(Cause::Control(error)))?
            .metadata_identity(profile.spec().namespace, 0)
            .map_err(|error| self.fatal(Cause::Control(error)))?;
        let payload = frame
            .initial_payload()
            .map_err(|error| self.fatal(Cause::Frame(error)))?;
        {
            let (root, rest) = operation.split_at_mut(PAGE_BYTES);
            let root: &mut [u8; PAGE_BYTES] = root.try_into().expect("fixed root workspace");
            let scratch: &mut [u8; PAGE_BYTES] = (&mut rest[..PAGE_BYTES])
                .try_into()
                .expect("fixed read scratch");
            let mut reader = self
                .fresh
                .open_reader(root, scratch)
                .map_err(|error| self.fatal(Cause::PageIo(error)))?;
            reader
                .read_page(identity, PageKind::Allocation, 0, payload)
                .map_err(|error| self.fatal(Cause::PageIo(error)))?;
        }
        let page =
            allocation_page::decode_allocation_page(payload, identity, profile.spec().namespace)
                .map_err(|error| self.fatal(Cause::Allocation(error)))?;
        if page.header().lsn != PageLsn(0) {
            return Err(self.fatal(Cause::Invariant));
        }
        for index in 1..64 {
            if page
                .entry(index)
                .map_err(|error| self.fatal(Cause::Allocation(error)))?
                != AllocationEntry::Free(Generation::new(1).expect("nonzero generation"))
            {
                return Err(self.fatal(Cause::Invariant));
            }
        }
        self.health()?;
        Ok(identity)
    }
}

struct BindingClaim<'s, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    complete: bool,
}
impl<W: WriterNamespace> Drop for BindingClaim<'_, '_, '_, W> {
    fn drop(&mut self) {
        if !self.complete {
            self.storage.session_fence().fence();
            self.storage.binding.store(ABANDONED, Ordering::Release);
        }
    }
}

pub(crate) struct BoundAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: SharedPool<'c, 's, 'p, 'd>,
    writer: WriterControl<'s, 'o>,
    flush: FlushControl<'c, 's, 'w>,
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    BoundAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn split<'b>(
        &'b mut self,
    ) -> (
        AllocationWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        DirtyReader<'b, 'c, 's, 'p, 'd>,
    ) {
        (
            AllocationWriter {
                control: &mut self.writer,
                pool: &self.pool,
                storage: self.storage,
            },
            DirtyReader { pool: &self.pool },
        )
    }

    /// Disjoint views over the same original storage, pool and dedicated workspace.
    /// A pending flush remains in this Bound when a temporary view is dropped.
    pub(crate) fn split_with_flush<'b>(
        &'b mut self,
    ) -> (
        AllocationWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        DirtyReader<'b, 'c, 's, 'p, 'd>,
        AllocatorFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        (
            AllocationWriter {
                control: &mut self.writer,
                pool: &self.pool,
                storage: self.storage,
            },
            DirtyReader { pool: &self.pool },
            AllocatorFlusher {
                control: &mut self.flush,
                pool: &self.pool,
                storage: self.storage,
            },
        )
    }
}
impl<W: WriterNamespace> Drop for BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W> {
    fn drop(&mut self) {
        // Flush completion is not a clean shutdown/owner-release protocol.
        self.storage.session_fence().fence();
        self.storage.binding.store(ABANDONED, Ordering::Release);
        let _ = self.pool.has_pins(); // Census may diagnose orphaned tokens; never reset them.
    }
}

/// Shared read view has no storage/W field; its auto traits follow the pool only.
pub(crate) struct DirtyReader<'b, 'c, 's, 'p, 'd> {
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
}
impl<'b, 'c, 's, 'p, 'd> DirtyReader<'b, 'c, 's, 'p, 'd> {
    pub(crate) fn read<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
        request: &TargetRequest,
    ) -> dirty_frame::Result<ReadTargets<'a, 'b, 'c, 's, 'p, 'd>> {
        self.pool
            .reserve_targets(owner, std::slice::from_ref(request))?
            .try_read()
    }
    pub(crate) fn observe_dirty(
        &self,
        owner: &mut Owner<'c, 's>,
        request: &TargetRequest,
    ) -> dirty_frame::Result<DirtyObservation> {
        let read = self.read(owner, request)?;
        let observed = read.observe(0);
        let cleanup = read.release_checked();
        match (observed, cleanup) {
            (Err(error), _) if error.effect != Effect::Unchanged => Err(error),
            (_, Err(error)) => Err(error),
            (result, Ok(())) => result,
        }
    }
}

/// Source-value admission is separate from the selected physical-stack observer.
fn fits<W: WriterNamespace>() -> bool {
    let owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    let control = size_of::<WriterControl<'_, '_>>();
    let reader = size_of::<PageFileReader<'_, W>>();
    let file = size_of::<W::File>();
    let (hot, cold) = shared_dirty_peaks();
    owner <= 32768
        && hot <= 196608
        && cold <= 196608
        && control <= 4096
        && file <= 16384
        && dirty_frame::fits()
        && dirty_allocation::typed_fits()
        && wal::durability_types_fit()
        && allocator_flush::fits::<W>()
        && size_of::<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>() <= 25136
        && size_of::<Result<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>>() <= 25152
        && budget::COMMON <= 7168
        && budget::HASH <= 2048
        && budget::TEMPORARIES <= 16384
        && budget::CONSTRUCTOR <= 16384
        && budget::PLANS <= 4096
        && owner
            .checked_add(reader)
            .and_then(|n| n.checked_add(file))
            .and_then(|n| n.checked_add(budget::CONSTRUCTOR))
            .is_some_and(|n| n <= 65536)
        && owner
            + control
            + budget::PLANS
            + budget::TEMPORARIES
            + dirty_frame::CALL_BYTES
            + 2048
            + wal::DURABILITY_CALL_BYTES
            <= 65536
}

/// New shared pool and trusted-budget fields also live in allocator-only bounds.
/// Their source-value constructor copies stay in the original dirty reservation.
pub(super) fn shared_dirty_peaks() -> (usize, usize) {
    let pool = dirty_frame::init_pool_delta_bytes();
    let control = dirty_allocation::base_delta();
    let retained = 60672 + pool + control;
    (
        retained + 34 * dirty_frame::CALL_BYTES,
        retained + 90112 + (5 * pool + 2048).max(3 * pool + 8 * control + 2048),
    )
}

/// Read-only diagnostic for a failed pre-effect resource admission. This does
/// not create storage or certify any ceiling; the selected resource test does.
#[cfg(test)]
#[test]
fn dirty_layout_diagnostic() {
    type W = crate::storage::wal_io::bootstrap::test_support::Writer;
    let layout = dirty_frame::test_support::layout();
    println!("DIRTY_LAYOUT_RAW {layout:?}");
    let owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    let control = size_of::<WriterControl<'_, '_>>();
    let reader = size_of::<PageFileReader<'_, W>>();
    let file = size_of::<<W as WriterNamespace>::File>();
    for (name, value) in [
        ("owner", owner),
        ("control", control),
        ("reader", reader),
        ("file", file),
        (
            "bound",
            size_of::<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>(),
        ),
        (
            "bound_result",
            size_of::<Result<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>>(),
        ),
        (
            "obligation",
            size_of::<dirty_allocation::InstallObligation<'_>>(),
        ),
        (
            "obligation_option",
            size_of::<Option<dirty_allocation::InstallObligation<'_>>>(),
        ),
        ("permit", size_of::<DirtyInstallPermit<'_, '_, '_>>()),
        ("common", budget::COMMON),
        ("hash", budget::HASH),
        ("temporaries", budget::TEMPORARIES),
        ("constructor", budget::CONSTRUCTOR),
        ("plans", budget::PLANS),
        (
            "readback_aggregate",
            owner + reader + file + budget::CONSTRUCTOR,
        ),
        (
            "writer_aggregate",
            owner + control + budget::PLANS + budget::TEMPORARIES + dirty_frame::CALL_BYTES + 2048,
        ),
    ] {
        println!("DIRTY_LAYOUT_SCALAR {name} {value}");
    }
    println!(
        "DIRTY_LAYOUT_FITS frame={} typed={} binding={}",
        dirty_frame::fits(),
        dirty_allocation::typed_fits(),
        fits::<W>()
    );
    println!("DIRTY_LAYOUT_DIAGNOSTIC complete");
}

pub(in crate::storage) use status_init::StatusLoadPermit;

pub(in crate::storage) use status_init::EmptyTransactionInstall;
pub(crate) use status_init::EmptyTransactionObservation;
