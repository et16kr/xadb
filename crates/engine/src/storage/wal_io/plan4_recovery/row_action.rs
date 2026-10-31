//! The recovered Row action of the closed facade (XADB-MODULE-ROW-RECOVERY-001,
//! boundaries `#row-recovery-s6`). `next_action` keeps the original `step`'s
//! selection, effects and flush continuation (deviation 1: no FlushRequired
//! action) and hands only a Row loser's next CLR to A as a `RecoveryRowTask`.
//! The task borrows the session's MutationPort, the Owner, the recovery
//! private lane and the progress value disjointly; it holds C's selected
//! draft cut and the gate/source admission until its apply (deviation 2: no
//! reference lease or Undo pin). C does not import A: the product driver
//! dispatches the action to A's `handle_recovered_clr`.
use super::super::plan4_pages::RetentionFlushStep;
use super::super::plan4_pages::apply::{PreparedRecoveredClr, RecoveredClr, RecoveredClrStep};
use super::*;
use crate::storage::disk::{
    RetentionError, RowOperation, RowOwnedError, RowProducerLane, RowProposal,
};
use crate::transaction::concurrency::Owner;

/// The next of the recovered facade.
#[allow(
    clippy::large_enum_variant,
    reason = "the fixed recovery census counts the inline task, without heap allocation"
)]
pub(crate) enum RecoveryAction<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    /// C's own step (Catalog/Row cleanup, Cancel, RollbackStart, AbortEnd,
    /// flush or VERIFY) ran; call `next_action` again.
    Progress(Plan4RecoveryProgress),
    /// A Row loser's next U needs A's CLR.
    RowClr(RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>),
    /// Cleanup, physical readback and STABILIZE are complete: `into_live`.
    Ready,
}

/// One recovered loser's CLR: C's selected task and the facade's progress.
/// No Clone/Copy and no raw constructor; dropped before its apply it
/// returns the admission checked and fences the original session.
pub(crate) struct RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    clr: RecoveredClr<'r, 'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    progress: &'r mut RecoveryProgressState,
    runtime: &'s Plan4RecoveryRuntime<'a, 'm, 'w, W>,
}

/// A task whose proposal C prepared; `reopen` returns the same task.
pub(crate) struct PreparedRecoveryRow<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    prepared: PreparedRecoveredClr<'r, 'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    progress: &'r mut RecoveryProgressState,
    runtime: &'s Plan4RecoveryRuntime<'a, 'm, 'w, W>,
}

#[allow(
    clippy::large_enum_variant,
    reason = "the same consuming task is returned inline for retry"
)]
pub(crate) enum RecoveryOwnedError<T> {
    /// Refused before the step's first effect; the task keeps its selection.
    Before { error: Plan4RecoveryError, owned: T },
    /// The original fence and the remaining recovery obligations stay.
    Fatal(Plan4RecoveryError),
}

fn bound<W: WriterNamespace>(
    runtime: &Plan4RecoveryRuntime<'_, '_, '_, W>,
    progress: &RecoveryProgressState,
    error: RetentionError,
) -> Plan4RecoveryError {
    runtime.bound_error(
        progress.phase,
        Plan4RecoveryCause::Retention(error.cause),
        error.effect,
    )
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The recovered action. Every C step and flush continuation runs as in
    /// `step`; a Row loser whose next U is a CLR is selected again from the
    /// original ATT/Jobs and returned to A. While the task lives, this
    /// recovered facade is borrowed.
    pub(crate) fn next_action<'r>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
    ) -> Result<RecoveryAction<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let actor = match self.advance(owner, flusher)? {
            Advanced::Progress(progress) if progress.ready => return Ok(RecoveryAction::Ready),
            Advanced::Progress(progress) => return Ok(RecoveryAction::Progress(progress)),
            Advanced::RowClr(actor) => actor,
        };
        let runtime = self.runtime;
        let _unwind = runtime.session_fence().unwind_guard();
        let progress = &mut self.progress;
        let (mutation, _, mut flush) = self.inner.split();
        let step = mutation
            .recovered_clr(owner, actor, &mut *self.private)
            .map_err(|e| bound(runtime, progress, e))?;
        match step {
            RecoveredClrStep::Candidate(clr) => Ok(RecoveryAction::RowClr(RecoveryRowTask {
                clr,
                progress,
                runtime,
            })),
            RecoveredClrStep::FlushRequired { target } => {
                let step = flush
                    .try_flush(flusher, &target)
                    .map_err(|e| bound(runtime, progress, e))?;
                progress.waiting_flush = matches!(
                    step,
                    RetentionFlushStep::WaitingWal
                        | RetentionFlushStep::WaitingSource
                        | RetentionFlushStep::RetiredPendingFinalize
                );
                progress.observation.phase = progress.phase;
                Ok(RecoveryAction::Progress(Plan4RecoveryProgress {
                    phase: progress.phase,
                    progressed: !progress.waiting_flush,
                    ready: false,
                }))
            }
        }
    }
}

impl<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The loser this task compensates.
    pub(crate) fn actor(&self) -> crate::storage::transaction_record::TxnRef {
        self.clr.actor()
    }
    /// A's CLR lane over C's selected draft cut; it ends before `prepare`.
    pub(crate) fn producer_lane<'l>(&'l mut self) -> Result<RowProducerLane<'l>> {
        let (runtime, progress) = (self.runtime, &*self.progress);
        self.clr
            .producer_lane()
            .map_err(|e| bound(runtime, progress, e))
    }
    /// C's preparation of A's proposal; Before returns the same task.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original task"
    )]
    pub(crate) fn prepare(
        self,
        proposal: RowProposal,
    ) -> std::result::Result<
        PreparedRecoveryRow<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        RecoveryOwnedError<Self>,
    > {
        let Self {
            clr,
            progress,
            runtime,
        } = self;
        match clr.prepare(proposal) {
            Ok(prepared) => Ok(PreparedRecoveryRow {
                prepared,
                progress,
                runtime,
            }),
            Err(RowOwnedError::Before { error, owned }) => Err(RecoveryOwnedError::Before {
                error: bound(runtime, progress, error),
                owned: Self {
                    clr: owned,
                    progress,
                    runtime,
                },
            }),
            Err(RowOwnedError::Fatal(error)) => {
                Err(RecoveryOwnedError::Fatal(bound(runtime, progress, error)))
            }
        }
    }
}

impl<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    PreparedRecoveryRow<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The CLR install. After it the loser's Status head is the selected
    /// next, the admission is returned and the facade is in Losers again.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure retains the original prepared task"
    )]
    pub(crate) fn apply(
        self,
    ) -> std::result::Result<Plan4RecoveryProgress, RecoveryOwnedError<Self>> {
        let Self {
            prepared,
            progress,
            runtime,
        } = self;
        match prepared.apply() {
            Ok(observation) if observation.operation() == RowOperation::Clr => {
                progress.phase = Plan4RecoveryPhase::Losers;
                progress.observation.phase = progress.phase;
                Ok(Plan4RecoveryProgress {
                    phase: progress.phase,
                    progressed: true,
                    ready: false,
                })
            }
            Ok(_) => Err(RecoveryOwnedError::Fatal(runtime.bound_error(
                progress.phase,
                Plan4RecoveryCause::Invariant,
                Effect::CallerMustResolve,
            ))),
            Err(RowOwnedError::Before { error, owned }) => Err(RecoveryOwnedError::Before {
                error: bound(runtime, progress, error),
                owned: Self {
                    prepared: owned,
                    progress,
                    runtime,
                },
            }),
            Err(RowOwnedError::Fatal(error)) => {
                Err(RecoveryOwnedError::Fatal(bound(runtime, progress, error)))
            }
        }
    }
    pub(crate) fn reopen(self) -> RecoveryRowTask<'r, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> {
        RecoveryRowTask {
            clr: self.prepared.reopen(),
            progress: self.progress,
            runtime: self.runtime,
        }
    }
}

/// The recovered task carriers for the recovery census (S6R-C01..C03).
#[cfg(test)]
pub(super) fn recovery_row_task_bytes<W: WriterNamespace + 'static>() -> (usize, usize, usize) {
    use std::mem::size_of;
    type T<W> = RecoveryRowTask<
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
    type P<W> = PreparedRecoveryRow<
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
    type A<W> = RecoveryAction<
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
    (
        size_of::<T<W>>(),
        size_of::<P<W>>(),
        size_of::<Result<A<W>>>(),
    )
}

/// S6R-C01..C03 (MBC40-V07..09): the recovered Row sites stay inside the
/// original Row charge and short source cap with the facade's next_action
/// frame and A's handler frame on the path, for the memory writer, the
/// native writer and its File. Source bounds of the Rust carriers, not
/// measured stack.
#[cfg(test)]
mod census {
    use super::*;
    use std::mem::size_of;
    type Closed<W> =
        Plan4Recovered<'static, 'static, 'static, 'static, 'static, 'static, 'static, 'static, W>;
    type Step<W> = RecoveredClrStep<
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
    fn inspect<W: WriterNamespace + 'static>(id: &str, label: &str) {
        let (task, prepared, action) = recovery_row_task_bytes::<W>();
        let (clr, prepared_clr) =
            super::super::super::plan4_pages::apply::recovered_clr_bytes::<W>();
        let (charge, shared, short, flush, sites) =
            super::super::super::plan4_pages::apply::recovered_row_census::<W>();
        let handler = crate::storage::row::insert::driver::recovered::handler_source_bytes::<W>();
        let producer = crate::storage::row::insert::local_source_carrier_bytes().1;
        // next_action's own frame: the facade borrow, both Owners, the
        // advance result, C's step result and the returned action, which
        // the product driver then holds while A's handler runs.
        let frame = 4 * size_of::<&()>()
            + size_of::<Result<Advanced>>()
            + size_of::<std::result::Result<Step<W>, RetentionError>>()
            + action
            + 512;
        println!(
            "{id} writer={label} charge={charge} shared={shared} short={short} facade={} progress={} task={task} prepared={prepared} action={action} clr={clr} prepared_clr={prepared_clr} handler={handler} producer={producer} frame={frame} flush={flush} file={}",
            size_of::<Closed<W>>(),
            size_of::<RecoveryProgressState>(),
            size_of::<W::File>()
        );
        // The task is the C task plus two references; nothing is boxed.
        assert_eq!(task, clr + 2 * size_of::<&()>());
        assert_eq!(prepared, prepared_clr + 2 * size_of::<&()>());
        for (name, body) in sites {
            let held = match name {
                "recovery.clr_lane" | "recovery.clr_prepare" | "recovery.clr_apply" => {
                    frame + handler
                }
                _ => frame,
            };
            println!(
                "{id} writer={label} site={name} body={body} frame={held} total={}",
                held + body
            );
            assert!(
                held + body <= charge,
                "{label} {name}: {} > {charge}",
                held + body
            );
        }
        println!(
            "{id} writer={label} site=recovery.clr body={producer} frame={} total={}",
            frame + handler,
            frame + handler + producer
        );
        assert!(frame + handler + producer <= charge);
        assert!(shared + charge <= short);
        assert!(flush + frame <= short, "{label} flush {}", flush + frame);
        assert!(size_of::<W::File>() <= 16_384);
    }
    #[test]
    fn s6r_c01_memory_recovery_row_census() {
        inspect::<crate::storage::wal_io::plan4_test_support::Writer>("S6R_C01", "memory");
    }
    #[test]
    fn s6r_c02_native_recovery_row_census() {
        inspect::<super::super::super::plan4_pages::native::Writer>(
            "S6R_C02",
            "native-instrumented",
        );
    }
    /// The progress move adds no owner: the facade is its former fields.
    #[test]
    fn s6r_c03_recovered_facade_carriers() {
        fn facade<W: WriterNamespace + 'static>(label: &str) {
            let fields = size_of::<&()>()
                + size_of::<
                    crate::storage::disk::DiskSession<
                        'static,
                        'static,
                        'static,
                        'static,
                        'static,
                        'static,
                        'static,
                        'static,
                        W,
                    >,
                >()
                + size_of::<Plan4RecoveryPhase>()
                + size_of::<Plan4RecoveryObservation>()
                + 2 * size_of::<bool>()
                + size_of::<&()>()
                + size_of::<Option<super::super::super::plan4_pages::VisibilityLoan<'static>>>();
            println!(
                "S6R_C03 writer={label} facade={} fields={fields} progress={}",
                size_of::<Closed<W>>(),
                size_of::<RecoveryProgressState>()
            );
            assert!(size_of::<Closed<W>>() <= fields + 16);
            assert!(
                size_of::<RecoveryProgressState>() <= size_of::<Plan4RecoveryObservation>() + 16
            );
        }
        facade::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
        facade::<super::super::super::plan4_pages::native::Writer>("native-instrumented");
    }
}
