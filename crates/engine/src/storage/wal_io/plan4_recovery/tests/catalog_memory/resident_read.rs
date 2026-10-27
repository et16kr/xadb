//! The resident class uses the same product branch and actual original pages.
use super::*;
use crate::{
    storage::{dirty_frame::status_load_test_support, disk::RetentionCause},
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;

#[test]
fn resident_snapshot_visibility_has_no_io() {
    case("C2_RESIDENT01", || {
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
            let (mut writer, reader, _) = live.split();
            let old = reader.catalog_snapshot(owner).unwrap();
            assert_eq!(old.q(), 0);
            let mut cursor = Plan4ResidencyCursor::new();
            let before = state.lock().unwrap().events.len();
            assert!(matches!(
                reader
                    .lookup_catalog_resident(
                        owner,
                        &old,
                        ObjectId::new(6).unwrap(),
                        &mut cursor,
                        reader_private
                    )
                    .unwrap(),
                CatalogLookupStep::Missing
            ));
            assert_eq!(state.lock().unwrap().events.len(), before);
            writer.commit_catalog(owner, prepared).unwrap();
            let before = state.lock().unwrap().events.len();
            assert!(matches!(
                reader
                    .lookup_catalog_resident(
                        owner,
                        &old,
                        ObjectId::new(6).unwrap(),
                        &mut cursor,
                        reader_private
                    )
                    .unwrap(),
                CatalogLookupStep::Missing
            ));
            assert_eq!(state.lock().unwrap().events.len(), before);
            let current = reader.catalog_snapshot(owner).unwrap();
            assert_eq!(current.q(), 1);
            match reader
                .lookup_catalog_resident(
                    owner,
                    &current,
                    ObjectId::new(6).unwrap(),
                    &mut cursor,
                    reader_private,
                )
                .unwrap()
            {
                CatalogLookupStep::Found(record) => {
                    let header = record.header();
                    assert_eq!(
                        (
                            header.identity.object.get(),
                            header.identity.generation.get(),
                            header.identity.schema_epoch.get()
                        ),
                        (6, 1, 1)
                    );
                    assert!(!header.retired);
                    let crate::storage::catalog_record::CatalogDefinitionView::Table(table) =
                        record.definition()
                    else {
                        panic!("table definition");
                    };
                    assert_eq!(table.name(), "T");
                    assert_eq!(table.len(), 1);
                    let column = table.column(0).unwrap();
                    assert_eq!(
                        (column.name, column.data_type, column.nullable),
                        ("i", DataType::Int64, false)
                    );
                }
                _ => panic!("committed table is visible to q=1"),
            }
            assert_eq!(state.lock().unwrap().events.len(), before);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            current.release_checked().unwrap();
            old.release_checked().unwrap();
            owner.check_idle().unwrap();
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn nonresident_undo_returns_busy_before_full_retry() {
    case("C2_RESIDENT02", || {
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
            let _actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            super::read_boundary::evict_undo(live, flusher, undo);
            let before = state.lock().unwrap().events.len();
            let (writer, reader, _) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
            let mut cursor = Plan4ResidencyCursor::new();
            let error = reader
                .lookup_catalog_resident(
                    owner,
                    &snapshot,
                    ObjectId::new(6).unwrap(),
                    &mut cursor,
                    reader_private,
                )
                .err()
                .unwrap();
            assert!(matches!(error.cause, RetentionCause::Busy));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(state.lock().unwrap().events.len(), before);
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            owner.check_idle().unwrap();
            assert!(matches!(
                reader
                    .lookup_catalog(
                        owner,
                        &snapshot,
                        ObjectId::new(6).unwrap(),
                        &mut cursor,
                        reader_private
                    )
                    .unwrap(),
                CatalogLookupStep::Missing
            ));
            assert!(state.lock().unwrap().events.len() > before);
            assert_eq!(snapshot.q(), 0);
            assert_eq!(reader.catalog_test_source_used(), 0);
            snapshot.release_checked().unwrap();
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn two_resident_operations_hold_separate_fixed_charges() {
    case("C2_RESIDENT03", || {
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
            let before = state.lock().unwrap().events.len();
            let (writer, reader, _) = live.split();
            let reader_ref = &reader;
            let mut second_private = boxed::<PRIVATE_BYTES>();
            std::thread::scope(|scope| {
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, Point::LookupCaptured, entered, release);
                let first = scope.spawn(move || {
                    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let snapshot = reader_ref.catalog_snapshot(&mut owner).unwrap();
                    assert!(matches!(
                        reader_ref
                            .lookup_catalog_resident(
                                &mut owner,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private
                            )
                            .unwrap(),
                        CatalogLookupStep::Missing
                    ));
                    snapshot.release_checked().unwrap();
                    owner.check_idle().unwrap();
                });
                arrived.recv_timeout(WAIT).unwrap();
                let charged = reader.catalog_test_source_used();
                assert!(charged > 0);
                let (entered2, arrived2) = sync_channel(1);
                let (resume2, release2) = sync_channel(1);
                hook::install(core, Point::LookupCaptured, entered2, release2);
                let second = scope.spawn(move || {
                    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let snapshot = reader_ref.catalog_snapshot(&mut owner).unwrap();
                    assert!(matches!(
                        reader_ref
                            .lookup_catalog_resident(
                                &mut owner,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                &mut second_private
                            )
                            .unwrap(),
                        CatalogLookupStep::Missing
                    ));
                    snapshot.release_checked().unwrap();
                    owner.check_idle().unwrap();
                });
                arrived2.recv_timeout(WAIT).unwrap();
                assert_eq!(reader.catalog_test_source_used(), charged * 2);
                assert_eq!(hook::observe(core).active_read_operations, 2);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                resume.send(Resume::Continue).unwrap();
                first.join().unwrap();
                assert_eq!(reader.catalog_test_source_used(), charged);
                assert_eq!(hook::observe(core).active_read_operations, 1);
                resume2.send(Resume::Continue).unwrap();
                second.join().unwrap();
            });
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(hook::observe(core).active_read_operations, 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(state.lock().unwrap().events.len(), before);
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn resident_corruption_remains_fatal_without_io() {
    case("C2_RESIDENT04", || {
        for page in [9, 10] {
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
                let _actor = create_forward(
                    live, owner, flusher, status, undo, private, &oracle, state, 1,
                );
                let index = resident_index(live, page).unwrap();
                status_load_test_support::edit_content(live.catalog_test_pool(), index, |actual| {
                    actual[200] ^= 1
                });
                let before = state.lock().unwrap().events.len();
                let (_, reader, _) = live.split();
                let snapshot = reader.catalog_snapshot(owner).unwrap();
                let error = reader
                    .lookup_catalog_resident(
                        owner,
                        &snapshot,
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .err()
                    .unwrap();
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(!matches!(error.cause, RetentionCause::Busy));
                assert!(fence.is_fenced());
                assert!(reader.catalog_test_source_used() > 0);
                assert_eq!(state.lock().unwrap().events.len(), before);
            });
        }
    });
}
