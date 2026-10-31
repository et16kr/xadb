//! S4 live rollback consumer. Expectations are state-machine facts, not values
//! read back from the same producer as an oracle.
use super::row_preflight::{create_table, system_init};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RetentionCause, RollbackScope, RollbackStep, RowOperation, RowOwnedError,
        RowPreflightStep, TableStep,
    },
    retention_page::row::{self as job3, JobState},
    row::insert::{produce_clr, produce_insert, validate_insert},
    row_record::ValueRef,
};

macro_rules! applied {
    ($e:expr) => {
        match $e {
            Ok(value) => value,
            Err(RowOwnedError::Before { error, .. }) => panic!("Before {error:?}"),
            Err(RowOwnedError::Fatal(error)) => panic!("Fatal {error:?}"),
        }
    };
}
macro_rules! put_row {
    ($writer:ident,$owner:ident,$view:ident,$table:ident,$private:ident,$flush:ident,$flusher:ident,$value:expr) => {{
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
                    break applied!(prepared.apply()).into_parts().0;
                }
                RowPreflightStep::FlushRequired { target } => {
                    flush_target!($flush, $flusher, target)
                }
            }
        }
    }};
}
macro_rules! drain_rollback {
    ($rollback:ident) => {{
        let mut steps = 0;
        loop {
            let target = match $rollback.next().unwrap() {
                RollbackStep::Candidate(mut task) => {
                    let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
                    let prepared = applied!(task.prepare(proposal));
                    assert_eq!(applied!(prepared.apply()).operation(), RowOperation::Clr);
                    steps += 1;
                    None
                }
                RollbackStep::FlushRequired { target } => Some(target),
                RollbackStep::Complete => break,
            };
            if let Some(target) = target {
                $rollback.flush(&target).unwrap();
            }
            assert!(steps <= 3);
        }
        steps
    }};
}

#[test]
fn s4_r01_statement_suffix_then_commit() {
    case("S4-R01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                system_init(live, owner, flusher, true);
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
                let first = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, first, reader_private, flush, flusher);
                let kept = put_row!(writer, owner, first, table, private, flush, flusher, 7);
                assert!(writer.finish_statement(owner, first).is_ok());
                let second = writer.begin_statement(owner, &tx).unwrap();
                assert_eq!(second.test_mark(), Some(kept.undo()));
                put_row!(writer, owner, second, table, private, flush, flusher, 8);
                put_row!(writer, owner, second, table, private, flush, flusher, 9);
                let actor = second.own_actor().unwrap();
                {
                    let mut rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            Some(&second),
                            RollbackScope::Statement,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    assert_eq!(drain_rollback!(rollback), 2);
                    applied!(rollback.finish());
                }
                assert!(
                    second.q().is_err(),
                    "failed-finished statement cannot be reused"
                );
                assert!(writer.finish_statement(owner, second).is_ok());
                let third = writer.begin_statement(owner, &tx).unwrap();
                assert_eq!(third.test_mark(), Some(kept.undo()));
                put_row!(writer, owner, third, table, private, flush, flusher, 10);
                assert!(writer.finish_statement(owner, third).is_ok());
                let mut retained = 0;
                let mut compensated = 0;
                for bytes in writer.catalog_test_jobs() {
                    let job = job3::decode_job(bytes, super::super::support::namespace()).unwrap();
                    if job.source() == Some(actor) {
                        match job.state() {
                            JobState::RootRetained => retained += 1,
                            JobState::RootReleasingCompensated => compensated += 1,
                            _ => {}
                        }
                    }
                }
                assert_eq!((retained, compensated), (2, 2));
                let commit = match writer.commit_row(owner, tx, private) {
                    Ok(value) => value,
                    Err(_) => panic!("commit after suffix rollback"),
                };
                assert_eq!(commit.q(), 2);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn s4_r02_full_abort_one_row() {
    case("S4-R02", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
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
                let before = super::row_forward::files(state).0;
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
                    assert_eq!(drain_rollback!(rollback), 1);
                    applied!(rollback.finish());
                }
                assert!(writer.finish_aborted(owner, tx).is_ok());
                let expected = include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/fixtures/dev3-module-c3/row-rollback/rollback.bin"
                ));
                assert_eq!(super::row_forward::wal_since(state, before), expected);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

macro_rules! with_row {
    ($id:literal, |$writer:ident,$reader:ident,$flush:ident,$tx:ident,$owner:ident,$flusher:ident,$private:ident,$core:ident,$fence:ident,$state:ident| $body:block) => {
        case($id, || {
            let oracle = Oracle::new();
            with_fresh!(
                oracle,
                frames = 12,
                |live,
                 $owner,
                 $flusher,
                 status,
                 undo,
                 $private,
                 reader_private,
                 $core,
                 $fence,
                 $state| {
                    let (row_status, _) =
                        create_table(live, $owner, $flusher, status, undo, $private, true);
                    let (mut $writer, $reader, mut $flush) = live.split();
                    let $tx = $writer
                        .begin_row_transaction(
                            $owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            $private,
                            &mut $flush,
                        )
                        .unwrap();
                    let view = $writer.begin_statement($owner, &$tx).unwrap();
                    let table =
                        issue_table!($reader, $owner, view, reader_private, $flush, $flusher);
                    put_row!($writer, $owner, view, table, $private, $flush, $flusher, 7);
                    assert!($writer.finish_statement($owner, view).is_ok());
                    $body
                }
            );
        });
    };
}

#[test]
fn s4_r03_premature_finish_bad_proposal_and_actual_stage_retry() {
    with_row!("S4-R03", |writer,
                         reader,
                         flush,
                         tx,
                         owner,
                         flusher,
                         private,
                         core,
                         fence,
                         state| {
        let _ = (core, state);
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
        rollback = match rollback.finish() {
            Err(RowOwnedError::Before { error, owned }) => {
                assert!(matches!(error.cause, RetentionCause::WrongState));
                owned
            }
            _ => panic!("premature finish"),
        };
        let mut task = match rollback.next().unwrap() {
            RollbackStep::Candidate(task) => task,
            _ => panic!("head candidate"),
        };
        let before = super::row_forward::files(state);
        let wrong = crate::storage::disk::RowProposal::new(
            RowOperation::Forward,
            1,
            1,
            [crate::storage::disk::RowPageRole::RowLeaf; 5],
        );
        task = match task.prepare(wrong) {
            Err(RowOwnedError::Before { error, owned }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                owned
            }
            _ => panic!("operation mismatch"),
        };
        assert_eq!(super::row_forward::files(state), before);
        let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
        let prepared = applied!(task.prepare(proposal));
        let fault = crate::storage::wal_io::durability_test_support::stage_capacity_once();
        let prepared = match prepared.apply() {
            Err(RowOwnedError::Before { error, owned }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(matches!(error.cause, RetentionCause::Wal(_)));
                owned
            }
            _ => panic!("actual stage refusal"),
        };
        assert_eq!(fault.hits(), 1);
        drop(fault);
        assert_eq!(super::row_forward::files(state), before);
        applied!(prepared.apply());
        assert!(matches!(rollback.next().unwrap(), RollbackStep::Complete));
        applied!(rollback.finish());
        assert!(writer.finish_aborted(owner, tx).is_ok());
        assert_eq!(reader.catalog_test_source_used(), 0);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn s4_r04_unfinished_drop_fences_without_more_io() {
    with_row!("S4-R04", |writer,
                         reader,
                         flush,
                         tx,
                         owner,
                         flusher,
                         private,
                         core,
                         fence,
                         state| {
        let _ = (core, reader);
        let rollback = writer
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
        let before = super::row_forward::files(state);
        drop(rollback);
        assert!(fence.is_fenced());
        assert_eq!(super::row_forward::files(state), before);
        drop(tx);
    });
}

fn clr_io_failure(sync: bool) {
    with_row!("S4-R05", |writer,
                         reader,
                         flush,
                         tx,
                         owner,
                         flusher,
                         private,
                         core,
                         fence,
                         state| {
        let _ = (core, reader);
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
        let mut task = match rollback.next().unwrap() {
            RollbackStep::Candidate(task) => task,
            _ => panic!("head candidate"),
        };
        let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
        let prepared = applied!(task.prepare(proposal));
        let at = super::row_forward::files(state).0;
        {
            let mut state = state.lock().unwrap();
            state.events.clear();
            state.fault_hits = 0;
            state.fault = Some(port::Fault {
                key: port::Key::new(
                    if sync { "sync" } else { "write" },
                    "d3-wal-00000001",
                    if sync { 0 } else { at as u64 },
                    1,
                ),
                action: if sync {
                    port::Action::AfterError
                } else {
                    port::Action::PrefixErrorBytes(17)
                },
            });
        }
        match prepared.apply() {
            Err(RowOwnedError::Fatal(error)) => assert_eq!(error.effect, Effect::CallerMustResolve),
            _ => panic!("uncertain I/O must fence"),
        }
        assert!(fence.is_fenced());
        assert_eq!(state.lock().unwrap().fault_hits, 1);
        drop(rollback);
        drop(tx);
    });
}
#[test]
fn s4_r05_partial_clr_write_is_fatal() {
    clr_io_failure(false);
}
#[test]
fn s4_r05_clr_sync_uncertainty_is_fatal() {
    clr_io_failure(true);
}

#[test]
fn s4_r06_empty_statement_and_empty_full_abort() {
    case("S4-R06", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (reader_private, core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, false);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                // Begin reserves full-abort progress only. Without a RowReserve
                // this statement has no 6080-byte rollback credit: refuse before
                // WAL and close the original empty view normally.
                let before = super::row_forward::files(state);
                let error = writer
                    .start_row_rollback(
                        owner,
                        &tx,
                        Some(&view),
                        RollbackScope::Statement,
                        private,
                        &mut flush,
                        flusher,
                    )
                    .err()
                    .expect("unfunded statement");
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(super::row_forward::files(state), before);
                assert!(!fence.is_fenced());
                assert!(writer.finish_statement(owner, view).is_ok());
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
                assert!(matches!(rollback.next().unwrap(), RollbackStep::Complete));
                applied!(rollback.finish());
                assert!(writer.finish_aborted(owner, tx).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[path = "row_rollback/native.rs"]
mod native;
#[path = "row_rollback/v09.rs"]
mod v09;

#[test]
fn s4_r07_flush_before_error_preserves_a_retryable_rollback() {
    with_row!("S4-R07", |writer,
                         reader,
                         flush,
                         tx,
                         owner,
                         flusher,
                         private,
                         core,
                         fence,
                         state| {
        let _ = (core, state);
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
        let target = rollback.test_root();
        rollback.test_flush_refusal();
        let error = rollback.flush(&target).unwrap_err();
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(!fence.is_fenced());
        rollback.flush(&target).unwrap();
        assert_eq!(drain_rollback!(rollback), 1);
        applied!(rollback.finish());
        assert!(writer.finish_aborted(owner, tx).is_ok());
        assert_eq!(reader.catalog_test_source_used(), 0);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn s4_r08_serializable_conflict_then_full_abort() {
    case("S4-R08", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let other_status = system_init(live, owner, flusher, false);
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
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                put_row!(writer, owner, view, table, private, flush, flusher, 7);
                assert!(writer.finish_statement(owner, view).is_ok());
                let other = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &other).unwrap();
                put_row!(writer, owner, view, table, private, flush, flusher, 8);
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.commit_row(owner, other, private).is_ok());
                use crate::storage::row::insert::driver::{
                    After, Stepped,
                    live::{
                        LiveCall, commit_live, finish_aborted_live, finish_rollback_live,
                        rollback_step_live, start_full_rollback_live,
                    },
                };
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let mut pending = match commit_live(&mut call, tx, private) {
                    Err(error) => match error.after {
                        After::RollbackRequired(pending) => pending,
                        _ => panic!("expected rollback-required"),
                    },
                    Ok(_) => panic!("expected conflict"),
                };
                {
                    let mut rollback =
                        match start_full_rollback_live(&mut call, &mut pending, private) {
                            Ok(r) => r,
                            Err(error) => panic!("start {error:?}"),
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
                let tx = match pending.rolled_back() {
                    Ok(tx) => tx,
                    Err(_) => panic!("pending completion"),
                };
                assert!(finish_aborted_live(&mut call, tx).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn s4_r09_wrong_after_digest_is_unchanged_and_same_task_retries() {
    with_row!("S4-R09", |writer,
                         reader,
                         flush,
                         tx,
                         owner,
                         flusher,
                         private,
                         core,
                         fence,
                         state| {
        let _ = core;
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
        let mut task = match rollback.next().unwrap() {
            RollbackStep::Candidate(task) => task,
            _ => panic!("head"),
        };
        let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
        {
            let (_, _, _, _, _, after, _, _) = task.producer_lane().unwrap().parts();
            after[0][100] ^= 1;
        }
        let before = super::row_forward::files(state);
        task = match task.prepare(proposal) {
            Err(RowOwnedError::Before { error, owned }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(matches!(error.cause, RetentionCause::RowSemantic(_)));
                owned
            }
            _ => panic!("bad after digest"),
        };
        assert_eq!(super::row_forward::files(state), before);
        assert!(!fence.is_fenced());
        let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
        applied!(applied!(task.prepare(proposal)).apply());
        assert!(matches!(rollback.next().unwrap(), RollbackStep::Complete));
        applied!(rollback.finish());
        assert!(writer.finish_aborted(owner, tx).is_ok());
        assert_eq!(reader.catalog_test_source_used(), 0);
        assert!(!fence.is_fenced());
    });
}
#[test]
fn s4_r10_full_abort_three_roots_from_two_statements() {
    case("S4-R10", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let first = writer.begin_statement(owner, &tx).unwrap();
                let actor = first.own_actor().unwrap();
                let table = issue_table!(reader, owner, first, reader_private, flush, flusher);
                put_row!(writer, owner, first, table, private, flush, flusher, 7);
                put_row!(writer, owner, first, table, private, flush, flusher, 8);
                assert!(writer.finish_statement(owner, first).is_ok());
                let second = writer.begin_statement(owner, &tx).unwrap();
                put_row!(writer, owner, second, table, private, flush, flusher, 9);
                assert!(writer.finish_statement(owner, second).is_ok());
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
                assert_eq!(drain_rollback!(rollback), 3);
                applied!(rollback.finish());
                let mut roots = Vec::new();
                for bytes in writer.catalog_test_jobs() {
                    let job = job3::decode_job(bytes, super::super::support::namespace()).unwrap();
                    if job.source() == Some(actor) && job.state().is_root() {
                        assert_eq!(job.state(), JobState::RootReleasingCompensated);
                        assert_eq!(job.commit_q(), 0);
                        roots.push((job.mutation(), job.statement()));
                    }
                }
                roots.sort_unstable();
                assert_eq!(roots, [(1, 1), (2, 1), (3, 2)]);
                assert!(writer.finish_aborted(owner, tx).is_ok());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}
#[test]
fn s4_r11_wrong_capsule_view_and_scope_are_unchanged() {
    case("S4-R11", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let other_status = system_init(live, owner, flusher, false);
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
                let other = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let before = super::row_forward::files(state);
                for variant in 0..3 {
                    let (capsule, statement, scope) = match variant {
                        0 => (&other, Some(&view), RollbackScope::Statement),
                        1 => (&tx, None, RollbackScope::Transaction),
                        _ => (&tx, None, RollbackScope::Statement),
                    };
                    let error = writer
                        .start_row_rollback(
                            owner, capsule, statement, scope, private, &mut flush, flusher,
                        )
                        .err()
                        .expect("bad binding/scope");
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(super::row_forward::files(state), before);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!fence.is_fenced());
                }
                let read = writer
                    .begin_transaction(
                        owner,
                        crate::storage::disk::TransactionOptions {
                            isolation: IsolationLevel::Serializable,
                            read_only: true,
                        },
                        private,
                    )
                    .unwrap();
                let error = writer
                    .start_row_rollback(
                        owner,
                        &read,
                        None,
                        RollbackScope::Transaction,
                        private,
                        &mut flush,
                        flusher,
                    )
                    .err()
                    .expect("read-only scope");
                assert!(matches!(error.cause, RetentionCause::Unsupported));
                assert_eq!(super::row_forward::files(state), before);
                assert!(writer.finish_read_only(owner, read).is_ok());
                assert!(writer.finish_statement(owner, view).is_ok());
                for tx in [other, tx] {
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
                    let count = drain_rollback!(rollback);
                    assert!(count <= 1);
                    applied!(rollback.finish());
                    assert!(writer.finish_aborted(owner, tx).is_ok());
                }
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}
