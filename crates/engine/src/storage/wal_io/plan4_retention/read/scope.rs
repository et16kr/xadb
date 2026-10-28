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
/// No pool/visibility registry guard is acquired before original admission.
fn compact_entry_health<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    pool: &SharedPool<'_, '_, '_, '_>,
) -> CallResult<()> {
    storage.health_compact().map_err(|e| {
        storage.session_fence().fence();
        storage.wal().completion.notify_all();
        CatalogCallError(CallFailure::PrepareStorage(e))
    })?;
    pool.check_retention_entry_health()
        .map_err(|e| CatalogCallError(CallFailure::PrepareFrame(e)))?;
    if !pool.uses_storage_origin(&storage.binding, storage.session_fence()) {
        storage.session_fence().fence();
        storage.wal().completion.notify_all();
        return Err(CatalogCallError(CallFailure::PrepareBinding));
    }
    Ok(())
}
fn compact_health<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    pool: &SharedPool<'_, '_, '_, '_>,
) -> CallResult<()> {
    compact_entry_health(storage, pool)?;
    if !pool
        .retention_payloads()
        .map_err(|e| CatalogCallError(CallFailure::PrepareFrame(e)))?
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

/// Finish the original ticket before constructing the much larger Err<Scope>
/// return carrier. Both failures stay small throughout the blocking cleanup.
fn finish_failed_source(
    error: SourceFailure,
    progress: ReadOperationAdmission<'_, '_>,
) -> CatalogCallError {
    let cleanup = progress.finish_checked().map_err(progress_error);
    match compact_merge::<()>(Err(CatalogCallError(CallFailure::Source(error))), cleanup) {
        Err(error) => error,
        Ok(()) => unreachable!("original source failure retained"),
    }
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
        compact_entry_health(backend.storage, backend.pool)?;
        // Keep the original Frame projection for foreign/malformed Owners.
        // These core checks grant no page or snapshot authority.
        if !owner
            .check_binding(backend.pool.bound_core())
            .map_err(|e| CatalogCallError(CallFailure::CensusFrame(e.into())))?
        {
            return Err(CatalogCallError(CallFailure::CensusFrame(
                dirty_frame::DirtyError {
                    cause: dirty_frame::Cause::BindingMismatch,
                    effect: Effect::Unchanged,
                },
            )));
        }
        let visibility = backend
            .visibility
            .ok_or(CatalogCallError(CallFailure::WrongState))?;
        if !snapshot
            .confirms_origin(visibility)
            .map_err(visibility_error)?
        {
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
                return Err(finish_failed_source(error, progress));
            }
        };
        let scope = Self {
            backend,
            owner,
            snapshot,
            source,
            progress,
        };
        // Admission rejection precedes unobserved metadata errors. Successful
        // admission retains its fixed charge through every original validator.
        if let Err(error) = scope.validate() {
            return Err(scope.finish_entry_error(error));
        }
        Ok(scope)
    }

    fn validate(&self) -> CallResult<()> {
        compact_health(self.backend.storage, self.backend.pool)?;
        self.backend
            .pool
            .check_retention_reader(self.owner)
            .map_err(|e| CatalogCallError(CallFailure::CensusFrame(e)))?;
        let visibility = self
            .backend
            .visibility
            .ok_or(CatalogCallError(CallFailure::WrongState))?;
        if !self
            .snapshot
            .confirms(visibility)
            .map_err(visibility_error)?
        {
            return Err(CatalogCallError(CallFailure::WrongState));
        }
        Ok(())
    }

    fn finish_entry_error(self, error: CatalogCallError) -> CatalogCallError {
        let cleanup = self.finish_checked();
        match compact_merge::<()>(Err(error), cleanup) {
            Err(error) => error,
            Ok(()) => unreachable!("original validation failure retained"),
        }
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
        crate::storage::catalog::read::lookup(self.operation(private)?, object, cursor, None)
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
        // Keep only the original wide error while cleanup may wait. Construct
        // a replacement wide error after the small cleanup result has returned.
        let cleanup = self.finish_checked();
        match cleanup {
            _ if error.effect == Effect::CallerMustResolve => error,
            Err(cleanup) => cleanup.into(),
            Ok(()) => error,
        }
    }

    pub(super) fn finish_result<T>(self, result: Result<T>) -> Result<T> {
        merge(result, self.finish_checked().map_err(Into::into))
    }

    #[cfg(test)]
    pub(super) fn test_record_core(
        &self,
    ) -> &'c crate::transaction::concurrency::PageConcurrency<'s> {
        self.backend.pool.bound_core()
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
        // Preserve both factory forwarding Results with the compact error type.
        + 2 * size_of::<CallResult<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>()
        + size_of::<OperationClass>()
        + 4 * size_of::<CallResult<()>>()
        + 16 * size_of::<&()>()
        + read_operation_source_bytes()
        + super::super::source::leaf_source_bytes()
}

/// Successful entry owns its source while validating both original registries.
/// Keep all three columns and both closures additively; this does not require
/// guards to overlap. The enclosing fixed class takes this phase's max.
pub(super) fn validation_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    let (pw, pg, pr) = dirty_frame::retention_entry_source_phases();
    let (vw, vg, vr) = visibility::entry_source_phases();
    2 * size_of::<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 8 * size_of::<CallResult<()>>()
        + 16 * size_of::<&()>()
        + pw
        + pg
        + pr
        + vw
        + vg
        + vr
        + crate::transaction::concurrency::read_operation_source_bytes()
        + close_source_bytes()
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
    fn admitted_validation_is_inside_both_fixed_source_classes() {
        let validation = validation_source_bytes::<port::Writer>();
        let full = super::super::lookup_source_bytes::<port::Writer>();
        let resident = super::super::resident_lookup_source_bytes::<port::Writer>();
        let (_, pool_guard, _) = dirty_frame::retention_entry_source_phases();
        let (_, visibility_guard, _) = visibility::entry_source_phases();
        println!(
            "ADMITTED_VALIDATION validation={validation} pool_guard={pool_guard} visibility_guard={visibility_guard} full={full} resident={resident} whole_floor=NOT_CONNECTED"
        );
        assert!(validation > pool_guard + visibility_guard);
        assert!(full >= validation && resident >= validation);
    }

    #[test]
    fn failed_source_consumes_original_ticket_and_keeps_error_priority() {
        use super::super::super::source::SourceCause;
        use crate::storage::session_fence::SessionFence;
        use crate::transaction::concurrency::catalog_test_support::observe;
        for (fatal_source, fenced_cleanup) in [(false, false), (false, true), (true, true)] {
            let fence = SessionFence::new();
            let core = PageConcurrency::new(&fence).unwrap();
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let progress = owner.try_read_operation(&core).unwrap();
            assert_eq!(observe(&core).active_read_operations, 1);
            if fenced_cleanup {
                fence.fence();
            }
            let error = finish_failed_source(
                SourceFailure {
                    cause: if fatal_source {
                        SourceCause::Invariant
                    } else {
                        SourceCause::Capacity
                    },
                    effect: if fatal_source {
                        Effect::CallerMustResolve
                    } else {
                        Effect::Unchanged
                    },
                },
                progress,
            );
            assert_eq!(observe(&core).active_read_operations, 0);
            let error: RetentionError = error.into();
            match (fatal_source, fenced_cleanup) {
                (true, _) => assert!(matches!(error.cause, RetentionCause::Invariant)),
                (false, true) => assert!(
                    matches!(error.cause, RetentionCause::Concurrency(e) if e.cause == Cause::Fenced)
                ),
                (false, false) => {
                    assert!(matches!(error.cause, RetentionCause::Capacity));
                    assert_eq!(error.effect, Effect::Unchanged);
                    owner.check_idle().unwrap();
                }
            }
        }
    }

    #[test]
    fn uncharged_call_phases_keep_independent_guards_and_wide_tail_separate() {
        use std::mem::size_of;
        type Scope<'a> =
            CatalogCallScope<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, port::Writer>;
        let (owners, core_wait, core_guard, core_return) =
            crate::transaction::concurrency::read_entry_source_observation();
        let (pool_wait, pool_guard, pool_return) = dirty_frame::retention_entry_source_phases();
        let (visibility_wait, visibility_guard, visibility_return) =
            visibility::entry_source_phases();
        let (source_wait, source_guard, source_return) =
            super::super::super::source::entry_source_observation();
        let guards = core_guard + pool_guard + visibility_guard + source_guard;
        // Factory/begin each have three references; begin also has visibility
        // and class. The progress ticket is kept separately until consumed.
        let outer = 7 * size_of::<&()>() + size_of::<OperationClass>();
        let progress = size_of::<ReadOperationAdmission<'_, '_>>();
        let leaf_wait = core_wait
            .max(pool_wait)
            .max(visibility_wait)
            .max(source_wait);
        let leaf_return = core_return
            .max(pool_return)
            .max(visibility_return)
            .max(source_return);
        // health_compact uses no guard: poison/atomic queries and all its small
        // error/return/argument values belong to each caller, not guards.
        let health = 8 * size_of::<&()>()
            + 4 * size_of::<std::result::Result<(), HealthFailure>>()
            + 4 * size_of::<CallResult<()>>()
            + 8 * size_of::<bool>();
        let entry = outer + progress + health.max(leaf_wait).max(leaf_return);
        let failed_source = outer
            + 2 * progress
            + size_of::<std::result::Result<OperationSourceAdmission<'_, '_, '_, '_>, SourceFailure>>(
            )
            + 2 * size_of::<SourceFailure>()
            + 4 * size_of::<CallResult<()>>()
            + 2 * size_of::<CatalogCallError>()
            + core_wait;
        let returning =
            outer + 2 * size_of::<CallResult<Scope<'_>>>() + 4 * size_of::<CatalogCallError>();
        let compact_tail = size_of::<(
            OperationSourceAdmission<'_, '_, '_, '_>,
            ReadOperationAdmission<'_, '_>,
        )>() + 4 * size_of::<&()>()
            + 4 * size_of::<CallResult<()>>()
            + 2 * size_of::<SourceFailure>()
            + 2 * size_of::<ConcurrencyError>()
            + 2 * size_of::<CatalogCallError>()
            + leaf_wait.max(leaf_return);
        let compact_prefix = entry.max(failed_source).max(returning).max(compact_tail);
        let compact_floor = owners * compact_prefix + guards;
        // Preserve the original broad baseline from c8d47ed. The normal query
        // now ends its consumer block before close, but its error branch still
        // closes inside that block. This remains an upper observation, not the
        // narrowed normal/error tail or an implemented reservation floor.
        let current_query_tail = consumer_source_bytes::<port::Writer>()
            + legacy_boundary_source_bytes::<port::Writer>()
            + close_source_bytes();
        let current_query_floor = owners * compact_prefix.max(current_query_tail) + guards;
        println!(
            "UNCHARGED_OBSERVATION owners={owners} core={core_wait}/{core_guard}/{core_return} pool={pool_wait}/{pool_guard}/{pool_return} visibility={visibility_wait}/{visibility_guard}/{visibility_return} isolation={source_wait}/{source_guard}/{source_return} guards_sum={guards} entry={entry} failed_source={failed_source} return={returning} compact_tail={compact_tail} compact_prefix={compact_prefix} compact_floor={compact_floor} current_query_tail={current_query_tail} current_query_floor={current_query_floor} shared={} full={} resident={} admission=NOT_CONNECTED",
            super::super::super::source::shared_bytes::<port::Writer>(),
            super::super::lookup_source_bytes::<port::Writer>(),
            super::super::resident_lookup_source_bytes::<port::Writer>(),
        );
        assert_eq!(
            owners, 34,
            "32 foreground + maintenance + flusher rejection"
        );
        assert!(
            guards
                > core_guard
                    .max(pool_guard)
                    .max(visibility_guard)
                    .max(source_guard)
        );
        assert!(current_query_tail > compact_tail);
        assert!(current_query_floor > compact_floor);
        assert!(size_of::<CatalogCallError>() < size_of::<CallResult<Scope<'_>>>());
    }

    #[test]
    fn selected_query_tails_keep_normal_packet_and_error_conversion_separate() {
        use std::mem::size_of;
        // Normal query ends the lookup/result/record inspection block first.
        // Target output/extraction and the remaining loop inputs are explicit;
        // original ports/Owner/snapshot remain their separately owned values.
        let loop_values = 2 * size_of::<Plan4ResidencyCursor>()
            + 2 * size_of::<std::ops::Range<usize>>()
            + 4 * size_of::<u64>()
            + 8 * size_of::<&()>()
            + 4 * size_of::<bool>();
        let normal = loop_values + 2 * size_of::<Option<Plan4Target>>() + close_source_bytes();
        // finish_error keeps the original RetentionError during close. After
        // close, From can construct its phase/cause/effect and wide return;
        // these phases are exclusive but original error survives either one.
        let conversion = 2 * size_of::<CatalogCallError>()
            + 2 * size_of::<RetentionError>()
            + size_of::<(RetentionPhase, RetentionCause)>()
            + size_of::<Effect>()
            + 2 * size_of::<SourceFailure>()
            + 4 * size_of::<&()>();
        let finish_error = size_of::<RetentionError>()
            + (4 * size_of::<CallResult<()>>() + close_source_bytes()).max(conversion);
        // The failure arm is inside the block: its lookup Result cannot be
        // removed with the normal branch. Preserve complete return/extraction
        // and the extracted original error beside the finishing call.
        let error = loop_values
            + 2 * size_of::<Result<CatalogLookupStep<'_>>>()
            + size_of::<RetentionError>()
            + finish_error;
        let old_broad = consumer_source_bytes::<port::Writer>()
            + legacy_boundary_source_bytes::<port::Writer>()
            + close_source_bytes();
        println!(
            "QUERY_TAIL normal={normal} error={error} finish_error={finish_error} conversion={conversion} retained_error={} avoided_result_unit={} lookup_result={} target_option={} previous_broad={old_broad} floor=NOT_CONNECTED",
            size_of::<RetentionError>(),
            size_of::<Result<()>>(),
            size_of::<Result<CatalogLookupStep<'_>>>(),
            size_of::<Option<Plan4Target>>()
        );
        assert!(error > normal);
        assert!(normal < old_broad && error < old_broad);
        assert!(size_of::<CallResult<()>>() < size_of::<Result<()>>());
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
