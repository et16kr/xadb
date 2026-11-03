//! The autocommit INSERT executor (boundaries `#autocommit-insert`) and an
//! explicit transaction's entries (`#explicit-transaction`): A's
//! `autocommit_insert_live`, and `begin_transaction_live`,
//! `transaction_insert_live`, `commit_transaction_live` and
//! `rollback_transaction_live`, over the actual C supply, from Begin to the
//! transaction's end, then the caller's cleanup (`retire_finished_live`,
//! again while a drain is not complete). Expectations are state-machine
//! facts (the outcome, the groups each drain installed, freed Jobs, the
//! rows a new statement and a restart read), never values read back from
//! the same producer.
use super::super::super::super::super::RowRecoveryPolicy;
use super::super::join_restart;
use super::*;
use crate::storage::{
    disk::{
        CheckedTable, MaintenancePort, ReadPort, RowCommitObservation, RowMutationObservation,
        RowStatementView,
    },
    row::insert::{
        InsertError,
        driver::{
            DriverError, Failure, Retired, Stage,
            autocommit::Autocommit,
            explicit::Explicit,
            live::{
                LiveCall, LiveTables, autocommit_insert_live, begin_transaction_live,
                commit_transaction_live, retire_finished_live, rollback_transaction_live,
                transaction_insert_live,
            },
        },
    },
};

/// The caller's table source: B's statement call over the statement view
/// and `finish_table` of `object`, each FlushRequired flushed through the
/// executor's flusher and retried (as `issue_table!`).
struct Tables<'r, 'q, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    reader: &'r ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    private: &'q mut [u8; 425_984],
    object: u64,
}

impl<'a, 'c, 's, W: WriterNamespace> LiveTables<'a, 'c, 's, W>
    for Tables<'_, '_, '_, 'a, 's, 'c, '_, '_, '_, '_, W>
{
    fn issue(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        flusher: &mut MaintenancePort<'_, 's, 'c, '_, '_, '_, '_, W>,
        flush_owner: &mut Owner<'c, 's>,
    ) -> std::result::Result<Option<CheckedTable<'c, 's>>, RetentionError> {
        for _ in 0..8 {
            let call = self.reader.catalog_statement_call(owner, statement)?;
            let target = match call.finish_table(
                ObjectId::new(self.object).unwrap(),
                &mut Plan4ResidencyCursor::new(),
                self.private,
            )? {
                TableStep::Table(table) => return Ok(Some(table)),
                TableStep::Missing => return Ok(None),
                TableStep::FlushRequired { target } => target,
            };
            let mut step = flusher.try_flush(flush_owner, &target)?;
            for _ in 0..8 {
                if matches!(
                    step,
                    RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
                ) {
                    break;
                }
                step = flusher.continue_flush(flush_owner)?;
            }
        }
        panic!("bounded table issue")
    }
}

/// One autocommit INSERT of `values` into `object` on `status` through A's
/// executor: its outcome and the rows' slots.
macro_rules! autocommit {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $status:expr, $private:expr, $object:expr, $values:expr) => {{
        let values: Vec<[ValueRef<'static>; 1]> = $values;
        let rows: Vec<&[ValueRef<'_>]> = values.iter().map(|v| &v[..]).collect();
        let mut slots: Vec<Option<RowMutationObservation>> = rows.iter().map(|_| None).collect();
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: $object,
        };
        let mut out = Autocommit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        autocommit_insert_live(
            &mut call,
            $status,
            IsolationLevel::Serializable,
            &mut tables,
            $private,
            &rows,
            &mut slots,
            &mut out,
        );
        (out, slots)
    }};
}

/// The caller's cleanup: A's drain again while it is not complete; the
/// groups each drain installed.
macro_rules! drains {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $private:expr) => {{
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        let mut drains = Vec::new();
        for _ in 0..4 {
            match retire_finished_live(&mut call, $private) {
                Ok(retired) => {
                    drains.push(retired);
                    if retired.complete {
                        break;
                    }
                }
                Err(stop) => panic!("A's cleanup drain stopped {stop:?}"),
            }
        }
        drains
    }};
}

fn drain(groups: usize, complete: bool) -> Retired {
    Retired {
        groups,
        held: 0,
        complete,
    }
}

fn committed(out: Autocommit<RowCommitObservation>) -> (u64, usize) {
    match out {
        Autocommit::Committed { committed, rows } => (committed.q(), rows),
        other => panic!("expected Committed, got {other:?}"),
    }
}

fn aborted(out: Autocommit<RowCommitObservation>) -> Failure {
    match out {
        Autocommit::Aborted(failure) => {
            assert!(!failure.must_resolve && failure.cleanup.is_none());
            failure
        }
        other => panic!("expected Aborted, got {other:?}"),
    }
}

fn ints(values: &[i64]) -> Vec<[ValueRef<'static>; 1]> {
    values.iter().map(|v| [ValueRef::Int64(*v)]).collect()
}

/// LAC-J01: one autocommit INSERT of 13 rows through A's executor commits
/// at q 2 with every row applied (the first Row profile writes each row
/// under its own Free Undo page, so the setup adds twelve). Its cleanup is 5 × 13 + 1 = 66 groups, so
/// the caller's first drain stops at its 64-group bound (not complete) and
/// a second installs the other two. No Row Job is left, a new statement
/// reads the 13 rows, and C's recovery of the files reaches READY at q 2
/// with no CLR (no loser), no Row Job left and B's cursor reading the same
/// rows.
#[test]
fn lac_j01_autocommit_of_thirteen_rows_commits_and_two_drains_clean_it() {
    case_within("LAC-J01", std::time::Duration::from_secs(600), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                // Each row is written under its own Free Undo page.
                for _ in 1..13 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let values: Vec<i64> = (100..113).collect();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&values)
                );
                assert_eq!(committed(out), (2, 13));
                let rows: Vec<_> = slots
                    .into_iter()
                    .zip(&values)
                    .map(|(slot, value)| (slot.expect("applied row").row(), *value))
                    .collect();
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(64, false), drain(2, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
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
                assert_eq!(view.q().unwrap(), 2);
                let table = issue_table!(reader, b, view, reader_private, flush, flusher);
                for (row, value) in &rows {
                    assert_eq!(
                        point_at!(reader, b, reader_private, &view, &table, *row),
                        Some(*value)
                    );
                }
                assert!(writer.finish_statement(b, view).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert!(!fence.is_fenced());
                let expected: Vec<(u64, i64)> = rows
                    .iter()
                    .map(|(row, value)| (row.get(), *value))
                    .collect();
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        let mut read = joined.rows.clone();
        read.sort_unstable();
        let mut expected = expected;
        expected.sort_unstable();
        assert_eq!(read, expected);
        println!(
            "LAC_J01 q=2 rows=13 drains=64,2 restart_steps={} restart_rows={}",
            joined.steps,
            joined.rows.len()
        );
    });
}

/// LAC-J02: on one Row Status, (1) a statement whose second row A refuses
/// (NULL in a NOT NULL column) after the first applied: the executor rolls
/// the statement back to its mark, finishes it, rolls the transaction back
/// and ends it (`Aborted`, A's Failure); (2) a statement whose table does
/// not exist: the caller's source issues none, the statement is finished
/// with no row and the transaction ended (`Aborted`, Table InvalidInput);
/// (3) a statement of one row that commits at q 2. After each, the caller's
/// one drain is complete with the abort's 4n + 1 groups (5 for the row
/// written and compensated, 1 for none) or the commit's 5n + 1 (6), and no
/// Row Job is left. A new statement reads the committed row and not the
/// rolled back one.
#[test]
fn lac_j02_refused_row_and_missing_table_abort_then_a_commit() {
    case_within("LAC-J02", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                // The refused second row's preflight selects a second Free
                // Undo page before A's validation refuses it.
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();

                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    vec![[ValueRef::Int64(7)], [ValueRef::Null]]
                );
                let failure = aborted(out);
                assert!(
                    matches!(failure.error, DriverError::Insert(_)),
                    "{failure:?}"
                );
                let rolled_back = slots[0].as_ref().expect("the first row applied").row();
                assert!(slots[1].is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(5, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());

                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    99,
                    ints(&[8])
                );
                let failure = aborted(out);
                assert_eq!(failure.stage, Stage::Table);
                assert!(
                    matches!(
                        failure.error,
                        DriverError::Insert(InsertError::InvalidInput)
                    ),
                    "{failure:?}"
                );
                assert!(slots[0].is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(1, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());

                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[9])
                );
                assert_eq!(committed(out), (2, 1));
                let row = slots[0].as_ref().expect("applied row").row();
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());

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
                let table = issue_table!(reader, b, view, reader_private, flush, flusher);
                assert_eq!(
                    point_at!(reader, b, reader_private, &view, &table, row),
                    Some(9)
                );
                assert_eq!(
                    point_at!(reader, b, reader_private, &view, &table, rolled_back),
                    None
                );
                assert!(writer.finish_statement(b, view).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert!(!fence.is_fenced());
                println!(
                    "LAC_J02 aborted_drains=5,1 committed_drain=6 q=2 rolled_back={rolled_back:?}"
                );
            }
        );
    });
}

// ---- An explicit transaction (`#explicit-transaction`, LXT-J01..J05) ----

use foundation::identity::RowId;

/// C's Row Begin into `holder` through A's entry: the slot.
macro_rules! begin {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $status:expr, $private:expr, $holder:expr) => {{
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        begin_transaction_live(
            &mut call,
            $status,
            IsolationLevel::Serializable,
            $private,
            $holder,
            &mut out,
        );
        out
    }};
}

/// One INSERT statement of `values` into `object` in the transaction in
/// `holder` through A's entry: the slot and the rows' slots.
macro_rules! statement {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $holder:expr, $private:expr, $object:expr, $values:expr) => {{
        let values: Vec<[ValueRef<'static>; 1]> = $values;
        let rows: Vec<&[ValueRef<'_>]> = values.iter().map(|v| &v[..]).collect();
        let mut slots: Vec<Option<RowMutationObservation>> = rows.iter().map(|_| None).collect();
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: $object,
        };
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        transaction_insert_live(
            &mut call,
            $holder,
            &mut tables,
            $private,
            &rows,
            &mut slots,
            &mut out,
        );
        (out, slots)
    }};
}

/// C's Commit of the transaction in `holder` through A's entry: the slot.
macro_rules! commit {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $holder:expr, $private:expr, $rows:expr) => {{
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        commit_transaction_live(&mut call, $holder, $private, $rows, &mut out);
        out
    }};
}

/// The caller's rollback of the transaction in `holder` through A's entry:
/// the slot.
macro_rules! rollback {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $holder:expr, $private:expr) => {{
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        rollback_transaction_live(&mut call, $holder, $private, &mut out);
        out
    }};
}

fn begun(out: Explicit<RowCommitObservation>) {
    assert!(
        matches!(out, Explicit::Begun),
        "expected Begun, got {out:?}"
    );
}

fn done(out: Explicit<RowCommitObservation>) -> usize {
    match out {
        Explicit::Done { rows } => rows,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn statement_failed(out: Explicit<RowCommitObservation>) -> Failure {
    match out {
        Explicit::Failed(failure) => {
            assert!(!failure.must_resolve && failure.cleanup.is_none());
            failure
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

fn explicit_committed(out: Explicit<RowCommitObservation>) -> (u64, usize) {
    match out {
        Explicit::Committed { committed, rows } => (committed.q(), rows),
        other => panic!("expected Committed, got {other:?}"),
    }
}

/// The stage of a `NoTransaction` slot whose entry found the holder not as
/// it needs it (no C call: `OrderMismatch`).
fn order_mismatch(out: Explicit<RowCommitObservation>) -> Stage {
    match out {
        Explicit::NoTransaction(failure) => {
            assert!(
                matches!(
                    failure.error,
                    DriverError::Insert(InsertError::OrderMismatch)
                ) && !failure.must_resolve,
                "{failure:?}"
            );
            failure.stage
        }
        other => panic!("expected NoTransaction, got {other:?}"),
    }
}

/// The rows the slots name, with their values, in order.
fn applied(slots: Vec<Option<RowMutationObservation>>, values: &[i64]) -> Vec<(RowId, i64)> {
    slots
        .into_iter()
        .zip(values)
        .map(|(slot, value)| (slot.expect("applied row").row(), *value))
        .collect()
}

/// A new ReadCommitted statement's point read of each row: its value, or
/// None.
macro_rules! read_rows {
    ($writer:ident, $reader:ident, $core:expr, $reader_private:ident, $flush:ident, $flusher:ident, $rows:expr) => {{
        let mut b_owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let b = &mut b_owner;
        let read = $writer
            .begin_transaction(
                b,
                TransactionOptions {
                    read_only: true,
                    isolation: IsolationLevel::ReadCommitted,
                },
                $reader_private,
            )
            .unwrap();
        let view = $writer.begin_statement(b, &read).unwrap();
        let table = issue_table!($reader, b, view, $reader_private, $flush, $flusher);
        let read_back: Vec<Option<i64>> = $rows
            .iter()
            .map(|row| point_at!($reader, b, $reader_private, &view, &table, *row))
            .collect();
        assert!($writer.finish_statement(b, view).is_ok());
        assert!($writer.finish_read_only(b, read).is_ok());
        read_back
    }};
}

/// LXT-J01: one explicit transaction of two statements (two rows, then one)
/// through A's entries commits at q 2 with the caller's count of 3 rows. Its
/// cleanup is 5 × 3 + 1 = 16 groups in one drain, no Row Job is left, a new
/// statement reads the three rows, and C's recovery of the files reaches
/// READY at q 2 with no CLR, no Row Job left and B's cursor reading the same
/// rows.
#[test]
fn lxt_j01_two_statements_commit_and_restart() {
    case_within("LXT-J01", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, first) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(done(out), 2);
                let (out, second) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[102])
                );
                assert_eq!(done(out), 1);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        3
                    )),
                    (2, 3)
                );
                assert!(holder.is_none());
                let mut rows = applied(first, &[100, 101]);
                rows.extend(applied(second, &[102]));
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(16, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                let ids: Vec<RowId> = rows.iter().map(|(row, _)| *row).collect();
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [Some(100), Some(101), Some(102)]
                );
                assert!(!fence.is_fenced());
                let expected: Vec<(u64, i64)> = rows
                    .iter()
                    .map(|(row, value)| (row.get(), *value))
                    .collect();
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        let mut read = joined.rows.clone();
        read.sort_unstable();
        let mut expected = expected;
        expected.sort_unstable();
        assert_eq!(read, expected);
        println!(
            "LXT_J01 q=2 rows=3 drain=16 restart_steps={} restart_rows={}",
            joined.steps,
            joined.rows.len()
        );
    });
}

/// LXT-J02: a failed statement between two that stand. Statement 1 applies
/// one row; statement 2 applies one row and A refuses its second (NULL in a
/// NOT NULL column): the entry rolls statement 2 back to its mark and
/// finishes it (`Failed`), and the transaction goes on; statement 3 applies
/// one row; the Commit is at q 2. Its cleanup is one drain of 5 × 2 + 4 + 1
/// = 15 groups (the rolled back row has no Prune), and no Row Job is left. A
/// new statement reads rows 1 and 3, not the rolled back one, and C's
/// recovery of the files reaches READY at q 2 with no CLR (the transaction
/// is no loser) and reads the same two rows.
#[test]
fn lxt_j02_failed_statement_between_two_that_commit() {
    case_within("LXT-J02", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                // Rows 1, 2 and 3 each take a Free Undo page; the refused
                // row's preflight selects the third before A refuses it.
                for _ in 1..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, first) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[7])
                );
                assert_eq!(done(out), 1);
                let (out, failed) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    vec![[ValueRef::Int64(8)], [ValueRef::Null]]
                );
                let failure = statement_failed(out);
                assert!(
                    matches!(failure.error, DriverError::Insert(_)),
                    "{failure:?}"
                );
                assert!(holder.is_some(), "the transaction goes on");
                let rolled_back = failed[0].as_ref().expect("the row applied").row();
                assert!(failed[1].is_none());
                let (out, third) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[9])
                );
                assert_eq!(done(out), 1);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        2
                    )),
                    (2, 2)
                );
                let mut rows = applied(first, &[7]);
                rows.extend(applied(third, &[9]));
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(15, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                let ids = [rows[0].0, rolled_back, rows[1].0];
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [Some(7), None, Some(9)]
                );
                assert!(!fence.is_fenced());
                let expected: Vec<(u64, i64)> = rows
                    .iter()
                    .map(|(row, value)| (row.get(), *value))
                    .collect();
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        let mut read = joined.rows.clone();
        read.sort_unstable();
        let mut expected = expected;
        expected.sort_unstable();
        assert_eq!(read, expected);
        println!(
            "LXT_J02 q=2 standing=2 rolled_back=1 drain=15 restart_steps={} restart_rows={}",
            joined.steps,
            joined.rows.len()
        );
    });
}

/// LXT-J03: the caller's rollback. A transaction of one statement of two
/// rows is rolled back through A's entry (`RolledBack`): its cleanup is one
/// drain of 4 × 2 + 1 = 9 groups and no Row Job is left. A second explicit
/// transaction on the same Status commits one row at q 2 (6 groups). A new
/// statement reads that row and neither rolled back one.
#[test]
fn lxt_j03_caller_rollback_then_a_commit() {
    case_within("LXT-J03", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[7, 8])
                );
                assert_eq!(done(out), 2);
                let rolled_back: Vec<RowId> = applied(slots, &[7, 8])
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect();
                let out = rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                assert!(
                    matches!(out, Explicit::RolledBack),
                    "expected RolledBack, got {out:?}"
                );
                assert!(holder.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(9, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());

                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[9])
                );
                assert_eq!(done(out), 1);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        1
                    )),
                    (2, 1)
                );
                let kept = applied(slots, &[9])[0].0;
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                let ids = [kept, rolled_back[0], rolled_back[1]];
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [Some(9), None, None]
                );
                assert!(!fence.is_fenced());
                println!("LXT_J03 rolled_back_drain=9 committed_drain=6 q=2");
            }
        );
    });
}

/// LXT-J04: the holder's order and a missing table. Begin into a holder
/// that holds the transaction is refused with no C call (`NoTransaction`,
/// Begin, `OrderMismatch`) and the transaction is kept. Statement 1 applies
/// one row; statement 2 names a table that does not exist: the caller's
/// source issues none and the statement is finished with no row (`Failed`,
/// Table `InvalidInput`), the transaction going on; the Commit is at q 2.
/// Then the holder is empty: a statement, a Commit and a rollback are each
/// refused with no C call. One drain of 6 groups, no Row Job left, and a
/// new statement reads the row.
#[test]
fn lxt_j04_holder_order_and_missing_table() {
    case_within("LXT-J04", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                assert_eq!(
                    order_mismatch(begin!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &row_status,
                        private,
                        &mut holder
                    )),
                    Stage::Begin
                );
                assert!(holder.is_some(), "the held transaction is kept");
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[7])
                );
                assert_eq!(done(out), 1);
                let row = applied(slots, &[7])[0].0;
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    99,
                    ints(&[8])
                );
                let failure = statement_failed(out);
                assert!(
                    failure.stage == Stage::Table
                        && matches!(
                            failure.error,
                            DriverError::Insert(InsertError::InvalidInput)
                        ),
                    "{failure:?}"
                );
                assert!(slots[0].is_none() && holder.is_some());
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        1
                    )),
                    (2, 1)
                );
                let (out, _) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[9])
                );
                assert_eq!(order_mismatch(out), Stage::Statement);
                assert_eq!(
                    order_mismatch(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        1
                    )),
                    Stage::Commit
                );
                assert_eq!(
                    order_mismatch(rollback!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private
                    )),
                    Stage::FinishAborted
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, [row]),
                    [Some(7)]
                );
                assert!(!fence.is_fenced());
                println!("LXT_J04 order_mismatch=Begin,Statement,Commit,Rollback drain=6 q=2");
            }
        );
    });
}

/// LXT-J05: refusals that leave the holder empty. C refuses a Begin on the
/// Root target (not a Status page) unchanged (Membership): `NoTransaction`
/// with C's refusal, and the holder stays empty. Then a Begin on the Status
/// page, and one live control byte flipped (as R1D-A5N): C's statement
/// start (the Row statement head's ledger check) answers CallerMustResolve,
/// so the entry calls C no more, drops the transaction into C's fence
/// (`Fenced`, must resolve) and empties the holder; no row was applied.
#[test]
fn lxt_j05_refused_begin_and_fenced_statement_leave_the_holder_empty() {
    case_within("LXT-J05", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let wrong = live.target(Plan4Page::Root).unwrap();
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = None;
                match begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &wrong,
                    private,
                    &mut holder
                ) {
                    Explicit::NoTransaction(failure) => assert!(
                        failure.stage == Stage::Begin
                            && !failure.must_resolve
                            && matches!(
                                &failure.error,
                                DriverError::Disk(error)
                                    if error.effect == Effect::Unchanged
                                        && matches!(error.cause, RetentionCause::Membership)
                            ),
                        "{failure:?}"
                    ),
                    other => panic!("expected NoTransaction, got {other:?}"),
                }
                assert!(holder.is_none(), "a refused Begin holds nothing");
                assert!(!fence.is_fenced());
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                // The ATT actor field of the live control bank (R1D-A5N).
                writer.row_test_flip_control(
                    super::super::super::row_transactions::ATT_ACTOR.start + 40,
                );
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[7])
                );
                match out {
                    Explicit::Fenced(failure) => assert!(
                        failure.stage == Stage::Statement
                            && failure.must_resolve
                            && failure.cleanup.is_none()
                            && matches!(
                                &failure.error,
                                DriverError::Disk(error)
                                    if error.effect == Effect::CallerMustResolve
                            ),
                        "{failure:?}"
                    ),
                    other => panic!("expected Fenced, got {other:?}"),
                }
                assert!(holder.is_none(), "the fenced transaction is not held");
                assert!(slots[0].is_none());
                assert!(fence.is_fenced());
                println!("LXT_J05 begin=NoTransaction(Membership) statement=Fenced holder=None");
            }
        );
    });
}

/// The DELETE first path over the actual C (dev3-row-delete.md#tests,
/// RDL-J): A's autocommit and explicit DELETE entries from Begin to the
/// transaction's end, the caller's cleanup and restart.
#[path = "delete.rs"]
mod delete;
