//! Original writable statement reads its registered CREATE, including Undo.
use super::*;
use crate::storage::disk::{
    IsolationLevel, RetentionCause, TransactionFinishError, TransactionOptions,
};
use crate::transaction::concurrency::catalog_test_support as hook;

#[test]
fn original_own_create_and_other_snapshot_keep_distinct_visibility() {
    case("OCR01", || {
        for mode in [IsolationLevel::Serializable, IsolationLevel::ReadCommitted] {
            for resident in [false, true] {
                for cancel in [false, true] {
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
                        let _ = state;
                        prepare_create(live, owner, flusher, status, undo);
                        let (mut writer, reader, mut flush) = live.split();
                        let observer = writer
                            .begin_transaction(
                                owner,
                                TransactionOptions {
                                    isolation: mode,
                                    read_only: true,
                                },
                                reader_private,
                            )
                            .unwrap();
                        let observer_view = writer.begin_statement(owner, &observer).unwrap();
                        let tx = writer
                            .begin_catalog_transaction(owner, mode, status, private, &mut flush)
                            .unwrap();
                        let view = writer.begin_statement(owner, &tx).unwrap();
                        let definition = table_spec("OWN_T");
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let reserved = writer
                            .reserve_catalog_create(
                                owner,
                                &view,
                                undo,
                                &definition,
                                private,
                                &mut flush,
                            )
                            .ok()
                            .unwrap();
                        let error = reader
                            .catalog_statement_call(&mut late, &view)
                            .err()
                            .unwrap();
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(hook::observe(core).read_gate_closed);
                        let reserved = if cancel {
                            let private = reserved.cancel().ok().unwrap().into_private();
                            writer
                                .reserve_catalog_create(
                                    owner,
                                    &view,
                                    undo,
                                    &definition,
                                    private,
                                    &mut flush,
                                )
                                .ok()
                                .unwrap()
                        } else {
                            reserved
                        };
                        let (object, private) = reserved.initialize().ok().unwrap().into_parts();
                        assert_eq!(object.object.get(), if cancel { 7 } else { 6 });
                        let before = writer.free_undo_test_state().unwrap().0;
                        let mut call = if resident {
                            reader.catalog_resident_statement_call(owner, &view)
                        } else {
                            reader.catalog_statement_call(owner, &view)
                        }
                        .unwrap();
                        match call
                            .lookup(
                                object.object,
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                            .unwrap()
                        {
                            CatalogLookupStep::Found(record) => {
                                assert_eq!(record.header().identity, object);
                                assert_eq!(
                                    record.header().stamp,
                                    crate::storage::transaction_record::VersionStamp::Transaction(
                                        tx.test_write_identity().0.unwrap()
                                    )
                                );
                                let crate::storage::catalog_record::CatalogDefinitionView::Table(
                                    table,
                                ) = record.definition()
                                else {
                                    panic!("table")
                                };
                                assert_eq!(table.name(), "OWN_T");
                            }
                            _ => panic!("registered own CREATE must be Found at Q0"),
                        }
                        call.finish_checked().unwrap();
                        assert_eq!(
                            (
                                view.q().unwrap(),
                                view.revision(),
                                view.test_catalog_reads()
                            ),
                            (0, 1, 3)
                        );
                        let mut other = reader
                            .catalog_statement_call(owner, &observer_view)
                            .unwrap();
                        assert!(matches!(
                            other
                                .lookup(
                                    object.object,
                                    &mut Plan4ResidencyCursor::new(),
                                    reader_private
                                )
                                .unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        other.finish_checked().unwrap();
                        assert_eq!(writer.free_undo_test_state().unwrap().0, before);
                        assert_eq!(reader.catalog_test_source_used(), 0);
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        assert!(writer.finish_statement(owner, view).is_ok());
                        assert_eq!(
                            writer
                                .commit_catalog_transaction(owner, tx, private)
                                .ok()
                                .unwrap()
                                .q(),
                            1
                        );
                        let mut other = reader
                            .catalog_statement_call(owner, &observer_view)
                            .unwrap();
                        assert!(matches!(
                            other
                                .lookup(
                                    object.object,
                                    &mut Plan4ResidencyCursor::new(),
                                    reader_private
                                )
                                .unwrap(),
                            CatalogLookupStep::Missing
                        ));
                        other.finish_checked().unwrap();
                        assert!(writer.finish_statement(owner, observer_view).is_ok());
                        let next = writer.begin_statement(owner, &observer).unwrap();
                        let mut other = reader.catalog_statement_call(owner, &next).unwrap();
                        let found = matches!(
                            other
                                .lookup(
                                    object.object,
                                    &mut Plan4ResidencyCursor::new(),
                                    reader_private
                                )
                                .unwrap(),
                            CatalogLookupStep::Found(_)
                        );
                        assert_eq!(found, mode == IsolationLevel::ReadCommitted);
                        other.finish_checked().unwrap();
                        assert!(writer.finish_statement(owner, next).is_ok());
                        let end = writer.finish_read_only(owner, observer);
                        assert!(if mode == IsolationLevel::Serializable {
                            matches!(end, Err(TransactionFinishError::Rejected(error)) if matches!(error.cause, RetentionCause::SerializationConflict))
                        } else {
                            end.is_ok()
                        });
                        assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                        assert!(!fence.is_fenced());
                    });
                }
            }
        }
    });
}

#[test]
fn unregistered_own_forward_and_corrupt_view_never_fall_back() {
    case("OCR02", || {
        for fault in 0..4 {
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
                if fault == 0 {
                    writer.catalog_test_forward_beside_capsule(
                        owner,
                        tx.test_write_identity().0.unwrap(),
                        undo,
                        &table_spec("T"),
                        private,
                        &mut flush,
                    );
                } else if fault < 3 {
                    tx.test_corrupt_catalog_completion(fault == 2);
                } else {
                    // Canonical mode value, wrong original snapshot shape.
                    tx.test_flip_catalog_mode();
                }
                let before = state.lock().unwrap().images();
                let error = match reader.catalog_statement_call(owner, &view) {
                    Ok(mut call) => {
                        let error = call
                            .lookup(
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                            .err()
                            .unwrap();
                        call.finish_error(error)
                    }
                    Err(error) => error,
                };
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(hook::observe(core).active_read_operations, 0);
            });
        }
    });
}

#[test]
fn actual_own_status_head_and_terminal_contradictions_are_fatal() {
    use crate::storage::{
        dirty_frame::status_load_test_support,
        page,
        status_home::{STATUS_HOME_BYTES, StatusHome},
        transaction_record::{TransactionState, TransactionStatus, TransactionStatusCell},
    };
    case("OCR03", || {
        for terminal in [false, true] {
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
                let definition = table_spec("T");
                let (object, _private) = writer
                    .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                    .ok()
                    .unwrap()
                    .initialize()
                    .ok()
                    .unwrap()
                    .into_parts();
                let index = resident_index(&writer, 9).unwrap();
                let request = reader.catalog_test_pool().target_request(index).unwrap();
                status_load_test_support::edit_content(
                    reader.catalog_test_pool(),
                    index,
                    |actual| {
                        let decoded = page::decode_page(
                            actual,
                            request.identity,
                            page::PageKind::TransactionStatus,
                        )
                        .unwrap();
                        let cell = decoded.cell(0).unwrap();
                        let offset = cell.as_ptr() as usize - actual.as_ptr() as usize;
                        let home = StatusHome::decode(cell).unwrap();
                        let TransactionStatusCell::Occupied(status) = home.core() else {
                            panic!("actual occupied")
                        };
                        let changed = TransactionStatus::new(
                            status.reference(),
                            if terminal {
                                TransactionState::Aborted
                            } else {
                                TransactionState::Active
                            },
                            if terminal { status.undo_head() } else { None },
                        )
                        .unwrap();
                        StatusHome::new(TransactionStatusCell::Occupied(changed), home.incoming())
                            .unwrap()
                            .encode(&mut actual[offset..offset + STATUS_HOME_BYTES])
                            .unwrap();
                        let digest = content_digest(&actual[..page::DIGEST_OFFSET]);
                        actual[page::DIGEST_OFFSET..].copy_from_slice(&digest);
                    },
                );
                let before = state.lock().unwrap().images();
                let mut call = reader
                    .catalog_resident_statement_call(owner, &view)
                    .unwrap();
                let error = call
                    .lookup(
                        object.object,
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert_eq!(error.effect, Effect::CallerMustResolve);
                let error = call.finish_error(error);
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(hook::observe(core).active_read_operations, 0);
            });
        }
    });
}

#[test]
fn own_found_requires_actual_undo_target_and_creator() {
    use crate::storage::{dirty_frame::status_load_test_support, page, undo_record};
    case("OCR04", || {
        for creator in [false, true] {
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
                let definition = table_spec("T");
                let (object, _private) = writer
                    .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                    .ok()
                    .unwrap()
                    .initialize()
                    .ok()
                    .unwrap()
                    .into_parts();
                let index = resident_index(&writer, 10).unwrap();
                status_load_test_support::edit_content(
                    reader.catalog_test_pool(),
                    index,
                    |actual| {
                        // XUR1 fixed core: creator generation at64, target entity at112.
                        // Keep the page digest valid so the actual semantic source is checked.
                        let offset =
                            undo_record::INITIAL_UNDO_CELL.start + if creator { 64 } else { 112 };
                        let original = u64_at(actual, offset);
                        assert!(original > 0);
                        actual[offset..offset + 8]
                            .copy_from_slice(&original.checked_add(1).unwrap().to_le_bytes());
                        let digest = content_digest(&actual[..page::DIGEST_OFFSET]);
                        actual[page::DIGEST_OFFSET..].copy_from_slice(&digest);
                    },
                );
                let before = state.lock().unwrap().images();
                let mut call = reader
                    .catalog_resident_statement_call(owner, &view)
                    .unwrap();
                let error = call
                    .lookup(
                        object.object,
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .err()
                    .unwrap();
                assert!(if creator {
                    matches!(error.cause, RetentionCause::Invariant)
                } else {
                    matches!(
                        error.cause,
                        RetentionCause::Undo(undo_record::UndoError::IdentityMismatch)
                    )
                });
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert_eq!(call.finish_error(error).effect, Effect::CallerMustResolve);
                assert!(fence.is_fenced());
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(hook::observe(core).active_read_operations, 0);
            });
        }
    });
}
