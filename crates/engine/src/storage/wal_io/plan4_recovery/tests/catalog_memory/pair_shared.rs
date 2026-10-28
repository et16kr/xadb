//! Actual finish/cancel return reopens ordinary original-pool admission.
use super::*;
use crate::storage::dirty_frame::{LoadClaimStep, init_test_support as init};

#[test]
fn pair_shared_actual_finish_and_cancel_reopen_ordinary_admission() {
    case("MPS01", || {
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
                let _ = (reader_private, core);
                prepare_create(live, owner, flusher, status, undo);
                let actor = live
                    .catalog_begin(owner, status, private)
                    .unwrap()
                    .actor
                    .unwrap();
                oracle.check(live, state, "R01");
                let definition = table_spec("T");
                let reserved = live
                    .reserve_catalog_create(owner, actor, undo, &definition, private)
                    .unwrap();
                oracle.check(reserved.catalog_test_bound(), state, "R02");
                if cancel {
                    match reserved.cancel() {
                        Ok(()) => {}
                        Err(RetentionReservationError::Before { error, .. })
                        | Err(RetentionReservationError::Fatal(error)) => {
                            panic!("actual Cancel {error:?}")
                        }
                    }
                    oracle.check(live, state, "K03");
                } else {
                    match reserved.initialize() {
                        Ok(_) => {}
                        Err(RetentionReservationError::Before { error, .. })
                        | Err(RetentionReservationError::Fatal(error)) => {
                            panic!("actual Forward {error:?}")
                        }
                    }
                    oracle.check(live, state, "R03");
                }
                assert_eq!(
                    init::pair_close_state(live.catalog_test_pool()),
                    (None, None, false, 0)
                );
                {
                    let pool = live.catalog_test_pool();
                    let allocator = pool.target_request(0).unwrap();
                    pool.reserve_target_set(owner, &[allocator])
                        .unwrap()
                        .try_read()
                        .unwrap()
                        .release_checked()
                        .unwrap();
                    let root = pool
                        .target_request(resident_index(live, 1).unwrap())
                        .unwrap();
                    assert!(matches!(
                        pool.claim_retention_load(owner, root, 1, &[]).unwrap(),
                        LoadClaimStep::Resident(_)
                    ));
                    pool.flush_preflight().unwrap();
                }
                // This is the actual selected-DWB flush, after claim retirement.
                drive_flush(live, flusher, None);
                let pool = live.catalog_test_pool();
                let allocator = pool.target_request(0).unwrap();
                let mut image = [0; PAGE_BYTES];
                assert!(
                    pool.capture_flush(flusher, allocator, &mut image)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(init::pair_close_state(pool), (None, None, false, 0));
                owner.check_idle().unwrap();
                flusher.check_idle().unwrap();
                assert!(!fence.is_fenced());
            });
        }
    });
}

#[test]
fn pair_shared_close_after_actual_dirty_selection_blocks_flush_hint() {
    case("MPS02", || {
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
            let actor = live
                .catalog_begin(owner, status, private)
                .unwrap()
                .actor
                .unwrap();
            oracle.check(live, state, "R01");
            let pool = live.catalog_test_pool();
            let existing = [0, 1, 2, 7, 8, 9, 10].map(|page| {
                pool.target_request(resident_index(live, page).unwrap())
                    .unwrap()
            });
            let mut pair = [existing[3]; 2];
            for (at, target) in pair.iter_mut().enumerate() {
                target.identity.page = page_ref(11 + at as u64, 2);
                target.identity.owner = ObjectId::new(6).unwrap();
                target.kind = if at == 0 {
                    crate::storage::page::PageKind::TreeAnchor
                } else {
                    crate::storage::page::PageKind::RowLeaf
                };
            }
            let selected = pool.catalog_pair_candidates(&pair, 1, &existing).unwrap();
            assert!(!selected.requires_flush());
            let root = resident_index(live, 1).unwrap();
            assert!(
                probe::actual_requirement(pool, root).is_some(),
                "actual Begin leaves a dirty Root with original WAL requirement"
            );
            let protected: Vec<_> = (1..9)
                .filter(|index| *index != root)
                .map(|index| pool.target_request(index).unwrap())
                .collect();
            assert!(matches!(
                pool.claim_retention_load(owner, pair[0], 1, &protected)
                    .unwrap(),
                LoadClaimStep::FlushRequired(_)
            ));
            let before_files = state.lock().unwrap().images();
            let before_wal = live.catalog_test_wal_state();
            let (entered, arrived) = std::sync::mpsc::sync_channel(1);
            let (resume, release) = std::sync::mpsc::sync_channel(1);
            init::install_init_pause(pool, init::InitPoint::LoadFlushSelected, entered, release);
            std::thread::scope(|scope| {
                let load = scope.spawn(|| {
                    let mut reader = core.try_owner(OwnerRole::Foreground).unwrap();
                    match pool.claim_retention_load(&mut reader, pair[0], 1, &protected) {
                        Err(error) => assert_eq!(
                            (error.cause, error.effect),
                            (crate::storage::dirty_frame::Cause::Busy, Effect::Unchanged)
                        ),
                        Ok(_) => panic!("dirty hint returned after the CREATE claim closed"),
                    }
                    reader.check_idle().unwrap();
                });
                arrived.recv_timeout(WAIT).unwrap();
                // Pool close only; no synthetic Reserve/Cancel/durability proof.
                let key =
                    init::close_pair_metadata_fixture(pool, actor, &existing, &pair, &selected)
                        .unwrap();
                let closed = (init::pair_close_state(pool), init::pair_gate_counters(pool));
                resume.send(probe::Resume::Continue).unwrap();
                load.join().unwrap();
                assert_eq!(
                    (init::pair_close_state(pool), init::pair_gate_counters(pool)),
                    closed
                );
                assert_eq!(live.catalog_test_wal_state(), before_wal);
                assert_eq!(state.lock().unwrap().images(), before_files);
                assert!(!fence.is_fenced());
                drop(key);
                assert!(fence.is_fenced(), "unresolved metadata close fences");
            });
        });
    });
}
