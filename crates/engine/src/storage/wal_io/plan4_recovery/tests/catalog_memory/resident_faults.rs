//! Adversarial allocator/status cuts of the new resident resolver. Re-encoded
//! corruption is injected only into actual original frames, never an authority.
use super::*;
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry},
        dirty_frame::status_load_test_support,
        disk::RetentionCause,
        page::{self, PageKind},
        status_home::{self, StatusHome},
        transaction_record::TransactionStatusCell,
    },
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;

fn resident<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    flusher: &mut Owner<'c, 's>,
    target: &Plan4Target,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        match live.residency_step(target, &mut cursor).unwrap() {
            RetentionResidencyStep::Resident { .. } => return,
            RetentionResidencyStep::FlushRequired { target, .. } => {
                drive_flush(live, flusher, Some(&target))
            }
        }
    }
    panic!("bounded residency exhausted");
}
fn additional_status<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) -> Plan4Target {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        match live.prepare_status_slot(owner, &mut cursor).unwrap() {
            None => {
                let reserved = live.reserve_status(owner).unwrap();
                assert!(reserved.page().page_no > 10);
                return match reserved.initialize() {
                    Ok(target) => target,
                    Err(RetentionReservationError::Before { error, .. })
                    | Err(RetentionReservationError::Fatal(error)) => {
                        panic!("actual second Status INIT: {error:?}")
                    }
                };
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected preparation"),
        }
    }
    panic!("bounded second Status preparation exhausted");
}
fn evict<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    flusher: &mut Owner<'c, 's>,
    target: &Plan4Target,
) {
    resident(live, flusher, target);
    drive_flush(live, flusher, Some(target));
    let index = match live
        .residency_step(target, &mut Plan4ResidencyCursor::new())
        .unwrap()
    {
        RetentionResidencyStep::Resident { index, frame, .. } => {
            assert!(frame.rec_lsn.is_none());
            index
        }
        _ => panic!("resident victim"),
    };
    let jobs = (1..5)
        .map(|index| live.target(Plan4Page::Jobs { index }).unwrap())
        .find(|t| resident_index(live, t.identity().page.page_no).is_none())
        .unwrap();
    assert!(live.load_into(index, &jobs).unwrap().before.is_some());
    assert!(resident_index(live, target.identity().page.page_no).is_none());
}

#[test]
fn nonresident_status_and_later_status_prevent_partial_success() {
    case("C2_RESIDENT05", || {
        for later in [false, true] {
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
                let missing = if later {
                    additional_status(live, owner, flusher)
                } else {
                    *status
                };
                prepare_catalog(live, owner, flusher);
                resident(live, flusher, status);
                resident(live, flusher, undo);
                evict(live, flusher, &missing);
                if later {
                    assert!(resident_index(live, status.identity().page.page_no).is_some());
                }
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
                // Full observes the original canonical history, including the
                // later free Status; same snapshot and exact Missing outcome.
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
                snapshot.release_checked().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            });
        }
    });
}

#[test]
fn checksummed_status_absence_and_duplicate_are_fatal() {
    case("C2_RESIDENT06", || {
        for duplicate in [false, true] {
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
                let target = if duplicate {
                    additional_status(live, owner, flusher)
                } else {
                    *status
                };
                prepare_catalog(live, owner, flusher);
                resident(live, flusher, status);
                resident(live, flusher, undo);
                resident(live, flusher, &target);
                let mut original = boxed::<PAGE_BYTES>();
                probe::diagnose(
                    live.catalog_test_pool(),
                    resident_index(live, status.identity().page.page_no).unwrap(),
                    &mut original,
                );
                let namespace = super::super::support::namespace();
                let original = status_home::decode_status_home_page(
                    original.as_slice(),
                    status.identity(),
                    namespace,
                )
                .unwrap();
                let home = if duplicate {
                    original.cell(0).unwrap()
                } else {
                    StatusHome::new(TransactionStatusCell::Free(Generation::new(1).unwrap()), 0)
                        .unwrap()
                };
                let index = resident_index(live, target.identity().page.page_no).unwrap();
                status_load_test_support::edit_content(live.catalog_test_pool(), index, |bytes| {
                    let header =
                        page::decode_page(bytes, target.identity(), PageKind::TransactionStatus)
                            .unwrap()
                            .header();
                    let mut cell = [0; 224];
                    home.encode(&mut cell).unwrap();
                    page::encode_page(header, &[&cell], bytes).unwrap();
                    status_home::decode_status_home_page(bytes, target.identity(), namespace)
                        .unwrap();
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
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(fence.is_fenced());
                assert!(reader.catalog_test_source_used() > 0);
                assert_eq!(state.lock().unwrap().events.len(), before);
            });
        }
    });
}

#[test]
fn allocator_flush_after_copy_is_retryable_and_releases_charge() {
    case("C2_RESIDENT07", || {
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
            let mut before_bytes = boxed::<PAGE_BYTES>();
            let old = probe::diagnose(live.catalog_test_pool(), 0, &mut before_bytes);
            assert!(old.frame.rec_lsn.is_some());
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::ResidentAllocatorCopied, entered, release);
            let (writer, reader, mut flush) = live.split();
            let reader_ref = &reader;
            std::thread::scope(|scope| {
                let reading = scope.spawn(move || {
                    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let snapshot = reader_ref.catalog_snapshot(&mut owner).unwrap();
                    let error = reader_ref
                        .lookup_catalog_resident(
                            &mut owner,
                            &snapshot,
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private,
                        )
                        .err()
                        .unwrap();
                    assert!(matches!(error.cause, RetentionCause::Busy));
                    assert_eq!(error.effect, Effect::Unchanged);
                    snapshot.release_checked().unwrap();
                    owner.check_idle().unwrap();
                });
                arrived.recv_timeout(WAIT).unwrap();
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                let mut step = flush.try_flush_allocator(flusher).unwrap();
                for _ in 0..8 {
                    if matches!(
                        step,
                        RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
                    ) {
                        break;
                    }
                    step = flush.continue_flush(flusher).unwrap();
                }
                assert!(matches!(step, RetentionFlushStep::Complete { .. }));
                let mut after_bytes = boxed::<PAGE_BYTES>();
                let new = probe::diagnose(writer.catalog_test_pool(), 0, &mut after_bytes);
                assert_eq!(before_bytes, after_bytes);
                assert_eq!(new.frame.rec_lsn, None);
                assert_ne!(new.frame, old.frame);
                let events = state.lock().unwrap().events.len();
                resume.send(Resume::Continue).unwrap();
                reading.join().unwrap();
                assert_eq!(state.lock().unwrap().events.len(), events);
            });
            assert_eq!(reader.catalog_test_source_used(), 0);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(hook::observe(core).active_read_operations, 0);
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            assert!(!fence.is_fenced());
        });
    });
}

fn corrupt_membership(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    identity: crate::storage::page::PageIdentity,
    namespace: crate::api::Namespace,
    number: usize,
) {
    status_load_test_support::edit_content(pool, 0, |bytes| {
        let (header, mut entries) = {
            let view = allocation_page::decode_allocation_page(bytes, identity, namespace).unwrap();
            (
                view.header(),
                std::array::from_fn::<_, 64, _>(|index| view.entry(index).unwrap()),
            )
        };
        let AllocationEntry::Allocated { generation, .. } = entries[number] else {
            panic!("actual allocated target");
        };
        entries[number] = AllocationEntry::Free(generation);
        let mut cell = [0; allocation_page::ALLOCATION_CELL_BYTES];
        allocation_page::encode_allocation_cell(header, namespace, &entries, &mut cell).unwrap();
        allocation_page::encode_allocation_page(header, namespace, &cell, bytes).unwrap();
        allocation_page::decode_allocation_page(bytes, identity, namespace).unwrap();
    });
}

#[test]
fn selected_membership_corruption_is_not_busy() {
    case("C2_RESIDENT08", || {
        for cut in [
            None,
            Some(Point::ResidentStatusLocated),
            Some(Point::ResidentUndoLocated),
        ] {
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
                let number = match cut {
                    None => 8,
                    Some(Point::ResidentStatusLocated) => status.identity().page.page_no as usize,
                    Some(Point::ResidentUndoLocated) => undo.identity().page.page_no as usize,
                    _ => unreachable!(),
                };
                let namespace = super::super::support::namespace();
                let allocator = live.catalog_test_pool().target_request(0).unwrap();
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                if let Some(point) = cut {
                    hook::install(core, point, entered, release);
                } else {
                    corrupt_membership(
                        live.catalog_test_pool(),
                        allocator.identity,
                        namespace,
                        number,
                    );
                }
                let before = state.lock().unwrap().events.len();
                let (writer, reader, _) = live.split();
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut owner).unwrap();
                        let error = reader_ref
                            .lookup_catalog_resident(
                                &mut owner,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                reader_private,
                            )
                            .err()
                            .unwrap();
                        assert!(matches!(error.cause, RetentionCause::Invariant));
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                    });
                    if cut.is_some() {
                        arrived.recv_timeout(WAIT).unwrap();
                        assert_eq!(writer.catalog_test_physical_pins(), 0);
                        corrupt_membership(
                            writer.catalog_test_pool(),
                            allocator.identity,
                            namespace,
                            number,
                        );
                        resume.send(Resume::Continue).unwrap();
                    }
                    reading.join().unwrap();
                });
                assert!(fence.is_fenced());
                assert!(reader.catalog_test_source_used() > 0);
                assert_eq!(state.lock().unwrap().events.len(), before);
            });
        }
    });
}
