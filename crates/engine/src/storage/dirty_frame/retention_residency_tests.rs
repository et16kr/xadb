//! Metadata-only selection fixtures. These do not establish runtime, WAL or
//! durability authority; the actual CREATE progress evidence is separate.
use super::*;
use crate::storage::page_io::test_support::expected;
use foundation::identity::{Generation, ObjectId};

fn request(page: u64) -> TargetRequest {
    let mut identity = expected();
    identity.page.page_no = page;
    identity.page.incarnation = Generation::new(1).unwrap();
    identity.owner_generation = Generation::new(1).unwrap();
    let (kind, owner) = match page {
        0 => (PageKind::Allocation, 1),
        1 => (PageKind::RetentionRoot, 4),
        2..=6 => (PageKind::RetentionJobs, 4),
        7 => (PageKind::TreeAnchor, 5),
        8 => (PageKind::CatalogLeaf, 5),
        _ => unreachable!(),
    };
    identity.owner = ObjectId::new(owner).unwrap();
    TargetRequest {
        identity,
        kind,
        level: 0,
    }
}

fn with_pool(count: usize, run: impl FnOnce(&mut SharedPool<'_, '_, '_, '_>)) {
    let mut bytes = vec![[0xa7; PAGE_BYTES]; count];
    let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
    frames[0].register(request(0)).unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let mut pool = SharedPool::new_plan4(&core, &fence, &binding, &frames).unwrap();
    pool.activate_plan4_retention().unwrap();
    pool.directory_edit(|directory| {
        for index in 1..count {
            directory.replace_retention(
                index,
                Registration {
                    request: request(index as u64),
                    generation: 1,
                },
            );
        }
    })
    .unwrap();
    run(&mut pool);
}

type FrameContents = (Vec<u8>, PageLsn, u64, Option<PageLsn>, bool);

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    registrations: Vec<Option<Registration>>,
    successors: Vec<Option<u64>>,
    contents: Vec<FrameContents>,
    entries: [Option<DptEntry>; MAX_FRAMES],
    counts: [u16; MAX_FRAMES],
    total: u16,
}

fn snapshot(pool: &SharedPool<'_, '_, '_, '_>) -> Snapshot {
    let contents = pool
        .frames
        .iter()
        .map(|frame| {
            let content = frame.content.read().unwrap();
            (
                content.bytes.to_vec(),
                content.page_lsn,
                content.modification_counter,
                content.rec_lsn,
                content.dirty,
            )
        })
        .collect();
    let state = pool.registry.lock().unwrap();
    assert!(state.init_claim.is_none() && state.claim.is_none() && state.load.is_none());
    assert!(state.requirements.iter().all(Option::is_none));
    assert_eq!((state.last_attempt, state.last_load_attempt), (0, 0));
    assert_eq!(state.directory.closed_index(), None);
    assert_eq!(state.directory.closed_pair(), None);
    Snapshot {
        registrations: (0..pool.frames.len())
            .map(|index| state.directory.resolve(pool.frames, index))
            .collect(),
        successors: (0..pool.frames.len())
            .map(|index| state.directory.next_retention_generation(index))
            .collect(),
        contents,
        entries: state.entries,
        counts: state.counts,
        total: state.total,
    }
}

fn reject(result: Result<[StatusCandidate; 2]>, cause: Cause, effect: Effect) {
    match result {
        Err(error) => assert_eq!(error, DirtyError { cause, effect }),
        Ok(_) => panic!("invalid pair admitted"),
    }
}

#[test]
fn pair_room_second_victim_missing_is_unchanged() {
    with_pool(8, |pool| {
        let before = snapshot(pool);
        let required = std::array::from_fn(|index| request(index as u64));
        reject(
            pool.retention_pair_room(1, &required),
            Cause::Busy,
            Effect::Unchanged,
        );
        assert_eq!(snapshot(pool), before);
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn pair_room_generation_exhaustion_is_unchanged() {
    with_pool(9, |pool| {
        pool.directory_edit(|directory| {
            directory.replace_retention(
                8,
                Registration {
                    request: request(8),
                    generation: u64::MAX,
                },
            )
        })
        .unwrap();
        let before = snapshot(pool);
        let required = std::array::from_fn(|index| request(index as u64));
        reject(
            pool.retention_pair_room(1, &required),
            Cause::CounterExhausted,
            Effect::Unchanged,
        );
        assert_eq!(snapshot(pool), before);
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn pair_room_latch_busy_is_unchanged() {
    with_pool(9, |pool| {
        let before = snapshot(pool);
        let required = std::array::from_fn(|index| request(index as u64));
        let frames = pool.frames;
        let held = frames[8].content.write().unwrap();
        reject(
            pool.retention_pair_room(1, &required),
            Cause::Busy,
            Effect::Unchanged,
        );
        drop(held);
        assert_eq!(snapshot(pool), before);
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn pair_room_missing_rec_lsn_fences_before_candidate() {
    with_pool(9, |pool| {
        {
            let mut content = pool.frames[8].content.write().unwrap();
            content.dirty = true;
            content.page_lsn = PageLsn(1);
            content.modification_counter = 1;
            assert_eq!(content.rec_lsn, None);
        }
        let before = snapshot(pool);
        let required = std::array::from_fn(|index| request(index as u64));
        reject(
            pool.retention_pair_room(1, &required),
            Cause::Invariant,
            Effect::CallerMustResolve,
        );
        assert_eq!(snapshot(pool), before);
        assert!(pool.fence.is_fenced());
    });
}
