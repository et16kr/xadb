//! The original Owner's finite Catalog call, including borrowed result use.
//! This is an accounting/progress lifetime, never a snapshot or page authority.
use super::super::source::{OperationClass, OperationSourceAdmission, SourceFailure};
use super::super::visibility::{self, SnapshotGuard};
use super::*;
use crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::HealthFailure;
use crate::transaction::concurrency::{
    ConcurrencyError, ReadOperationAdmission, read_operation_source_bytes,
};

/// Finite entry/close failure. Conversion constructs the wide legacy error only
/// outside the compact caller boundary; this value carries no authority.
#[derive(Debug)]
pub(crate) struct CatalogCallError(CallFailure);
#[derive(Debug)]
enum CallFailure {
    PrepareStorage(HealthFailure),
    PrepareFrame(dirty_frame::DirtyError),
    PrepareBinding,
    CensusFrame(dirty_frame::DirtyError),
    WrongState,
    Visibility(visibility::VisibilityError),
    Progress(ConcurrencyError),
    Source(SourceFailure),
}
type CallResult<T> = std::result::Result<T, CatalogCallError>;
impl CatalogCallError {
    fn effect(&self) -> Effect {
        match &self.0 {
            CallFailure::PrepareStorage(_) | CallFailure::PrepareBinding => {
                Effect::CallerMustResolve
            }
            CallFailure::PrepareFrame(e) | CallFailure::CensusFrame(e) => e.effect,
            CallFailure::WrongState => Effect::Unchanged,
            CallFailure::Visibility(e) => e.effect,
            CallFailure::Progress(e) => e.effect,
            CallFailure::Source(e) => e.effect,
        }
    }
}
impl From<CatalogCallError> for RetentionError {
    fn from(error: CatalogCallError) -> Self {
        let effect = error.effect();
        let (phase, cause) = match error.0 {
            CallFailure::PrepareStorage(e) => {
                (RetentionPhase::Prepare, RetentionCause::Storage(e.into()))
            }
            CallFailure::PrepareFrame(e) => (RetentionPhase::Prepare, RetentionCause::Frame(e)),
            CallFailure::PrepareBinding => {
                (RetentionPhase::Prepare, RetentionCause::BindingMismatch)
            }
            CallFailure::CensusFrame(e) => (RetentionPhase::Census, RetentionCause::Frame(e)),
            CallFailure::WrongState => (RetentionPhase::Census, RetentionCause::WrongState),
            CallFailure::Visibility(e) => (RetentionPhase::Census, RetentionCause::Visibility(e)),
            CallFailure::Progress(e) => (RetentionPhase::Census, RetentionCause::Concurrency(e)),
            CallFailure::Source(e) => return e.into(),
        };
        Self {
            phase,
            cause,
            effect,
        }
    }
}
fn compact_merge<T>(result: CallResult<T>, cleanup: CallResult<()>) -> CallResult<T> {
    match (result, cleanup) {
        (Err(e), _) if e.effect() == Effect::CallerMustResolve => Err(e),
        (_, Err(e)) => Err(e),
        (result, Ok(())) => result,
    }
}
fn compact_health<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    pool: &SharedPool<'_, '_, '_, '_>,
) -> CallResult<()> {
    storage.health_compact().map_err(|e| {
        // Match original plan4 health/fatal: notify at the failure, not when
        // a legacy caller eventually converts the small error.
        storage.session_fence().fence();
        storage.wal().completion.notify_all();
        CatalogCallError(CallFailure::PrepareStorage(e))
    })?;
    if !pool
        .retention_payloads()
        .map_err(|e| CatalogCallError(CallFailure::PrepareFrame(e)))?
        || !pool.uses_storage_origin(&storage.binding, storage.session_fence())
    {
        storage.session_fence().fence();
        storage.wal().completion.notify_all();
        return Err(CatalogCallError(CallFailure::PrepareBinding));
    }
    Ok(())
}

pub(crate) struct CatalogCallScope<
    'u,
    'r,
    'v,
    'j,
    'b,
    'a,
    's,
    'c,
    'p,
    'd,
    'm,
    'w,
    W: WriterNamespace,
> {
    backend: &'r ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    owner: &'u mut Owner<'c, 's>,
    snapshot: &'r SnapshotGuard<'v, 'j, 'c, 's>,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
    progress: ReadOperationAdmission<'c, 's>,
}

fn visibility_error(error: visibility::VisibilityError) -> CatalogCallError {
    CatalogCallError(CallFailure::Visibility(error))
}
fn progress_error(error: ConcurrencyError) -> CatalogCallError {
    CatalogCallError(CallFailure::Progress(error))
}

impl<'u, 'r, 'v, 'j, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogCallScope<'u, 'r, 'v, 'j, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn begin(
        backend: &'r ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
        owner: &'u mut Owner<'c, 's>,
        snapshot: &'r SnapshotGuard<'v, 'j, 'c, 's>,
        class: OperationClass,
    ) -> CallResult<Self> {
        compact_health(backend.storage, backend.pool)?;
        backend
            .pool
            .check_retention_reader(owner)
            .map_err(|e| CatalogCallError(CallFailure::CensusFrame(e)))?;
        let visibility = backend
            .visibility
            .ok_or(CatalogCallError(CallFailure::WrongState))?;
        if !snapshot.confirms(visibility).map_err(visibility_error)? {
            return Err(CatalogCallError(CallFailure::WrongState));
        }
        owner
            .check_read_operation(backend.pool.bound_core())
            .map_err(progress_error)?;
        // Original progress precedes the fixed byte charge. Neither lock is
        // retained across the other leaf and neither path waits for capacity.
        let progress = owner
            .try_read_operation(backend.pool.bound_core())
            .map_err(progress_error)?;
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            backend.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeSourceReserve,
        );
        let source = match OperationSourceAdmission::catalog(
            backend.isolation,
            &backend.source_charges,
            class,
        ) {
            Ok(source) => source,
            Err(error) => {
                return compact_merge(
                    Err(CatalogCallError(CallFailure::Source(error))),
                    progress.finish_checked().map_err(progress_error),
                );
            }
        };
        Ok(Self {
            backend,
            owner,
            snapshot,
            source,
            progress,
        })
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "selected actual Catalog caller precedes SQL integration"
        )
    )]
    /// Result views (including copied views and nested strings) borrow this
    /// scope. A later lookup or consuming finish requires their last use.
    pub(in crate::storage) fn lookup<'call>(
        &'call mut self,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'call mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'call>> {
        crate::storage::catalog::read::lookup(self.operation(private)?, object, cursor)
    }

    /// Only the legacy synchronous wrapper may detach the ordinary result's
    /// borrow from this scope. Its post-return caller use has no scope guarantee.
    pub(super) fn operation<'call, 'i>(
        &'call mut self,
        private: &'i mut [u8; 425_984],
    ) -> Result<CatalogReadOperation<'call, 'call, 'v, 'j, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>>
    {
        CatalogReadOperation::begin(
            self.backend,
            &mut *self.owner,
            self.snapshot,
            &self.source,
            private,
        )
    }

    /// Complete a normal error path without losing a fatal cleanup error.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "selected actual Catalog caller precedes SQL integration"
        )
    )]
    pub(in crate::storage) fn finish_error(self, error: RetentionError) -> RetentionError {
        match self.finish_result::<()>(Err(error)) {
            Err(error) => error,
            Ok(()) => unreachable!("original error retained"),
        }
    }

    pub(super) fn finish_result<T>(self, result: Result<T>) -> Result<T> {
        merge(result, self.finish_checked().map_err(Into::into))
    }

    pub(in crate::storage) fn finish_checked(self) -> CallResult<()> {
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.backend.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeSourceReturn,
        );
        // The pair preserves source-before-progress unwind order even after
        // discarding backend/snapshot/Owner borrows. No hook/fallible leaf runs
        // between the destructure and consuming source close.
        let Self {
            source, progress, ..
        } = self;
        let cleanup = (source, progress);
        let source = cleanup
            .0
            .finish_checked()
            .map_err(|e| CatalogCallError(CallFailure::Source(e)));
        let progress = cleanup.1.finish_checked().map_err(progress_error);
        compact_merge(source, progress)
    }
}

// Ordinary struct field Drop closes source before progress, including unwind.
// Explicit finish consumes both in the same order and reports fatal cleanup.

pub(super) fn held_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    size_of::<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 4 * size_of::<CallResult<()>>()
        + 4 * size_of::<&()>()
}

pub(super) fn entry_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    2 * size_of::<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<CallResult<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>(
        )
        + 4 * size_of::<CallResult<()>>()
        + 16 * size_of::<&()>()
        + read_operation_source_bytes()
        + super::super::source::leaf_source_bytes()
}

/// One consuming tail, not a bound on simultaneous pre/post-admission calls.
/// Source close's result survives progress close; the tuple is counted whole.
pub(super) fn close_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(
        OperationSourceAdmission<'_, '_, '_, '_>,
        ReadOperationAdmission<'_, '_>,
    )>() + 2 * size_of::<std::result::Result<(), SourceFailure>>()
        + 2 * size_of::<std::result::Result<(), ConcurrencyError>>()
        + 4 * size_of::<CallResult<()>>()
        + 2 * size_of::<CatalogCallError>()
        + 4 * size_of::<&()>()
        + super::super::source::leaf_source_bytes().max(read_operation_source_bytes())
}

/// Legacy wrappers still retain wide input/output errors and lookup Results.
/// These values are excluded only from the selected compact entry/close API,
/// never from the legacy one-call charge. Arbitrary returned caller copies are
/// outside either finite selected caller's proof.
pub(super) fn legacy_boundary_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    let conversion = 2 * size_of::<CatalogCallError>()
        + 2 * size_of::<RetentionError>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<Result<CatalogLookupStep<'_>>>()
        + 8 * size_of::<&()>();
    conversion + entry_source_bytes::<W>().max(close_source_bytes())
}

/// Finite selected caller: query's result/header/table/column inspection and
/// copied-view inspection. This phase starts after Operation has returned.
/// Arbitrary caller copies, foreign pre-entry rejection and release tails are
/// not globally bounded by this per-admitted-call expression.
pub(super) fn consumer_source_bytes<W: WriterNamespace>() -> usize {
    use crate::storage::catalog_record::{
        CatalogColumn, CatalogDefinitionView, CatalogError, CatalogHeader, CatalogRecordView,
        TableDefinitionView,
    };
    use std::mem::size_of;
    held_source_bytes::<W>()
        + 3 * size_of::<CatalogLookupStep<'_>>()
        + 2 * size_of::<Result<CatalogLookupStep<'_>>>()
        + 3 * size_of::<CatalogRecordView<'_>>()
        + 2 * size_of::<CatalogHeader>()
        + 2 * size_of::<CatalogDefinitionView<'_>>()
        + 2 * size_of::<TableDefinitionView<'_>>()
        + 2 * size_of::<CatalogColumn<'_>>()
        + 2 * size_of::<std::result::Result<CatalogColumn<'_>, CatalogError>>()
        + 3 * size_of::<Plan4Target>()
        + 2 * size_of::<Option<Plan4Target>>()
        + 2 * size_of::<Plan4ResidencyCursor>()
        + 4 * size_of::<RetentionError>()
        + 6 * size_of::<Result<()>>()
        + 16 * size_of::<&()>()
        + 16 * size_of::<u64>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        dirty_frame::DirtyFrame,
        wal_frame::MAX_FRAME_BYTES,
        wal_io::{FreshStorageSession, plan4_test_support as port},
    };
    use crate::transaction::concurrency::{Cause, OwnerRole, PageConcurrency};
    use foundation::namespace::DirectoryPort;

    fn boxed<const N: usize>() -> Box<[u8; N]> {
        vec![0; N].into_boxed_slice().try_into().unwrap()
    }

    #[test]
    fn compact_health_keeps_poison_before_fence_and_original_phase() {
        for poison in [false, true] {
            let directory = port::Directory::fresh();
            let mut slot = boxed::<MAX_FRAME_BYTES>();
            let mut maintenance = boxed::<12_582_912>();
            let mut dwb = boxed::<4_194_304>();
            let storage = FreshStorageSession::create_retention_catalog(
                directory.acquire_writer().unwrap(),
                1,
                &mut slot,
                &mut maintenance,
            )
            .unwrap()
            .install_doublewrite(&mut dwb)
            .unwrap()
            .into_mutable_storage();
            let core = PageConcurrency::new(storage.session_fence()).unwrap();
            let _owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut backing = vec![[0; PAGE_BYTES]; 4];
            let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
            let mut operation = boxed::<114_688>();
            let pages = storage
                .bind_plan4_pages(&core, &mut frames, &mut operation)
                .unwrap();
            if poison {
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _append = storage.wal().append.lock().unwrap();
                        panic!("poison original WAL append before compact health");
                    }))
                    .is_err()
                );
            }
            storage.session_fence().fence();
            let compact = compact_health(&storage, &pages.pool).unwrap_err();
            let error: RetentionError = compact.into();
            assert_eq!(error.phase, RetentionPhase::Prepare);
            assert_eq!(error.effect, Effect::CallerMustResolve);
            let RetentionCause::Storage(error) = error.cause else {
                panic!("storage health precedes pool")
            };
            assert!(matches!((poison, error.cause),
                (true, crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::Cause::Poisoned)
                | (false, crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::Cause::Fenced)));
            assert!(storage.session_fence().is_fenced());
            assert_eq!(storage.wal().append.is_poisoned(), poison);
        }
    }

    #[test]
    fn compact_capacity_yields_to_original_progress_fatal() {
        use super::super::super::source::{SourceCause, SourceFailure};
        let capacity = || {
            CatalogCallError(CallFailure::Source(SourceFailure {
                cause: SourceCause::Capacity,
                effect: Effect::Unchanged,
            }))
        };
        let original = || {
            CatalogCallError(CallFailure::Source(SourceFailure {
                cause: SourceCause::Invariant,
                effect: Effect::CallerMustResolve,
            }))
        };
        let progress = || {
            Err(progress_error(ConcurrencyError {
                cause: Cause::Poisoned,
                effect: Effect::CallerMustResolve,
            }))
        };
        let error: RetentionError = compact_merge::<()>(Err(capacity()), progress())
            .unwrap_err()
            .into();
        assert_eq!(error.phase, RetentionPhase::Census);
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(
            matches!(error.cause, RetentionCause::Concurrency(e) if e.cause == Cause::Poisoned)
        );
        let error: RetentionError = compact_merge::<()>(Err(original()), progress())
            .unwrap_err()
            .into();
        assert!(matches!(error.cause, RetentionCause::Invariant));
        let error: RetentionError = compact_merge::<()>(Err(capacity()), Ok(()))
            .unwrap_err()
            .into();
        assert!(matches!(error.cause, RetentionCause::Capacity));
        assert_eq!(error.effect, Effect::Unchanged);
    }

    #[test]
    fn compact_call_counts_whole_return_and_consuming_pair() {
        use std::mem::size_of;
        type Scope<'a> =
            CatalogCallScope<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, port::Writer>;
        let scope = size_of::<Scope<'_>>();
        let result = size_of::<CallResult<Scope<'_>>>();
        let error = size_of::<CatalogCallError>();
        let pair = size_of::<(
            OperationSourceAdmission<'_, '_, '_, '_>,
            ReadOperationAdmission<'_, '_>,
        )>();
        println!(
            "COMPACT_CALL scope={scope} result_scope={result} error={error} source_failure={} progress={} cleanup_pair={pair} entry={} held={} consumer={} close={} legacy={} source_leaf={} full={} resident={} shared={}",
            size_of::<SourceFailure>(),
            size_of::<ReadOperationAdmission<'_, '_>>(),
            entry_source_bytes::<port::Writer>(),
            held_source_bytes::<port::Writer>(),
            consumer_source_bytes::<port::Writer>(),
            close_source_bytes(),
            legacy_boundary_source_bytes::<port::Writer>(),
            super::super::super::source::leaf_source_bytes(),
            super::super::lookup_source_bytes::<port::Writer>(),
            super::super::resident_lookup_source_bytes::<port::Writer>(),
            super::super::super::source::shared_bytes::<port::Writer>()
        );
        assert!(result >= scope);
        assert!(error < size_of::<RetentionError>());
        assert_eq!(
            pair,
            size_of::<OperationSourceAdmission<'_, '_, '_, '_>>()
                + size_of::<ReadOperationAdmission<'_, '_>>()
        );
        assert!(entry_source_bytes::<port::Writer>() >= result + pair);
    }
}
