//! Exact profile seam under the one Catalog scope2 transition body. A Format
//! selects only the Root/Job codecs, the complete Job census and the Root-bound
//! Aging descriptor; the Catalog transition semantics stay in the shared body.
//! Profile2 selects the original version2 codecs and Profile3 the separate
//! version3 codecs. There is no version rewriting, `>=` widening or retry of
//! another observer, and no view here grants WAL, pin or install authority.
use super::{Error, Result};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::SystemWorkRef,
        control::Profile,
        page::{PAGE_BYTES, PageHeader, PageRef, PageView},
        retention_obligation,
        retention_page::{
            JOB_BYTES, JOB_COUNT, ROOT_BYTES,
            catalog::{self, JobState},
            catalog3, row,
        },
        transaction_record::{StatusSlotRef, TxnRef},
        undo_ref::UndoRef,
    },
};
use foundation::identity::{Generation, ObjectHandle};

/// The exact formats this Catalog observer accepts. Other profiles keep the
/// original identity rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Selected {
    Catalog2,
    Row3,
}
pub(crate) fn select(profile: Profile) -> Result<Selected> {
    match profile.format() {
        2 => Ok(Selected::Catalog2),
        3 => Ok(Selected::Row3),
        _ => Err(Error::IdentityMismatch),
    }
}

/// Catalog meaning of one canonical Job cell decoded under one exact version.
pub(crate) trait CatalogJob<'a>: Copy {
    /// Profile, Root and Job version this view was decoded under.
    const VERSION: u16;
    fn bytes(self) -> &'a [u8; JOB_BYTES];
    fn state(self) -> JobState;
    fn generation(self) -> Generation;
    fn source(self) -> Option<TxnRef>;
    fn status(self) -> Option<StatusSlotRef>;
    fn undo(self) -> Option<UndoRef>;
    fn object(self) -> Option<ObjectHandle>;
    fn pair(self) -> Option<[PageRef; 2]>;
    fn lease_lsn(self) -> u64;
    fn outcome(self) -> u8;
    fn release_cursor(self) -> u8;
    fn undo_sequence(self) -> u64;
    fn statement(self) -> u64;
    fn mutation(self) -> u64;
    fn commit_q(self) -> u64;
    fn catalog_bytes(self) -> u32;
    fn projection_digest(self) -> &'a [u8; 32];
    fn last_step(self) -> u64;
}

/// Root fields read by the shared queue transition.
pub(crate) trait CatalogRoot<'a>: Copy {
    fn bytes(self) -> &'a [u8; ROOT_BYTES];
    fn aging(self) -> Option<SystemWorkRef>;
    fn revision(self) -> u64;
    fn cursor(self) -> u64;
    fn apply(self) -> u64;
    fn issued(self) -> u64;
    fn completed(self) -> u64;
    fn canceled(self) -> u64;
    fn undo_high(self) -> u64;
    fn object_high(self) -> u64;
    fn commit_c(self) -> u64;
    fn consumed_wal(self) -> u64;
}

/// Both views expose the same Catalog accessor names; only the exact version
/// they were decoded under differs.
macro_rules! catalog_job {
    ($view:ident, $version:literal) => {
        impl<'a> CatalogJob<'a> for $view::JobView<'a> {
            const VERSION: u16 = $version;
            catalog_job! {
                @accessors $view,
                bytes -> &'a [u8; JOB_BYTES],
                state -> JobState,
                generation -> Generation,
                source -> Option<TxnRef>,
                status -> Option<StatusSlotRef>,
                undo -> Option<UndoRef>,
                object -> Option<ObjectHandle>,
                pair -> Option<[PageRef; 2]>,
                lease_lsn -> u64,
                outcome -> u8,
                release_cursor -> u8,
                undo_sequence -> u64,
                statement -> u64,
                mutation -> u64,
                commit_q -> u64,
                catalog_bytes -> u32,
                projection_digest -> &'a [u8; 32],
                last_step -> u64,
            }
        }
    };
    (@accessors $view:ident, $($name:ident -> $value:ty),+ $(,)?) => {$(
        fn $name(self) -> $value {
            $view::JobView::$name(self)
        }
    )+};
}
catalog_job!(catalog, 2);
// A Job3 cell with Catalog meaning (Free, registry9 Terminal, kind2 root);
// Row kind3 roots and registry16 Row Terminals never construct this view.
catalog_job!(catalog3, 3);

macro_rules! root_accessors {
    ($view:ty, $($name:ident -> $value:ty),+ $(,)?) => {$(
        fn $name(self) -> $value {
            <$view>::$name(self)
        }
    )+};
}
impl<'a> CatalogRoot<'a> for catalog::RootView<'a> {
    root_accessors! {
        catalog::RootView<'a>,
        bytes -> &'a [u8; ROOT_BYTES],
        aging -> Option<SystemWorkRef>,
        revision -> u64,
        cursor -> u64,
        apply -> u64,
        issued -> u64,
        completed -> u64,
        canceled -> u64,
        undo_high -> u64,
        object_high -> u64,
        commit_c -> u64,
        consumed_wal -> u64,
    }
}
impl<'a> CatalogRoot<'a> for row::RootView<'a> {
    root_accessors! {
        row::RootView<'a>,
        bytes -> &'a [u8; ROOT_BYTES],
        aging -> Option<SystemWorkRef>,
        revision -> u64,
        cursor -> u64,
        apply -> u64,
        issued -> u64,
        completed -> u64,
        canceled -> u64,
        undo_high -> u64,
        object_high -> u64,
        commit_c -> u64,
        consumed_wal -> u64,
    }
}

/// One bank cell seen by an actor scan. A Row cell belongs to the separate
/// Row observer and is never a Catalog Job.
pub(crate) enum Scan<J> {
    Catalog(J),
    Row(TxnRef),
}

/// Inputs for the Root-bound Aging successor of one group.
pub(crate) struct AgingStep<'a> {
    pub(crate) profile: Profile,
    pub(crate) before: Option<&'a [u8; 1536]>,
    pub(crate) before_page: &'a [u8; PAGE_BYTES],
    pub(crate) after_page: &'a [u8; PAGE_BYTES],
    /// The stable bank and the one Job cell this group replaced, if any.
    pub(crate) jobs: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    pub(crate) replaced: Option<(usize, &'a [u8; JOB_BYTES])>,
}

pub(crate) trait Format {
    type Root<'a>: CatalogRoot<'a>
    where
        Self: 'a;
    type Job<'a>: CatalogJob<'a>
    where
        Self: 'a;
    type Cells<'a>: Copy
    where
        Self: 'a;
    /// Views are covariant in their borrow; generic code cannot see that.
    fn shorten<'long: 'short, 'short>(job: Self::Job<'long>) -> Self::Job<'short>;
    fn decode_root_page(input: &[u8], namespace: Namespace)
    -> Result<(PageHeader, Self::Root<'_>)>;
    fn decode_root(input: &[u8], namespace: Namespace) -> Result<Self::Root<'_>>;
    /// Encode the after Root of a group whose before Root is `before`.
    fn encode_root_page(
        before: Self::Root<'_>,
        cell: &[u8; ROOT_BYTES],
        namespace: Namespace,
        output: &mut [u8],
    ) -> Result<()>;
    fn decode_jobs_page(input: &[u8], namespace: Namespace, index: u8) -> Result<PageView<'_>>;
    fn encode_jobs_page(
        namespace: Namespace,
        index: u8,
        lsn: u64,
        cells: &[&[u8]],
        output: &mut [u8],
    ) -> Result<()>;
    /// A Catalog Job only; other scopes are rejected, never reinterpreted.
    fn decode_job(input: &[u8], namespace: Namespace) -> Result<Self::Job<'_>>;
    fn scan_job(input: &[u8], namespace: Namespace) -> Result<Scan<Self::Job<'_>>>;
    fn free_job(generation: Generation, output: &mut [u8; JOB_BYTES]);
    fn cells(original: &[[u8; JOB_BYTES]; JOB_COUNT]) -> Self::Cells<'_>;
    fn replace<'a>(
        cells: Self::Cells<'a>,
        ordinal: usize,
        cell: &'a [u8; JOB_BYTES],
    ) -> Result<Self::Cells<'a>>;
    fn cell<'a>(cells: Self::Cells<'a>, ordinal: usize) -> &'a [u8; JOB_BYTES];
    fn summarize_jobs(
        cells: Self::Cells<'_>,
        namespace: Namespace,
        aggregate: &mut [u8; 640],
    ) -> Result<([u64; 3], [u8; 32])>;
    fn verify_selected_census(
        root: Self::Root<'_>,
        namespace: Namespace,
        cells: Self::Cells<'_>,
        aggregate: &mut [u8; 640],
    ) -> Result<()>;
    fn verify_census(
        root: Self::Root<'_>,
        namespace: Namespace,
        cells: &[[u8; JOB_BYTES]; JOB_COUNT],
        aggregate: &mut [u8; 640],
    ) -> Result<()>;
    /// Next Aging descriptor bound to `after_page`. The aggregate is scratch
    /// that already holds the after census and may change on error.
    fn next_aging(
        step: AgingStep<'_>,
        aggregate: &mut [u8; 640],
        output: &mut [u8; 1536],
    ) -> Result<()>;
    /// Validate the stable Aging descriptor bound to `root_page`.
    fn check_aging(
        bytes: &[u8; 1536],
        profile: Profile,
        root_page: &[u8; PAGE_BYTES],
    ) -> Result<()>;
}

/// The original version2 Catalog codecs, census and Root2-bound descriptor.
pub(crate) struct Catalog2;
impl Format for Catalog2 {
    type Root<'a> = catalog::RootView<'a>;
    type Job<'a> = catalog::JobView<'a>;
    type Cells<'a> = catalog::JobCells<'a>;
    fn shorten<'long: 'short, 'short>(job: Self::Job<'long>) -> Self::Job<'short> {
        job
    }
    fn decode_root_page(
        input: &[u8],
        namespace: Namespace,
    ) -> Result<(PageHeader, Self::Root<'_>)> {
        Ok(catalog::decode_root_page(input, namespace)?)
    }
    fn decode_root(input: &[u8], namespace: Namespace) -> Result<Self::Root<'_>> {
        Ok(catalog::decode_root(input, namespace)?)
    }
    fn encode_root_page(
        _before: Self::Root<'_>,
        cell: &[u8; ROOT_BYTES],
        namespace: Namespace,
        output: &mut [u8],
    ) -> Result<()> {
        Ok(catalog::encode_root_page(cell, namespace, output)?)
    }
    fn decode_jobs_page(input: &[u8], namespace: Namespace, index: u8) -> Result<PageView<'_>> {
        Ok(catalog::decode_jobs_page(input, namespace, index)?)
    }
    fn encode_jobs_page(
        namespace: Namespace,
        index: u8,
        lsn: u64,
        cells: &[&[u8]],
        output: &mut [u8],
    ) -> Result<()> {
        Ok(catalog::encode_jobs_page(
            namespace, index, lsn, cells, output,
        )?)
    }
    fn decode_job(input: &[u8], namespace: Namespace) -> Result<Self::Job<'_>> {
        Ok(catalog::decode_job(input, namespace)?)
    }
    fn scan_job(input: &[u8], namespace: Namespace) -> Result<Scan<Self::Job<'_>>> {
        Ok(Scan::Catalog(catalog::decode_job(input, namespace)?))
    }
    fn free_job(generation: Generation, output: &mut [u8; JOB_BYTES]) {
        catalog::free_job(generation, output);
    }
    fn cells(original: &[[u8; JOB_BYTES]; JOB_COUNT]) -> Self::Cells<'_> {
        catalog::JobCells::original(original)
    }
    fn replace<'a>(
        cells: Self::Cells<'a>,
        ordinal: usize,
        cell: &'a [u8; JOB_BYTES],
    ) -> Result<Self::Cells<'a>> {
        Ok(cells.replace(ordinal, cell)?)
    }
    fn cell<'a>(cells: Self::Cells<'a>, ordinal: usize) -> &'a [u8; JOB_BYTES] {
        cells.cell(ordinal)
    }
    fn summarize_jobs(
        cells: Self::Cells<'_>,
        namespace: Namespace,
        aggregate: &mut [u8; 640],
    ) -> Result<([u64; 3], [u8; 32])> {
        Ok(catalog::summarize_jobs(cells, namespace, aggregate)?)
    }
    fn verify_selected_census(
        root: Self::Root<'_>,
        namespace: Namespace,
        cells: Self::Cells<'_>,
        aggregate: &mut [u8; 640],
    ) -> Result<()> {
        Ok(catalog::verify_selected_census(
            root, namespace, cells, aggregate,
        )?)
    }
    fn verify_census(
        root: Self::Root<'_>,
        namespace: Namespace,
        cells: &[[u8; JOB_BYTES]; JOB_COUNT],
        aggregate: &mut [u8; 640],
    ) -> Result<()> {
        Ok(catalog::verify_census(root, namespace, cells, aggregate)?)
    }
    fn next_aging(
        step: AgingStep<'_>,
        aggregate: &mut [u8; 640],
        output: &mut [u8; 1536],
    ) -> Result<()> {
        super::ledger::next_aging(
            step.profile,
            step.before,
            step.before_page,
            step.after_page,
            aggregate,
            output,
        )
    }
    fn check_aging(
        bytes: &[u8; 1536],
        profile: Profile,
        root_page: &[u8; PAGE_BYTES],
    ) -> Result<()> {
        retention_obligation::catalog::decode(bytes, profile, root_page)?;
        Ok(())
    }
}

/// The separate exact version3 Root/Job codecs through the Catalog-scope Job3
/// queue view (`retention_page::catalog3`), the Job3 census (which also covers
/// other actors' Row Jobs) and the Root3-bound Aging descriptor.
pub(crate) struct Row3;
impl Format for Row3 {
    type Root<'a> = catalog3::RootView<'a>;
    type Job<'a> = catalog3::JobView<'a>;
    type Cells<'a> = catalog3::JobCells<'a>;
    fn shorten<'long: 'short, 'short>(job: Self::Job<'long>) -> Self::Job<'short> {
        job
    }
    fn decode_root_page(
        input: &[u8],
        namespace: Namespace,
    ) -> Result<(PageHeader, Self::Root<'_>)> {
        Ok(catalog3::decode_root_page(input, namespace)?)
    }
    fn decode_root(input: &[u8], namespace: Namespace) -> Result<Self::Root<'_>> {
        Ok(catalog3::decode_root(input, namespace)?)
    }
    /// Only the Row issuer raises the Root3 RowId high-water. Every Catalog
    /// group carries `row_high@432` from its before Root unchanged; the guard
    /// is the queue view's own `catalog3::encode_root_page`.
    fn encode_root_page(
        before: Self::Root<'_>,
        cell: &[u8; ROOT_BYTES],
        namespace: Namespace,
        output: &mut [u8],
    ) -> Result<()> {
        Ok(catalog3::encode_root_page(before, cell, namespace, output)?)
    }
    fn decode_jobs_page(input: &[u8], namespace: Namespace, index: u8) -> Result<PageView<'_>> {
        Ok(catalog3::decode_jobs_page(input, namespace, index)?)
    }
    fn encode_jobs_page(
        namespace: Namespace,
        index: u8,
        lsn: u64,
        cells: &[&[u8]],
        output: &mut [u8],
    ) -> Result<()> {
        Ok(catalog3::encode_jobs_page(
            namespace, index, lsn, cells, output,
        )?)
    }
    fn decode_job(input: &[u8], namespace: Namespace) -> Result<Self::Job<'_>> {
        match Self::scan_job(input, namespace)? {
            Scan::Catalog(job) => Ok(job),
            Scan::Row(_) => Err(Error::Unsupported),
        }
    }
    fn scan_job(input: &[u8], namespace: Namespace) -> Result<Scan<Self::Job<'_>>> {
        let job = row::decode_job(input, namespace)?;
        Ok(match catalog3::JobView::catalog(job) {
            Some(catalog) => Scan::Catalog(catalog),
            None => Scan::Row(job.source().expect("occupied Row Job")),
        })
    }
    fn free_job(generation: Generation, output: &mut [u8; JOB_BYTES]) {
        catalog3::free_job(generation, output);
    }
    fn cells(original: &[[u8; JOB_BYTES]; JOB_COUNT]) -> Self::Cells<'_> {
        catalog3::JobCells::original(original)
    }
    fn replace<'a>(
        cells: Self::Cells<'a>,
        ordinal: usize,
        cell: &'a [u8; JOB_BYTES],
    ) -> Result<Self::Cells<'a>> {
        Ok(cells.replace(ordinal, cell)?)
    }
    fn cell<'a>(cells: Self::Cells<'a>, ordinal: usize) -> &'a [u8; JOB_BYTES] {
        cells.cell(ordinal)
    }
    fn summarize_jobs(
        cells: Self::Cells<'_>,
        namespace: Namespace,
        aggregate: &mut [u8; 640],
    ) -> Result<([u64; 3], [u8; 32])> {
        Ok(catalog3::summarize_jobs(cells, namespace, aggregate)?)
    }
    fn verify_selected_census(
        root: Self::Root<'_>,
        namespace: Namespace,
        cells: Self::Cells<'_>,
        aggregate: &mut [u8; 640],
    ) -> Result<()> {
        Ok(catalog3::verify_selected_census(
            root, namespace, cells, aggregate,
        )?)
    }
    fn verify_census(
        root: Self::Root<'_>,
        namespace: Namespace,
        cells: &[[u8; JOB_BYTES]; JOB_COUNT],
        aggregate: &mut [u8; 640],
    ) -> Result<()> {
        Ok(catalog3::verify_census(root, namespace, cells, aggregate)?)
    }
    fn next_aging(
        step: AgingStep<'_>,
        aggregate: &mut [u8; 640],
        output: &mut [u8; 1536],
    ) -> Result<()> {
        let original = catalog3::JobCells::original(step.jobs);
        let jobs = match step.replaced {
            Some((ordinal, cell)) => original.replace(ordinal, cell)?,
            None => original,
        };
        retention_obligation::row::next(
            retention_obligation::row::AgingTransition {
                profile: step.profile,
                before_descriptor: step.before,
                before_root: step.before_page,
                after_root: step.after_page,
                jobs,
            },
            aggregate,
            output,
        )?;
        Ok(())
    }
    fn check_aging(
        bytes: &[u8; 1536],
        profile: Profile,
        root_page: &[u8; PAGE_BYTES],
    ) -> Result<()> {
        retention_obligation::row::decode(bytes, profile, root_page)?;
        Ok(())
    }
}
