//! B's point/cursor driver over the C0 Row read boundary.
//!
//! B owns the order: page, B's slot choice on the leaf, capture, Status or
//! Missing-before only when the version needs it, copy, candidate release and
//! operation finish, and only then publication or progress. C owns every
//! latch, pin, Status/Undo read and the actual copy. The source is reached
//! through the narrow private traits below and is opened by the driver on the
//! same view it judges with (and, for a cursor, on the cursor's own table):
//! `live` opens the original operation from the ReadPort, and the tests feed
//! literal page bytes and logical answers. See
//! `devdoc/contracts/dev3-module-boundaries.md` (6.3, 6.8) and the F02 table in
//! `dev3-task-06-tree-rows.md`.
use super::{
    CursorProgress, ReadError, RowRange, SeekPosition, Visibility, check_output, seek_point,
    visible,
};
use crate::storage::{
    disk::RetentionError,
    row_record::RecordVersion,
    transaction_record::{TransactionState, TxnRef},
    tree_page::TreePageView,
};
use foundation::{error::Effect, identity::RowId, runtime::Cancellation};

mod live;

/// The statement observations B reads. The original view validates itself on
/// each call; a credit is RAM accounting only, never snapshot or pin authority.
trait ReadView {
    type Credit<'r>: CursorCredit
    where
        Self: 'r;
    fn q(&self) -> Result<u64, RetentionError>;
    fn own_actor(&self) -> Option<TxnRef>;
    fn revision(&self) -> u64;
    fn reserve_cursor_credit(&self) -> Result<Self::Credit<'_>, RetentionError>;
}

trait CursorCredit {
    fn release_checked(self) -> Result<(), RetentionError>;
}

/// Only what B needs from a captured candidate or a copy observation.
trait Observed {
    fn version(&self) -> RecordVersion;
    fn canonical_len(&self) -> usize;
}

/// One short source operation. Each phase reborrows the source; candidates
/// and copies are observations, and `Flush` carries C's own target.
trait ReadSource {
    type Target;
    type Candidate: Observed;
    type Missing;
    type Copied: Observed;
    type Page<'g>: SearchPage<Candidate = Self::Candidate>
    where
        Self: 'g;
    fn page(&mut self) -> Result<Step<Self::Page<'_>, Self::Target>, RetentionError>;
    fn status(
        &mut self,
        candidate: &Self::Candidate,
    ) -> Result<Step<TransactionState, Self::Target>, RetentionError>;
    fn missing_before(
        &mut self,
        candidate: &Self::Candidate,
    ) -> Result<Step<Self::Missing, Self::Target>, RetentionError>;
    fn copy_candidate(
        &mut self,
        candidate: &Self::Candidate,
        output: &mut [u8],
    ) -> Result<Self::Copied, RetentionError>;
    fn release_candidate(&mut self, candidate: Self::Candidate) -> Result<(), RetentionError>;
    fn finish(self) -> Result<(), RetentionError>;
}

trait SearchPage {
    type Candidate;
    fn leaf(&self) -> Result<TreePageView<'_>, RetentionError>;
    /// C rechecks the chosen slot against the latched row before any pin.
    fn capture(self, at: usize) -> Result<Self::Candidate, RetentionError>;
    fn release_checked(self) -> Result<(), RetentionError>;
}

/// A source phase or driver result; `Flush` is passed back unchanged after
/// the candidate and the operation have been released.
#[derive(Debug)]
enum Step<T, F> {
    Ready(T),
    Flush(F),
}

#[derive(Debug)]
enum PointRead<O> {
    Found(O),
    Missing,
}

#[derive(Debug)]
enum CursorStep<O> {
    Row(O),
    End,
}

#[derive(Debug)]
enum RowReadError {
    /// B's own decision; no source effect remains.
    Read(ReadError),
    /// The source answered for another row or version than B selected.
    SourceMismatch,
    /// C's error with its original cause and effect.
    Source(RetentionError),
}

impl RowReadError {
    fn effect(&self) -> Effect {
        match self {
            Self::Source(error) => error.effect,
            Self::Read(_) | Self::SourceMismatch => Effect::Unchanged,
        }
    }
}

impl From<ReadError> for RowReadError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

impl From<RetentionError> for RowReadError {
    fn from(error: RetentionError) -> Self {
        Self::Source(error)
    }
}

/// C's precedence: a caller-must-resolve result survives, otherwise a failed
/// checked cleanup replaces the result. Cleanup never relies on Drop alone.
fn merge<T>(
    result: Result<T, RowReadError>,
    cleanup: Result<(), RetentionError>,
) -> Result<T, RowReadError> {
    match (result, cleanup) {
        (Err(error), _) if error.effect() == Effect::CallerMustResolve => Err(error),
        (_, Err(error)) => Err(error.into()),
        (result, Ok(())) => result,
    }
}

/// A sampled value, not cancellation authority.
fn cancelled(cancel: &Cancellation) -> bool {
    cancel.check().is_err()
}

/// q and own actor sampled once per call from the original view.
#[derive(Clone, Copy)]
struct ViewSample {
    q: u64,
    own_actor: Option<TxnRef>,
}

impl ViewSample {
    fn of<V: ReadView>(view: &V) -> Result<Self, RowReadError> {
        Ok(Self {
            q: view.q()?,
            own_actor: view.own_actor(),
        })
    }
}

/// One captured candidate: `Some` is a complete copy of a visible version and
/// `None` an invisible version with C's Missing-before evidence. Errors, such
/// as a missing Undo, are never turned into absence.
fn select_row<S: ReadSource>(
    source: &mut S,
    candidate: &S::Candidate,
    at: SeekPosition,
    sample: ViewSample,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<Step<Option<S::Copied>, S::Target>, RowReadError> {
    let version = candidate.version();
    if version.row != at.row {
        return Err(RowReadError::SourceMismatch);
    }
    let mut choice = visible(sample.q, sample.own_actor, version, None)?;
    if choice == Visibility::ReadStatus {
        let state = match source.status(candidate)? {
            Step::Ready(state) => state,
            Step::Flush(target) => return Ok(Step::Flush(target)),
        };
        choice = visible(sample.q, sample.own_actor, version, Some(state))?;
    }
    match choice {
        Visibility::Current => {}
        Visibility::ReadMissingBefore => {
            return Ok(match source.missing_before(candidate)? {
                Step::Ready(_evidence) => Step::Ready(None),
                Step::Flush(target) => Step::Flush(target),
            });
        }
        // A resolved status never asks for another one.
        Visibility::ReadStatus => return Err(ReadError::UnexpectedStatus.into()),
    }
    // Shortage and cancellation are decided before C writes the output.
    check_output(candidate.canonical_len(), output.len())?;
    if cancelled(cancel) {
        return Err(ReadError::Cancelled.into());
    }
    let copied = source.copy_candidate(candidate, output)?;
    if copied.version() != version || copied.canonical_len() != candidate.canonical_len() {
        return Err(RowReadError::SourceMismatch);
    }
    Ok(Step::Ready(Some(copied)))
}

/// Point read over one operation that `open` creates on this same view and
/// that is always finished before returning. Found and Missing are published
/// only after the checked cleanup succeeds.
fn read_point<'r, V: ReadView, S: ReadSource>(
    view: &'r V,
    open: impl FnOnce(&'r V) -> Result<S, RetentionError>,
    row: RowId,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<Step<PointRead<S::Copied>, S::Target>, RowReadError> {
    // Statement and fence first, then cancellation; neither opens a source.
    let sample = ViewSample::of(view)?;
    if cancelled(cancel) {
        return Err(ReadError::Cancelled.into());
    }
    let mut source = open(view)?;
    let selected = select_point(&mut source, sample, row, output, cancel);
    merge(selected, source.finish())
}

fn select_point<S: ReadSource>(
    source: &mut S,
    sample: ViewSample,
    row: RowId,
    output: &mut [u8],
    cancel: &Cancellation,
) -> Result<Step<PointRead<S::Copied>, S::Target>, RowReadError> {
    let page = match source.page()? {
        Step::Ready(page) => page,
        Step::Flush(target) => return Ok(Step::Flush(target)),
    };
    let selected = match page.leaf() {
        Ok(leaf) => seek_point(&leaf, row).map_err(RowReadError::from),
        Err(error) => Err(error.into()),
    };
    let at = match selected {
        Ok(Some(at)) => at,
        // The actual leaf has no such key: an ordinary checked absence.
        Ok(None) => return merge(Ok(Step::Ready(PointRead::Missing)), page.release_checked()),
        Err(error) => return merge(Err(error), page.release_checked()),
    };
    let candidate = page.capture(at.index)?;
    let selected = select_row(source, &candidate, at, sample, output, cancel);
    Ok(
        match merge(selected, source.release_candidate(candidate))? {
            Step::Ready(Some(copied)) => Step::Ready(PointRead::Found(copied)),
            Step::Ready(None) => Step::Ready(PointRead::Missing),
            Step::Flush(target) => Step::Flush(target),
        },
    )
}

enum Scan<O> {
    Returned(RowId, O),
    End,
}

/// Opaque, non-Clone cursor. It borrows the original view and the table it
/// was opened on and holds its own credit, so the statement cannot finish
/// while the cursor is live and no call can switch the table under the kept
/// keys. Calls keep no page, slot or latch: each `next` reseeks by the full
/// last examined RowId.
struct RowCursor<'r, V: ReadView + 'r, T: 'r> {
    view: &'r V,
    table: &'r T,
    credit: V::Credit<'r>,
    progress: CursorProgress,
}

impl<'r, V: ReadView + 'r, T: 'r> RowCursor<'r, V, T> {
    /// Statement and fence first, then the range, then the credit, which is
    /// the only effect.
    fn open(
        view: &'r V,
        table: &'r T,
        start: Option<RowId>,
        end: Option<RowId>,
    ) -> Result<Self, RowReadError> {
        view.q()?;
        let range = RowRange::new(start, end)?;
        let credit = view.reserve_cursor_credit()?;
        Ok(Self {
            view,
            table,
            credit,
            progress: CursorProgress::new(range, view.revision()),
        })
    }

    /// Returns only this cursor's credit; the view and other cursors are not
    /// touched. Dropping instead returns it through the credit's own Drop.
    fn close(self) -> Result<(), RetentionError> {
        self.credit.release_checked()
    }

    /// One operation per call, opened by `open` on this cursor's own view
    /// and table. Invisible rows advance only `last_examined` after
    /// Missing-before and release; a returned row or End is recorded only
    /// after the operation's checked finish.
    fn next<S: ReadSource>(
        &mut self,
        open: impl FnOnce(&'r V, &'r T) -> Result<S, RetentionError>,
        output: &mut [u8],
        cancel: &Cancellation,
    ) -> Result<Step<CursorStep<S::Copied>, S::Target>, RowReadError> {
        // Statement first; revision precedes End and cancellation. None of
        // these opens an operation.
        let revision = self.view.revision();
        let sample = ViewSample::of(self.view)?;
        self.progress.check_revision(revision)?;
        if self.progress.exhausted {
            return Ok(Step::Ready(CursorStep::End));
        }
        if cancelled(cancel) {
            return Err(ReadError::Cancelled.into());
        }
        let mut source = open(self.view, self.table)?;
        let scanned = self.scan(&mut source, sample, revision, output, cancel);
        match merge(scanned, source.finish())? {
            Step::Flush(target) => Ok(Step::Flush(target)),
            Step::Ready(Scan::End) => {
                self.progress.finish_end(revision)?;
                Ok(Step::Ready(CursorStep::End))
            }
            Step::Ready(Scan::Returned(row, copied)) => {
                self.progress.finish_returned(row, revision)?;
                Ok(Step::Ready(CursorStep::Row(copied)))
            }
        }
    }

    fn scan<S: ReadSource>(
        &mut self,
        source: &mut S,
        sample: ViewSample,
        revision: u64,
        output: &mut [u8],
        cancel: &Cancellation,
    ) -> Result<Step<Scan<S::Copied>, S::Target>, RowReadError> {
        loop {
            let page = match source.page()? {
                Step::Ready(page) => page,
                Step::Flush(target) => return Ok(Step::Flush(target)),
            };
            let selected = match page.leaf() {
                Ok(leaf) => self
                    .progress
                    .seek(&leaf, revision, cancelled(cancel))
                    .map_err(RowReadError::from),
                Err(error) => Err(error.into()),
            };
            let at = match selected {
                Ok(Some(at)) => at,
                Ok(None) => return merge(Ok(Step::Ready(Scan::End)), page.release_checked()),
                Err(error) => return merge(Err(error), page.release_checked()),
            };
            let candidate = page.capture(at.index)?;
            let selected = self
                .progress
                .check_advance(at.row, revision)
                .map_err(RowReadError::from)
                .and_then(|()| select_row(source, &candidate, at, sample, output, cancel));
            match merge(selected, source.release_candidate(candidate))? {
                Step::Ready(Some(copied)) => {
                    return Ok(Step::Ready(Scan::Returned(at.row, copied)));
                }
                Step::Ready(None) => self.progress.finish_invisible(at.row, revision)?,
                Step::Flush(target) => return Ok(Step::Flush(target)),
            }
        }
    }
}

#[cfg(test)]
mod tests;
