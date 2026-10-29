//! Thin forwarding of the driver traits to the actual C0 Row read types.
//! Nothing here constructs a source value: every pin, credit, candidate and
//! observation comes from the original operation and view.
use super::{
    CursorCredit, CursorStep, Observed, PointRead, ReadSource, ReadView, RowCursor, RowReadError,
    SearchPage, Step, read_point,
};
use crate::storage::{
    disk::{
        CallerCredit, CheckedTable, MissingRowEvidence, Plan4ResidencyCursor, Plan4Target,
        ReadStep, RetentionError, RowCandidate, RowCopyObservation, RowReadOperation,
        RowSearchPage, RowStatementView,
    },
    row_record::RecordVersion,
    transaction_record::{TransactionState, TxnRef},
    tree_page::TreePageView,
};
use foundation::{identity::RowId, namespace::WriterNamespace, runtime::Cancellation};

impl<'a, 'c, 's> ReadView for RowStatementView<'_, '_, 'a, 'c, 's> {
    type Credit<'r>
        = CallerCredit<'r, 'a, 'c, 's>
    where
        Self: 'r;
    fn q(&self) -> Result<u64, RetentionError> {
        RowStatementView::q(self)
    }
    fn own_actor(&self) -> Option<TxnRef> {
        RowStatementView::own_actor(self)
    }
    fn revision(&self) -> u64 {
        RowStatementView::revision(self)
    }
    fn reserve_cursor_credit(&self) -> Result<CallerCredit<'_, 'a, 'c, 's>, RetentionError> {
        RowStatementView::reserve_cursor_credit(self)
    }
}

impl CursorCredit for CallerCredit<'_, '_, '_, '_> {
    fn release_checked(self) -> Result<(), RetentionError> {
        CallerCredit::release_checked(self)
    }
}

impl Observed for RowCandidate {
    fn version(&self) -> RecordVersion {
        RowCandidate::version(self)
    }
    fn canonical_len(&self) -> usize {
        RowCandidate::canonical_len(self)
    }
}

impl Observed for RowCopyObservation {
    fn version(&self) -> RecordVersion {
        RowCopyObservation::version(self)
    }
    fn canonical_len(&self) -> usize {
        self.len()
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd> SearchPage for RowSearchPage<'_, 'b, 'a, 's, 'c, 'p, 'd> {
    type Candidate = RowCandidate;
    fn leaf(&self) -> Result<TreePageView<'_>, RetentionError> {
        RowSearchPage::leaf(self)
    }
    fn capture(self, at: usize) -> Result<RowCandidate, RetentionError> {
        RowSearchPage::capture(self, at)
    }
    fn release_checked(self) -> Result<(), RetentionError> {
        RowSearchPage::release_checked(self)
    }
}

/// The original operation with the table and residency cursor its source
/// phases take. Finishing the adapter finishes the operation.
struct LiveSource<'x, 'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    operation: RowReadOperation<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    table: &'x CheckedTable<'c, 's>,
    residency: &'x mut Plan4ResidencyCursor,
}

fn source_step<T>(step: ReadStep<T>) -> Step<T, Plan4Target> {
    match step {
        ReadStep::Ready(value) => Step::Ready(value),
        ReadStep::FlushRequired { target } => Step::Flush(target),
    }
}

fn read_step<T>(step: Step<T, Plan4Target>) -> ReadStep<T> {
    match step {
        Step::Ready(value) => ReadStep::Ready(value),
        Step::Flush(target) => ReadStep::FlushRequired { target },
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, W: WriterNamespace> ReadSource
    for LiveSource<'_, '_, '_, '_, '_, '_, 'b, 'a, 's, 'c, 'p, 'd, '_, '_, W>
{
    type Target = Plan4Target;
    type Candidate = RowCandidate;
    type Missing = MissingRowEvidence;
    type Copied = RowCopyObservation;
    type Page<'g>
        = RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>
    where
        Self: 'g;
    fn page(
        &mut self,
    ) -> Result<Step<RowSearchPage<'_, 'b, 'a, 's, 'c, 'p, 'd>, Plan4Target>, RetentionError> {
        self.operation
            .page(self.table, self.residency)
            .map(source_step)
    }
    fn status(
        &mut self,
        candidate: &RowCandidate,
    ) -> Result<Step<TransactionState, Plan4Target>, RetentionError> {
        self.operation
            .status(candidate, self.residency)
            .map(source_step)
    }
    fn missing_before(
        &mut self,
        candidate: &RowCandidate,
    ) -> Result<Step<MissingRowEvidence, Plan4Target>, RetentionError> {
        self.operation
            .missing_before(candidate, self.residency)
            .map(source_step)
    }
    fn copy_candidate(
        &mut self,
        candidate: &RowCandidate,
        output: &mut [u8],
    ) -> Result<RowCopyObservation, RetentionError> {
        self.operation.copy_candidate(candidate, output)
    }
    fn release_candidate(&mut self, candidate: RowCandidate) -> Result<(), RetentionError> {
        self.operation.release_candidate(candidate)
    }
    fn finish(self) -> Result<(), RetentionError> {
        self.operation.finish()
    }
}

/// Live point read. `operation` must be the one the caller opened on `view`:
/// the C0 operation exposes no statement check, so B cannot verify it here.
fn read_point_live<'a, 'c, 's, W: WriterNamespace>(
    operation: RowReadOperation<'_, '_, '_, '_, '_, '_, 'a, 's, 'c, '_, '_, '_, '_, W>,
    view: &RowStatementView<'_, '_, 'a, 'c, 's>,
    table: &CheckedTable<'c, 's>,
    residency: &mut Plan4ResidencyCursor,
    row: RowId,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<ReadStep<PointRead<RowCopyObservation>>, RowReadError> {
    let source = LiveSource {
        operation,
        table,
        residency,
    };
    read_point(view, source, row, output, cancel).map(read_step)
}

type LiveCursor<'r, 't, 'v, 'a, 'c, 's> = RowCursor<'r, RowStatementView<'t, 'v, 'a, 'c, 's>>;

fn open_cursor_live<'r, 't, 'v, 'a, 'c, 's>(
    view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    start: Option<RowId>,
    end: Option<RowId>,
) -> Result<LiveCursor<'r, 't, 'v, 'a, 'c, 's>, RowReadError> {
    RowCursor::open(view, start, end)
}

/// Live cursor step with a new operation opened on the cursor's own view.
fn next_live<'a, 'c, 's, W: WriterNamespace>(
    cursor: &mut LiveCursor<'_, '_, '_, 'a, 'c, 's>,
    operation: RowReadOperation<'_, '_, '_, '_, '_, '_, 'a, 's, 'c, '_, '_, '_, '_, W>,
    table: &CheckedTable<'c, 's>,
    residency: &mut Plan4ResidencyCursor,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<ReadStep<CursorStep<RowCopyObservation>>, RowReadError> {
    let source = LiveSource {
        operation,
        table,
        residency,
    };
    cursor.next(source, output, cancel).map(read_step)
}
