//! Metadata-only atomic close fixtures. Actual Reserve/Forward/Cancel and
//! durable recovery are selected separately through the original live harness.
use super::*;
use crate::storage::page_io::test_support::expected;
use crate::transaction::TxnId;
use foundation::identity::{Generation, ObjectId};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn request(page: u64) -> TargetRequest {
    let mut identity = expected();
    identity.page.page_no = page;
    identity.page.incarnation = Generation::new(if page >= 9 { 2 } else { 1 }).unwrap();
    identity.owner_generation = Generation::new(1).unwrap();
    let (owner, kind) = match page {
        0 => (1, PageKind::Allocation),
        1 => (4, PageKind::RetentionRoot),
        2..=6 => (4, PageKind::RetentionJobs),
        7 => (5, PageKind::TreeAnchor),
        8 => (5, PageKind::CatalogLeaf),
        9 => (2, PageKind::TransactionStatus),
        10 => (3, PageKind::Undo),
        11 => (6, PageKind::TreeAnchor),
        12 => (6, PageKind::RowLeaf),
        _ => unreachable!(),
    };
    identity.owner = ObjectId::new(owner).unwrap();
    TargetRequest {
        identity,
        kind,
        level: 0,
    }
}
fn fixture(
    run: impl for<'c, 's, 'p, 'd, 'i> FnOnce(
        &mut SharedPool<'c, 's, 'p, 'd>,
        &TxPairAuthority<'i, 'c, 's>,
    ),
) {
    let mut bytes = vec![[0xa7; PAGE_BYTES]; 9];
    let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
    frames[0].register(request(0)).unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let mut pool = SharedPool::new_plan4(&core, &fence, &binding, &frames).unwrap();
    pool.activate_plan4_retention().unwrap();
    // Existing indices0..6 have all seven distinct roles; clean victims7/8
    // have unrelated Job addresses. Their payload bytes are deliberately opaque.
    let existing = [0, 1, 2, 7, 8, 9, 10].map(request);
    pool.directory_edit(|directory| {
        for (index, target) in existing
            .iter()
            .copied()
            .enumerate()
            .skip(1)
            .chain([(7, request(3)), (8, request(4))])
        {
            directory.replace_retention(
                index,
                Registration {
                    request: target,
                    generation: 1,
                },
            );
        }
    })
    .unwrap();
    drop(
        pool.entry_registry()
            .expect("valid metadata fixture before selected fault"),
    );
    let actor = TxnRef::new(
        TxnId {
            database: existing[0].identity.database,
            timeline: 1,
            writer_epoch: 1,
            local: 1,
        },
        Generation::new(1).unwrap(),
    )
    .unwrap();
    let pair = [request(11), request(12)];
    let authority = TxPairAuthority::metadata_fixture(
        &core,
        &binding,
        &fence,
        actor,
        PageLsn(7),
        &existing,
        &pair,
    );
    run(&mut pool, &authority);
}

type Contents = (Vec<u8>, PageLsn, u64, Option<PageLsn>, bool);
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    registrations: Vec<Option<Registration>>,
    successors: Vec<Option<u64>>,
    contents: Vec<Contents>,
    closed: Option<[usize; 2]>,
    claim: Option<TxPairClaimRecord>,
    counts: [u16; MAX_FRAMES],
    total: u16,
    entries: [Option<DptEntry>; MAX_FRAMES],
    requirements: Vec<Option<u64>>,
    attempts: (u64, u64),
}
fn snapshot(pool: &SharedPool<'_, '_, '_, '_>) -> Snapshot {
    let contents = pool
        .frames
        .iter()
        .map(|frame| {
            let content = frame.content.read().unwrap_or_else(|e| e.into_inner());
            (
                content.bytes.to_vec(),
                content.page_lsn,
                content.modification_counter,
                content.rec_lsn,
                content.dirty,
            )
        })
        .collect();
    let state = pool.registry.lock().unwrap_or_else(|e| e.into_inner());
    Snapshot {
        registrations: (0..pool.frames.len())
            .map(|i| state.directory.resolve(pool.frames, i))
            .collect(),
        successors: (0..pool.frames.len())
            .map(|i| state.directory.next_retention_generation(i))
            .collect(),
        contents,
        closed: state.directory.closed_pair(),
        claim: state.init_claim.map(|slot| match slot.identity {
            InitClaimIdentity::TxPair(identity) => identity,
            _ => panic!("wrong fixture claim"),
        }),
        counts: state.counts,
        total: state.total,
        entries: state.entries,
        requirements: state
            .requirements
            .iter()
            .map(|r| r.map(|r| r.location().lsn))
            .collect(),
        attempts: (state.last_attempt, state.last_load_attempt),
    }
}
fn rejected(result: Result<TxPairClaimKey<'_, '_>>, cause: Cause, effect: Effect) {
    match result {
        Err(error) => assert_eq!(error, DirtyError { cause, effect }),
        Ok(_) => panic!("invalid close admitted"),
    }
}

#[test]
fn pair_close_rejections_preserve_directory_generations_and_contents() {
    // Every rejected input uses a fresh fixture. No retry-until-success loop.
    for variant in 0..10 {
        println!("PAIR_ATOMIC_REJECTION variant={variant}");
        fixture(|pool, authority| {
            let mut indices = [7, 8];
            let mut existing = *authority.existing();
            let mut pair = *authority.pair();
            let expected = match variant {
                0 => {
                    indices = [7, 7];
                    Cause::Shape
                }
                1 => {
                    indices = [0, 8];
                    Cause::Shape
                }
                2 => {
                    indices = [6, 8];
                    Cause::DuplicateTarget
                }
                3 => {
                    existing[6] = existing[5];
                    Cause::DuplicateTarget
                }
                4 => {
                    existing[6].identity.page.incarnation = Generation::new(3).unwrap();
                    Cause::MissingTarget
                }
                5 => {
                    pool.directory_edit(|d| {
                        d.replace_retention(
                            8,
                            Registration {
                                request: request(4),
                                generation: u64::MAX,
                            },
                        )
                    })
                    .unwrap();
                    Cause::CounterExhausted
                }
                6 => {
                    let mut state = pool.registry.lock().unwrap();
                    state.counts[1] = 1;
                    state.total = 1;
                    Cause::Busy
                }
                7 => {
                    pair[1].identity.page = pair[0].identity.page;
                    Cause::Shape
                }
                8 => {
                    pool.directory_edit(|d| d.prepare_retention_slot(8, 2))
                        .unwrap();
                    Cause::MissingTarget
                }
                9 => {
                    super::super::super::flush::test_support::seed_uncaptured_claim(pool);
                    Cause::Busy
                }
                _ => unreachable!(),
            };
            let changed = TxPairAuthority::metadata_fixture(
                authority.core(),
                authority.binding(),
                authority.fence(),
                authority.actor(),
                authority.reserve(),
                &existing,
                &pair,
            );
            let before = snapshot(pool);
            rejected(
                pool.prepare_tx_pair(&changed, indices),
                expected,
                Effect::Unchanged,
            );
            assert_eq!(snapshot(pool), before, "variant {variant}");
            assert!(!pool.fence.is_fenced());
        });
    }
}

#[test]
fn pair_close_content_busy_and_corruption_never_consume_generations() {
    fixture(|pool, authority| {
        let before = snapshot(pool);
        let frames = pool.frames;
        let held = frames[8].content.write().unwrap();
        rejected(
            pool.prepare_tx_pair(authority, [7, 8]),
            Cause::Busy,
            Effect::Unchanged,
        );
        drop(held);
        assert_eq!(snapshot(pool), before);
        assert!(!pool.fence.is_fenced());
    });
    fixture(|pool, authority| {
        {
            let mut content = pool.frames[8].content.write().unwrap();
            content.dirty = true;
            content.page_lsn = PageLsn(2);
            content.modification_counter = 1;
        }
        let before = snapshot(pool);
        rejected(
            pool.prepare_tx_pair(authority, [7, 8]),
            Cause::Invariant,
            Effect::CallerMustResolve,
        );
        assert_eq!(snapshot(pool), before);
        assert!(pool.fence.is_fenced());
    });
}

#[test]
fn pair_close_publishes_exact_claim_and_unwind_keeps_it_fenced() {
    println!(
        "PAIR_ATOMIC_SOURCE parent_entry={} total={}",
        atomic_pair_source_bytes(),
        source_census()
    );
    for empty in [false, true] {
        fixture(|pool, authority| {
            if empty {
                pool.directory_edit(|d| {
                    d.retire_retention(7);
                    d.retire_retention(8);
                })
                .unwrap();
            }
            if !empty {
                for index in [7, 8] {
                    let mut content = pool.frames[index].content.write().unwrap();
                    content.page_lsn = PageLsn(41 + index as u64);
                    content.modification_counter = 3 + index as u64;
                }
            }
            let before = snapshot(pool);
            let key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
            let after = snapshot(pool);
            assert_eq!(after.closed, Some([7, 8]));
            assert_eq!(after.successors[7..], [Some(3), Some(3)]);
            assert_eq!(after.registrations[..7], before.registrations[..7]);
            assert_eq!(after.registrations[7..], [None, None]);
            assert_eq!(after.contents[..7], before.contents[..7]);
            for index in [7, 8] {
                assert_eq!(after.contents[index].0, before.contents[index].0);
                assert_eq!(after.contents[index].1, PageLsn(0));
                assert_eq!(after.contents[index].2, 0);
                assert_eq!(after.contents[index].3, None);
                assert!(!after.contents[index].4);
            }
            let claim = after.claim.unwrap();
            assert_eq!(claim.target_mask, 511);
            assert_eq!(claim.pair, *authority.pair());
            assert_eq!(claim.actor, authority.actor());
            assert_eq!(claim.reserve, authority.reserve());
            assert_eq!(claim.generations, [2, 2]);
            assert!(key.matches_pool(pool).unwrap());
            assert!(!pool.fence.is_fenced());
            rejected(
                pool.prepare_tx_pair(authority, [7, 8]),
                Cause::Busy,
                Effect::Unchanged,
            );
            assert_eq!(snapshot(pool), after, "active claim is retained");
            drop(key);
            assert!(pool.fence.is_fenced());
            assert_eq!(snapshot(pool), after, "Drop must not forge Cancel");
        });
    }
    for point in [
        test_support::InitPoint::PairClosedBeforeClaim,
        test_support::InitPoint::PairClaimInstalled,
    ] {
        fixture(|pool, authority| {
            let before = snapshot(pool);
            let (entered, arrived) = std::sync::mpsc::sync_channel(1);
            let (resume, release) = std::sync::mpsc::sync_channel(1);
            test_support::install_init_pause(pool, point, entered, release);
            std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    resume
                        .send(super::super::super::test_support::Resume::Panic)
                        .unwrap();
                });
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _key = pool.prepare_tx_pair(authority, [7, 8]).unwrap();
                    }))
                    .is_err()
                );
                controller.join().unwrap();
            });
            assert!(pool.fence.is_fenced());
            assert!(pool.registry.is_poisoned());
            let after = snapshot(pool);
            assert_eq!(after.closed, Some([7, 8]));
            assert_eq!(after.successors[7..], [Some(3), Some(3)]);
            assert_eq!(after.contents, before.contents);
            assert_eq!(
                after.claim.is_some(),
                point == test_support::InitPoint::PairClaimInstalled
            );
            assert_eq!(after.total, 0);
            rejected(
                pool.prepare_tx_pair(authority, [7, 8]),
                Cause::Fenced,
                Effect::CallerMustResolve,
            );
            assert_eq!(snapshot(pool), after);
        });
    }
}
