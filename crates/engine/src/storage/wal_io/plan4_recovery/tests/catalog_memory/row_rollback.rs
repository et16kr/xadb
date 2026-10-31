//! S4 live rollback consumer. Expectations are state-machine facts, not values
//! read back from the same producer as an oracle.
use super::row_preflight::{create_table, system_init};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RollbackScope, RollbackStep, RowOperation, RowOwnedError, RowPreflightStep,
        TableStep,
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
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}
