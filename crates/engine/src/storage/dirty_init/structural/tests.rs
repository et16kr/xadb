//! Metadata-only fixtures in the original pool. Opaque image bytes provide
//! no canonical tree, allocation, WAL stage or durable completion authority.
use super::*;
use crate::api::Namespace;
use crate::storage::page_io::test_support::expected;
use crate::transaction::concurrency::OwnerRole;
use foundation::identity::{Generation, ObjectId};

fn target(page: u64, owner: u64, kind: PageKind, level: u8) -> TargetRequest {
    let mut identity = expected();
    identity.page.page_no = page;
    identity.page.incarnation = Generation::new(if page == 0 { 1 } else { 2 }).unwrap();
    identity.owner = ObjectId::new(owner).unwrap();
    identity.owner_generation = Generation::new(1).unwrap();
    TargetRequest {
        identity,
        kind,
        level,
    }
}

fn fixture(
    root: bool,
    level: u8,
    run: impl FnOnce(&SharedPool<'_, '_, '_, '_>, StructuralClaimRecord),
) {
    let mut bytes = vec![[0xa7; PAGE_BYTES]; 7];
    let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
    let allocator = target(0, 1, PageKind::Allocation, 0);
    frames[0].register(allocator).unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let mut pool = SharedPool::new_plan4(&core, &fence, &binding, &frames).unwrap();
    pool.activate_plan4_retention().unwrap();
    let row_kind = if level == 0 {
        PageKind::RowLeaf
    } else {
        PageKind::RowInternal
    };
    let right = target(20, 6, row_kind, level);
    let record = StructuralClaimRecord {
        init: [
            right,
            if root {
                target(21, 6, PageKind::RowInternal, level + 1)
            } else {
                right
            },
        ],
        work: SystemWorkRef::new(
            Namespace {
                database: allocator.identity.database,
                timeline: 1,
            },
            SystemWorkKind::StructuralChange,
            7,
            Generation::new(2).unwrap(),
        )
        .unwrap(),
        birth: PageLsn(7),
        generations: if root { [2, 3] } else { [2, 0] },
        indices: if root { [3, 4] } else { [4, 0] },
        init_count: if root { 2 } else { 1 },
        target_mask: 31,
    };
    pool.directory_edit(|directory| {
        for (index, request) in [
            (1, target(11, 6, PageKind::TreeAnchor, 0)),
            (2, target(12, 6, row_kind, level)),
            (5, target(30, 7, PageKind::RowLeaf, 0)),
            (6, target(31, 7, PageKind::RowLeaf, 0)),
        ] {
            directory.replace_retention(
                index,
                Registration {
                    request,
                    generation: 1,
                },
            );
        }
        if root {
            directory.prepare_retention_pair([3, 4], record.generations);
        } else {
            directory.replace_retention(
                3,
                Registration {
                    request: target(13, 6, PageKind::RowInternal, level + 1),
                    generation: 1,
                },
            );
            directory.prepare_retention_slot(4, record.generations[0]);
        }
    })
    .unwrap();
    {
        let mut state = pool.entry_registry().unwrap();
        assert!(record.valid_in(&pool, &state));
        state.init_claim = Some(InitClaimSlot {
            identity: InitClaimIdentity::Structural(record),
            initializer_pins_held: false,
        });
    }
    run(&pool, record);
}

fn publish(pool: &SharedPool<'_, '_, '_, '_>, record: StructuralClaimRecord) {
    let mut state = pool.entry_registry().unwrap();
    let first = Registration {
        request: record.init[0],
        generation: record.generations[0],
    };
    if record.init_count == 1 {
        assert!(state.directory.publish(first));
    } else {
        assert!(state.directory.publish_pair(
            [
                first,
                Registration {
                    request: record.init[1],
                    generation: record.generations[1],
                }
            ],
            pool
        ));
    }
    assert!(record.valid_in(pool, &state));
}

#[test]
fn structural_claim_preserves_prior_layout_and_entry_budget() {
    use std::mem::size_of;
    let actual = [
        size_of::<InitClaimRecord>(),
        size_of::<super::super::tx_pair::TxPairClaimRecord>(),
        size_of::<InitClaimIdentity>(),
        size_of::<InitClaimSlot>(),
        size_of::<Option<InitClaimSlot>>(),
        size_of::<PoolRegistry<'_>>(),
    ];
    // Real pre-change types measured on this toolchain, not field arithmetic:
    // module-gate/evidence/dev3-08-claim/prior-layout.{patch,log}.
    assert_eq!(size_of::<usize>(), 8);
    assert_eq!(actual, [216, 216, 224, 232, 232, 13616]);
    assert_eq!(size_of::<StructuralClaimRecord>(), 216);
    println!(
        "STRUCTURAL_LAYOUT actual={actual:?} structural={} claim_source={} entry_phases={:?}",
        size_of::<StructuralClaimRecord>(),
        source_observation(),
        super::super::super::retention_residency::entry_source_phases()
    );
    assert!(super::super::fits());
}

#[test]
fn structural_claim_accepts_root_child_shapes_and_all_active_pins() {
    for root in [false, true] {
        for level in [0, 1, 14] {
            fixture(root, level, |pool, record| {
                let state = pool.entry_registry().unwrap();
                assert!(record.valid_in(pool, &state));
                let slot = state.init_claim.as_ref().unwrap();
                assert!(slot.blocks_ordinary());
                assert!(slot.system().is_none());
                assert!(!slot.owns_unpublished_pin(0));
                assert!(!slot.owns_unpublished_pin(4));
                drop(state);
                {
                    let mut state = pool.entry_registry().unwrap();
                    state.init_claim.as_mut().unwrap().initializer_pins_held = true;
                    for index in 0..5 {
                        state.counts[index] = 1;
                    }
                    state.total = 5;
                    assert!(pool.init_pins_valid(&state));
                    let slot = state.init_claim.as_ref().unwrap();
                    for index in 0..7 {
                        assert_eq!(slot.owns_unpublished_pin(index), record.owns_index(index));
                        assert_eq!(
                            slot.diagnostic_registration(index).is_some(),
                            record.owns_index(index)
                        );
                    }
                    for index in 0..5 {
                        state.counts[index] = 0;
                        assert!(!pool.init_pins_valid(&state), "missing target pin {index}");
                        state.counts[index] = 1;
                    }
                    state.counts.fill(0);
                    state.total = 0;
                    state.init_claim.as_mut().unwrap().initializer_pins_held = false;
                }
                publish(pool, record);
                assert!(!pool.fence.is_fenced());
            });
        }
    }
}

#[test]
fn structural_claim_rejects_malformed_count_identity_and_mask_without_panics() {
    for root in [false, true] {
        fixture(root, 0, |pool, record| {
            let state = pool.entry_registry().unwrap();
            for variant in 0..23 {
                let mut bad = record;
                match variant {
                    0 => bad.init_count = 0,
                    1 => bad.init_count = 3,
                    2 => bad.init_count = u8::MAX,
                    3 => bad.birth = PageLsn(0),
                    4 => bad.birth = PageLsn(8),
                    5 => bad.generations[0] = 0,
                    6 => bad.generations[0] += 1,
                    7 => bad.indices[0] = 0,
                    8 => bad.indices[0] = 7,
                    9 => bad.indices[0] = u8::MAX,
                    10 => bad.target_mask = 15,
                    11 => bad.target_mask = 63,
                    12 => bad.target_mask = 62,
                    13 => bad.target_mask = (31 & !(1 << record.indices[0])) | (1 << 5),
                    14 => bad.target_mask = (31 & !(1 << 2)) | (1 << 7),
                    15 => bad.init[0].level = 15,
                    16 => bad.init[0].kind = PageKind::IndexLeaf,
                    17 => bad.init[0].identity.owner = ObjectId::new(5).unwrap(),
                    18 => bad.init[0].identity.owner_generation = Generation::new(2).unwrap(),
                    19 => bad.init[0].identity.page.incarnation = Generation::new(1).unwrap(),
                    20 => bad.init[0].identity.page.page_no = 64,
                    21 => {
                        bad.work = SystemWorkRef::new(
                            record.work.namespace(),
                            SystemWorkKind::AllocationMaintenance,
                            7,
                            Generation::new(2).unwrap(),
                        )
                        .unwrap()
                    }
                    22 => bad.target_mask = (31 & !(1 << 2)) | (1 << 5),
                    _ => unreachable!(),
                }
                assert!(
                    !bad.valid_in(pool, &state),
                    "root={root} mutation={variant}"
                );
            }
            assert!(record.valid_in(pool, &state));
            assert!(!pool.fence.is_fenced());
        });
    }
    fixture(false, 0, |pool, record| {
        let state = pool.entry_registry().unwrap();
        for variant in 0..3 {
            let mut bad = record;
            match variant {
                0 => bad.init[1].identity.page.page_no += 1,
                1 => bad.generations[1] = 1,
                2 => bad.indices[1] = 1,
                _ => unreachable!(),
            }
            assert!(!bad.valid_in(pool, &state));
        }
        assert!(!record.owns_index(0));
        assert!(record.diagnostic_registration(0).is_none());
    });
    fixture(true, 14, |pool, record| {
        let state = pool.entry_registry().unwrap();
        for variant in 0..4 {
            let mut bad = record;
            match variant {
                0 => bad.init[1].identity.page = bad.init[0].identity.page,
                1 => {
                    bad.init[1].identity.page.page_no = bad.init[0].identity.page.page_no;
                    bad.init[1].identity.page.incarnation = Generation::new(3).unwrap();
                }
                2 => bad.init[1].level = 14,
                3 => bad.indices[1] = bad.indices[0],
                _ => unreachable!(),
            }
            assert!(!bad.valid_in(pool, &state));
        }
    });
}

#[test]
fn structural_claim_rejects_wrong_closed_state_and_published_identity() {
    for root in [false, true] {
        fixture(root, 0, |pool, record| {
            publish(pool, record);
            let mut state = pool.entry_registry().unwrap();
            let index = usize::from(record.indices[0]);
            let exact = state.directory.resolve(pool.frames, index).unwrap();
            for variant in 0..3 {
                let mut wrong = exact;
                match variant {
                    0 => wrong.generation += 1,
                    1 => wrong.request.identity.page.incarnation = Generation::new(3).unwrap(),
                    2 => wrong.request.identity.owner = ObjectId::new(7).unwrap(),
                    _ => unreachable!(),
                }
                state.directory.replace_retention(index, wrong);
                assert!(!record.valid_in(pool, &state));
                state.directory.replace_retention(index, exact);
            }
            if root {
                state.directory.finish_retention_pair();
            } else {
                assert!(state.directory.finish_retention_slot(index, exact));
            }
            assert!(!record.valid_in(pool, &state));
            state.init_claim = None;
            assert!(pool.registry_valid(&state));
        });
    }
}

#[test]
fn structural_single_finish_preserves_resident_and_consumed_generation() {
    fixture(false, 0, |pool, record| {
        let expected = Registration {
            request: record.init[0],
            generation: record.generations[0],
        };
        {
            let mut state = pool.entry_registry().unwrap();
            assert!(!state.directory.finish_retention_slot(4, expected));
            assert_eq!(state.directory.closed_index(), Some(4));
        }
        publish(pool, record);
        let mut state = pool.entry_registry().unwrap();
        let successor = state.directory.next_retention_generation(4);
        assert!(!state.directory.finish_retention_slot(3, expected));
        assert!(!state.directory.finish_retention_slot(
            4,
            Registration {
                generation: 99,
                ..expected
            }
        ));
        assert_eq!(state.directory.closed_index(), Some(4));
        assert!(state.directory.finish_retention_slot(4, expected));
        assert_eq!(state.directory.closed_index(), None);
        assert_eq!(state.directory.resolve(pool.frames, 4), Some(expected));
        assert_eq!(state.directory.next_retention_generation(4), successor);
        state.init_claim = None;
        assert!(pool.registry_valid(&state));
        // A later unpublished cancellation consumes a fresh slot generation.
        state
            .directory
            .prepare_retention_slot(4, successor.unwrap());
        state.directory.retire_retention(4);
        assert_eq!(state.directory.next_retention_generation(4), Some(4));
        assert!(state.directory.resolve(pool.frames, 4).is_none());
        assert!(state.directory.closed_index().is_none());
    });
    fixture(true, 0, |pool, record| {
        publish(pool, record);
        let mut state = pool.entry_registry().unwrap();
        let before = state.directory.resolve(pool.frames, 3);
        assert!(!state.directory.finish_retention_slot(3, before.unwrap()));
        assert_eq!(state.directory.closed_pair(), Some([3, 4]));
        assert_eq!(state.directory.resolve(pool.frames, 3), before);
    });
}

#[test]
fn structural_directory_allows_only_dynamic_row_internal_levels() {
    fixture(false, 0, |pool, _| {
        let state = pool.entry_registry().unwrap();
        for page in [8, 9, 63, 64] {
            for level in [0, 1, 14, 15, 16, 255] {
                let request = target(page, 6, PageKind::RowInternal, level);
                assert_eq!(
                    state.directory.accepts_retention_payload(request),
                    (9..64).contains(&page) && (1..=15).contains(&level)
                );
            }
        }
        for (kind, owner) in [
            (PageKind::TransactionStatus, 2),
            (PageKind::Undo, 3),
            (PageKind::TreeAnchor, 6),
            (PageKind::RowLeaf, 6),
        ] {
            assert!(
                state
                    .directory
                    .accepts_retention_payload(target(22, owner, kind, 0))
            );
            assert!(
                !state
                    .directory
                    .accepts_retention_payload(target(22, owner, kind, 1))
            );
        }
        for kind in [
            PageKind::CatalogInternal,
            PageKind::IndexInternal,
            PageKind::IndexLeaf,
        ] {
            assert!(
                !state
                    .directory
                    .accepts_retention_payload(target(22, 6, kind, 1))
            );
        }
        for owner in [1, 2, 3, 4, 5] {
            assert!(!state.directory.accepts_retention_payload(target(
                22,
                owner,
                PageKind::RowInternal,
                1
            )));
        }
    });
}

#[test]
fn structural_ordinary_gate_blocks_original_pin_load_and_capture() {
    for root in [false, true] {
        fixture(root, 0, |pool, record| {
            let allocator = target(0, 1, PageKind::Allocation, 0);
            let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            let error = pool
                .reserve_target_set(&mut owner, &[allocator])
                .err()
                .unwrap();
            assert_eq!(
                (error.cause, error.effect),
                (Cause::Busy, Effect::Unchanged)
            );
            let error = pool
                .claim_retention_load(&mut owner, record.init[0], 1, &[])
                .err()
                .unwrap();
            assert_eq!(
                (error.cause, error.effect),
                (Cause::Busy, Effect::Unchanged)
            );
            owner.check_idle().unwrap();
            drop(owner);
            let mut flusher = pool.core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let mut output = [0x5a; PAGE_BYTES];
            let error = pool
                .capture_flush(&mut flusher, allocator, &mut output)
                .err()
                .unwrap();
            assert_eq!(
                (error.cause, error.effect),
                (Cause::Busy, Effect::Unchanged)
            );
            assert_eq!(output, [0x5a; PAGE_BYTES]);
            flusher.check_idle().unwrap();
            let state = pool.entry_registry().unwrap();
            assert_eq!(state.total, 0);
            assert!(state.load.is_none() && state.claim.is_none());
            let slot = state.init_claim.as_ref().unwrap();
            assert!(slot.blocks_load(0, allocator));
            let mut prior_incarnation = record.init[0];
            prior_incarnation.identity.page.incarnation = Generation::new(99).unwrap();
            assert!(slot.blocks_load(6, prior_incarnation));
            assert!(!slot.blocks_load(6, target(31, 7, PageKind::RowLeaf, 0)));
            assert!(!pool.fence.is_fenced());
        });
    }
}
