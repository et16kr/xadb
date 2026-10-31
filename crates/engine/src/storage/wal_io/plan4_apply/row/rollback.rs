//! Live statement and full-transaction rollback over the actual Row head chain.
//! C chooses the next U and consumes CLR candidates; A only produces them.
use super::super::source::OperationSourceAdmission;
use super::super::visibility::{ReferenceLease, UndoPin};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RollbackScope {
    Statement,
    Transaction,
}

/// Scalar cut of the original origin, head, statement mark and next link.
struct RollbackSelection<'c, 's> {
    core: &'c PageConcurrency<'s>,
    head: Option<UndoRef>,
    mark: Option<UndoRef>,
    next: Option<UndoLink>,
}

/// The original reference lease and the source-issued currentU/nextU pins.
/// Field order is the implicit drop order: both pins before their lease.
struct RollbackRefs<'b, 'a, 'c, 's> {
    current: Option<UndoPin<'b, 'a, 'c, 's>>,
    next: Option<UndoPin<'b, 'a, 'c, 's>>,
    lease: ReferenceLease<'b, 'a, 'c, 's>,
}

pub(crate) struct RowRollback<
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
    'o,
    'm,
    'w,
    W: WriterNamespace,
> {
    mutation: &'r mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'r mut Owner<'c, 's>,
    transaction: &'r RowTransaction<'v, 'a, 'c, 's>,
    statement: Option<&'r RowStatementView<'t, 'v, 'a, 'c, 's>>,
    private: Option<&'i mut [u8; 425_984]>,
    selection: RollbackSelection<'c, 's>,
    refs: Option<RollbackRefs<'b, 'a, 'c, 's>>,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
}

pub(crate) enum RollbackStep<
    'x,
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
    'o,
    'm,
    'w,
    W: WriterNamespace,
> {
    Candidate(RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>),
    FlushRequired { target: Plan4Target },
    Complete,
}

/// A short wrapper over the rollback; it never owns a second port or Owner.
pub(crate) struct RowRollbackTask<
    'x,
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
    'o,
    'm,
    'w,
    W: WriterNamespace,
> {
    rollback: &'x mut RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    draft: RowDraftCut,
}

pub(crate) struct PreparedRollbackTask<
    'x,
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
    'o,
    'm,
    'w,
    W: WriterNamespace,
> {
    task: RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    checked: RowPreparedCut,
}

pub(crate) struct RowRollbackDone<'i> {
    private: &'i mut [u8; 425_984],
    scope: RollbackScope,
}
impl<'i> RowRollbackDone<'i> {
    pub(in crate::storage) fn into_parts(self) -> (RollbackScope, &'i mut [u8; 425_984]) {
        (self.scope, self.private)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Unsupported before RollbackStart or any pin until the actual Row
    /// rollback adapter is supplied.
    pub(in crate::storage) fn start_row_rollback<'r, 't, 'v, 'i>(
        &'r mut self,
        _owner: &'r mut Owner<'c, 's>,
        _transaction: &'r RowTransaction<'v, 'a, 'c, 's>,
        _statement: Option<&'r RowStatementView<'t, 'v, 'a, 'c, 's>>,
        _scope: RollbackScope,
        _private: &'i mut [u8; 425_984],
    ) -> Result<RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        Err(unsupported())
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RowRollback<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn next<'x>(
        &'x mut self,
    ) -> Result<RollbackStep<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        Err(unsupported())
    }
    /// Requires the actual mark/None and the StatementFinish/AbortEnd receipt.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same rollback"
    )]
    pub(in crate::storage) fn finish(
        self,
    ) -> std::result::Result<RowRollbackDone<'i>, RowOwnedError<Self>> {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
}

impl<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self) -> Result<RowProducerLane<'l>> {
        Err(unsupported())
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same task"
    )]
    pub(in crate::storage) fn prepare(
        self,
        _proposal: RowProposal,
    ) -> std::result::Result<
        PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RowOwnedError<Self>,
    > {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
}

impl<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreparedRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same prepared task"
    )]
    pub(in crate::storage) fn apply(
        self,
    ) -> std::result::Result<RowMutationObservation, RowOwnedError<Self>> {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
    pub(in crate::storage) fn reopen(
        self,
    ) -> RowRollbackTask<'x, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        self.task
    }
}
