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
//!   refuses it (Census, Invariant).
//!
//! Each edit path first runs as a control that re-encodes the page's own
//! cell unchanged: the read then takes the committed row 100 through the
//! tombstone's U, so each refusal comes from the state, not from the edit.
//!
//! The other half of the contract's B09, a RowValue U under an Inline
//! `Transaction` row, stays the typed Unsupported of `read_missing`
//! (MCB04-V14, consumers F02): see dev3-row-delete.md#implementation-2b2.
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
                Broken::SameTombstone | Broken::CommittedTombstone => {
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
                        let version = match broken {
                            Broken::SameTombstone => tombstone.version(),
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
                            RowPayload::Deleted,
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
