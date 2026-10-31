//! S7 C5 join (XADB-MODULE-ROW-JOIN-001, boundaries `#row-join-s7`): A's
//! INSERT driver and B's point/cursor driver over the actual C supply of
//! S2–S6 in one session, for the MCI01/MCI03 interactions the module plan
//! leaves to the join (consumers `#index-negative-input`). Values, RowIds,
//! q and visibility are fixed here from the history each case runs; B's
//! output is never the oracle of A's write or the other way round.
//!
//! A writes through `insert_live`/`commit_live` and its rollback entries.
//! The own-cursor case (MCI01-V02) is the one exception: `insert_live`
//! consumes the statement view a cursor borrows, so a cursor cannot stay
//! open across an A driver statement; that case applies the row through A's
//! validate/produce and C's reserve/prepare/apply on the same view.
use super::super::super::{Plan4RecoveryAcquireFailure, RowRecoveryPolicy};
use super::row_forward::{VALUE, files};
use super::row_preflight::{create_table, system_init};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RecoveryAction, RetentionCause, RetentionError,
        RowMutationObservation, RowOwnedError, RowPreflightStep, TableStep, TransactionFinishError,
        TransactionOptions,
    },
    row::{
        insert::{
            driver::{
                After, DriveError, DriverError, Failure, PendingRollback, Stepped,
                live::{
                    LiveCall, commit_live, finish_aborted_live, finish_rollback_live,
                    finish_statement_live, insert_live, rollback_step_live,
                    start_full_rollback_live, start_statement_rollback_live,
                },
                recovered::handle_recovered_clr,
            },
            produce_insert, validate_insert,
        },
        read::driver::{
            CursorStep, PointRead, RowReadError,
            live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
        },
    },
    row_record::ValueRef,
};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use foundation::{identity::RowId, runtime::Cancellation};
use std::{
    sync::{Mutex, mpsc::sync_channel},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(10);

/// The Int64 value of T's (or U's) one column in a copied canonical row.
fn int_of(output: &[u8], len: usize) -> i64 {
    let row = crate::storage::row_record::decode_canonical_row(
        &output[..len],
        &[Column {
            name: "i".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    )
    .unwrap();
    match row.value(0).unwrap() {
        ValueRef::Int64(value) => value,
        other => panic!("column value {other:?}"),
    }
}

/// C's error inside a failed A driver call.
fn disk(failure: &Failure) -> &RetentionError {
    match &failure.error {
        DriverError::Disk(error) => error,
        DriverError::Insert(error) => panic!("A's {error:?}, not C's"),
    }
}

/// A failed driver call's pending rollback.
fn rollback_required<H>(after: After<H>) -> PendingRollback<H> {
    match after {
        After::RollbackRequired(pending) => pending,
        _ => panic!("the call is not RollbackRequired"),
    }
}

/// B's cursor over the whole table on `view`: (RowId, value) of each
/// returned row, in cursor order. A FlushRequired step flushes and retries.
macro_rules! scan {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $flush:ident, $flusher:ident) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let cancel = Cancellation::default();
        let mut output = [0u8; 64];
        let mut rows = Vec::new();
        let mut cursor = open_cursor_live($view, $table, None, None).unwrap();
        for _ in 0..16 {
            match next_live(
                &mut cursor,
                ReadCall {
                    port: &$reader,
                    owner: $owner,
                    private: $private,
                    residency: &mut residency,
                },
                &mut output,
                &cancel,
            )
            .unwrap()
            {
                ReadStep::Ready(CursorStep::Row(copy)) => {
                    rows.push((copy.version().row.get(), int_of(&output, copy.len())))
                }
                ReadStep::Ready(CursorStep::End) => break,
                ReadStep::FlushRequired { target } => flush_target!($flush, $flusher, target),
            }
        }
        cursor.close().unwrap();
        rows
    }};
}

/// B's point read of `row` on `view`: its value, or None for Missing.
macro_rules! point {
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
            ReadStep::Ready(PointRead::Missing) => {
                assert_eq!(output, [0xA5; 64], "Missing publishes no bytes");
                None
            }
            ReadStep::FlushRequired { .. } => panic!("resident read sources"),
        }
    }};
}

/// One A driver statement inserting `values` (one row each): the rows'
/// observations. The driver finishes the statement.
macro_rules! a_insert {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $view:expr, $table:expr, $private:expr, $values:expr) => {{
        let values: Vec<[ValueRef<'static>; 1]> = $values
            .iter()
            .map(|v: &i64| [ValueRef::Int64(*v)])
            .collect();
        let rows: Vec<&[ValueRef<'_>]> = values.iter().map(|v| &v[..]).collect();
        let mut slots: Vec<Option<RowMutationObservation>> = rows.iter().map(|_| None).collect();
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        match insert_live(&mut call, $view, $table, $private, &rows, &mut slots) {
            Ok(n) => assert_eq!(n, rows.len()),
            Err(error) => panic!("A's statement refused {:?}", error.failure),
        }
        slots
            .into_iter()
            .map(|slot| slot.unwrap())
            .collect::<Vec<_>>()
    }};
}

/// A's Commit through the driver, which must succeed: its q.
macro_rules! a_commit {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $tx:expr, $private:expr) => {{
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        match commit_live(&mut call, $tx, $private) {
            Ok(committed) => committed.q(),
            Err(error) => panic!("A's Commit refused {:?}", error.failure),
        }
    }};
}

/// A's Commit refused for SerializationConflict, then A's full rollback of
/// its `clrs` roots through the driver and its AbortEnd.
macro_rules! a_conflict_abort {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $tx:expr, $private:expr, $clrs:expr) => {{
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        let mut pending = match commit_live(&mut call, $tx, $private) {
            Err(DriveError { failure, after }) => {
                assert!(matches!(
                    disk(&failure).cause,
                    RetentionCause::SerializationConflict
                ));
                rollback_required(after)
            }
            Ok(_) => panic!("conflicting Commit succeeded"),
        };
        {
            let mut rollback = match start_full_rollback_live(&mut call, &mut pending, $private) {
                Ok(rollback) => rollback,
                Err(failure) => panic!("full RollbackStart {failure:?}"),
            };
            for _ in 0..$clrs {
                assert!(matches!(
                    rollback_step_live(&mut rollback),
                    Ok(Stepped::Compensated)
                ));
            }
            assert!(matches!(
                rollback_step_live(&mut rollback),
                Ok(Stepped::Complete)
            ));
            assert!(finish_rollback_live(rollback).is_ok());
        }
        let tx = match pending.rolled_back() {
            Ok(tx) => tx,
            Err(_) => panic!("full rollback incomplete"),
        };
        assert!(finish_aborted_live(&mut call, tx).is_ok());
    }};
}

/// The committed table `object` issued on `view` (T is 6, U is 7).
macro_rules! issue_object {
    ($reader:ident, $owner:ident, $view:ident, $reader_private:ident, $flush:ident, $flusher:ident, $object:expr) => {{
        let mut issued = None;
        for _ in 0..8 {
            match $reader
                .catalog_statement_call($owner, &$view)
                .unwrap()
                .finish_table(
                    ObjectId::new($object).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    $reader_private,
                )
                .unwrap()
            {
                TableStep::Table(table) => {
                    issued = Some(table);
                    break;
                }
                TableStep::FlushRequired { target } => flush_target!($flush, $flusher, target),
                TableStep::Missing => panic!("committed table missing"),
            }
        }
        issued.expect("bounded table issue")
    }};
}

/// A read-only transaction of `mode` on `owner`.
macro_rules! read_only {
    ($writer:expr, $owner:expr, $mode:expr, $private:expr) => {
        $writer
            .begin_transaction(
                $owner,
                TransactionOptions {
                    read_only: true,
                    isolation: $mode,
                },
                $private,
            )
            .unwrap()
    };
}

/// MCI01-V01/V06/V09/V10 (Serializable) and V04 (ReadCommitted): A's row
/// commits after B's old statement was taken. That statement's scan and
/// point read stay unchanged; the next statement keeps the transaction's
/// snapshot (Serializable) or takes the new q (ReadCommitted); a new
/// transaction sees the row. B's transaction reads T through the
/// CheckedTable A's statement issued, so its table read bit comes only from
/// B's read (V09). A read-only Serializable transaction that read T ends
/// Rejected(SerializationConflict) (V06). The Row Commit leaves the
/// Catalog namespace epoch and moves only T's epoch (V10).
fn snapshot_join(id: &str, mode: IsolationLevel) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let initial_epochs = live.catalog_test_epochs();
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let b_owner = &mut b_owner;
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
                    let rows = a_insert!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        view,
                        &table,
                        private,
                        [VALUE]
                    );
                    let row = rows[0].row();
                    let b = read_only!(writer, b_owner, mode, reader_private);
                    let old = writer.begin_statement(b_owner, &b).unwrap();
                    assert_eq!(old.q().unwrap(), 1);
                    assert_eq!(old.test_table_masks(), (0, 0), "B issued no table");
                    assert_eq!(
                        scan!(
                            reader,
                            b_owner,
                            reader_private,
                            &old,
                            &table,
                            flush,
                            flusher
                        ),
                        vec![],
                        "A's Active row is invisible"
                    );
                    assert_eq!(old.test_table_masks(), (1, 0), "B's read registers T");
                    let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                    assert_eq!(q, 2);
                    assert_eq!(
                        scan!(
                            reader,
                            b_owner,
                            reader_private,
                            &old,
                            &table,
                            flush,
                            flusher
                        ),
                        vec![]
                    );
                    assert_eq!(
                        point!(reader, b_owner, reader_private, &old, &table, row),
                        None
                    );
                    assert_eq!(old.q().unwrap(), 1);
                    assert!(writer.finish_statement(b_owner, old).is_ok());
                    let next = writer.begin_statement(b_owner, &b).unwrap();
                    let expected = if mode == IsolationLevel::Serializable {
                        assert_eq!(next.q().unwrap(), 1);
                        vec![]
                    } else {
                        assert_eq!(next.q().unwrap(), 2);
                        vec![(row.get(), VALUE)]
                    };
                    assert_eq!(
                        scan!(
                            reader,
                            b_owner,
                            reader_private,
                            &next,
                            &table,
                            flush,
                            flusher
                        ),
                        expected
                    );
                    assert!(writer.finish_statement(b_owner, next).is_ok());
                    let end = writer.finish_read_only(b_owner, b);
                    if mode == IsolationLevel::Serializable {
                        assert!(matches!(
                            end,
                            Err(TransactionFinishError::Rejected(ref error))
                                if matches!(error.cause, RetentionCause::SerializationConflict)
                        ));
                    } else {
                        assert!(end.is_ok());
                    }
                    let c = read_only!(
                        writer,
                        b_owner,
                        IsolationLevel::Serializable,
                        reader_private
                    );
                    let new = writer.begin_statement(b_owner, &c).unwrap();
                    assert_eq!(new.q().unwrap(), 2);
                    assert_eq!(
                        scan!(
                            reader,
                            b_owner,
                            reader_private,
                            &new,
                            &table,
                            flush,
                            flusher
                        ),
                        vec![(row.get(), VALUE)]
                    );
                    assert_eq!(
                        point!(reader, b_owner, reader_private, &new, &table, row),
                        Some(VALUE)
                    );
                    assert!(writer.finish_statement(b_owner, new).is_ok());
                    assert!(writer.finish_read_only(b_owner, c).is_ok());
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    b_owner.check_idle().unwrap();
                    owner.check_idle().unwrap();
                }
                let epochs = live.catalog_test_epochs();
                assert_eq!(
                    epochs.1, initial_epochs.1,
                    "Row Commit keeps the Catalog epoch"
                );
                assert_eq!(epochs.2, vec![(2, 6, 2)], "T's epoch is the Commit q");
                assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                println!("MCI01_SNAPSHOT {id} mode={mode:?} epochs={epochs:?}");
            }
        );
    });
}
#[test]
fn mci01_v01_v06_v09_v10_serializable_old_snapshot() {
    snapshot_join("MCI01-V01", IsolationLevel::Serializable);
}
#[test]
fn mci01_v04_read_committed_statement_snapshot() {
    snapshot_join("MCI01-V04", IsolationLevel::ReadCommitted);
}

/// One row of `value` through A's validate/produce and C's reserve/
/// prepare/apply on `view`, which stays open: its RowId.
macro_rules! put {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident, $flusher:ident, $value:expr) => {{
        macro_rules! accepted {
            ($call:expr) => {
                match $call {
                    Ok(value) => value,
                    Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                        panic!("Row operation {error:?}")
                    }
                }
            };
        }
        loop {
            match $writer
                .preflight_row($owner, &$view, &$table, $private, &mut $flush)
                .unwrap()
            {
                RowPreflightStep::Ready(mut preflight) => {
                    let values = [ValueRef::Int64($value)];
                    let input =
                        validate_insert(&values, preflight.validation_lane().unwrap()).unwrap();
                    let mut reserved = accepted!(preflight.reserve(input));
                    let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                    let prepared = accepted!(reserved.prepare(proposal));
                    break accepted!(prepared.apply()).into_parts().0.row();
                }
                RowPreflightStep::FlushRequired { target } => {
                    flush_target!($flush, $flusher, target)
                }
            }
        }
    }};
}

/// MCI01-V02: B's own cursor on A's statement returns A's first row; A's
/// next applied row moves the statement revision and the same cursor's
/// next call is CursorInvalidated. A new cursor on that statement returns
/// both own rows; the Commit makes them visible to a new transaction.
#[test]
fn mci01_v02_own_cursor_is_invalidated_by_a_row_apply() {
    case("MCI01-V02", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
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
                let first = put!(writer, owner, view, table, private, flush, flusher, VALUE);
                let revision = view.revision();
                let cancel = Cancellation::default();
                let mut residency = Plan4ResidencyCursor::new();
                let mut output = [0u8; 64];
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                match next_live(
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
                .unwrap()
                {
                    ReadStep::Ready(CursorStep::Row(copy)) => {
                        assert_eq!(copy.version().row, first);
                        assert_eq!(int_of(&output, copy.len()), VALUE);
                    }
                    _ => panic!("own first row"),
                }
                let second = put!(
                    writer,
                    owner,
                    view,
                    table,
                    private,
                    flush,
                    flusher,
                    VALUE + 1
                );
                assert_eq!(
                    view.revision(),
                    revision + 1,
                    "C's apply moves the revision"
                );
                let marker = [0x5A; 64];
                output = marker;
                let error = next_live(
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
                .err()
                .unwrap();
                assert!(
                    matches!(
                        error,
                        RowReadError::Read(crate::storage::row::read::ReadError::CursorInvalidated)
                    ),
                    "{error:?}"
                );
                assert_eq!(output, marker, "an invalidated cursor publishes nothing");
                cursor.close().unwrap();
                assert_eq!(
                    scan!(reader, owner, reader_private, &view, &table, flush, flusher),
                    vec![(first.get(), VALUE), (second.get(), VALUE + 1)]
                );
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &b).unwrap();
                assert_eq!(new.q().unwrap(), q);
                assert_eq!(
                    scan!(
                        reader,
                        b_owner,
                        reader_private,
                        &new,
                        &table,
                        flush,
                        flusher
                    ),
                    vec![(first.get(), VALUE), (second.get(), VALUE + 1)]
                );
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, b).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
                println!("MCI01_V02 rows={first:?},{second:?} revision={revision} q={q}");
            }
        );
    });
}

/// MCI01-V03: two Serializable writers each read T empty through B and
/// insert one row through A. The first Commit succeeds; the second is
/// RollbackRequired(SerializationConflict) and A's driver fully aborts it.
/// A new reader sees the first writer's row only.
#[test]
fn mci01_v03_two_serializable_writers_of_one_absence() {
    case("MCI01-V03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let other_status = system_init(live, owner, flusher, false);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let x = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let y = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let xv = writer.begin_statement(owner, &x).unwrap();
                let table = issue_table!(reader, owner, xv, reader_private, flush, flusher);
                assert_eq!(
                    scan!(reader, owner, reader_private, &xv, &table, flush, flusher),
                    vec![]
                );
                let x_rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    xv,
                    &table,
                    private,
                    [VALUE]
                );
                let yv = writer.begin_statement(owner, &y).unwrap();
                assert_eq!(
                    scan!(reader, owner, reader_private, &yv, &table, flush, flusher),
                    vec![],
                    "X's Active row is invisible to Y"
                );
                assert_eq!(yv.test_table_masks().0, 1);
                let y_rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    yv,
                    &table,
                    private,
                    [VALUE + 1]
                );
                assert_ne!(x_rows[0].row(), y_rows[0].row());
                assert_eq!(a_commit!(&mut writer, owner, flush, flusher, x, private), 2);
                a_conflict_abort!(&mut writer, owner, flush, flusher, y, private, 1);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &b).unwrap();
                assert_eq!(
                    scan!(
                        reader,
                        b_owner,
                        reader_private,
                        &new,
                        &table,
                        flush,
                        flusher
                    ),
                    vec![(x_rows[0].row().get(), VALUE)]
                );
                assert_eq!(
                    point!(
                        reader,
                        b_owner,
                        reader_private,
                        &new,
                        &table,
                        y_rows[0].row()
                    ),
                    None
                );
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, b).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                println!(
                    "MCI01_V03 x={:?} y={:?} committed=x aborted=y",
                    x_rows[0].row(),
                    y_rows[0].row()
                );
            }
        );
    });
}

/// MCI01-V05/V08: writer X's first statement (optionally) reads T through
/// B, inserts into U and fails on its second row's input, so A rolls the
/// statement back; its second statement inserts into U. Writer Y inserts
/// into T and commits first. With the read, X's failed statement kept T in
/// its read set and X's Commit is a conflict that A fully aborts (V08);
/// without it the two writers of different tables both commit (V05).
fn failed_statement_reads(id: &str, read: bool) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 20,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let second_undo = system_init(live, owner, flusher, true);
                let catalog_status = system_init(live, owner, flusher, false);
                let other_status = system_init(live, owner, flusher, false);
                prepare_create(live, owner, flusher, &catalog_status, &second_undo);
                {
                    let (mut writer, _, mut flush) = live.split();
                    let tx = writer
                        .begin_catalog_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &catalog_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    writer
                        .catalog_test_forward_in_statement(
                            owner,
                            &view,
                            &second_undo,
                            &table_spec("U"),
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    assert!(writer.finish_statement(owner, view).is_ok());
                    match writer.commit_catalog_transaction(owner, tx, private) {
                        Ok(commit) => assert_eq!(commit.q(), 2),
                        Err(_) => panic!("CREATE U"),
                    }
                }
                system_init(live, owner, flusher, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let x = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &x).unwrap();
                let u = issue_object!(reader, owner, s1, reader_private, flush, flusher, 7);
                let t = (read).then(|| {
                    let t = issue_object!(reader, owner, s1, reader_private, flush, flusher, 6);
                    assert_eq!(
                        scan!(reader, owner, reader_private, &s1, &t, flush, flusher),
                        vec![]
                    );
                    t
                });
                let masks = s1.test_table_masks();
                // The second row's NULL is refused by A's input validation
                // after the first row applied: RollbackRequired.
                let good = [ValueRef::Int64(VALUE)];
                let bad = [ValueRef::Null];
                let input: [&[ValueRef<'_>]; 2] = [&good, &bad];
                let mut slots: [Option<RowMutationObservation>; 2] = [None, None];
                {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let mut pending =
                        match insert_live(&mut call, s1, &u, private, &input, &mut slots) {
                            Err(DriveError { failure, after }) => {
                                assert!(
                                    matches!(failure.error, DriverError::Insert(_)),
                                    "{failure:?}"
                                );
                                rollback_required(after)
                            }
                            Ok(n) => panic!("{n} rows inserted"),
                        };
                    {
                        let mut rollback = match start_statement_rollback_live(
                            &mut call,
                            &x,
                            &mut pending,
                            private,
                        ) {
                            Ok(rollback) => rollback,
                            Err(failure) => panic!("statement RollbackStart {failure:?}"),
                        };
                        assert!(matches!(
                            rollback_step_live(&mut rollback),
                            Ok(Stepped::Compensated)
                        ));
                        assert!(matches!(
                            rollback_step_live(&mut rollback),
                            Ok(Stepped::Complete)
                        ));
                        assert!(finish_rollback_live(rollback).is_ok());
                    }
                    let s1 = match pending.rolled_back() {
                        Ok(view) => view,
                        Err(_) => panic!("statement rollback incomplete"),
                    };
                    assert!(finish_statement_live(&mut call, s1).is_ok());
                }
                let s2 = writer.begin_statement(owner, &x).unwrap();
                assert_eq!(
                    s2.test_table_masks().0,
                    masks.0,
                    "the failed statement's reads stay"
                );
                let kept = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s2,
                    &u,
                    private,
                    [VALUE + 1]
                );
                let y = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let yv = writer.begin_statement(owner, &y).unwrap();
                let t_y = issue_object!(reader, owner, yv, reader_private, flush, flusher, 6);
                let y_rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    yv,
                    &t_y,
                    private,
                    [VALUE + 2]
                );
                assert_eq!(a_commit!(&mut writer, owner, flush, flusher, y, private), 3);
                if read {
                    a_conflict_abort!(&mut writer, owner, flush, flusher, x, private, 1);
                } else {
                    assert_eq!(a_commit!(&mut writer, owner, flush, flusher, x, private), 4);
                }
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &b).unwrap();
                assert_eq!(
                    scan!(reader, b_owner, reader_private, &new, &t_y, flush, flusher),
                    vec![(y_rows[0].row().get(), VALUE + 2)]
                );
                let in_u = scan!(reader, b_owner, reader_private, &new, &u, flush, flusher);
                if read {
                    assert_eq!(in_u, vec![]);
                } else {
                    assert_eq!(in_u, vec![(kept[0].row().get(), VALUE + 1)]);
                }
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, b).is_ok());
                drop(t);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced() && !hook::observe(core).read_gate_closed);
                println!("MCI01_FAILED_READS {id} read={read} masks={masks:?} u={in_u:?}");
            }
        );
    });
}
#[test]
fn mci01_v08_failed_statement_reads_survive() {
    failed_statement_reads("MCI01-V08", true);
}
#[test]
fn mci01_v05_writers_of_different_tables_both_commit() {
    failed_statement_reads("MCI01-V05", false);
}

/// T's CheckedTable issued on a thread's own `view`; the sources are
/// resident in these cases (no flush is available off the main thread).
macro_rules! thread_table {
    ($reader:expr, $owner:expr, $view:expr, $private:expr) => {
        match $reader
            .catalog_statement_call($owner, $view)
            .unwrap()
            .finish_table(
                ObjectId::new(6).unwrap(),
                &mut Plan4ResidencyCursor::new(),
                $private,
            )
            .unwrap()
        {
            TableStep::Table(table) => table,
            _ => panic!("resident committed T"),
        }
    };
}

/// A thread's point read of `row`: Ok(value or None) or B's error.
fn thread_point<'a, 'c, 's, W: WriterNamespace>(
    reader: &crate::storage::disk::ReadPort<'_, 'a, 's, 'c, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    private: &mut [u8; PRIVATE_BYTES],
    view: &crate::storage::disk::RowStatementView<'_, '_, 'a, 'c, 's>,
    table: &crate::storage::disk::CheckedTable<'c, 's>,
    row: RowId,
) -> std::result::Result<Option<i64>, RowReadError> {
    let mut residency = Plan4ResidencyCursor::new();
    let mut output = [0xA5; 64];
    match read_point_live(
        ReadCall {
            port: reader,
            owner,
            private,
            residency: &mut residency,
        },
        view,
        table,
        row,
        &mut output,
        &Cancellation::default(),
    )? {
        ReadStep::Ready(PointRead::Found(copy)) => Ok(Some(int_of(&output, copy.len()))),
        ReadStep::Ready(PointRead::Missing) => {
            assert_eq!(output, [0xA5; 64]);
            Ok(None)
        }
        ReadStep::FlushRequired { .. } => panic!("resident read sources"),
    }
}

/// B's refusal while a progress-class writer holds the terminal read gate.
fn installation_pending(error: &RowReadError) -> bool {
    matches!(
        error,
        RowReadError::Source(RetentionError {
            cause: RetentionCause::Concurrency(error),
            effect: Effect::Unchanged,
            ..
        }) if error.cause == crate::transaction::concurrency::Cause::InstallationPending
    )
}

/// The A step a MCI03 barrier holds back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Barrier {
    /// V01: A's statement rollback (its CLR) of the row B has captured.
    Rollback,
    /// V02: A's next row, whose preflight starts while B's read is admitted.
    Preflight,
}

/// MCI03-V01/V02 under the S2-0 terminal read gate (consumers
/// `#index-negative-input` amendment; boundaries `#row-join-s7`). B1's Row
/// read of A's Active row 1 is paused after its source capture, holding the
/// row's Txn/U pins. A's next progress step — the statement rollback (V01)
/// or the next row's preflight (V02) — closes the gate and waits for B1;
/// it is neither Busy nor run beside B1's pins. A late reader B2 is
/// InstallationPending meanwhile. Once B1 returns, A's step runs; B2's
/// retry then reads the row by the original snapshot rules.
fn gate_barrier(id: &str, barrier: Barrier) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let a = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &a).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let _ = settle!(writer, owner, s1, table, private, flush, flusher);
                let good = [ValueRef::Int64(VALUE)];
                let bad = [ValueRef::Null];
                let mut slots: [Option<RowMutationObservation>; 2] = [None, None];
                let (mut pending, mut s2) = match barrier {
                    Barrier::Rollback => {
                        let input: [&[ValueRef<'_>]; 2] = [&good, &bad];
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        match insert_live(&mut call, s1, &table, private, &input, &mut slots) {
                            Err(DriveError { after, .. }) => (Some(rollback_required(after)), None),
                            Ok(n) => panic!("{n} rows inserted"),
                        }
                    }
                    Barrier::Preflight => {
                        let input: [&[ValueRef<'_>]; 1] = [&good];
                        {
                            let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                            assert!(matches!(
                                insert_live(
                                    &mut call,
                                    s1,
                                    &table,
                                    private,
                                    &input,
                                    &mut slots[..1]
                                ),
                                Ok(1)
                            ));
                        }
                        (None, Some(writer.begin_statement(owner, &a).unwrap()))
                    }
                };
                let row = slots[0].as_ref().unwrap().row();
                let mut b1_private = boxed::<PRIVATE_BYTES>();
                let mut b2_private = boxed::<PRIVATE_BYTES>();
                let cell = Mutex::new(&mut writer);
                let used = reader.catalog_test_source_used();
                let mut second = None;
                std::thread::scope(|scope| {
                    let (cell, reader) = (&cell, &reader);
                    let (b1_in, b1_captured) = sync_channel(1);
                    let (b1_go, b1_release) = sync_channel(1);
                    let (gate_in, gate_waiting) = sync_channel(1);
                    let (gate_go, gate_release) = sync_channel(1);
                    let (ready_tx, ready) = sync_channel(1);
                    let (done_tx, done) = sync_channel::<()>(1);
                    // B2 issues its table before B1's read is admitted: a
                    // second admitted read source does not fit beside B1's.
                    let b2_private = &mut *b2_private;
                    let b2 = scope.spawn(move || {
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::ReadCommitted,
                            b2_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        let table = thread_table!(reader, &mut o, &view, b2_private);
                        ready_tx.send(()).unwrap();
                        gate_waiting.recv_timeout(WAIT).unwrap();
                        let at_wait = hook::observe(core);
                        assert_eq!(at_wait.active_read_operations, 1);
                        assert!(at_wait.read_gate_closed && !at_wait.fenced);
                        let late = thread_point(reader, &mut o, b2_private, &view, &table, row)
                            .err()
                            .unwrap();
                        assert!(installation_pending(&late), "{late:?}");
                        gate_go.send(Resume::Continue).unwrap();
                        // A's step keeps waiting while B1 is admitted: no
                        // WAL, the gate closed, B1's operation still counted.
                        let wal = files(state).0;
                        std::thread::sleep(Duration::from_millis(200));
                        assert_eq!(files(state).0, wal, "A's step waits for B1");
                        let still = hook::observe(core);
                        assert!(still.read_gate_closed && still.active_read_operations == 1);
                        b1_go.send(Resume::Continue).unwrap();
                        done.recv_timeout(WAIT).unwrap();
                        // The retry after A's step: the row is A's Active
                        // (V02) or compensated (V01) one, Missing either way.
                        let retry = thread_point(reader, &mut o, b2_private, &view, &table, row);
                        assert!(matches!(retry, Ok(None)), "{retry:?}");
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        assert!(w.finish_read_only(&mut o, tx).is_ok());
                        o.check_idle().unwrap();
                        at_wait
                    });
                    ready.recv_timeout(WAIT).unwrap();
                    hook::install(core, Point::RowReadCaptured, b1_in, b1_release);
                    let b1_private = &mut *b1_private;
                    let b1 = scope.spawn(move || {
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::ReadCommitted,
                            b1_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        let table = thread_table!(reader, &mut o, &view, b1_private);
                        // Paused at RowReadCaptured; A's row is Active.
                        let read = thread_point(reader, &mut o, b1_private, &view, &table, row);
                        assert!(matches!(read, Ok(None)), "{read:?}");
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        assert!(w.finish_read_only(&mut o, tx).is_ok());
                        o.check_idle().unwrap();
                    });
                    b1_captured.recv_timeout(WAIT).unwrap();
                    let at_capture = hook::observe(core);
                    assert_eq!(at_capture.active_read_operations, 1);
                    assert!(!at_capture.read_gate_closed);
                    let pins = reader.catalog_test_references();
                    assert_ne!(pins.0, (0, 0, 0), "B1 holds the captured row's pins");
                    hook::install(core, Point::ReadGateWaiting, gate_in, gate_release);
                    {
                        let mut w = cell.lock().unwrap();
                        let mut call = LiveCall::new(&mut **w, owner, &mut flush, flusher);
                        match barrier {
                            Barrier::Rollback => {
                                let mut pending = pending.take().unwrap();
                                {
                                    let mut rollback = match start_statement_rollback_live(
                                        &mut call,
                                        &a,
                                        &mut pending,
                                        private,
                                    ) {
                                        Ok(rollback) => rollback,
                                        Err(failure) => {
                                            panic!("statement RollbackStart {failure:?}")
                                        }
                                    };
                                    assert!(matches!(
                                        rollback_step_live(&mut rollback),
                                        Ok(Stepped::Compensated)
                                    ));
                                    assert!(matches!(
                                        rollback_step_live(&mut rollback),
                                        Ok(Stepped::Complete)
                                    ));
                                    assert!(finish_rollback_live(rollback).is_ok());
                                }
                                let s1 = match pending.rolled_back() {
                                    Ok(view) => view,
                                    Err(_) => panic!("statement rollback incomplete"),
                                };
                                assert!(finish_statement_live(&mut call, s1).is_ok());
                            }
                            Barrier::Preflight => {
                                let next = [ValueRef::Int64(VALUE + 1)];
                                let input: [&[ValueRef<'_>]; 1] = [&next];
                                let mut slot = [None];
                                match insert_live(
                                    &mut call,
                                    s2.take().unwrap(),
                                    &table,
                                    private,
                                    &input,
                                    &mut slot,
                                ) {
                                    Ok(1) => second = Some(slot[0].take().unwrap().row()),
                                    Ok(n) => panic!("{n} rows"),
                                    Err(error) => panic!("A's next row {:?}", error.failure),
                                }
                            }
                        }
                        assert!(!hook::observe(core).read_gate_closed);
                    }
                    done_tx.send(()).unwrap();
                    b1.join().unwrap();
                    let at_wait = b2.join().unwrap();
                    println!(
                        "MCI03_GATE {id} barrier={barrier:?} pins={pins:?} at_wait={at_wait:?}"
                    );
                });
                drop(cell);
                assert!(pending.is_none() && s2.is_none());
                drop((pending, s2));
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &b).unwrap();
                let expected = match second {
                    Some(second) => vec![(row.get(), VALUE), (second.get(), VALUE + 1)],
                    None => vec![],
                };
                assert_eq!(
                    scan!(
                        reader,
                        b_owner,
                        reader_private,
                        &new,
                        &table,
                        flush,
                        flusher
                    ),
                    expected
                );
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, b).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
                println!("MCI03_GATE {id} q={q} rows={expected:?}");
            }
        );
    });
}
#[test]
fn mci03_v01_rollback_waits_for_the_captured_read() {
    gate_barrier("MCI03-V01", Barrier::Rollback);
}
#[test]
fn mci03_v02_preflight_waits_for_the_admitted_read() {
    gate_barrier("MCI03-V02", Barrier::Preflight);
}

/// MCI03-V03: A's Commit stops after its WAL is durable and its Status is
/// installed, before Q is published (BeforePublish). B's statement from
/// before the Commit reads A's row Missing there and after the Commit
/// returns; a new transaction then reads it.
#[test]
fn mci03_v03_old_snapshot_before_q_publication() {
    case("MCI03-V03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let a = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &a).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE]
                );
                let row = rows[0].row();
                let mut b_private = boxed::<PRIVATE_BYTES>();
                let cell = Mutex::new(&mut writer);
                let mut q = 0;
                std::thread::scope(|scope| {
                    let (cell, reader) = (&cell, &reader);
                    let (publish_in, publishing) = sync_channel(1);
                    let (publish_go, publish_release) = sync_channel(1);
                    hook::install(core, Point::BeforePublish, publish_in, publish_release);
                    let (ready_tx, ready) = sync_channel(1);
                    let (done_tx, done) = sync_channel::<()>(1);
                    let b_private = &mut *b_private;
                    let wal = files(state).0;
                    let b = scope.spawn(move || {
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::Serializable,
                            b_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        assert_eq!(view.q().unwrap(), 1);
                        let table = thread_table!(reader, &mut o, &view, b_private);
                        ready_tx.send(()).unwrap();
                        publishing.recv_timeout(WAIT).unwrap();
                        let at_publish = hook::observe(core);
                        assert!(files(state).0 > wal, "the Commit frame is durable");
                        let during = thread_point(reader, &mut o, b_private, &view, &table, row);
                        publish_go.send(Resume::Continue).unwrap();
                        done.recv_timeout(WAIT).unwrap();
                        let after = thread_point(reader, &mut o, b_private, &view, &table, row);
                        assert!(matches!(after, Ok(None)), "{after:?}");
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        let end = w.finish_read_only(&mut o, tx);
                        assert!(matches!(
                            end,
                            Err(TransactionFinishError::Rejected(ref error))
                                if matches!(error.cause, RetentionCause::SerializationConflict)
                        ));
                        o.check_idle().unwrap();
                        (at_publish, during)
                    });
                    ready.recv_timeout(WAIT).unwrap();
                    {
                        let mut w = cell.lock().unwrap();
                        q = a_commit!(&mut **w, owner, flush, flusher, a, private);
                    }
                    done_tx.send(()).unwrap();
                    let (at_publish, during) = b.join().unwrap();
                    println!("MCI03_V03 at_publish={at_publish:?} during={during:?}");
                    assert!(matches!(during, Ok(None)), "{during:?}");
                });
                drop(cell);
                assert_eq!(q, 2);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &b).unwrap();
                assert_eq!(new.q().unwrap(), 2);
                assert_eq!(
                    point!(reader, b_owner, reader_private, &new, &table, row),
                    Some(VALUE)
                );
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, b).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// MCI03-V04: A's Commit frame is staged and its WAL write fails after a
/// prefix, so C fences the session (S3-R06's fault). B's statement taken
/// before the fault and A's next statement are both refused afterwards,
/// with no file change and the fatal Commit's accounting kept for cleanup.
#[test]
fn mci03_v04_poststage_fault_fences_a_and_b() {
    case("MCI03-V04", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let other_status = system_init(live, owner, flusher, false);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let a = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &a).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE]
                );
                let row = rows[0].row();
                let next = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let next_view = writer.begin_statement(owner, &next).unwrap();
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::ReadCommitted,
                    reader_private
                );
                let b_view = writer.begin_statement(b_owner, &b).unwrap();
                assert_eq!(
                    point!(reader, b_owner, reader_private, &b_view, &table, row),
                    None
                );
                let at = files(state).0;
                {
                    let mut state = state.lock().unwrap();
                    state.events.clear();
                    state.fault_hits = 0;
                    state.fault = Some(port::Fault {
                        key: port::Key::new("write", "d3-wal-00000001", at as u64, 1),
                        action: port::Action::PrefixErrorBytes(96),
                    });
                }
                {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match commit_live(&mut call, a, private) {
                        Err(DriveError { failure, after }) => {
                            assert!(failure.must_resolve, "{failure:?}");
                            assert!(matches!(after, After::MustResolve(_)));
                        }
                        Ok(_) => panic!("a failed Commit write was acknowledged"),
                    }
                }
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().fault_hits, 1);
                let used = reader.catalog_test_source_used();
                assert!(used > 0, "the fatal Commit keeps its accounting");
                let after_fault = files(state);
                let mut residency = Plan4ResidencyCursor::new();
                let mut output = [0xC3; 64];
                let read = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner: b_owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &b_view,
                    &table,
                    row,
                    &mut output,
                    &Cancellation::default(),
                );
                assert!(read.is_err(), "B read in a fenced session");
                assert_eq!(output, [0xC3; 64]);
                let more = [ValueRef::Int64(VALUE + 1)];
                let input: [&[ValueRef<'_>]; 1] = [&more];
                let mut slot = [None];
                {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    assert!(
                        insert_live(&mut call, next_view, &table, private, &input, &mut slot)
                            .is_err(),
                        "A write in a fenced session"
                    );
                }
                assert!(slot[0].is_none());
                assert!(files(state) == after_fault, "refusals change no file");
                assert_eq!(reader.catalog_test_source_used(), used);
                println!("MCI03_V04 fault_at={at} used={used} read={:?}", read.err());
                drop(b_view);
                drop(b);
                drop(next);
            }
        );
    });
}

/// The Catalog epoch view of a session (`catalog_test_epochs`): active,
/// the namespace epoch and each table's (state, object, last change q).
type Epochs = (bool, u64, Vec<(u8, u64, u64)>);

/// One restart's join result (`join_restart`).
struct Joined {
    /// C's Progress actions to READY.
    steps: usize,
    /// A's recovered CLRs to READY.
    clrs: usize,
    /// B's cursor rows (RowId, value) after READY.
    rows: Vec<(u64, i64)>,
    /// A's new row and its Commit q, when the restart ACKs one.
    acked: Option<(u64, u64)>,
    /// The published Q and the epochs at the READY handoff.
    ready: (u64, Epochs),
    /// The epochs after B's read and A's Commit (if any).
    after: Epochs,
}

/// One recovery process over `directory`: C's recovery under `policy` to
/// READY (A's handler for each RowClr), B's cursor over T, then, with
/// `ack`, A's new row of `ack` and its Commit. The memory MCI01-V11 case
/// and the native MCI02 worker share it.
fn join_restart<D: DirectoryPort>(
    directory: &D,
    policy: RowRecoveryPolicy,
    ack: Option<i64>,
) -> Joined {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let acquired = match Plan4Recovery::acquire(
        directory,
        Some(super::super::support::namespace()),
        &mut maintenance,
        &mut dwb,
        &mut slot,
    ) {
        Ok(owner) => owner,
        Err(Plan4RecoveryAcquireFailure::Acquired(_))
        | Err(Plan4RecoveryAcquireFailure::Before { .. }) => panic!("acquire"),
    };
    let runtime = acquired.recover_with_rows(policy).ok().unwrap();
    let core = PageConcurrency::new(runtime.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 16];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let mut closed = runtime
        .bind(&owner, &core, &mut frames, &mut operation)
        .unwrap();
    let (mut steps, mut clrs, mut ready) = (0, 0, false);
    for _ in 0..512 {
        match closed.next_action(&mut owner, &mut flusher).unwrap() {
            RecoveryAction::Ready => {
                ready = true;
                break;
            }
            RecoveryAction::Progress(progress) => {
                assert!(!progress.ready);
                steps += 1;
            }
            RecoveryAction::RowClr(task) => {
                let actor = task.actor();
                match handle_recovered_clr(task) {
                    Ok(progress) => assert!(progress.progressed && !progress.ready),
                    Err(_) => panic!("A's recovered CLR for {actor:?}"),
                }
                clrs += 1;
            }
        }
    }
    assert!(ready, "bounded Row recovery exhausted");
    let (mut live, private) = match closed.into_live_with_private() {
        Ok(value) => value,
        Err(_) => panic!("original READY handoff"),
    };
    let at_ready = (live.catalog_test_q(), live.catalog_test_epochs());
    let row_status = live.recovery_test_expected_target(11).unwrap();
    let first_jobs = crate::storage::retention_page::JobSlotRef::new(
        page_ref(2, 1),
        0,
        foundation::identity::Generation::new(1).unwrap(),
    )
    .unwrap();
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..16 {
        match live
            .prepare_empty_targets(&owner, &row_status, first_jobs, &mut cursor)
            .unwrap()
        {
            None => break,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(&mut live, &mut flusher, Some(&target))
            }
            Some(_) => panic!("unexpected Begin preparation"),
        }
    }
    prepare_catalog(&mut live, &owner, &mut flusher);
    if ack.is_some() {
        system_init(&mut live, &mut owner, &mut flusher, true);
    }
    let mut reader_private = boxed::<PRIVATE_BYTES>();
    let reader_private = &mut *reader_private;
    let owner = &mut owner;
    let flusher = &mut flusher;
    let rows;
    let mut acked = None;
    {
        let (mut writer, reader, mut flush) = live.split();
        let b = read_only!(writer, owner, IsolationLevel::Serializable, reader_private);
        let view = writer.begin_statement(owner, &b).unwrap();
        let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
        rows = scan!(reader, owner, reader_private, &view, &table, flush, flusher);
        assert!(writer.finish_statement(owner, view).is_ok());
        assert!(writer.finish_read_only(owner, b).is_ok());
        if let Some(value) = ack {
            let a = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let s = writer.begin_statement(owner, &a).unwrap();
            let table = issue_table!(reader, owner, s, reader_private, flush, flusher);
            let new = a_insert!(
                &mut writer,
                owner,
                flush,
                flusher,
                s,
                &table,
                private,
                [value]
            );
            let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
            acked = Some((new[0].row().get(), q));
        }
        assert_eq!(reader.catalog_test_source_used(), 0);
        owner.check_idle().unwrap();
    }
    let after = live.catalog_test_epochs();
    assert!(!runtime.session_fence().is_fenced());
    drop(live);
    Joined {
        steps,
        clrs,
        rows,
        acked,
        ready: at_ready,
        after,
    }
}

/// The logical-epoch oracle of a restart whose history's last ACKed Commit
/// is `q` (MCI01-V11): READY's exclusive activation seeds the namespace
/// epoch and T's epoch with the published Q (task-10
/// `#module-row-isolation`), and that Q is still `q` after every physical
/// cleanup the recovery ran (Prune, ReleaseRole, RootFree, TerminalFinish,
/// and a loser's CLRs and AbortEnd): none of them publishes a q. A later Row
/// Commit at `q'` moves only T's epoch (V10).
fn check_epochs(joined: &Joined, q: u64) {
    assert_eq!(joined.ready, (q, (true, q, vec![(2, 6, q)])));
    let t = joined.acked.map_or(q, |(_, acked)| acked);
    assert_eq!(joined.after, (true, q, vec![(2, 6, t)]));
}

/// MCI01-V11 (memory): the physical cleanup of a committed Row actor does
/// not advance the logical epoch. A commits one row of T at q; the durable
/// files are restarted under WithRowClr. Before READY the recovery runs the
/// root's Prune, three ReleaseRole, RootFree and TerminalFinish (S6R-M01's
/// trace; here only counted), each with its own WAL record. The READY Q and
/// the seeded epochs are still q, B reads A's row, and A's next Commit at
/// q + 1 moves only T's epoch. Live Row retirement under foreground readers
/// is not supplied (boundaries `#row-recovery-s6`), so the recovery is the
/// only physical Row cleanup there is to run.
#[test]
fn mci01_v11_recovered_cleanup_keeps_logical_epochs() {
    case_within("MCI01-V11", Duration::from_secs(240), || {
        let oracle = Oracle::new();
        let (images, q, row) = with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (q, row) = {
                    let (mut writer, reader, mut flush) = live.split();
                    let a = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let s = writer.begin_statement(owner, &a).unwrap();
                    let table = issue_table!(reader, owner, s, reader_private, flush, flusher);
                    let rows = a_insert!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        s,
                        &table,
                        private,
                        [VALUE]
                    );
                    let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                    (q, rows[0].row().get())
                };
                // The live session before the crash: T's epoch is the Commit q.
                let before = live.catalog_test_epochs();
                assert_eq!(before.2, vec![(2, 6, q)]);
                assert!(!fence.is_fenced());
                println!("MCI01_V11 committed q={q} row={row} epochs={before:?}");
                (state.lock().unwrap().images(), q, row)
            }
        );
        let directory = clone_directory(&images);
        let joined = join_restart(&directory, RowRecoveryPolicy::WithRowClr, Some(VALUE + 10));
        println!(
            "MCI01_V11 steps={} clrs={} rows={:?} acked={:?} ready={:?} after={:?}",
            joined.steps, joined.clrs, joined.rows, joined.acked, joined.ready, joined.after
        );
        assert_eq!(joined.clrs, 0);
        assert!(joined.steps >= 6, "the committed cleanup ran before READY");
        assert_eq!(joined.rows, vec![(row, VALUE)]);
        let (new, acked) = joined.acked.unwrap();
        assert!(new > row);
        assert_eq!(acked, q + 1);
        check_epochs(&joined, q);
    });
}

#[cfg(target_os = "linux")]
#[path = "row_join/native.rs"]
mod native;
