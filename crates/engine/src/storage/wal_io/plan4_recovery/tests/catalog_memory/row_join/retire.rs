//! Live Row retirement (DEV3 3.14.1 for the first Row profile, reclamation
//! `#live-row-retirement`): C's Prune, ReleaseRole, RootFree and
//! TerminalFinish of a finished Row actor while the session is open, through
//! `row_cleanup_next` and `retire_row`. Expectations are state-machine facts
//! (the cleanup order, freed Jobs, a reusable Status, Undo and Job, the
//! snapshot horizon), not values read back from the same producer.
use super::super::super::super::super::plan4_pages::{VisibilityCause, apply::RowRetireStep};
use super::super::row_forward::files;
use super::super::row_literal::int_of;
use super::super::row_preflight::{create_table, system_init};
use super::super::row_reopen::{AtReady, reopen};
use super::super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RetentionCause, RetentionError, RollbackScope, RollbackStep,
        RowOperation, RowOwnedError, RowPreflightStep, TableStep, TransactionFinishError,
        TransactionOptions,
    },
    retention_page::row::{self as job3, JobScope, JobState},
    row::{
        insert::{produce_clr, produce_insert, validate_insert},
        read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        },
    },
    row_record::ValueRef,
    semantic_row::Operation as RowCleanup,
};
use foundation::{error::Effect, runtime::Cancellation};

/// One transaction more than the 64 Job cells hold without live cleanup
/// (each one-row transaction leases a Terminal and a Root Job).
const TRANSACTIONS: usize = 33;

/// The live cleanup of one committed one-row actor, in order.
const COMMITTED: [RowCleanup; 6] = [
    RowCleanup::Prune,
    RowCleanup::ReleaseRole,
    RowCleanup::ReleaseRole,
    RowCleanup::ReleaseRole,
    RowCleanup::RootFree,
    RowCleanup::TerminalFinish,
];

macro_rules! applied {
    ($e:expr) => {
        match $e {
            Ok(value) => value,
            Err(RowOwnedError::Before { error, .. }) => panic!("Before {error:?}"),
            Err(RowOwnedError::Fatal(error)) => panic!("Fatal {error:?}"),
        }
    };
}
macro_rules! committed {
    ($call:expr) => {
        match $call {
            Ok(value) => value,
            Err(TransactionFinishError::Before { error, .. })
            | Err(TransactionFinishError::RollbackRequired { error, .. })
            | Err(TransactionFinishError::Rejected(error))
            | Err(TransactionFinishError::Fatal(error)) => panic!("Commit {error:?}"),
        }
    };
}

/// One INSERT of `value` through C's preflight, reserve, prepare and apply,
/// flushing each residency victim; returns the issued RowId.
macro_rules! put_row {
    ($writer:ident,$owner:ident,$view:ident,$table:ident,$private:ident,$flush:ident,$flusher:ident,$value:expr) => {{
        let mut flushed = 0;
        loop {
            match $writer
                .preflight_row($owner, &$view, &$table, $private, &mut $flush)
                .unwrap()
            {
                RowPreflightStep::Ready(mut preflight) => {
                    let values = [ValueRef::Int64($value)];
                    let input =
                        validate_insert(&values, preflight.validation_lane().unwrap()).unwrap();
                    let mut reserved = applied!(preflight.reserve(input));
                    let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                    let prepared = applied!(reserved.prepare(proposal));
                    break applied!(prepared.apply()).into_parts().0.row();
                }
                RowPreflightStep::FlushRequired { target } => {
                    flushed += 1;
                    assert!(flushed < 16, "bounded preflight residency");
                    flush_target!($flush, $flusher, target)
                }
            }
        }
    }};
}

std::thread_local! {
    /// The `FlushRequired` steps `retire_row` returned on this test thread.
    static RETIRE_FLUSHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One `retire_row` of the selected step with its residency victims
/// flushed: the call's final result.
macro_rules! retire_one {
    ($writer:ident, $owner:ident, $private:ident, $flush:ident, $flusher:ident, $ordinal:expr, $operation:expr) => {{
        let mut flushed = 0;
        loop {
            match $writer.retire_row($owner, $ordinal, $operation, $private, &mut $flush) {
                Ok(RowRetireStep::FlushRequired(target)) => {
                    flushed += 1;
                    assert!(flushed < 16, "bounded cleanup residency");
                    RETIRE_FLUSHES.with(|count| count.set(count.get() + 1));
                    flush_target!($flush, $flusher, target);
                }
                Ok(RowRetireStep::Retired(observation)) => {
                    assert_eq!(
                        (observation.operation, observation.job_ordinal),
                        ($operation, Some($ordinal))
                    );
                    break Ok(observation);
                }
                Err(error) => break Err(error),
            }
        }
    }};
}

/// Every live Row cleanup step `row_cleanup_next` selects, each through
/// `retire_row` with its residency victims flushed, until none is left.
/// Returns the operations in order.
macro_rules! retire_all {
    ($writer:ident, $owner:ident, $private:ident, $flush:ident, $flusher:ident) => {{
        let mut done = Vec::new();
        while let Some((ordinal, operation)) = $writer.row_cleanup_next(&[]).unwrap() {
            assert!(done.len() < 64, "bounded live cleanup");
            if let Err(error) = retire_one!(
                $writer, $owner, $private, $flush, $flusher, ordinal, operation
            ) {
                panic!("live {operation:?} of Job {ordinal}: {error:?}");
            }
            $owner.check_idle().unwrap();
            done.push(operation);
        }
        done
    }};
}

/// B's point read of `row` on `view`: its value, or None for Missing.
macro_rules! point_at {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let mut output = [0xA5; 64];
        match read_point_live(
            ReadCall {
                port: &$reader,
                owner: $owner,
                private: $private,
                residency: &mut residency,
            },
            $view,
            $table,
            $row,
            &mut output,
            &Cancellation::default(),
        )
        .unwrap()
        {
            ReadStep::Ready(PointRead::Found(copy)) => {
                assert_eq!(copy.version().row, $row);
                Some(int_of(&output, copy.len()))
            }
            ReadStep::Ready(PointRead::Missing) => None,
            ReadStep::FlushRequired { .. } => panic!("resident read sources"),
        }
    }};
}

/// The Row-scope Jobs in `jobs`: (ordinal, state, release cursor, Undo
/// sequence).
fn row_jobs(jobs: &[[u8; 1024]; 64]) -> Vec<(usize, JobState, u8, u64)> {
    jobs.iter()
        .enumerate()
        .filter_map(|(ordinal, bytes)| {
            let job = job3::decode_job(bytes, super::super::super::support::namespace()).unwrap();
            (job.scope() == Some(JobScope::Row)).then(|| {
                (
                    ordinal,
                    job.state(),
                    job.release_cursor(),
                    job.undo_sequence(),
                )
            })
        })
        .collect()
}

/// No Row Job is held: every Row cell the cleanup touched is Free again.
fn assert_row_jobs_free(jobs: &[[u8; 1024]; 64]) {
    for (ordinal, state, _, _) in row_jobs(jobs) {
        assert_eq!(state, JobState::Free, "Row Job {ordinal} still held");
    }
    for (ordinal, bytes) in jobs.iter().enumerate() {
        let job = job3::decode_job(bytes, super::super::super::support::namespace()).unwrap();
        assert!(
            job.scope() != Some(JobScope::Row) || job.state() == JobState::Free,
            "Row Job {ordinal}"
        );
    }
}

/// C's recovery of the files reached READY: every Row Job is Free (the
/// recovered cleanup finished what the live cleanup left). Returns Q and the
/// count of Row-scope Job cells.
struct Ready;
impl AtReady for Ready {
    type Out = (u64, usize);
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        _owner: &mut Owner<'c, 's>,
        _flusher: &mut Owner<'c, 's>,
        _private: &mut [u8; super::super::super::super::CATALOG_PRIVATE_BYTES],
    ) -> Self::Out {
        assert_row_jobs_free(live.catalog_test_jobs());
        (
            live.catalog_test_q(),
            row_jobs(live.catalog_test_jobs()).len(),
        )
    }
}

fn busy(error: &RetentionError) -> bool {
    error.effect == Effect::Unchanged
        && matches!(
            error.cause,
            RetentionCause::Visibility(visibility) if matches!(visibility.cause, VisibilityCause::Busy)
        )
}

/// LRR-01: 33 one-row transactions on the same Row Status, each committed
/// and then cleaned live (Prune, three ReleaseRole, RootFree,
/// TerminalFinish). Without the live cleanup the 64 Job cells hold 32 such
/// transactions and the second Begin on the same Status has no free slot.
/// After every cleanup no Row Job is held; at the end a new statement reads
/// all 33 rows with their values, and C's recovery of the files reaches
/// READY at the same Q with no Row Job left to clean.
///
/// Every group revalidates the whole WAL (F6-a), so the cost grows with the
/// square of the transaction count: about 500 s and 41 MiB of files in a
/// debug build (each round adds about 0.6 MB). The bounds are raised for
/// this case only; the WAL cap is 256 MiB.
#[test]
fn lrr01_live_cleanup_reuses_jobs_status_and_undo_beyond_32_transactions() {
    case_within("LRR-01", std::time::Duration::from_secs(1200), || {
        let oracle = Oracle::new();
        let images = with_fresh!(
            oracle,
            frames = 16,
            file_bytes = 48 * 1_048_576,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let mut rows = Vec::new();
                let started = std::time::Instant::now();
                {
                    let (mut writer, reader, mut flush) = live.split();
                    for round in 0..TRANSACTIONS {
                        let tx = writer
                            .begin_row_transaction(
                                owner,
                                IsolationLevel::Serializable,
                                &row_status,
                                private,
                                &mut flush,
                            )
                            .unwrap_or_else(|error| panic!("Begin {round}: {error:?}"));
                        let view = writer.begin_statement(owner, &tx).unwrap();
                        let table =
                            issue_table!(reader, owner, view, reader_private, flush, flusher);
                        let value = 100 + round as i64;
                        rows.push((
                            put_row!(writer, owner, view, table, private, flush, flusher, value),
                            value,
                        ));
                        assert!(writer.finish_statement(owner, view).is_ok());
                        // A running actor has nothing to clean.
                        let q = committed!(writer.commit_row(owner, tx, private)).q();
                        assert_eq!(q, 2 + round as u64);
                        let done = retire_all!(writer, owner, private, flush, flusher);
                        assert_eq!(done, COMMITTED, "round {round}");
                        assert_row_jobs_free(writer.catalog_test_jobs());
                        assert!(!fence.is_fenced());
                        if round % 8 == 0 || round + 1 == TRANSACTIONS {
                            let bytes: u64 = state
                                .lock()
                                .unwrap()
                                .files
                                .values()
                                .map(|file| file.length)
                                .sum();
                            println!(
                                "LRR_01 round={round} elapsed_ms={} file_bytes={bytes}",
                                started.elapsed().as_millis()
                            );
                        }
                    }
                    let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let b = &mut b_owner;
                    let read = writer
                        .begin_transaction(
                            b,
                            TransactionOptions {
                                read_only: true,
                                isolation: IsolationLevel::ReadCommitted,
                            },
                            reader_private,
                        )
                        .unwrap();
                    let view = writer.begin_statement(b, &read).unwrap();
                    assert_eq!(view.q().unwrap(), 1 + TRANSACTIONS as u64);
                    let table = issue_table!(reader, b, view, reader_private, flush, flusher);
                    for (row, value) in &rows {
                        assert_eq!(
                            point_at!(reader, b, reader_private, &view, &table, *row),
                            Some(*value)
                        );
                    }
                    assert!(writer.finish_statement(b, view).is_ok());
                    assert!(writer.finish_read_only(b, read).is_ok());
                    assert_eq!(reader.catalog_test_source_used(), 0);
                }
                assert_eq!(live.catalog_test_q(), 1 + TRANSACTIONS as u64);
                println!(
                    "LRR_01 transactions={TRANSACTIONS} rows={} q={} cleanup_per_actor={COMMITTED:?}",
                    rows.len(),
                    live.catalog_test_q()
                );
                state.lock().unwrap().images()
            }
        );
        let directory = clone_directory(&images);
        let reopened = std::time::Instant::now();
        let (q, cells) = reopen(&directory, &mut Ready);
        assert_eq!(q, 1 + TRANSACTIONS as u64);
        println!(
            "LRR_01 reopen ready_q={q} row_cells={cells} elapsed_ms={} ROW_JOBS_FREE",
            reopened.elapsed().as_millis()
        );
    });
}

/// LRR-02: a statement snapshot taken before the Commit keeps the head
/// Prune Busy (Unchanged: no stage, WAL, Job or fence change), since the
/// actor's Q is newer than that snapshot. The old statement still reads
/// nothing. After it ends, the whole cleanup runs, and a new statement of
/// the same read-only transaction reads the pruned row.
#[test]
fn lrr02_older_snapshot_keeps_prune_busy_until_it_ends() {
    case("LRR-02", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let row = put_row!(writer, owner, view, table, private, flush, flusher, 7);
                assert!(writer.finish_statement(owner, view).is_ok());
                let read = writer
                    .begin_transaction(
                        b,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer.begin_statement(b, &read).unwrap();
                let q = committed!(writer.commit_row(owner, tx, private)).q();
                assert!(old.q().unwrap() < q);
                let (ordinal, operation) = writer.row_cleanup_next(&[]).unwrap().unwrap();
                assert_eq!(operation, RowCleanup::Prune);
                let jobs = writer.catalog_test_jobs().to_vec();
                let (wal, _) = files(state);
                for _ in 0..2 {
                    match writer.retire_row(owner, ordinal, operation, private, &mut flush) {
                        Err(error) => assert!(busy(&error), "{error:?}"),
                        Ok(step) => panic!("Prune beside an older snapshot: {step:?}"),
                    }
                    assert_eq!(files(state).0, wal, "a refused retirement stages nothing");
                    assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
                    assert!(!fence.is_fenced());
                    owner.check_idle().unwrap();
                }
                let old_table = issue_table!(reader, b, old, reader_private, flush, flusher);
                assert_eq!(
                    point_at!(reader, b, reader_private, &old, &old_table, row),
                    None
                );
                assert!(writer.finish_statement(b, old).is_ok());
                assert_eq!(
                    retire_all!(writer, owner, private, flush, flusher),
                    COMMITTED
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                let new = writer.begin_statement(b, &read).unwrap();
                let new_table = issue_table!(reader, b, new, reader_private, flush, flusher);
                assert_eq!(
                    point_at!(reader, b, reader_private, &new, &new_table, row),
                    Some(7)
                );
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert!(!fence.is_fenced());
                println!("LRR_02 q={q} prune_busy=2 then cleanup={COMMITTED:?}");
            }
        );
    });
}

/// LRR-03: refusals, each Unchanged with no WAL or Job change. A running
/// actor has no cleanup step, and its root refuses Prune; a non-cleanup
/// operation is Unsupported. After the Commit of two roots, the non-head
/// root refuses Prune and the Terminal refuses TerminalFinish while a root
/// is left. Then the whole cleanup runs head first.
#[test]
fn lrr03_running_actor_non_head_and_early_terminal_are_refused() {
    case("LRR-03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                // A second Free Undo container for the second root.
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let actor = view.own_actor().unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 7);
                put_row!(writer, owner, view, table, private, flush, flusher, 8);
                assert!(writer.finish_statement(owner, view).is_ok());
                let mut roots: Vec<(usize, u64)> = Vec::new();
                let mut terminal = None;
                for (ordinal, bytes) in writer.catalog_test_jobs().iter().enumerate() {
                    let job =
                        job3::decode_job(bytes, super::super::super::support::namespace()).unwrap();
                    if job.source() != Some(actor) {
                        continue;
                    }
                    match job.state() {
                        JobState::RootRetained => roots.push((ordinal, job.undo_sequence())),
                        JobState::TerminalLeased => terminal = Some(ordinal),
                        other => panic!("running actor Job {other:?}"),
                    }
                }
                roots.sort_by_key(|(_, sequence)| *sequence);
                assert_eq!(roots.len(), 2);
                let terminal = terminal.unwrap();
                let (head, behind) = (roots[1].0, roots[0].0);
                let mut refused = 0;
                macro_rules! refused {
                    ($ordinal:expr, $operation:expr) => {{
                        let jobs = writer.catalog_test_jobs().to_vec();
                        let (wal, _) = files(state);
                        match writer.retire_row(owner, $ordinal, $operation, private, &mut flush) {
                            Err(error) => {
                                assert_eq!(error.effect, Effect::Unchanged, "{error:?}");
                                refused += 1;
                            }
                            Ok(step) => panic!("{:?} accepted: {step:?}", $operation),
                        }
                        assert_eq!(files(state).0, wal);
                        assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
                        assert!(!fence.is_fenced());
                        owner.check_idle().unwrap();
                    }};
                }
                // Running: nothing to select, and the producer refuses.
                assert_eq!(writer.row_cleanup_next(&[]).unwrap(), None);
                refused!(head, RowCleanup::Prune);
                refused!(terminal, RowCleanup::TerminalFinish);
                refused!(head, RowCleanup::Cancel);
                refused!(head, RowCleanup::Commit);
                assert!(committed!(writer.commit_row(owner, tx, private)).q() == 2);
                assert_eq!(
                    writer.row_cleanup_next(&[]).unwrap(),
                    Some((head, RowCleanup::Prune))
                );
                refused!(behind, RowCleanup::Prune);
                refused!(terminal, RowCleanup::TerminalFinish);
                refused!(head, RowCleanup::RootFree);
                let done = retire_all!(writer, owner, private, flush, flusher);
                let count = |operation| done.iter().filter(|done| **done == operation).count();
                assert_eq!(
                    (
                        count(RowCleanup::Prune),
                        count(RowCleanup::ReleaseRole),
                        count(RowCleanup::RootFree),
                        count(RowCleanup::TerminalFinish)
                    ),
                    (2, 6, 2, 1)
                );
                assert_eq!(done[0], RowCleanup::Prune);
                assert_eq!(done.last(), Some(&RowCleanup::TerminalFinish));
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                println!("LRR_03 refused={refused} cleanup={done:?}");
            }
        );
    });
}

/// LRR-04: a fully rolled back actor (its root RootReleasingCompensated
/// after the CLR, its Terminal TerminalReady Aborted) is cleaned live by
/// ReleaseRole ×3, RootFree and TerminalFinish, with no Prune. The same
/// Status then begins, commits and cleans another transaction.
#[test]
fn lrr04_aborted_actor_is_cleaned_and_its_status_reused() {
    case("LRR-04", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 7);
                assert!(writer.finish_statement(owner, view).is_ok());
                {
                    let mut rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            None,
                            RollbackScope::Transaction,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    let mut clr = 0;
                    loop {
                        let target = match rollback.next().unwrap() {
                            RollbackStep::Candidate(mut task) => {
                                let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
                                let prepared = applied!(task.prepare(proposal));
                                assert_eq!(
                                    applied!(prepared.apply()).operation(),
                                    RowOperation::Clr
                                );
                                clr += 1;
                                None
                            }
                            RollbackStep::FlushRequired { target } => Some(target),
                            RollbackStep::Complete => break,
                        };
                        if let Some(target) = target {
                            rollback.flush(&target).unwrap();
                        }
                        assert!(clr <= 1);
                    }
                    applied!(rollback.finish());
                }
                assert!(writer.finish_aborted(owner, tx).is_ok());
                assert_eq!(
                    retire_all!(writer, owner, private, flush, flusher),
                    COMMITTED[1..]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 8);
                assert!(writer.finish_statement(owner, view).is_ok());
                assert_eq!(committed!(writer.commit_row(owner, tx, private)).q(), 2);
                assert_eq!(
                    retire_all!(writer, owner, private, flush, flusher),
                    COMMITTED
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                println!(
                    "LRR_04 aborted_cleanup={:?} reused_status_q=2",
                    &COMMITTED[1..]
                );
            }
        );
    });
}

/// LRR-05: a Busy Prune does not hide another finished actor. A commits
/// after an older statement snapshot, so its head Prune is Busy; B (on a
/// second Status, its Jobs after A's) then rolls back fully. The first step
/// is A's Prune and it is refused; with A's ordinal skipped the selection
/// reaches B, whose whole cleanup runs (ReleaseRole ×3, RootFree,
/// TerminalFinish) while A's Prune stays Busy, and nothing else is left.
/// After the snapshot ends A's cleanup runs.
#[test]
fn lrr05_busy_prune_does_not_hide_another_finished_actor() {
    case("LRR-05", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let second = system_init(live, owner, flusher, false);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let a = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &a).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 7);
                assert!(writer.finish_statement(owner, view).is_ok());
                let read = writer
                    .begin_transaction(
                        b,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer.begin_statement(b, &read).unwrap();
                assert_eq!(committed!(writer.commit_row(owner, a, private)).q(), 2);
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &second,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 8);
                assert!(writer.finish_statement(owner, view).is_ok());
                {
                    let mut rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            None,
                            RollbackScope::Transaction,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    loop {
                        let target = match rollback.next().unwrap() {
                            RollbackStep::Candidate(mut task) => {
                                let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
                                let prepared = applied!(task.prepare(proposal));
                                assert_eq!(
                                    applied!(prepared.apply()).operation(),
                                    RowOperation::Clr
                                );
                                None
                            }
                            RollbackStep::FlushRequired { target } => Some(target),
                            RollbackStep::Complete => break,
                        };
                        if let Some(target) = target {
                            rollback.flush(&target).unwrap();
                        }
                    }
                    applied!(rollback.finish());
                }
                assert!(writer.finish_aborted(owner, tx).is_ok());
                let (held, operation) = writer.row_cleanup_next(&[]).unwrap().unwrap();
                assert_eq!(operation, RowCleanup::Prune);
                let jobs = writer.catalog_test_jobs().to_vec();
                let (wal, _) = files(state);
                match retire_one!(writer, owner, private, flush, flusher, held, operation) {
                    Err(error) => assert!(busy(&error), "{error:?}"),
                    Ok(observation) => panic!("Prune beside an older snapshot: {observation:?}"),
                }
                assert_eq!(files(state).0, wal, "a refused retirement stages nothing");
                assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
                owner.check_idle().unwrap();
                let mut other = Vec::new();
                while let Some((ordinal, operation)) = writer.row_cleanup_next(&[held]).unwrap() {
                    assert!(other.len() < 8, "bounded live cleanup");
                    if let Err(error) =
                        retire_one!(writer, owner, private, flush, flusher, ordinal, operation)
                    {
                        panic!("live {operation:?} of Job {ordinal}: {error:?}");
                    }
                    owner.check_idle().unwrap();
                    other.push(operation);
                }
                assert_eq!(other, COMMITTED[1..]);
                // Only A's root and A's Terminal are still held.
                let mut left: Vec<_> = row_jobs(writer.catalog_test_jobs())
                    .into_iter()
                    .filter(|(_, state, _, _)| *state != JobState::Free)
                    .map(|(ordinal, state, _, _)| (ordinal, state))
                    .collect();
                left.sort_by_key(|(_, state)| *state != JobState::RootRetained);
                assert_eq!(left.len(), 2, "{left:?}");
                assert_eq!(left[0], (held, JobState::RootRetained));
                assert_eq!(left[1].1, JobState::TerminalReady);
                assert_eq!(
                    writer.row_cleanup_next(&[]).unwrap(),
                    Some((held, RowCleanup::Prune))
                );
                match retire_one!(writer, owner, private, flush, flusher, held, operation) {
                    Err(error) => assert!(busy(&error), "{error:?}"),
                    Ok(observation) => panic!("Prune beside an older snapshot: {observation:?}"),
                }
                assert!(writer.finish_statement(b, old).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert_eq!(
                    retire_all!(writer, owner, private, flush, flusher),
                    COMMITTED
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                println!("LRR_05 busy_prune_skipped other_cleanup={other:?} then={COMMITTED:?}");
            }
        );
    });
}

/// LRR-06: the live cleanup stops after Prune and one ReleaseRole (each
/// durable). C's recovery of those files finishes the remaining
/// ReleaseRole ×2, RootFree and TerminalFinish and reaches READY at the same
/// Q with no Row Job held. The count of `FlushRequired` steps is printed:
/// the setup needs more than ten frames, and from eleven up the cleanup's
/// residency has not needed a victim, so that branch stays unexercised.
#[test]
fn lrr06_recovery_finishes_a_partial_live_cleanup() {
    case("LRR-06", || {
        let oracle = Oracle::new();
        RETIRE_FLUSHES.with(|count| count.set(0));
        let images = with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 7);
                assert!(writer.finish_statement(owner, view).is_ok());
                assert_eq!(committed!(writer.commit_row(owner, tx, private)).q(), 2);
                let mut done = Vec::new();
                for _ in 0..2 {
                    let (ordinal, operation) = writer.row_cleanup_next(&[]).unwrap().unwrap();
                    if let Err(error) =
                        retire_one!(writer, owner, private, flush, flusher, ordinal, operation)
                    {
                        panic!("live {operation:?} of Job {ordinal}: {error:?}");
                    }
                    done.push(operation);
                }
                assert_eq!(done, COMMITTED[..2]);
                let cursors: Vec<_> = row_jobs(writer.catalog_test_jobs())
                    .into_iter()
                    .filter(|(_, state, _, _)| *state != JobState::Free)
                    .map(|(_, state, cursor, _)| (state, cursor))
                    .collect();
                assert!(
                    cursors.contains(&(JobState::RootReleasingCommitted, 1)),
                    "{cursors:?}"
                );
                assert!(!fence.is_fenced());
                println!(
                    "LRR_06 live={done:?} held={cursors:?} flushes={}",
                    RETIRE_FLUSHES.with(|count| count.get())
                );
                state.lock().unwrap().images()
            }
        );
        let directory = clone_directory(&images);
        let (q, cells) = reopen(&directory, &mut Ready);
        assert_eq!(q, 2);
        println!("LRR_06 reopen ready_q={q} row_cells={cells} ROW_JOBS_FREE");
    });
}

/// F6 M0: the WAL revalidation counts of a one-row transaction and of its
/// live cleanup (evidence `f6-wal-revalidation-survey-20261102.md` §7).
#[path = "retire/revalidation.rs"]
mod revalidation;

/// The live Row cleanup caller: A's cleanup drain after A's Commit or
/// finish_aborted (reclamation `#live-row-cleanup-caller`).
#[path = "retire/driver.rs"]
mod driver;

/// The autocommit INSERT executor over the actual C, with the caller's
/// cleanup after it (boundaries `#autocommit-insert`).
#[path = "retire/autocommit.rs"]
mod autocommit;
