//! Original-directory selection only. Metadata fixtures do not establish
//! allocator/WAL/INIT authority; no claim or persistent image is installed.
use super::*;
use foundation::identity::{Generation, ObjectId};

fn target(number: u64, kind: PageKind, level: u8) -> TargetRequest {
    let mut identity = crate::storage::page_io::test_support::expected();
    identity.page.page_no = number;
    identity.page.incarnation = Generation::new(if number == 0 { 1 } else { 2 }).unwrap();
    identity.owner = ObjectId::new(if number == 0 { 1 } else { 6 }).unwrap();
    identity.owner_generation = Generation::new(1).unwrap();
    TargetRequest {
        identity,
        kind,
        level,
    }
}

fn with_pool(
    child: bool,
    victims: usize,
    run: impl FnOnce(&SharedPool<'_, '_, '_, '_>, &[TargetRequest; 5], &[TargetRequest]),
) {
    let allocator = target(0, PageKind::Allocation, 0);
    let anchor = target(10, PageKind::TreeAnchor, 0);
    let source = target(11, PageKind::RowLeaf, 0);
    let right = target(17, PageKind::RowLeaf, 0);
    let root = target(18, PageKind::RowInternal, 1);
    let parent = target(12, PageKind::RowInternal, 1);
    let existing = if child {
        vec![allocator, anchor, source, parent]
    } else {
        vec![allocator, anchor, source]
    };
    let mut bytes = vec![[0xa7; PAGE_BYTES]; existing.len() + victims];
    let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
    frames[0].register(allocator).unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let mut pool = SharedPool::new_plan4(&core, &fence, &binding, &frames).unwrap();
    pool.activate_plan4_retention().unwrap();
    pool.directory_edit(|directory| {
        for (index, request) in existing.iter().enumerate().skip(1) {
            directory.replace_retention(
                index,
                Registration {
                    request: *request,
                    generation: 1,
                },
            );
        }
    })
    .unwrap();
    let mut targets = [
        allocator,
        anchor,
        source,
        right,
        if child { parent } else { root },
    ];
    targets.sort_by_key(TargetRequest::rank);
    let init = [right, root];
    run(&pool, &targets, &init[..if child { 1 } else { 2 }]);
}

fn state(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<(Option<Registration>, Option<u64>, u16)> {
    let registry = pool.entry_registry().unwrap();
    assert!(registry.init_claim.is_none() && registry.load.is_none() && registry.claim.is_none());
    assert_eq!(registry.total, 0);
    (0..pool.frames.len())
        .map(|index| {
            (
                registry.directory.resolve(pool.frames, index),
                registry.directory.next_retention_generation(index),
                registry.counts[index],
            )
        })
        .collect()
}

fn rejected(result: Result<StructuralCandidates<'_, '_, '_>>, cause: Cause) {
    match result {
        Err(error) => assert_eq!(error, unchanged(cause)),
        Ok(_) => panic!("invalid selection admitted"),
    }
}

impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    /// Separate metadata pool with an authentic requirement borrowed from the
    /// calling fixture. This does not claim an original live source or flush.
    pub(in crate::storage) fn test_structural_dirty_selection(
        &self,
        requirement: WalRequirement<'s>,
    ) {
        assert!(requirement.uses_fence(self.fence));
        let mut bytes = vec![[0xa7; PAGE_BYTES]; 5];
        let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
        let mut requests = [
            target(0, PageKind::Allocation, 0),
            target(10, PageKind::TreeAnchor, 0),
            target(11, PageKind::RowLeaf, 0),
            target(12, PageKind::RowInternal, 1),
            target(30, PageKind::RowLeaf, 0),
        ];
        let original = self.target_request(0).unwrap();
        for request in &mut requests {
            request.identity.database = original.identity.database;
            request.identity.page.file = original.identity.page.file;
        }
        frames[0].register(requests[0]).unwrap();
        let mut pool = SharedPool::new_plan4(self.core, self.fence, self.binding, &frames).unwrap();
        pool.activate_plan4_retention().unwrap();
        pool.directory_edit(|directory| {
            for (index, request) in requests.iter().enumerate().skip(1) {
                directory.replace_retention(
                    index,
                    Registration {
                        request: *request,
                        generation: 1,
                    },
                );
            }
        })
        .unwrap();
        let lsn = PageLsn(requirement.location().lsn);
        {
            let mut content = frames[4].content.write().unwrap();
            content.page_lsn = lsn;
            content.modification_counter = 1;
            content.rec_lsn = Some(lsn);
            content.dirty = true;
            let mut registry = pool.registry.lock().unwrap();
            registry.entries[4] = Some(DptEntry {
                generation: 1,
                rec_lsn: lsn,
                modification_counter: 1,
            });
            registry.requirements[4] = Some(requirement);
        }
        let mut init = requests[2];
        init.identity.page.page_no = 17;
        let targets = [requests[0], requests[1], requests[2], requests[3], init];
        let before = state(&pool);
        let selected = pool.structural_candidates(&[init], 1, &targets).unwrap();
        assert!(selected.requires_flush());
        assert_eq!(selected.slots[0].kind, StatusCandidateKind::Flush);
        assert_eq!(selected.slots[0].index, 4);
        assert_eq!(selected.next_generations, [2, 0]);
        assert_eq!(state(&pool), before);
        let registry = pool.entry_registry().unwrap();
        assert_eq!(
            registry.requirements[4].unwrap().location(),
            requirement.location()
        );
        assert_eq!(registry.entries[4].unwrap().rec_lsn, lsn);
        assert!(!pool.fence.is_fenced());
    }
}

#[test]
fn structural_selection_root_two_child_one_preserves_registry_and_excludes_all_targets() {
    for child in [false, true] {
        let count = if child { 1 } else { 2 };
        with_pool(child, count, |pool, targets, init| {
            let before = state(pool);
            let selected = pool.structural_candidates(init, 1, targets).unwrap();
            assert_eq!(usize::from(selected.count), count);
            assert!(!selected.requires_flush());
            assert_eq!(selected.targets, *targets);
            assert!(std::ptr::eq(selected.anchor, &pool.stamp_anchor));
            assert!(std::ptr::eq(selected.core, pool.core));
            assert!(std::ptr::eq(selected.binding, pool.binding));
            assert!(std::ptr::eq(selected.fence, pool.fence));
            for at in 0..count {
                assert!(selected.slots[at].index >= if child { 4 } else { 3 });
                assert_eq!(selected.next_generations[at], 1);
                assert_eq!(selected.slots[at].frame, None);
            }
            if child {
                assert_eq!(selected.init[1], selected.init[0]);
                assert_eq!(selected.slots[1].index, selected.slots[0].index);
                assert_eq!(selected.next_generations[1], 0);
            } else {
                assert_ne!(selected.slots[0].index, selected.slots[1].index);
            }
            assert_eq!(state(pool), before);
            assert!(!pool.fence.is_fenced());
        });
    }
    with_pool(false, 1, |pool, targets, init| {
        let before = state(pool);
        rejected(pool.structural_candidates(init, 1, targets), Cause::Busy);
        assert_eq!(state(pool), before);
    });
}

#[test]
fn structural_selection_resident_stale_generation_exhaustion_and_busy_are_unchanged() {
    with_pool(true, 1, |pool, targets, init| {
        let index = 4;
        pool.directory_edit(|d| {
            d.replace_retention(
                index,
                Registration {
                    request: init[0],
                    generation: 3,
                },
            )
        })
        .unwrap();
        let before = state(pool);
        rejected(
            pool.structural_candidates(init, 1, targets),
            Cause::AlreadyInitialized,
        );
        let mut newer = init[0];
        newer.identity.page.incarnation = Generation::new(3).unwrap();
        let mut changed = *targets;
        *changed.iter_mut().find(|r| **r == init[0]).unwrap() = newer;
        rejected(
            pool.structural_candidates(&[newer], 1, &changed),
            Cause::Stale,
        );
        assert_eq!(state(pool), before);
    });
    with_pool(true, 1, |pool, targets, init| {
        let extra = target(30, PageKind::RowLeaf, 0);
        pool.directory_edit(|d| {
            d.replace_retention(
                4,
                Registration {
                    request: extra,
                    generation: u64::MAX,
                },
            )
        })
        .unwrap();
        let before = state(pool);
        rejected(
            pool.structural_candidates(init, 1, targets),
            Cause::CounterExhausted,
        );
        assert_eq!(state(pool), before);
    });
    with_pool(true, 1, |pool, targets, init| {
        let before = state(pool);
        let guard = pool.frames[4].content.write().unwrap();
        rejected(pool.structural_candidates(init, 1, targets), Cause::Busy);
        drop(guard);
        assert_eq!(state(pool), before);
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn structural_selection_freeze_detects_stamp_change_and_tracks_successor_generation() {
    with_pool(true, 1, |pool, targets, init| {
        let extra = target(30, PageKind::RowLeaf, 0);
        pool.directory_edit(|d| {
            d.replace_retention(
                4,
                Registration {
                    request: extra,
                    generation: 4,
                },
            )
        })
        .unwrap();
        let old = pool.structural_candidates(init, 1, targets).unwrap();
        assert_eq!(old.next_generations, [5, 0]);
        pool.directory_edit(|d| {
            d.replace_retention(
                4,
                Registration {
                    request: extra,
                    generation: 5,
                },
            )
        })
        .unwrap();
        rejected(
            pool.freeze_structural_candidates(init, targets, old.slots),
            Cause::Stale,
        );
        let new = pool.structural_candidates(init, 1, targets).unwrap();
        assert_eq!(new.next_generations, [6, 0]);
        {
            let mut content = pool.frames[4].content.write().unwrap();
            content.modification_counter = 1;
            content.page_lsn = PageLsn(1);
        }
        rejected(
            pool.freeze_structural_candidates(init, targets, new.slots),
            Cause::Stale,
        );
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn structural_selection_rejects_invalid_count_order_alias_and_missing_init_without_change() {
    with_pool(false, 2, |pool, targets, init| {
        let before = state(pool);
        rejected(pool.structural_candidates(&[], 1, targets), Cause::Shape);
        rejected(
            pool.structural_candidates(&[init[0]; 3], 1, targets),
            Cause::Shape,
        );
        rejected(pool.structural_candidates(init, 0, targets), Cause::Shape);
        let mut swapped = *targets;
        swapped.swap(1, 2);
        rejected(
            pool.structural_candidates(init, 1, &swapped),
            Cause::DuplicateTarget,
        );
        let mut aliased = *targets;
        aliased[2] = aliased[1];
        rejected(
            pool.structural_candidates(init, 1, &aliased),
            Cause::DuplicateTarget,
        );
        let absent = target(31, PageKind::RowLeaf, 0);
        rejected(
            pool.structural_candidates(&[absent], 1, targets),
            Cause::DuplicateTarget,
        );
        rejected(
            pool.structural_candidates(&[init[0], init[0]], 1, targets),
            Cause::DuplicateTarget,
        );
        assert_eq!(state(pool), before);
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn structural_selection_source_covers_return_and_guarded_freeze() {
    use std::mem::size_of;
    let minimum = super::super::source_census()
        + size_of::<StructuralCandidates<'_, '_, '_>>()
        + 2 * size_of::<Result<StructuralCandidates<'_, '_, '_>>>()
        + size_of::<[Option<RwLockReadGuard<'_, FrameContent<'_>>>; 2]>()
        + size_of::<[Option<&RwLockReadGuard<'_, FrameContent<'_>>>; 2]>()
        + size_of::<MutexGuard<'_, PoolRegistry<'_>>>()
        + size_of::<[StatusCandidate; 2]>()
        + size_of::<[u64; 2]>();
    let bound = structural_selection_source_bytes();
    assert!(minimum <= bound);
    println!("STRUCTURAL_SELECTION_SOURCE bound={bound} partial_floor={minimum}");
}
