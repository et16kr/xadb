use super::super::pressure::StatusResidencyStep;
use super::*;

#[test]
fn empty_pressure_resource_gate() {
    let memory = EmptyTransactions::<port::Writer>::pressure_test_layout();
    let native =
        EmptyTransactions::<xadb_platform::namespace::LinuxWriterLease>::pressure_test_layout();
    println!("pressure memory={memory:?} native={native:?}");
    assert!(memory.2 && native.2);
}

macro_rules! cold_fixture {
    ($profile:expr, |$live:ident,$core:ident,$state:ident,$storage:ident,$pages:ident,$owner:ident $(,$foreign:ident)?| $body:block) => {
        fixture!(
            2,
            $profile,
            |$live, $core, $state, $storage, $pages, $owner $(,$foreign)?| $body,
            prepare | provision | {
                let mut dedicated = $core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                for page in &$pages {
                    provision
                        .split_with_flush()
                        .2
                        .try_flush_page(&mut dedicated, *page)
                        .unwrap();
                }
                provision.prepare_slot(1).unwrap();
                let page = match provision
                    .split_with_flush()
                    .0
                    .reserve_one(&mut $owner)
                    .unwrap()
                    .initialize()
                {
                    Ok(v) => v.page,
                    Err(_) => panic!("third INIT"),
                };
                $pages.push(page);
                provision
                    .split_with_flush()
                    .2
                    .try_flush_page(&mut dedicated, page)
                    .unwrap();
            }
        );
    };
}

fn ready(step: StatusResidencyStep, page: PageRef, loaded: bool) -> usize {
    match step {
        StatusResidencyStep::Resident {
            index,
            frame,
            loaded: actual,
        } => {
            assert_eq!(frame.identity.page, page);
            assert_eq!(actual, loaded);
            index
        }
        other => panic!("expected residency, got {other:?}"),
    }
}

#[test]
fn empty_pressure_exact_hit_precedes_clean_and_round_robin_wraps() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut cursor = live.residency_cursor();
        let before = pool_state(&live);
        let prefix = storage.wal().prefix().unwrap();
        assert_eq!(
            ready(
                live.residency_step(&mut owner, pages[1], &mut cursor)
                    .unwrap(),
                pages[1],
                false
            ),
            2
        );
        assert_eq!(pool_state(&live), before);
        assert_eq!(state.lock().unwrap().reads, 0);
        assert!(state.lock().unwrap().events.is_empty());
        for (page, index) in [(pages[0], 1), (pages[2], 2), (pages[1], 1), (pages[0], 2)] {
            assert_eq!(
                ready(
                    live.residency_step(&mut owner, page, &mut cursor).unwrap(),
                    page,
                    true
                ),
                index
            );
        }
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert!(state.lock().unwrap().events.is_empty());
        println!("PRESSURE_HIT_AND_WRAP variants=5");
    });
}

#[test]
fn empty_pressure_empty_destination_and_reused_cursor_are_hints() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        live.inner
            .inner
            .inner
            .pool
            .prepare_resident_slot(1)
            .unwrap();
        let mut cursor = live.residency_cursor();
        // A hint from a larger pool is normalized to this pool's payload count.
        cursor.0 = std::num::NonZeroUsize::new(63);
        assert_eq!(
            ready(
                live.residency_step(&mut owner, pages[0], &mut cursor)
                    .unwrap(),
                pages[0],
                true
            ),
            1
        );
        assert_eq!(cursor.0.unwrap().get(), 2);
        assert!(!storage.session_fence().is_fenced());
        println!("PRESSURE_EMPTY_HINT variants=1");
    });
}

#[test]
fn empty_pressure_oldest_dirty_preserves_every_frame_until_flush() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        live.begin(&mut owner, pages[1]).unwrap();
        live.begin(&mut owner, pages[2]).unwrap();
        let mut cursor = live.residency_cursor();
        let before = pool_state(&live);
        let bank = live.bank.to_vec();
        let prefix = storage.wal().prefix().unwrap();
        let events = state.lock().unwrap().events.len();
        let reads = state.lock().unwrap().reads;
        assert_eq!(
            live.residency_step(&mut owner, pages[0], &mut cursor)
                .unwrap(),
            StatusResidencyStep::FlushRequired {
                page: pages[1],
                rec_lsn: PageLsn(7)
            }
        );
        assert!(cursor.0.is_none());
        assert_eq!(pool_state(&live), before);
        assert_eq!(live.bank.as_slice(), bank);
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert_eq!(state.lock().unwrap().reads, reads);
        assert_eq!(state.lock().unwrap().events.len(), events);
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        live.flush_status(&mut dedicated, pages[1]).unwrap();
        assert_eq!(
            ready(
                live.residency_step(&mut owner, pages[0], &mut cursor)
                    .unwrap(),
                pages[0],
                true
            ),
            2
        );
        assert_eq!(pool_state(&live)[1], before[1]);
        assert_eq!(live.bank.as_slice(), bank);
        println!("PRESSURE_DIRTY variants=2");
    });
}

#[test]
fn empty_pressure_original_actor_continues_after_automatic_replacement() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut cursor = live.residency_cursor();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let actor = live.begin(&mut owner, pages[1]).unwrap().actor;
        for phase in 0..3 {
            if phase == 1 {
                live.rollback_start(&mut owner, actor).unwrap();
            }
            if phase == 2 {
                live.abort_end(&mut owner, actor).unwrap();
            }
            // The caller never chooses a frame. Advance both other addresses,
            // servicing reported victims and explicitly flushing the known actor
            // when clean-first selection would otherwise keep it resident.
            let mut evicted = false;
            for attempt in 0..6 {
                let wanted = if attempt % 2 == 0 { pages[0] } else { pages[2] };
                match live
                    .residency_step(&mut owner, wanted, &mut cursor)
                    .unwrap()
                {
                    StatusResidencyStep::FlushRequired { page, .. } => {
                        live.flush_status(&mut dedicated, page).unwrap();
                    }
                    StatusResidencyStep::Resident { .. } => {}
                }
                let request = TargetRequest {
                    identity: PageIdentity {
                        database: storage.profile().spec().namespace.database,
                        page: pages[1],
                        owner: live.inner.inner.inner.control.role.object(),
                        owner_generation: live.inner.inner.inner.control.role.generation(),
                    },
                    kind: PageKind::TransactionStatus,
                    level: 0,
                };
                if !live
                    .inner
                    .inner
                    .inner
                    .pool
                    .resident_address_present(request)
                    .unwrap()
                {
                    evicted = true;
                    break;
                }
                // Once both other pages are alternately requested, explicitly
                // service the actor's dirty image to make replacement possible.
                if attempt == 1 {
                    live.flush_status(&mut dedicated, pages[1]).unwrap();
                }
            }
            assert!(evicted);
            let bank = live.bank.to_vec();
            let loaded = live
                .residency_step(&mut owner, pages[1], &mut cursor)
                .unwrap();
            ready(loaded, pages[1], true);
            assert_eq!(live.bank.as_slice(), bank);
        }
        assert!(live.bank.iter().all(|b| *b == 0));
        assert!(!storage.session_fence().is_fenced());
        println!("PRESSURE_ACTOR variants=3");
    });
}

#[derive(Debug, PartialEq, Eq)]
struct LoadPoolSlot {
    frame: FrameStamp,
    dirty: bool,
    entry: Option<dirty_frame::DirtyEntry>,
    pins: Vec<u16>,
    total: u16,
    bytes: Vec<u8>,
}
fn pool_state<W: WriterNamespace>(
    live: &EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> Vec<LoadPoolSlot> {
    (0..3)
        .map(|index| {
            let mut bytes = boxed::<PAGE_BYTES>(0);
            let metadata = dirty_frame::test_support::diagnose(
                &live.inner.inner.inner.pool,
                index,
                &mut bytes,
            );
            LoadPoolSlot {
                frame: metadata.frame,
                dirty: metadata.dirty,
                entry: metadata.entry,
                pins: metadata.pins.to_vec(),
                total: metadata.total,
                bytes: bytes.to_vec(),
            }
        })
        .collect()
}

#[test]
fn empty_pressure_generation_limits_hit_replacement_and_busy() {
    for variant in 0..6 {
        cold_fixture!(1, |live, core, state, storage, pages, owner| {
            let mut cursor = live.residency_cursor();
            if variant == 2 {
                live.begin(&mut owner, pages[1]).unwrap();
            }
            dirty_frame::pressure_test_support::generation(
                &mut live.inner.inner.inner.pool,
                1,
                u64::MAX,
            );
            if variant >= 3 {
                dirty_frame::pressure_test_support::generation(
                    &mut live.inner.inner.inner.pool,
                    2,
                    u64::MAX,
                );
            }
            let before = pool_state(&live);
            if variant == 4 {
                dirty_frame::pressure_test_support::forget_content_write(
                    &live.inner.inner.inner.pool,
                    2,
                );
            }
            let wanted = if variant == 0 || variant == 5 {
                pages[2]
            } else {
                pages[0]
            };
            let result = live.residency_step(&mut owner, wanted, &mut cursor);
            match variant {
                0 | 5 => {
                    assert_eq!(ready(result.unwrap(), wanted, false), 1);
                }
                1 => {
                    assert_eq!(ready(result.unwrap(), wanted, true), 2);
                }
                2 => {
                    assert!(
                        matches!(result.unwrap(), StatusResidencyStep::FlushRequired { page, .. } if page == pages[1])
                    );
                    assert!(cursor.0.is_none());
                }
                3 | 4 => {
                    let error = result.unwrap_err();
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(
                        matches!(error.cause, Cause::Frame(e) if e.cause == if variant == 3 { dirty_frame::Cause::CounterExhausted } else { dirty_frame::Cause::Busy })
                    );
                    assert!(cursor.0.is_none());
                }
                _ => unreachable!(),
            }
            if variant != 1 && variant != 4 {
                assert_eq!(pool_state(&live), before);
            }
            assert!(!storage.session_fence().is_fenced());
            println!("PRESSURE_GENERATION variant={variant}");
        });
    }
}

#[test]
fn empty_pressure_dirty_tie_uses_scan_order() {
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        live.begin(&mut owner, pages[1]).unwrap();
        live.begin(&mut owner, pages[2]).unwrap();
        dirty_frame::pressure_test_support::conservative_rec_lsn(
            &live.inner.inner.inner.pool,
            1,
            PageLsn(7),
        );
        let mut cursor = live.residency_cursor();
        let before = pool_state(&live);
        for (start, page) in [(1, pages[2]), (2, pages[1])] {
            cursor.0 = std::num::NonZeroUsize::new(start);
            assert_eq!(
                live.residency_step(&mut owner, pages[0], &mut cursor)
                    .unwrap(),
                StatusResidencyStep::FlushRequired {
                    page,
                    rec_lsn: PageLsn(7)
                }
            );
            assert_eq!(cursor.0.unwrap().get(), start);
            assert_eq!(pool_state(&live), before);
        }
        println!("PRESSURE_TIE variants=2");
    });
}

#[test]
fn empty_pressure_foreign_owner_and_invalid_membership_are_no_effect() {
    cold_fixture!(1, |live, core, state, storage, pages, owner, foreign| {
        let mut cursor = live.residency_cursor();
        let mut other = foreign.try_owner(OwnerRole::Maintenance).unwrap();
        let before = pool_state(&live);
        for variant in 0..4 {
            let mut page = pages[0];
            match variant {
                0 => page.page_no = 63,
                1 => page.incarnation = Generation::new(3).unwrap(),
                2 => page.file = crate::storage::page::FileId::new(0x100000002).unwrap(),
                _ => {}
            }
            let error = live
                .residency_step(
                    if variant == 3 { &mut other } else { &mut owner },
                    page,
                    &mut cursor,
                )
                .unwrap_err();
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(
                error.cause,
                Cause::Membership | Cause::BindingMismatch
            ));
            assert_eq!(pool_state(&live), before);
            assert!(cursor.0.is_none());
            assert_eq!(state.lock().unwrap().reads, 0);
            assert!(state.lock().unwrap().events.is_empty());
        }
        println!("PRESSURE_MEMBERSHIP variants=4");
    });
}

#[test]
fn empty_pressure_late_content_corruption_and_poison_are_not_hidden() {
    for poison in [false, true] {
        cold_fixture!(1, |live, core, state, storage, pages, owner| {
            let mut cursor = live.residency_cursor();
            if poison {
                dirty_frame::test_support::poison_content(&live.inner.inner.inner.pool, 2);
            } else {
                dirty_frame::test_support::seed_counter(&live.inner.inner.inner.pool, 2, 0);
            }
            // Slot1 is an exact hit, but the selected full scan must observe slot2.
            let error = live
                .residency_step(&mut owner, pages[2], &mut cursor)
                .unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(
                matches!(error.cause, Cause::Frame(e) if e.cause == if poison { dirty_frame::Cause::Poisoned } else { dirty_frame::Cause::Invariant })
            );
            assert!(storage.session_fence().is_fenced());
            assert!(cursor.0.is_none());
            assert_eq!(state.lock().unwrap().reads, 0);
            assert!(state.lock().unwrap().events.is_empty());
            println!("PRESSURE_LATE_CORRUPTION poison={poison}");
        });
    }
}

#[test]
fn empty_pressure_retained_pin_flush_claim_and_poison_prevent_io() {
    for variant in 0..3 {
        cold_fixture!(1, |live, core, state, storage, pages, owner| {
            let mut cursor = live.residency_cursor();
            let bank = live.bank.to_vec();
            let mut reader = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let mut scratch = boxed::<PAGE_BYTES>(0);
            match variant {
                0 => {
                    let request = live.inner.inner.inner.pool.target_request(1).unwrap();
                    let held = live
                        .inner
                        .inner
                        .inner
                        .pool
                        .reserve_target_set(&mut reader, &[request])
                        .unwrap()
                        .try_read()
                        .unwrap();
                    std::mem::forget(held);
                }
                1 => {
                    let request = live.inner.inner.inner.pool.target_request(0).unwrap();
                    let capture = live
                        .inner
                        .inner
                        .inner
                        .pool
                        .capture_flush(&mut flusher, request, &mut scratch)
                        .unwrap()
                        .unwrap();
                    #[allow(clippy::forget_non_drop)]
                    std::mem::forget(capture);
                }
                2 => dirty_frame::test_support::poison_dpt(&live.inner.inner.inner.pool),
                _ => unreachable!(),
            }
            let error = live
                .residency_step(&mut owner, pages[0], &mut cursor)
                .unwrap_err();
            assert_eq!(
                error.effect,
                if variant == 2 {
                    Effect::CallerMustResolve
                } else {
                    Effect::Unchanged
                }
            );
            assert_eq!(storage.session_fence().is_fenced(), variant == 2);
            assert!(matches!(error.cause, Cause::Busy | Cause::Frame(_)));
            assert!(cursor.0.is_none());
            assert_eq!(live.bank.as_slice(), bank);
            assert_eq!(state.lock().unwrap().reads, 0);
            assert!(state.lock().unwrap().events.is_empty());
            println!("PRESSURE_RETAINED variant={variant}");
        });
    }
}

#[test]
fn empty_pressure_read_failure_retry_and_final_drift() {
    let final_read;
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut cursor = live.residency_cursor();
        ready(
            live.residency_step(&mut owner, pages[0], &mut cursor)
                .unwrap(),
            pages[0],
            true,
        );
        let guard = state.lock().unwrap();
        let reads: Vec<_> = guard
            .read_events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.name == "d3-data-0000000100000001"
                    && e.offset == PAGE_BYTES as u64
                    && e.length == PAGE_BYTES
            })
            .map(|(index, _)| index)
            .collect();
        assert_eq!(reads.len(), 3);
        final_read = reads[2];
        println!("PRESSURE_READ scenario=calibration final_read={final_read}");
    });
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut cursor = live.residency_cursor();
        let before = pool_state(&live);
        state.lock().unwrap().named_read_plan = Some((
            "d3-data-0000000100000001",
            PAGE_BYTES as u64,
            PAGE_BYTES,
            port::Action::BeforeError,
        ));
        let error = live
            .residency_step(&mut owner, pages[0], &mut cursor)
            .unwrap_err();
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(pool_state(&live), before);
        assert!(cursor.0.is_none());
        assert!(!storage.session_fence().is_fenced());
        assert_eq!(state.lock().unwrap().configured_read_hits, 1);
        assert!(state.lock().unwrap().events.is_empty());
        ready(
            live.residency_step(&mut owner, pages[0], &mut cursor)
                .unwrap(),
            pages[0],
            true,
        );
        println!("PRESSURE_READ scenario=retry");
    });
    cold_fixture!(1, |live, core, state, storage, pages, owner| {
        let mut cursor = live.residency_cursor();
        let before = pool_state(&live);
        let mut changed = state.lock().unwrap().files["d3-data-0000000100000001"]
            .bytes
            .clone();
        changed[PAGE_BYTES + 64..PAGE_BYTES + 72].copy_from_slice(&1u64.to_le_bytes());
        history_reseal_page(&mut changed[PAGE_BYTES..2 * PAGE_BYTES]);
        let expected = changed[PAGE_BYTES..2 * PAGE_BYTES].to_vec();
        state.lock().unwrap().read_edits.push((
            final_read,
            "d3-data-0000000100000001".into(),
            changed,
        ));
        let error = live
            .residency_step(&mut owner, pages[0], &mut cursor)
            .unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(pool_state(&live), before);
        assert!(cursor.0.is_none());
        let guard = state.lock().unwrap();
        assert!(guard.read_edits.is_empty());
        assert_eq!(guard.read_events[final_read].data, expected);
        assert!(guard.events.is_empty());
        println!("PRESSURE_READ scenario=final_drift");
    });
}

#[test]
fn empty_pressure_late_registration_and_dpt_mismatch_fence() {
    for duplicate in [true, false] {
        cold_fixture!(1, |live, core, state, storage, pages, owner| {
            let mut cursor = live.residency_cursor();
            if duplicate {
                dirty_frame::pressure_test_support::duplicate_registration(
                    &mut live.inner.inner.inner.pool,
                );
            } else {
                live.begin(&mut owner, pages[1]).unwrap();
                dirty_frame::test_support::seed_dpt(&live.inner.inner.inner.pool, 2, None);
            }
            let reads = state.lock().unwrap().reads;
            let events = state.lock().unwrap().events.len();
            let bank = live.bank.to_vec();
            let prefix = storage.wal().prefix().unwrap();
            let error = live
                .residency_step(&mut owner, pages[2], &mut cursor)
                .unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(
                matches!(error.cause, Cause::Frame(e) if e.cause == dirty_frame::Cause::Invariant)
            );
            assert!(storage.session_fence().is_fenced());
            assert!(cursor.0.is_none());
            assert_eq!(live.bank.as_slice(), bank);
            // Prefix observation is unavailable after the mandatory fence; the
            // original durable WAL bytes and write counter remain unchanged.
            assert_eq!(state.lock().unwrap().reads, reads);
            assert_eq!(state.lock().unwrap().events.len(), events);
            assert!(prefix.complete_lsn >= 6);
            println!("PRESSURE_LATE_METADATA duplicate={duplicate}");
        });
    }
}

macro_rules! pressure_s5_fixture {
    ($count:expr, $frame_count:expr, $profile:expr, |$live:ident,$core:ident,$state:ident,$storage:ident,$pages:ident,$owner:ident $(,$foreign:ident)?| $body:block $(, prepare |$provision:ident| $setup:block)?) => {{
        let mut wal_slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(
            namespace,
            $profile,
            &mut wal_slot,
            &mut [[0; PAGE_BYTES]; 3],
        )
        .unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        $(let $foreign = PageConcurrency::new($storage.session_fence()).unwrap();)?
        let mut bytes = vec![[0; PAGE_BYTES]; $frame_count];
        let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114688>(0);
        let binding = $storage
            .bind_resident_status(&$core, &mut frames, &mut operation)
            .unwrap();
        let mut bound = match binding.activate() {
            Ok(v) => v,
            Err(IntoFlushError::Before { error, .. }) | Err(IntoFlushError::Fatal(error)) => {
                panic!("activate {error:?}")
            }
        };
        let mut $owner = $core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut $pages = Vec::new();
        for index in 1..=$count {
            if index > 1 {
                bound.prepare_slot(index).unwrap();
            }
            let (mut writer, _, _) = bound.split_with_flush();
            let page = match writer.reserve_one(&mut $owner).unwrap().initialize() {
                Ok(v) => v.page,
                Err(_) => panic!("INIT"),
            };
            $pages.push(page);
        }
        $(let mut $provision = bound; $setup let bound = $provision;)?
        let mut bank = boxed::<{ crate::storage::wal_io::EMPTY_TRANSACTION_BANK_BYTES }>(0);
        #[allow(unused_mut)] // Admission-only cases observe the same fixture without a live mutation.
        let mut $live = match bound.into_empty_transactions(&mut $owner, &mut bank) {
            Ok(v) => v,
            Err(_) => panic!("handoff"),
        };
        {
            let mut state = $state.lock().unwrap();
            state.events.clear();
            state.read_events.clear();
            state.hit = 0;
            state.reads = 0;
        }
        $body
    }};
}

#[test]
#[ignore = "DEV3 S5 expanded profile/scan-position/generation matrix; 438 scenarios"]
fn empty_pressure_s5_profiles_slots_and_obstruction() {
    for profile in 1..=3 {
        for frame_count in [4, 8, 64] {
            // First-extent membership holds at most 63 Status pages. The 64
            // frame case therefore has one empty payload slot and does not
            // claim a 65th-address N+1 working set.
            let resident_count = if frame_count == 64 {
                62
            } else {
                frame_count - 1
            };
            for start in 1..frame_count {
                for exhausted in [false, true] {
                    pressure_s5_fixture!(
                        resident_count,
                        frame_count,
                        profile,
                        |live, core, state, storage, pages, owner| {
                            let mut cursor = live.residency_cursor();
                            cursor.0 = std::num::NonZeroUsize::new(start);
                            if exhausted {
                                dirty_frame::status_load_test_support::seed_generation(
                                    &mut live.inner.inner.inner.pool,
                                    start,
                                    u64::MAX,
                                );
                            }
                            let expected = if exhausted {
                                1 + start % (frame_count - 1)
                            } else {
                                start
                            };
                            let actual = ready(
                                live.residency_step(&mut owner, pages[0], &mut cursor)
                                    .unwrap(),
                                pages[0],
                                true,
                            );
                            assert_eq!(actual, expected);
                            assert!(!storage.session_fence().is_fenced());
                            println!(
                                "PRESSURE_S5_PASS profile={profile} frames={} start={start} exhausted={exhausted}",
                                frame_count
                            );
                        },
                        prepare | provision | {
                            let mut dedicated =
                                core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                            for page in &pages {
                                provision
                                    .split_with_flush()
                                    .2
                                    .try_flush_page(&mut dedicated, *page)
                                    .unwrap();
                            }
                            provision.prepare_slot(1).unwrap();
                            let page = match provision
                                .split_with_flush()
                                .0
                                .reserve_one(&mut owner)
                                .unwrap()
                                .initialize()
                            {
                                Ok(v) => v.page,
                                Err(_) => panic!("S5 next Status"),
                            };
                            pages.push(page);
                            provision
                                .split_with_flush()
                                .2
                                .try_flush_page(&mut dedicated, page)
                                .unwrap();
                        }
                    );
                }
            }
        }
    }
}
