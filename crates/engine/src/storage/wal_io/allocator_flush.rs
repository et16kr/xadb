//! Actual allocator1 copy→WAL→DWB→home→retirement→conditional RAM finalize.
//! Progress stays inside the bound owner; observations cannot resume another owner.

use super::{
    MutableStorageOwner,
    dirty_allocation::WriterControl,
    flush_io::{self, FlushIo, RetiredBatch},
};
use crate::{
    storage::{
        dirty_frame::{self, CapturedPage, FinalizedPage, FrameStamp, SharedPool, TargetRequest},
        page::{PageIdentity, PageKind},
        wal_io::{self as wal, EnsureDurable},
    },
    transaction::concurrency::Owner,
};
use std::mem::size_of;
use xadb_foundation::{error::Effect, namespace::WriterNamespace};

const CAPTURE_FINALIZE_BYTES: usize = 8192;
const IO_NESTED_BYTES: usize = 8192;

#[cfg(all(test, target_os = "linux"))]
#[path = "allocator_flush_native_tests.rs"]
mod native_tests;
#[cfg(test)]
#[path = "allocator_flush_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Ready,
    Captured,
    WaitingWal,
    RetiredPendingFinalize,
}

#[derive(Debug)]
pub(super) enum Cause {
    Busy,
    NoPending,
    Capacity,
    Invariant,
    Storage(super::MutableStorageError),
    Frame(dirty_frame::DirtyError),
    Wal(wal::WalIoError),
    Io(flush_io::FlushIoError),
}

/// The original lower error is retained. A failed attempt never resets its phase.
#[derive(Debug)]
pub(crate) struct AllocatorFlushError {
    pub(super) phase: Phase,
    pub(super) cause: Cause,
    pub(super) effect: Effect,
}
type Result<T> = std::result::Result<T, AllocatorFlushError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FlushObservation {
    pub(super) captured: FrameStamp,
    pub(super) finalized: FinalizedPage,
    pub(super) io: flush_io::IoObservation,
}

/// Waiting states retain the exact old image and address claim, with no guards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// Fixed source-value/result caps below admit this diagnostic payload before
// effects. Boxing would introduce an unreserved allocation into completion.
#[allow(clippy::large_enum_variant)]
pub(crate) enum FlushProgress {
    NoDirty,
    WaitingWal,
    RetiredPendingFinalize,
    Complete(FlushObservation),
}

/// Retained progress only. Absence of a copied image does not prove claim absence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FlushSnapshot {
    pub(super) phase: Phase,
    pub(super) captured: Option<FrameStamp>,
    pub(super) io: flush_io::IoObservation,
    pub(super) fenced: bool,
}

pub(super) struct FlushControl<'c, 's, 'w> {
    io: FlushIo<'s, 'w>,
    identity: PageIdentity,
    capture: Option<CapturedPage<'c, 's>>,
    retired: Option<RetiredBatch<'s>>,
    phase: Phase,
}
impl<'c, 's, 'w> FlushControl<'c, 's, 'w> {
    pub(super) fn new(io: FlushIo<'s, 'w>, identity: PageIdentity) -> Self {
        Self {
            io,
            identity,
            capture: None,
            retired: None,
            phase: Phase::Ready,
        }
    }
}

/// A temporary exclusive view. Drop leaves its control and obligations in Bound.
pub(crate) struct AllocatorFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    pub(super) control: &'b mut FlushControl<'c, 's, 'w>,
    pub(super) pool: &'b SharedPool<'c, 's, 'p, 'd>,
    pub(super) storage: &'s MutableStorageOwner<'m, 'w, W>,
}

/// Only this actual orchestration constructs the borrowed retirement capability.
pub(in crate::storage) struct FlushFinalizePermit<'a, 'c, 's> {
    capture: &'a CapturedPage<'c, 's>,
    retired: &'a RetiredBatch<'s>,
}
impl<'c, 's> FlushFinalizePermit<'_, 'c, 's> {
    pub(in crate::storage) fn matches(&self, capture: &CapturedPage<'c, 's>) -> bool {
        std::ptr::eq(self.capture, capture) && self.retired.matches(capture)
    }
}

impl<'s, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> AllocatorFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W> {
    pub(crate) fn snapshot(&self) -> FlushSnapshot {
        FlushSnapshot {
            phase: self.control.phase,
            captured: self.control.capture.as_ref().map(CapturedPage::stamp),
            io: self.control.io.snapshot(),
            fenced: self.storage.session_fence().is_fenced(),
        }
    }
    fn error(&self, cause: Cause, effect: Effect) -> AllocatorFlushError {
        AllocatorFlushError {
            phase: self.control.phase,
            cause,
            effect,
        }
    }
    fn fatal(&self, cause: Cause) -> AllocatorFlushError {
        self.storage.session_fence().fence();
        self.error(cause, Effect::CallerMustResolve)
    }
    fn frame_error(&self, error: dirty_frame::DirtyError) -> AllocatorFlushError {
        if error.effect == Effect::Unchanged {
            self.error(Cause::Frame(error), Effect::Unchanged)
        } else {
            self.fatal(Cause::Frame(error))
        }
    }
    fn enter(&self, owner: &Owner<'c, 's>) -> Result<()> {
        // Only atomic/poison/metadata health; the Retired continuation does no I/O.
        self.storage
            .health()
            .map_err(|error| self.fatal(Cause::Storage(error)))?;
        self.pool
            .check_flusher(owner)
            .map_err(|error| self.frame_error(error))
    }

    /// No caller image, LSN, slot, home path or flush authority is accepted.
    /// Busy before capture is Unchanged; later failures fence retained ownership.
    pub(crate) fn try_flush_one(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.enter(owner)?;
        if self.control.phase != Phase::Ready {
            return Err(self.error(Cause::Busy, Effect::Unchanged));
        }
        if self.control.capture.is_some() || self.control.retired.is_some() {
            return Err(self.fatal(Cause::Invariant));
        }
        if !fits::<W>() {
            return Err(self.error(Cause::Capacity, Effect::Unchanged));
        }
        self.control.io.preflight::<W>().map_err(|error| {
            if error.effect == Effect::Unchanged {
                self.error(Cause::Io(error), Effect::Unchanged)
            } else {
                self.fatal(Cause::Io(error))
            }
        })?;
        self.pool
            .flush_preflight()
            .map_err(|error| self.frame_error(error))?;
        let request = TargetRequest {
            identity: self.control.identity,
            kind: PageKind::Allocation,
            level: 0,
        };
        let destination = self
            .control
            .io
            .image_mut()
            .map_err(|error| AllocatorFlushError {
                phase: Phase::Ready,
                effect: error.effect,
                cause: Cause::Io(error),
            })?;
        let capture = self
            .pool
            .capture_flush(owner, request, destination)
            .map_err(|error| self.frame_error(error))?;
        let Some(capture) = capture else {
            return Ok(FlushProgress::NoDirty);
        };
        // No fallible call between coherent capture and retained obligation.
        self.control.capture = Some(capture);
        self.control.phase = Phase::Captured;
        let captured = self.control.capture.as_ref().expect("retained capture");
        self.pool
            .validate_capture(captured)
            .map_err(|error| self.fatal(Cause::Frame(error)))?;
        self.control
            .io
            .prepare(storage, captured)
            .map_err(|error| self.fatal(Cause::Io(error)))?;
        self.advance(owner)
    }

    /// Continue the same attempt. A retired attempt performs RAM-only work.
    pub(crate) fn continue_flush(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.enter(owner)?;
        match self.control.phase {
            Phase::Ready => Err(self.error(Cause::NoPending, Effect::Unchanged)),
            Phase::WaitingWal => self.advance(owner),
            Phase::RetiredPendingFinalize => self.finalize(owner),
            Phase::Captured => Err(self.fatal(Cause::Invariant)),
        }
    }

    fn advance(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        let captured = self
            .control
            .capture
            .as_ref()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        self.pool
            .validate_capture(captured)
            .map_err(|error| self.fatal(Cause::Frame(error)))?;
        let proof = match self
            .storage
            .wal()
            .ensure_durable(&captured.requirement())
            .map_err(|error| self.fatal(Cause::Wal(error)))?
        {
            EnsureDurable::AlreadySynced(proof) | EnsureDurable::SyncedHere(proof) => proof,
            EnsureDurable::AwaitWalOwner => {
                self.control.phase = Phase::WaitingWal;
                return Ok(FlushProgress::WaitingWal);
            }
        };
        let retired = self
            .control
            .io
            .execute(self.storage, captured, &proof)
            .map_err(|error| self.fatal(Cause::Io(error)))?;
        // Durable retirement remains retained even if all following RAM work fails.
        self.control.retired = Some(retired);
        self.control.phase = Phase::RetiredPendingFinalize;
        self.finalize(owner)
    }

    fn finalize(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        let captured = self
            .control
            .capture
            .as_ref()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        let retired = self
            .control
            .retired
            .as_ref()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        let transition = match self.control.io.ready_transition(retired) {
            Ok(transition) => transition,
            Err(error) => {
                self.storage.session_fence().fence();
                return Err(AllocatorFlushError {
                    phase: self.control.phase,
                    effect: Effect::CallerMustResolve,
                    cause: Cause::Io(error),
                });
            }
        };
        let permit = FlushFinalizePermit {
            capture: captured,
            retired,
        };
        let finalized = match self.pool.finalize_flush(owner, captured, &permit) {
            Ok(Some(finalized)) => finalized,
            Ok(None) => return Ok(FlushProgress::RetiredPendingFinalize),
            Err(error) => {
                self.storage.session_fence().fence();
                return Err(AllocatorFlushError {
                    phase: self.control.phase,
                    effect: Effect::CallerMustResolve,
                    cause: Cause::Frame(error),
                });
            }
        };
        let captured_stamp = captured.stamp();
        // All fallible work, including claim return, precedes this infallible commit.
        transition.complete();
        self.control.capture = None;
        self.control.retired = None;
        self.control.phase = Phase::Ready;
        Ok(FlushProgress::Complete(FlushObservation {
            captured: captured_stamp,
            finalized,
            io: self.control.io.snapshot(),
        }))
    }
}

pub(super) fn fits<W: WriterNamespace>() -> bool {
    size_of::<FlushControl<'_, '_, '_>>() <= 4096
        && size_of::<FlushFinalizePermit<'_, '_, '_>>() <= 32
        && size_of::<AllocatorFlushError>() <= 768
        && size_of::<FlushProgress>() <= 768
        && size_of::<Result<FlushProgress>>() <= 1024
        && size_of::<FlushSnapshot>() <= 512
        && flush_io::fits::<W>()
        && wal::durability_types_fit()
        && flush_metadata_fits()
        && size_of::<MutableStorageOwner<'_, '_, W>>()
            .checked_add(size_of::<WriterControl<'_, '_>>())
            .and_then(|n| n.checked_add(size_of::<FlushControl<'_, '_, '_>>()))
            .and_then(|n| {
                size_of::<W::File>()
                    .checked_mul(4)
                    .and_then(|files| n.checked_add(files))
            })
            .and_then(|n| n.checked_add(wal::DURABILITY_CALL_BYTES))
            .and_then(|n| n.checked_add(IO_NESTED_BYTES))
            .and_then(|n| n.checked_add(CAPTURE_FINALIZE_BYTES))
            .is_some_and(|n| n <= 65536)
}

/// Explicit new 64KiB metadata reservation, separate from the old dirty192KiB.
/// The upper 1024 covers new owner/Condvar/workspace-reference carriers.
/// Source-value copies are conservative and do not claim physical stack bytes.
pub(super) fn flush_metadata_peaks() -> (usize, usize) {
    let registry = dirty_frame::flush_registry_bytes();
    let control = size_of::<FlushControl<'_, '_, '_>>();
    let retained = registry + control + 1024;
    // 34 concurrent original admissions; each new receipt family <=1024.
    // One dedicated capture/finalizer, WAL completion chain, and I/O chain.
    let hot = retained
        + 34 * 1024
        + CAPTURE_FINALIZE_BYTES
        + wal::DURABILITY_CALL_BYTES
        + IO_NESTED_BYTES;
    // Pool construction completes before the Bound/FlushControl expression.
    // Count initialized source values per phase, never moved-from stack slots.
    let pool_phase = 5 * registry + 8192;
    let bound_phase = 3 * registry + 8 * control + 8192;
    let cold = retained + pool_phase.max(bound_phase);
    (hot, cold)
}
fn flush_metadata_fits() -> bool {
    let (hot, cold) = flush_metadata_peaks();
    hot <= 65536 && cold <= 65536
}
