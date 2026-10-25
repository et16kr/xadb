//! Exact immutable mixed-home load, sealed by the original complete WAL history.
use super::*;
use crate::storage::{
    dirty_frame::{self, StatusLoadSnapshot},
    page_io::{PageFileReader, PageIoError},
    wal_io::{MutableStorageError, StatusLoadPermit, WalIoError},
};
use foundation::{codec::content_digest, error::Certainty, namespace::NamespaceErrorKind};

#[derive(Debug)]
pub(crate) enum LoadCause {
    Busy,
    Capacity,
    Membership,
    AlreadyResident,
    Invariant,
    Storage(MutableStorageError),
    Frame(dirty_frame::DirtyError),
    PageIo(PageIoError),
    Wal(WalIoError),
    Semantic(SemanticError),
    Allocation(super::super::super::dirty_allocation::Cause),
    History(history::HistoryError),
}
#[derive(Debug)]
pub(crate) struct SystemLoadError {
    pub(crate) cause: LoadCause,
    pub(crate) effect: Effect,
}
type LoadResult<T> = std::result::Result<T, SystemLoadError>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemLoadObservation {
    pub(crate) before: Option<FrameStamp>,
    pub(crate) after: FrameStamp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SystemResidencyStep {
    Resident {
        index: usize,
        frame: FrameStamp,
        loaded: bool,
    },
    FlushRequired {
        target: SystemPage,
        rec_lsn: PageLsn,
    },
}
fn unchanged(cause: LoadCause) -> SystemLoadError {
    SystemLoadError {
        cause,
        effect: Effect::Unchanged,
    }
}
fn frame(source: dirty_frame::DirtyError) -> SystemLoadError {
    SystemLoadError {
        effect: source.effect,
        cause: LoadCause::Frame(source),
    }
}

/// Only the complete snapshot/history/final-read path constructs this proof.
/// Its immutable loans survive until the original directory installs the image.
pub(in super::super) struct MixedLoadInstall<'a, 'c, 's> {
    snapshot: &'a StatusLoadSnapshot<'c, 's>,
    index: usize,
    allocator: &'a [u8; PAGE_BYTES],
    bytes: &'a [u8; PAGE_BYTES],
    request: TargetRequest,
    lsn: PageLsn,
}
impl<'a, 'c, 's> MixedLoadInstall<'a, 'c, 's> {
    pub(in super::super) fn snapshot(&self) -> &'a StatusLoadSnapshot<'c, 's> {
        self.snapshot
    }
    pub(in super::super) fn index(&self) -> usize {
        self.index
    }
    pub(in super::super) fn allocator(&self) -> &'a [u8; PAGE_BYTES] {
        self.allocator
    }
    pub(in super::super) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }
    pub(in super::super) fn request(&self) -> TargetRequest {
        self.request
    }
    pub(in super::super) fn lsn(&self) -> PageLsn {
        self.lsn
    }
}
impl<W: WriterNamespace> MixedSystem<'_, '_, '_, '_, '_, '_, '_, W> {
    fn load_fatal(&self, cause: LoadCause) -> SystemLoadError {
        self.inner.inner.storage.session_fence().fence();
        SystemLoadError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
    fn load_ready(&self) -> LoadResult<()> {
        let bound = &self.inner.inner;
        bound
            .storage
            .health()
            .map_err(|source| self.load_fatal(LoadCause::Storage(source)))?;
        if !fits::<W>() {
            return Err(unchanged(LoadCause::Capacity));
        }
        if !bound.control.role.mixed() || !bound.pool.mixed_payloads() {
            return Err(self.load_fatal(LoadCause::Invariant));
        }
        if bound.control.installation.is_some()
            || bound.control.base.installation.is_some()
            || bound.control.base.batch.is_some()
            || bound.control.base.terminal_pending
            || !operational::ready(&self.inner.flush)
        {
            return Err(unchanged(LoadCause::Busy));
        }
        Ok(())
    }
    fn read_home(&mut self, request: TargetRequest) -> LoadResult<PageLsn> {
        let storage = self.inner.inner.storage;
        let control = &mut self.inner.inner.control;
        let (root, rest) = control.base.operation[..32768].split_at_mut(PAGE_BYTES);
        let read = (|| -> std::result::Result<(), PageIoError> {
            let mut reader = storage.core.open_reader(
                root.try_into().expect("root loan"),
                rest.try_into().expect("read loan"),
            )?;
            reader
                .read_page(
                    request.identity,
                    request.kind,
                    request.level,
                    control.status_after,
                )
                .map(|_| ())
        })();
        read.map_err(|source| {
            let retryable = matches!(&source, PageIoError::Read(error) if error.effect() == Effect::Unchanged
                && error.certainty() == Certainty::Known
                && error.io_kind.is_some_and(|kind| kind != std::io::ErrorKind::UnexpectedEof))
                || matches!(&source, PageIoError::Namespace(error) if error.kind == NamespaceErrorKind::Io && error.effect == Effect::Unchanged);
            if retryable && !storage.session_fence().is_fenced() { unchanged(LoadCause::PageIo(source)) }
            else { self.load_fatal(LoadCause::PageIo(source)) }
        })?;
        let bytes = &*self.inner.inner.control.status_after;
        let lsn = if request.kind == PageKind::Undo {
            crate::storage::undo_record::initial_undo_page_lsn(bytes, request.identity)
                .map_err(|_| SemanticError::PageMismatch)
        } else {
            operation::status_lsn(bytes, &request.identity)
        };
        lsn.map_err(|source| self.load_fatal(LoadCause::Semantic(source)))
    }
    pub(crate) fn load_into(
        &mut self,
        index: usize,
        target: SystemPage,
    ) -> LoadResult<SystemLoadObservation> {
        let storage = self.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.load_ready()?;
        let bound = &mut self.inner.inner;
        let snapshot = bound
            .pool
            .prepare_status_load_at(
                index,
                (&mut bound.control.base.operation[65536..81920])
                    .try_into()
                    .expect("allocator loan"),
            )
            .map_err(frame)?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|source| self.load_fatal(LoadCause::Wal(source)))?;
        let bound = &self.inner.inner;
        bound
            .control
            .base
            .observation(storage.profile(), prefix, &snapshot.allocator(), false)
            .map_err(|source| self.load_fatal(LoadCause::Allocation(source)))?;
        let request =
            operation::fixed::request(&bound.control.role, target.page, target.kind.internal())
                .map_err(|_| unchanged(LoadCause::Membership))?;
        operation::fixed::membership(&storage.profile(), &snapshot.allocator().identity, &request.identity,
            &bound.control.role, bound.control.base.before(),
            super::super::super::super::super::allocation_reservation::operation::AllocationPopulation::MixedSystem,
            target.kind.internal()).map_err(|_| unchanged(LoadCause::Membership))?;
        if bound
            .pool
            .resident_address_present(request)
            .map_err(frame)?
        {
            return Err(unchanged(LoadCause::AlreadyResident));
        }
        let allocator_digest = content_digest(bound.control.base.before());
        let initial_lsn = self.read_home(request)?;
        let initial_digest = content_digest(self.inner.inner.control.status_after);
        if initial_lsn.0 == 0 || initial_lsn.0 > prefix.synced_lsn {
            return Err(self.load_fatal(LoadCause::Invariant));
        }
        let sealed = {
            let mut source = match storage.wal().file.try_lock() {
                Ok(source) => source,
                Err(TryLockError::WouldBlock) => return Err(unchanged(LoadCause::Busy)),
                Err(TryLockError::Poisoned(_)) => return Err(self.load_fatal(LoadCause::Invariant)),
            };
            let control = &mut self.inner.inner.control;
            history::reconstruct(&mut *source, control.base.operation, control.status_after,
                &control.role, storage.profile(), snapshot.allocator().identity, allocator_digest,
                &control.base.descriptor, prefix, request)
        }.map_err(|source| {
            let retryable = matches!(&source, history::HistoryError::Read(error) if error.effect() == Effect::Unchanged
                && error.certainty() == Certainty::Known
                && error.io_kind.is_some_and(|kind| kind != std::io::ErrorKind::UnexpectedEof));
            if retryable && !storage.session_fence().is_fenced() { unchanged(LoadCause::History(source)) }
            else { self.load_fatal(LoadCause::History(source)) }
        })?;
        let lsn = self.read_home(request)?;
        let control = &self.inner.inner.control;
        if lsn != initial_lsn
            || lsn != sealed.lsn()
            || initial_digest != *sealed.digest()
            || content_digest(control.status_after) != *sealed.digest()
            || content_digest(control.base.before()) != allocator_digest
            || *sealed.allocator_digest() != allocator_digest
            || sealed.high_water() != control.base.work_high_water
            || sealed.terminal() != control.base.terminal
            || storage
                .wal()
                .prefix()
                .map_err(|source| self.load_fatal(LoadCause::Wal(source)))?
                != prefix
        {
            return Err(self.load_fatal(LoadCause::Invariant));
        }
        storage
            .health()
            .map_err(|source| self.load_fatal(LoadCause::Storage(source)))?;
        control
            .base
            .observation(storage.profile(), prefix, &snapshot.allocator(), false)
            .map_err(|source| self.load_fatal(LoadCause::Allocation(source)))?;
        let bound = &mut self.inner.inner;
        let permit = StatusLoadPermit::from_mixed(MixedLoadInstall {
            snapshot: &snapshot,
            index,
            allocator: bound.control.base.before(),
            bytes: bound.control.status_after,
            request,
            lsn,
        });
        let after = bound.pool.install_status_load(permit).map_err(|source| {
            storage.session_fence().fence();
            SystemLoadError {
                cause: LoadCause::Frame(source),
                effect: Effect::CallerMustResolve,
            }
        })?;
        Ok(SystemLoadObservation {
            before: snapshot.payload(),
            after,
        })
    }
    pub(crate) fn residency_step(
        &mut self,
        target: SystemPage,
        cursor: &mut SystemResidencyCursor,
    ) -> LoadResult<SystemResidencyStep> {
        let storage = self.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.load_ready()?;
        let request = operation::fixed::request(
            &self.inner.inner.control.role,
            target.page,
            target.kind.internal(),
        )
        .map_err(|_| unchanged(LoadCause::Membership))?;
        let bound = &mut self.inner.inner;
        let allocator = bound
            .pool
            .mixed_allocator_snapshot(
                (&mut bound.control.base.operation[65536..81920])
                    .try_into()
                    .expect("allocator loan"),
            )
            .map_err(frame)?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|source| self.load_fatal(LoadCause::Wal(source)))?;
        let bound = &self.inner.inner;
        bound
            .control
            .base
            .observation(storage.profile(), prefix, &allocator, false)
            .map_err(|source| self.load_fatal(LoadCause::Allocation(source)))?;
        operation::fixed::membership(&storage.profile(), &allocator.identity, &request.identity,
            &bound.control.role, bound.control.base.before(),
            super::super::super::super::super::allocation_reservation::operation::AllocationPopulation::MixedSystem,
            target.kind.internal()).map_err(|_| unchanged(LoadCause::Membership))?;
        let candidate = self
            .inner
            .inner
            .pool
            .mixed_candidate(request, cursor.0.map_or(1, |value| value.get()))
            .map_err(frame)?;
        let result = match candidate.kind {
            StatusCandidateKind::Resident => SystemResidencyStep::Resident {
                index: candidate.index,
                frame: candidate.frame.expect("resident"),
                loaded: false,
            },
            StatusCandidateKind::Load => SystemResidencyStep::Resident {
                index: candidate.index,
                frame: self.load_into(candidate.index, target)?.after,
                loaded: true,
            },
            StatusCandidateKind::Flush => {
                let request = self
                    .inner
                    .inner
                    .pool
                    .target_request(candidate.index)
                    .map_err(frame)?;
                let target = SystemPage::from_request(request)
                    .ok_or_else(|| self.load_fatal(LoadCause::Invariant))?;
                return Ok(SystemResidencyStep::FlushRequired {
                    target,
                    rec_lsn: candidate
                        .frame
                        .expect("dirty candidate")
                        .rec_lsn
                        .expect("dirty recLSN"),
                });
            }
        };
        cursor.0 = std::num::NonZeroUsize::new(candidate.next);
        Ok(result)
    }
}

pub(super) fn call_bytes<W: WriterNamespace>() -> usize {
    // Snapshot/return carriers live through the largest sequential history,
    // page-reader or registry phase. Caller page scratch is charged separately.
    size_of::<crate::storage::wal_io::MutableStorageOwner<'_, '_, W>>()
        + size_of::<MixedSystem<'_, '_, '_, '_, '_, '_, '_, W>>()
        + 4 * size_of::<StatusLoadSnapshot<'_, '_>>()
        + 4 * size_of::<StatusLoadPermit<'_, '_, '_>>()
        + 4 * size_of::<MixedLoadInstall<'_, '_, '_>>()
        + 4 * size_of::<LoadResult<SystemLoadObservation>>()
        + 4 * size_of::<wal::WalPrefix>()
        + 4 * size_of::<history::History>()
        + 4 * size_of::<W::File>()
        + 2 * size_of::<PageFileReader<'_, W>>()
        + history::call_bytes::<W>()
            .max(16384)
            .max(dirty_frame::CALL_BYTES)
        + 4096
}
pub(super) fn fits<W: WriterNamespace>() -> bool {
    call_bytes::<W>() <= 65536
}
