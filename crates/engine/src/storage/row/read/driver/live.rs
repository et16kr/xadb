//! Thin forwarding of the driver traits to the actual C0 Row read types.
//! Nothing here constructs a source value: every pin, credit, candidate and
//! observation comes from the original operation and view. The entries open
//! the operation themselves through `ReadPort::row_operation` on the view the
//! driver judges with, so a caller cannot pair one view's operation with
//! another view's snapshot, or move a cursor to another table.
use super::{
    CursorCredit, CursorStep, Observed, PointRead, ReadSource, ReadView, RowCursor, RowReadError,
    SearchPage, Step, read_point,
};
use crate::storage::{
    disk::{
        CallerCredit, CheckedTable, MissingRowEvidence, Owner, Plan4ResidencyCursor, Plan4Target,
        ReadPort, ReadStep, RetentionError, RowCandidate, RowCopyObservation, RowReadOperation,
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

impl Observed for RowCandidate<'_> {
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
    type Candidate = RowCandidate<'b>;
    fn leaf(&self) -> Result<TreePageView<'_>, RetentionError> {
        RowSearchPage::leaf(self)
    }
    fn capture(self, at: usize) -> Result<RowCandidate<'b>, RetentionError> {
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

impl<'b, 'a, 's, 'c, 'p, 'd, W: WriterNamespace> ReadSource
    for LiveSource<'_, '_, '_, '_, '_, '_, 'b, 'a, 's, 'c, 'p, 'd, '_, '_, W>
{
    type Target = Plan4Target;
    type Candidate = RowCandidate<'b>;
    type Missing = MissingRowEvidence;
    type Copied = RowCopyObservation;
    type Page<'g>
        = RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>
    where
        Self: 'g;
    fn page(
        &mut self,
    ) -> Result<Step<RowSearchPage<'_, 'b, 'a, 's, 'c, 'p, 'd>, Plan4Target>, RetentionError> {
        self.operation.page(self.table, self.residency)
    }
    fn status(
        &mut self,
        candidate: &RowCandidate<'b>,
    ) -> Result<Step<TransactionState, Plan4Target>, RetentionError> {
        self.operation.status(candidate, self.residency)
    }
    fn missing_before(
        &mut self,
        candidate: &RowCandidate<'b>,
    ) -> Result<Step<MissingRowEvidence, Plan4Target>, RetentionError> {
        self.operation.missing_before(candidate, self.residency)
    }
    fn copy_candidate(
        &mut self,
        candidate: &RowCandidate<'b>,
        output: &mut [u8],
    ) -> Result<RowCopyObservation, RetentionError> {
        self.operation.copy_candidate(candidate, output)
    }
    fn release_candidate(&mut self, candidate: RowCandidate<'b>) -> Result<(), RetentionError> {
        self.operation.release_candidate(candidate)
    }
    fn finish(self) -> Result<(), RetentionError> {
        self.operation.finish()
    }
}

/// The caller's original port, Owner, private loan and residency cursor for
/// one call. The entry opens the operation from them; none is kept.
pub(in crate::storage) struct LiveCall<'x, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    pub(in crate::storage) port: &'x ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    pub(in crate::storage) owner: &'x mut Owner<'c, 's>,
    pub(in crate::storage) private: &'x mut [u8; 425_984],
    pub(in crate::storage) residency: &'x mut Plan4ResidencyCursor,
}

/// Live point read on an operation opened here on `view`.
pub(in crate::storage) fn read_point_live<'a, 'c, 's, W: WriterNamespace>(
    call: LiveCall<'_, '_, 'a, 's, 'c, '_, '_, '_, '_, W>,
    view: &RowStatementView<'_, '_, 'a, 'c, 's>,
    table: &CheckedTable<'c, 's>,
    row: RowId,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<ReadStep<PointRead<RowCopyObservation>>, RowReadError> {
    let LiveCall {
        port,
        owner,
        private,
        residency,
    } = call;
    read_point(
        view,
        move |view| {
            Ok(LiveSource {
                operation: port.row_operation(owner, view, private)?,
                table,
                residency,
            })
        },
        row,
        output,
        cancel,
    )
}

pub(in crate::storage) struct LiveCursor<'r, 't, 'v, 'a, 'c, 's> {
    inner: RowCursor<'r, RowStatementView<'t, 'v, 'a, 'c, 's>, CheckedTable<'c, 's>>,
}
impl LiveCursor<'_, '_, '_, '_, '_, '_> {
    pub(in crate::storage) fn close(self) -> Result<(), RetentionError> {
        self.inner.close()
    }
}

pub(in crate::storage) fn open_cursor_live<'r, 't, 'v, 'a, 'c, 's>(
    view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    table: &'r CheckedTable<'c, 's>,
    start: Option<RowId>,
    end: Option<RowId>,
) -> Result<LiveCursor<'r, 't, 'v, 'a, 'c, 's>, RowReadError> {
    RowCursor::open(view, table, start, end).map(|inner| LiveCursor { inner })
}

/// Live cursor step on an operation opened here on the cursor's own view,
/// reading the table the cursor was opened on.
pub(in crate::storage) fn next_live<'a, 'c, 's, W: WriterNamespace>(
    cursor: &mut LiveCursor<'_, '_, '_, 'a, 'c, 's>,
    call: LiveCall<'_, '_, 'a, 's, 'c, '_, '_, '_, '_, W>,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<ReadStep<CursorStep<RowCopyObservation>>, RowReadError> {
    let LiveCall {
        port,
        owner,
        private,
        residency,
    } = call;
    cursor.inner.next(
        move |view, table| {
            Ok(LiveSource {
                operation: port.row_operation(owner, view, private)?,
                table,
                residency,
            })
        },
        output,
        cancel,
    )
}
