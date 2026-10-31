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
/// rollback is the next step, and its start is C's Unchanged Unsupported (S4
/// is out of scope): the pending statement stays as it was, with no fence.
/// Dropping it hands it to C's fence, which is the only end this branch has
/// for it.
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
                let effects = files(state);
                let start = start_statement_rollback_live(&mut call, &row, &mut pending, private)
                    .err()
                    .unwrap();
                println!("S2E_E09 rollback_start={start:?}");
                assert_eq!(start.stage, Stage::RollbackStart);
                let error = disk(&start);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Unsupported));
                assert!(!start.must_resolve);
                assert_eq!(files(state), effects);
                // The call is done with the writer. The Unchanged start left
                // the RootJob as the first row's apply did: slot 0's RowId and
                // UndoRef, RootRetained in statement 1.
                let jobs = writer.catalog_test_jobs();
                let job = job3::decode_job(&jobs[ROOT_JOB], namespace()).unwrap();
                assert_eq!(job.row_id(), Some(observation.row()));
                assert_eq!(job.undo(), Some(observation.undo()));
                assert_eq!((job.state(), job.statement()), (JobState::RootRetained, 1));
                assert!(!fence.is_fenced());
                abandon(pending);
                assert!(fence.is_fenced());
                drop(row);
            }
        );
    });
}
