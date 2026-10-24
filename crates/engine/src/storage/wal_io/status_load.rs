//! Selected immutable Status home loading through the original operational owner.
use super::super::super::{dirty_allocation, operation};
use super::super::BoundStatusWithFlush;
use super::{OperationalStatus, ready};
use crate::storage::{
    dirty_frame::{self, FrameStamp, StatusLoadSnapshot, TargetRequest},
    page::{PAGE_BYTES, PageLsn, PageRef},
    page_io::{PageFileReader, PageIoError},
    wal_io::{MutableStorageError, WalIoError},
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
    AlreadyResident,
    Membership,
    Invariant,
    Storage(MutableStorageError),
    Frame(dirty_frame::DirtyError),
    PageIo(PageIoError),
    Wal(WalIoError),
    Allocation(dirty_allocation::Cause),
    Semantic(super::super::super::SemanticError),
}
#[derive(Debug)]
pub(crate) struct StatusLoadError {
    pub(crate) cause: Cause,
    pub(crate) effect: Effect,
}
type Result<T> = std::result::Result<T, StatusLoadError>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatusLoadObservation {
    pub(crate) before: Option<FrameStamp>,
    pub(crate) after: FrameStamp,
}

/// Only this module can construct the permit after authentic membership and
/// selected disk read. Its accessors do not mint or change installation rights.
pub(in crate::storage) struct StatusLoadPermit<'a, 'c, 's> {
    snapshot: &'a StatusLoadSnapshot<'c, 's>,
    index: usize,
    allocator: &'a [u8; PAGE_BYTES],
    bytes: &'a [u8; PAGE_BYTES],
    request: TargetRequest,
    lsn: PageLsn,
}
impl<'a, 'c, 's> StatusLoadPermit<'a, 'c, 's> {
    pub(in super::super) fn from_mixed(
        value: super::super::mixed::MixedLoadInstall<'a, 'c, 's>,
    ) -> Self {
        Self {
            snapshot: value.snapshot(),
            index: value.index(),
            allocator: value.allocator(),
            bytes: value.bytes(),
            request: value.request(),
            lsn: value.lsn(),
        }
    }
    pub(in super::super) fn from_live(
        value: super::super::resident::EmptyLoadInstall<'a, 'c, 's>,
    ) -> Self {
        Self {
            snapshot: value.snapshot(),
            index: value.index(),
            allocator: value.allocator(),
            bytes: value.bytes(),
            request: value.request(),
            lsn: value.lsn(),
        }
    }
    pub(in crate::storage) fn index(&self) -> usize {
        self.index
    }
    pub(in crate::storage) fn snapshot(&self) -> &'a StatusLoadSnapshot<'c, 's> {
        self.snapshot
    }
    pub(in crate::storage) fn allocator(&self) -> &'a [u8; PAGE_BYTES] {
        self.allocator
    }
    pub(in crate::storage) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.request
    }
    pub(in crate::storage) fn lsn(&self) -> PageLsn {
        self.lsn
    }
}
fn unchanged(cause: Cause) -> StatusLoadError {
    StatusLoadError {
        cause,
        effect: Effect::Unchanged,
    }
}
fn frame(source: dirty_frame::DirtyError) -> StatusLoadError {
    StatusLoadError {
        effect: source.effect,
        cause: Cause::Frame(source),
    }
}

impl<W: WriterNamespace> OperationalStatus<'_, '_, '_, '_, '_, '_, '_, W> {
    pub(crate) fn load_status(&mut self, page: PageRef) -> Result<StatusLoadObservation> {
        self.inner.load_status_at(1, page)
    }
}

impl<W: WriterNamespace> BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W> {
    fn load_fatal(&self, cause: Cause) -> StatusLoadError {
        self.inner.storage.session_fence().fence();
        StatusLoadError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
    fn load_read_error(&self, source: PageIoError) -> StatusLoadError {
        let retryable = matches!(&source, PageIoError::Read(error) if error.effect() == Effect::Unchanged && error.certainty() == Certainty::Known)
            || matches!(&source, PageIoError::Namespace(error) if error.kind == NamespaceErrorKind::Io && error.effect == Effect::Unchanged);
        if retryable && !self.inner.storage.session_fence().is_fenced() {
            unchanged(Cause::PageIo(source))
        } else {
            self.load_fatal(Cause::PageIo(source))
        }
    }
    /// Read an exact Allocated Status member into the existing second frame.
    /// The allocator may be dirty; the payload must be clean and unpinned. A
    /// known, unchanged read failure keeps the old residency and allows an
    /// explicit retry. Corruption, source drift or unwind fences this owner.
    /// No namespace/content lock spans file read/checksum, and no DB write or
    /// sync is performed. The returned stamps are diagnostics, not capabilities.
    pub(in super::super::super) fn load_status_at(
        &mut self,
        index: usize,
        page: PageRef,
    ) -> Result<StatusLoadObservation> {
        let original = self.inner.storage;
        let _unwind = original.session_fence().unwind_guard();
        original
            .health()
            .map_err(|e| self.load_fatal(Cause::Storage(e)))?;
        if self.inner.control.role.mixed() {
            return Err(unchanged(Cause::Membership));
        }
        if !fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        let control = &self.inner.control;
        if control.installation.is_some()
            || control.base.installation.is_some()
            || control.base.terminal_pending
            || control.base.batch.is_some()
            || !ready(&self.flush)
        {
            return Err(unchanged(Cause::Busy));
        }
        if !control.role.uses_fence(original.session_fence())
            || !self
                .inner
                .pool
                .uses_storage_origin(&original.binding, original.session_fence())
        {
            return Err(self.load_fatal(Cause::Invariant));
        }
        let snapshot = self
            .inner
            .pool
            .prepare_status_load_at(
                index,
                (&mut self.inner.control.base.operation[65536..81920])
                    .try_into()
                    .expect("allocator scratch"),
            )
            .map_err(frame)?;
        let prefix = original
            .wal()
            .prefix()
            .map_err(|e| self.load_fatal(Cause::Wal(e)))?;
        self.inner
            .control
            .base
            .observation(original.profile(), prefix, &snapshot.allocator(), false)
            .map_err(|e| self.load_fatal(Cause::Allocation(e)))?;
        let control = &self.inner.control;
        let request = TargetRequest {
            identity: crate::storage::page::PageIdentity {
                database: control.role.namespace().database,
                page,
                owner: control.role.object(),
                owner_generation: control.role.generation(),
            },
            kind: control.role.kind(),
            level: control.role.level(),
        };
        // Full allocator/descriptor validation above makes the remaining failure
        // a caller membership mismatch, never a masked allocator corruption.
        operation::fixed::membership(
            &original.profile(),
            &snapshot.allocator().identity,
            &request.identity,
            &control.role,
            control.base.before(),
            super::AllocationPopulation::Status,
            operation::mixed::Kind::Status,
        )
        .map_err(|_| unchanged(Cause::Membership))?;
        if self
            .inner
            .pool
            .resident_address_present(request)
            .map_err(frame)?
        {
            return Err(unchanged(Cause::AlreadyResident));
        }
        let descriptor = control.base.descriptor;
        {
            let control = &mut self.inner.control;
            let (root, rest) = control.base.operation[..32768].split_at_mut(PAGE_BYTES);
            let read = (|| -> std::result::Result<(), PageIoError> {
                let mut reader = original.core.open_reader(
                    root.try_into().expect("root scratch"),
                    rest.try_into().expect("read scratch"),
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
            read.map_err(|e| self.load_read_error(e))?;
        }
        let lsn = operation::status_lsn(self.inner.control.status_after, &request.identity)
            .map_err(|e| self.load_fatal(Cause::Semantic(e)))?;
        if lsn.0 == 0 || lsn.0 > prefix.synced_lsn {
            return Err(self.load_fatal(Cause::Invariant));
        }
        original
            .health()
            .map_err(|e| self.load_fatal(Cause::Storage(e)))?;
        let current = original
            .wal()
            .prefix()
            .map_err(|e| self.load_fatal(Cause::Wal(e)))?;
        if current != prefix || self.inner.control.base.descriptor != descriptor {
            return Err(self.load_fatal(Cause::Invariant));
        }
        self.inner
            .control
            .base
            .observation(original.profile(), current, &snapshot.allocator(), false)
            .map_err(|e| self.load_fatal(Cause::Allocation(e)))?;
        let control = &self.inner.control;
        let permit = StatusLoadPermit {
            snapshot: &snapshot,
            index,
            allocator: control.base.before(),
            bytes: control.status_after,
            request,
            lsn,
        };
        let after = self
            .inner
            .pool
            .install_status_load(permit)
            .map_err(|e| self.load_fatal(Cause::Frame(e)))?;
        Ok(StatusLoadObservation {
            before: snapshot.payload(),
            after,
        })
    }
}

/// Logical simultaneously live values, not a compiler physical-stack claim.
/// The original bound remains live. Reader moves/open errors, four file values,
/// descriptor copy, snapshot/result carriers and the largest sequential codec/
/// observation/registry helper are all charged. Page buffers are existing loans.
pub(in super::super) fn call_bytes<W: WriterNamespace>() -> usize {
    size_of::<crate::storage::wal_io::MutableStorageOwner<'_, '_, W>>()
        + size_of::<OperationalStatus<'_, '_, '_, '_, '_, '_, '_, W>>()
        + 2 * size_of::<PageFileReader<'_, W>>()
        + 4 * size_of::<W::File>()
        + 2 * size_of::<StatusLoadSnapshot<'_, '_>>()
        + 2 * size_of::<StatusLoadPermit<'_, '_, '_>>()
        + 2 * size_of::<Result<StatusLoadObservation>>()
        + 1536
        + 4096
        + 16384
        + dirty_frame::CALL_BYTES
}
pub(in super::super) fn metadata_bytes<W: WriterNamespace>() -> (usize, usize) {
    // Conservative composition with the existing retained baseline. The bound
    // is also in call_bytes, so its retained fields are intentionally overcharged.
    let retained = 60672
        + super::super::history_retained_bytes()
        + dirty_frame::init_pool_delta_bytes()
        + size_of::<super::super::super::StatusControl<'_, '_, '_>>()
            .saturating_sub(dirty_allocation::baseline_control_bytes());
    let dirty = retained + call_bytes::<W>();
    (dirty, dirty + 65536)
}
pub(in super::super) fn fits<W: WriterNamespace>() -> bool {
    let (dirty, whole) = metadata_bytes::<W>();
    dirty <= 196608
        && whole <= 262144
        && call_bytes::<W>() <= 65536
        && super::fits::<W>()
        && dirty_frame::init_fits()
        && operation::reader_bytes() <= 3648
}

#[cfg(test)]
#[path = "status_load_tests.rs"]
mod tests;
