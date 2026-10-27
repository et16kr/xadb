//! Actual statement reads, automatic catalog dependence and terminal validation.
use super::*;
use crate::storage::{
    dirty_frame,
    disk::{
        IsolationLevel, RetentionCause, RetentionPhase, TransactionFinishError, TransactionOptions,
    },
};
use crate::transaction::concurrency::catalog_test_support::observe;

#[test]
fn missing_read_survives_statement_drop_and_rejects_serializable_commit() {
    case("C2_STMT01", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            let mut reader_owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let (mut writer, reader, _) = live.split();
            let transaction = writer
                .begin_transaction(
                    &mut reader_owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    reader_private,
                )
                .unwrap();
            let view = writer
                .begin_statement(&mut reader_owner, &transaction)
                .unwrap();
            let mut call = reader
                .catalog_statement_call(&mut reader_owner, &view)
                .unwrap();
            assert!(matches!(
                call.lookup(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    reader_private
                )
                .unwrap(),
                CatalogLookupStep::Missing
            ));
            assert_eq!(view.test_catalog_reads(), 1);
            assert_eq!(observe(core).active_read_operations, 1);
            assert!(reader.catalog_test_source_used() > 0);
            call.finish_checked().unwrap();
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(observe(core).active_read_operations, 0);
            // Abandoning a read-only statement does not erase its observation.
            drop(view);
            writer.commit_catalog(owner, prepared).unwrap();
            let after_commit = state.lock().unwrap().images();
            match writer.finish_read_only(&mut reader_owner, transaction) {
                Err(TransactionFinishError::Rejected(error)) => {
                    assert!(matches!(error.cause, RetentionCause::SerializationConflict));
                    assert_eq!(error.phase, RetentionPhase::Finalize);
                    assert_eq!(error.effect, Effect::Unchanged);
                }
                _ => panic!("actual catalog absence must conflict"),
            }
            assert_eq!(state.lock().unwrap().images(), after_commit);
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn read_committed_keeps_one_statement_and_refreshes_the_next() {
    case("C2_STMT02", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            let mut reader_owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let (mut writer, reader, _) = live.split();
            let transaction = writer
                .begin_transaction(
                    &mut reader_owner,
                    TransactionOptions {
                        read_only: true,
                        isolation: IsolationLevel::ReadCommitted,
                    },
                    reader_private,
                )
                .unwrap();
            let view = writer
                .begin_statement(&mut reader_owner, &transaction)
                .unwrap();
            let mut prepared = Some(prepared);
            for committed in [false, true] {
                if committed {
                    writer
                        .commit_catalog(owner, prepared.take().unwrap())
                        .unwrap();
                }
                let mut call = reader
                    .catalog_statement_call(&mut reader_owner, &view)
                    .unwrap();
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
                assert_eq!(view.q().unwrap(), 0);
                assert_eq!(view.test_catalog_reads(), 1);
            }
            assert!(writer.finish_statement(&mut reader_owner, view).is_ok());
            let view = writer
                .begin_statement(&mut reader_owner, &transaction)
                .unwrap();
            assert_eq!(view.q().unwrap(), 1);
            let mut call = reader
                .catalog_statement_call(&mut reader_owner, &view)
                .unwrap();
            let CatalogLookupStep::Found(record) = call
                .lookup(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                )
                .unwrap()
            else {
                panic!("new statement must observe committed table")
            };
            assert_eq!(record.header().identity.object.get(), 6);
            let crate::storage::catalog_record::CatalogDefinitionView::Table(table) =
                record.definition()
            else {
                panic!("table")
            };
            assert_eq!(table.column(0).unwrap().name, "i");
            assert_eq!(view.test_catalog_reads(), 1);
            call.finish_checked().unwrap();
            assert!(writer.finish_statement(&mut reader_owner, view).is_ok());
            let committed = match writer.finish_read_only(&mut reader_owner, transaction) {
                Ok(value) => value,
                Err(_) => panic!("RC completion"),
            };
            assert_eq!(committed.q(), 1);
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn found_read_registers_before_consumption_and_foreign_owner_does_not() {
    case("C2_STMT03", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            {
                let (mut writer, _, _) = live.split();
                writer.commit_catalog(owner, prepared).unwrap();
            }
            let before = state.lock().unwrap().images();
            let (mut writer, reader, _) = live.split();
            let transaction = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    reader_private,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &transaction).unwrap();
            let mut other = foreign.try_owner(OwnerRole::Foreground).unwrap();
            let error = reader
                .catalog_statement_call(&mut other, &view)
                .err()
                .unwrap();
            assert!(
                matches!(error.cause, RetentionCause::Frame(e) if e.cause == dirty_frame::Cause::BindingMismatch)
            );
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(view.test_catalog_reads(), 0);
            let mut call = reader.catalog_statement_call(owner, &view).unwrap();
            let CatalogLookupStep::Found(record) = call
                .lookup(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                )
                .unwrap()
            else {
                panic!("found")
            };
            assert_eq!(view.test_catalog_reads(), 1);
            assert_eq!(record.header().identity.object.get(), 6);
            call.finish_checked().unwrap();
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(writer.finish_read_only(owner, transaction).is_ok());
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(observe(core).active_read_operations, 0);
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn missing_original_visibility_is_rejected_before_admission() {
    case("C2_STMT04", || {
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
            let _ = (status, undo, private);
            prepare_catalog(live, owner, flusher);
            let (mut writer, reader, _) = live.split();
            let transaction = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    reader_private,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &transaction).unwrap();
            let before = state.lock().unwrap().images();
            let error = reader
                .catalog_test_missing_statement_visibility(owner, &view)
                .unwrap_err();
            assert!(matches!(error.cause, RetentionCause::BindingMismatch));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(observe(core).active_read_operations, 0);
            assert_eq!(view.test_catalog_reads(), 0);
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(writer.finish_read_only(owner, transaction).is_ok());
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn busy_and_io_error_do_not_register_a_catalog_observation() {
    for io_error in [false, true] {
        case(if io_error { "C2_STMT06" } else { "C2_STMT05" }, || {
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
                let _actor = create_forward(
                    live, owner, flusher, status, undo, private, &oracle, state, 1,
                );
                super::read_boundary::evict_undo(live, flusher, undo);
                let (mut writer, reader, _) = live.split();
                let transaction = writer
                    .begin_transaction(
                        owner,
                        TransactionOptions {
                            read_only: true,
                            ..TransactionOptions::default()
                        },
                        reader_private,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &transaction).unwrap();
                let mut call = if io_error {
                    reader.catalog_statement_call(owner, &view).unwrap()
                } else {
                    reader
                        .catalog_resident_statement_call(owner, &view)
                        .unwrap()
                };
                let charge = reader.catalog_test_source_used();
                let before = state.lock().unwrap().events.len();
                if io_error {
                    let mut io = state.lock().unwrap();
                    io.events.clear();
                    io.fault_hits = 0;
                    io.fault = Some(port::Fault {
                        key: port::Key::new("read", HOME_NAME, 10 * PAGE_BYTES as u64, 1),
                        action: port::Action::BeforeError,
                    });
                }
                let original = call
                    .lookup(
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .err()
                    .unwrap();
                assert_eq!(view.test_catalog_reads(), 0);
                let error = call.finish_error(original);
                assert_eq!(observe(core).active_read_operations, 0);
                assert_eq!(
                    reader.catalog_test_source_used(),
                    if io_error { charge } else { 0 }
                );
                if io_error {
                    assert_eq!(state.lock().unwrap().fault_hits, 1);
                    assert!(matches!(error.cause, RetentionCause::PageIo(_)));
                    assert_eq!(error.phase, RetentionPhase::Load);
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                } else {
                    assert!(matches!(error.cause, RetentionCause::Busy));
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(state.lock().unwrap().events.len(), before);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    assert!(writer.finish_read_only(owner, transaction).is_ok());
                }
                assert_eq!(fence.is_fenced(), io_error);
            });
        });
    }
}

#[test]
fn flush_required_records_only_the_completed_retry() {
    case("C2_STMT07", || {
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
            let _actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            super::read_boundary::evict_undo(live, flusher, undo);
            let mut clean = Vec::new();
            for index in 1..9 {
                let pool = live.catalog_test_pool();
                let request = pool.target_request(index).unwrap();
                let guard = pool
                    .reserve_target_set(owner, &[request])
                    .unwrap()
                    .try_read()
                    .unwrap();
                if guard.observe(0).unwrap().frame.rec_lsn.is_none() {
                    clean.push(request);
                }
                guard.release_checked().unwrap();
            }
            assert!(!clean.is_empty());
            let mut holders: Vec<_> = clean
                .iter()
                .map(|_| core.try_owner(OwnerRole::Foreground).unwrap())
                .collect();
            let (mut writer, reader, mut flush) = live.split();
            let transaction = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    reader_private,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &transaction).unwrap();
            let mut guards = Vec::new();
            for (holder, request) in holders.iter_mut().zip(&clean) {
                guards.push(
                    writer
                        .catalog_test_pool()
                        .reserve_target_set(holder, &[*request])
                        .unwrap()
                        .try_read()
                        .unwrap(),
                );
            }
            let retained_pins = writer.catalog_test_physical_pins();
            assert_eq!(usize::from(retained_pins), clean.len());
            let mut cursor = Plan4ResidencyCursor::new();
            let mut call = reader.catalog_statement_call(owner, &view).unwrap();
            let target = match call
                .lookup(ObjectId::new(6).unwrap(), &mut cursor, reader_private)
                .unwrap()
            {
                CatalogLookupStep::FlushRequired { target } => target,
                _ => panic!("all clean frames pinned: actual dirty victim required"),
            };
            assert_eq!(view.test_catalog_reads(), 0);
            call.finish_checked().unwrap();
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), retained_pins);
            owner.check_idle().unwrap();
            assert_eq!(view.q().unwrap(), 0);
            for guard in guards {
                guard.release_checked().unwrap();
            }
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            for holder in &holders {
                holder.check_idle().unwrap();
            }
            let mut step = flush.try_flush(flusher, &target).unwrap();
            for _ in 0..8 {
                if matches!(
                    step,
                    RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
                ) {
                    break;
                }
                step = flush.continue_flush(flusher).unwrap();
            }
            assert!(matches!(
                step,
                RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
            ));
            let mut call = reader.catalog_statement_call(owner, &view).unwrap();
            assert!(matches!(
                call.lookup(ObjectId::new(6).unwrap(), &mut cursor, reader_private)
                    .unwrap(),
                CatalogLookupStep::Missing
            ));
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            assert_eq!(view.q().unwrap(), 0);
            assert_eq!(view.test_catalog_reads(), 1);
            call.finish_checked().unwrap();
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(writer.finish_read_only(owner, transaction).is_ok());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn completed_lookup_cannot_publish_when_registration_fails() {
    use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
    use std::sync::mpsc::sync_channel;
    case("C2_STMT08", || {
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
            let _ = (status, undo, private);
            prepare_catalog(live, owner, flusher);
            let (mut writer, reader, _) = live.split();
            let transaction = writer
                .begin_transaction(
                    owner,
                    TransactionOptions {
                        read_only: true,
                        ..TransactionOptions::default()
                    },
                    reader_private,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &transaction).unwrap();
            let mut call = reader.catalog_statement_call(owner, &view).unwrap();
            let charge = reader.catalog_test_source_used();
            let before = state.lock().unwrap().images();
            let (arrived, arrival) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::LookupBeforeCatalogRecord, arrived, release);
            std::thread::scope(|scope| {
                let fencing = scope.spawn(move || {
                    arrival.recv_timeout(WAIT).unwrap();
                    fence.fence();
                    resume.send(Resume::Continue).unwrap();
                });
                let error = call
                    .lookup(
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .err()
                    .expect("completed Missing is withheld on registration failure");
                assert!(matches!(error.cause, RetentionCause::Fenced));
                assert_eq!(error.phase, RetentionPhase::Census);
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert_eq!(view.test_catalog_reads(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                let error = call.finish_error(error);
                assert!(matches!(error.cause, RetentionCause::Fenced));
                assert_eq!(observe(core).active_read_operations, 0);
                assert_eq!(reader.catalog_test_source_used(), charge);
                fencing.join().unwrap();
            });
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(fence.is_fenced());
        });
    });
}
