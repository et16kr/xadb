//! One retained physical claim. Durability precedes canonical source replay;
//! DWB retirement precedes the original conditional RAM finalization.
use super::super::super::flush_io::{FlushIo, RetiredBatch};
use super::*;
use crate::storage::{
    dirty_frame::{CapturedPage, FinalizedPage},
    wal_io::{EnsureDurable, FlushFinalizePermit},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionFlushPhase {
    Ready,
    WaitingWal,
    WaitingSource,
    RetiredPendingFinalize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionFlushStep {
    NoDirty,
    WaitingWal,
    WaitingSource,
    RetiredPendingFinalize,
    Complete {
        captured: FrameStamp,
        #[allow(
            private_interfaces,
            reason = "original storage-private finalize observation"
        )]
        finalized: FinalizedPage,
    },
}
pub(super) struct RetentionFlushControl<'c, 's, 'w> {
    pub(super) io: FlushIo<'s, 'w>,
    capture: Option<CapturedPage<'c, 's>>,
    durable: Option<DurableWal<'s>>,
    authority: Option<RetentionFlushAuthority<'c, 's>>,
    retired: Option<RetiredBatch<'s>>,
    pub(super) phase: RetentionFlushPhase,
    source: super::recovery::HistorySource<'s>,
}
impl<'c, 's, 'w> RetentionFlushControl<'c, 's, 'w> {
    pub(super) fn new<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, '_, W>,
        workspace: &'w mut [u8; super::super::FLUSH_BYTES],
    ) -> Self {
        Self {
            io: FlushIo::new(
                storage.plan.selected,
                workspace,
                &storage.binding,
                storage.session_fence(),
            ),
            capture: None,
            durable: None,
            authority: None,
            retired: None,
            phase: RetentionFlushPhase::Ready,
            source: super::recovery::HistorySource::Genesis,
        }
    }
    pub(super) fn from_recovery<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, '_, W>,
        workspace: &'w mut [u8; super::super::FLUSH_BYTES],
        selected: &super::super::super::plan4_recovery::SelectedRecoveryDwb,
        source: super::recovery::HistorySource<'s>,
    ) -> Self {
        Self {
            io: FlushIo::from_plan4_recovery(
                selected,
                workspace,
                &storage.binding,
                storage.session_fence(),
            ),
            capture: None,
            durable: None,
            authority: None,
            retired: None,
            phase: RetentionFlushPhase::Ready,
            source,
        }
    }
}
impl<'s, 'c, W: WriterNamespace> RetentionFlusher<'_, 's, 'c, '_, '_, '_, '_, W> {
    pub(crate) fn phase(&self) -> RetentionFlushPhase {
        self.control.phase
    }
    fn enter(&self, owner: &Owner<'c, 's>) -> Result<()> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Flush, e))
    }
    pub(crate) fn try_flush(
        &mut self,
        owner: &mut Owner<'c, 's>,
        target: &Plan4Target,
    ) -> Result<RetentionFlushStep> {
        self.start(owner, target.request)
    }
    pub(crate) fn try_flush_allocator(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<RetentionFlushStep> {
        self.start(
            owner,
            self.pool
                .target_request(0)
                .map_err(|e| frame(RetentionPhase::Flush, e))?,
        )
    }
    fn start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        request: TargetRequest,
    ) -> Result<RetentionFlushStep> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.enter(owner)?;
        if self.control.phase != RetentionFlushPhase::Ready {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::Busy));
        }
        if !matches!(
            request.kind,
            crate::storage::page::PageKind::Allocation
                | crate::storage::page::PageKind::RetentionRoot
                | crate::storage::page::PageKind::RetentionJobs
                | crate::storage::page::PageKind::TransactionStatus
                | crate::storage::page::PageKind::Undo
                | crate::storage::page::PageKind::CatalogLeaf
                | crate::storage::page::PageKind::TreeAnchor
                | crate::storage::page::PageKind::RowLeaf
        ) {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::Membership));
        }
        self.control
            .io
            .preflight::<W>()
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Flush,
                effect: e.effect,
                cause: RetentionCause::Flush(e),
            })?;
        self.pool
            .flush_preflight()
            .map_err(|e| frame(RetentionPhase::Flush, e))?;
        let image = self.control.io.image_mut().map_err(|e| RetentionError {
            phase: RetentionPhase::Flush,
            effect: e.effect,
            cause: RetentionCause::Flush(e),
        })?;
        let Some(capture) = self
            .pool
            .capture_flush(owner, request, image)
            .map_err(|e| frame(RetentionPhase::Flush, e))?
        else {
            return Ok(RetentionFlushStep::NoDirty);
        };
        self.control.capture = Some(capture);
        self.control.phase = RetentionFlushPhase::WaitingWal;
        #[cfg(test)]
        super::tests::after_capture(self.pool);
        self.advance(owner)
    }
    pub(crate) fn continue_flush(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<RetentionFlushStep> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.enter(owner)?;
        match self.control.phase {
            RetentionFlushPhase::Ready => {
                Err(unchanged(RetentionPhase::Flush, RetentionCause::WrongState))
            }
            RetentionFlushPhase::WaitingWal | RetentionFlushPhase::WaitingSource => {
                self.advance(owner)
            }
            RetentionFlushPhase::RetiredPendingFinalize => self.finalize(owner),
        }
    }
    fn advance(&mut self, owner: &mut Owner<'c, 's>) -> Result<RetentionFlushStep> {
        let storage = self.storage;
        let capture = self
            .control
            .capture
            .as_ref()
            .ok_or_else(|| fatal(storage, RetentionPhase::Flush, RetentionCause::Invariant))?;
        self.pool
            .validate_capture(capture)
            .map_err(|e| fatal(storage, RetentionPhase::Flush, RetentionCause::Frame(e)))?;
        if self.control.durable.is_none() {
            // No target pin, latch or append guard is held across this call.
            let durable = match storage
                .wal()
                .ensure_durable(&capture.requirement())
                .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?
            {
                EnsureDurable::AlreadySynced(value) | EnsureDurable::SyncedHere(value) => value,
                EnsureDurable::AwaitWalOwner => return Ok(RetentionFlushStep::WaitingWal),
            };
            self.control.durable = Some(durable);
        }
        self.control.phase = RetentionFlushPhase::WaitingSource;
        if self.control.authority.is_none() {
            let (image, tail) =
                self.control.io.retention_source_workspace().map_err(|e| {
                    fatal(storage, RetentionPhase::Source, RetentionCause::Flush(e))
                })?;
            // The original executor owns [0,164000); replay starts at196608.
            // This disjoint loan ends before prepare/execute borrows whole I/O.
            const GAP: usize = DWB_REPLAY_OFFSET - 164_000;
            let replay = tail
                .get_mut(GAP..GAP + RETENTION_REPLAY_BYTES)
                .ok_or_else(|| fatal(storage, RetentionPhase::Source, RetentionCause::Capacity))?;
            match history::prove_capture(
                storage,
                self.control.source,
                self.pool,
                capture,
                self.control
                    .durable
                    .as_ref()
                    .expect("retained original durable proof"),
                image,
                replay,
            ) {
                Ok(authority) => self.control.authority = Some(authority),
                Err(error)
                    if error.effect == Effect::Unchanged
                        && matches!(error.cause, RetentionCause::Busy) =>
                {
                    return Ok(RetentionFlushStep::WaitingSource);
                }
                Err(error) => return Err(fatal(storage, RetentionPhase::Source, error.cause)),
            }
        }
        let authority = self
            .control
            .authority
            .as_ref()
            .expect("sealed canonical source");
        if !authority.matching(capture) {
            return Err(fatal(
                storage,
                RetentionPhase::Flush,
                RetentionCause::Invariant,
            ));
        }
        self.control
            .io
            .prepare_retention(storage, capture, authority)
            .map_err(|e| fatal(storage, RetentionPhase::Flush, RetentionCause::Flush(e)))?;
        let retired = self
            .control
            .io
            .execute_retention(
                storage,
                capture,
                authority,
                self.control.durable.as_ref().expect("durable"),
            )
            .map_err(|e| fatal(storage, RetentionPhase::Flush, RetentionCause::Flush(e)))?;
        self.control.retired = Some(retired);
        self.control.phase = RetentionFlushPhase::RetiredPendingFinalize;
        self.finalize(owner)
    }
    fn finalize(&mut self, owner: &mut Owner<'c, 's>) -> Result<RetentionFlushStep> {
        let storage = self.storage;
        let capture =
            self.control.capture.as_ref().ok_or_else(|| {
                fatal(storage, RetentionPhase::Finalize, RetentionCause::Invariant)
            })?;
        let retired =
            self.control.retired.as_ref().ok_or_else(|| {
                fatal(storage, RetentionPhase::Finalize, RetentionCause::Invariant)
            })?;
        self.pool
            .validate_capture(capture)
            .map_err(|e| fatal(storage, RetentionPhase::Finalize, RetentionCause::Frame(e)))?;
        let permit = FlushFinalizePermit::from_retired(capture, retired)
            .ok_or_else(|| fatal(storage, RetentionPhase::Finalize, RetentionCause::Invariant))?;
        let transition = self
            .control
            .io
            .ready_transition(retired)
            .map_err(|e| fatal(storage, RetentionPhase::Finalize, RetentionCause::Flush(e)))?;
        let Some(finalized) = self
            .pool
            .finalize_flush(owner, capture, &permit)
            .map_err(|e| fatal(storage, RetentionPhase::Finalize, RetentionCause::Frame(e)))?
        else {
            return Ok(RetentionFlushStep::RetiredPendingFinalize);
        };
        let captured = capture.stamp();
        transition.complete();
        self.control.capture = None;
        self.control.retired = None;
        self.control.authority = None;
        self.control.durable = None;
        self.control.phase = RetentionFlushPhase::Ready;
        Ok(RetentionFlushStep::Complete {
            captured,
            finalized,
        })
    }
}
