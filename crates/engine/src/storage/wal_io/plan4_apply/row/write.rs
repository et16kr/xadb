//! Preflight, RowReserve, producer lane, prepare/apply and cancel. Each step
//! consumes the previous wrapper; one source admission, one terminal read
//! gate, one flush lease and one reservation move through the chain instead
//! of being acquired again (S2-0 Row write amendment §1).
use super::super::create_pair::catalog_live::row_preflight::{RowPreflightOutcome, RowTableSource};
use super::super::flush::RetentionFlushControl;
use super::super::source::{OperationClass, OperationSourceAdmission};
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
/// from preflight. It is the only retry request: an Unchanged `Err`, Busy
/// included (a LOAD claim refused, or the WAL prefix moved during the
/// preflight), has also returned the source, gate and lease, but it ends the
/// row with no retry. Neither this step nor `PreflightRow` has a Drop impl, for
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

/// Upper bound of A's driver share at the preflight call of a Row write: what
/// the driver holds while `preflight_row` runs, before it holds any Row
/// wrapper (S2-0b amendment, decision 3). The preflight phase is checked as
/// `C body + carriers + ROW_PREFLIGHT_CALLER_BYTES ≤ CatalogProgressCharge` in
/// both writers (S2W-C01/C02/C06), and A checks its preflight call-site share
/// against it (MCA00-D15, counted per call site). The value is fixed from A's
/// side: the estimated 4.0–4.5 KB preflight share plus at least 1.6 KB for
/// A-W1's flush retry and slice references, rounded to 1 KiB. It is not the
/// largest value that fits. A negative C margin is a Q06 overrun, never a
/// reason for a smaller constant; preflight's ladder is (E), then (d4), each
/// through its own D2. Changing the value reruns the census of every
/// committed RowWrite phase.
pub(crate) const ROW_PREFLIGHT_CALLER_BYTES: usize = 6144;

/// Upper bound of A's driver share at every other C call of a Row write: what
/// the driver holds while the lanes, reserve, prepare, apply, cancel or
/// finish runs (S2-0 Row write amendment §4, S2-0b amendment decision 3).
/// Each of those phases is checked as `C body + carriers +
/// ROW_WRITE_CALLER_BYTES ≤ CatalogProgressCharge` in both writers
/// (S2W-C01/C02/C06), and A checks each of those call-site shares against it
/// (MCA00-D15, counted per call site), not its path-wide inventory. The value
/// is fixed from A's side: the A driver's path-wide inventory (D12, 11355 B)
/// rounded up to 256 B. It is not the largest value that fits. A negative C
/// margin is a Q06 overrun, never a reason for a smaller constant; the ladder
/// is (d3), the group terms at each phase's largest actual target count.
/// `prepare` is the Forward `ReservedRow::prepare`; the CLR task's prepare is
/// S4's own entry census, not a phase here. Changing the value reruns the
/// census of every committed RowWrite phase.
pub(crate) const ROW_WRITE_CALLER_BYTES: usize = 11520;

/// The fixed RowWrite charge: the split-time CatalogProgressCharge that
/// `OperationSourceAdmission::progress(RowWrite)` reserves, as a byte count
/// for the A-side census. Not a grant and not an admission.
pub(crate) fn row_write_charge<W: WriterNamespace>() -> usize {
    super::super::source::progress_charge_bytes::<W>()
}

/// One RowWrite C phase as the Row write census charges it
/// (S2W-C01/C02/C06).
#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct RowWritePhase {
    pub(in crate::storage) name: &'static str,
    /// The C body upper estimate with the progress shell (`RowWriteBodies`).
    pub(in crate::storage) body: usize,
    /// The step's own receiver/argument values and its Result/return
    /// carriers, counted twice for a by-value result (callee and forward).
    pub(in crate::storage) carriers: usize,
    /// The A share bound at this phase's call site: `ROW_PREFLIGHT_CALLER_BYTES`
    /// for preflight, `ROW_WRITE_CALLER_BYTES` for every other phase.
    pub(in crate::storage) bound: usize,
}

/// The eight RowWrite C phases (S2-0 Row write amendment §4): the six effect
/// and release steps and the two lane issues. `prepare` is the Forward
/// `ReservedRow::prepare`; the CLR task's prepare is S4's. A's share at each
/// call is the phase's `bound`, added by the caller of this census.
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
    let phase = |name, body, carriers, bound| RowWritePhase {
        name,
        body,
        carriers,
        bound,
    };
    let (first, other) = (ROW_PREFLIGHT_CALLER_BYTES, ROW_WRITE_CALLER_BYTES);
    [
        phase("preflight", bodies.preflight, preflight, first),
        phase(
            "validation_lane",
            bodies.validation_lane,
            validation_lane,
            other,
        ),
        phase("reserve", bodies.reserve, reserve, other),
        phase("producer_lane", bodies.producer_lane, producer_lane, other),
        phase("prepare", bodies.prepare, prepare, other),
        phase("apply", bodies.apply, apply, other),
        phase("cancel", bodies.cancel, cancel, other),
        phase("finish", bodies.finish, finish, other),
    ]
}

/// The one actual Root/Job/UndoSlot reservation of a ReservedRow. Only C
/// creates it after the Root/Job/Undo before checks, the original WAL Reserve
/// and whole installation. Not Clone and not transferable to another actor.
/// While active it is a durable obligation only a completed apply or cancel
/// discharges (S2-c, S2-d); dropping it active fences the session
/// (S2W-R12, S2W-R23(2)). It does no I/O.
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
impl Drop for RowSlotReservation<'_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.fence.fence();
        }
    }
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
    /// The Row write preflight (S2-0 Row write amendment §1 "order", S2-0b
    /// amendment `#row-write-validate-once`). Before the gate: the guardless
    /// health and origin, the view's kind (a Catalog or read-only view is
    /// Unsupported), origin and phase, the CheckedTable's origin, and the
    /// flusher's binding with `check_catalog_pair_io`. Then the terminal read
    /// gate, the RowWrite admission, and the body (`row_preflight`): deep
    /// checks, residency through the retention LOAD, Free Undo selection, the
    /// row's one `validate_live` and the arena comparison of T, registration
    /// and the cut. A Ready preflight keeps the admission, the gate and the
    /// flusher's control as the row's lease. A refusal after the gate and a
    /// `FlushRequired` return the admission and the gate checked, in that
    /// order, before the lease borrow ends; neither registers anything.
    pub(in crate::storage) fn preflight_row<'r, 't, 'v, 'i>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        table: &'r CheckedTable<'c, 's>,
        private: &'i mut [u8; 425_984],
        flusher: &'r mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowPreflightStep<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        // Guardless health and origin.
        storage.health().map_err(|error| {
            fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Storage(error),
            )
        })?;
        self.pool
            .check_retention_entry_health()
            .map_err(|error| frame(RetentionPhase::Prepare, error))?;
        if !self
            .pool
            .uses_storage_origin(&storage.binding, storage.session_fence())
        {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        // Kind and view: a Catalog or read-only view is the mixed refusal;
        // a foreign or ended view is the view boundary's own refusal.
        if !statement.row_scope() {
            return Err(unsupported());
        }
        statement.catalog_snapshot(self.isolation, self.visibility)?;
        statement.check_catalog_read()?;
        if !std::ptr::eq(table.core, self.pool.bound_core())
            || !std::ptr::eq(table.binding, &storage.binding)
            || !std::ptr::eq(table.fence, storage.session_fence())
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        if !std::ptr::eq(flusher.pool, self.pool) || !std::ptr::eq(flusher.storage, storage) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        flusher
            .control
            .check_catalog_pair_io::<W>(&storage.binding, storage.session_fence())?;
        let gate = owner
            .close_reads_for_terminal(self.pool.bound_core())
            .map_err(|error| RetentionError {
                phase: RetentionPhase::Prepare,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })?;
        let source = match OperationSourceAdmission::progress(
            self.isolation,
            &self.commit_charges,
            &self.progress_charge,
            &gate,
            OperationClass::RowWrite,
        ) {
            Ok(source) => source,
            Err(error) => return merge(Err(RetentionError::from(error)), reopen(gate)),
        };
        let outcome = self.row_preflight(
            owner,
            statement,
            RowTableSource {
                handle: table.table,
                slot: table.slot,
                anchor: table.anchor,
            },
            private,
        );
        match outcome {
            Ok(RowPreflightOutcome::Ready(source_cut)) => {
                Ok(RowPreflightStep::Ready(PreflightRow {
                    mutation: self,
                    owner,
                    statement,
                    table,
                    private: Some(private),
                    source_cut,
                    source,
                    gate: Some(gate),
                    io: Some(RowIoLease {
                        original: &mut *flusher.control,
                    }),
                }))
            }
            Ok(RowPreflightOutcome::FlushRequired(target)) => {
                release(source, gate).map(|()| RowPreflightStep::FlushRequired { target })
            }
            Err(error) => merge(Err(error), release(source, gate)),
        }
    }
}

#[cfg(test)]
impl<W: WriterNamespace> PreflightRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// S2W-R03: the cut's scalars (source commitment, complete LSN, role map,
    /// RowLeaf free bytes, RootJob); it cannot change the cut.
    pub(in crate::storage) fn test_cut(&self) -> ([u8; 32], u64, u64, u16, u8) {
        let cut = &self.source_cut;
        (
            cut.source,
            cut.complete_lsn,
            cut.roles,
            cut.leaf_free,
            cut.root_job,
        )
    }
}

#[cfg(test)]
impl<W: WriterNamespace> PreflightRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// S2-b1 review A2b: SHA-256 of the SCHEMA and the BEFORE ranges of the
    /// loan, which preflight wrote and neither `validation_lane` nor a
    /// refused `reserve` may change. It cannot change the loan.
    pub(in crate::storage) fn test_copy_digests(&self) -> Option<([u8; 32], [u8; 32])> {
        let private = self.private.as_deref()?;
        Some((
            foundation::codec::content_digest(&private[RowLoanLayout::SCHEMA]),
            foundation::codec::content_digest(&private[RowLoanLayout::BEFORE]),
        ))
    }
    /// S2W-R07: the next `reserve` commitment recompute on this thread sees
    /// T's first stamp with its modification counter moved by one, once,
    /// until the returned guard drops. No page, control or WAL byte changes.
    pub(in crate::storage) fn test_stale_stamp_once(
        &self,
    ) -> super::super::transaction::writable::TestOnce {
        super::super::create_pair::catalog_live::lifecycle::row::write::test_stale_stamp_once()
    }
    /// S2-b2 review A6: the next Row write install on this thread is refused
    /// Unchanged (Stage, Busy) after its table write bit and before the
    /// existing-group install, once, until the returned guard drops.
    pub(in crate::storage) fn test_stage_refusal_once(
        &self,
    ) -> super::super::transaction::writable::TestOnce {
        super::super::create_pair::catalog_live::lifecycle::row::write::test_stage_refusal_once()
    }
}

#[cfg(test)]
impl<W: WriterNamespace> ReservedRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// S2W-R06: the issued identity (actor, RootJob slot, RowId, UndoRef,
    /// Reserve LSN); it cannot change the reservation.
    pub(in crate::storage) fn test_issued(&self) -> (TxnRef, JobSlotRef, RowId, UndoRef, u64) {
        let reservation = &self.reservation;
        (
            reservation.actor,
            reservation.root_job,
            reservation.row,
            reservation.undo,
            reservation.reserve_lsn,
        )
    }

    /// S2W-R06: the draft cut's scalars (source, input, complete LSN, planned
    /// mutation, role map, input length, operation).
    pub(in crate::storage) fn test_draft(&self) -> ([u8; 32], [u8; 32], u64, u64, u64, u16, u8) {
        let draft = &self.draft;
        (
            draft.source,
            draft.input,
            draft.complete_lsn,
            draft.planned_mutation,
            draft.roles,
            draft.input_len,
            draft.operation,
        )
    }
}

/// Reopen a terminal read gate with its Census error, as `finish` does.
fn reopen(gate: TerminalReadGate<'_, '_>) -> Result<()> {
    gate.reopen_checked().map_err(|error| RetentionError {
        phase: RetentionPhase::Census,
        cause: RetentionCause::Concurrency(error),
        effect: error.effect,
    })
}

/// Return a preflight's admission, then its gate, checked; the lease borrow
/// ends with the caller's frame after both.
fn release(
    source: OperationSourceAdmission<'_, '_, '_, '_>,
    gate: TerminalReadGate<'_, '_>,
) -> Result<()> {
    let source = source.finish_checked().map_err(RetentionError::from);
    merge(source, reopen(gate))
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn table_source(&self) -> RowTableSource {
        RowTableSource {
            handle: self.table.table,
            slot: self.table.slot,
            anchor: self.table.anchor,
        }
    }

    /// A's validation lane (S2-b decision A7, `#row-write-validate-once`):
    /// the context rebuilt from preflight's private copies and the Catalog
    /// leaf stamp observed again (`row_validation_context`), the Catalog leaf
    /// copy and the RowLeaf before image read only, and the candidate range
    /// writable. It writes neither the SCHEMA nor the BEFORE range, so a retry
    /// after a Before(Stale) rebuilds from the same copies (S2-b1 review
    /// A2c). It checks nothing against the cut's commitment: `reserve`'s
    /// recompute refuses any change before an effect.
    pub(in crate::storage) fn validation_lane<'l>(&'l mut self) -> Result<RowValidationLane<'l>> {
        let storage = self.mutation.storage;
        let table = self.table_source();
        let private = self
            .private
            .as_deref_mut()
            .ok_or_else(|| fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let (context, leaf_at) =
            self.mutation
                .row_validation_context(self.owner, table, &self.source_cut, private)?;
        let (low, high) = private.split_at_mut(RowLoanLayout::CANDIDATE.start);
        let (candidate, high) = high.split_at_mut(range_len(RowLoanLayout::CANDIDATE));
        let schema = &high[..range_len(RowLoanLayout::SCHEMA)];
        let leaf = &low[RowLoanLayout::BEFORE.start + leaf_at * PAGE_BYTES..][..PAGE_BYTES];
        Ok(RowValidationLane {
            context,
            schema: schema.try_into().expect("schema copy page"),
            leaf: leaf.try_into().expect("RowLeaf before image"),
            candidate: candidate.try_into().expect("candidate page"),
        })
    }

    /// The only place for the first Reserve effect (`row_reserve`, S2-0 Row
    /// write amendment §1 "order", `#row-write-validate-once`). RowInputPlan
    /// is untrusted. An Unchanged refusal (the commitment recompute's
    /// Before(Stale), a forged candidate, Capacity, a WrongState actor) is
    /// Before with the same PreflightRow, which keeps its admission, gate,
    /// lease and loan; the caller retries from `validation_lane`. Any other
    /// failure is Fatal: the admission and the gate are returned checked, the
    /// lease ends, and the fence stays. A completed install moves the
    /// admission, gate, lease and loan into the ReservedRow with the one
    /// RowSlotReservation and the draft cut.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same preflight for retry"
    )]
    pub(in crate::storage) fn reserve(
        mut self,
        input: RowInputPlan,
    ) -> std::result::Result<
        ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RowOwnedError<Self>,
    > {
        let storage = self.mutation.storage;
        let table = self.table_source();
        let reserved = match self.private.as_deref_mut() {
            Some(private) => self.mutation.row_reserve(
                self.owner,
                self.statement,
                (table, &self.source_cut, &input),
                private,
            ),
            None => Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            )),
        };
        match reserved {
            Ok(reserved) => {
                let reservation = RowSlotReservation {
                    core: self.mutation.pool.bound_core(),
                    binding: &storage.binding,
                    fence: storage.session_fence(),
                    actor: reserved.actor,
                    root_job: reserved.root_job,
                    row: reserved.row,
                    undo: reserved.undo,
                    reserve_lsn: reserved.lsn,
                    active: true,
                };
                let PreflightRow {
                    mutation,
                    owner,
                    statement,
                    table,
                    private,
                    source_cut: _,
                    source,
                    gate,
                    io,
                } = self;
                Ok(ReservedRow {
                    mutation,
                    owner,
                    statement,
                    table,
                    private,
                    reservation,
                    draft: reserved.draft,
                    source,
                    gate,
                    io,
                })
            }
            Err(error) if error.effect == Effect::Unchanged => {
                Err(RowOwnedError::Before { error, owned: self })
            }
            Err(error) => {
                let PreflightRow { source, gate, .. } = self;
                let cleanup = match gate {
                    Some(gate) => release(source, gate),
                    None => source.finish_checked().map_err(RetentionError::from),
                };
                // `merge` keeps a CallerMustResolve error over the cleanup's.
                let error = match merge(Err(error), cleanup) {
                    Err(error) => error,
                    Ok(()) => fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant),
                };
                Err(RowOwnedError::Fatal(error))
            }
        }
    }
    /// Returns the loan after the checked admission release, the gate reopen
    /// and the lease end, in that order and with no I/O. The read set already
    /// registered by the actual source stays with the statement.
    pub(in crate::storage) fn finish(mut self) -> Result<&'i mut [u8; 425_984]> {
        let source = self.source.release_checked().map_err(RetentionError::from);
        let gate = self.gate.take().map_or(Ok(()), reopen);
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
