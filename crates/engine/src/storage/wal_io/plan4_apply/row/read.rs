//! Row read operation: the original Owner is reborrowed for one source phase at
//! a time. Candidates carry observations, never a pin or a source slice.
use super::super::source::OperationSourceAdmission;
use super::super::visibility::{ReferenceLease, TxnPin, UndoPin};
use super::*;
use crate::storage::{
    dirty_frame::ReadTargets,
    row_record::{RecordExpectation, RecordVersion},
    transaction_record::TransactionState,
    tree_page::TreePageView,
};

/// Source-issued Txn and U pins of the last capture; held by the operation.
struct CapturedRowRefs<'b, 'a, 'c, 's> {
    transaction: Option<TxnPin<'b, 'a, 'c, 's>>,
    undo: Option<UndoPin<'b, 'a, 'c, 's>>,
}

pub(crate) struct RowReadOperation<
    'u,
    'r,
    't,
    'v,
    'i,
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
    backend: ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    owner: &'u mut Owner<'c, 's>,
    statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    loader: &'i mut [u8; 409_600],
    copy: &'i mut [u8; PAGE_BYTES],
    // Field order is the implicit drop order and matches `finish`: captured
    // refs before their lease, and the admission last.
    captured: Option<CapturedRowRefs<'b, 'a, 'c, 's>>,
    lease: Option<ReferenceLease<'b, 'a, 'c, 's>>,
    lease_generation: u64,
    capture_sequence: u64,
    stopped: bool,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
}

/// The validated anchor/leaf/allocation set. Its guards reborrow the Owner for
/// `'g` only; the other fields are disjoint borrows of the operation.
pub(crate) struct RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd> {
    source: ReadTargets<'g, 'b, 'c, 's, 'p, 'd, 3>,
    lease: &'g mut ReferenceLease<'b, 'a, 'c, 's>,
    captured: &'g mut Option<CapturedRowRefs<'b, 'a, 'c, 's>>,
    copy: &'g mut [u8; PAGE_BYTES],
    capture_sequence: &'g mut u64,
    expectation: RecordExpectation,
}

/// Lease generation, capture sequence and the observed row; not Copy.
pub(crate) struct RowCandidate {
    lease_generation: u64,
    capture_sequence: u64,
    version: RecordVersion,
    canonical_len: usize,
}
impl RowCandidate {
    pub(in crate::storage) fn row(&self) -> RowId {
        self.version.row
    }
    pub(in crate::storage) fn version(&self) -> RecordVersion {
        self.version
    }
    pub(in crate::storage) fn canonical_len(&self) -> usize {
        self.canonical_len
    }
}

/// C has checked the same U's Missing, row identity and creator/sequence.
pub(crate) struct MissingRowEvidence {
    _private: (),
}

/// Bytes copied from the actual capture into the caller output.
pub(crate) struct RowCopyObservation {
    version: RecordVersion,
    len: usize,
}
impl RowCopyObservation {
    pub(in crate::storage) fn version(&self) -> RecordVersion {
        self.version
    }
    pub(in crate::storage) fn len(&self) -> usize {
        self.len
    }
}

fn visibility_error(error: super::super::visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}
fn release_refs(refs: CapturedRowRefs<'_, '_, '_, '_>) -> Result<()> {
    let tx = refs.transaction.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    let undo = refs.undo.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    merge(tx, undo)
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// Unsupported before any admission, lease or source read until C2.
    pub(in crate::storage) fn row_operation<'u, 'r, 't, 'v, 'i>(
        &self,
        _owner: &'u mut Owner<'c, 's>,
        _view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        _private: &'i mut [u8; 425_984],
    ) -> Result<RowReadOperation<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        Err(unsupported())
    }
}

impl<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    RowReadOperation<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(in crate::storage) fn page<'g>(
        &'g mut self,
        _table: &CheckedTable<'c, 's>,
        _residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>>> {
        Err(unsupported())
    }
    pub(in crate::storage) fn status(
        &mut self,
        _candidate: &RowCandidate,
        _residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<TransactionState>> {
        Err(unsupported())
    }
    pub(in crate::storage) fn missing_before(
        &mut self,
        _candidate: &RowCandidate,
        _residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<MissingRowEvidence>> {
        Err(unsupported())
    }
    /// The visible decision stays with B; this only copies the actual capture.
    pub(in crate::storage) fn copy_candidate(
        &mut self,
        _candidate: &RowCandidate,
        _output: &mut [u8],
    ) -> Result<RowCopyObservation> {
        Err(unsupported())
    }
    pub(in crate::storage) fn release_candidate(&mut self, candidate: RowCandidate) -> Result<()> {
        if candidate.lease_generation != self.lease_generation
            || candidate.capture_sequence != self.capture_sequence
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        self.captured.take().map_or(Ok(()), release_refs)
    }
    /// Checked cleanup in source order: captured refs, lease, then admission.
    pub(in crate::storage) fn finish(mut self) -> Result<()> {
        self.stopped = true;
        let refs = self.captured.take().map_or(Ok(()), release_refs);
        let lease = self.lease.take().map_or(Ok(()), |lease| {
            lease.release_checked().map_err(visibility_error)
        });
        let source = self.source.release_checked().map_err(RetentionError::from);
        merge(merge(refs, lease), source)
    }
}

impl<'g, 'b, 'a, 's, 'c, 'p, 'd> RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd> {
    pub(in crate::storage) fn leaf(&self) -> Result<TreePageView<'_>> {
        Err(unsupported())
    }
    /// Checks the slot chosen by B against the latched row before any pin.
    pub(in crate::storage) fn capture(self, _at: usize) -> Result<RowCandidate> {
        merge(Err(unsupported()), self.release_checked())
    }
    pub(in crate::storage) fn release_checked(self) -> Result<()> {
        self.source
            .release_checked()
            .map_err(|e| frame(RetentionPhase::Source, e))
    }
}
