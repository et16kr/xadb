//! The original READY owner binds its metadata loan to the accepted source.
//! A raw sequence or a parsed Root outside this path cannot issue ReadyFrontier.
use super::super::{Plan4Retention, RetentionCause, RetentionPhase, WriterNamespace, history};
use super::*;

fn error(error: VisibilityError) -> super::super::RetentionError {
    super::super::RetentionError {
        phase: RetentionPhase::Activate,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}
impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Called once on the actual fresh/recovered READY instance. Its original
    /// mutable borrow excludes both installers and split readers until the
    /// checked source and replay loan have been released.
    pub(in crate::storage) fn enable_catalog_visibility(
        &mut self,
        owner: &mut Owner<'c, 's>,
        loan: VisibilityLoan<'a>,
    ) -> super::super::Result<()> {
        let storage = self.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        super::super::health(storage, &self.inner.pool)?;
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| super::super::frame(RetentionPhase::Activate, e))?;
        if self.catalog_visibility.is_some() || self.control.pending.is_some() {
            return Err(super::super::unchanged(
                RetentionPhase::Activate,
                RetentionCause::WrongState,
            ));
        }
        let cut = self.history.source_cut()?;
        cut.check_origin(storage)?;
        if cut.requirement().is_some() {
            return Err(super::super::unchanged(
                RetentionPhase::Activate,
                RetentionCause::WrongState,
            ));
        }
        let mut scope = self.history.replay_scope(owner)?;
        let result = match scope.checkout() {
            Ok(mut replay) => {
                let checked = (|| {
                    history::validate_live(storage, &self.control, replay.bytes_mut(), cut.seed())?;
                    cut.unchanged()?;
                    let (_, root) = crate::storage::retention_page::catalog::decode_root_page(
                        &replay.bytes()[crate::storage::page::PAGE_BYTES
                            ..2 * crate::storage::page::PAGE_BYTES],
                        storage.profile().spec().namespace,
                    )
                    .map_err(|e| {
                        super::super::fatal(
                            storage,
                            RetentionPhase::Activate,
                            RetentionCause::Retention(e),
                        )
                    })?;
                    Ok(root.commit_c())
                })();
                super::super::merge(checked, replay.return_checked())
            }
            Err(error) => Err(error),
        };
        let q = super::super::merge(result, scope.finish_checked())?;
        let ready = ReadyFrontier {
            origin: Origin {
                core: self.inner.pool.bound_core(),
                binding: &storage.binding,
                fence: storage.session_fence(),
            },
            q,
        };
        self.catalog_visibility = Some(BoundVisibility::bind(loan, ready).map_err(error)?);
        Ok(())
    }
}

/// Nested source values of the original source-to-Q binding. The retained
/// Plan4Retention and caller backing are charged by the whole owner census.
pub(in super::super) fn source_bytes<W: WriterNamespace>() -> usize {
    use super::super::{Result, RetentionError};
    use std::mem::size_of;
    let wrapper = size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<history::runtime::LookupSourceCut<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 3 * size_of::<Result<u64>>()
        + 3 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 8 * size_of::<&()>();
    let replay = size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>()
        + history::source_bytes::<W>().peak;
    let root = size_of::<crate::storage::page::PageHeader>()
        + size_of::<crate::storage::retention_page::catalog::RootView<'_>>()
        + size_of::<
            std::result::Result<
                (
                    crate::storage::page::PageHeader,
                    crate::storage::retention_page::catalog::RootView<'_>,
                ),
                crate::storage::retention_page::RetentionError,
            >,
        >()
        + crate::storage::retention_page::catalog::source_bytes();
    let publish = size_of::<VisibilityLoan<'_>>()
        + size_of::<ReadyFrontier<'_, '_>>()
        + size_of::<BoundVisibility<'_, '_, '_>>()
        + size_of::<VisibilityResult<BoundVisibility<'_, '_, '_>>>()
        + super::source_census().peak_leaf;
    wrapper + history::runtime::source_bytes() + replay.max(root).max(publish)
}
