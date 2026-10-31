//! The recovered Row CLR task of the closed facade (XADB-MODULE-ROW-RECOVERY-001,
//! boundaries `#row-recovery-s6`). The selected AbortingFull loser's next U is
//! a CLR that A produces; C selects its retained head from the original
//! ATT/Jobs through the live selector, holds the terminal read gate and the
//! RowWrite source admission from selection until the apply, and prepares
//! and installs A's proposal through the live CLR bodies. There is no
//! statement, no reference lease and no Undo pin: before READY the facade
//! admits no reader, and retirement is sequential in the same driver
//! (deviation 2). A task dropped before its apply returns its admission
//! checked and fences the original session.
use super::super::create_pair::catalog_live::row_preflight::rollback::{
    RollbackCandidate, RollbackSelection,
};
use super::super::source::{OperationClass, OperationSourceAdmission};
use super::*;
use crate::storage::transaction_record::{RollbackState, TransactionState};
use crate::transaction::concurrency::TerminalReadGate;

/// The next of a recovered CLR selection: A's task, or the dirty victim the
/// original flusher writes before the same loser is selected again.
#[allow(
    clippy::large_enum_variant,
    reason = "the fixed recovery census counts the inline task, without heap allocation"
)]
pub(crate) enum RecoveredClrStep<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    Candidate(RecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>),
    FlushRequired { target: Plan4Target },
}

/// The selected CLR of one recovered loser: the original mutation port,
/// Owner and private loan, the C draft cut of the selection and the held
/// gate/source admission. No Clone/Copy and no raw constructor.
pub(crate) struct RecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    mutation: MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &'x mut Owner<'c, 's>,
    private: &'x mut [u8; 425_984],
    actor: TxnRef,
    candidate: RollbackCandidate,
    source: Option<OperationSourceAdmission<'b, 'a, 'c, 's>>,
    gate: Option<TerminalReadGate<'c, 's>>,
}

/// A recovered CLR whose proposal C prepared; `reopen` returns the task.
pub(crate) struct PreparedRecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    task: RecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    checked: RowPreparedCut,
}

fn release(
    source: Option<OperationSourceAdmission<'_, '_, '_, '_>>,
    gate: Option<TerminalReadGate<'_, '_>>,
) -> Result<()> {
    let source = source.map_or(Ok(()), |s| s.finish_checked().map_err(RetentionError::from));
    let gate = gate.map_or(Ok(()), |g| {
        g.reopen_checked().map_err(|e| RetentionError {
            phase: RetentionPhase::Finalize,
            cause: RetentionCause::Concurrency(e),
            effect: e.effect,
        })
    });
    merge(source, gate)
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Select the recovered loser's CLR under the terminal read gate and the
    /// RowWrite source admission. The loser must be AbortingFull with the
    /// next U at its Status head; the live selector proves its sources
    /// against canonical replay. A residency step releases the admission and
    /// returns the victim.
    pub(in crate::storage) fn recovered_clr<'x>(
        self,
        owner: &'x mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'x mut [u8; 425_984],
    ) -> Result<RecoveredClrStep<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
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
            Err(error) => {
                return merge(Err(RetentionError::from(error)), release(None, Some(gate)));
            }
        };
        let bad = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let selected = (|| {
            let tx = self.row_rollback_state(actor)?;
            let head = tx.undo_head().ok_or_else(bad)?;
            if tx.state()
                != TransactionState::Aborting(RollbackState::Transaction { next: Some(head) })
            {
                return Err(bad());
            }
            let selection = self.row_rollback_select(owner, actor, None, private)?;
            // The selector's candidate is the Status head named above.
            if matches!(&selection, RollbackSelection::Candidate(c) if c.issued.undo != head) {
                return Err(bad());
            }
            Ok(selection)
        })();
        match selected {
            Ok(RollbackSelection::Candidate(candidate)) => {
                Ok(RecoveredClrStep::Candidate(RecoveredClr {
                    mutation: self,
                    owner,
                    private,
                    actor,
                    candidate,
                    source: Some(source),
                    gate: Some(gate),
                }))
            }
            Ok(RollbackSelection::Flush(target)) => {
                release(Some(source), Some(gate))?;
                Ok(RecoveredClrStep::FlushRequired { target })
            }
            // The state names a next U: no candidate is the selector's own
            // contradiction.
            Ok(RollbackSelection::Complete) => merge(Err(bad()), release(Some(source), Some(gate))),
            Err(error) => merge(Err(error), release(Some(source), Some(gate))),
        }
    }
}

impl<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    fn check_admitted(&self) -> Result<()> {
        if self.source.is_none() || self.gate.is_none() {
            return Err(fatal(
                self.mutation.storage,
                RetentionPhase::Install,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
    /// A's CLR producer lane over the selected draft cut, as the live
    /// rollback task's: no statement view (full rollback scope).
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self) -> Result<RowProducerLane<'l>> {
        self.check_admitted()?;
        let c = &self.candidate;
        let private = &mut *self.private;
        let context = self.mutation.row_producer_context(
            self.owner,
            None,
            (c.table, &c.draft, c.issued),
            private,
        )?;
        let control = self.mutation.row_control_observation(self.actor)?;
        let (frame, rest) = private.split_at_mut(RowLoanLayout::FRAME.end);
        let (before, rest) = rest.split_at_mut(range_len(RowLoanLayout::BEFORE));
        let (after, rest) = rest.split_at_mut(range_len(RowLoanLayout::AFTER));
        let (candidate, rest) = rest.split_at_mut(range_len(RowLoanLayout::CANDIDATE));
        let (schema, rest) = rest.split_at_mut(range_len(RowLoanLayout::SCHEMA));
        let scratch = &mut rest[RowLoanLayout::A_SCRATCH.start - RowLoanLayout::XOI.start..];
        scratch.fill(0);
        Ok(RowProducerLane {
            context,
            control,
            before: before
                .as_chunks::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("nine before"),
            schema: (&*schema).try_into().expect("schema"),
            frame: frame.try_into().expect("frame"),
            after: after
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("five after"),
            candidate: candidate.try_into().expect("candidate"),
            scratch: scratch.try_into().expect("A scratch"),
        })
    }
    /// C's CLR preparation of A's proposal; an Unchanged refusal returns the
    /// same task.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original task"
    )]
    pub(in crate::storage) fn prepare(
        self,
        proposal: RowProposal,
    ) -> std::result::Result<
        PreparedRecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RowOwnedError<Self>,
    > {
        let result = match self.check_admitted() {
            Ok(()) => {
                let c = &self.candidate;
                self.mutation.row_prepare_clr(
                    self.owner,
                    c.table,
                    &c.draft,
                    c.issued,
                    &proposal,
                    self.private,
                )
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(checked) => Ok(PreparedRecoveredClr {
                task: self,
                checked,
            }),
            Err(error) if error.effect == Effect::Unchanged => {
                Err(RowOwnedError::Before { error, owned: self })
            }
            Err(error) => Err(RowOwnedError::Fatal(error)),
        }
    }
}

impl<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreparedRecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The CLR install through the live body. After it the Status head is
    /// the selected next and the admission is returned checked; an
    /// Unchanged refusal returns the prepared task.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original prepared task"
    )]
    pub(in crate::storage) fn apply(
        mut self,
    ) -> std::result::Result<RowMutationObservation, RowOwnedError<Self>> {
        let t = &mut self.task;
        let result = match t.check_admitted() {
            Ok(()) => {
                let c = &t.candidate;
                t.mutation.row_apply_clr(
                    t.owner,
                    c.table,
                    &c.draft,
                    c.issued,
                    &self.checked,
                    t.private,
                )
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => {
                let storage = t.mutation.storage;
                let fatal_cause = |cause| fatal(storage, RetentionPhase::Install, cause);
                let tx = t
                    .mutation
                    .row_rollback_state(t.actor)
                    .map_err(|e| RowOwnedError::Fatal(fatal_cause(e.cause)))?;
                if tx.undo_head() != t.candidate.next.map(|link| link.reference) {
                    return Err(RowOwnedError::Fatal(fatal_cause(RetentionCause::Invariant)));
                }
                release(t.source.take(), t.gate.take())
                    .map_err(|e| RowOwnedError::Fatal(fatal_cause(e.cause)))?;
                Ok(RowMutationObservation {
                    row: t.candidate.issued.row,
                    undo: t.candidate.issued.undo,
                    operation: RowOperation::Clr,
                })
            }
            Err(error) if error.effect == Effect::Unchanged => {
                Err(RowOwnedError::Before { error, owned: self })
            }
            Err(error) => Err(RowOwnedError::Fatal(error)),
        }
    }
    pub(in crate::storage) fn reopen(
        self,
    ) -> RecoveredClr<'x, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        self.task
    }
}

impl<W: WriterNamespace> Drop for RecoveredClr<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Dropped before its apply: the admission is returned checked and the
    /// original session is fenced; nothing else is retried.
    fn drop(&mut self) {
        if self.source.is_some() || self.gate.is_some() {
            let _ = release(self.source.take(), self.gate.take());
            self.mutation.storage.session_fence().fence();
        }
    }
}

/// The C bodies of the recovered Row sites for the recovery census
/// (S6R-C01/C02), as S4_C01 counts the live rollback's: (charge, shared,
/// short, flush body, sites as (name, body)). The facade and A's handler
/// frames are added by the caller.
#[cfg(test)]
pub(in crate::storage) fn recovered_row_census<W: WriterNamespace + 'static>()
-> (usize, usize, usize, usize, Vec<(&'static str, usize)>) {
    use super::super::{create_pair::catalog_live as c, resources, source};
    use crate::storage::semantic_row::status::StatementBoundary;
    use std::mem::size_of;
    type T<W> = RecoveredClr<
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        W,
    >;
    let charge = row_write_charge::<W>();
    let shared = source::shared_bytes::<W>();
    let existing = c::row_write_bodies::<W>();
    let phases = super::write::row_write_phases::<W>();
    let phase = |name| {
        let p = phases.iter().find(|p| p.name == name).expect("phase");
        p.body + p.carriers
    };
    let wrapper = 16 * size_of::<&()>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<TransactionState>()
        + size_of::<RollbackState>()
        + 2 * size_of::<Option<OperationSourceAdmission<'_, '_, '_, '_>>>()
        + 2 * size_of::<Option<TerminalReadGate<'_, '_>>>();
    // The live selector, the gate/source admission and the task's own
    // Result carrier; no reference lease or Undo pin (deviation 2).
    let selection = c::row_preflight::rollback::source_bytes::<W>()
        + size_of::<Result<RecoveredClrStep<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>()
        + size_of::<Result<RollbackSelection>>()
        + wrapper;
    let group = existing.group_install
        + size_of::<
            super::super::create_pair::catalog_live::lifecycle::row::PreparedRowGroup<
                '_,
                '_,
                '_,
                9,
            >,
        >()
        + size_of::<crate::storage::semantic_row::transition::Observation>()
        + 2 * size_of::<Result<()>>();
    // Recovered RollbackStart/AbortEnd: S4's control body.
    let control = c::row_commit_prepare_source_bytes::<W>().max(group)
        + c::lifecycle::row::rollback_preparation_packet_bytes::<W>()
        + 4 * size_of::<TargetRequest>()
        + size_of::<Option<UndoRef>>()
        + size_of::<Option<StatementBoundary>>()
        + size_of::<crate::storage::semantic_row::Operation>()
        + 2 * size_of::<Result<crate::storage::semantic_row::transition::Observation>>()
        + 2 * size_of::<Result<crate::storage::transaction_record::TransactionStatus>>()
        + wrapper;
    // Recovered Cancel/Prune/ReleaseRole/RootFree/TerminalFinish: the
    // existing-group RowRecovery install of up to nine targets selected from
    // the canonical arena, and the closed facade check before and at install.
    let closed = 2 * size_of::<super::super::visibility::VisibilityResult<()>>()
        + 4 * size_of::<Option<UndoRef>>()
        + 2 * size_of::<Option<TxnRef>>()
        + 2 * size_of::<Option<u64>>()
        + 512;
    let cleanup = c::row_commit_prepare_source_bytes::<W>().max(group)
        + c::lifecycle::row::rollback_preparation_packet_bytes::<W>()
        + 2 * size_of::<(Option<usize>, [(TargetRequest, u8); 9])>()
        + 2 * size_of::<Result<(Option<usize>, [(TargetRequest, u8); 9])>>()
        + closed
        + wrapper;
    let flush = shared + resources::source_census::<W>().flush + 2 * size_of::<Result<()>>();
    let sites = vec![
        ("recovery.select", selection),
        ("recovery.control", control),
        ("recovery.cleanup", cleanup),
        (
            "recovery.clr_lane",
            phase("producer_lane") + wrapper + size_of::<T<W>>(),
        ),
        ("recovery.clr_prepare", phase("prepare") + wrapper),
        ("recovery.clr_apply", phase("apply") + wrapper),
        ("recovery.drop", wrapper),
    ];
    (charge, shared, source::SHORT_BYTES as usize, flush, sites)
}

/// The recovered CLR's owned carriers for the recovery census
/// (S6R-C01..C03): the task, its prepared form and their Result carriers.
#[cfg(test)]
pub(in crate::storage) fn recovered_clr_bytes<W: WriterNamespace + 'static>() -> (usize, usize) {
    use std::mem::size_of;
    type T<W> = RecoveredClr<
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        W,
    >;
    type P<W> = PreparedRecoveredClr<
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        W,
    >;
    (size_of::<T<W>>(), size_of::<P<W>>())
}
