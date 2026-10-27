//! The extracted product path over the original empty Catalog and live owner.
use super::*;

#[test]
fn missing_leaf_and_foreign_owner_release_original_resources() {
    case("C2_READ_EMPTY", || {
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
            let _ = (status, undo, private, core);
            prepare_catalog(live, owner, flusher);
            let before = state.lock().unwrap().images();
            let (writer, reader, _) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
            let mut foreign_owner = foreign.try_owner(OwnerRole::Foreground).unwrap();
            let mut cursor = Plan4ResidencyCursor::new();
            let error = match reader.lookup_catalog(
                &mut foreign_owner,
                &snapshot,
                ObjectId::new(6).unwrap(),
                &mut cursor,
                reader_private,
            ) {
                Err(error) => error,
                Ok(_) => panic!("foreign owner must be rejected before capture"),
            };
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            foreign_owner.check_idle().unwrap();
            for key in [6, 9] {
                assert!(matches!(
                    reader
                        .lookup_catalog(
                            owner,
                            &snapshot,
                            ObjectId::new(key).unwrap(),
                            &mut cursor,
                            reader_private
                        )
                        .unwrap(),
                    CatalogLookupStep::Missing
                ));
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                owner.check_idle().unwrap();
            }
            snapshot.release_checked().unwrap();
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
        });
    });
}

/// Normal flush and original clean-victim replacement make U nonresident.
pub(super) fn evict_undo<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    flusher: &mut Owner<'c, 's>,
    undo: &Plan4Target,
) {
    drive_flush(live, flusher, Some(undo));
    let index = match live
        .residency_step(undo, &mut Plan4ResidencyCursor::new())
        .unwrap()
    {
        RetentionResidencyStep::Resident { index, frame, .. } => {
            assert!(frame.rec_lsn.is_none());
            index
        }
        _ => panic!("clean original Undo"),
    };
    let jobs = live.target(Plan4Page::Jobs { index: 1 }).unwrap();
    assert!(live.load_into(index, &jobs).unwrap().before.is_some());
    assert_eq!(resident_index(live, 10), None);
}

#[test]
fn dirty_victim_returns_flush_then_retries_same_snapshot() {
    case("C2_READ_FLUSH", || {
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
            evict_undo(live, flusher, undo);
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
            let (writer, reader, mut flush) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
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
            let target = match reader
                .lookup_catalog(
                    owner,
                    &snapshot,
                    ObjectId::new(6).unwrap(),
                    &mut cursor,
                    reader_private,
                )
                .unwrap()
            {
                CatalogLookupStep::FlushRequired { target } => target,
                _ => panic!("all clean frames pinned: actual dirty victim required"),
            };
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), retained_pins);
            owner.check_idle().unwrap();
            assert_eq!(snapshot.q(), 0);
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
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(writer.catalog_test_physical_pins(), 0);
            assert_eq!(snapshot.q(), 0);
            snapshot.release_checked().unwrap();
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn captured_undo_read_error_preserves_fatal_cause() {
    case("C2_READ_ERROR", || {
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
            evict_undo(live, flusher, undo);
            let (_, reader, _) = live.split();
            let snapshot = reader.catalog_snapshot(owner).unwrap();
            {
                let mut io = state.lock().unwrap();
                io.events.clear();
                io.fault_hits = 0;
                io.fault = Some(port::Fault {
                    key: port::Key::new("read", HOME_NAME, 10 * PAGE_BYTES as u64, 1),
                    action: port::Action::BeforeError,
                });
            }
            let error = match reader.lookup_catalog(
                owner,
                &snapshot,
                ObjectId::new(6).unwrap(),
                &mut Plan4ResidencyCursor::new(),
                reader_private,
            ) {
                Err(error) => error,
                Ok(_) => panic!("actual captured Undo I/O error must not become a result"),
            };
            assert_eq!(state.lock().unwrap().fault_hits, 1);
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert_eq!(error.phase, crate::storage::disk::RetentionPhase::Load);
            assert!(
                matches!(error.cause, crate::storage::disk::RetentionCause::PageIo(_)),
                "{error:?}"
            );
            assert!(fence.is_fenced());
            // Fatal cleanup may preserve fenced registry evidence; never claim
            // successful ref release or a reusable healthy snapshot afterward.
            drop(snapshot);
        });
    });
}
