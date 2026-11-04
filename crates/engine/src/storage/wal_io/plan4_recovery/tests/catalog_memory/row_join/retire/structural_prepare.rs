//! The new original-source split preparer over real CREATE/INSERT/cleanup.
//! Expected row partition and unchanged live state are independent facts.
use super::*;
use crate::storage::{page::PageKind, semantic_row::structure::Operation as Split};
use foundation::identity::{ObjectHandle, SchemaEpoch};

#[test]
fn structural_admission_original_active_rows_preserves_debt_and_exact_wal_fit() {
    case_within("SMO-P03", std::time::Duration::from_secs(180), || {
        use crate::storage::wal_io::durability_test_support::wal_share;
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let inserted = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    view,
                    &table,
                    private,
                    [7, 8]
                );
                assert_eq!(inserted.len(), 2);
                // Leave the authentic transaction active: its rollback and Aging
                // credit must survive the structural group's zero-debt successor.
                let source = writer.catalog_test_with_pool(|pool| {
                    (0..64)
                        .filter_map(|number| probe::registered_index(pool, number))
                        .map(|at| pool.target_request(at).unwrap())
                        .find(|r| r.kind == PageKind::RowLeaf && r.identity.owner.get() == 6)
                        .unwrap()
                });
                let object = ObjectHandle {
                    object: ObjectId::new(6).unwrap(),
                    generation: Generation::new(1).unwrap(),
                    schema_epoch: SchemaEpoch::new(1).unwrap(),
                };
                let before = writer.test_structural_inventory_before().to_vec();
                let wal = files(state).0;
                let bank = writer.recovery_test_bank().to_vec();
                let jobs = writer.catalog_test_jobs().to_vec();
                // Independent XOD1 wire sum, not InventoryRows or the
                // encoder's returned future. Live bank: ATT0..31, Aging32,
                // Allocation33; Finished phase4 and Structural34 owe nothing.
                let mut expected_future = (0u64, 0u64); // WAL, steps
                let mut att_wal = 0u64;
                let mut aging_wal = 0u64;
                for slot in 0..34 {
                    let descriptor = &bank[slot * 1664..slot * 1664 + 1536];
                    if descriptor.iter().all(|b| *b == 0) || slot >= 32 && descriptor[353] == 4 {
                        continue;
                    }
                    assert_eq!(&descriptor[..4], b"XOD1");
                    for kind in 0..5 {
                        let start = 640 + 128 * kind;
                        assert_eq!(descriptor[start], kind as u8 + 1);
                        let steps = u64::from_le_bytes(
                            descriptor[start + 8..start + 16].try_into().unwrap(),
                        );
                        let wal = u64::from_le_bytes(
                            descriptor[start + 16..start + 24].try_into().unwrap(),
                        );
                        expected_future.0 = expected_future.0.checked_add(wal).unwrap();
                        expected_future.1 = expected_future.1.checked_add(steps).unwrap();
                        if slot < 32 {
                            att_wal = att_wal.checked_add(wal).unwrap();
                        }
                        if slot == 32 {
                            aging_wal = aging_wal.checked_add(wal).unwrap();
                        }
                    }
                }
                assert!(att_wal > 0 && aging_wal > 0);
                writer.test_structural_dirty_selection();
                let observed = {
                    let share = wal_share(None);
                    let prepared = writer
                        .prepare_row_split(owner, object, source.identity.page, private)
                        .unwrap();
                    let (inventory, future) = prepared.test_inventory();
                    assert_eq!(future, expected_future);
                    assert!(expected_future.0 > 0 && expected_future.1 > 0);
                    let record = share.record();
                    assert_eq!((record.admissions, record.stages), (1, 0));
                    let admission = record.first.unwrap();
                    assert_eq!(
                        (admission.frame, admission.future, admission.refused),
                        (84_744, expected_future.0, false)
                    );
                    // Independent byte expectation: no owner/item/debt changes.
                    // Only capture LSN and the root's trailing digest advance.
                    let mut expected = before.clone();
                    let capture = u64::from_le_bytes(inventory[48..56].try_into().unwrap());
                    let old_capture = u64::from_le_bytes(before[48..56].try_into().unwrap());
                    assert_eq!(capture, old_capture + 1);
                    expected[48..56].copy_from_slice(&(old_capture + 1).to_le_bytes());
                    let end = u32::from_le_bytes(expected[12..16].try_into().unwrap()) as usize;
                    let digest = foundation::codec::content_digest(&expected[..end - 32]);
                    expected[end - 32..end].copy_from_slice(&digest);
                    assert_eq!(inventory.as_slice(), expected);
                    assert!(u16::from_le_bytes(expected[56..58].try_into().unwrap()) >= 1);
                    admission
                };
                let exact = observed.offset + 84_744 + expected_future.0;
                for (cap, success) in [(exact - 1, false), (exact, true)] {
                    let share = wal_share(Some(cap));
                    let result =
                        writer.prepare_row_split(owner, object, source.identity.page, private);
                    match (success, result) {
                        (true, Ok(_)) => {}
                        (false, Err(e)) => {
                            assert_eq!(e.effect, Effect::Unchanged);
                            assert!(matches!(e.cause, RetentionCause::Capacity));
                        }
                        (true, Err(e)) => panic!("exact fit rejected: {e:?}"),
                        (false, Ok(_)) => panic!("one-byte short admitted"),
                    }
                    let record = share.record();
                    assert_eq!((record.admissions, record.stages), (1, 0));
                    assert_eq!(record.refused_admissions, u32::from(!success));
                    assert!(writer.test_structural_replay_returned());
                    owner.check_idle().unwrap();
                    writer.catalog_test_with_pool(|pool| {
                        assert_eq!(
                            crate::storage::dirty_frame::init_test_support::claim_state(pool),
                            (false, 0)
                        );
                    });
                    assert_eq!(files(state).0, wal);
                    assert_eq!(writer.recovery_test_bank(), bank);
                    assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                    assert_eq!(writer.test_structural_inventory_before().as_slice(), before);
                    assert!(!fence.is_fenced());
                }
                // Resolve the original transaction through A after all preparation
                // outcomes, establishing that no prepared split consumed its credit.
                assert_eq!(
                    a_commit!(&mut writer, owner, flush, flusher, tx, private),
                    2
                );
                assert_eq!(
                    drained!(&mut writer, owner, flush, flusher, private),
                    done(11, 0)
                );
                println!(
                    "SMO_P03 active actual Row debts; exact WAL fit/one-byte short; XOI unchanged owners and items; commit/drain still usable"
                );
            }
        );
    });
}

#[test]
fn structural_prepare_child_reads_four_raw_fixture_images_and_parent_stamp() {
    case_within("SMO-P02", std::time::Duration::from_secs(60), || {
        use crate::storage::{
            control as format,
            dirty_frame::{FrameStamp, SharedPool, TargetRequest},
            page::{self, PageHeader, PageLsn, PageRef},
            plan4_recovery_base as base,
            semantic_row::structure::frame::Observation,
            wal_frame::{FrameKind, WalFrameHeader, WalStreamIdentity},
        };
        const RAW: &[u8; base::row::BASE_BYTES] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/structural-replay/internal-child-before.xrb"
        ));
        let profile = format::row_profile::decode_profile(
            &include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
            ))[..4096],
            None,
        )
        .unwrap();
        let expected = base::Expected {
            profile,
            filemap: format::SelectedControl {
                generation: 1,
                reference: format::ControlRef::decode(&RAW[384..448]).unwrap(),
            },
            bootstrap_digest: RAW[464..496].try_into().unwrap(),
        };
        let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
        let view = base::row::decode(RAW, expected, &mut scratch).unwrap();
        let mut arena = vec![0; 64 * PAGE_BYTES];
        for i in 0..64 {
            if let Some(page) = view.image(i).unwrap() {
                arena[i * PAGE_BYTES..(i + 1) * PAGE_BYTES].copy_from_slice(page);
            }
        }
        let numbers = [0usize, 10, 17, 21];
        let headers: [PageHeader; 4] = numbers.map(|i| {
            let cert = view.certificate(i).unwrap().unwrap();
            let image = view.image(i).unwrap().unwrap();
            page::decode_page(image, cert.header.identity, cert.header.kind)
                .unwrap()
                .header()
        });
        let mut right = headers[2];
        right.identity.page = PageRef {
            page_no: 19,
            incarnation: Generation::new(3).unwrap(),
            ..right.identity.page
        };
        let observed = Observation {
            header: WalFrameHeader {
                stream: WalStreamIdentity {
                    database: profile.spec().namespace.database,
                    timeline: 1,
                    writer_epoch: 1,
                },
                kind: FrameKind::System,
                lsn: 91,
                previous_lsn: 90,
                txn: None,
                previous_txn_lsn: 0,
            },
            operation: Split::ChildSplit,
            frame_bytes: Split::ChildSplit.frame_bytes(),
            targets: [headers[0], headers[1], headers[2], right, headers[3]],
        };
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
            let _ = (
                owner,
                flusher,
                status,
                undo,
                private,
                reader_private,
                core,
                fence,
                state,
            );
            let (writer, _, _) = live.split();
            // This is a raw physical-helper fixture, not the original source
            // factory: no canonical admission or Prepared token is issued.
            let local_fence = crate::storage::session_fence::SessionFence::new();
            let local_core = PageConcurrency::new(&local_fence).unwrap();
            let binding = std::sync::atomic::AtomicU8::new(2);
            let mut pages: Vec<[u8; PAGE_BYTES]> = numbers
                .iter()
                .map(|i| *view.image(*i).unwrap().unwrap())
                .collect();
            let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
            for (frame, header) in frames.iter_mut().zip(headers) {
                frame
                    .register(TargetRequest {
                        identity: header.identity,
                        kind: header.kind,
                        level: header.level,
                    })
                    .unwrap();
            }
            let pool = SharedPool::new(&local_core, &local_fence, &binding, &frames).unwrap();
            let mut reader = local_core.try_owner(OwnerRole::Foreground).unwrap();
            let mut before = Box::new([[0xa7; PAGE_BYTES]; 9]);
            let stamps = writer
                .test_split_read4(&pool, &mut reader, &observed, &mut before, &arena)
                .unwrap();
            assert!(stamps[3].is_none());
            for (at, role) in [0usize, 1, 2, 4].into_iter().enumerate() {
                assert_eq!(
                    stamps[role],
                    Some(FrameStamp {
                        identity: headers[at].identity,
                        generation: 1,
                        page_lsn: PageLsn(0),
                        modification_counter: 0,
                        rec_lsn: None
                    })
                );
                assert_eq!(&before[role], view.image(numbers[at]).unwrap().unwrap());
            }
            assert_eq!(headers[3].level, 2);
            assert!(before[3].iter().all(|b| *b == 0xa7));
            assert_eq!(
                crate::storage::dirty_frame::init_test_support::claim_state(&pool),
                (false, 0)
            );
            reader.check_idle().unwrap();
            println!(
                "SMO_P02 raw physical fixture read4; parent role4 level2; no source/claim authority"
            );
        });
    });
}

#[test]
fn structural_prepare_original_live_two_rows_preserves_state_and_binds_stamps() {
    case_within("SMO-P01", std::time::Duration::from_secs(180), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live,
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
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    view,
                    &table,
                    private,
                    [7, 8]
                );
                assert_eq!(
                    a_commit!(&mut writer, owner, flush, flusher, tx, private),
                    2
                );
                assert_eq!(
                    drained!(&mut writer, owner, flush, flusher, private),
                    done(11, 0)
                );
                let source = writer.catalog_test_with_pool(|pool| {
                    (0..64)
                        .filter_map(|number| probe::registered_index(pool, number))
                        .map(|at| pool.target_request(at).unwrap())
                        .find(|r| r.kind == PageKind::RowLeaf && r.identity.owner.get() == 6)
                        .unwrap()
                });
                let object = ObjectHandle {
                    object: ObjectId::new(6).unwrap(),
                    generation: Generation::new(1).unwrap(),
                    schema_epoch: SchemaEpoch::new(1).unwrap(),
                };
                let physical_before = writer.catalog_test_with_pool(|pool| {
                    let mut page = vec![0; PAGE_BYTES].into_boxed_slice();
                    (0..64)
                        .filter_map(|number| probe::registered_index(pool, number))
                        .map(|at| {
                            let target = pool.target_request(at).unwrap();
                            {
                                (
                                    target,
                                    probe::diagnose(pool, at, page.as_mut().try_into().unwrap())
                                        .frame,
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                });
                // Hold the last source content latch through the original
                // write target set; the new read must unwind its earlier pins.
                {
                    let pool = writer.catalog_test_pool();
                    let mut blocker = core.try_owner(OwnerRole::Foreground).unwrap();
                    let held = pool
                        .reserve_target_set(&mut blocker, &[source])
                        .unwrap()
                        .try_write()
                        .unwrap();
                    let e = match writer.prepare_row_split(
                        owner,
                        object,
                        source.identity.page,
                        private,
                    ) {
                        Err(e) => e,
                        Ok(_) => panic!("last target is held"),
                    };
                    assert_eq!(e.effect, Effect::Unchanged);
                    assert_eq!(
                        crate::storage::dirty_frame::init_test_support::claim_state(pool),
                        (false, 1)
                    );
                    held.release_checked().unwrap();
                    assert_eq!(
                        crate::storage::dirty_frame::init_test_support::claim_state(pool),
                        (false, 0)
                    );
                }
                owner.check_idle().unwrap();
                let wal = files(state).0;
                let jobs = writer.catalog_test_jobs().to_vec();
                let bank = writer.recovery_test_bank().to_vec();
                // A foreign owner cannot borrow this original source/pool.
                let mut other = foreign.try_owner(OwnerRole::Foreground).unwrap();
                let e = match writer.prepare_row_split(
                    &mut other,
                    object,
                    source.identity.page,
                    private,
                ) {
                    Err(e) => e,
                    Ok(_) => panic!("foreign owner"),
                };
                assert_eq!(e.effect, Effect::Unchanged);
                assert!(!fence.is_fenced());
                // A valid handle with a stale schema and a wrong incarnation
                // are rejected by the canonical source, without installation.
                for (handle, page) in [
                    (
                        ObjectHandle {
                            schema_epoch: SchemaEpoch::new(2).unwrap(),
                            ..object
                        },
                        source.identity.page,
                    ),
                    (
                        object,
                        crate::storage::page::PageRef {
                            incarnation: Generation::new(99).unwrap(),
                            ..source.identity.page
                        },
                    ),
                ] {
                    let e = match writer.prepare_row_split(owner, handle, page, private) {
                        Err(e) => e,
                        Ok(_) => panic!("stale source"),
                    };
                    assert_eq!(e.effect, Effect::Unchanged);
                    owner.check_idle().unwrap();
                }
                {
                    let prepared = writer
                        .prepare_row_split(owner, object, source.identity.page, private)
                        .unwrap();
                    let (raw, after, requests, roles, stamps) = prepared.test_parts();
                    assert_eq!(raw.len(), Split::RootSplit.frame_bytes());
                    assert_eq!(stamps.iter().filter(|s| s.is_some()).count(), 3);
                    assert!(
                        requests
                            .windows(2)
                            .all(|p| p[0].identity.page.page_no < p[1].identity.page.page_no)
                    );
                    for (at, role) in roles.iter().enumerate() {
                        assert_eq!(stamps[at].is_some(), *role < 3);
                        if *role < 3 {
                            assert_eq!(
                                stamps[at],
                                physical_before
                                    .iter()
                                    .find(|(target, _)| *target == requests[at])
                                    .map(|(_, stamp)| *stamp)
                            );
                        }
                        let page = crate::storage::page::decode_page(
                            &after[usize::from(*role)],
                            requests[at].identity,
                            requests[at].kind,
                        )
                        .unwrap();
                        assert_eq!(page.header().level, requests[at].level);
                    }
                    for (role, row) in [(2, rows[0].row()), (3, rows[1].row())] {
                        let at = roles.iter().position(|r| usize::from(*r) == role).unwrap();
                        let request = requests[at];
                        let tree = crate::storage::tree_page::decode_tree_page(
                            &after[role],
                            crate::storage::tree_page::TreeExpectation {
                                identity: request.identity,
                                kind: request.kind,
                                level: request.level,
                                arity: 0,
                            },
                        )
                        .unwrap();
                        assert_eq!(tree.len(), 1);
                        assert_eq!(tree.entry(0).unwrap().key, row.get().to_be_bytes());
                    }
                }
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(writer.recovery_test_bank(), bank);
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                writer.test_structural_cut_guards();
                writer.catalog_test_with_pool(|pool| {
                    let (entered, arrived) = std::sync::mpsc::sync_channel(1);
                    let (resume, release) = std::sync::mpsc::sync_channel(1);
                    probe::install_release_pause(pool, entered, release);
                    std::thread::scope(|scope| {
                        let controller = scope.spawn(move || {
                            arrived
                                .recv_timeout(std::time::Duration::from_secs(5))
                                .unwrap();
                            fence.fence();
                            resume.send(probe::Resume::Continue).unwrap();
                        });
                        let e = match writer.prepare_row_split(
                            owner,
                            object,
                            source.identity.page,
                            private,
                        ) {
                            Err(e) => e,
                            Ok(_) => panic!("fenced release returned Prepared"),
                        };
                        assert_eq!(e.effect, Effect::CallerMustResolve);
                        controller.join().unwrap();
                    });
                    assert_eq!(
                        crate::storage::dirty_frame::init_test_support::claim_state(pool),
                        (false, 0)
                    );
                });
                assert!(writer.test_structural_replay_returned());
                assert!(fence.is_fenced());
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(writer.recovery_test_bank(), bank);
                println!(
                    "SMO_P01 actual replay; existing3 exact stamps; Busy and fenced cleanup; no WAL/control effect"
                );
            }
        );
    });
}
