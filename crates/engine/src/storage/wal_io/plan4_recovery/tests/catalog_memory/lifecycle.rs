//! Separate selectors for new actual CREATE cancel/CLR/drop consumers.
use super::*;

#[test]
fn mc01_reserve_cancel_two_generations() {
    case("MC01", || {
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
            for cycle in 0..2 {
                prepare_create(live, owner, flusher, status, undo);
                let at = cycle * 6 + 1;
                let id = |offset| format!("K{:02}", at + offset);
                let expected = oracle.actor(&id(0));
                let begin = live.catalog_begin(owner, status, private).unwrap();
                assert_eq!(begin.actor, Some(expected));
                oracle.check(live, state, &id(0));
                let definition = table_spec(if cycle == 0 { "T" } else { "U" });
                let reserved = live
                    .reserve_catalog_create(owner, expected, undo, &definition, private)
                    .unwrap();
                oracle.check(reserved.catalog_test_bound(), state, &id(1));
                assert_eq!(
                    reserved.catalog_test_bound().catalog_test_epochs(),
                    (true, 0, vec![(1, 6 + cycle as u64, 0)])
                );
                match reserved.cancel() {
                    Ok(()) => {}
                    Err(RetentionReservationError::Before { error, .. })
                    | Err(RetentionReservationError::Fatal(error)) => {
                        panic!("actual Cancel {error:?}")
                    }
                }
                oracle.check(live, state, &id(2));
                assert_eq!(live.catalog_test_epochs(), (true, 0, vec![]));
                live.catalog_rollback_start(owner, expected, private)
                    .unwrap();
                oracle.check(live, state, &id(3));
                live.catalog_abort_end(owner, expected, private).unwrap();
                oracle.check(live, state, &id(4));
                live.catalog_retire(
                    owner,
                    oracle.ordinal(&id(5)),
                    Operation::TerminalFinish,
                    private,
                )
                .unwrap();
                oracle.check(live, state, &id(5));
                query(
                    live,
                    owner,
                    flusher,
                    reader_private,
                    0,
                    6 + cycle as u64,
                    false,
                );
                assert!(!fence.is_fenced());
            }
            assert!(
                live.recovery_test_bank()[..32 * 1664]
                    .iter()
                    .all(|b| *b == 0)
            );
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn mc02_forward_clr_abort_cleanup() {
    case("MC02", || {
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
            let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
            {
                let (mut writer, _, mut flush) = live.split();
                writer
                    .catalog_rollback_start(&mut maintenance, actor, private, &mut flush)
                    .unwrap();
            }
            oracle.check(live, state, "R04");
            retirement_prepare::retire(live, &mut maintenance, 1, Operation::Clr, private);
            oracle.check(live, state, "R05");
            assert_eq!(live.catalog_test_epochs(), (true, 0, vec![]));
            assert_eq!(resident_index(live, 11), None);
            assert_eq!(resident_index(live, 12), None);
            {
                let (mut writer, _, mut flush) = live.split();
                writer
                    .catalog_abort_end(&mut maintenance, actor, private, &mut flush)
                    .unwrap();
            }
            oracle.check(live, state, "R06");
            for id in ["R07", "R08", "R09"] {
                {
                    let (mut writer, _, mut flush) = live.split();
                    writer
                        .catalog_release_role(&mut maintenance, 1, private, &mut flush)
                        .unwrap();
                }
                oracle.check(live, state, id);
            }
            retirement_prepare::retire(live, &mut maintenance, 1, Operation::RootFree, private);
            oracle.check(live, state, "R10");
            retirement_prepare::retire(
                live,
                &mut maintenance,
                0,
                Operation::TerminalFinish,
                private,
            );
            oracle.check(live, state, "R11");
            maintenance.check_idle().unwrap();
            assert_eq!(live.catalog_test_epochs(), (true, 0, vec![]));
            query(live, owner, flusher, reader_private, 0, 6, false);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn mc04_reserved_drop_fence_fresh_recovery() {
    case("MC04", || {
        let oracle = Oracle::new();
        let images = with_fresh!(oracle, |live,
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
            let begin = live.catalog_begin(owner, status, private).unwrap();
            assert_eq!(begin.actor, Some(oracle.actor("K01")));
            oracle.check(live, state, "K01");
            let definition = table_spec("T");
            let reserved = live
                .reserve_catalog_create(owner, oracle.actor("K01"), undo, &definition, private)
                .unwrap();
            oracle.check(reserved.catalog_test_bound(), state, "K02");
            let before = state.lock().unwrap().images();
            drop(reserved);
            assert!(fence.is_fenced());
            assert_eq!(
                state.lock().unwrap().images(),
                before,
                "Drop emitted bytes or Cancel"
            );
            let (prefix, first_write) = live.catalog_test_wal_state();
            assert_eq!(
                (prefix.complete_lsn, prefix.synced_lsn, first_write),
                (7, 7, 7)
            );
            assert_eq!(live.catalog_test_q(), 0);
            before
        });
        recover_check(&images, &oracle, "MC04");
    });
}

#[test]
fn empty_catalog_commit_is_rejected_without_epoch_change() {
    case("EPOCH_EMPTY", || {
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
            let _ = (reader_private, core, state);
            prepare_create(live, owner, flusher, status, undo);
            let actor = live
                .catalog_begin(owner, status, private)
                .unwrap()
                .actor
                .unwrap();
            assert_eq!(live.catalog_test_epochs(), (true, 0, vec![]));
            // The existing Catalog producer requires a retained CREATE undo.
            // Queue-only Commit decoding is not a live empty-Commit capability.
            let before_wal = live.catalog_test_wal_state();
            let before_bank = content_digest(live.recovery_test_bank());
            let error = match live.prepare_catalog_commit(owner, actor, private) {
                Ok(_) => panic!("empty Catalog Commit unexpectedly prepared"),
                Err(error) => error,
            };
            assert!(matches!(
                error.cause,
                super::super::super::super::plan4_pages::RetentionCause::CatalogSemantic(
                    crate::storage::semantic_catalog::Error::InvalidInput
                )
            ));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(live.catalog_test_wal_state(), before_wal);
            assert_eq!(content_digest(live.recovery_test_bank()), before_bank);
            assert_eq!(live.catalog_test_q(), 0);
            assert_eq!(live.catalog_test_epochs(), (true, 0, vec![]));
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}
