//! An actual Begin preflight consumes an issued local without publishing it.
use super::*;

#[test]
fn mc03_issued_local_gap_recovers() {
    case("MC03", || {
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
            let index = resident_index(live, 1).unwrap();
            let mut bytes = boxed::<PAGE_BYTES>();
            let original = probe::diagnose(live.catalog_test_pool(), index, &mut bytes);
            let mut max_entry = original.entry;
            if let Some(entry) = &mut max_entry {
                entry.modification_counter = u64::MAX;
            }
            probe::seed_counter(live.catalog_test_pool(), index, u64::MAX);
            probe::seed_dpt(live.catalog_test_pool(), index, max_entry);
            let prefix = live.catalog_test_wal_state();
            let before_files = state.lock().unwrap().images();
            let before_bank = live.recovery_test_bank().to_vec();
            let before_jobs = live.catalog_test_jobs().to_vec();
            assert_eq!(live.catalog_test_locals(), (0, 0));
            let failure = match live.catalog_begin(owner, status, private) {
                Err(error) => error,
                Ok(_) => panic!("exhausted original Root counter must reject Begin"),
            };
            assert_eq!(
                failure.phase,
                super::super::super::super::plan4_pages::RetentionPhase::Prepare
            );
            assert!(matches!(
                failure.cause,
                super::super::super::super::plan4_pages::RetentionCause::Exhausted
            ));
            assert_eq!(failure.effect, Effect::Unchanged);
            assert!(!fence.is_fenced());
            assert_eq!(live.catalog_test_locals(), (1, 0));
            assert_eq!(live.catalog_test_wal_state(), prefix);
            assert_eq!(live.recovery_test_bank(), before_bank);
            assert_eq!(live.catalog_test_jobs().as_slice(), before_jobs);
            assert_eq!(state.lock().unwrap().images(), before_files);
            let mut after_bytes = boxed::<PAGE_BYTES>();
            let after = probe::diagnose(live.catalog_test_pool(), index, &mut after_bytes);
            assert_eq!(after_bytes, bytes);
            assert_eq!(after.frame.modification_counter, u64::MAX);
            assert_eq!(after.entry, max_entry);
            assert_eq!(
                (
                    after.frame.page_lsn,
                    after.frame.rec_lsn,
                    after.dirty,
                    after.total
                ),
                (
                    original.frame.page_lsn,
                    original.frame.rec_lsn,
                    original.dirty,
                    0
                )
            );
            owner.check_idle().unwrap();
            probe::seed_counter(
                live.catalog_test_pool(),
                index,
                original.frame.modification_counter,
            );
            probe::seed_dpt(live.catalog_test_pool(), index, original.entry);
            let begin = live.catalog_begin(owner, status, private).unwrap();
            let expected = actor(1, 2, 2);
            assert_eq!((begin.header.lsn, begin.actor), (6, Some(expected)));
            assert_eq!(live.catalog_test_locals(), (2, 2));
            oracle.check_scenario(live, state, "MC03", 6);
            let definition = table_spec("T");
            let reserved = live
                .reserve_catalog_create(owner, expected, undo, &definition, private)
                .unwrap();
            oracle.check_scenario(reserved.catalog_test_bound(), state, "MC03", 7);
            match reserved.initialize() {
                Ok(object) => assert_eq!(object.into_parts().0.object.get(), 6),
                Err(RetentionReservationError::Before { error, .. })
                | Err(RetentionReservationError::Fatal(error)) => {
                    panic!("actual Forward {error:?}")
                }
            }
            oracle.check_scenario(live, state, "MC03", 8);
            let prepared = live
                .prepare_catalog_commit(owner, expected, private)
                .unwrap();
            let (mut writer, _, _) = live.split();
            assert!(writer.commit_catalog(owner, prepared).is_ok());
            oracle.check_scenario(live, state, "MC03", 9);
            assert_eq!(live.catalog_test_q(), 1);
            state.lock().unwrap().images()
        });
        recover_check(&images, &oracle, "MC03");
    });
}
