//! RUP-B06/B07 (dev3-row-update.md#read, #tests): B's reads of an updated
//! row over the actual C supply. An explicit transaction INSERTs 102 and
//! then updates 100 to 200 (the new row Inline `Transaction(T)` and its
//! Live RowValue U on the shared pool's resident pages); T is Active, so a
//! foreign statement reads U's old value.
//! - B07, point reads after one edit of the resident U page (RDL-B07's
//!   harness, `delete_read.rs`): the U re-encoded unchanged is the control
//!   (100); a Releasing U (as UpdateClr and UpdatePrune leave it) and a
//!   RowValue before stamped by its creator's `Transaction` are Fatal at
//!   `missing_before`'s outline table (Census, Invariant), the system
//!   fenced and no output written; a RowDeleted before is the typed
//!   Unsupported, Unchanged and not fenced.
//! - B07, C's read steps called directly: `value_before` on the Inline
//!   candidate before `missing_before` reported its value is Unchanged
//!   WrongState; after the report it reads the old value into its own
//!   copy range, so `copy_candidate` still gives the new value and
//!   `copy_before` the old one, in either order. A U made Releasing after
//!   the report is Fatal at `value_before` (Census, Undo).
//! - B06: an UpdateClr waits at the terminal read gate for a read that has
//!   captured the new row (RDL-B06's order).
use super::super::delete::read::{expectation, replace_undo};
use super::*;
use crate::storage::{
    dirty_frame::status_load_test_support,
    disk::RetentionPhase,
    page::{PageKind, decode_page},
    row::read::driver::{RowReadError, live::read_point_live},
    row_record::RowSchemaInput,
    transaction_record::{LogicalRowVersion, TransactionState, TxnRef, VersionStamp},
    undo_record::{self, UndoBeforeKind, UndoCell, UndoEdge, UndoPayload},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    /// Control: the U re-encoded unchanged.
    SameU,
    /// As UpdateClr and UpdatePrune leave it: Releasing, its value dropped.
    ReleasingU,
    /// The RowValue before stamped `Transaction` by its own creator.
    CreatorStampU,
    /// A RowDeleted before (which UPDATE never writes).
    DeletedU,
}

/// The Live update U of `reference` (row `row`) rewritten per `edit` on
/// the resident pool page.
fn edit_u(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    reference: crate::storage::undo_ref::UndoRef,
    row: u64,
    edit: Edit,
) {
    let spec = table_spec("T");
    let schema = RowSchemaInput::Columns(&spec.columns);
    let expected = expectation(reference, row);
    let index = (1..28)
        .find(|index| {
            pool.target_request(*index)
                .is_ok_and(|request| request.identity.page.page_no == reference.page().page_no)
        })
        .expect("the U page resident");
    let identity = pool.target_request(index).unwrap().identity;
    status_load_test_support::edit_content(pool, index, |bytes| {
        let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
        let slot = usize::from(reference.slot());
        let live = undo_record::decode_undo_record(page.cell(slot).unwrap(), expected, &schema)
            .unwrap()
            .live_before()
            .unwrap();
        assert_eq!(live.before_kind, UndoBeforeKind::RowValue);
        let cell = match edit {
            Edit::SameU => UndoCell::Live(live),
            Edit::ReleasingU => UndoCell::Releasing(undo_record::UndoRecord {
                incoming: 0,
                edges: [
                    match live.edges[0] {
                        UndoEdge::Linked(_) => UndoEdge::Cut,
                        other => other,
                    },
                    UndoEdge::Never,
                ],
                payload: UndoPayload::None,
                ..live
            }),
            // The codec admits it only over a logical of the creator's
            // own earlier mutation (the U is the transaction's second).
            Edit::CreatorStampU => UndoCell::Live(undo_record::UndoRecord {
                before_stamp: Some(VersionStamp::Transaction(
                    TxnRef::new(live.creator.id, live.creator.status_generation).unwrap(),
                )),
                before_logical: Some(
                    LogicalRowVersion::new(live.creator.id, live.mutation_sequence - 1).unwrap(),
                ),
                ..live
            }),
            Edit::DeletedU => UndoCell::Live(undo_record::UndoRecord {
                before_kind: UndoBeforeKind::RowDeleted,
                payload: UndoPayload::None,
                ..live
            }),
        };
        let mut out = vec![0; PAGE_BYTES];
        let length = undo_record::encode_undo_record(cell, expected, &schema, &mut out).unwrap();
        replace_undo(bytes, identity, slot, &out[..length]);
    });
}

/// An explicit transaction's INSERT of 102 and then its UPDATE of the first
/// of two committed and drained rows (100, 101) to 200, so that the U is
/// the transaction's second mutation (`Edit::CreatorStampU` needs an
/// earlier one): the row, the Active holder and the U.
macro_rules! updated {
    ($writer:ident, $owner:ident, $flush:ident, $flusher:ident, $reader:ident, $reader_private:ident, $row_status:ident, $private:ident) => {{
        let (out, slots) = autocommit!(
            &mut $writer,
            $owner,
            $flush,
            $flusher,
            $reader,
            $reader_private,
            &$row_status,
            $private,
            6,
            ints(&[100, 101])
        );
        assert_eq!(committed(out), (2, 2));
        let rows = applied(slots, &[100, 101]);
        assert_eq!(
            drains!(&mut $writer, $owner, $flush, $flusher, $private),
            [drain(11, true)]
        );
        let target = rows[0].0;
        let mut holder = None;
        begun(begin!(
            &mut $writer,
            $owner,
            $flush,
            $flusher,
            &$row_status,
            $private,
            &mut holder
        ));
        let (out, _) = statement!(
            &mut $writer,
            $owner,
            $flush,
            $flusher,
            $reader,
            $reader_private,
            &mut holder,
            $private,
            6,
            ints(&[102])
        );
        assert_eq!(done(out), 1);
        let (out, slot) = update_statement!(
            &mut $writer,
            $owner,
            $flush,
            $flusher,
            $reader,
            $reader_private,
            &mut holder,
            $private,
            6,
            target,
            int(200)
        );
        assert_eq!(done(out), 1);
        let reference = slot.expect("updated row").undo();
        (target, holder, reference)
    }};
}

/// One world: rows 100 and 101 committed and pruned, then an explicit
/// transaction's INSERT of 102 and UPDATE of the first to 200 (Active);
/// `edit` rewrites its U; a foreign ReadCommitted point read of the row:
/// its value or the error, whether the system is fenced, and whether the
/// output was left as it was.
fn edited_read(edit: Edit) -> (std::result::Result<i64, RowReadError>, bool, bool) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 28,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = state;
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            for _ in 1..3 {
                system_init(live, owner, flusher, true);
            }
            let (mut writer, reader, mut flush) = live.split();
            let (target, holder, reference) = updated!(
                writer,
                owner,
                flush,
                flusher,
                reader,
                reader_private,
                row_status,
                private
            );
            edit_u(reader.catalog_test_pool(), reference, target.get(), edit);
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
            let table = issue_table!(reader, b, view, reader_private, flush, flusher);
            let mut residency = Plan4ResidencyCursor::new();
            let mut output = [0xA5; 64];
            let result = match read_point_live(
                ReadCall {
                    port: &reader,
                    owner: b,
                    private: reader_private,
                    residency: &mut residency,
                },
                &view,
                &table,
                target,
                &mut output,
                &Cancellation::default(),
            ) {
                Ok(ReadStep::Ready(PointRead::Found(copy))) => {
                    assert_eq!(copy.version().row, target);
                    assert_eq!(
                        copy.version().stamp,
                        VersionStamp::Committed(foundation::identity::CommitSeq::new(2).unwrap())
                    );
                    Ok(int_of(&output, copy.len()))
                }
                Ok(_) => panic!("{edit:?}: the old row is Found"),
                Err(error) => Err(error),
            };
            let untouched = output == [0xA5; 64];
            let fenced = fence.is_fenced();
            drop(view);
            drop(read);
            drop(holder);
            (result, fenced, untouched)
        }
    )
}

#[test]
fn rup_b07_broken_update_states_are_never_a_row() {
    case_within("RUP-B07", std::time::Duration::from_secs(600), || {
        let (control, fenced, _) = edited_read(Edit::SameU);
        assert_eq!((control.unwrap(), fenced), (100, false));
        println!("RUP_B07\tSameU\tread 100");
        for edit in [Edit::ReleasingU, Edit::CreatorStampU] {
            let (result, fenced, untouched) = edited_read(edit);
            let error = result.expect_err("a broken UPDATE state is never a row");
            let RowReadError::Source(error) = &error else {
                panic!("{edit:?}: {error:?}");
            };
            assert_eq!(
                (error.effect, error.phase),
                (Effect::CallerMustResolve, RetentionPhase::Census),
                "{edit:?}"
            );
            assert!(
                matches!(error.cause, RetentionCause::Invariant),
                "{edit:?}: {error:?}"
            );
            assert!(fenced && untouched, "{edit:?}: fenced, no output");
            println!("RUP_B07\t{edit:?}\t{:?}\t{:?}", error.phase, error.cause);
        }
        let (result, fenced, untouched) = edited_read(Edit::DeletedU);
        let error = result.expect_err("a RowDeleted before is Unsupported");
        let RowReadError::Source(error) = &error else {
            panic!("DeletedU: {error:?}");
        };
        assert_eq!(
            (error.effect, error.phase),
            (Effect::Unchanged, RetentionPhase::Census)
        );
        assert!(
            matches!(error.cause, RetentionCause::Unsupported),
            "{error:?}"
        );
        assert!(!fenced && untouched, "DeletedU: not fenced, no output");
        println!("RUP_B07\tDeletedU\t{:?}\t{:?}", error.phase, error.cause);
    });
}

/// The resident read page of `table` and its first capture (the updated
/// row, RowId 1).
macro_rules! first_capture {
    ($op:ident, $table:expr, $residency:expr) => {{
        let page = match $op.page($table, $residency).unwrap() {
            ReadStep::Ready(page) => page,
            _ => panic!("resident"),
        };
        page.capture(0).unwrap()
    }};
}

#[test]
fn rup_b07_value_before_needs_the_report_and_keeps_the_capture() {
    case_within("RUP-B07-steps", std::time::Duration::from_secs(300), || {
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
                let (target, holder, reference) = updated!(
                    writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    row_status,
                    private
                );
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
                let mut residency = Plan4ResidencyCursor::new();
                let mut new = [0xA5; 64];
                let mut old = [0xA5; 64];
                {
                    let mut op = reader.row_operation(b, &view, reader_private).unwrap();
                    let candidate = first_capture!(op, &table, &mut residency);
                    // No report in this capture yet.
                    let early = op.value_before(&candidate, &mut residency).err().unwrap();
                    assert!(
                        matches!(
                            early,
                            RetentionError {
                                effect: Effect::Unchanged,
                                cause: RetentionCause::WrongState,
                                ..
                            }
                        ),
                        "{early:?}"
                    );
                    assert!(matches!(
                        op.status(&candidate, &mut residency).unwrap(),
                        ReadStep::Ready(TransactionState::Active)
                    ));
                    let ReadStep::Ready(missing) =
                        op.missing_before(&candidate, &mut residency).unwrap()
                    else {
                        panic!("resident U")
                    };
                    assert!(missing.value_before());
                    let ReadStep::Ready(before) =
                        op.value_before(&candidate, &mut residency).unwrap()
                    else {
                        panic!("resident U")
                    };
                    let current = op.copy_candidate(&candidate, &mut new).unwrap();
                    let copied = op.copy_before(&before, &mut old).unwrap();
                    let again = op.copy_candidate(&candidate, &mut new).unwrap();
                    assert_eq!(
                        (current.version(), current.len()),
                        (again.version(), again.len())
                    );
                    assert_eq!(current.version().row, target);
                    assert!(matches!(
                        current.version().stamp,
                        VersionStamp::Transaction(_)
                    ));
                    assert_eq!(int_of(&new, current.len()), 200);
                    assert_eq!(
                        copied.version().stamp,
                        VersionStamp::Committed(foundation::identity::CommitSeq::new(2).unwrap())
                    );
                    assert_eq!(int_of(&old, copied.len()), 100);
                    op.release_candidate(candidate).unwrap();
                    op.finish().unwrap();
                }
                assert!(!fence.is_fenced());
                {
                    let mut op = reader.row_operation(b, &view, reader_private).unwrap();
                    let candidate = first_capture!(op, &table, &mut residency);
                    assert!(matches!(
                        op.missing_before(&candidate, &mut residency).unwrap(),
                        ReadStep::Ready(missing) if missing.value_before()
                    ));
                    // A U released under the reader is corruption.
                    edit_u(
                        reader.catalog_test_pool(),
                        reference,
                        target.get(),
                        Edit::ReleasingU,
                    );
                    let error = op.value_before(&candidate, &mut residency).err().unwrap();
                    assert!(
                        matches!(
                            error,
                            RetentionError {
                                effect: Effect::CallerMustResolve,
                                phase: RetentionPhase::Census,
                                cause: RetentionCause::Undo(_)
                            }
                        ),
                        "{error:?}"
                    );
                    assert!(fence.is_fenced());
                    let _ = candidate;
                    drop(op);
                }
                drop(view);
                drop(read);
                drop(holder);
                println!("RUP_B07_STEPS early=WrongState new=200 old=100 released=Undo");
            }
        );
    });
}

/// RUP-B06 (dev3-row-update.md#read, #tests; RDL-B06's order): an
/// UpdateClr waits at the terminal read gate for a read that has captured
/// the new row. Rows 100 and 101 are committed and pruned, then an explicit
/// transaction updates the first to 200 (Active, its root Retained). B1's
/// foreign ReadCommitted read of that row is paused after its capture
/// (`Point::RowReadCaptured`), holding the new row's Txn and U pins. The
/// caller's rollback closes the gate when it starts, before its
/// RollbackStart and UpdateClr, and waits for B1 (`Point::ReadGateWaiting`):
/// the WAL is as it was before the rollback was called, at the wait and
/// while B1 holds its pins, and a late reader B2 is refused Unchanged
/// InstallationPending. When B1 resumes it reads the old value 100 through
/// U's value before; the rollback then writes its groups and ends
/// RolledBack, and B2's retry reads the restored row 100. The drain cleans
/// the compensated root and the Terminal (5 groups) and a new statement
/// reads both rows.
#[test]
fn rup_b06_update_clr_waits_for_a_captured_read() {
    use super::super::super::super::{WAIT, installation_pending, thread_point};
    use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
    use std::{
        sync::{Mutex, mpsc::sync_channel},
        time::Duration,
    };

    case_within("RUP-B06", Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..3 {
                    system_init(live, owner, flusher, true);
                }
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
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let (target, kept) = (rows[0].0, rows[1]);
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
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    target,
                    int(200)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), target);
                let mut b1_private = boxed::<PRIVATE_BYTES>();
                let mut b2_private = boxed::<PRIVATE_BYTES>();
                // The WAL before the rollback is called: B1 and B2 only read,
                // so it stays this until the rollback writes.
                let wal = files(state).0;
                let cell = Mutex::new(&mut writer);
                std::thread::scope(|scope| {
                    let (cell, reader) = (&cell, &reader);
                    let (b1_in, b1_captured) = sync_channel(1);
                    let (b1_go, b1_release) = sync_channel(1);
                    let (gate_in, gate_waiting) = sync_channel(1);
                    let (gate_go, gate_release) = sync_channel(1);
                    let (ready_tx, ready) = sync_channel(1);
                    let (done_tx, done) = sync_channel::<()>(1);
                    // B2 issues its table before B1's read is admitted.
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
                        // The gate closed at the rollback's start: no
                        // RollbackStart or UpdateClr before the wait.
                        assert_eq!(files(state).0, wal, "nothing before the gate wait");
                        let at_wait = hook::observe(core);
                        assert_eq!(at_wait.active_read_operations, 1);
                        assert!(at_wait.read_gate_closed && !at_wait.fenced);
                        let late = thread_point(reader, &mut o, b2_private, &view, &table, target)
                            .err()
                            .unwrap();
                        assert!(installation_pending(&late), "{late:?}");
                        gate_go.send(Resume::Continue).unwrap();
                        // The rollback keeps waiting while B1 is admitted: no
                        // WAL, the gate closed, B1's operation still counted.
                        std::thread::sleep(Duration::from_millis(200));
                        assert_eq!(files(state).0, wal, "the UpdateClr waits for B1");
                        let still = hook::observe(core);
                        assert!(still.read_gate_closed && still.active_read_operations == 1);
                        b1_go.send(Resume::Continue).unwrap();
                        done.recv_timeout(WAIT).unwrap();
                        assert!(files(state).0 != wal, "the rollback wrote after B1");
                        // The retry after the rollback: the restored row.
                        let retry = thread_point(reader, &mut o, b2_private, &view, &table, target);
                        assert!(matches!(retry, Ok(Some(100))), "{retry:?}");
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
                        // Paused at RowReadCaptured; the updater is Active,
                        // so the read is U's value before.
                        let read = thread_point(reader, &mut o, b1_private, &view, &table, target);
                        assert!(matches!(read, Ok(Some(100))), "{read:?}");
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
                    assert_ne!(pins.0, (0, 0, 0), "B1 holds the new row's pins");
                    hook::install(core, Point::ReadGateWaiting, gate_in, gate_release);
                    {
                        let mut w = cell.lock().unwrap();
                        let out = rollback!(&mut **w, owner, flush, flusher, &mut holder, private);
                        assert!(
                            matches!(out, Explicit::RolledBack),
                            "expected RolledBack, got {out:?}"
                        );
                        assert!(!hook::observe(core).read_gate_closed);
                    }
                    done_tx.send(()).unwrap();
                    b1.join().unwrap();
                    let at_wait = b2.join().unwrap();
                    println!("RUP_B06 pins={pins:?} at_wait={at_wait:?}");
                });
                assert!(holder.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(5, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        [target, kept.0]
                    ),
                    [Some(100), Some(101)]
                );
                assert!(!fence.is_fenced());
            }
        );
    });
}
