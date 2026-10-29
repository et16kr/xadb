//! Preflight, RowReserve, producer lane, prepare/apply and cancel. Each step
//! consumes the previous wrapper; one source admission and one reservation
//! move through the chain instead of being acquired again.
use super::super::source::OperationSourceAdmission;
use super::*;

/// The one actual Root/Job/UndoSlot reservation of a ReservedRow. Only C
/// creates it after the Root/Job/Undo before checks, the original WAL Reserve
/// and whole installation. Not Clone and not transferable to another actor.
struct RowSlotReservation<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    actor: TxnRef,
    root_job: JobSlotRef,
    row: RowId,
    undo: UndoRef,
    reserve_lsn: u64,
    active: bool,
}

/// Validated source state before the first Row effect. Errors, an explicit
/// finish or Drop return the admission and loan with no WAL/page/issuer change.
pub(crate) struct PreflightRow<
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
    statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    table: &'r CheckedTable<'c, 's>,
    private: Option<&'i mut [u8; 425_984]>,
    source_cut: RowPreflightCut,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
}

pub(crate) struct ReservedRow<
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
    statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    table: &'r CheckedTable<'c, 's>,
    private: Option<&'i mut [u8; 425_984]>,
    reservation: RowSlotReservation<'c, 's>,
    draft: RowDraftCut,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
}

/// Holds the whole reservation and a scalar cut of the checked proposal, not a
/// borrowed frame slice; reopen discards only that cut.
pub(crate) struct PreparedRow<
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
    reserved: ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    checked: RowPreparedCut,
}

pub(crate) struct RowApplied<'i> {
    observation: RowMutationObservation,
    private: &'i mut [u8; 425_984],
}
impl<'i> RowApplied<'i> {
    pub(in crate::storage) fn into_parts(self) -> (RowMutationObservation, &'i mut [u8; 425_984]) {
        (self.observation, self.private)
    }
}

pub(crate) struct RowCancelled<'i> {
    private: &'i mut [u8; 425_984],
}
impl<'i> RowCancelled<'i> {
    pub(in crate::storage) fn into_private(self) -> &'i mut [u8; 425_984] {
        self.private
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Unsupported before any source read, admission or effect until the
    /// actual schema/index/leaf preflight adapter is supplied.
    pub(in crate::storage) fn preflight_row<'r, 't, 'v, 'i>(
        &'r mut self,
        _owner: &'r mut Owner<'c, 's>,
        _statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        _table: &'r CheckedTable<'c, 's>,
        _private: &'i mut [u8; 425_984],
    ) -> Result<PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        Err(unsupported())
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn validation_lane<'l>(&'l mut self) -> Result<RowValidationLane<'l>> {
        Err(unsupported())
    }
    /// The only place for the first Reserve effect. RowInputPlan is untrusted.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same preflight for retry"
    )]
    pub(in crate::storage) fn reserve(
        self,
        _input: RowInputPlan,
    ) -> std::result::Result<
        ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RowOwnedError<Self>,
    > {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
    /// Returns the loan after the checked admission release. The read set
    /// already registered by the actual source stays with the statement.
    pub(in crate::storage) fn finish(mut self) -> Result<&'i mut [u8; 425_984]> {
        let cleanup = self.source.release_checked().map_err(RetentionError::from);
        let private = self.private.take().ok_or_else(|| {
            fatal(
                self.mutation.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            )
        });
        merge(private, cleanup)
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Lends the producer the disjoint private slices for the borrow only.
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self) -> Result<RowProducerLane<'l>> {
        Err(unsupported())
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same reservation"
    )]
    pub(in crate::storage) fn prepare(
        self,
        _proposal: RowProposal,
    ) -> std::result::Result<
        PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RowOwnedError<Self>,
    > {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same reservation"
    )]
    pub(in crate::storage) fn cancel(
        self,
    ) -> std::result::Result<RowCancelled<'i>, RowOwnedError<Self>> {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same prepared reservation"
    )]
    pub(in crate::storage) fn apply(
        self,
    ) -> std::result::Result<RowApplied<'i>, RowOwnedError<Self>> {
        Err(RowOwnedError::Before {
            error: unsupported(),
            owned: self,
        })
    }
    /// Before stage only: drop the checked cut and return the same reservation.
    pub(in crate::storage) fn reopen(
        self,
    ) -> ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        self.reserved
    }
}
