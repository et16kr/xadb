//! The original Owner's finite Catalog call, including borrowed result use.
//! This is an accounting/progress lifetime, never a snapshot or page authority.
use super::super::source::{OperationClass, OperationSourceAdmission};
use super::super::visibility::{self, SnapshotGuard};
use super::*;
use crate::transaction::concurrency::{ReadOperationAdmission, read_operation_source_bytes};

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

fn visibility_error(error: visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}
fn progress_error(error: crate::transaction::concurrency::ConcurrencyError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Concurrency(error),
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
    ) -> Result<Self> {
        health(backend.storage, backend.pool)?;
        backend
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let visibility = backend
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::WrongState))?;
        if !snapshot.confirms(visibility).map_err(visibility_error)? {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        // Original progress precedes the fixed byte charge. Neither lock is
        // retained across the other leaf and neither path waits for capacity.
        let mut progress = owner
            .try_read_operation(backend.pool.bound_core())
            .map_err(progress_error)?;
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            backend.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeSourceReserve,
        );
        let source = match OperationSourceAdmission::catalog::<W>(backend.isolation, class) {
            Ok(source) => source,
            Err(error) => {
                return merge(
                    Err(error),
                    progress.release_checked().map_err(progress_error),
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

    fn close(&mut self) -> Result<()> {
        let source = self.source.release_checked();
        // Even a fatal source release must return the original progress flag.
        merge(
            source,
            self.progress.release_checked().map_err(progress_error),
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
        merge(result, self.finish_checked())
    }

    pub(in crate::storage) fn finish_checked(mut self) -> Result<()> {
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.backend.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeSourceReturn,
        );
        self.close()
    }
}

impl<W: WriterNamespace> Drop
    for CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>
{
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub(super) fn held_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    size_of::<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<&()>()
}

pub(super) fn entry_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    2 * size_of::<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<Result<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>()
        // begin returns through the selected ReadPort wrapper before caller use.
        + 2 * size_of::<Result<CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>()
        + size_of::<OperationClass>()
        + 4 * size_of::<Result<()>>()
        + 16 * size_of::<&()>()
        + read_operation_source_bytes()
        + super::super::source::leaf_source_bytes()
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
