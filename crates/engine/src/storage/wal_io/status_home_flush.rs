//! One actual INIT-published Status page and its allocator, sequential batch1.
//! The whole original bound moves into this owner; failed admission returns it.

use super::super::{
    BOUND, MutableStorageError, MutableStorageOwner,
    allocator_flush::{FlushFinalizePermit, FlushObservation, FlushProgress, FlushSnapshot, Phase},
    flush_io::{self, FlushIo, RetiredBatch},
};
use super::{
    BoundStatusBootstrap, SemanticError, StatusBootstrapWriter, StatusControl, StatusReader,
    operation,
};
use crate::{
    storage::{
        dirty_frame::{self, CapturedPage, SharedPool, TargetRequest},
        page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
        session_fence::SessionFence,
        wal_io::{self as wal, EnsureDurable, bootstrap::FixedSystemRole},
    },
    transaction::concurrency::{Owner, PageConcurrency},
};
use foundation::{error::Effect, namespace::WriterNamespace};
use std::{
    marker::PhantomData,
    mem::size_of,
    sync::{
        TryLockError,
        atomic::{AtomicU8, Ordering},
    },
};

#[cfg(test)]
#[path = "status_home_flush/tests.rs"]
mod tests;

#[path = "mixed_system_owner.rs"]
pub(super) mod mixed;
#[path = "operational_status.rs"]
mod operational;
#[path = "resident_status.rs"]
pub(super) mod resident;
pub(crate) use operational::{
    IntoOperationalStatusError, OperationalStatus, OperationalStatusReader, OperationalStatusWriter,
};
pub(crate) use resident::{
    EMPTY_TRANSACTION_BANK_BYTES, EmptyTransactionSnapshot, EmptyTransactions,
    IntoEmptyTransactionsError, ReadStatusSet, ResidentReadAdmission, ResidentStatus,
    ResidentStatusBinding, ResidentStatusReader,
};

#[cfg(test)]
pub(super) fn before_init_force(binding: &AtomicU8) {
    tests::before_init_force(binding);
}

#[derive(Debug)]
pub(super) enum Cause {
    Busy,
    NoPending,
    NotInitialized,
    Capacity,
    BindingMismatch,
    Invariant,
    Storage(MutableStorageError),
    Allocation(super::super::dirty_allocation::Cause),
    Frame(dirty_frame::DirtyError),
    Wal(wal::WalIoError),
    Io(flush_io::FlushIoError),
}
#[derive(Debug)]
pub(crate) struct StatusFlushError {
    pub(super) phase: Phase,
    pub(super) cause: Cause,
    pub(super) effect: Effect,
}
type Result<T> = std::result::Result<T, StatusFlushError>;

/// A healthy failed conversion preserves the exact original owner and loans.
/// Fatal conversion cannot return an owner advertised as usable.
pub(crate) enum IntoFlushError<B> {
    Before { error: StatusFlushError, bound: B },
    Fatal(StatusFlushError),
}

pub(crate) struct BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    // Field drop order fences through the original bound before flush release.
    inner: BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    flush: StatusFlushControl<'c, 's, 'w>,
}
struct StatusFlushControl<'c, 's, 'w> {
    io: FlushIo<'s, 'w>,
    capture: Option<CapturedPage<'c, 's>>,
    retired: Option<RetiredBatch<'s>>,
    authority: Option<StatusFlushAuthority<'c, 's>>,
    phase: Phase,
}
impl<'c, 's, 'w> StatusFlushControl<'c, 's, 'w> {
    fn new(io: FlushIo<'s, 'w>) -> Self {
        Self {
            io,
            capture: None,
            retired: None,
            authority: None,
            phase: Phase::Ready,
        }
    }
}

/// Constructed solely from this owner's immutable role and actual publication.
/// Every reference points outside the movable bound/control. No raw caller
/// identity, image, LSN or diagnostic registration constructs this capability.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PayloadClass {
    Status,
    MixedStatus,
    MixedUndo,
}

pub(in crate::storage) struct StatusFlushAuthority<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    request: TargetRequest,
    allocator: PageIdentity,
    history: Option<resident::History>,
    class: PayloadClass,
}
#[allow(dead_code)] // Source-value baseline only; never constructed.
struct BeforeClassAuthority<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    request: TargetRequest,
    allocator: PageIdentity,
    history: Option<resident::History>,
}
#[allow(dead_code)] // Original enclosing control, only for source-value layout.
struct BeforeClassControl<'c, 's, 'w> {
    io: FlushIo<'s, 'w>,
    capture: Option<CapturedPage<'c, 's>>,
    retired: Option<RetiredBatch<'s>>,
    authority: Option<BeforeClassAuthority<'c, 's>>,
    phase: Phase,
}
fn class_retained_bytes() -> usize {
    size_of::<Option<StatusFlushAuthority<'_, '_>>>()
        .saturating_sub(size_of::<Option<BeforeClassAuthority<'_, '_>>>())
        .max(
            size_of::<StatusFlushControl<'_, '_, '_>>()
                .saturating_sub(size_of::<BeforeClassControl<'_, '_, '_>>()),
        )
}
impl StatusFlushAuthority<'_, '_> {
    pub(in crate::storage) fn matching(&self, capture: &CapturedPage<'_, '_>) -> bool {
        capture.uses_core(self.core)
            && std::ptr::eq(self.binding, capture.binding())
            && capture.requirement().uses_fence(self.fence)
            && self.accepts_kind(capture.request().kind)
            && capture.request() == self.request
            && capture.stamp().identity == self.request.identity
    }
    pub(in crate::storage) fn mixed(&self) -> bool {
        self.class != PayloadClass::Status
    }
    pub(in crate::storage) fn accepts_kind(&self, kind: PageKind) -> bool {
        kind == if self.class == PayloadClass::MixedUndo {
            PageKind::Undo
        } else {
            PageKind::TransactionStatus
        }
    }
    pub(in crate::storage) fn request(&self) -> &TargetRequest {
        &self.request
    }
    pub(in crate::storage) fn allocator(&self) -> &PageIdentity {
        &self.allocator
    }
    pub(in crate::storage) fn image_lsn(
        &self,
        bytes: &[u8; PAGE_BYTES],
    ) -> std::result::Result<PageLsn, SemanticError> {
        if self.class == PayloadClass::MixedUndo {
            return crate::storage::undo_record::initial_undo_page_lsn(
                bytes,
                self.request.identity,
            )
            .map_err(|_| SemanticError::PageMismatch);
        }
        match &self.history {
            None => operation::status_lsn(bytes, &self.request.identity),
            Some(history) if *history.current() == foundation::codec::content_digest(bytes) => {
                operation::status_envelope(bytes, &self.request.identity)
            }
            Some(_) => Err(SemanticError::PageMismatch),
        }
    }
    pub(in crate::storage) fn accepts_home(&self, home: &[u8], image: &[u8]) -> bool {
        match &self.history {
            Some(history) => history.contains(home),
            None => home == image,
        }
    }
    pub(in crate::storage) fn envelope_lsn(
        &self,
        bytes: &[u8; PAGE_BYTES],
    ) -> std::result::Result<PageLsn, SemanticError> {
        if self.class == PayloadClass::MixedUndo {
            crate::storage::undo_record::initial_undo_page_lsn(bytes, self.request.identity)
                .map_err(|_| SemanticError::PageMismatch)
        } else {
            operation::status_envelope(bytes, &self.request.identity)
        }
    }
}

fn error(cause: Cause, effect: Effect) -> StatusFlushError {
    StatusFlushError {
        phase: Phase::Ready,
        cause,
        effect,
    }
}
fn fatal(
    storage: &MutableStorageOwner<'_, '_, impl WriterNamespace>,
    cause: Cause,
) -> StatusFlushError {
    storage.session_fence().fence();
    error(cause, Effect::CallerMustResolve)
}

// The fixed error/return carriers must fit even when runtime generic admission
// fails. A bad future fixed ABI is a compile-time failure, not an oversized
// Capacity return. W appears by borrowed origin in both bound types.
struct AdapterLayout<W>(PhantomData<W>);
impl<W: WriterNamespace> AdapterLayout<W> {
    const CHECK: () = {
        let b = size_of::<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>();
        let f = size_of::<StatusFlushControl<'_, '_, '_>>();
        let n = size_of::<BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>>();
        let e = size_of::<IntoFlushError<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>>();
        let q = size_of::<
            std::result::Result<
                BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>,
                IntoFlushError<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>,
            >,
        >();
        assert!(b + 8192 <= 65536);
        assert!(b + 4 * f + 8192 <= 65536);
        assert!(b + e + 8192 <= 65536 && e + q + 8192 <= 65536);
        assert!(b + n + 8192 <= 65536 && n + q + 8192 <= 65536);
    };
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// One nonblocking workspace take after all admission checks. No I/O.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure must return the exact inline bound without an unreserved heap allocation"
    )]
    pub(crate) fn into_with_flush(
        self,
    ) -> std::result::Result<
        BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        IntoFlushError<Self>,
    > {
        self.into_fixed_with_flush(false)
    }
    #[allow(
        clippy::result_large_err,
        reason = "retain the exact inline owner on unchanged refusal"
    )]
    fn into_fixed_with_flush(
        self,
        mixed: bool,
    ) -> std::result::Result<
        BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        IntoFlushError<Self>,
    > {
        let () = AdapterLayout::<W>::CHECK;
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        if let Err(source) = storage.health() {
            return Err(IntoFlushError::Fatal(fatal(
                storage,
                Cause::Storage(source),
            )));
        }
        if storage.binding.load(Ordering::Acquire) != BOUND
            || !self
                .pool
                .uses_storage_origin(&storage.binding, storage.session_fence())
            || !self.pool.bound_core().uses_fence(storage.session_fence())
            || !self.control.role.uses_fence(storage.session_fence())
            || self.control.role.namespace() != storage.profile().spec().namespace
        {
            return Err(IntoFlushError::Fatal(fatal(
                storage,
                Cause::BindingMismatch,
            )));
        }
        if self.control.role.mixed() != mixed {
            return Err(IntoFlushError::Before {
                error: error(Cause::BindingMismatch, Effect::Unchanged),
                bound: self,
            });
        }
        if !fits::<W>() {
            return Err(IntoFlushError::Before {
                error: error(Cause::Capacity, Effect::Unchanged),
                bound: self,
            });
        }
        #[cfg(test)]
        tests::adapter_pre_take(&storage.binding);
        let workspace = {
            let mut loan = match storage.workspace.try_lock() {
                Ok(loan) => loan,
                Err(TryLockError::WouldBlock) => {
                    return Err(IntoFlushError::Before {
                        error: error(Cause::Busy, Effect::Unchanged),
                        bound: self,
                    });
                }
                Err(TryLockError::Poisoned(_)) => {
                    return Err(IntoFlushError::Fatal(fatal(
                        storage,
                        Cause::Storage(storage.fatal(super::super::Cause::Poisoned)),
                    )));
                }
            };
            if let Err(source) = storage.health() {
                return Err(IntoFlushError::Fatal(fatal(
                    storage,
                    Cause::Storage(source),
                )));
            }
            match loan.take() {
                Some(workspace) => workspace,
                None => return Err(IntoFlushError::Fatal(fatal(storage, Cause::Invariant))),
            }
        };
        // Infallible assignments only after take; no callbacks or panic hooks.
        let flush = StatusFlushControl::new(FlushIo::new(
            storage.plan.selected,
            workspace,
            &storage.binding,
            storage.session_fence(),
        ));
        Ok(BoundStatusWithFlush { inner: self, flush })
    }
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn split_with_flush<'b>(
        &'b mut self,
    ) -> (
        StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        StatusReader<'b, 'c, 's, 'p, 'd>,
        StatusFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        assert!(
            !self.inner.control.role.mixed(),
            "Status facade is plan2 only"
        );
        self.split_fixed_with_flush()
    }
    fn split_fixed_with_flush<'b>(
        &'b mut self,
    ) -> (
        StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        StatusReader<'b, 'c, 's, 'p, 'd>,
        StatusFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        let StatusControl {
            base,
            status_after,
            role,
            installation,
        } = &mut self.inner.control;
        (
            StatusBootstrapWriter {
                base,
                status_after,
                role,
                installation,
                pool: &self.inner.pool,
                storage: self.inner.storage,
            },
            StatusReader {
                pool: &self.inner.pool,
                role,
                profile: &self.inner.storage.core.plan.profile,
                fence: self.inner.storage.session_fence(),
            },
            StatusFlusher {
                control: &mut self.flush,
                pool: &self.inner.pool,
                storage: self.inner.storage,
                role,
            },
        )
    }
}

pub(crate) struct StatusFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    control: &'b mut StatusFlushControl<'c, 's, 'w>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    role: &'b FixedSystemRole<'s>,
}
impl<'s, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> StatusFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W> {
    pub(crate) fn snapshot(&self) -> FlushSnapshot {
        FlushSnapshot {
            phase: self.control.phase,
            captured: self.control.capture.as_ref().map(CapturedPage::stamp),
            io: self.control.io.snapshot(),
            fenced: self.storage.session_fence().is_fenced(),
        }
    }
    fn error(&self, cause: Cause, effect: Effect) -> StatusFlushError {
        StatusFlushError {
            phase: self.control.phase,
            cause,
            effect,
        }
    }
    fn fatal(&self, cause: Cause) -> StatusFlushError {
        self.storage.session_fence().fence();
        self.error(cause, Effect::CallerMustResolve)
    }
    fn frame_error(&self, source: dirty_frame::DirtyError) -> StatusFlushError {
        if source.effect == Effect::Unchanged {
            self.error(Cause::Frame(source), Effect::Unchanged)
        } else {
            self.fatal(Cause::Frame(source))
        }
    }
    fn enter(&self, owner: &Owner<'c, 's>) -> Result<()> {
        self.storage
            .health()
            .map_err(|e| self.fatal(Cause::Storage(e)))?;
        self.pool
            .check_flusher(owner)
            .map_err(|e| self.frame_error(e))
    }
    fn status_authority(&self) -> Result<StatusFlushAuthority<'c, 's>> {
        self.status_authority_fixed(None, operation::mixed::Kind::Status)
    }
    fn status_authority_at(&self, page: Option<PageRef>) -> Result<StatusFlushAuthority<'c, 's>> {
        self.status_authority_fixed(page, operation::mixed::Kind::Status)
    }
    fn status_authority_fixed(
        &self,
        page: Option<PageRef>,
        kind: operation::mixed::Kind,
    ) -> Result<StatusFlushAuthority<'c, 's>> {
        let request = match page {
            None => self.pool.initialized_request(),
            Some(page) => self.pool.resident_request(
                operation::fixed::request(self.role, page, kind)
                    .map_err(|_| self.error(Cause::BindingMismatch, Effect::Unchanged))?,
            ),
        }
        .map_err(|e| self.frame_error(e))?
        .ok_or_else(|| self.error(Cause::NotInitialized, Effect::Unchanged))?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| self.frame_error(e))?;
        if !self.role.uses_fence(self.storage.session_fence())
            || !self
                .pool
                .uses_storage_origin(&self.storage.binding, self.storage.session_fence())
            || self.role.namespace() != self.storage.profile().spec().namespace
            || operation::fixed::request(self.role, request.identity.page, kind).ok()
                != Some(request)
            || allocator.kind != PageKind::Allocation
            || allocator.level != 0
            || allocator.identity.page.page_no != 0
            || if self
                .pool
                .operational_payloads()
                .map_err(|error| self.frame_error(error))?
            {
                !(1..64).contains(&request.identity.page.page_no)
            } else {
                request.identity.page.page_no != 1
            }
            || request.identity.page.file != allocator.identity.page.file
            || request.identity.database != allocator.identity.database
        {
            return Err(self.fatal(Cause::BindingMismatch));
        }
        Ok(StatusFlushAuthority {
            core: self.pool.bound_core(),
            binding: &self.storage.binding,
            fence: self.storage.session_fence(),
            request,
            allocator: allocator.identity,
            history: None,
            class: if self.role.mixed() {
                if kind == operation::mixed::Kind::UndoRoot {
                    PayloadClass::MixedUndo
                } else {
                    PayloadClass::MixedStatus
                }
            } else {
                PayloadClass::Status
            },
        })
    }
    pub(crate) fn try_flush_allocator(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<FlushProgress> {
        self.try_flush(owner, false, None, None)
    }
    pub(crate) fn try_flush_status(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        self.try_flush(owner, true, None, None)
    }
    pub(crate) fn try_flush_page(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
    ) -> Result<FlushProgress> {
        self.try_flush(owner, true, Some(page), None)
    }
    fn try_flush_history(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
        history: resident::History,
    ) -> Result<FlushProgress> {
        self.try_flush(owner, true, Some(page), Some(history))
    }
    fn try_flush(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: bool,
        page: Option<PageRef>,
        history: Option<resident::History>,
    ) -> Result<FlushProgress> {
        self.enter(owner)?;
        if self.role.mixed() {
            return Err(self.error(Cause::BindingMismatch, Effect::Unchanged));
        }
        self.try_flush_fixed(owner, status, page, history, operation::mixed::Kind::Status)
    }
    fn try_flush_fixed(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: bool,
        page: Option<PageRef>,
        history: Option<resident::History>,
        kind: operation::mixed::Kind,
    ) -> Result<FlushProgress> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.enter(owner)?;
        if self.role.mixed() && history.is_some() {
            return Err(self.fatal(Cause::Invariant));
        }
        if self.control.phase != Phase::Ready {
            return Err(self.error(Cause::Busy, Effect::Unchanged));
        }
        if self.control.capture.is_some()
            || self.control.retired.is_some()
            || self.control.authority.is_some()
        {
            return Err(self.fatal(Cause::Invariant));
        }
        // Publication lookup precedes headroom/DPT/content. C5 may hold both
        // DPT and content while this lookup returns NotInitialized immediately.
        let mut authority = if status {
            Some(self.status_authority_fixed(page, kind)?)
        } else {
            None
        };
        if let Some(history) = history {
            authority
                .as_mut()
                .ok_or_else(|| self.fatal(Cause::Invariant))?
                .history = Some(history);
        }
        if !fits::<W>() {
            return Err(self.error(Cause::Capacity, Effect::Unchanged));
        }
        self.control.io.preflight::<W>().map_err(|e| {
            if e.effect == Effect::Unchanged {
                self.error(Cause::Io(e), Effect::Unchanged)
            } else {
                self.fatal(Cause::Io(e))
            }
        })?;
        self.pool
            .flush_preflight()
            .map_err(|e| self.frame_error(e))?;
        let request = match &authority {
            Some(authority) => *authority.request(),
            None => self
                .pool
                .target_request(0)
                .map_err(|e| self.frame_error(e))?,
        };
        let destination = self.control.io.image_mut().map_err(|e| StatusFlushError {
            phase: Phase::Ready,
            effect: e.effect,
            cause: Cause::Io(e),
        })?;
        let capture = self
            .pool
            .capture_flush(owner, request, destination)
            .map_err(|e| self.frame_error(e))?;
        let Some(capture) = capture else {
            return Ok(FlushProgress::NoDirty);
        };
        self.control.capture = Some(capture);
        self.control.authority = authority;
        self.control.phase = Phase::Captured;
        let captured = self.control.capture.as_ref().expect("retained capture");
        self.pool
            .validate_capture(captured)
            .map_err(|e| self.fatal(Cause::Frame(e)))?;
        let prepared = if let Some(authority) = self.control.authority.as_ref() {
            self.control.io.prepare_status(storage, captured, authority)
        } else if captured.request().kind == PageKind::Allocation {
            self.control.io.prepare(storage, captured)
        } else {
            return Err(self.fatal(Cause::Invariant));
        };
        prepared.map_err(|e| self.fatal(Cause::Io(e)))?;
        self.advance(owner)
    }
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
    fn validate_retained(&self) -> Result<()> {
        let captured = self
            .control
            .capture
            .as_ref()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        self.pool
            .validate_capture(captured)
            .map_err(|e| self.fatal(Cause::Frame(e)))?;
        match self.control.authority.as_ref() {
            Some(authority) if authority.matching(captured) => Ok(()),
            None if captured.request().kind == PageKind::Allocation
                && captured.request().level == 0 =>
            {
                Ok(())
            }
            _ => Err(self.fatal(Cause::BindingMismatch)),
        }
    }
    fn advance(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        self.validate_retained()?;
        let captured = self.control.capture.as_ref().expect("validated capture");
        let proof = match self
            .storage
            .wal()
            .ensure_durable(&captured.requirement())
            .map_err(|e| self.fatal(Cause::Wal(e)))?
        {
            EnsureDurable::AlreadySynced(proof) | EnsureDurable::SyncedHere(proof) => proof,
            EnsureDurable::AwaitWalOwner => {
                self.control.phase = Phase::WaitingWal;
                return Ok(FlushProgress::WaitingWal);
            }
        };
        let retired = if let Some(authority) = self.control.authority.as_ref() {
            self.control
                .io
                .execute_status(self.storage, captured, authority, &proof)
        } else {
            self.control.io.execute(self.storage, captured, &proof)
        }
        .map_err(|e| self.fatal(Cause::Io(e)))?;
        self.control.retired = Some(retired);
        self.control.phase = Phase::RetiredPendingFinalize;
        self.finalize(owner)
    }
    fn finalize(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        self.validate_retained()?;
        let captured = self.control.capture.as_ref().expect("validated capture");
        let retired = self
            .control
            .retired
            .as_ref()
            .ok_or_else(|| self.fatal(Cause::Invariant))?;
        let permit = FlushFinalizePermit::from_retired(captured, retired)
            .ok_or_else(|| self.fatal(Cause::BindingMismatch))?;
        let transition = match self.control.io.ready_transition(retired) {
            Ok(transition) => transition,
            Err(e) => {
                self.storage.session_fence().fence();
                return Err(StatusFlushError {
                    phase: self.control.phase,
                    effect: Effect::CallerMustResolve,
                    cause: Cause::Io(e),
                });
            }
        };
        let finalized = match self.pool.finalize_flush(owner, captured, &permit) {
            Ok(Some(finalized)) => finalized,
            Ok(None) => return Ok(FlushProgress::RetiredPendingFinalize),
            Err(e) => {
                self.storage.session_fence().fence();
                return Err(StatusFlushError {
                    phase: self.control.phase,
                    effect: Effect::CallerMustResolve,
                    cause: Cause::Frame(e),
                });
            }
        };
        let stamp = captured.stamp();
        transition.complete();
        self.control.capture = None;
        self.control.retired = None;
        self.control.authority = None;
        self.control.phase = Phase::Ready;
        Ok(FlushProgress::Complete(FlushObservation {
            captured: stamp,
            finalized,
            io: self.control.io.snapshot(),
        }))
    }
}

fn metadata_peaks() -> (usize, usize) {
    let r = dirty_frame::flush_registry_bytes();
    let f = size_of::<StatusFlushControl<'_, '_, '_>>();
    let retained = r + f + 1024;
    (
        retained + 34 * 1024 + 8192 + wal::DURABILITY_CALL_BYTES + 8192,
        retained + (5 * r + 8192).max(3 * r + 8 * f + 8192),
    )
}
fn fits<W: WriterNamespace>() -> bool {
    let (hot, cold) = metadata_peaks();
    size_of::<StatusFlushControl<'_, '_, '_>>() <= 4096
        && size_of::<StatusFlushAuthority<'_, '_>>() <= 384
        && size_of::<Option<StatusFlushAuthority<'_, '_>>>() <= 384
        && size_of::<Result<StatusFlushAuthority<'_, '_>>>() <= 1024
        && size_of::<std::result::Result<Option<TargetRequest>, dirty_frame::DirtyError>>() <= 256
        && size_of::<StatusFlushError>() <= 768
        && size_of::<Result<FlushProgress>>() <= 1024
        && size_of::<StatusFlusher<'_, '_, '_, '_, '_, '_, '_, W>>() <= 128
        && flush_io::fits::<W>()
        && wal::durability_types_fit()
        && hot <= 65536
        && cold <= 65536
        && size_of::<MutableStorageOwner<'_, '_, W>>()
            .checked_add(size_of::<StatusControl<'_, '_, '_>>())
            .and_then(|n| n.checked_add(size_of::<StatusFlushControl<'_, '_, '_>>()))
            .and_then(|n| {
                size_of::<W::File>()
                    .checked_mul(4)
                    .and_then(|files| n.checked_add(files))
            })
            .and_then(|n| n.checked_add(wal::DURABILITY_CALL_BYTES + 8192 + 8192))
            .is_some_and(|n| n <= 65536)
}

pub(in crate::storage) use operational::StatusLoadPermit;

pub(in crate::storage) use resident::EmptyTransactionInstall;
pub(crate) use resident::EmptyTransactionObservation;

// The original 60,672-byte retained baseline predates the optional history.
// Charge its complete value, including the Option discriminant and padding.
fn history_retained_bytes() -> usize {
    size_of::<Option<resident::History>>() + class_retained_bytes()
}
