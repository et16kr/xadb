//! Preflight, RowReserve, producer lane, prepare/apply and cancel. Each step
//! consumes the previous wrapper; one source admission, one terminal read
//! gate, one flush lease and one reservation move through the chain instead
//! of being acquired again (S2-0 Row write amendment §1).
use super::super::flush::RetentionFlushControl;
use super::super::source::OperationSourceAdmission;
use super::*;
use crate::transaction::concurrency::TerminalReadGate;

/// The original flush control's one mutable borrow, held from preflight to
/// finish, apply success or cancel success and kept across every Before
/// (CREATE-LEASE-001 precedent). Its acquisition is `check_catalog_pair_io`'s
/// check; no pair or selected generation is frozen and C performs no flush
/// I/O under it. Its purpose is that no flush capture or eviction reaches a
/// write target until the row ends.
///
/// Deliberately no Drop impl, like `CatalogPairIoLease`: with one, `'r` would
/// stay alive to the end of the preflight step match and the `FlushRequired`
/// arm could not borrow the flusher again.
pub(crate) struct RowIoLease<'r, 'c, 's, 'w> {
    original: &'r mut RetentionFlushControl<'c, 's, 'w>,
}

/// What `preflight_row` returns. `FlushRequired` is returned before any
/// effect or read-set registration, after the source, gate and lease were
/// returned: the residency of the write targets needs `target`, a dirty
/// victim, flushed first; the caller flushes it and tries the same row again
/// from preflight. Neither this step nor `PreflightRow` has a Drop impl, for
/// the reason given at `RowIoLease`.
#[allow(
    clippy::large_enum_variant,
    reason = "the preflight wrapper is returned inline as the contract's Ready value"
)]
pub(crate) enum RowPreflightStep<
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
    Ready(PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>),
    FlushRequired { target: Plan4Target },
}

/// Upper bound of A's driver share at each C call of a Row write: what the
/// driver holds while preflight, the lanes, reserve, prepare, apply, cancel or
/// finish runs (S2-0 Row write amendment §4). Every RowWrite C phase is checked
/// as `C body + carriers + ROW_WRITE_CALLER_BYTES ≤ CatalogProgressCharge` in
/// both writers (S2W-C01/C02); A checks its own call-site shares against it
/// (MCA00-D15). It must not exceed the smallest room that the C phase
/// estimates leave in either writer; a negative room is a Q06 overrun, never
/// a reason for a smaller constant. At S2-a the tightest phase is the Forward
/// prepare (`ReservedRow::prepare`), which leaves 58 B in both writers
/// (Reserve 218 B, Cancel 178 B): far below the A driver's measured share, so
/// A's call-site check cannot pass until the C bodies or the A driver shrink
/// (Q06). The CLR task's prepare is S4's own entry census, not a phase here.
/// Changing the value reruns the census of every committed RowWrite phase.
pub(crate) const ROW_WRITE_CALLER_BYTES: usize = 58;

/// The fixed RowWrite charge: the split-time CatalogProgressCharge that
/// `OperationSourceAdmission::progress(RowWrite)` reserves, as a byte count
/// for the A-side census. Not a grant and not an admission.
pub(crate) fn row_write_charge<W: WriterNamespace>() -> usize {
    super::super::source::progress_charge_bytes::<W>()
}

/// One RowWrite C phase as the Row write census charges it (S2W-C01/C02).
#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct RowWritePhase {
    pub(in crate::storage) name: &'static str,
    /// The C body upper estimate with the progress shell (`RowWriteBodies`).
    pub(in crate::storage) body: usize,
    /// The step's own receiver/argument values and its Result/return
    /// carriers, counted twice for a by-value result (callee and forward).
    pub(in crate::storage) carriers: usize,
}

/// The eight RowWrite C phases (S2-0 Row write amendment §4): the six effect
/// and release steps and the two lane issues. `prepare` is the Forward
/// `ReservedRow::prepare`; the CLR task's prepare is S4's. A's share at each call is
/// `ROW_WRITE_CALLER_BYTES`, added by the caller of this census.
pub(in crate::storage) fn row_write_phases<W: WriterNamespace>() -> [RowWritePhase; 8] {
    use std::mem::size_of;
    type Pre<'a, W> = PreflightRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Res<'a, W> = ReservedRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Prep<'a, W> = PreparedRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Step<'a, W> = RowPreflightStep<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Loan<'a> = &'a mut [u8; 425_984];
    let bodies = super::super::create_pair::catalog_live::row_write_bodies::<W>();
    let reference = size_of::<&()>();
    // Receiver, Owner, statement, table, private and flusher; the cut being
    // recorded; the wrapper assembled into the Ready step.
    let preflight = 6 * reference
        + size_of::<RowPreflightCut>()
        + size_of::<Pre<'_, W>>()
        + 2 * size_of::<Result<Step<'_, W>>>();
    let validation_lane = reference + 2 * size_of::<Result<RowValidationLane<'_>>>();
    let reserve = size_of::<Pre<'_, W>>()
        + size_of::<RowInputPlan>()
        + size_of::<RowSlotReservation<'_, '_>>()
        + size_of::<RowDraftCut>()
        + 2 * size_of::<std::result::Result<Res<'_, W>, RowOwnedError<Pre<'_, W>>>>();
    let producer_lane = reference + 2 * size_of::<Result<RowProducerLane<'_>>>();
    let prepare = size_of::<Res<'_, W>>()
        + size_of::<RowProposal>()
        + size_of::<RowPreparedCut>()
        + 2 * size_of::<std::result::Result<Prep<'_, W>, RowOwnedError<Res<'_, W>>>>();
    let apply = size_of::<Prep<'_, W>>()
        + size_of::<RowMutationObservation>()
        + 2 * size_of::<std::result::Result<RowApplied<'_>, RowOwnedError<Prep<'_, W>>>>();
    let cancel = size_of::<Res<'_, W>>()
        + 2 * size_of::<std::result::Result<RowCancelled<'_>, RowOwnedError<Res<'_, W>>>>();
    let finish =
        size_of::<Pre<'_, W>>() + 2 * size_of::<Result<Loan<'_>>>() + 3 * size_of::<Result<()>>();
    let phase = |name, body, carriers| RowWritePhase {
        name,
        body,
        carriers,
    };
    [
        phase("preflight", bodies.preflight, preflight),
        phase("validation_lane", bodies.validation_lane, validation_lane),
        phase("reserve", bodies.reserve, reserve),
        phase("producer_lane", bodies.producer_lane, producer_lane),
        phase("prepare", bodies.prepare, prepare),
        phase("apply", bodies.apply, apply),
        phase("cancel", bodies.cancel, cancel),
        phase("finish", bodies.finish, finish),
    ]
}

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
/// finish or Drop return the admission, gate and lease in that order (field
/// order is the implicit drop order) and the loan, with no I/O and no
/// WAL/page/issuer change.
///
/// The gate is closed with `Owner::close_reads_for_terminal`, the foreground
/// entry Commit uses, so a maintenance Owner is refused as InvalidRole; not
/// with `close_reads_for_retirement`, which the lifecycle progress shell uses
/// (S2-0 Row write amendment §1 "gate").
///
/// No Drop impl, for the reason given at `RowIoLease`: a Drop impl here, or on
/// any field that carries `'r`, keeps `'r` alive to the end of the preflight
/// step match, and the `FlushRequired` arm can no longer borrow the flusher
/// (MBC00-P01). A Drop on ReservedRow or PreparedRow is not limited by this.
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
    gate: Option<TerminalReadGate<'c, 's>>,
    io: Option<RowIoLease<'r, 'c, 's, 'w>>,
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
    gate: Option<TerminalReadGate<'c, 's>>,
    io: Option<RowIoLease<'r, 'c, 's, 'w>>,
}

/// Holds the whole reservation, with its admission, gate and lease, and a
/// scalar cut of the checked proposal, not a borrowed frame slice; reopen
/// discards only that cut.
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
    /// Unsupported before any source read, flush check, gate, admission or
    /// effect until the actual schema/index/leaf/residency preflight adapter
    /// is supplied. The flusher is the session's original MaintenancePort;
    /// a Ready preflight keeps its flush control as the row's lease.
    pub(in crate::storage) fn preflight_row<'r, 't, 'v, 'i>(
        &'r mut self,
        _owner: &'r mut Owner<'c, 's>,
        _statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        _table: &'r CheckedTable<'c, 's>,
        _private: &'i mut [u8; 425_984],
        _flusher: &'r mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowPreflightStep<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
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
    /// Returns the loan after the checked admission release, the gate reopen
    /// and the lease end, in that order and with no I/O. The read set already
    /// registered by the actual source stays with the statement.
    pub(in crate::storage) fn finish(mut self) -> Result<&'i mut [u8; 425_984]> {
        let source = self.source.release_checked().map_err(RetentionError::from);
        let gate = self.gate.take().map_or(Ok(()), |gate| {
            gate.reopen_checked().map_err(|error| RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })
        });
        self.io = None;
        let cleanup = merge(source, gate);
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
