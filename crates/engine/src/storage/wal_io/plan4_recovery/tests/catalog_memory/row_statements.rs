//! Row statements on a Row capsule (XADB-MODULE-ROW-BEGIN-001 "statement
//! mark"): the mark is the actor's ATT undo_head in slot bytes 88..128, it is
//! compared with the view at every boundary, a clean end closes the sequence
//! and the mark with no WAL, and a second statement follows. Expected values
//! are the actual ATT row and WAL of the same fixture: S1 has no RowReserve or
//! Forward, so every live mark is the None of an empty undo chain.
use super::row_transactions::{row_job, second_status};
use super::*;
use crate::storage::{
    disk::{
        CatalogCreateError, IsolationLevel, RetentionCause, RetentionError, RetentionPhase,
        StatementFinishError, TableStep, TransactionOptions,
    },
    retention_page::row::JobState,
    transaction_record::{TransactionState, TransactionStatusCell},
};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

/// The first ATT row's TransactionStatusCell (descriptor bytes 160..352).
const ATT_STATUS: std::ops::Range<usize> = 160..352;
/// A valid UndoRef wire identity (file 0x0000_0001_0000_0002, page 3,
/// incarnation 4, slot 5, generation 6): a stale mark no S1 view holds.
const STALE: [u8; 40] = [
    2, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0,
    6, 0, 0, 0, 0, 0, 0, 0,
];

/// The live Row actor's ATT status: Active with the given undo head.
fn att_status(bank: &[u8]) -> (TransactionState, Option<crate::storage::undo_ref::UndoRef>) {
    let TransactionStatusCell::Occupied(status) =
        TransactionStatusCell::decode(&bank[ATT_STATUS]).expect("ATT status cell")
    else {
        panic!("occupied ATT status")
    };
    (status.state(), status.undo_head())
}

/// The original `history::validate_live` plus the replayed rows equal live.
macro_rules! live_equals_replay {
    ($writer:expr, $owner:expr) => {{
        let (checked, copies) = $writer.row_test_live_replay($owner);
        checked.unwrap();
        assert_eq!(copies[0], copies[2], "replayed ATT rows");
        assert_eq!(copies[1], copies[3], "replayed Job cells");
    }};
}

/// S1E-R04: the mark of a Row statement is its actor's ATT undo_head, here
/// the None of an empty chain, written as 40 zero bytes beside sequence 1.
/// Begin and a clean finish write no WAL and leave every file, the ATT bank
/// and the Job cells unchanged; finish closes the sequence and the mark.
#[test]
fn row_statement_mark_is_the_att_undo_head_and_writes_no_wal() {
    case("S1E-R04", || {
        for isolation in [IsolationLevel::Serializable, IsolationLevel::ReadCommitted] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = reader_private;
                prepare_create(live, owner, flusher, status, undo);
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let tx = writer
                        .begin_row_transaction(owner, isolation, status, private, &mut flush)
                        .unwrap();
                    let actor = tx.test_write_identity().0.unwrap();
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
                    let images = state.lock().unwrap().images();
                    let bank = writer.recovery_test_bank().to_vec();
                    let jobs = *writer.catalog_test_jobs();
                    assert_eq!(att_status(&bank), (TransactionState::Active, None));
                    assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    assert_eq!(view.own_actor(), Some(actor));
                    assert!(view.row_scope());
                    assert_eq!(view.test_sequence(), 1);
                    // view mark == ATT undo_head == the slot's canonical None.
                    assert_eq!(view.test_mark(), att_status(&bank).1);
                    assert_eq!(tx.test_statement_slot(), (1, [0; 40]));
                    assert_eq!(view.test_charges(), (1024, 1024));
                    assert_eq!(view.q().unwrap(), 0);
                    // No WAL, file, ATT or Job change at begin.
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
                    assert_eq!(state.lock().unwrap().images(), images);
                    assert_eq!(writer.recovery_test_bank(), bank.as_slice());
                    assert_eq!(writer.catalog_test_jobs(), &jobs);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
                    assert_eq!(tx.test_write_identity().4, 512);
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
                    assert_eq!(state.lock().unwrap().images(), images);
                    assert_eq!(writer.recovery_test_bank(), bank.as_slice());
                    assert_eq!(writer.catalog_test_jobs(), &jobs);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    live_equals_replay!(writer, owner);
                    // The empty abort is unchanged by the statement: LSN 7, 8.
                    assert!(
                        writer
                            .abort_empty_transaction(owner, tx, private, &mut flush)
                            .is_ok()
                    );
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                    assert_eq!(
                        row_job(writer.catalog_test_jobs(), actor),
                        (0, JobState::TerminalReady, 2, 0)
                    );
                    live_equals_replay!(writer, owner);
                }
                assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
            });
        }
    });
}

/// S1E-R05: a second statement on the same Row transaction after a clean
/// finish. Each statement's Catalog lookup registers through the view's mark
/// comparison; the ATT row (and so the unused 6080-byte statement credit,
/// reserved only at a first Reserve) is carried unchanged to the next mark.
#[test]
fn second_row_statement_after_clean_finish_carries_the_credit() {
    case("S1E-R05", || {
        for isolation in [IsolationLevel::Serializable, IsolationLevel::ReadCommitted] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                prepare_create(live, owner, flusher, status, undo);
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let tx = writer
                        .begin_row_transaction(owner, isolation, status, private, &mut flush)
                        .unwrap();
                    let identity = tx.test_write_identity();
                    let images = state.lock().unwrap().images();
                    let bank = writer.recovery_test_bank().to_vec();
                    for sequence in 1..=2 {
                        let view = writer.begin_statement(owner, &tx).unwrap();
                        assert_eq!(view.test_sequence(), sequence);
                        assert_eq!(view.test_mark(), None);
                        assert_eq!(tx.test_statement_slot(), (sequence, [0; 40]));
                        assert_eq!(view.q().unwrap(), 0);
                        assert_eq!(view.test_catalog_reads(), u8::from(sequence > 1));
                        let mut call = reader.catalog_statement_call(owner, &view).unwrap();
                        assert!(matches!(
                            call.lookup(
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private
                            )
                            .unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        call.finish_checked().unwrap();
                        assert_eq!(view.test_catalog_reads(), 1);
                        assert!(writer.finish_statement(owner, view).is_ok());
                        assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
                        assert_eq!(writer.recovery_test_bank(), bank.as_slice());
                        assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
                    }
                    assert_eq!(tx.test_write_identity(), identity);
                    assert_eq!(state.lock().unwrap().images(), images);
                    assert!(
                        writer
                            .abort_empty_transaction(owner, tx, private, &mut flush)
                            .is_ok()
                    );
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                    assert_eq!(
                        row_job(writer.catalog_test_jobs(), identity.0.unwrap()),
                        (0, JobState::TerminalReady, 2, 0)
                    );
                    live_equals_replay!(writer, owner);
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                    assert_eq!(writer.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                }
                assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                assert_eq!(live.catalog_test_q(), 0);
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
            });
        }
    });
}

/// S1E-R05T: a Row statement issues a CheckedTable for a committed table
/// through `finish_table`, whose read bit is registered under the view's
/// mark comparison; the next statement registers a Missing lookup the same
/// way. The Row actor uses a second Status beside the committed Catalog one.
#[test]
fn row_statements_issue_a_checked_table_and_register_missing() {
    case("S1E-R05T", || {
        let oracle = Oracle::new();
        // One more frame for the second Status beside the CREATE pair.
        with_fresh!(
            oracle,
            frames = 10,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let row_status = second_status(live, owner, flusher);
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_catalog_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                writer
                    .catalog_test_forward_in_statement(
                        owner,
                        &view,
                        undo,
                        &table_spec("T"),
                        private,
                        &mut flush,
                    )
                    .unwrap();
                assert!(writer.finish_statement(owner, view).is_ok());
                let commit = match writer.commit_catalog_transaction(owner, tx, private) {
                    Ok(commit) => commit,
                    Err(_) => panic!("Catalog Commit"),
                };
                assert_eq!(commit.q(), 1);
                let row = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let actor = row.test_write_identity().0.unwrap();
                let lsn = writer.free_undo_test_state().unwrap().0.complete_lsn;
                let images = state.lock().unwrap().images();
                let view = writer.begin_statement(owner, &row).unwrap();
                assert_eq!((view.test_sequence(), view.test_mark()), (1, None));
                assert_eq!(view.q().unwrap(), 1);
                let call = reader.catalog_statement_call(owner, &view).unwrap();
                let step = call
                    .finish_table(
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .unwrap();
                let TableStep::Table(table) = step else {
                    panic!("a committed visible Table issues to a Row statement")
                };
                assert_eq!(table.test_identity().1, 0);
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (1, (1, 0))
                );
                assert!(table.test_same_origin(core, fence));
                assert!(writer.finish_statement(owner, view).is_ok());
                let view = writer.begin_statement(owner, &row).unwrap();
                assert_eq!((view.test_sequence(), view.test_mark()), (2, None));
                assert_eq!(row.test_statement_slot(), (2, [0; 40]));
                let mut call = reader.catalog_statement_call(owner, &view).unwrap();
                assert!(matches!(
                    call.lookup(
                        ObjectId::new(7).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private
                    )
                    .unwrap(),
                    CatalogLookupStep::Missing
                ));
                call.finish_checked().unwrap();
                // Bits stay on the transaction across statements.
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (1, (1, 0))
                );
                assert!(writer.finish_statement(owner, view).is_ok());
                assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, lsn);
                assert_eq!(state.lock().unwrap().images(), images);
                assert!(
                    writer
                        .abort_empty_transaction(owner, row, private, &mut flush)
                        .is_ok()
                );
                assert_eq!(
                    writer.free_undo_test_state().unwrap().0.complete_lsn,
                    lsn + 2
                );
                assert_eq!(
                    row_job(writer.catalog_test_jobs(), actor).1,
                    JobState::TerminalReady
                );
                live_equals_replay!(writer, owner);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// The boundaries a live view can be refused at. Each runs in its own
/// fixture, since the refusal fences the session.
#[derive(Clone, Copy, Debug)]
enum Boundary {
    Query,
    Cursor,
    Lookup,
    Finish,
    IdleBegin,
}

/// S1E-R06: a view whose mark differs from the slot's is refused as a fenced
/// Fatal(Invariant) at every boundary, with no WAL, file, credit or read-set
/// effect. The stale slot mark is a valid UndoRef, so the refusal is the mark
/// comparison itself, not a malformed encoding. A stale mark left in an idle
/// slot is refused by the next begin. The Catalog and read-only views (the
/// empty-mark special case) refuse the same valid nonzero mark.
#[test]
fn stale_statement_mark_is_refused_at_every_boundary() {
    case("S1E-R06", || {
        let cases = [
            (2, Boundary::Query),
            (2, Boundary::Cursor),
            (2, Boundary::Lookup),
            (2, Boundary::Finish),
            (2, Boundary::IdleBegin),
            (1, Boundary::Query),
            (0, Boundary::Query),
        ];
        for (kind, boundary) in cases {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, reader, mut flush) = live.split();
                let tx = match kind {
                    2 => writer.begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    ),
                    1 => writer.begin_catalog_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    ),
                    _ => writer.begin_transaction(
                        owner,
                        TransactionOptions {
                            isolation: IsolationLevel::Serializable,
                            read_only: true,
                        },
                        private,
                    ),
                }
                .unwrap();
                let charges = tx.test_write_identity().4;
                let images = state.lock().unwrap().images();
                let lsn = writer.free_undo_test_state().unwrap().0.complete_lsn;
                let refused = |error: RetentionError| {
                    assert!(
                        matches!(error.cause, RetentionCause::Invariant),
                        "{kind} {boundary:?}: {error:?}"
                    );
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                };
                if let Boundary::IdleBegin = boundary {
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    assert!(writer.finish_statement(owner, view).is_ok());
                    tx.test_write_statement_mark(STALE);
                    refused(writer.begin_statement(owner, &tx).err().unwrap());
                    assert_eq!(tx.test_statement_slot(), (0, STALE));
                    assert_eq!(tx.test_write_identity().4, charges);
                } else {
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    tx.test_write_statement_mark(STALE);
                    match boundary {
                        Boundary::Query => refused(view.q().err().unwrap()),
                        Boundary::Cursor => {
                            refused(view.reserve_cursor_credit().err().unwrap());
                            assert_eq!(view.test_charges().1, 1024);
                        }
                        Boundary::Lookup => {
                            // Refused after the call's source admission; the
                            // fenced call keeps its full charge (fail-stop).
                            match reader.catalog_statement_call(owner, &view) {
                                Err(error) => refused(error),
                                Ok(_) => panic!("stale mark must not open a call"),
                            }
                            assert_eq!(view.test_catalog_reads(), 0);
                            assert_eq!(
                                reader.catalog_test_source_used(),
                                reader.catalog_test_fixed_charges()[1] as u64
                            );
                        }
                        Boundary::Finish => match writer.finish_statement(owner, view) {
                            Err(StatementFinishError::Fatal(error)) => refused(error),
                            _ => panic!("stale mark must not finish"),
                        },
                        Boundary::IdleBegin => unreachable!(),
                    }
                    assert_eq!(tx.test_statement_slot(), (1, STALE));
                }
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().images(), images);
                let _ = (lsn, core, reader_private);
                drop(tx);
                assert_eq!(state.lock().unwrap().images(), images);
            });
        }
    });
}

/// S1E-R07 (ADV-07): with a Row view, the Catalog CREATE entry refuses it by
/// kind before its gate, source or read-set, and returns the private loan.
#[test]
fn row_view_is_refused_by_catalog_create_before_any_effect() {
    case("S1E-R07", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = reader_private;
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            let images = state.lock().unwrap().images();
            let definition = table_spec("T");
            match writer.reserve_catalog_create(
                owner,
                &view,
                undo,
                &definition,
                private,
                &mut flush,
            ) {
                Err(CatalogCreateError::Before { error, .. }) => {
                    assert!(matches!(error.cause, RetentionCause::Unsupported));
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert_eq!(error.effect, Effect::Unchanged);
                }
                _ => panic!("a Row view must not reach the Catalog CREATE"),
            }
            assert_eq!(view.test_catalog_reads(), 0);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(state.lock().unwrap().images(), images);
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
            assert!(!fence.is_fenced());
        });
    });
}

/// S1E-R08 (ADV-07): an Aborting Row actor. The only S1 producer of that
/// state under a live view is the Row RollbackStart ingress itself (the
/// capsule's own abort needs the view gone). `finish_statement` returns the
/// same view as Before(WrongState) with the slot unchanged and no WAL; the
/// unfinished writable view then fences on drop. `begin_statement` on the
/// Aborting actor is Unchanged WrongState, and the empty abort completes with
/// AbortEnd only. A RowReserved actor needs S2's RowReserve.
#[test]
fn aborting_row_actor_refuses_statement_finish_and_begin() {
    case("S1E-R08", || {
        for open_view in [true, false] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = reader_private;
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let actor = tx.test_write_identity().0.unwrap();
                let view = open_view.then(|| writer.begin_statement(owner, &tx).unwrap());
                writer
                    .row_rollback_start(owner, actor, private, &mut flush)
                    .unwrap();
                assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 7);
                let bank = writer.recovery_test_bank().to_vec();
                assert!(matches!(att_status(&bank).0, TransactionState::Aborting(_)));
                let images = state.lock().unwrap().images();
                if let Some(view) = view {
                    let view = match writer.finish_statement(owner, view) {
                        Err(StatementFinishError::Before { error, view }) => {
                            assert!(matches!(error.cause, RetentionCause::WrongState));
                            assert_eq!(error.phase, RetentionPhase::Finalize);
                            assert_eq!(error.effect, Effect::Unchanged);
                            view
                        }
                        _ => panic!("an Aborting actor's statement must not finish"),
                    };
                    assert_eq!(view.test_sequence(), 1);
                    assert_eq!(tx.test_statement_slot(), (1, [0; 40]));
                    assert_eq!(view.test_charges(), (1024, 1024));
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(state.lock().unwrap().images(), images);
                    assert_eq!(writer.recovery_test_bank(), bank.as_slice());
                    assert!(!fence.is_fenced());
                    drop(view);
                    assert!(fence.is_fenced());
                } else {
                    drop(view);
                    let error = writer.begin_statement(owner, &tx).err().unwrap();
                    assert!(matches!(error.cause, RetentionCause::WrongState));
                    assert_eq!(error.phase, RetentionPhase::Prepare);
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
                    assert_eq!(tx.test_write_identity().4, 512);
                    assert_eq!(state.lock().unwrap().images(), images);
                    // The empty abort sees the Aborting actor: AbortEnd only.
                    assert!(
                        writer
                            .abort_empty_transaction(owner, tx, private, &mut flush)
                            .is_ok()
                    );
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                    assert_eq!(
                        row_job(writer.catalog_test_jobs(), actor),
                        (0, JobState::TerminalReady, 2, 0)
                    );
                    live_equals_replay!(writer, owner);
                    assert!(!fence.is_fenced());
                }
                assert_eq!(writer.free_undo_test_state().is_ok(), !open_view);
            });
        }
    });
}

/// S1E-V07 (MBC10-V07, the view and cursor parts on a Row capsule): the long
/// 512-byte class with 511, 512 and 513 bytes remaining, at view creation and
/// at cursor creation. Refusal is Capacity before any credit, sequence or mark.
#[test]
fn row_view_and_cursor_long_class_boundary() {
    case("S1E-V07", || {
        for remaining in [511_u64, 512, 513] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = (reader_private, core, state);
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                // View creation: the capsule holds 512 of the 49152 bytes.
                let held = 49_152 - 512 - remaining;
                let created = reader.catalog_test_with_held_long(held, || {
                    let result = writer.begin_statement(owner, &tx);
                    if remaining < 512 {
                        let error = result.err().unwrap();
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
                        assert_eq!(tx.test_write_identity().4, held + 512);
                        None
                    } else {
                        let view = result.unwrap();
                        assert_eq!(tx.test_write_identity().4, held + 1024);
                        Some(view)
                    }
                });
                let view = created.unwrap_or_else(|| writer.begin_statement(owner, &tx).unwrap());
                assert_eq!(view.test_charges(), (1024, 1024));
                // Cursor creation: capsule and view hold 1024 bytes.
                let held = 49_152 - 1024 - remaining;
                reader.catalog_test_with_held_long(held, || {
                    let result = view.reserve_cursor_credit();
                    if remaining < 512 {
                        let error = result.err().unwrap();
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert_eq!(view.test_charges(), (held + 1024, 1024));
                    } else {
                        let credit = result.unwrap();
                        assert_eq!(view.test_charges(), (held + 1536, 1536));
                        credit.release_checked().unwrap();
                    }
                });
                assert_eq!(view.test_charges(), (1024, 1024));
                assert_eq!(tx.test_statement_slot(), (1, [0; 40]));
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(
                    writer
                        .abort_empty_transaction(owner, tx, private, &mut flush)
                        .is_ok()
                );
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            });
        }
    });
}

/// S1E-V09 (MBC10-V09, the RowProgress writer as far as S1 reaches it).
/// The Row statement end reserves the one CatalogProgressCharge beside held
/// short loans at one byte short (Capacity, the same view returned), exactly
/// (admitted) and one byte over (admitted). Two resident readers then hold
/// their fixed charges; the statement end closes the read gate and waits,
/// so its charge is admitted only after both returned. A writer overlapping
/// two resident readers is not reachable: the gate makes the progress class
/// exclusive with readers (§6.9), the nested sum is shared + max(writer,
/// 2 resident).
#[test]
fn row_statement_end_progress_admission_boundary_and_two_resident_readers() {
    case("S1E-V09", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            prepare_create(live, owner, flusher, status, undo);
            prepare_catalog(live, owner, flusher);
            let before = state.lock().unwrap().images();
            let mut second_private = boxed::<PRIVATE_BYTES>();
            let mut late_private = boxed::<PRIVATE_BYTES>();
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let lsn = writer.free_undo_test_state().unwrap().0.complete_lsn;
            let shared = writer.catalog_test_commit_charges()[0];
            let charge = writer.catalog_test_retirement_charge();
            let resident = reader.catalog_test_fixed_charges()[2];
            assert_eq!(shared, reader.catalog_test_fixed_charges()[0]);
            for over in [-1_i64, 0, 1] {
                let view = writer.begin_statement(owner, &tx).unwrap();
                let held = (65_536 - shared - charge) as i64 - over;
                let view = reader.catalog_test_with_held_source(held as usize, || {
                    match writer.finish_statement(owner, view) {
                        Ok(()) => {
                            assert!(over >= 0);
                            None
                        }
                        Err(StatementFinishError::Before { error, view }) => {
                            assert_eq!(over, -1);
                            assert!(matches!(error.cause, RetentionCause::Capacity));
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert_eq!(reader.catalog_test_source_used(), held as u64);
                            assert_eq!(tx.test_statement_slot().0, view.test_sequence());
                            Some(view)
                        }
                        Err(StatementFinishError::Fatal(error)) => panic!("{error:?}"),
                    }
                });
                if let Some(view) = view {
                    assert!(writer.finish_statement(owner, view).is_ok());
                }
                assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
                assert_eq!(reader.catalog_test_source_used(), 0);
            }
            assert!(shared + 2 * resident <= 65_536 && shared + charge <= 65_536);
            // Two resident readers in progress, then the Row statement end.
            let view = writer.begin_statement(owner, &tx).unwrap();
            let reader_ref = &reader;
            std::thread::scope(|scope| {
                let mut readers = Vec::new();
                let mut releases = Vec::new();
                for private in [reader_private, &mut *second_private] {
                    let (entered, captured) = sync_channel(1);
                    let (resume, release) = sync_channel(1);
                    hook::install(core, Point::LookupCaptured, entered, release);
                    readers.push(scope.spawn(move || {
                        let mut active = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut active).unwrap();
                        let step = reader_ref
                            .lookup_catalog_resident(
                                &mut active,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                private,
                            )
                            .unwrap();
                        assert!(matches!(step, CatalogLookupStep::Missing));
                        snapshot.release_checked().unwrap();
                        active.check_idle().unwrap();
                    }));
                    captured.recv_timeout(WAIT).unwrap();
                    releases.push(resume);
                }
                assert_eq!(hook::observe(core).active_read_operations, 2);
                assert_eq!(reader.catalog_test_source_used(), 2 * resident as u64);
                let (entered, waiting) = sync_channel(1);
                let (resume_wait, release) = sync_channel(1);
                hook::install(core, Point::ReadGateWaiting, entered, release);
                let late_private = &mut *late_private;
                let controlling = scope.spawn(move || {
                    waiting.recv_timeout(WAIT).unwrap();
                    // The writer waits at the closed gate holding no charge.
                    assert!(hook::observe(core).read_gate_closed);
                    assert_eq!(reader_ref.catalog_test_source_used(), 2 * resident as u64);
                    let (entered, checking) = sync_channel(1);
                    let (resume_check, release) = sync_channel(1);
                    hook::install(core, Point::TransactionChecking, entered, release);
                    resume_wait.send(Resume::Continue).unwrap();
                    for resume in releases {
                        resume.send(Resume::Continue).unwrap();
                    }
                    for reading in readers {
                        reading.join().unwrap();
                    }
                    checking.recv_timeout(WAIT).unwrap();
                    // Admitted after both readers returned: the charge alone.
                    assert_eq!(reader_ref.catalog_test_source_used(), charge as u64);
                    assert_eq!(hook::observe(core).active_read_operations, 0);
                    let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                    let snapshot = reader_ref.catalog_snapshot(&mut late).unwrap();
                    let error = reader_ref
                        .lookup_catalog_resident(
                            &mut late,
                            &snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            late_private,
                        )
                        .err()
                        .unwrap();
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                        if error.cause == crate::transaction::concurrency::Cause::InstallationPending));
                    assert_eq!(reader_ref.catalog_test_source_used(), charge as u64);
                    snapshot.release_checked().unwrap();
                    late.check_idle().unwrap();
                    resume_check.send(Resume::Continue).unwrap();
                });
                assert!(writer.finish_statement(owner, view).is_ok());
                controlling.join().unwrap();
            });
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, lsn);
            assert_eq!(tx.test_statement_slot(), (0, [0; 40]));
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            let _ = before;
            assert!(!fence.is_fenced());
        });
    });
}
