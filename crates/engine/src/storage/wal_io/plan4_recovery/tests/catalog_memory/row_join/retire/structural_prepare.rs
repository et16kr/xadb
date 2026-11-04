//! The new original-source split preparer over real CREATE/INSERT/cleanup.
//! Expected row partition and unchanged live state are independent facts.
use super::*;
use crate::storage::{page::PageKind, semantic_row::structure::Operation as Split};
use foundation::identity::{ObjectHandle, SchemaEpoch};

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
