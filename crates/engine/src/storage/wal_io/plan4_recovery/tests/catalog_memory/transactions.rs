//! New read-only ownership paths through the actual original session.
use super::*;
use crate::storage::disk::{
    IsolationLevel, StatementFinishError, TransactionFinishError, TransactionOptions,
};

fn lifecycle(mode: IsolationLevel) {
    case(
        if mode == IsolationLevel::Serializable {
            "TX_SER"
        } else {
            "TX_RC"
        },
        || {
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
                let _ = core;
                let actor = create_forward(
                    live, owner, flusher, status, undo, private, &oracle, state, 1,
                );
                let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
                let (mut writer, _, _) = live.split();
                let transaction = match writer.begin_transaction(
                    owner,
                    TransactionOptions {
                        isolation: mode,
                        read_only: true,
                    },
                    reader_private,
                ) {
                    Ok(t) => t,
                    Err(e) => panic!("begin {e:?}"),
                };
                let view = writer.begin_statement(owner, &transaction).unwrap();
                assert_eq!(view.q().unwrap(), 0);
                assert_eq!(view.test_sequence(), 1);
                assert_eq!(view.own_actor(), None);
                assert_eq!(view.revision(), 0);
                assert!(writer.begin_statement(owner, &transaction).is_err());
                assert!(writer.finish_statement(owner, view).is_ok());
                writer.commit_catalog(owner, prepared).unwrap();
                let view = writer.begin_statement(owner, &transaction).unwrap();
                assert_eq!(view.test_sequence(), 2);
                assert_eq!(
                    view.q().unwrap(),
                    if mode == IsolationLevel::Serializable {
                        0
                    } else {
                        1
                    }
                );
                assert!(writer.finish_statement(owner, view).is_ok());
                let completed = match writer.finish_read_only(owner, transaction) {
                    Ok(v) => v,
                    Err(_) => panic!("empty read-set finish"),
                };
                assert_eq!(completed.q(), 1);
                assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
                flusher.check_idle().unwrap();
            });
        },
    );
}
#[test]
fn serializable_keeps_original_snapshot_across_statements() {
    lifecycle(IsolationLevel::Serializable);
}
#[test]
fn read_committed_renews_statement_snapshot() {
    lifecycle(IsolationLevel::ReadCommitted);
}

#[test]
fn read_only_capacity_drop_and_generation_are_bounded() {
    case("TX_CAP", || {
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
            let _ = (status, undo, reader_private, core);
            let before = state.lock().unwrap().images();
            let (mut writer, _, _) = live.split();
            let mut transactions = Vec::new();
            for _ in 0..32 {
                let t = writer
                    .begin_transaction(
                        owner,
                        TransactionOptions {
                            isolation: IsolationLevel::ReadCommitted,
                            read_only: true,
                        },
                        private,
                    )
                    .unwrap();
                transactions.push(t);
            }
            let failure = match writer.begin_transaction(
                owner,
                TransactionOptions {
                    isolation: IsolationLevel::ReadCommitted,
                    read_only: true,
                },
                private,
            ) {
                Err(e) => e,
                Ok(_) => panic!("33rd slot admitted"),
            };
            assert!(matches!(
                failure.cause,
                super::super::super::super::plan4_pages::RetentionCause::Capacity
            ));
            assert_eq!(failure.effect, Effect::Unchanged);
            drop(transactions);
            let t = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        isolation: IsolationLevel::ReadCommitted,
                        read_only: true,
                    },
                    private,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &t).unwrap();
            drop(view);
            let view = writer.begin_statement(owner, &t).unwrap();
            assert_eq!(view.test_sequence(), 2);
            drop(view);
            drop(t);
            assert_eq!(live.catalog_test_isolation(), (34, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn foreign_owner_returns_original_handles_and_writes_are_unsupported() {
    case("TX_FOREIGN", || {
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
                             state,
                             foreign| {
            let _ = (status, undo, reader_private, core);
            let mut other = foreign
                .try_owner(crate::transaction::concurrency::OwnerRole::Foreground)
                .unwrap();
            let before = state.lock().unwrap().images();
            let (mut writer, _, _) = live.split();
            assert!(
                writer
                    .begin_transaction(owner, TransactionOptions::default(), private)
                    .is_err()
            );
            assert!(
                writer
                    .begin_transaction(
                        &mut other,
                        TransactionOptions {
                            read_only: true,
                            ..TransactionOptions::default()
                        },
                        private
                    )
                    .is_err()
            );
            let t = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    private,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &t).unwrap();
            let view = match writer.finish_statement(&mut other, view) {
                Err(StatementFinishError::Before { error, view }) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    view
                }
                _ => panic!("foreign finish must return same healthy view"),
            };
            assert_eq!(view.q().unwrap(), 0);
            assert_eq!(view.test_sequence(), 1);
            assert!(writer.finish_statement(owner, view).is_ok());
            let t = match writer.finish_read_only(&mut other, t) {
                Err(TransactionFinishError::Before { error, transaction }) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    transaction
                }
                _ => panic!("foreign finish must return same healthy transaction"),
            };
            assert!(writer.finish_read_only(owner, t).is_ok());
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn failed_snapshot_registration_returns_slot_and_credit() {
    case("TX_SNAPSHOT_FULL", || {
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
            let _ = (status, undo, reader_private, core);
            let before = state.lock().unwrap().images();
            let (mut writer, reader, _) = live.split();
            let mut snapshots = Vec::new();
            for _ in 0..32 {
                snapshots.push(reader.catalog_snapshot(owner).unwrap());
            }
            assert!(
                writer
                    .begin_transaction(
                        owner,
                        TransactionOptions {
                            read_only: true,
                            ..TransactionOptions::default()
                        },
                        private
                    )
                    .is_err()
            );
            for snapshot in snapshots {
                snapshot.release_checked().unwrap();
            }
            let t = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    private,
                )
                .unwrap();
            assert!(writer.finish_read_only(owner, t).is_ok());
            assert_eq!(live.catalog_test_isolation(), (3, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn failed_rc_statement_snapshot_returns_credit_without_consuming_sequence() {
    case("TX_RC_SNAPSHOT_FULL", || {
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
            let _ = (status, undo, reader_private, core);
            let before = state.lock().unwrap().images();
            let (mut writer, reader, _) = live.split();
            let t = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        isolation: IsolationLevel::ReadCommitted,
                        read_only: true,
                    },
                    private,
                )
                .unwrap();
            let mut snapshots = Vec::new();
            for _ in 0..32 {
                snapshots.push(reader.catalog_snapshot(owner).unwrap());
            }
            let error = match writer.begin_statement(owner, &t) {
                Err(e) => e,
                Ok(_) => panic!("RC snapshot capacity must reject"),
            };
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(
                matches!(error.cause, super::super::super::super::plan4_pages::RetentionCause::Visibility(e)
                if e.cause == super::super::super::super::plan4_pages::VisibilityCause::Capacity)
            );
            for snapshot in snapshots {
                snapshot.release_checked().unwrap();
            }
            let view = writer.begin_statement(owner, &t).unwrap();
            assert_eq!(view.test_sequence(), 1);
            assert_eq!(view.q().unwrap(), 0);
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(writer.finish_read_only(owner, t).is_ok());
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}
