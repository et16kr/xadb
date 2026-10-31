//! S2-e: A's INSERT driver in its live mode (`LiveCall`, `insert_live`) over
//! the actual C Row write wrappers and the actual committed CREATE of table T
//! that the preflight, reserve, Forward and Cancel cases use. The C steps are
//! the ones S2-b1..S2-d supplied; here A's driver calls them in its own order
//! and ends each row by its own policy (S2-0 Row write amendment §2, A's call
//! shape; consumers' MCA05-V31 reading and the `MCA00-D15` paragraph's "A의
//! 기존 Before 정책(finish·cancel)"). No product caller exists: the S2
//! contract names none, so these tests are `insert_live`'s callers (gap G2).
//!
//! The cases reuse the S2W fixtures and literals and carry their IDs with
//! `live` in the name: one row lands the R09 literal through the driver; a
//! dirty victim is flushed with the dedicated flusher Owner and the same row
//! retried from preflight (MCA05-V31); a reserve Before is finished without a
//! retry (R07, R11); a prepare or apply Before is cancelled by the driver
//! (R10, R11) and the Cancel lands the S2-d literal; a write target missing
//! after Ready is Fatal and must be resolved (R21(2)); a stale stamp at apply
//! is reopened and cancelled, and one at the Cancel after a refused apply
//! leaves the reservation to C's fence and the statement must be resolved
//! (R21(1)); a preflight Busy ends the row without a retry (R19). The S2W-R07
//! and R21(1) apply cases end their Returned statement through A's
//! `finish_statement_live`. `s2e_e09_…` is not a contract row: a
//! statement with an applied row and then a refused row is RollbackRequired,
//! and its rollback start is C's Unsupported (S4), so it ends in C's fence.
//! One-shots armed before the call are aimed at a step (`TestAimed`). A Row
//! transaction with a durable Forward has no healthy end before S3/S4, so
//! those cases end with the statement finished and the transaction dropped.
use super::super::support::namespace;
use super::row_cancel::CANCEL_FRAME;
use super::row_forward::{
    FORWARD_FRAME, PREVIOUS_LSN, RESERVE_FRAME, VALUE, files, stale, wal_since,
};
use super::row_preflight::{FREE_UNDO, SOURCES, create_table, evict, frames, with_pins};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RetentionCause, RetentionError, RetentionFlushPhase, RetentionPhase,
        RowMutationObservation, RowOperation, RowPreflightStep, TableStep,
    },
    retention_page::row::{self as job3, JobState},
    row::insert::driver::{
        After, DriveError, DriverError, Failure, PendingRollback, Stage,
        live::{LiveCall, finish_statement_live, insert_live, start_statement_rollback_live},
    },
    row_record::ValueRef,
};
use crate::transaction::concurrency::catalog_test_support as hook;

/// The literal's RootJob ordinal (row_forward's `ROOT_JOB.0`).
const ROOT_JOB: usize = 3;

/// The C error a failed driver call kept, which must be C's.
fn disk(failure: &Failure) -> &RetentionError {
    match &failure.error {
        DriverError::Disk(error) => error,
        DriverError::Insert(error) => panic!("A's {error:?}, not C's"),
    }
}

// The helpers below take a call's result or `After` by value, so nothing of
// it, and of the view it holds, outlives the match.

/// The rows a call inserted, which must have succeeded.
fn rows_inserted<H>(result: std::result::Result<usize, DriveError<H>>) -> usize {
    match result {
        Ok(rows) => rows,
        Err(refused) => panic!("insert_live refused {:?}", refused.failure),
    }
}

/// A statement finish, which must have succeeded.
fn finished<H>(result: std::result::Result<(), DriveError<H>>) {
    if let Err(refused) = result {
        panic!("finish_statement_live refused {:?}", refused.failure);
    }
}

/// A failed call's error, which must be one.
fn refused<H>(result: std::result::Result<usize, DriveError<H>>) -> (Failure, After<H>) {
    match result {
        Err(DriveError { failure, after }) => (failure, after),
        Ok(rows) => panic!("{rows} rows inserted"),
    }
}

/// The handle a failed call returned as it was.
fn returned<H>(after: After<H>) -> H {
    match after {
        After::Returned(view) => view,
        _ => panic!("the statement is not Returned"),
    }
}

/// The handle a failed call left to be resolved.
fn must_resolve<H>(after: After<H>) -> H {
    match after {
        After::MustResolve(Some(view)) => view,
        _ => panic!("the statement is not MustResolve"),
    }
}

/// The pending statement a failed call left with rows applied.
fn rollback_required<H>(after: After<H>) -> PendingRollback<H> {
    match after {
        After::RollbackRequired(pending) => pending,
        _ => panic!("the statement is not RollbackRequired"),
    }
}

/// A pending statement whose rollback never finished has one end in this
/// branch: it drops, and the view it holds fences the session.
fn abandon<H>(pending: PendingRollback<H>) {
    if pending.rolled_back().is_ok() {
        panic!("no rollback finished");
    }
}

/// The one statement of T's row through A's driver: `insert_live` over a
/// `LiveCall` of the writer, the Owner, the split MaintenancePort and the
/// dedicated flusher Owner.
macro_rules! drive {
    ($writer:ident, $owner:ident, $flush:ident, $flusher:ident, $view:ident, $table:ident,
     $private:ident, $rows:expr, $slots:expr) => {{
        let mut call = LiveCall::new(&mut $writer, $owner, &mut $flush, $flusher);
        insert_live(&mut call, $view, &$table, $private, $rows, $slots)
    }};
}

/// `close_row!` with the statement finished through A's
/// `finish_statement_live` over a `LiveCall`: the statement slot is closed,
/// then the empty transaction ends.
macro_rules! close_live {
    ($writer:ident, $owner:ident, $flush:ident, $flusher:ident, $row:ident, $view:ident,
     $private:ident) => {
        finished({
            let mut call = LiveCall::new(&mut $writer, $owner, &mut $flush, $flusher);
            finish_statement_live(&mut call, $view)
        });
        assert_eq!($row.test_statement_slot().0, 0);
        assert!(
            $writer
                .abort_empty_transaction($owner, $row, $private, &mut $flush)
                .is_ok()
        );
        $owner.check_idle().unwrap();
    };
}

/// S2W-R09 through A's driver: one row, every source resident. The call
/// returns one row and finishes the statement itself. The WAL gained the
/// RowReserve literal and then the Forward literal (LSN 15, 16) and nothing
/// for the statement end. The slot holds the Forward of the RowId and UndoRef
/// the RootJob records (RootRetained, statement 1); the statement slot is
/// closed, the admission, the gate and the lease are returned and the
/// flusher is Ready, with no fence. The producer
/// ran in the actual producer lane (`ProducerLane::forward`).
#[test]
fn s2w_r09_live_driver_inserts_one_row_and_finishes_the_statement() {
    case("S2W-R09", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let used = reader.catalog_test_source_used();
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let count = rows_inserted(drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                ));
                let written_wal = wal_since(state, wal);
                let observation = slots[0].take().unwrap();
                let jobs = writer.catalog_test_jobs();
                let job = job3::decode_job(&jobs[ROOT_JOB], namespace()).unwrap();
                println!(
                    "S2W_R09_LIVE rows={count} wal_bytes={} observation={observation:?} \
                     root_job=({:?},{}) statement_slot={:?}",
                    written_wal.len(),
                    job.state(),
                    job.statement(),
                    row.test_statement_slot().0,
                );
                assert_eq!(count, 1);
                assert_eq!(written_wal.len(), RESERVE_FRAME.0 + FORWARD_FRAME.0);
                let (reserve, forward) = written_wal.split_at(RESERVE_FRAME.0);
                assert_eq!(hex(reserve), RESERVE_FRAME.1);
                assert_eq!(hex(forward), FORWARD_FRAME.1);
                assert_eq!(
                    writer.free_undo_test_state().unwrap().0.complete_lsn,
                    PREVIOUS_LSN + 2
                );
                assert_eq!(observation.operation(), RowOperation::Forward);
                assert_eq!(job.row_id(), Some(observation.row()));
                assert_eq!(job.undo(), Some(observation.undo()));
                assert_eq!((job.state(), job.statement()), (JobState::RootRetained, 1));
                assert_eq!(row.test_statement_slot().0, 0);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                drop(row);
            }
        );
    });
}

/// MCA05-V31 (the S2-0 reading, consumers' contract) through A's driver, on
/// S2W-R16's fixture: the Free Undo 14 is not resident and every clean frame
/// that is not a source is pinned by another owner, so the first preflight
/// answers `FlushRequired` before any effect. The driver flushes the target
/// with the dedicated flusher Owner, continues the flush until the flusher is
/// Ready and tries the same row again from preflight in the same statement.
/// The row then lands the same RowReserve and Forward literals (LSN 15, 16)
/// as a row that needed no flush, so nothing was issued or consumed by the
/// first attempt: same RowId, UndoRef and RootJob. The driver called the
/// preflight twice, and exactly one of the dirty pages is no longer dirty
/// (the one flush target, written; its frame may hold the Undo 14 now); the
/// Undo 14 is resident.
#[test]
fn mca05_v31_live_dirty_victim_flush_retries_the_same_row() {
    case("MCA05-V31", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                evict(live, flusher, &row_undo.unwrap());
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                view.test_clear_reads();
                let pool = reader.catalog_test_pool();
                let listed = frames(pool, 12);
                let clean: Vec<usize> = listed
                    .iter()
                    .filter(|(_, page, dirty)| !dirty && !SOURCES.contains(page))
                    .map(|(index, ..)| *index)
                    .collect();
                let dirty: Vec<u64> = listed
                    .iter()
                    .filter(|(_, page, dirty)| *dirty && !SOURCES.contains(page))
                    .map(|(_, page, _)| *page)
                    .collect();
                assert!(!dirty.is_empty());
                assert!(resident_index(&writer, FREE_UNDO).is_none());
                let used = reader.catalog_test_source_used();
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let preflights = writer.test_row_preflights();
                let count = with_pins(pool, &mut holder, &clean, || {
                    rows_inserted(drive!(
                        writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                    ))
                });
                let preflights = writer.test_row_preflights() - preflights;
                holder.check_idle().unwrap();
                let after = frames(pool, 12);
                let written: Vec<u64> = dirty
                    .iter()
                    .copied()
                    .filter(|page| !after.iter().any(|(_, p, d)| p == page && *d))
                    .collect();
                let written_wal = wal_since(state, wal);
                let observation = slots[0].take().unwrap();
                let jobs = writer.catalog_test_jobs();
                let job = job3::decode_job(&jobs[ROOT_JOB], namespace()).unwrap();
                println!(
                    "MCA05_V31_LIVE rows={count} preflights={preflights} dirty={dirty:?} \
                     written={written:?} wal_bytes={} observation={observation:?}",
                    written_wal.len()
                );
                assert_eq!(count, 1);
                assert_eq!(preflights, 2, "FlushRequired once, then Ready");
                assert_eq!(written.len(), 1, "exactly one dirty victim was flushed");
                assert!(resident_index(&writer, FREE_UNDO).is_some());
                assert_eq!(written_wal.len(), RESERVE_FRAME.0 + FORWARD_FRAME.0);
                let (reserve, forward) = written_wal.split_at(RESERVE_FRAME.0);
                assert_eq!(hex(reserve), RESERVE_FRAME.1);
                assert_eq!(hex(forward), FORWARD_FRAME.1);
                assert_eq!(observation.operation(), RowOperation::Forward);
                assert_eq!(job.row_id(), Some(observation.row()));
                assert_eq!(job.undo(), Some(observation.undo()));
                assert_eq!(job.state(), JobState::RootRetained);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(!fence.is_fenced());
                drop(row);
            }
        );
    });
}

/// S2W-R07 through A's driver: the reserve commitment recompute sees one
/// stale stamp, so reserve is Before(Stale) on the same PreflightRow. The
/// driver does not retry (gap G3): it finishes the preflight and the
/// statement is Returned with no row applied. No WAL or file byte changed,
/// no write bit was registered, the admission, the gate and the lease are
/// returned. The statement then ends through A's `finish_statement_live`,
/// and the empty transaction after it.
#[test]
fn s2w_r07_live_reserve_stale_ends_the_row_with_finish() {
    case("S2W-R07", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let used = reader.catalog_test_source_used();
                let effects = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let once = writer.test_row_stale_stamp_once().at_reserve();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(once);
                let (failure, after) = refused(inserted);
                println!("S2W_R07_LIVE failure={failure:?}");
                assert_eq!(failure.stage, Stage::Reserve);
                assert!(stale(disk(&failure)));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let view = returned(after);
                assert!(slots[0].is_none());
                assert_eq!(view.test_table_masks().1, 0);
                assert_eq!(files(state), effects);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R11 (reserve half) through A's driver: the RowReserve install is
/// refused Unchanged (Stage, Busy) after the table write bit. The driver
/// finishes the preflight without a retry and the statement is Returned. No
/// WAL or file byte changed, the conservative write bit stays, and the
/// statement and the empty transaction then end.
#[test]
fn s2w_r11_live_reserve_stage_refusal_ends_the_row_with_finish() {
    case("S2W-R11", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let effects = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let once = writer.test_row_stage_refusal_once().at_reserve();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(once);
                let (failure, after) = refused(inserted);
                println!("S2W_R11_LIVE reserve failure={failure:?}");
                assert_eq!(failure.stage, Stage::Reserve);
                let error = disk(&failure);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Stage, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Busy));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let view = returned(after);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert_eq!(files(state), effects);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R10 through A's driver: A's proposal is prepared against a draft
/// whose recompute sees one stale stamp, so prepare is Before(Stale) on the
/// same ReservedRow. The driver cancels that reservation: the WAL gained the
/// RowReserve literal and then the Cancel literal (LSN 15, 16), nothing the
/// Reserve issued is returned (the Cancel literal's Root), the write bit
/// stays, and the statement is Returned with no row applied. The statement
/// and the transaction, empty after the Cancel, then end.
#[test]
fn s2w_r10_live_prepare_before_is_cancelled_by_the_driver() {
    case("S2W-R10", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let once = writer.test_row_stale_stamp_once().at_prepare();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(once);
                let (failure, after) = refused(inserted);
                println!("S2W_R10_LIVE failure={failure:?}");
                assert_eq!(failure.stage, Stage::Prepare);
                assert!(stale(disk(&failure)));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let view = returned(after);
                assert!(slots[0].is_none());
                cancelled_literals(state, wal);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(!fence.is_fenced());
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R11 (apply half) through A's driver: apply's install is refused
/// Unchanged (Stage, Busy) after the table write bit, a Before with the same
/// PreparedRow. The driver reopens the ReservedRow and cancels it: the WAL
/// gained the RowReserve and the Cancel literals and no Forward, the write
/// bit stays, and the statement is Returned and then ends with the
/// transaction.
#[test]
fn s2w_r11_live_apply_before_is_reopened_and_cancelled() {
    case("S2W-R11", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let once = writer.test_row_stage_refusal_once().at_apply();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(once);
                let (failure, after) = refused(inserted);
                println!("S2W_R11_LIVE apply failure={failure:?}");
                assert_eq!(failure.stage, Stage::Apply);
                let error = disk(&failure);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Stage, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Busy));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let view = returned(after);
                assert!(slots[0].is_none());
                cancelled_literals(state, wal);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// The WAL from `wal` on is the RowReserve literal and then the Cancel
/// literal (S2-d), LSN 15 and 16.
fn cancelled_literals(state: &State, wal: usize) {
    let written_wal = wal_since(state, wal);
    assert_eq!(written_wal.len(), RESERVE_FRAME.0 + CANCEL_FRAME.0);
    let (reserve, cancel) = written_wal.split_at(RESERVE_FRAME.0);
    assert_eq!(hex(reserve), RESERVE_FRAME.1);
    assert_eq!(hex(cancel), CANCEL_FRAME.1);
}

/// S2W-R21(2) through A's driver: after a Ready preflight a write target of
/// T is reported not resident at apply's draft recompute, which is Fatal
/// (Prepare, Invariant) with the fence (consumers' MCA05-V31 reading: "비상주가
/// 관측되면 Fatal(Invariant)"). The driver makes no further C call and the
/// statement is MustResolve; the WAL has the RowReserve and no Forward, the
/// RowWrite charge stays visible under the fence and the write bit stays.
#[test]
fn s2w_r21_live_missing_target_is_must_resolve() {
    case("S2W-R21", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let once = writer.test_row_missing_target_once().at_apply();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(once);
                let (failure, after) = refused(inserted);
                println!("S2W_R21_LIVE missing_target failure={failure:?}");
                assert_eq!(failure.stage, Stage::Apply);
                let error = disk(&failure);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::CallerMustResolve)
                );
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert!(failure.must_resolve && failure.cleanup.is_none());
                let view = must_resolve(after);
                assert!(slots[0].is_none());
                let written_wal = wal_since(state, wal);
                assert_eq!(
                    (written_wal.len(), hex(&written_wal).as_str()),
                    (RESERVE_FRAME.0, RESERVE_FRAME.1)
                );
                assert!(fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert_eq!(view.test_table_masks().1, 1 << slot);
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R21(1) at apply through A's driver: apply's draft recompute sees one
/// stale stamp, a Before Unchanged (Prepare, Frame Stale) with the same
/// PreparedRow (the cut commitment of apply). The driver reopens the
/// ReservedRow and cancels it: the WAL gained the RowReserve and the Cancel
/// literals and no Forward, the write bit stays, and the statement is
/// Returned. It then ends through A's `finish_statement_live`, and the empty
/// transaction after it.
#[test]
fn s2w_r21_live_apply_stale_is_reopened_and_cancelled() {
    case("S2W-R21", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let once = writer.test_row_stale_stamp_once().at_apply();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(once);
                let (failure, after) = refused(inserted);
                println!("S2W_R21_LIVE apply_stale failure={failure:?}");
                assert_eq!(failure.stage, Stage::Apply);
                assert!(stale(disk(&failure)));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let view = returned(after);
                assert!(slots[0].is_none());
                cancelled_literals(state, wal);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(!fence.is_fenced());
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R11 (apply half) and then S2W-R21(1) at the Cancel, through A's
/// driver: apply's install is refused Unchanged (Stage, Busy), so the driver
/// reopens the ReservedRow and cancels it, and the Cancel's recompute sees
/// one stale stamp: Before Unchanged (Prepare, Frame Stale) with the same
/// ReservedRow. The driver does not retry the Cancel, and Drop does not
/// stand in for it: the reservation is dropped into C's fence and the
/// release is reported refused. The failure is apply's Busy with the
/// Cancel's Stale as its cleanup, it must be resolved, and so must the
/// statement. The WAL has the RowReserve literal and nothing after it, the
/// RowWrite charge stays visible under the fence and the write bit stays.
#[test]
fn s2w_r21_live_refused_cancel_is_fenced_and_must_resolve() {
    case("S2W-R21", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let (wal, _) = files(state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let refusal = writer.test_row_stage_refusal_once().at_apply();
                let stale_once = writer.test_row_stale_stamp_once().at_cancel();
                let inserted = drive!(
                    writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                );
                drop(stale_once);
                drop(refusal);
                let (failure, after) = refused(inserted);
                println!("S2W_R21_LIVE refused_cancel failure={failure:?}");
                assert_eq!(failure.stage, Stage::Apply);
                let error = disk(&failure);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Stage, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Busy));
                match &failure.cleanup {
                    Some((Stage::Cancel, DriverError::Disk(error))) => assert!(stale(error)),
                    cleanup => panic!("not the Cancel's Stale: {cleanup:?}"),
                }
                assert!(failure.must_resolve);
                let view = must_resolve(after);
                assert!(slots[0].is_none());
                let written_wal = wal_since(state, wal);
                assert_eq!(
                    (written_wal.len(), hex(&written_wal).as_str()),
                    (RESERVE_FRAME.0, RESERVE_FRAME.1)
                );
                assert!(fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert_eq!(view.test_table_masks().1, 1 << slot);
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R19 (Busy) through A's driver: the Free Undo 14 is not resident and
/// every frame that is not a source is pinned by another owner, so the
/// preflight's LOAD claim is refused Unchanged (Load, Busy). The driver ends
/// the row with that error and no retry: it called the preflight once (and
/// a retry under the held pins could not return). The statement is
/// Returned, nothing was registered or written, and the admission, the gate
/// and the lease are returned. After
/// the pins are released the residency settles and the statement and the
/// empty transaction end.
#[test]
fn s2w_r19_live_preflight_busy_ends_the_row_without_retry() {
    case("S2W-R19", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                evict(live, flusher, &row_undo.unwrap());
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                view.test_clear_reads();
                let pool = reader.catalog_test_pool();
                let others: Vec<usize> = frames(pool, 12)
                    .iter()
                    .filter(|(_, page, _)| !SOURCES.contains(page))
                    .map(|(index, ..)| *index)
                    .collect();
                let used = reader.catalog_test_source_used();
                let before = effects!(writer, state);
                let values = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let preflights = writer.test_row_preflights();
                let (failure, after) = with_pins(pool, &mut holder, &others, || {
                    refused(drive!(
                        writer, owner, flush, flusher, view, table, private, &rows, &mut slots
                    ))
                });
                let preflights = writer.test_row_preflights() - preflights;
                holder.check_idle().unwrap();
                println!("S2W_R19_LIVE busy preflights={preflights} failure={failure:?}");
                assert_eq!(preflights, 1, "no retry");
                assert_eq!(failure.stage, Stage::Preflight);
                let error = disk(&failure);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Load, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Frame(ref frame)
                    if frame.cause == crate::storage::dirty_frame::Cause::Busy));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let view = returned(after);
                returned_unchanged!(writer, reader, view, state, core, fence, used, before);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// Not a contract row (S2-e): a statement of two rows whose first row lands
/// the R09 literals (both halves) with the R09 identity in its slot. The
/// second row's preflight finds no Free Undo candidate and is Unchanged
/// Capacity, before validation: S2W-R05's refusal, one Free Undo container
/// per RowReserve, and the fixture's one Free Undo, 14, is leased by the
/// first row. With a row applied, the statement is RollbackRequired and the
/// applied row's slot is kept. The statement
/// rollback is the next step. S4 extends this original S2-E09 boundary
/// through the actual A CLR driver, StatementFinish and a healthy Commit.

#[test]
fn s2e_e09_applied_then_rejected_row_is_rollback_required_up_to_the_s4_start() {
    case("S2E-E09", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let used = reader.catalog_test_source_used();
                let (wal, _) = files(state);
                let first = [ValueRef::Int64(VALUE)];
                let second = [ValueRef::Int64(VALUE)];
                let rows: [&[ValueRef<'_>]; 2] = [&first, &second];
                let mut slots: [Option<RowMutationObservation>; 2] = [None, None];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let inserted = insert_live(&mut call, view, &table, private, &rows, &mut slots);
                let (failure, after) = refused(inserted);
                println!("S2E_E09 failure={failure:?}");
                assert_eq!(failure.stage, Stage::Preflight);
                let error = disk(&failure);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Capacity));
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                let mut pending = rollback_required(after);
                assert!(slots[1].is_none());
                let observation = slots[0].as_ref().unwrap();
                assert_eq!(observation.operation(), RowOperation::Forward);
                let written_wal = wal_since(state, wal);
                assert_eq!(written_wal.len(), RESERVE_FRAME.0 + FORWARD_FRAME.0);
                let (reserve, forward) = written_wal.split_at(RESERVE_FRAME.0);
                assert_eq!(hex(reserve), RESERVE_FRAME.1);
                assert_eq!(hex(forward), FORWARD_FRAME.1);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                {
                    use crate::storage::row::insert::driver::{
                        Stepped,
                        live::{finish_rollback_live, rollback_step_live},
                    };
                    let mut rollback =
                        match start_statement_rollback_live(&mut call, &row, &mut pending, private)
                        {
                            Ok(r) => r,
                            Err(e) => panic!("start {e:?}"),
                        };
                    assert!(matches!(
                        rollback_step_live(&mut rollback),
                        Ok(Stepped::Compensated)
                    ));
                    assert!(matches!(
                        rollback_step_live(&mut rollback),
                        Ok(Stepped::Complete)
                    ));
                    match finish_rollback_live(rollback) {
                        Ok(()) => {}
                        Err(_) => panic!("finish rollback"),
                    }
                }
                let view = match pending.rolled_back() {
                    Ok(view) => view,
                    Err(_) => panic!("pending completion"),
                };
                assert!(finish_statement_live(&mut call, view).is_ok());
                let jobs = writer.catalog_test_jobs();
                let job = job3::decode_job(&jobs[ROOT_JOB], namespace()).unwrap();
                assert_eq!(job.row_id(), Some(observation.row()));
                assert_eq!(job.undo(), Some(observation.undo()));
                assert_eq!(job.state(), JobState::RootReleasingCompensated);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                assert!(writer.commit_row(owner, row, private).is_ok());
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S5 factory retains exactly one Full charge until checked finish. Reopening
/// the same original Owner proves both reference rows and read progress return.
#[test]
fn s5r_r01_read_admission_returns_original_resources() {
    case("S5R-R01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let used = reader.catalog_test_source_used();
                println!(
                    "S5_SOURCE_OBSERVATIONS full/load/operation/page/candidate={:?}",
                    reader.row_test_source_observations()
                );
                let before = files(state);
                for _ in 0..2 {
                    let operation = reader
                        .row_operation(owner, &view, reader_private)
                        .unwrap_or_else(|e| panic!("read entry: {e:?}"));
                    assert!(reader.catalog_test_source_used() > used);
                    operation.finish().unwrap();
                    assert_eq!(reader.catalog_test_source_used(), used);
                    owner.check_idle().unwrap();
                }
                assert_eq!(files(state), before);
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
                let _ = (core, table);
            }
        );
    });
}

#[test]
fn s5r_r02_empty_table_and_r04_actual_capture_copy() {
    use crate::storage::disk::{Plan4ResidencyCursor, ReadStep};
    use crate::storage::row::{
        insert::{produce_insert, validate_insert},
        read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R02-R04", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut residency = Plan4ResidencyCursor::new();
                let cancel = Cancellation::default();
                let mut output = [0xA5; 64];
                let before = files(state);
                let read = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &view,
                    &table,
                    RowId::new(1).unwrap(),
                    &mut output,
                    &cancel,
                )
                .unwrap();
                assert!(matches!(read, ReadStep::Ready(PointRead::Missing)));
                assert_eq!(output, [0xA5; 64]);
                assert_eq!(files(state), before);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(p) => p,
                    _ => panic!("resident"),
                };
                let input =
                    validate_insert(&[ValueRef::Int64(7)], preflight.validation_lane().unwrap())
                        .unwrap();
                let mut reserved = preflight
                    .reserve(input)
                    .unwrap_or_else(|_| panic!("reserve"));
                let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                let applied = reserved
                    .prepare(proposal)
                    .unwrap_or_else(|_| panic!("prepare"))
                    .apply()
                    .unwrap_or_else(|_| panic!("apply"));
                let (observation, _) = applied.into_parts();
                let read = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &view,
                    &table,
                    observation.row(),
                    &mut output,
                    &cancel,
                )
                .unwrap();
                let ReadStep::Ready(PointRead::Found(copied)) = read else {
                    panic!("own row")
                };
                assert_eq!(copied.version().row, observation.row());
                let expected = crate::storage::row_record::decode_canonical_row(
                    &output[..copied.len()],
                    &[crate::api::Column {
                        name: "i".into(),
                        data_type: crate::api::DataType::Int64,
                        nullable: false,
                    }],
                )
                .unwrap();
                assert_eq!(expected.value(0).unwrap(), ValueRef::Int64(7));
                // Capture is source-backed; short output, stale candidate and
                // sequence exhaustion do not publish bytes or leak references.
                let stale = {
                    let mut op = reader.row_operation(owner, &view, reader_private).unwrap();
                    let page = match op.page(&table, &mut residency).unwrap() {
                        ReadStep::Ready(p) => p,
                        _ => panic!("resident"),
                    };
                    let candidate = page.capture(0).unwrap();
                    assert!(matches!(
                        op.status(&candidate, &mut residency).unwrap(),
                        ReadStep::Ready(
                            crate::storage::transaction_record::TransactionState::Active
                        )
                    ));
                    assert!(matches!(
                        op.missing_before(&candidate, &mut residency).unwrap(),
                        ReadStep::Ready(_)
                    ));
                    let mut short = [0xBC; 1];
                    assert!(op.copy_candidate(&candidate, &mut short).is_err());
                    assert_eq!(short, [0xBC]);
                    let stale = candidate.test_duplicate();
                    op.release_candidate(candidate).unwrap();
                    op.finish().unwrap();
                    stale
                };
                {
                    let mut op = reader.row_operation(owner, &view, reader_private).unwrap();
                    assert!(op.copy_candidate(&stale, &mut output).is_err());
                    op.test_set_sequence(u64::MAX);
                    let page = match op.page(&table, &mut residency).unwrap() {
                        ReadStep::Ready(p) => p,
                        _ => panic!("resident"),
                    };
                    assert!(page.capture(0).is_err());
                    op.finish().unwrap();
                }
                {
                    // Another original Owner has its own lease slot. A
                    // source-issued candidate cannot cross that identity even
                    // when both owners use the same statement and table.
                    let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                    let mut operation = reader
                        .row_operation(&mut other, &view, reader_private)
                        .unwrap();
                    let before = output;
                    assert!(operation.copy_candidate(&stale, &mut output).is_err());
                    assert_eq!(output, before);
                    operation.finish().unwrap();
                    other.check_idle().unwrap();
                }
                {
                    use crate::storage::row::read::driver::{
                        CursorStep,
                        live::{next_live, open_cursor_live},
                    };
                    let mut cursor =
                        open_cursor_live(&view, &table, Some(observation.row()), None).unwrap();
                    let mut short = [0xB1; 1];
                    assert!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency
                            },
                            &mut short,
                            &cancel
                        )
                        .is_err()
                    );
                    assert_eq!(short, [0xB1]);
                    let cancelled = Cancellation::default();
                    cancelled.cancel();
                    assert!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency
                            },
                            &mut output,
                            &cancelled
                        )
                        .is_err()
                    );
                    let found = next_live(
                        &mut cursor,
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency,
                        },
                        &mut output,
                        &cancel,
                    )
                    .unwrap();
                    assert!(
                        matches!(found,ReadStep::Ready(CursorStep::Row(ref copy)) if copy.version().row==observation.row())
                    );
                    assert!(matches!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency
                            },
                            &mut output,
                            &cancel
                        )
                        .unwrap(),
                        ReadStep::Ready(CursorStep::End)
                    ));
                    cursor.close().unwrap();
                }
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.commit_row(owner, row, private).is_ok());
                assert!(!fence.is_fenced());
                let _ = core;
            }
        );
    });
}

#[test]
fn s5r_r06_old_and_new_statement_snapshot() {
    use crate::storage::{
        disk::{Plan4ResidencyCursor, ReadStep, TransactionOptions},
        row::{
            insert::{produce_insert, validate_insert},
            read::driver::{
                PointRead,
                live::{LiveCall as ReadCall, read_point_live},
            },
        },
    };
    use foundation::runtime::Cancellation;
    case("S5R-R06", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(p) => p,
                    _ => panic!("resident"),
                };
                let input =
                    validate_insert(&[ValueRef::Int64(7)], preflight.validation_lane().unwrap())
                        .unwrap();
                let mut reserved = preflight
                    .reserve(input)
                    .unwrap_or_else(|_| panic!("reserve"));
                let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                let applied = reserved
                    .prepare(proposal)
                    .unwrap_or_else(|_| panic!("prepare"))
                    .apply()
                    .unwrap_or_else(|_| panic!("apply"));
                let (observation, _) = applied.into_parts();
                let mut read_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let transaction = writer
                    .begin_transaction(
                        &mut read_owner,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer
                    .begin_statement(&mut read_owner, &transaction)
                    .unwrap();
                let q = old.q().unwrap();
                let cancel = Cancellation::default();
                let mut output = [0xAA; 64];
                let mut residency = Plan4ResidencyCursor::new();
                // Another actor's still Active INSERT is invisible, proven Missing.
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner: &mut read_owner,
                            private: reader_private,
                            residency: &mut residency
                        },
                        &old,
                        &table,
                        observation.row(),
                        &mut output,
                        &cancel
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert!(writer.finish_statement(owner, view).is_ok());
                let committed = writer
                    .commit_row(owner, row, private)
                    .unwrap_or_else(|_| panic!("commit"));
                assert!(committed.q() > q);
                // The same ReadCommitted statement retains its original q.
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner: &mut read_owner,
                            private: reader_private,
                            residency: &mut residency
                        },
                        &old,
                        &table,
                        observation.row(),
                        &mut output,
                        &cancel
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert_eq!(old.q().unwrap(), q);
                assert_eq!(output, [0xAA; 64]);
                assert!(writer.finish_statement(&mut read_owner, old).is_ok());
                let new = writer
                    .begin_statement(&mut read_owner, &transaction)
                    .unwrap();
                assert_eq!(new.q().unwrap(), committed.q());
                assert!(
                    matches!(read_point_live(ReadCall{port:&reader,owner:&mut read_owner,private:reader_private,residency:&mut residency},&new,&table,observation.row(),&mut output,&cancel).unwrap(),ReadStep::Ready(PointRead::Found(ref copy)) if copy.version().row==observation.row())
                );
                assert!(writer.finish_statement(&mut read_owner, new).is_ok());
                assert!(
                    writer
                        .finish_read_only(&mut read_owner, transaction)
                        .is_ok()
                );
                assert!(!fence.is_fenced());
                let _ = state;
            }
        );
    });
}

/// C/boundary join. Original ports are already in shared_bytes; operation,
/// its caller frames and each C callee are added once. This is a source-value
/// bound, not compiler stack/RSS measurement.
fn row_read_census<W: WriterNamespace>(kind: &str) {
    use crate::storage::{disk::ReadPort, row::read::driver::live};
    use std::mem::size_of as n;
    let c = ReadPort::<W>::row_test_load_inventory();
    let leaf = ReadPort::<W>::row_test_nonload_inventory();
    let b = live::test_load_callers::<W>();
    let nonload_b = live::test_nonload_caller::<W>();
    // Same-error `?`/match moves stay in their counted Result payloads.
    // Explicit conversion leaf slack: two whole error carriers plus eight
    // reference/scalar temporaries; ViewSample, merge, decode, extraction and
    // publication bodies are separate never-inline callees.
    // Eight words: cancel/error branch scalars(1), Range::next Option(2),
    // protected slice(2), backend and loader reborrows(2), conversion borrow(1).
    // Error values already in named Result payloads are not additional slots;
    // the two conversion temporaries cover the Source→RowReadError boundary.
    assert!(n::<crate::storage::row::read::driver::RowReadError>() <= n::<RetentionError>());
    let load_temporaries = 2 * n::<RetentionError>() + 8 * n::<&()>();
    let operation = c[6];
    let load = c[7];
    let full = c[8];
    let mut phases = Vec::new();
    for (name, offset) in [("point", 0), ("cursor", 3)] {
        phases.push((
            format!("{name}.catalog_load"),
            operation + load + c[0] + c[1] + b[offset] + load_temporaries,
        ));
        phases.push((
            format!("{name}.row_load"),
            operation + load + c[0] + c[2] + b[offset] + load_temporaries,
        ));
        phases.push((
            format!("{name}.status_load"),
            operation + load + c[9] + b[offset + 1] + load_temporaries,
        ));
        phases.push((
            format!("{name}.undo_load"),
            operation + load + c[10] + b[offset + 2] + load_temporaries,
        ));
        phases.push((
            format!("{name}.status_location"),
            operation + leaf[3] + c[9] + b[offset + 1] + load_temporaries,
        ));
        phases.push((
            format!("{name}.undo_location"),
            operation + leaf[3] + c[10] + b[offset + 2] + load_temporaries,
        ));
    }
    let parent = c[0].max(c[9]).max(c[10]);
    for (at, name) in [
        (0, "admission"),
        (1, "prepare_pages"),
        (2, "capture"),
        (4, "decode"),
        (5, "copy"),
        (6, "cleanup"),
    ] {
        phases.push((name.into(), operation + parent + leaf[at] + nonload_b));
    }
    println!(
        "S5_ROW_CENSUS kind={kind} full={full} operation={operation} load={load} B={b:?} nonload_B={nonload_b} C={c:?} leaf={leaf:?} slack={load_temporaries}"
    );
    for (name, bytes) in phases {
        println!("S5_ROW_PHASE {kind} {name} {bytes} {full}");
        assert!(bytes <= full, "{kind} {name}: {bytes}>{full}");
    }
}

#[test]
fn s5r_c01_memory_source_inventory() {
    row_read_census::<port::Writer>("memory");
}

#[cfg(target_os = "linux")]
#[test]
fn s5r_c02_native_source_inventory() {
    use super::super::super::super::plan4_pages::native;
    row_read_census::<native::Writer>("instrumented-native");
}

#[test]
fn s5r_r08_corrupt_row_tree_fences_and_preserves_cause() {
    use crate::storage::{
        dirty_frame::status_load_test_support,
        page::{PageKind, decode_page, encode_page},
        row::read::driver::{
            RowReadError,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R08", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let index = frames(pool, 12)
                    .into_iter()
                    .find(|(_, page, _)| *page == 13)
                    .unwrap()
                    .0;
                let identity = pool.target_request(index).unwrap().identity;
                // Damage grammar while keeping the original page identity, LSN,
                // generic envelope and checksum valid. No replacement authority.
                status_load_test_support::edit_content(pool, index, |bytes| {
                    let page = decode_page(bytes, identity, PageKind::RowLeaf).unwrap();
                    assert_eq!(page.len(), 1);
                    let header = page.header();
                    let mut metadata = page.cell(0).unwrap().to_vec();
                    metadata[0] ^= 1;
                    encode_page(header, &[metadata.as_slice()], bytes).unwrap();
                    decode_page(bytes, identity, PageKind::RowLeaf).unwrap();
                });
                let full = reader.row_test_source_observations()[0] as u64;
                let before = files(state);
                let mut output = [0xA9; 32];
                let error = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut Plan4ResidencyCursor::new(),
                    },
                    &view,
                    &table,
                    RowId::new(1).unwrap(),
                    &mut output,
                    &Cancellation::default(),
                )
                .err()
                .unwrap();
                assert!(
                    matches!(
                        error,
                        RowReadError::Source(RetentionError {
                            phase: RetentionPhase::Source,
                            effect: Effect::CallerMustResolve,
                            cause: RetentionCause::Tree(_)
                        })
                    ),
                    "{error:?}"
                );
                assert!(fence.is_fenced());
                assert_eq!(output, [0xA9; 32]);
                assert_eq!(files(state), before);
                // Fatal source bookkeeping stays quarantined, as for Catalog
                // reads: release must not manufacture reusable fenced capacity.
                assert_eq!(reader.catalog_test_source_used(), full);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                drop(view);
                drop(row);
            }
        );
    });
}

#[test]
fn s5r_r03_dirty_victim_flush_retries_the_same_snapshot() {
    use crate::storage::{
        disk::ReadStep,
        row::read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let index = frames(live.catalog_test_pool(), 12)
                    .into_iter()
                    .find(|(_, page, _)| *page == 13)
                    .unwrap()
                    .0;
                let leaf = live.row_test_resident_target(index).unwrap();
                evict(live, flusher, &leaf);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                assert!(resident_index(&writer, 13).is_none());
                let q = view.q().unwrap();
                let pool = reader.catalog_test_pool();
                let clean: Vec<usize> = frames(pool, 12)
                    .into_iter()
                    .filter(|(_, _, dirty)| !*dirty)
                    .map(|(index, ..)| index)
                    .collect();
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let before = files(state);
                let mut output = [0xA3; 32];
                let mut residency = Plan4ResidencyCursor::new();
                let cancel = Cancellation::default();
                let target = with_pins(pool, &mut holder, &clean, || {
                    match read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency,
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &cancel,
                    )
                    .unwrap()
                    {
                        ReadStep::FlushRequired { target } => target,
                        ReadStep::Ready(_) => {
                            panic!("nonresident leaf with only dirty eligible victims")
                        }
                    }
                });
                holder.check_idle().unwrap();
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(files(state), before);
                assert_eq!(output, [0xA3; 32]);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                owner.check_idle().unwrap();
                flush_target!(flush, flusher, target);
                let before = files(state);
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &cancel
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(files(state), before);
                assert_eq!(output, [0xA3; 32]);
                assert!(resident_index(&writer, 13).is_some());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
                // The read LOAD may legitimately evict the writer's Status.
                // Restore the writer preflight residency before its legacy
                // empty-abort fixture cleanup; the read assertions are above.
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[cfg(target_os = "linux")]
#[path = "row_live/native.rs"]
mod native_runtime;

#[test]
fn s5r_c03_original_full_admission_boundary_and_foreign_owner() {
    case("S5R-C03", || {
        let foreign_fence = crate::storage::session_fence::SessionFence::new();
        let foreign_core = PageConcurrency::new(&foreign_fence).unwrap();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let [shared, full, _] = reader.catalog_test_fixed_charges();
                let available = 65536 - shared - full;
                let before = files(state);
                let bytes = hex(reader_private);
                for held in [available + 1, available, available - 1] {
                    reader.catalog_test_with_held_source(held, || {
                        let refs = reader.catalog_test_references();
                        let result = reader.row_operation(owner, &view, reader_private);
                        if held > available {
                            let error = result.err().expect("one byte short");
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert!(matches!(error.cause, RetentionCause::Capacity));
                        } else {
                            let operation = result.unwrap();
                            assert_eq!(reader.catalog_test_source_used(), (held + full) as u64);
                            operation.finish().unwrap();
                        }
                        assert_eq!(reader.catalog_test_source_used(), held as u64);
                        assert_eq!(reader.catalog_test_references(), refs);
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        owner.check_idle().unwrap();
                    });
                    assert_eq!(reader.catalog_test_source_used(), 0);
                }
                let mut foreign = foreign_core.try_owner(OwnerRole::Foreground).unwrap();
                let error = reader
                    .row_operation(&mut foreign, &view, reader_private)
                    .err()
                    .unwrap();
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                foreign.check_idle().unwrap();
                assert_eq!(hex(reader_private), bytes);
                assert_eq!(files(state), before);
                assert!(!fence.is_fenced());
                close_live!(writer, owner, flush, flusher, row, view, private);
                let _ = table;
            }
        );
    });
}

#[test]
fn s5r_r01_pending_preflight_precedes_read_source_reservation() {
    use crate::storage::{
        disk::ReadStep,
        row::read::driver::{
            PointRead, RowReadError,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R01-PENDING", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                let q = view.q().unwrap();
                let preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(preflight) => preflight,
                    _ => panic!("settled preflight"),
                };
                let used = reader.catalog_test_source_used();
                let refs = reader.catalog_test_references();
                let before = files(state);
                let loan = hex(reader_private);
                let mut output = [0xD1; 32];
                assert!(hook::observe(core).read_gate_closed);
                let error = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner: &mut late,
                        private: reader_private,
                        residency: &mut Plan4ResidencyCursor::new(),
                    },
                    &view,
                    &table,
                    RowId::new(1).unwrap(),
                    &mut output,
                    &Cancellation::default(),
                )
                .err()
                .unwrap();
                assert!(
                    matches!(error,RowReadError::Source(RetentionError{effect:Effect::Unchanged,cause:RetentionCause::Concurrency(ref e),..}) if e.cause==crate::transaction::concurrency::Cause::InstallationPending),
                    "{error:?}"
                );
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(reader.catalog_test_references(), refs);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(hex(reader_private), loan);
                assert_eq!(files(state), before);
                assert_eq!(output, [0xD1; 32]);
                preflight.finish().unwrap();
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner: &mut late,
                            private: reader_private,
                            residency: &mut Plan4ResidencyCursor::new()
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &Cancellation::default()
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(reader.catalog_test_source_used(), 0);
                late.check_idle().unwrap();
                assert!(!fence.is_fenced());
                close_live!(writer, owner, flush, flusher, row, view, private);
            }
        );
    });
}
