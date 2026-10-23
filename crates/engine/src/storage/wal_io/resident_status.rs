//! Several Status residencies under the original storage binding and controls.
//! Exclusive replacement never creates a second pin, DPT or WAL authority.

use super::super::{
    BoundStatusBootstrap, Cause, Phase, Result, StatusBootstrapWriter, StatusReader, error,
    frame_error, merge_cleanup,
};
use super::{
    BoundStatusWithFlush, IntoFlushError, StatusFlushError, StatusFlusher,
    operational::{
        load::{StatusLoadError, StatusLoadObservation},
        ready,
    },
};
use crate::{
    storage::{
        dirty_frame::{FrameStamp, ReadTargets, TargetRequest},
        page::{PAGE_BYTES, PageIdentity, PageRef},
        session_fence::SessionFence,
    },
    transaction::concurrency::Owner,
};
use std::sync::{Mutex, MutexGuard, TryLockError};
use xadb_foundation::{error::Effect, namespace::WriterNamespace};

#[path = "empty_transactions.rs"]
mod empty_transactions;
pub(crate) use empty_transactions::{
    EMPTY_TRANSACTION_BANK_BYTES, EmptyTransactionSnapshot, EmptyTransactions,
    IntoEmptyTransactionsError,
};

pub(crate) struct ResidentStatusBinding<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
}
pub(crate) struct ResidentStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    inner: BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    read_gate: Mutex<()>,
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    ResidentStatusBinding<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[cfg(test)]
    pub(in crate::storage) fn resource_census() -> (usize, usize, usize, usize, bool) {
        (
            activation_bytes::<W>(),
            wide_call_bytes(),
            metadata_peaks::<W>().0,
            metadata_peaks::<W>().2,
            fits::<W>(),
        )
    }
    pub(in super::super) fn new(
        inner: BoundStatusBootstrap<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    ) -> Self {
        Self { inner }
    }
    #[allow(
        clippy::result_large_err,
        reason = "return the exact inline binding for retry without heap allocation"
    )]
    pub(crate) fn activate(
        self,
    ) -> std::result::Result<ResidentStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>, IntoFlushError<Self>>
    {
        let () = ActivationLayout::<W>::CHECK;
        match self.inner.into_with_flush() {
            Ok(inner) => Ok(ResidentStatus {
                inner,
                read_gate: Mutex::new(()),
            }),
            Err(IntoFlushError::Before { error, bound }) => Err(IntoFlushError::Before {
                error,
                bound: Self::new(bound),
            }),
            Err(IntoFlushError::Fatal(error)) => Err(IntoFlushError::Fatal(error)),
        }
    }
}
impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> ResidentStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    pub(crate) fn split_with_flush<'b>(
        &'b mut self,
    ) -> (
        StatusBootstrapWriter<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        ResidentStatusReader<'b, 'c, 's, 'p, 'd>,
        StatusFlusher<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) {
        let (writer, reader, flusher) = self.inner.split_with_flush();
        (
            writer,
            ResidentStatusReader {
                inner: reader,
                gate: &self.read_gate,
            },
            flusher,
        )
    }
    pub(crate) fn prepare_slot(
        &mut self,
        index: usize,
    ) -> std::result::Result<Option<FrameStamp>, StatusFlushError> {
        let storage = self.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        storage
            .health()
            .map_err(|source| super::fatal(storage, super::Cause::Storage(source)))?;
        let control = &self.inner.inner.control;
        if control.installation.is_some()
            || control.base.installation.is_some()
            || control.base.terminal_pending
            || control.base.batch.is_some()
            || !ready(&self.inner.flush)
        {
            return Err(super::error(super::Cause::Busy, Effect::Unchanged));
        }
        self.inner
            .inner
            .pool
            .prepare_resident_slot(index)
            .map_err(|source| {
                let effect = source.effect;
                super::error(super::Cause::Frame(source), effect)
            })
    }
    pub(crate) fn load_into(
        &mut self,
        index: usize,
        page: PageRef,
    ) -> std::result::Result<StatusLoadObservation, StatusLoadError> {
        self.inner.load_status_at(index, page)
    }
}

pub(crate) struct ResidentStatusReader<'b, 'c, 's, 'p, 'd> {
    inner: StatusReader<'b, 'c, 's, 'p, 'd>,
    gate: &'b Mutex<()>,
}
/// Small admission precedes the generic return carrier and all wide arrays.
pub(crate) struct ResidentReadAdmission<'a, 'b, 'c, 's, 'p, 'd> {
    reader: &'a ResidentStatusReader<'b, 'c, 's, 'p, 'd>,
    owner: Option<&'a mut Owner<'c, 's>>,
    gate: Option<MutexGuard<'b, ()>>,
}
impl<'b, 'c, 's, 'p, 'd> ResidentStatusReader<'b, 'c, 's, 'p, 'd> {
    pub(crate) fn try_admit<'a>(
        &'a self,
        owner: &'a mut Owner<'c, 's>,
    ) -> Result<ResidentReadAdmission<'a, 'b, 'c, 's, 'p, 'd>> {
        if self.inner.fence.is_fenced() {
            return Err(error(
                Phase::Observe,
                Effect::CallerMustResolve,
                Cause::Frame(crate::storage::dirty_frame::DirtyError {
                    cause: crate::storage::dirty_frame::Cause::Fenced,
                    effect: Effect::CallerMustResolve,
                }),
            ));
        }
        if !owner
            .check_binding(self.inner.pool.bound_core())
            .map_err(|source| frame_error(Phase::Observe, source.into()))?
        {
            return Err(error(
                Phase::Observe,
                Effect::Unchanged,
                Cause::BindingMismatch,
            ));
        }
        let gate = match self.gate.try_lock() {
            Ok(gate) => gate,
            Err(TryLockError::WouldBlock) => {
                return Err(error(
                    Phase::Observe,
                    Effect::Unchanged,
                    Cause::Frame(crate::storage::dirty_frame::DirtyError {
                        cause: crate::storage::dirty_frame::Cause::Busy,
                        effect: Effect::Unchanged,
                    }),
                ));
            }
            Err(TryLockError::Poisoned(error_guard)) => {
                self.inner.fence.fence();
                drop(error_guard.into_inner());
                return Err(error(
                    Phase::Observe,
                    Effect::CallerMustResolve,
                    Cause::Frame(crate::storage::dirty_frame::DirtyError {
                        cause: crate::storage::dirty_frame::Cause::Poisoned,
                        effect: Effect::CallerMustResolve,
                    }),
                ));
            }
        };
        Ok(ResidentReadAdmission {
            reader: self,
            owner: Some(owner),
            gate: Some(gate),
        })
    }
}
impl<'a, 'b, 'c, 's, 'p, 'd> ResidentReadAdmission<'a, 'b, 'c, 's, 'p, 'd> {
    pub(crate) fn read_set<const N: usize>(
        mut self,
        pages: &[PageRef],
    ) -> Result<ReadStatusSet<'a, 'b, 'c, 's, 'p, 'd, N>> {
        const {
            assert!(N >= 2 && N <= 64);
        }
        let reader = &self.reader.inner;
        if pages.len() != N - 1 {
            return Err(error(Phase::Observe, Effect::Unchanged, Cause::Capacity));
        }
        let allocator = reader
            .pool
            .target_request(0)
            .map_err(|source| frame_error(Phase::Observe, source))?;
        let mut requests = [allocator; N];
        for (request, page) in requests[1..].iter_mut().zip(pages) {
            *request = TargetRequest {
                identity: PageIdentity {
                    database: reader.role.namespace().database,
                    page: *page,
                    owner: reader.role.object(),
                    owner_generation: reader.role.generation(),
                },
                kind: reader.role.kind(),
                level: reader.role.level(),
            };
        }
        let targets = reader
            .pool
            .reserve_target_set(self.owner.take().expect("owned admission"), &requests)
            .map_err(|source| frame_error(Phase::Observe, source))?
            .try_read()
            .map_err(|source| frame_error(Phase::Observe, source))?;
        for position in 1..N {
            if let Err(source) = reader.validate_pair(&targets, position) {
                return merge_cleanup(
                    Err(source),
                    targets
                        .release_checked()
                        .map_err(|source| frame_error(Phase::Release, source)),
                );
            }
        }
        Ok(ReadStatusSet {
            targets: Some(targets),
            gate: self.gate.take(),
            fence: reader.fence,
        })
    }
}
impl Drop for ResidentReadAdmission<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.reader.inner.fence.fence();
        }
        // No wide pins/guards remain here on failed construction.
    }
}
pub(crate) struct ReadStatusSet<'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    targets: Option<ReadTargets<'a, 'b, 'c, 's, 'p, 'd, N>>,
    gate: Option<MutexGuard<'b, ()>>,
    fence: &'s SessionFence,
}
impl<const N: usize> ReadStatusSet<'_, '_, '_, '_, '_, '_, N> {
    pub(crate) fn bytes(&self, position: usize) -> Result<&[u8; PAGE_BYTES]> {
        self.targets
            .as_ref()
            .expect("live read set")
            .bytes(position)
            .map_err(|source| frame_error(Phase::Observe, source))
    }
    pub(crate) fn stamp(&self, position: usize) -> Result<FrameStamp> {
        self.targets
            .as_ref()
            .expect("live read set")
            .observe(position)
            .map(|value| value.frame)
            .map_err(|source| frame_error(Phase::Observe, source))
    }
    pub(crate) fn request(&self, position: usize) -> Result<TargetRequest> {
        self.targets
            .as_ref()
            .expect("live read set")
            .target_request(position)
            .map_err(|source| frame_error(Phase::Observe, source))
    }
    fn release(&mut self) -> Result<()> {
        if std::thread::panicking() {
            self.fence.fence();
        }
        let result = self.targets.take().map_or(Ok(()), |targets| {
            targets
                .release_checked()
                .map_err(|source| frame_error(Phase::Release, source))
        });
        self.gate.take();
        result
    }
    pub(crate) fn release_checked(mut self) -> Result<()> {
        self.release()
    }
}
impl<const N: usize> Drop for ReadStatusSet<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

// Source-value accounting; page loans and the original flush workspace remain
// caller payload charges. This is not a compiler-stack or process-RSS claim.
const fn activation_bytes<W: WriterNamespace>() -> usize {
    type B<'a, W> = ResidentStatusBinding<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type R<'a, W> = ResidentStatus<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    std::mem::size_of::<B<'_, W>>()
        + std::mem::size_of::<std::result::Result<R<'_, W>, IntoFlushError<B<'_, W>>>>()
        + std::mem::size_of::<
            std::result::Result<
                BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>,
                IntoFlushError<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>,
            >,
        >()
        + 2048
}
struct ActivationLayout<W>(std::marker::PhantomData<W>);
impl<W: WriterNamespace> ActivationLayout<W> {
    const CHECK: () = assert!(activation_bytes::<W>() <= 65536);
}
fn wide_call_bytes() -> usize {
    use std::mem::size_of;
    // Request array, native all-target chain, outer return/move carriers and
    // typed page observer. Gate serializes all these working values.
    64 * size_of::<TargetRequest>()
        + crate::storage::dirty_frame::target_set_call_bytes::<64>()
        + 2 * size_of::<Result<ReadStatusSet<'_, '_, '_, '_, '_, '_, 64>>>()
        + size_of::<ReadStatusSet<'_, '_, '_, '_, '_, '_, 64>>()
        + super::super::operation::reader_bytes()
        + 1024
}
fn metadata_peaks<W: WriterNamespace>() -> (usize, usize, usize) {
    use std::mem::size_of;
    let retained = 60672
        + crate::storage::dirty_frame::init_pool_delta_bytes()
        + size_of::<super::super::StatusControl<'_, '_, '_>>()
            .saturating_sub(super::super::dirty_allocation::baseline_control_bytes())
        + size_of::<Mutex<()>>();
    // A retained success OR failure Result borrows one of the 34 owners.
    // The compiler probes establish this even for a known shape-error result.
    let pending = size_of::<Result<ReadStatusSet<'_, '_, '_, '_, '_, '_, 64>>>()
        + 2 * size_of::<Result<ResidentReadAdmission<'_, '_, '_, '_, '_, '_>>>()
        + 128;
    let read_phase = retained + 34 * pending + wide_call_bytes() + 8192 + 3648;
    // During heavy producer preparation, the writer and working-wide caller
    // consume two owners, leaving at most 32 retained result carriers. If no
    // wide call is active, its whole-chain allowance covers a 33rd result.
    // Do not rely on content-latch exclusion to omit codec/producer temporaries.
    let prepare_phase = retained + 32 * pending + wide_call_bytes() + 8192 + 3648 + 4096 + 16384;
    let hot = read_phase.max(prepare_phase);
    let load = super::operational::load::metadata_bytes::<W>().0 + size_of::<Mutex<()>>();
    (hot, load, hot.max(load) + 65536)
}
pub(in super::super) fn fits<W: WriterNamespace>() -> bool {
    let (hot, load, whole) = metadata_peaks::<W>();
    hot <= 196608
        && load <= 196608
        && whole <= 262144
        && activation_bytes::<W>() <= 65536
        && wide_call_bytes() <= 65536
        && wide_call_bytes()
            >= std::mem::size_of::<Result<ReadStatusSet<'_, '_, '_, '_, '_, '_, 64>>>()
                + 2 * std::mem::size_of::<Result<ResidentReadAdmission<'_, '_, '_, '_, '_, '_>>>()
                + 128
        && super::fits::<W>()
        && super::super::fits::<W>()
        && super::operational::load::fits::<W>()
}

#[cfg(test)]
#[path = "resident_status_tests.rs"]
mod tests;

pub(in crate::storage) use empty_transactions::EmptyTransactionInstall;
pub(crate) use empty_transactions::EmptyTransactionObservation;
