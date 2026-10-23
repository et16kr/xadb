//! Consuming allocator-only resident epoch reload. The exclusive phase retains
//! the original owner, reservation descriptor, WAL and Drop fence throughout.
//! No Empty/eviction, other address, Status or ordinary recovery capability.
use super::{
    AllocationWriter, AllocatorFlusher, BoundAllocation, DirtyReader, MutableStorageError,
    allocator_flush, dirty_allocation,
};
use crate::storage::{
    dirty_frame::{self, FrameStamp, ReloadSnapshot},
    page::PAGE_BYTES,
    page_io::{PageFileReader, PageIoError},
    wal_io::{self as wal, bootstrap::BootstrapPlanKind},
};
use std::mem::size_of;
use xadb_foundation::{
    error::{Certainty, Effect},
    namespace::{NamespaceErrorKind, WriterNamespace},
};

#[derive(Debug)]
pub(crate) enum Cause {
    Busy,
    Capacity,
    Shape,
    Invariant,
    HomeMismatch,
    Storage(MutableStorageError),
    Frame(dirty_frame::DirtyError),
    PageIo(PageIoError),
    Wal(wal::WalIoError),
    Allocation(dirty_allocation::Cause),
}
#[derive(Debug)]
pub(crate) struct ReloadError {
    pub(crate) cause: Cause,
    pub(crate) effect: Effect,
}
pub(crate) type Result<T> = std::result::Result<T, ReloadError>;
pub(crate) enum IntoOperationalError<B> {
    Before { error: ReloadError, bound: B },
    Fatal(ReloadError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReloadObservation {
    pub(crate) before: FrameStamp,
    pub(crate) after: FrameStamp,
}

pub(crate) struct OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: BoundAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
/// Only this module constructs a loaded-image permit after actual selected I/O,
/// full byte equality and the original descriptor/durable-prefix verification.
pub(in crate::storage) struct ReloadInstallPermit<'a, 'c, 's> {
    snapshot: &'a ReloadSnapshot<'c, 's>,
    bytes: &'a [u8; PAGE_BYTES],
}
impl<'a, 'c, 's> ReloadInstallPermit<'a, 'c, 's> {
    pub(in crate::storage) fn snapshot(&self) -> &'a ReloadSnapshot<'c, 's> {
        self.snapshot
    }
    pub(in crate::storage) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }
}
fn unchanged(cause: Cause) -> ReloadError {
    ReloadError {
        cause,
        effect: Effect::Unchanged,
    }
}
fn frame(error: dirty_frame::DirtyError) -> ReloadError {
    ReloadError {
        effect: error.effect,
        cause: Cause::Frame(error),
    }
}
fn storage(error: MutableStorageError) -> ReloadError {
    ReloadError {
        effect: error.effect,
        cause: Cause::Storage(error),
    }
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    BoundAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Healthy pre-effect rejection returns the exact original owner and loans.
    /// The wrapper keeps its original Drop policy; no payload or field is moved
    /// out of the bound, and live split views statically prevent conversion.
    #[allow(
        clippy::result_large_err,
        reason = "return the exact inline owner without a heap allocation"
    )]
    pub(crate) fn into_operational(
        mut self,
    ) -> std::result::Result<
        OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        IntoOperationalError<Self>,
    > {
        let original = self.storage;
        let _unwind = original.session_fence().unwind_guard();
        if let Err(error) = original.health() {
            return Err(IntoOperationalError::Fatal(storage(error)));
        }
        let reject = if original.fresh.plan.kind != BootstrapPlanKind::AllocatorOnly {
            Some(unchanged(Cause::Shape))
        } else if !fits::<W>() {
            Some(unchanged(Cause::Capacity))
        } else if self.writer.installation.is_some()
            || self.writer.terminal_pending
            || !self.flush.ready_for_reload()
        {
            Some(unchanged(Cause::Busy))
        } else {
            None
        };
        if let Some(error) = reject {
            return Err(IntoOperationalError::Before { error, bound: self });
        }
        if !self
            .pool
            .uses_storage_origin(&original.binding, original.session_fence())
        {
            original.session_fence().fence();
            return Err(IntoOperationalError::Fatal(ReloadError {
                cause: Cause::Invariant,
                effect: Effect::CallerMustResolve,
            }));
        }
        match self.pool.enable_allocator_reload() {
            Ok(()) => Ok(OperationalAllocation { inner: self }),
            Err(error) if error.effect == Effect::Unchanged => Err(IntoOperationalError::Before {
                error: frame(error),
                bound: self,
            }),
            Err(error) => Err(IntoOperationalError::Fatal(frame(error))),
        }
    }
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn split_with_flush<'b>(
        &'b mut self,
    ) -> (
        AllocationWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        DirtyReader<'b, 'c, 's, 'p, 'd>,
        AllocatorFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        self.inner.split_with_flush()
    }

    fn fatal(&self, cause: Cause) -> ReloadError {
        self.inner.storage.session_fence().fence();
        ReloadError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
    fn read_error(&self, error: PageIoError) -> ReloadError {
        let retryable = matches!(&error, PageIoError::Read(error)
            if error.effect() == Effect::Unchanged && error.certainty() == Certainty::Known)
            || matches!(&error, PageIoError::Namespace(error)
                if error.kind == NamespaceErrorKind::Io && error.effect == Effect::Unchanged);
        if retryable && !self.inner.storage.session_fence().is_fenced() {
            unchanged(Cause::PageIo(error))
        } else {
            self.fatal(Cause::PageIo(error))
        }
    }
    /// Actual home read renews only this allocator's resident generation.
    /// No DB bytes or persistent reservation state changes. Ordinary read error
    /// preserves the registered frame and allows explicit retry; scratch is
    /// invalid. Structural/source disagreement or poison permanently fences.
    pub(crate) fn reload_allocator(&mut self) -> Result<ReloadObservation> {
        let original = self.inner.storage;
        let _unwind = original.session_fence().unwind_guard();
        original.health().map_err(storage)?;
        if !fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        if self.inner.writer.installation.is_some()
            || self.inner.writer.terminal_pending
            || !self.inner.flush.ready_for_reload()
        {
            return Err(unchanged(Cause::Busy));
        }
        let snapshot = self
            .inner
            .pool
            .prepare_allocator_reload(
                (&mut self.inner.writer.operation[65536..65536 + PAGE_BYTES])
                    .try_into()
                    .expect("fixed before page"),
            )
            .map_err(frame)?;
        let before = snapshot.stamp();
        let prefix = original
            .wal()
            .prefix()
            .map_err(|error| self.fatal(Cause::Wal(error)))?;
        self.inner
            .writer
            .observation(original.profile(), prefix, &before, false)
            .map_err(|error| self.fatal(Cause::Allocation(error)))?;
        let request = snapshot.request();
        let loaded = (|| -> std::result::Result<bool, PageIoError> {
            let (work, rest) = self.inner.writer.operation.split_at_mut(65536);
            let (root, work) = work.split_at_mut(PAGE_BYTES);
            let (scratch, work) = work.split_at_mut(PAGE_BYTES);
            let prior = &mut work[..PAGE_BYTES];
            let after: &mut [u8; PAGE_BYTES] = (&mut rest[..PAGE_BYTES])
                .try_into()
                .expect("fixed load page");
            prior.copy_from_slice(after);
            {
                let mut reader = original.fresh.open_reader(
                    root.try_into().expect("fixed root"),
                    scratch.try_into().expect("fixed scratch"),
                )?;
                reader.read_page(request.identity, request.kind, request.level, after)?;
            }
            Ok(prior == after)
        })()
        .map_err(|error| self.read_error(error))?;
        if !loaded {
            return Err(self.fatal(Cause::HomeMismatch));
        }
        original.health().map_err(storage)?;
        let current = original
            .wal()
            .prefix()
            .map_err(|error| self.fatal(Cause::Wal(error)))?;
        if current != prefix {
            return Err(self.fatal(Cause::Invariant));
        }
        self.inner
            .writer
            .observation(original.profile(), current, &before, false)
            .map_err(|error| self.fatal(Cause::Allocation(error)))?;
        // The reader and its root/scratch borrows ended before descriptor decode.
        let permit = ReloadInstallPermit {
            snapshot: &snapshot,
            bytes: (&self.inner.writer.operation[65536..65536 + PAGE_BYTES])
                .try_into()
                .expect("verified load page"),
        };
        let after = self
            .inner
            .pool
            .install_allocator_reload(permit)
            .map_err(frame)?;
        Ok(ReloadObservation { before, after })
    }
}

fn fits<W: WriterNamespace>() -> bool {
    let bound = size_of::<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>();
    let wrapper = size_of::<OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, W>>();
    let result = size_of::<
        std::result::Result<
            OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, W>,
            IntoOperationalError<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>,
        >,
    >();
    let reader = size_of::<PageFileReader<'_, W>>();
    let file = size_of::<W::File>();
    bound <= 20800
        && wrapper <= 20800
        && result <= 21504
        && bound + wrapper + result + 2048 <= 65536
        && reader <= 8192
        && file <= 4096
        && 2 * reader + 4 * file + dirty_frame::CALL_BYTES + 4096 + 16384 + 4096 <= 65536
        && size_of::<ReloadSnapshot<'_, '_>>() <= 256
        && size_of::<ReloadInstallPermit<'_, '_, '_>>() <= 320
        && size_of::<ReloadObservation>() <= 256
        && size_of::<Result<ReloadObservation>>() <= 512
        && size_of::<ReloadError>() <= 512
        && super::super::allocation_reservation::budget::CONSTRUCTOR <= 16384
        && super::super::allocation_reservation::budget::TEMPORARIES <= 16384
        && dirty_frame::fits()
        && allocator_flush::fits::<W>()
}

#[cfg(all(test, target_os = "linux"))]
#[path = "allocator_residency_native_tests.rs"]
mod native_tests;
#[cfg(test)]
#[path = "allocator_residency_tests.rs"]
mod tests;
