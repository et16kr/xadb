//! Additional DEV3 S5 inputs. Every input is excluded from mandatory selectors.
use super::*;

#[test]
#[ignore = "DEV3 final S5: distinct short-read inputs"]
fn retention_s5_short_reads() {
    let ids: Vec<_> = (1..=12).map(|i| format!("P4-LIVE-SD{i:02}")).collect();
    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&refs);
    for (i, count) in [1, 127, 128, 4095, 4096, 16383].into_iter().enumerate() {
        for (j, page) in [8, 3].into_iter().enumerate() {
            h.case(&ids[i * 2 + j], || {
                fixture!(|bound, owner, core, _foreign, state, storage| {
                    let target = bound
                        .target(if page == 8 {
                            Plan4Page::CatalogLeaf
                        } else {
                            Plan4Page::Jobs { index: 1 }
                        })
                        .unwrap();
                    inject(
                        &state,
                        "read",
                        "d3-data-0000000100000001",
                        page * PAGE_BYTES as u64,
                        1,
                        port::Action::Short(count),
                    );
                    let observed = bound.load_into(1, &target).unwrap();
                    assert_eq!(
                        (
                            observed.after.page_lsn,
                            observed.after.modification_counter,
                            observed.after.rec_lsn
                        ),
                        (PageLsn(0), 0, None)
                    );
                    assert_eq!(
                        page_bytes(&bound, page).1,
                        Oracle::new().genesis(page as usize)
                    );
                    assert_eq!(fault_hits(&state), 1);
                    assert_eq!(effects(&state).1, 0);
                });
            });
        }
    }
    h.finish();
}

#[test]
#[ignore = "DEV3 final S5: distinct fixed-prefix corruption inputs"]
fn retention_s5_prefix_mutations() {
    let ids: Vec<_> = (1..=8).map(|i| format!("P4-LIVE-SF{i:02}")).collect();
    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&refs);
    for (i, offset) in [0, 159, 160, 65535].into_iter().enumerate() {
        for (j, occurrence) in [1, 3].into_iter().enumerate() {
            h.case(&ids[i * 2 + j], || {
                fixture!(|bound, owner, core, _foreign, state, storage| {
                    let status = init(&mut bound, &mut owner, &core);
                    prepare(&mut bound, &mut owner, &core, &status);
                    {
                        let (mut writer, _, _) = bound.split();
                        writer.aging_start(&mut owner).unwrap();
                    }
                    let target = bound.target(Plan4Page::Root).unwrap();
                    clear(&state);
                    let mut changed = wal(&state);
                    changed[offset] ^= 1;
                    state.lock().unwrap().hook = Some(port::Hook {
                        key: port::Key::new(
                            "read",
                            crate::storage::wal_io::WAL_NAME,
                            0,
                            occurrence,
                        ),
                        edit: port::Edit::Bytes(crate::storage::wal_io::WAL_NAME.into(), changed),
                    });
                    let home = data(&state, 1);
                    let image = page_bytes(&bound, 1).1;
                    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    let error = {
                        let (_, _, mut f) = bound.split();
                        rejected(f.try_flush(&mut dedicated, &target))
                    };
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Source, Effect::CallerMustResolve)
                    );
                    assert!(if offset == 65535 && occurrence == 1 {
                        matches!(error.cause, RetentionCause::Semantic(_))
                    } else {
                        matches!(error.cause, RetentionCause::Invariant)
                    });
                    assert!(storage.session_fence().is_fenced());
                    let hook_hits = state.lock().unwrap().hook_hits;
                    assert_eq!(hook_hits, 1);
                    assert_eq!(effects(&state).1, 0);
                    assert_eq!(data(&state, 1), home);
                    let (frame, actual) = page_bytes(&bound, 1);
                    assert!(frame.dirty);
                    assert_eq!(frame.total, 0);
                    assert_eq!(actual, image);
                    assert!(
                        crate::storage::dirty_frame::test_support::flush_diagnose(
                            &bound.inner.pool
                        )
                        .claim
                        .is_some()
                    );
                });
            });
        }
    }
    h.finish();
}

#[test]
#[ignore = "DEV3 final S5: distinct successful WAL partial-write inputs"]
fn retention_s5_partial_writes() {
    let ids: Vec<_> = (1..=12).map(|i| format!("P4-LIVE-SW{i:02}")).collect();
    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&refs);
    for (i, count) in [1, 159, 160, 3040, 16383, 21200].into_iter().enumerate() {
        for stage in 0..2 {
            h.case(&ids[i * 2 + stage], || {
                let oracle = Oracle::new();
                fixture!(|bound, owner, core, _foreign, state, storage| {
                    if stage == 0 {
                        let mut cursor = Plan4ResidencyCursor::new();
                        assert!(
                            bound
                                .prepare_status_slot(&owner, &mut cursor)
                                .unwrap()
                                .is_none()
                        );
                        let reserved = bound.reserve_status(&mut owner).unwrap();
                        clear(&state);
                        inject(
                            &state,
                            "write",
                            crate::storage::wal_io::WAL_NAME,
                            25_296,
                            1,
                            port::Action::Short(count),
                        );
                        match reserved.initialize() {
                            Ok(_) => {}
                            Err(RetentionReservationError::Before { error, .. })
                            | Err(RetentionReservationError::Fatal(error)) => {
                                panic!("short INIT: {error:?}")
                            }
                        }
                        let reserve =
                            literals::population(&oracle, oracle.genesis(0), 1, 1, 0, &[]);
                        let initialized = literals::population(
                            &oracle,
                            &reserve.allocator,
                            1,
                            1,
                            2,
                            &reserve.descriptor,
                        );
                        assert_eq!(&wal(&state)[25_296..], initialized.frame);
                        assert_page(&bound, 9, oracle.page("status-free.page"));
                    } else {
                        let status = init(&mut bound, &mut owner, &core);
                        prepare(&mut bound, &mut owner, &core, &status);
                        {
                            let (mut writer, _, _) = bound.split();
                            writer.aging_start(&mut owner).unwrap();
                        }
                        clear(&state);
                        inject(
                            &state,
                            "write",
                            crate::storage::wal_io::WAL_NAME,
                            82_968,
                            1,
                            port::Action::Short(count),
                        );
                        {
                            let (mut writer, _, _) = bound.split();
                            assert_eq!(
                                writer.begin_empty(&mut owner, &status, job(2)).unwrap().lsn,
                                PageLsn(4)
                            );
                        }
                        assert_eq!(&wal(&state)[82_968..], oracle.raw("begin.wal"));
                        assert_page(&bound, 9, oracle.page("status-begin.page"));
                    }
                    assert_eq!(fault_hits(&state), 1);
                    assert!(!storage.session_fence().is_fenced());
                    assert!(bound.control.ledger.pending.is_none());
                    owner.check_idle().unwrap();
                });
            });
        }
    }
    h.finish();
}
