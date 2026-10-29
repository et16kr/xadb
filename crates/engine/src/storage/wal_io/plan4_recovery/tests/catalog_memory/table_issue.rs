//! The only CheckedTable issuer: a statement call's actual Found Table, its
//! Published epoch slot and the automatic table read bit (ROW-BEGIN-001).
use super::*;
use crate::storage::{
    catalog_record::CatalogDefinitionView,
    disk::{
        IsolationLevel, RetentionCause, RetentionPhase, TableStep, TransactionFinishError,
        TransactionOptions,
    },
    page::PageIdentity,
};
use crate::transaction::concurrency::catalog_test_support::observe;

fn read_only() -> TransactionOptions {
    TransactionOptions {
        isolation: IsolationLevel::Serializable,
        read_only: true,
    }
}

#[test]
fn committed_table_issues_only_after_read_bit_and_catalog_flag() {
    use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
    use std::sync::mpsc::sync_channel;
    for cut in [false, true] {
        case(if cut { "S1F_T01_CUT" } else { "S1F_T01" }, || {
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
                {
                    let (mut writer, _, _) = live.split();
                    writer.commit_catalog(owner, prepared).unwrap();
                }
                let database = live
                    .target(Plan4Page::CatalogLeaf)
                    .unwrap()
                    .identity()
                    .database;
                let before = state.lock().unwrap().images();
                let (mut writer, reader, _) = live.split();
                let transaction = writer
                    .begin_transaction(owner, read_only(), reader_private)
                    .unwrap();
                let view = writer.begin_statement(owner, &transaction).unwrap();
                let call = reader.catalog_statement_call(owner, &view).unwrap();
                let charge = reader.catalog_test_source_used();
                assert!(charge > 0);
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (0, (0, 0))
                );
                if cut {
                    // Exact cut after the released publication slot check and
                    // before the isolation registration: no Table escapes.
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
                            .finish_table(
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                            .err()
                            .expect("an unregistered Table is withheld");
                        assert!(matches!(error.cause, RetentionCause::Fenced));
                        assert_eq!(error.phase, RetentionPhase::Census);
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        fencing.join().unwrap();
                    });
                    assert_eq!(
                        (view.test_catalog_reads(), view.test_table_masks()),
                        (0, (0, 0))
                    );
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(observe(core).active_read_operations, 0);
                    assert_eq!(reader.catalog_test_source_used(), charge);
                    assert_eq!(state.lock().unwrap().images(), before);
                    assert!(fence.is_fenced());
                } else {
                    let step = call
                        .finish_table(
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private,
                        )
                        .unwrap();
                    // Both registrations precede the value reaching this caller.
                    assert_eq!(
                        (view.test_catalog_reads(), view.test_table_masks()),
                        (1, (1, 0))
                    );
                    let TableStep::Table(table) = step else {
                        panic!("a committed visible Table issues")
                    };
                    let (handle, slot, anchor) = table.test_identity();
                    assert_eq!(
                        (
                            handle.object.get(),
                            handle.generation.get(),
                            handle.schema_epoch.get(),
                            slot
                        ),
                        (6, 1, 1, 0)
                    );
                    assert!(table.test_same_origin(core, fence));
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert_eq!(observe(core).active_read_operations, 0);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    // The same statement's ordinary lookup names the same record.
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
                    assert!(matches!(
                        record.definition(),
                        CatalogDefinitionView::Table(_)
                    ));
                    let header = record.header();
                    assert_eq!(header.identity, handle);
                    assert_eq!(
                        anchor,
                        PageIdentity {
                            database,
                            page: header.anchor,
                            owner: handle.object,
                            owner_generation: handle.generation,
                        }
                    );
                    call.finish_checked().unwrap();
                    assert_eq!(
                        (view.test_catalog_reads(), view.test_table_masks()),
                        (1, (1, 0))
                    );
                    assert!(writer.finish_statement(owner, view).is_ok());
                    // Published table epoch 1 <= start 1: the stored bit validates.
                    assert!(writer.finish_read_only(owner, transaction).is_ok());
                    assert_eq!(state.lock().unwrap().images(), before);
                    assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
                    assert!(!fence.is_fenced());
                }
            });
        });
    }
}

/// Retired records are not reachable here: no DROP producer exists, epoch
/// seeding refuses one, and lookup selects a visible retired record as the
/// same Missing exercised below. A non-Table Found has no actual producer
/// (semantic Catalog refuses Index); its refusal is the shared classifier
/// covered by statement::tests::found_non_table_definition_is_unsupported_before_issue.
#[test]
fn missing_object_registers_only_the_catalog_flag() {
    case("S1F_T02", || {
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
                .begin_transaction(&mut reader_owner, read_only(), reader_private)
                .unwrap();
            let view = writer
                .begin_statement(&mut reader_owner, &transaction)
                .unwrap();
            // Never created; created by a foreign uncommitted CREATE at Q0.
            for object in [99, 6] {
                let call = reader
                    .catalog_statement_call(&mut reader_owner, &view)
                    .unwrap();
                assert!(matches!(
                    call.finish_table(
                        ObjectId::new(object).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private
                    )
                    .unwrap(),
                    TableStep::Missing
                ));
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (1, (0, 0))
                );
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(observe(core).active_read_operations, 0);
            }
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            // The flag outlives the statement and is a real dependency.
            drop(view);
            writer.commit_catalog(owner, prepared).unwrap();
            let after_commit = state.lock().unwrap().images();
            match writer.finish_read_only(&mut reader_owner, transaction) {
                Err(TransactionFinishError::Rejected(error)) => {
                    assert!(matches!(error.cause, RetentionCause::SerializationConflict));
                    assert_eq!(error.phase, RetentionPhase::Finalize);
                    assert_eq!(error.effect, Effect::Unchanged);
                }
                _ => panic!("an observed catalog absence must conflict"),
            }
            assert_eq!(state.lock().unwrap().images(), after_commit);
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn own_reserved_create_slot_is_unsupported_until_published() {
    case("S1F_T03", || {
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
            let definition = table_spec("OWN_T");
            let reserved = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap();
            let (object, private) = reserved.initialize().ok().unwrap().into_parts();
            assert_eq!(object.object.get(), 6);
            // CREATE planning already registered its conservative flags.
            assert_eq!(
                (view.test_catalog_reads(), view.test_table_masks()),
                (3, (0, 0))
            );
            let call = reader.catalog_statement_call(owner, &view).unwrap();
            let error = call
                .finish_table(
                    object.object,
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                )
                .err()
                .expect("own uncommitted CREATE has only a Reserved slot");
            assert!(matches!(error.cause, RetentionCause::Unsupported));
            assert_eq!(error.phase, RetentionPhase::Census);
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(
                (view.test_catalog_reads(), view.test_table_masks()),
                (3, (0, 0))
            );
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(observe(core).active_read_operations, 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert!(!fence.is_fenced());
            assert!(writer.finish_statement(owner, view).is_ok());
            assert_eq!(
                writer
                    .commit_catalog_transaction(owner, tx, private)
                    .ok()
                    .unwrap()
                    .q(),
                1
            );
            // Commit published the slot: the same object now issues.
            let observer = writer
                .begin_transaction(owner, read_only(), reader_private)
                .unwrap();
            let next = writer.begin_statement(owner, &observer).unwrap();
            let call = reader.catalog_statement_call(owner, &next).unwrap();
            let TableStep::Table(table) = call
                .finish_table(
                    object.object,
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                )
                .unwrap()
            else {
                panic!("published own CREATE issues after Commit")
            };
            assert_eq!(table.test_identity().0, object);
            assert_eq!(table.test_identity().1, 0);
            assert_eq!(
                (next.test_catalog_reads(), next.test_table_masks()),
                (1, (1, 0))
            );
            assert!(writer.finish_statement(owner, next).is_ok());
            assert!(writer.finish_read_only(owner, observer).is_ok());
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn flush_required_carries_the_lookup_target_and_registers_nothing() {
    case("S1F_T04", || {
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
                .begin_transaction(owner, read_only(), reader_private)
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
            // The ordinary statement lookup names the actual dirty victim.
            let mut call = reader.catalog_statement_call(owner, &view).unwrap();
            let expected = match call
                .lookup(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                )
                .unwrap()
            {
                CatalogLookupStep::FlushRequired { target } => target,
                _ => panic!("all clean frames pinned: actual dirty victim required"),
            };
            call.finish_checked().unwrap();
            let mut cursor = Plan4ResidencyCursor::new();
            let call = reader.catalog_statement_call(owner, &view).unwrap();
            let target = match call
                .finish_table(ObjectId::new(6).unwrap(), &mut cursor, reader_private)
                .unwrap()
            {
                TableStep::FlushRequired { target } => target,
                _ => panic!("the issuer forwards the lookup's FlushRequired"),
            };
            assert_eq!(target, expected);
            assert_eq!(
                (view.test_catalog_reads(), view.test_table_masks()),
                (0, (0, 0))
            );
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), retained_pins);
            owner.check_idle().unwrap();
            assert_eq!(view.q().unwrap(), 0);
            for guard in guards {
                guard.release_checked().unwrap();
            }
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
            // The completed retry at the same snapshot registers its result.
            let call = reader.catalog_statement_call(owner, &view).unwrap();
            assert!(matches!(
                call.finish_table(ObjectId::new(6).unwrap(), &mut cursor, reader_private)
                    .unwrap(),
                TableStep::Missing
            ));
            assert_eq!(
                (view.test_catalog_reads(), view.test_table_masks()),
                (1, (0, 0))
            );
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(writer.finish_read_only(owner, transaction).is_ok());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
            assert!(!fence.is_fenced());
        });
    });
}
