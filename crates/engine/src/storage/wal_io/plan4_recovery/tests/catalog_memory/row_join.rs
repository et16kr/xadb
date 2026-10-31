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
use super::row_forward::VALUE;
use super::row_preflight::{create_table, system_init};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RetentionCause, RetentionError, RowMutationObservation,
        RowOwnedError, RowPreflightStep, TableStep, TransactionFinishError, TransactionOptions,
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
