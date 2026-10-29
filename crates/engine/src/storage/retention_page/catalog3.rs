//! Catalog scope2 view of the exact Profile3 Root3/Job3 queue. It has the names
//! and call shapes of `retention_page::catalog`, so a Catalog owner bound to an
//! exact Profile3 can change only its `queue` alias, except `encode_root_page`:
//! it also takes the before Root so the row_high guard cannot be skipped, and a
//! caller of the version2 shape fails to compile. Every cell is decoded by
//! the exact version3 codec in `row`; this view yields Free, registry9 Catalog
//! Terminal and kind2 Catalog roots only, and refuses a Row-scope cell (kind3
//! root or registry16 Terminal) with `UnsupportedState` instead of giving it a
//! Catalog meaning. There is no version rewrite, legacy fallback or `>=`
//! comparison, and no view grants reservation, pin, publication or apply right.
//! The census entries are the Row codec's own: they validate the whole Job3
//! bank, including other actors' Row Jobs, which this view never returns.
use super::{
    Generation, JOB_BYTES, Namespace, PageRef, ROOT_BYTES, RemainingBudget, Result, RetentionError,
    StatusSlotRef, TxnRef, get, row,
};
use crate::storage::undo_ref::UndoRef;
use foundation::identity::ObjectHandle;

pub(crate) use super::catalog::JobState;
#[allow(
    unused_imports,
    reason = "same-name surface of retention_page::catalog; current callers name these there"
)]
pub(crate) use super::catalog::{CREATE_PEAK, MAX_CATALOG_CELL, MIN_CATALOG_CELL, RELEASE_BYTES};
pub(in crate::storage) use super::row::free_job;
#[allow(
    unused_imports,
    reason = "same-name surface of retention_page::catalog; genesis encodes Root3 itself"
)]
pub(crate) use super::row::initial_root;
pub(crate) use super::row::{
    JobCells, RootView, decode_jobs_page, decode_root, decode_root_page, encode_jobs_page,
    root_decode_source_bytes, summarize_jobs, verify_census, verify_selected_census,
};

/// One exact Job3 cell with Catalog meaning. Constructed only from a decoded
/// Job3 view whose scope is not Row.
#[derive(Clone, Copy)]
pub(crate) struct JobView<'a> {
    job: row::JobView<'a>,
}
impl<'a> JobView<'a> {
    /// The Catalog view of an exact Job3 decode, or None for a Row-scope cell.
    pub(crate) fn catalog(job: row::JobView<'a>) -> Option<Self> {
        (job.scope() != Some(row::JobScope::Row)).then_some(Self { job })
    }
    pub(crate) fn job3(self) -> row::JobView<'a> {
        self.job
    }
    pub(crate) fn bytes(self) -> &'a [u8; JOB_BYTES] {
        self.job.bytes()
    }
    pub(crate) fn state(self) -> JobState {
        match self.job.state() {
            row::JobState::Free => JobState::Free,
            row::JobState::TerminalLeased => JobState::TerminalLeased,
            row::JobState::TerminalReady => JobState::TerminalReady,
            row::JobState::RootReserved => JobState::RootReserved,
            row::JobState::RootRetained => JobState::RootRetained,
            row::JobState::RootReleasingCommitted => JobState::RootReleasingCommitted,
            row::JobState::RootReleasingCompensated => JobState::RootReleasingCompensated,
        }
    }
    pub(crate) fn generation(self) -> Generation {
        self.job.generation()
    }
    pub(crate) fn source(self) -> Option<TxnRef> {
        self.job.source()
    }
    pub(crate) fn status(self) -> Option<StatusSlotRef> {
        self.job.status()
    }
    pub(crate) fn undo(self) -> Option<UndoRef> {
        self.job.undo()
    }
    pub(crate) fn object(self) -> Option<ObjectHandle> {
        self.job.object()
    }
    pub(crate) fn pair(self) -> Option<[PageRef; 2]> {
        self.job.pair()
    }
    pub(crate) fn lease_lsn(self) -> u64 {
        self.job.lease_lsn()
    }
    pub(crate) fn adoption_lsn(self) -> u64 {
        self.job.adoption_lsn()
    }
    pub(crate) fn outcome(self) -> u8 {
        self.job.outcome()
    }
    pub(crate) fn release_cursor(self) -> u8 {
        self.job.release_cursor()
    }
    pub(crate) fn undo_sequence(self) -> u64 {
        self.job.undo_sequence()
    }
    pub(crate) fn statement(self) -> u64 {
        self.job.statement()
    }
    pub(crate) fn mutation(self) -> u64 {
        self.job.mutation()
    }
    pub(crate) fn commit_q(self) -> u64 {
        self.job.commit_q()
    }
    pub(crate) fn release_start(self) -> u64 {
        self.job.release_start()
    }
    /// Job3 keeps the Catalog cell-size field at 280 (`cell_bytes`); the exact
    /// decoder bounded kind2 cells to MIN_CATALOG_CELL..=MAX_CATALOG_CELL.
    pub(crate) fn catalog_bytes(self) -> u32 {
        self.job.cell_bytes()
    }
    pub(crate) fn projection_digest(self) -> &'a [u8; 32] {
        self.job.projection_digest()
    }
    pub(crate) fn sender_digest(self) -> &'a [u8; 32] {
        self.job.sender_digest()
    }
    pub(crate) fn last_step(self) -> u64 {
        self.job.last_step()
    }
    pub(crate) fn budget(self) -> &'a [u8; 640] {
        self.job.budget()
    }
    /// The Job3 recipe for Catalog scope: CREATE_PEAK and no Row h-term.
    pub(crate) fn cleanup(self) -> RemainingBudget {
        self.job.cleanup()
    }
}

/// Exact Job3 decode with Catalog meaning. Version2/4 cells keep the Job3
/// decoder's `UnsupportedFormat`; a valid Row-scope Job3 cell is
/// `UnsupportedState` for this Catalog consumer.
pub(crate) fn decode_job(input: &[u8], namespace: Namespace) -> Result<JobView<'_>> {
    JobView::catalog(row::decode_job(input, namespace)?).ok_or(RetentionError::UnsupportedState)
}

/// Root3 page encode for a Catalog-scope writer. Only the Row issuer raises
/// the Root3 RowId high-water, so the candidate cell must carry `row_high@432`
/// from its before Root unchanged; otherwise `OrderMismatch` and `output` is
/// untouched. This view exports no unguarded Root3 encode: unlike
/// `catalog::encode_root_page`, a caller must name the before Root.
pub(crate) fn encode_root_page(
    before: RootView<'_>,
    cell: &[u8; ROOT_BYTES],
    namespace: Namespace,
    output: &mut [u8],
) -> Result<()> {
    if get(cell, 432) != before.row_high() {
        return Err(RetentionError::OrderMismatch);
    }
    row::encode_root_page(cell, namespace, output)
}

/// Row codec bound plus this view's own decode wrapper frame. Every other entry
/// is the Row codec entry itself; caller-owned page/Job arrays and the 640B
/// aggregate scratch are charged by their owners, as for `catalog`.
pub(crate) fn source_bytes() -> usize {
    row::source_bytes() + wrapper_frame()
}

/// The larger of this view's two wrapper frames above the Row codec:
/// `decode_job` (with room for the copied views and state mapping of accessor
/// calls on its result) and the row_high guard of `encode_root_page`.
pub(in crate::storage) fn wrapper_frame() -> usize {
    use std::mem::size_of;
    let decode = size_of::<(&[u8], Namespace)>()
        + size_of::<Result<row::JobView<'_>>>()
        + size_of::<Option<JobView<'_>>>()
        + size_of::<Result<JobView<'_>>>()
        + 4 * size_of::<JobView<'_>>()
        + 2 * size_of::<(JobState, row::JobState)>();
    let encode = size_of::<(RootView<'_>, &[u8; ROOT_BYTES], Namespace, &mut [u8])>()
        + 2 * size_of::<u64>()
        + size_of::<Result<()>>();
    decode.max(encode)
}
