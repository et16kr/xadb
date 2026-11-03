//! RDL-B07..B09 (dev3-row-delete.md#read, #tests): a read never takes a
//! broken DELETE state as a row. Over the actual C supply an explicit
//! transaction's DELETE is applied (its tombstone and Live U on the shared
//! pool's resident pages); one edit of the resident RowLeaf or U page makes
//! the state one the first path never writes, and a new statement's point
//! read of the row is Fatal (CallerMustResolve, the system fenced), with no
//! output written:
//! - B07: the tombstone's U Releasing (its value dropped, as DeleteClr and
//!   DeletePrune leave it): `value_before`'s `live_before` refuses it
//!   (Undo).
//! - B08: the tombstone restamped `Committed` with no U, which DeletePrune
//!   never leaves: capture refuses it (Source, Invariant).
//! - B09: a Missing (INSERT) U under the tombstone: `tombstone_before`
//!   refuses it (Census, Invariant); and the delete's RowValue U under an
//!   Inline `Transaction` row (the tombstone re-encoded as a row of the same
//!   version): B takes the Missing-before path and `read_missing`'s
//!   schema-less decode refuses the inline RowValue (Census, Undo).
//!
//! Each edit path first runs as a control that re-encodes the page's own
//! cell unchanged: the read then takes the committed row 100 through the
//! tombstone's U, so each refusal comes from the state, not from the edit.
//!
//! `read_missing`'s typed Unsupported (MCB04-V14) is only for a valid
//! RowDeleted or chunked RowValue before, which DELETE never writes.
use super::*;
use crate::storage::{
    dirty_frame::status_load_test_support,
    disk::RetentionPhase,
    page::{PageIdentity, PageKind, decode_page, encode_page},
    row::read::driver::{RowReadError, live::read_point_live},
    row_record::{self, RecordExpectation, RecordVersion, RowPayload, RowSchemaInput},
    transaction_record::VersionStamp,
    tree_page::{self, EntryPayload, TreeExpectation, edit::LeafEdit},
    undo_record::{
        self, UndoBeforeKind, UndoCell, UndoEdge, UndoPayload, UndoRecordExpectation,
        UndoTargetKind, UndoTargetView,
    },
};
use foundation::identity::{CommitSeq, Generation, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Broken {
    /// Control: the U re-encoded unchanged.
    SameU,
    /// Control: the tombstone re-encoded unchanged.
    SameTombstone,
    ReleasingU,
    CommittedTombstone,
    MissingU,
    InlineRow,
}

/// The expectation of the Row U `reference` of `row` in table 6.
fn expectation(
    reference: crate::storage::undo_ref::UndoRef,
    row: u64,
) -> UndoRecordExpectation<'static> {
    UndoRecordExpectation {
        namespace: super::super::super::super::super::support::namespace(),
        reference,
        target: UndoTargetView {
            kind: UndoTargetKind::Row,
            owner: ObjectId::new(6).unwrap(),
            generation: Generation::new(1).unwrap(),
            entity: row,
            arity: 0,
            key: &[],
        },
    }
}

/// Rewrites the Undo page `bytes` with slot `slot` replaced by `cell`.
fn replace_undo(bytes: &mut [u8; PAGE_BYTES], identity: PageIdentity, slot: usize, cell: &[u8]) {
    let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
    let header = page.header();
    let mut cells = (0..page.len())
        .map(|i| page.cell(i).unwrap().to_vec())
        .collect::<Vec<_>>();
    cells[slot] = cell.to_vec();
    let refs = cells.iter().map(Vec::as_slice).collect::<Vec<_>>();
    encode_page(header, &refs, bytes).unwrap();
    decode_page(bytes, identity, PageKind::Undo).unwrap();
}

/// One world: rows 100 and 101 committed and pruned, then an explicit
/// transaction's DELETE of the first (Active, its root Retained); `broken`
/// edits the resident page; the point read of the row: its value, or the
/// error with the system fenced and no output written.
fn broken_read(broken: Broken) -> std::result::Result<i64, RowReadError> {
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
            let target = rows[0].0;
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
            let (out, slot) = delete_statement!(
                &mut writer,
                owner,
                flush,
                flusher,
                reader,
                reader_private,
                &mut holder,
                private,
                6,
                target
            );
            assert_eq!(done(out), 1);
            let reference = slot.expect("deleted row").undo();
            let spec = table_spec("T");
            let schema = RowSchemaInput::Columns(&spec.columns);
            let expected = expectation(reference, target.get());
            let pool = reader.catalog_test_pool();
            // The registered frame of a page (`target_request` only: a
            // probe of a dirty frame would fence the pool).
            let frame_of = |page_no: u64| {
                (1..28)
                    .find(|index| {
                        pool.target_request(*index)
                            .is_ok_and(|request| request.identity.page.page_no == page_no)
                    })
                    .unwrap_or_else(|| panic!("page {page_no} resident"))
            };
            match broken {
                Broken::SameU | Broken::ReleasingU | Broken::MissingU => {
                    let index = frame_of(reference.page().page_no);
                    let identity = pool.target_request(index).unwrap().identity;
                    status_load_test_support::edit_content(pool, index, |bytes| {
                        let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
                        let slot = usize::from(reference.slot());
                        let live = undo_record::decode_undo_record(
                            page.cell(slot).unwrap(),
                            expected,
                            &schema,
                        )
                        .unwrap()
                        .live_before()
                        .unwrap();
                        assert_eq!(live.before_kind, UndoBeforeKind::RowValue);
                        let cell = match broken {
                            Broken::SameU => UndoCell::Live(live),
                            // As DeleteClr leaves it (dev3-row-delete.md#delete-undo).
                            Broken::ReleasingU => UndoCell::Releasing(undo_record::UndoRecord {
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
                            // An INSERT's U under the tombstone.
                            Broken::MissingU => UndoCell::Live(undo_record::UndoRecord {
                                before_kind: UndoBeforeKind::Missing,
                                before_stamp: None,
                                before_logical: None,
                                payload: UndoPayload::None,
                                ..live
                            }),
                            _ => unreachable!(),
                        };
                        let mut out = vec![0; PAGE_BYTES];
                        let length =
                            undo_record::encode_undo_record(cell, expected, &schema, &mut out)
                                .unwrap();
                        replace_undo(bytes, identity, slot, &out[..length]);
                    });
                }
                Broken::SameTombstone | Broken::CommittedTombstone | Broken::InlineRow => {
                    let index = (1..28)
                        .find(|index| {
                            pool.target_request(*index)
                                .is_ok_and(|request| request.kind == PageKind::RowLeaf)
                        })
                        .expect("resident RowLeaf");
                    let request = pool.target_request(index).unwrap();
                    let leaf = TreeExpectation {
                        identity: request.identity,
                        kind: PageKind::RowLeaf,
                        level: 0,
                        arity: 0,
                    };
                    status_load_test_support::edit_content(pool, index, |bytes| {
                        let view = tree_page::decode_tree_page(bytes, leaf).unwrap();
                        let position = view.lower_bound(&target.get().to_be_bytes()).unwrap();
                        assert!(position.equal);
                        let EntryPayload::Value(value) =
                            view.entry(position.index).unwrap().payload
                        else {
                            panic!("value");
                        };
                        let records = RecordExpectation {
                            namespace: expected.namespace,
                            row: target,
                        };
                        let tombstone =
                            row_record::decode_row_record_with_schema(value, records, schema)
                                .unwrap();
                        assert!(matches!(tombstone.payload(), RowPayload::Deleted));
                        let mut canonical = [0; 64];
                        let length = row_record::encode_canonical_row_borrowed(
                            schema,
                            &[ValueRef::Int64(7)],
                            &mut canonical,
                        )
                        .unwrap();
                        let canonical = &canonical[..length];
                        let payload = match broken {
                            Broken::InlineRow => RowPayload::Inline(
                                row_record::decode_canonical_row_with_schema(canonical, schema)
                                    .unwrap(),
                            ),
                            _ => RowPayload::Deleted,
                        };
                        let version = match broken {
                            Broken::SameTombstone | Broken::InlineRow => tombstone.version(),
                            _ => RecordVersion {
                                stamp: VersionStamp::Committed(CommitSeq::new(3).unwrap()),
                                undo: None,
                                ..tombstone.version()
                            },
                        };
                        let mut record = vec![0; PAGE_BYTES];
                        let length = row_record::encode_row_record_with_schema(
                            records,
                            schema,
                            version,
                            payload,
                            &mut record,
                        )
                        .unwrap();
                        let mut cell = vec![0; PAGE_BYTES];
                        let cell = tree_page::encode_leaf_cell(
                            PageKind::RowLeaf,
                            0,
                            &target.get().to_be_bytes(),
                            &record[..length],
                            &mut cell,
                        )
                        .unwrap();
                        let mut refs: Vec<&[u8]> = vec![&[]; 128];
                        let plan = view
                            .prepare_leaf_edit(LeafEdit::Replace(cell), &mut refs)
                            .unwrap();
                        let lsn = decode_page(bytes, request.identity, PageKind::RowLeaf)
                            .unwrap()
                            .header()
                            .lsn;
                        let mut out = [0; PAGE_BYTES];
                        plan.encode(lsn, &mut out[..]).unwrap();
                        *bytes = out;
                    });
                }
            }
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
            let read_row = match read_point_live(
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
                    assert!(!fence.is_fenced(), "{broken:?}: not fenced");
                    Ok(int_of(&output, copy.len()))
                }
                Ok(_) => panic!("{broken:?}: a resident row is Found"),
                Err(error) => {
                    assert!(fence.is_fenced(), "{broken:?}: fenced");
                    assert_eq!(output, [0xA5; 64], "{broken:?}: no output");
                    Err(error)
                }
            };
            drop(view);
            drop(read);
            drop(holder);
            read_row
        }
    )
}

#[test]
fn rdl_b07_b09_broken_delete_states_are_fatal() {
    case_within("RDL-B07-B09", std::time::Duration::from_secs(600), || {
        for control in [Broken::SameU, Broken::SameTombstone] {
            assert_eq!(broken_read(control).unwrap(), 100, "{control:?}");
            println!("RDL_B07_B09\t{control:?}\tread 100");
        }
        for (broken, phase, undo) in [
            (Broken::ReleasingU, RetentionPhase::Census, true),
            (Broken::MissingU, RetentionPhase::Census, false),
            (Broken::CommittedTombstone, RetentionPhase::Source, false),
            (Broken::InlineRow, RetentionPhase::Census, true),
        ] {
            let error = broken_read(broken).expect_err("a broken DELETE state is never a row");
            let RowReadError::Source(error) = &error else {
                panic!("{broken:?}: {error:?}");
            };
            assert_eq!(error.effect, Effect::CallerMustResolve, "{broken:?}");
            assert_eq!(error.phase, phase, "{broken:?}");
            assert!(
                if undo {
                    matches!(error.cause, RetentionCause::Undo(_))
                } else {
                    matches!(error.cause, RetentionCause::Invariant)
                },
                "{broken:?}: {error:?}"
            );
            println!(
                "RDL_B07_B09\t{broken:?}\t{:?}\t{:?}",
                error.phase, error.cause
            );
        }
    });
}

/// RDL-B06 (dev3-row-delete.md#read, #tests): a DeleteClr waits at the
/// terminal read gate for a read that has captured the tombstone, as an
/// INSERT CLR does (MCB03-V02, MCI03-V01). Rows 100 and 101 are committed
/// and pruned, then an explicit transaction deletes the first (Active, its
/// root Retained). B1's foreign ReadCommitted read of that row is paused
/// after its capture (`Point::RowReadCaptured`), holding the tombstone's
/// Txn and U pins. The caller's rollback of the transaction closes the
/// gate when it starts, before its RollbackStart and DeleteClr, and waits
/// for B1 (`Point::ReadGateWaiting`): no WAL while B1 holds its pins, and a
/// late reader B2 is refused Unchanged InstallationPending. When B1 resumes
/// it reads the old value 100 through U's before; the rollback then writes
/// its groups and ends RolledBack, and B2's retry reads the restored row
/// 100. The drain cleans the compensated root and the Terminal (5 groups)
/// and a new statement reads both rows.
#[test]
fn rdl_b06_delete_clr_waits_for_a_captured_read() {
    use super::super::super::super::{WAIT, installation_pending, thread_point};
    use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
    use std::{
        sync::{Mutex, mpsc::sync_channel},
        time::Duration,
    };

    case_within("RDL-B06", Duration::from_secs(300), || {
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
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    target
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), target);
                let mut b1_private = boxed::<PRIVATE_BYTES>();
                let mut b2_private = boxed::<PRIVATE_BYTES>();
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
                        let wal = files(state).0;
                        std::thread::sleep(Duration::from_millis(200));
                        assert_eq!(files(state).0, wal, "the DeleteClr waits for B1");
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
                        // Paused at RowReadCaptured; the deleter is Active,
                        // so the read is U's before.
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
                    assert_ne!(pins.0, (0, 0, 0), "B1 holds the tombstone's pins");
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
                    println!("RDL_B06 pins={pins:?} at_wait={at_wait:?}");
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
