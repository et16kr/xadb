//! Closed plan3 actors over the original binding, pool and flush continuation.
//! No old Status actor or mutable frame escapes this facade.

use super::super::{
    BeforeEffectError, InitAttemptError, InitializedStatusObservation, ReservedStatus,
    StatusInitError, StatusReadGuard,
};
use super::*;
use crate::storage::dirty_frame::{FrameStamp, StatusCandidateKind};

#[path = "mixed_system_history.rs"]
mod history;
#[path = "mixed_system_load.rs"]
mod load;
#[path = "mixed_system_resources.rs"]
mod resources;
pub(in super::super) use resources::fits;
#[cfg(test)]
#[path = "mixed_system_owner_tests.rs"]
mod tests;
pub(super) use load::MixedLoadInstall;
pub(crate) use load::{SystemLoadError, SystemLoadObservation, SystemResidencyStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SystemPageKind {
    Status,
    UndoRoot,
}
impl SystemPageKind {
    fn internal(self) -> operation::mixed::Kind {
        match self {
            Self::Status => operation::mixed::Kind::Status,
            Self::UndoRoot => operation::mixed::Kind::UndoRoot,
        }
    }
}
/// A caller's expected identity, never a pin, membership or installation right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemPage {
    pub(crate) page: PageRef,
    pub(crate) kind: SystemPageKind,
}
impl SystemPage {
    fn from_request(request: TargetRequest) -> Option<Self> {
        let kind = match (request.identity.owner.get(), request.kind) {
            (2, PageKind::TransactionStatus) => SystemPageKind::Status,
            (3, PageKind::Undo) => SystemPageKind::UndoRoot,
            _ => return None,
        };
        (request.level == 0 && request.identity.owner_generation.get() == 1).then_some(Self {
            page: request.identity.page,
            kind,
        })
    }
}
pub(crate) struct MixedSystemBinding<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
pub(crate) struct MixedSystem<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MixedSystemBinding<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in super::super::super) fn new(
        inner: BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    ) -> Self {
        Self { inner }
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged activation returns the original inline owner"
    )]
    pub(crate) fn activate(
        self,
    ) -> std::result::Result<MixedSystem<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>, IntoFlushError<Self>> {
        let () = resources::ActivationLayout::<W>::CHECK;
        match self.inner.into_fixed_with_flush(true) {
            Ok(inner) => Ok(MixedSystem { inner }),
            Err(IntoFlushError::Before { error, bound }) => Err(IntoFlushError::Before {
                error,
                bound: Self { inner: bound },
            }),
            Err(IntoFlushError::Fatal(error)) => Err(IntoFlushError::Fatal(error)),
        }
    }
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> MixedSystem<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    pub(crate) fn split<'b>(
        &'b mut self,
    ) -> (
        MixedSystemWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        MixedSystemReader<'b, 'c, 's, 'p, 'd>,
        MixedSystemFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        let (writer, reader, flusher) = self.inner.split_fixed_with_flush();
        (
            MixedSystemWriter { inner: writer },
            MixedSystemReader { inner: reader },
            MixedSystemFlusher { inner: flusher },
        )
    }
    fn quiescent(&self) -> Result<()> {
        let bound = &self.inner.inner;
        let storage = bound.storage;
        storage
            .health()
            .map_err(|source| fatal(storage, Cause::Storage(source)))?;
        if !bound.control.role.mixed()
            || !bound
                .pool
                .mixed_payloads()
                .map_err(|source| error(Cause::Frame(source), source.effect))?
        {
            return Err(fatal(storage, Cause::BindingMismatch));
        }
        if bound.control.installation.is_some()
            || bound.control.base.installation.is_some()
            || bound.control.base.terminal_pending
            || bound.control.base.batch.is_some()
            || !operational::ready(&self.inner.flush)
        {
            return Err(error(Cause::Busy, Effect::Unchanged));
        }
        Ok(())
    }
    /// Finite selection for a new INIT, with no invented desired page identity.
    /// A dirty candidate remains owned; caller flushes it and scans again.
    pub(crate) fn prepare_next(
        &mut self,
        cursor: &mut SystemResidencyCursor,
    ) -> Result<SystemSlotStep> {
        let storage = self.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.quiescent()?;
        let bound = &mut self.inner.inner;
        let stamp = bound
            .pool
            .mixed_allocator_snapshot(
                (&mut bound.control.base.operation[65536..81920])
                    .try_into()
                    .expect("allocator loan"),
            )
            .map_err(|source| {
                let effect = source.effect;
                error(Cause::Frame(source), effect)
            })?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|source| fatal(storage, Cause::Wal(source)))?;
        bound
            .control
            .base
            .observation(storage.profile(), prefix, &stamp, false)
            .map_err(|source| fatal(storage, Cause::Allocation(source)))?;
        let pool = &mut bound.pool;
        let candidate = pool
            .mixed_init_candidate(cursor.0.map_or(1, |value| value.get()))
            .map_err(|source| {
                let effect = source.effect;
                error(Cause::Frame(source), effect)
            })?;
        if candidate.kind == StatusCandidateKind::Flush {
            let frame = candidate.frame.expect("dirty candidate");
            let request = pool.target_request(candidate.index).map_err(|source| {
                let effect = source.effect;
                error(Cause::Frame(source), effect)
            })?;
            let target = SystemPage::from_request(request)
                .ok_or_else(|| fatal(storage, Cause::Invariant))?;
            return Ok(SystemSlotStep::FlushRequired {
                target,
                rec_lsn: frame.rec_lsn.expect("dirty recLSN"),
            });
        }
        let already_selected = candidate.frame.is_none()
            && pool.init_payload_index().map_err(|source| {
                let effect = source.effect;
                error(Cause::Frame(source), effect)
            })? == candidate.index;
        let previous = if already_selected {
            None
        } else {
            pool.prepare_resident_slot(candidate.index)
                .map_err(|source| {
                    let effect = source.effect;
                    error(Cause::Frame(source), effect)
                })?
        };
        cursor.0 = std::num::NonZeroUsize::new(candidate.next);
        Ok(SystemSlotStep::Prepared {
            index: candidate.index,
            previous,
        })
    }
}
#[derive(Debug, Default)]
pub(crate) struct SystemResidencyCursor(Option<std::num::NonZeroUsize>);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SystemSlotStep {
    Prepared {
        index: usize,
        previous: Option<FrameStamp>,
    },
    FlushRequired {
        target: SystemPage,
        rec_lsn: PageLsn,
    },
}

pub(crate) struct MixedSystemWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
pub(crate) struct ReservedSystemPage<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: ReservedStatus<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
impl<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MixedSystemWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn reserve<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        kind: SystemPageKind,
    ) -> std::result::Result<
        ReservedSystemPage<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        StatusInitError,
    > {
        self.inner
            .reserve_fixed(owner, kind.internal())
            .map(|inner| ReservedSystemPage { inner })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InitializedSystemObservation {
    pub(crate) target: SystemPage,
    pub(crate) lsn: PageLsn,
    pub(crate) allocator: FrameStamp,
    pub(crate) payload: FrameStamp,
    pub(crate) wal: wal::WalPrefix,
}
impl<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    ReservedSystemPage<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn target(&self) -> SystemPage {
        SystemPage {
            page: self.inner.page,
            kind: match self.inner.kind {
                operation::mixed::Kind::Status => SystemPageKind::Status,
                operation::mixed::Kind::UndoRoot => SystemPageKind::UndoRoot,
            },
        }
    }
    fn failure(
        error: InitAttemptError<ReservedStatus<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>>,
    ) -> InitAttemptError<Self> {
        match error {
            InitAttemptError::Before(value) => InitAttemptError::Before(BeforeEffectError {
                error: value.error,
                reservation: Self {
                    inner: value.reservation,
                },
            }),
            InitAttemptError::Fatal(error) => InitAttemptError::Fatal(error),
        }
    }
    #[allow(
        clippy::result_large_err,
        reason = "before-effect failure retains the exact linear mixed reservation"
    )]
    pub(crate) fn initialize(
        self,
    ) -> std::result::Result<InitializedSystemObservation, InitAttemptError<Self>> {
        let target = self.target();
        self.inner
            .initialize_fixed()
            .map(
                |InitializedStatusObservation {
                     page: _,
                     lsn,
                     allocator,
                     status,
                     wal,
                 }| InitializedSystemObservation {
                    target,
                    lsn,
                    allocator,
                    payload: status,
                    wal,
                },
            )
            .map_err(Self::failure)
    }
    #[allow(
        clippy::result_large_err,
        reason = "before-effect failure retains the exact linear mixed reservation"
    )]
    pub(crate) fn cancel(
        self,
    ) -> std::result::Result<super::super::AllocationObservation, InitAttemptError<Self>> {
        self.inner.cancel_fixed().map_err(Self::failure)
    }
}
pub(crate) struct MixedSystemReader<'b, 'c, 's, 'p, 'd> {
    inner: StatusReader<'b, 'c, 's, 'p, 'd>,
}
pub(crate) struct MixedSystemReadGuard<'a, 'b, 'c, 's, 'p, 'd> {
    inner: StatusReadGuard<'a, 'b, 'c, 's, 'p, 'd>,
}
impl<'b, 'c, 's, 'p, 'd> MixedSystemReader<'b, 'c, 's, 'p, 'd> {
    pub(crate) fn read<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
        target: SystemPage,
    ) -> std::result::Result<MixedSystemReadGuard<'a, 'b, 'c, 's, 'p, 'd>, StatusInitError> {
        if self.inner.fence.is_fenced() {
            return Err(super::super::frame_error(
                super::super::Phase::Observe,
                dirty_frame::DirtyError {
                    effect: Effect::CallerMustResolve,
                    cause: dirty_frame::Cause::Fenced,
                },
            ));
        }
        let read = {
            let request =
                operation::fixed::request(self.inner.role, target.page, target.kind.internal())
                    .map_err(|source| {
                        super::super::error(
                            super::super::Phase::Observe,
                            Effect::Unchanged,
                            super::super::Cause::Semantic(source),
                        )
                    })?;
            self.inner.reserve_fixed_read(owner, Some(request))?
        };
        self.inner
            .finish_fixed_read(read)
            .map(|inner| MixedSystemReadGuard { inner })
    }
}
impl MixedSystemReadGuard<'_, '_, '_, '_, '_, '_> {
    pub(crate) fn bytes(&self) -> std::result::Result<&[u8; PAGE_BYTES], StatusInitError> {
        self.inner.bytes()
    }
    pub(crate) fn stamps(&self) -> std::result::Result<(FrameStamp, FrameStamp), StatusInitError> {
        self.inner.stamps()
    }
    pub(crate) fn release_checked(self) -> std::result::Result<(), StatusInitError> {
        self.inner.release_checked()
    }
}
pub(crate) struct MixedSystemFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    inner: StatusFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
}
impl<'c, 's, W: WriterNamespace> MixedSystemFlusher<'_, 's, 'c, '_, '_, '_, '_, W> {
    pub(crate) fn snapshot(&self) -> FlushSnapshot {
        self.inner.snapshot()
    }
    pub(crate) fn try_flush_allocator(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<FlushProgress> {
        self.inner
            .try_flush_fixed(owner, false, None, None, operation::mixed::Kind::Status)
    }
    pub(crate) fn try_flush(
        &mut self,
        owner: &mut Owner<'c, 's>,
        target: SystemPage,
    ) -> Result<FlushProgress> {
        self.inner
            .try_flush_fixed(owner, true, Some(target.page), None, target.kind.internal())
    }
    pub(crate) fn continue_flush(&mut self, owner: &mut Owner<'c, 's>) -> Result<FlushProgress> {
        self.inner.continue_flush(owner)
    }
}
