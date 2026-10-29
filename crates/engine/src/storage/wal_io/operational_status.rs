//! Serialized Status creation through the original bound and flush owner.
//! Preparing the next destination is an explicit exclusive eviction, completed
//! before a linear reservation borrows the writer. No inverse conversion exists.

use super::super::super::super::allocation_reservation::operation::AllocationPopulation;
use super::super::{ReservedStatus, StatusInitError, StatusReadGuard};
use super::{
    BoundStatusWithFlush, Cause, Phase, Result, StatusBootstrapWriter, StatusFlushControl,
    StatusFlushError, StatusFlusher, StatusReader, error, fatal, flush_io,
};
use crate::storage::dirty_frame::FrameStamp;
use crate::storage::page::PageKind;
use crate::transaction::concurrency::Owner;
use foundation::{error::Effect, namespace::WriterNamespace};
use std::mem::size_of;

#[path = "operational_status/load.rs"]
pub(in super::super) mod load;
pub(in crate::storage) use load::StatusLoadPermit;

#[cfg(test)]
#[path = "operational_status/tests.rs"]
mod tests;

pub(crate) enum IntoOperationalStatusError<B> {
    Before { error: StatusFlushError, bound: B },
    Fatal(StatusFlushError),
}

pub(crate) struct OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    // Original Drop fences before releasing any retained flush workspace.
    inner: BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}

pub(in super::super) fn ready(control: &StatusFlushControl<'_, '_, '_>) -> bool {
    control.phase == Phase::Ready
        && control.capture.is_none()
        && control.retired.is_none()
        && control.authority.is_none()
        && control.io.snapshot().state == flush_io::State::Ready
}

const fn conversion_bytes<W: WriterNamespace>() -> usize {
    type Bound<'a, W> = BoundStatusWithFlush<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Converted<'a, W> = OperationalStatus<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Error<'a, W> = IntoOperationalStatusError<Bound<'a, W>>;
    let converted = size_of::<Converted<'_, W>>();
    let error = size_of::<Error<'_, W>>();
    size_of::<Bound<'_, W>>()
        + if converted > error { converted } else { error }
        + size_of::<std::result::Result<Converted<'_, W>, Error<'_, W>>>()
        + 2048
}

struct ConversionLayout<W>(std::marker::PhantomData<W>);
impl<W: WriterNamespace> ConversionLayout<W> {
    // Fixed return/error carriers must fit even on an admission rejection.
    const CHECK: () = assert!(conversion_bytes::<W>() <= 65536);
}

fn fits<W: WriterNamespace>() -> bool {
    conversion_bytes::<W>() <= 65536 && super::fits::<W>() && super::super::fits::<W>()
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Consume the exact closed owner after quiescence. Healthy rejection
    /// returns it intact. All checks precede the infallible mode/owner move.
    #[allow(
        clippy::result_large_err,
        reason = "exact inline owner is returned without heap allocation"
    )]
    pub(crate) fn into_operational(
        mut self,
    ) -> std::result::Result<
        OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        IntoOperationalStatusError<Self>,
    > {
        let () = ConversionLayout::<W>::CHECK;
        let storage = self.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        if let Err(source) = storage.health() {
            return Err(IntoOperationalStatusError::Fatal(fatal(
                storage,
                Cause::Storage(source),
            )));
        }
        let role = &self.inner.control.role;
        if !self
            .inner
            .pool
            .uses_storage_origin(&storage.binding, storage.session_fence())
            || role.mixed()
            || !role.uses_fence(storage.session_fence())
            || role.namespace() != storage.profile().spec().namespace
            || role.object().get() != 2
            || role.generation().get() != 1
            || role.kind() != PageKind::TransactionStatus
            || role.level() != 0
        {
            return Err(IntoOperationalStatusError::Fatal(fatal(
                storage,
                Cause::BindingMismatch,
            )));
        }
        let base = &self.inner.control.base;
        let refusal = if !fits::<W>() {
            Some(Cause::Capacity)
        } else if self.inner.control.installation.is_some()
            || base.installation.is_some()
            || base.terminal_pending
            || base.batch.is_some()
            || !ready(&self.flush)
        {
            Some(Cause::Busy)
        } else {
            None
        };
        if let Some(cause) = refusal {
            return Err(IntoOperationalStatusError::Before {
                error: error(cause, Effect::Unchanged),
                bound: self,
            });
        }
        match self.inner.pool.enable_status_operation() {
            Ok(()) => {
                self.inner.control.base.population = AllocationPopulation::Status;
                Ok(OperationalStatus { inner: self })
            }
            Err(source) if source.effect == Effect::Unchanged => {
                Err(IntoOperationalStatusError::Before {
                    error: error(Cause::Frame(source), Effect::Unchanged),
                    bound: self,
                })
            }
            Err(source) => Err(IntoOperationalStatusError::Fatal(fatal(
                storage,
                Cause::Frame(source),
            ))),
        }
    }
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn split_with_flush<'b>(
        &'b mut self,
    ) -> (
        OperationalStatusWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        OperationalStatusReader<'b, 'c, 's, 'p, 'd>,
        StatusFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        let (writer, reader, flush) = self.inner.split_with_flush();
        (
            OperationalStatusWriter { inner: writer },
            OperationalStatusReader { inner: reader },
            flush,
        )
    }

    /// Evict the clean current Status and prepare the next slot generation.
    /// An already prepared empty slot returns MissingTarget unchanged; reuse it
    /// directly after cancel or before-effect reservation failure. The returned
    /// stamp describes the retired residency, not authority to reload it.
    pub(crate) fn prepare_next_status(&mut self) -> Result<FrameStamp> {
        let storage = self.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        storage
            .health()
            .map_err(|source| fatal(storage, Cause::Storage(source)))?;
        let control = &self.inner.inner.control;
        if control.installation.is_some()
            || control.base.installation.is_some()
            || control.base.terminal_pending
            || control.base.batch.is_some()
            || !ready(&self.inner.flush)
        {
            return Err(error(Cause::Busy, Effect::Unchanged));
        }
        self.inner
            .inner
            .pool
            .prepare_next_status()
            .map_err(|source| {
                let effect = source.effect;
                error(Cause::Frame(source), effect)
            })
    }
}

/// This view selects the operational population retained by its consuming owner.
/// It does not expose or reconstruct the original closed bound.
pub(crate) struct OperationalStatusWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
impl<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    OperationalStatusWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn reserve_one<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
    ) -> std::result::Result<ReservedStatus<'r, 'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, StatusInitError>
    {
        self.inner.reserve_one(owner)
    }
}

pub(crate) struct OperationalStatusReader<'b, 'c, 's, 'p, 'd> {
    inner: StatusReader<'b, 'c, 's, 'p, 'd>,
}
impl<'b, 'c, 's, 'p, 'd> OperationalStatusReader<'b, 'c, 's, 'p, 'd> {
    pub(crate) fn read<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
    ) -> std::result::Result<StatusReadGuard<'a, 'b, 'c, 's, 'p, 'd>, StatusInitError> {
        self.inner.read(owner)
    }
}

#[cfg(test)]
pub(super) fn load_layout<W: WriterNamespace>() -> (usize, (usize, usize), bool) {
    (
        load::call_bytes::<W>(),
        load::metadata_bytes::<W>(),
        load::fits::<W>(),
    )
}
