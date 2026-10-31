//! New G5 consumers only. Expected bytes are immutable independent plan4 raw.
use super::*;
use crate::{
    storage::{
        dirty_frame::{
            init_test_support as raw_probe, plan4_load_test_support as plan4_probe,
            pressure_test_support as pressure_probe, status_load_test_support as slot_probe,
        },
        page::{PageHeader, decode_page},
        retention_genesis::tests::current as raw,
        session_fence::SessionFence,
        wal_frame::MAX_FRAME_BYTES,
        wal_io::{FreshStorageSession, plan4_test_support as port},
    },
    transaction::concurrency::OwnerRole,
};
use foundation::{codec::content_digest, identity::Generation, namespace::DirectoryPort};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Barrier, Mutex, mpsc},
    time::Duration,
};

const DATA: &str = "d3-data-0000000100000001";
type State = Arc<Mutex<port::State>>;
type Snapshot = (
    raw_probe::RawSnapshot,
    Vec<u8>,
    Option<(TargetRequest, u64)>,
);

pub(super) fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn error<T>(result: Result<T>) -> Plan4PageError {
    result.err().expect("G5 boundary unexpectedly succeeded")
}
fn page(number: usize) -> Plan4Page {
    match number {
        1 => Plan4Page::Root,
        2..=6 => Plan4Page::Jobs {
            index: (number - 2) as u8,
        },
        7 => Plan4Page::CatalogAnchor,
        8 => Plan4Page::CatalogLeaf,
        _ => panic!("not a G5 payload address"),
    }
}
fn expected_request(number: usize) -> TargetRequest {
    let bytes = raw::raw(&format!("P{number:02}"));
    let identity = PageIdentity {
        database: raw::namespace().database,
        page: crate::storage::page::PageRef {
            file: crate::storage::page::FileId::new(0x1_0000_0001).unwrap(),
            page_no: number as u64,
            incarnation: Generation::new(1).unwrap(),
        },
        owner: ObjectId::new(if number < 7 { 4 } else { 5 }).unwrap(),
        owner_generation: Generation::new(1).unwrap(),
    };
    let kind = match number {
        1 => PageKind::RetentionRoot,
        2..=6 => PageKind::RetentionJobs,
        7 => PageKind::TreeAnchor,
        8 => PageKind::CatalogLeaf,
        _ => unreachable!(),
    };
    assert_eq!(
        decode_page(&bytes, identity, kind).unwrap().header(),
        PageHeader {
            identity,
            kind,
            level: 0,
            lsn: PageLsn(0)
        }
    );
    TargetRequest {
        identity,
        kind,
        level: 0,
    }
}
fn snapshots(pool: &SharedPool<'_, '_, '_, '_>, count: usize) -> Vec<Snapshot> {
    (0..count)
        .map(|index| {
            let mut bytes = boxed::<PAGE_BYTES>(0);
            let mut metadata = raw_probe::snapshot(pool, index, &mut bytes);
            metadata.published = None;
            (metadata, bytes.to_vec(), slot_probe::slot(pool, index))
        })
        .collect()
}
fn io_start(state: &State) -> (usize, usize, usize) {
    let state = state.lock().unwrap();
    (state.events.len(), state.mutations(), state.entropy_calls)
}
fn no_effect(state: &State, start: (usize, usize, usize)) {
    let observed = io_start(state);
    assert_eq!(observed.1, start.1);
    assert_eq!(observed.2, start.2);
}
fn no_io(state: &State, start: (usize, usize, usize)) {
    no_effect(state, start);
    let events = state.lock().unwrap().events.len();
    assert_eq!(events, start.0);
}
fn clear_trace(state: &State) {
    let pending = {
        let state = state.lock().unwrap();
        state.fault.is_some() || state.hook.is_some()
    };
    assert!(!pending);
    let mut state = state.lock().unwrap();
    state.events.clear();
    state.fault_hits = 0;
    state.hook_hits = 0;
}
fn clear_runtime_trace(state: &State) {
    // Every completed G5 segment must be clean before its event counters reset.
    let mutations = state.lock().unwrap().mutations();
    assert_eq!(mutations, 0);
    clear_trace(state);
}
fn install_fault(
    state: &State,
    op: &'static str,
    name: &str,
    offset: u64,
    occurrence: usize,
    action: port::Action,
) {
    let mut state = state.lock().unwrap();
    assert!(state.fault.is_none());
    state.fault = Some(port::Fault {
        key: port::Key::new(op, name, offset, occurrence),
        action,
    });
}
fn replace_page(state: &State, index: usize, bytes: &[u8]) {
    assert_eq!(bytes.len(), PAGE_BYTES);
    state.lock().unwrap().files.get_mut(DATA).unwrap().bytes
        [index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
        .copy_from_slice(bytes);
}
fn assert_clean(snapshot: &Snapshot, expected_page: usize) {
    assert_eq!(snapshot.0.page_lsn, PageLsn(0));
    assert_eq!(snapshot.0.counter, 0);
    assert_eq!(snapshot.0.rec_lsn, None);
    assert!(!snapshot.0.dirty);
    assert!(snapshot.0.entry.is_none() && snapshot.0.requirement.is_none());
    assert_eq!(snapshot.0.total, 0);
    assert!(!snapshot.0.claim_present && !snapshot.0.initializer_pins_held);
    assert_eq!(snapshot.1, raw::raw(&format!("P{expected_page:02}")));
    assert_eq!(snapshot.2.unwrap().0, expected_request(expected_page));
}
fn locate(pool: &SharedPool<'_, '_, '_, '_>, count: usize, number: usize) -> Option<(usize, u64)> {
    (1..count).find_map(|index| {
        slot_probe::slot(pool, index)
            .filter(|(request, _)| request.identity.page.page_no == number as u64)
            .map(|(_, generation)| (index, generation))
    })
}
fn assert_census(value: Plan4GenesisCensus) {
    assert_eq!((value.revision, value.free_jobs), (1, 64));
    let mut cells = Vec::with_capacity(64 * 1024);
    for number in 2..=6 {
        let bytes = raw::raw(&format!("P{number:02}"));
        let slots = if number == 6 { 4 } else { 15 };
        for slot in 0..slots {
            let at = 128 + slot * 4;
            let start = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
            let length = u16::from_le_bytes(bytes[at + 2..at + 4].try_into().unwrap()) as usize;
            assert_eq!(length, 1024);
            cells.extend_from_slice(&bytes[start..start + length]);
        }
    }
    assert_eq!(value.job_digest, content_digest(&cells));
    assert_eq!(
        value.job_digest.as_slice(),
        &raw::raw("P01")[15_840 + 128..15_840 + 160]
    );
}
pub(super) fn lifecycle<'s, 'c, W: WriterNamespace>(
    bound: &mut Plan4Pages<'s, 'c, '_, '_, '_, '_, '_, W>,
    core: &'c PageConcurrency<'s>,
    count: usize,
) {
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut cursor = Plan4ResidencyCursor::new();
    assert_census(
        bound
            .inspect_genesis_retention(&mut owner, &mut cursor)
            .unwrap(),
    );
    let after_first = snapshots(&bound.pool, count);
    assert_eq!(
        bound
            .lookup_catalog_empty(&mut owner, ObjectId::new(6).unwrap(), &mut cursor)
            .unwrap(),
        Plan4CatalogLookup::NotFound
    );
    assert_census(
        bound
            .inspect_genesis_retention(&mut owner, &mut cursor)
            .unwrap(),
    );
    let after = snapshots(&bound.pool, count);
    for value in &after {
        if let Some((request, _)) = value.2 {
            let number = request.identity.page.page_no as usize;
            if number != 0 {
                assert_clean(value, number);
            }
        } else {
            assert_eq!(value.0.total, 0);
            assert!(!value.0.dirty && value.0.entry.is_none() && value.0.requirement.is_none());
        }
    }
    if count == 4 {
        assert!(
            after
                .iter()
                .zip(&after_first)
                .any(|(a, b)| a.2.map(|v| v.1) > b.2.map(|v| v.1))
        );
    } else if count == 64 {
        assert_eq!(after.iter().filter(|value| value.2.is_some()).count(), 9);
        for (a, b) in after.iter().zip(&after_first) {
            if b.2.is_some() {
                assert_eq!(a.2, b.2);
            }
        }
    }
    let target = bound.target(Plan4Page::Root).unwrap();
    let before = locate(&bound.pool, count, 1).unwrap();
    let hit = bound.residency_step(&target, &mut cursor).unwrap();
    assert!(!hit.loaded);
    assert_eq!((hit.index, hit.frame.generation), before);
}

macro_rules! fresh {
    ($profile:expr, |$storage:ident, $core:ident, $state:ident| $body:block) => {{
        fresh!($profile, 0, |$storage, $core, $state| $body);
    }};
    ($profile:expr, $retained_handles:expr, |$storage:ident, $core:ident, $state:ident| $body:block) => {{
        let directory = port::Directory::fresh();
        let $state = directory.state();
        let mut slot = boxed::<MAX_FRAME_BYTES>(0);
        let mut maintenance = boxed::<12_582_912>(0);
        let mut flush = boxed::<FLUSH_BYTES>(0);
        let owner = FreshStorageSession::create_retention_catalog(directory.acquire_writer().unwrap(), $profile, &mut slot, &mut maintenance).unwrap();
        let installed = owner.install_doublewrite(&mut flush).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        // Constructor effects belong to setup; retain the entropy baseline.
        clear_trace(&$state);
        let before = io_start(&$state);
        $body
        no_effect(&$state, before);
        drop($storage);
        let remaining = {
            let state = $state.lock().unwrap();
            (state.writers, state.handles)
        };
        assert_eq!(remaining, (0, $retained_handles));
    }};
}
macro_rules! fixture {
    ($profile:expr, $count:expr, |$bound:ident, $core:ident, $state:ident, $storage:ident $(, $foreign:ident)?| $body:block) => {
        fresh!($profile, |$storage, $core, $state| {
            $(let $foreign = PageConcurrency::new($storage.session_fence()).unwrap();)?
            let count = $count;
            let mut backing = vec![[0xa7; PAGE_BYTES]; count];
            let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
            let mut operation = boxed::<OPERATION_BYTES>(0x73);
            let mut $bound = $storage.bind_plan4_pages(&$core, &mut frames, &mut operation).unwrap();
            $body
            let before_drop = io_start(&$state);
            drop($bound);
            assert!($storage.session_fence().is_fenced());
            assert_eq!($storage.binding.load(Ordering::Acquire), ABANDONED);
            no_io(&$state, before_drop);
        });
    };
}

#[test]
fn g5_clean_lifecycle() {
    let mut h = raw::Harness::new(raw::ids("G5-C", 1, 5));
    for case in 1..=5 {
        h.case(&format!("P4-G5-C{case:02}"), || {
            fixture!(
                1,
                if case == 2 { 64 } else { 4 },
                |bound, core, state, storage| {
                    match case {
                        1 | 2 => lifecycle(&mut bound, &core, if case == 2 { 64 } else { 4 }),
                        3 => {
                            let anchor = bound.target(Plan4Page::CatalogAnchor).unwrap();
                            bound.load_into(1, &anchor).unwrap();
                            let other = bound.target(Plan4Page::Jobs { index: 0 }).unwrap();
                            bound.load_into(2, &other).unwrap();
                            let before = locate(&bound.pool, 4, 7).unwrap();
                            let mut cursor = Plan4ResidencyCursor::new();
                            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                            assert_eq!(
                                bound
                                    .lookup_catalog_empty(
                                        &mut owner,
                                        ObjectId::new(6).unwrap(),
                                        &mut cursor
                                    )
                                    .unwrap(),
                                Plan4CatalogLookup::NotFound
                            );
                            assert_eq!(locate(&bound.pool, 4, 7), Some(before));
                        }
                        4 => required_positions(&mut bound, &core, [1, 2, 7], 1),
                        5 => {
                            let target = bound.target(Plan4Page::Root).unwrap();
                            bound.load_into(1, &target).unwrap();
                            let before = snapshots(&bound.pool, 4);
                            let start = io_start(&state);
                            let mut cursor = Plan4ResidencyCursor::new();
                            assert!(!bound.residency_step(&target, &mut cursor).unwrap().loaded);
                            let e = error(bound.load_into(1, &target));
                            assert_eq!(e.effect, Effect::Unchanged);
                            assert!(matches!(e.cause, Plan4PageCause::AlreadyResident));
                            assert_eq!(snapshots(&bound.pool, 4), before);
                            no_io(&state, start);
                        }
                        _ => unreachable!(),
                    }
                }
            );
        });
    }
    h.finish();
}
fn required_positions<'s, 'c, W: WriterNamespace>(
    bound: &mut Plan4Pages<'s, 'c, '_, '_, '_, '_, '_, W>,
    core: &'c PageConcurrency<'s>,
    order: [usize; 3],
    start: usize,
) {
    for (index, number) in order.into_iter().enumerate() {
        let target = bound.target(page(number)).unwrap();
        bound.load_into(index + 1, &target).unwrap();
    }
    let root = locate(&bound.pool, 4, 1).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut cursor = Plan4ResidencyCursor { next: start };
    assert_census(
        bound
            .inspect_genesis_retention(&mut owner, &mut cursor)
            .unwrap(),
    );
    assert_eq!(locate(&bound.pool, 4, 1), Some(root));
    assert_eq!(snapshots(&bound.pool, 4)[0].0.total, 0);
}

fn identity_mutation(number: usize, dimension: usize) -> Vec<u8> {
    let mut bytes = raw::raw(&format!("P{number:02}"));
    match dimension {
        1 => bytes[8] ^= 1,
        2 => raw::put64(&mut bytes, 24, 0x1_0000_0002),
        3 => raw::put64(&mut bytes, 32, number as u64 + 1),
        4 => raw::put64(&mut bytes, 40, 2),
        5 => raw::put64(&mut bytes, 48, if number < 7 { 5 } else { 4 }),
        6 => raw::put64(&mut bytes, 56, 2),
        7 => bytes[6] = if number == 1 { 13 } else { 12 },
        8 => bytes[7] = 1,
        9 => raw::put64(&mut bytes, 64, 1),
        _ => unreachable!(),
    }
    raw::resign(&mut bytes);
    assert_eq!(
        content_digest(&bytes[..PAGE_BYTES - 32]).as_slice(),
        &bytes[PAGE_BYTES - 32..]
    );
    bytes
}
fn rejected_home(number: usize, dimension: usize) {
    fixture!(1, 4, |bound, core, state, storage| {
        let target = bound.target(page(number)).unwrap();
        let old = bound.target(page(if number == 8 { 7 } else { 8 })).unwrap();
        bound.load_into(1, &old).unwrap();
        replace_page(&state, number, &identity_mutation(number, dimension));
        let before = snapshots(&bound.pool, 4);
        let e = error(bound.load_into(1, &target));
        assert_eq!(e.phase, Plan4PagePhase::Load);
        assert_eq!(e.effect, Effect::CallerMustResolve);
        use crate::storage::page::PageError;
        match (dimension, &e.cause) {
            (1..=6, Plan4PageCause::PageIo(PageIoError::Page(PageError::IdentityMismatch))) => {}
            (7, Plan4PageCause::PageIo(PageIoError::Page(PageError::KindMismatch))) => {}
            (8, Plan4PageCause::PageIo(PageIoError::Page(PageError::Corrupt))) => {}
            (
                9,
                Plan4PageCause::Retention(
                    crate::storage::retention_genesis::RetentionGenesisError::Corrupt,
                ),
            ) if number < 7 => {}
            (9, Plan4PageCause::Genesis(genesis::Plan4GenesisCause::IdentityMismatch))
                if number >= 7 => {}
            _ => panic!(
                "unexpected home boundary {number}/{dimension}: {:?}",
                e.cause
            ),
        }
        assert!(storage.session_fence().is_fenced());
        assert_eq!(snapshots(&bound.pool, 4), before);
    });
}

#[test]
fn g5_target_identity_boundaries() {
    let mut h = raw::Harness::new(raw::ids("G5-R", 1, 13));
    for case in 1..=13 {
        h.case(&format!("P4-G5-R{case:02}"), || {
            if case <= 9 {
                rejected_home(1, case);
                return;
            }
            fixture!(1, 4, |bound, core, state, storage| {
                let start = io_start(&state);
                if matches!(case, 10 | 11) {
                    let e = error(bound.target(Plan4Page::Jobs {
                        index: if case == 10 { 5 } else { 255 },
                    }));
                    assert_eq!(e.phase, Plan4PagePhase::Preflight);
                    assert_eq!(e.effect, Effect::Unchanged);
                    assert!(matches!(e.cause, Plan4PageCause::Shape));
                    no_io(&state, start);
                } else if case == 12 {
                    foreign_target(&mut bound, &state);
                } else {
                    let mut target = bound.target(Plan4Page::Root).unwrap();
                    bound.load_into(1, &target).unwrap();
                    let before = snapshots(&bound.pool, 4);
                    let start = io_start(&state);
                    target.request.kind = PageKind::RetentionJobs;
                    let mut cursor = Plan4ResidencyCursor::new();
                    let e = error(bound.residency_step(&target, &mut cursor));
                    assert_eq!(e.effect, Effect::Unchanged);
                    assert!(matches!(e.cause, Plan4PageCause::Membership));
                    assert_eq!(snapshots(&bound.pool, 4), before);
                    assert!(!storage.session_fence().is_fenced());
                    no_io(&state, start);
                }
            });
        });
    }
    h.finish();
}
fn foreign_target<W: WriterNamespace>(
    bound: &mut Plan4Pages<'_, '_, '_, '_, '_, '_, '_, W>,
    state: &State,
) {
    let directory = port::Directory::fresh();
    directory.0.lock().unwrap().entropy = [0x65; 16];
    let mut slot = boxed::<MAX_FRAME_BYTES>(0);
    let mut maintenance = boxed::<12_582_912>(0);
    let mut flush = boxed::<FLUSH_BYTES>(0);
    let fresh = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        1,
        &mut slot,
        &mut maintenance,
    )
    .unwrap();
    let storage = fresh
        .install_doublewrite(&mut flush)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut pages = vec![[0; PAGE_BYTES]; 4];
    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<OPERATION_BYTES>(0);
    let other = storage
        .bind_plan4_pages(&core, &mut frames, &mut operation)
        .unwrap();
    let target = other.target(Plan4Page::Root).unwrap();
    let before = snapshots(&bound.pool, 4);
    let start = io_start(state);
    let e = error(bound.load_into(1, &target));
    assert_eq!(e.effect, Effect::Unchanged);
    assert!(matches!(e.cause, Plan4PageCause::Membership));
    assert_eq!(snapshots(&bound.pool, 4), before);
    assert!(!bound.storage.session_fence().is_fenced());
    no_io(state, start);
}

fn canonical_mutation(case: usize) -> (usize, Vec<u8>) {
    let number = match case {
        1 | 2 => 0,
        3 => 1,
        4 | 7 => 6,
        5 => 7,
        6 => 8,
        8 => 63,
        _ => unreachable!(),
    };
    let mut bytes = if number == 63 {
        vec![0; PAGE_BYTES]
    } else {
        raw::raw(&format!("P{number:02}"))
    };
    match case {
        1 => raw::put64(&mut bytes, 10_208 + 63 * 96 + 8, 2),
        2 => raw::put64(&mut bytes, 10_208 + 96 + 16, 5),
        // Case 3 changes the Root cell's PageRef of Job page 4. The Root2 and
        // Root3 decoders check that reference identically and report
        // IdentityMismatch; the Corrupt expected for it below is the Profile1
        // v1 byte-compare value. That match already failed before the
        // current-format switch (pre-existing, left to its owner).
        3 => raw::put64(&mut bytes, 15_840 + 256 + 4 * 24 + 8, 5),
        4 => {
            raw::put16(&mut bytes, 72, 3);
            raw::put16(&mut bytes, 74, 140);
            raw::put16(&mut bytes, 76, 13_280);
            bytes[140..13_280].fill(0);
        }
        5 => raw::put64(&mut bytes, 16_304 + 16, 9),
        6 => bytes[16_320 + 6] = 1,
        7 => raw::put64(&mut bytes, 16_352 - 4 * 1024 + 8, 2),
        8 => bytes[PAGE_BYTES - 1] = 1,
        _ => unreachable!(),
    }
    if case != 8 {
        raw::resign(&mut bytes);
    }
    (number, bytes)
}
#[test]
fn g5_canonical_source_boundaries() {
    let mut h = raw::Harness::new(raw::ids("G5-M", 1, 8));
    for case in 1..=8 {
        h.case(&format!("P4-G5-M{case:02}"), || {
            let (number, bytes) = canonical_mutation(case);
            if matches!(case, 1 | 2 | 8) {
                fresh!(1, |storage, core, state| {
                    replace_page(&state, number, &bytes);
                    let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
                    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
                    let mut operation = boxed::<OPERATION_BYTES>(0x73);
                    let e = error(storage.bind_plan4_pages(&core, &mut frames, &mut operation));
                    assert_eq!(
                        (e.phase, e.effect),
                        (Plan4PagePhase::Bind, Effect::CallerMustResolve)
                    );
                    assert!(matches!(
                        e.cause,
                        Plan4PageCause::Genesis(genesis::Plan4GenesisCause::IdentityMismatch)
                    ));
                    assert!(storage.session_fence().is_fenced());
                    assert_eq!(storage.binding.load(Ordering::Acquire), ABANDONED);
                });
            } else {
                fixture!(1, 4, |bound, core, state, storage| {
                    let target = bound.target(page(number)).unwrap();
                    let old = bound.target(page(if number == 8 { 7 } else { 8 })).unwrap();
                    bound.load_into(1, &old).unwrap();
                    replace_page(&state, number, &bytes);
                    let before = snapshots(&bound.pool, 4);
                    let e = error(bound.load_into(1, &target));
                    assert_eq!(
                        (e.phase, e.effect),
                        (Plan4PagePhase::Load, Effect::CallerMustResolve)
                    );
                    match (case, &e.cause) {
                        (
                            3 | 4 | 7,
                            Plan4PageCause::Retention(
                                crate::storage::retention_genesis::RetentionGenesisError::Corrupt,
                            ),
                        ) => {}
                        (
                            5,
                            Plan4PageCause::Genesis(genesis::Plan4GenesisCause::IdentityMismatch),
                        ) => {}
                        (
                            6,
                            Plan4PageCause::Tree(crate::storage::tree_page::TreeError::Corrupt),
                        ) => {}
                        _ => panic!("unexpected canonical boundary {case}: {:?}", e.cause),
                    }
                    assert!(storage.session_fence().is_fenced());
                    assert_eq!(snapshots(&bound.pool, 4), before);
                });
            }
        });
    }
    h.finish();
}

fn assert_frame_cause(error: Plan4PageError, expected: dirty_frame::Cause, effect: Effect) {
    assert_eq!(error.effect, effect);
    match error.cause {
        Plan4PageCause::Frame(inner) => {
            assert_eq!(inner.cause, expected);
            assert_eq!(inner.effect, effect);
        }
        other => panic!("expected exact Frame({expected:?}), got {other:?}"),
    }
}
#[test]
fn g5_clean_invariant_preflight() {
    let mut h = raw::Harness::new(raw::ids("G5-J", 1, 11));
    for case in 1..=11 {
        h.case(&format!("P4-G5-J{case:02}"), || {
            let count = if case == 10 { 5 } else { 4 };
            fixture!(1, count, |bound, core, state, storage, foreign| {
                if case == 1 {
                    let old = bound.target(Plan4Page::CatalogLeaf).unwrap();
                    bound.load_into(1, &old).unwrap();
                    let previous = slot_probe::slot(&bound.pool, 1).unwrap().1;
                    let target = bound.target(Plan4Page::Root).unwrap();
                    let result = bound.load_into(1, &target).unwrap();
                    assert_eq!(result.before.unwrap().generation, previous);
                    assert_eq!(result.after.generation, previous + 1);
                    assert_clean(&snapshots(&bound.pool, count)[1], 1);
                } else if matches!(case, 2..=4) {
                    for (index, number) in [(1, if case == 2 { 1 } else { 2 }), (2, 7), (3, 8)] {
                        let target = bound.target(page(number)).unwrap();
                        bound.load_into(index, &target).unwrap();
                    }
                    if case == 4 { plan4_probe::seed_generation(&mut bound.pool, 3, u64::MAX); }
                    plan4_probe::seed_dirty(&bound.pool, 3);
                    let before = snapshots(&bound.pool, count);
                    let target = bound.target(Plan4Page::Root).unwrap();
                    let mut cursor = Plan4ResidencyCursor::new();
                    let start = io_start(&state);
                    let e = error(bound.residency_step(&target, &mut cursor));
                    assert_frame_cause(e, dirty_frame::Cause::Invariant, Effect::CallerMustResolve);
                    assert!(storage.session_fence().is_fenced());
                    assert_eq!(cursor.next, 1);
                    assert_eq!(snapshots(&bound.pool, count), before);
                    no_io(&state, start);
                } else if matches!(case, 5..=8) {
                    for (index, number) in [(1, 2), (2, 3), (3, 4)] {
                        let target = bound.target(page(number)).unwrap();
                        bound.load_into(index, &target).unwrap();
                    }
                    let mut owner = if matches!(case, 5 | 7) { foreign.try_owner(OwnerRole::Foreground).unwrap() } else { core.try_owner(OwnerRole::Foreground).unwrap() };
                    if matches!(case, 6 | 8) { std::mem::forget(owner.try_apply().unwrap()); }
                    let before = snapshots(&bound.pool, count);
                    let mut cursor = Plan4ResidencyCursor::new();
                    let start = io_start(&state);
                    let e = if matches!(case, 5 | 8) {
                        error(bound.lookup_catalog_empty(&mut owner, ObjectId::new(6).unwrap(), &mut cursor))
                    } else {
                        error(bound.inspect_genesis_retention(&mut owner, &mut cursor))
                    };
                    if matches!(case, 5 | 7) {
                        assert_frame_cause(e, dirty_frame::Cause::BindingMismatch, Effect::Unchanged);
                        assert!(!storage.session_fence().is_fenced());
                        owner.check_idle().unwrap();
                    } else {
                        assert_eq!(e.effect, Effect::CallerMustResolve);
                        assert!(storage.session_fence().is_fenced());
                        assert!(matches!(e.cause, Plan4PageCause::Frame(inner) if inner.cause == dirty_frame::Cause::Invariant));
                    }
                    assert_eq!(snapshots(&bound.pool, count), before);
                    assert_eq!(cursor.next, 1);
                    no_io(&state, start);
                } else {
                    for (index, number) in [(1, 1), (2, 2), (3, 7)] {
                        let target = bound.target(page(number)).unwrap();
                        bound.load_into(index, &target).unwrap();
                    }
                    if case == 10 {
                        let target = bound.target(Plan4Page::CatalogLeaf).unwrap();
                        bound.load_into(4, &target).unwrap();
                    }
                    plan4_probe::seed_generation(&mut bound.pool, if case == 9 { 1 } else { 3 }, u64::MAX);
                    let protected = if case == 9 { 1 } else { 7 };
                    let registration = locate(&bound.pool, count, protected).unwrap();
                    let before = snapshots(&bound.pool, count);
                    let start = io_start(&state);
                    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let mut cursor = Plan4ResidencyCursor::new();
                    let result = bound.inspect_genesis_retention(&mut owner, &mut cursor);
                    if case == 11 {
                        assert_frame_cause(error(result), dirty_frame::Cause::CounterExhausted, Effect::Unchanged);
                        assert_eq!(snapshots(&bound.pool, count), before);
                        assert_eq!(cursor.next, 1);
                        no_io(&state, start);
                    } else {
                        assert_census(result.unwrap());
                        assert_eq!(locate(&bound.pool, count, protected), Some(registration));
                        if case == 10 { assert_eq!(locate(&bound.pool, count, 1), Some((1, before[1].2.unwrap().1))); }
                    }
                }
            });
        });
    }
    h.finish();
}

#[test]
fn g5_residency_state_boundaries() {
    let mut h = raw::Harness::new(raw::ids("G5-Q", 1, 8));
    for case in 1..=8 {
        h.case(&format!("P4-G5-Q{case:02}"), || {
            fixture!(1, 4, |bound, core, state, storage| {
                let target = bound.target(Plan4Page::Root).unwrap();
                if case != 8 {
                    let old = bound.target(Plan4Page::CatalogAnchor).unwrap();
                    bound.load_into(1, &old).unwrap();
                }
                let before = snapshots(&bound.pool, 4);
                clear_runtime_trace(&state);
                let start = io_start(&state);
                match case {
                    1 | 2 => {
                        assert_frame_cause(
                            error(bound.load_into(if case == 1 { 0 } else { 4 }, &target)),
                            dirty_frame::Cause::Shape,
                            Effect::Unchanged,
                        );
                        assert_eq!(snapshots(&bound.pool, 4), before);
                        no_io(&state, start);
                    }
                    3 => {
                        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                        let request = bound.target(Plan4Page::CatalogAnchor).unwrap().request;
                        let pins = bound
                            .pool
                            .reserve_target_set::<1>(&mut owner, &[request])
                            .unwrap();
                        std::mem::forget(pins);
                        let pinned = snapshots(&bound.pool, 4);
                        assert_eq!(pinned[0].0.total, 1);
                        let mut cursor = Plan4ResidencyCursor::new();
                        assert_frame_cause(
                            error(bound.residency_step(&target, &mut cursor)),
                            dirty_frame::Cause::Busy,
                            Effect::Unchanged,
                        );
                        assert_eq!(snapshots(&bound.pool, 4), pinned);
                        no_io(&state, start);
                    }
                    4 => {
                        pressure_probe::forget_content_write(&bound.pool, 3);
                        let mut cursor = Plan4ResidencyCursor::new();
                        assert_frame_cause(
                            error(bound.residency_step(&target, &mut cursor)),
                            dirty_frame::Cause::Busy,
                            Effect::Unchanged,
                        );
                        assert_eq!(cursor.next, 1);
                        for (index, previous) in before.iter().enumerate() {
                            assert_eq!(slot_probe::slot(&bound.pool, index), previous.2);
                        }
                        // The deliberately forgotten write guard prevents reading slot3.
                        // No source I/O/publication occurred; retained lock is not reset.
                        no_io(&state, start);
                    }
                    5 => {
                        plan4_probe::seed_generation(&mut bound.pool, 1, u64::MAX);
                        let seeded = snapshots(&bound.pool, 4);
                        assert_frame_cause(
                            error(bound.load_into(1, &target)),
                            dirty_frame::Cause::CounterExhausted,
                            Effect::Unchanged,
                        );
                        assert_eq!(snapshots(&bound.pool, 4), seeded);
                        no_io(&state, start);
                    }
                    6 => {
                        install_fault(
                            &state,
                            "read",
                            DATA,
                            PAGE_BYTES as u64,
                            1,
                            port::Action::BeforePanic,
                        );
                        assert!(
                            catch_unwind(AssertUnwindSafe(|| bound.load_into(1, &target))).is_err()
                        );
                        assert!(storage.session_fence().is_fenced());
                        assert_eq!(state.lock().unwrap().fault_hits, 1);
                        assert_eq!(snapshots(&bound.pool, 4), before);
                    }
                    7 => {
                        install_fault(
                            &state,
                            "read",
                            DATA,
                            PAGE_BYTES as u64,
                            1,
                            port::Action::BeforeError,
                        );
                        let mut cursor = Plan4ResidencyCursor { next: 2 };
                        let e = error(bound.residency_step(&target, &mut cursor));
                        assert_eq!(e.effect, Effect::Unchanged);
                        assert!(!storage.session_fence().is_fenced());
                        assert_eq!(cursor.next, 2);
                        assert_eq!(snapshots(&bound.pool, 4), before);
                        assert_eq!(state.lock().unwrap().fault_hits, 1);
                        let loaded = bound.residency_step(&target, &mut cursor).unwrap();
                        assert!(loaded.loaded);
                        assert_eq!(loaded.index, 2);
                        assert_eq!(cursor.next, 3);
                        assert_clean(&snapshots(&bound.pool, 4)[2], 1);
                    }
                    8 => {
                        install_fault(
                            &state,
                            "read",
                            DATA,
                            (4 * PAGE_BYTES) as u64,
                            1,
                            port::Action::BeforeError,
                        );
                        let mut cursor = Plan4ResidencyCursor::new();
                        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                        let e = error(bound.inspect_genesis_retention(&mut owner, &mut cursor));
                        assert_eq!(e.effect, Effect::Unchanged);
                        assert!(!storage.session_fence().is_fenced());
                        assert_eq!(state.lock().unwrap().fault_hits, 1);
                        assert_eq!(locate(&bound.pool, 4, 1), Some((1, 1)));
                        let after = snapshots(&bound.pool, 4);
                        assert_clean(&after[1], 1);
                        assert!(after[2].2.is_some() && after[3].2.is_some());
                        assert_eq!(after[0].0.total, 0);
                        assert_ne!(after, before);
                    }
                    _ => unreachable!(),
                }
            });
        });
    }
    h.finish();
}

fn assert_source<W: WriterNamespace>(label: &str) {
    let c = source_census::<W>();
    let core = dirty_frame::plan4_core_census();
    println!(
        "P4_CENSUS\t{label}-original-core\tload={}\tcandidate={}\tpool={}\tdirectory={}\tmode={}\told_common_delta={}",
        core.load_nested,
        core.candidate_nested,
        core.pool,
        core.directory,
        core.mode,
        c.old_common_delta
    );
    println!(
        "P4_CENSUS\t{label}\twriter={}\tfile={}\towner={}\tbound={}\tretained={}\tbind={}\tload={}\tcatalog={}\tqueue={}\tset3={}\tset4={}\tpeak={}\twhole={}\told_common_delta={}\tcaller_operation={}\tcaller_flush={}\ttemporary_files={}",
        std::mem::size_of::<W>(),
        std::mem::size_of::<W::File>(),
        c.owner,
        c.bound,
        c.retained,
        c.bind_nested,
        c.load_nested,
        c.catalog_nested,
        c.queue_nested,
        c.set3_nested,
        c.set4_nested,
        c.peak,
        c.whole,
        c.old_common_delta,
        c.caller_operation,
        c.caller_flush,
        c.temporary_files
    );
    assert!(resources::fits::<W>());
    assert!(c.owner <= 32_768 && std::mem::size_of::<W::File>() <= 16_384);
    assert_eq!(c.original_core, std::mem::size_of::<PageConcurrency<'_>>());
    assert_eq!(c.retained, c.owner + c.bound + c.original_core);
    for nested in [
        c.bind_nested,
        c.load_nested,
        c.catalog_nested,
        c.queue_nested,
    ] {
        assert!(c.owner + c.original_core + nested <= 65_536);
    }
    assert!(c.peak <= 65_536 && c.whole <= 262_144);
    assert_eq!(c.old_common_growth, 0);
    assert_eq!(
        (c.caller_operation, c.caller_flush, c.temporary_files),
        (114_688, 4_194_304, 2)
    );
}
#[test]
fn g5_source_memory() {
    let mut h = raw::Harness::new(vec!["P4-G5-ST01".into()]);
    h.case("P4-G5-ST01", || {
        assert_source::<port::Writer>("actual-memory");
        padding_census::<256>();
        padding_census::<512>();
        padding_census::<1024>();
        padding_census::<2048>();
        padding_census::<4096>();
    });
    h.finish();
}
#[cfg(target_os = "linux")]
#[test]
fn g5_source_native() {
    let mut h = raw::Harness::new(vec!["P4-G5-ST02".into()]);
    h.case("P4-G5-ST02", || {
        assert_source::<super::native::Writer>("actual-native")
    });
    h.finish();
}

fn legacy_bind(plan: usize) {
    let directory = port::Directory::fresh();
    let state = directory.state();
    let mut slot = boxed::<MAX_FRAME_BYTES>(0);
    let mut scratch = vec![[0; PAGE_BYTES]; 3];
    let scratch: &mut [[u8; PAGE_BYTES]; 3] = scratch.as_mut_slice().try_into().unwrap();
    let writer = directory.acquire_writer().unwrap();
    let fresh = match plan {
        1 => FreshStorageSession::create(writer, 1, &mut slot, scratch).unwrap(),
        2 => FreshStorageSession::create_status(writer, 1, &mut slot, scratch).unwrap(),
        3 => FreshStorageSession::create_undo(writer, 1, &mut slot, scratch).unwrap(),
        _ => unreachable!(),
    };
    let mut flush = boxed::<FLUSH_BYTES>(0);
    let storage = fresh
        .install_doublewrite(&mut flush)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut pages = vec![[0xa7; PAGE_BYTES]; 4];
    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<OPERATION_BYTES>(0x73);
    let before = io_start(&state);
    let e = error(storage.bind_plan4_pages(&core, &mut frames, &mut operation));
    assert_eq!(
        (e.phase, e.effect),
        (Plan4PagePhase::Preflight, Effect::Unchanged)
    );
    assert!(matches!(e.cause, Plan4PageCause::UnsupportedPlan));
    assert!(!storage.session_fence().is_fenced());
    assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
    no_io(&state, before);
    drop(storage);
    let state = state.lock().unwrap();
    assert_eq!((state.writers, state.handles), (0, 0));
}

#[test]
fn g5_bind_boundaries() {
    let mut h = raw::Harness::new(raw::ids("G5-B", 1, 16));
    for case in 1..=16 {
        h.case(&format!("P4-G5-B{case:02}"), || {
            if case <= 3 { legacy_bind(case); return; }
            if case == 8 { capacity_bind(); return; }
            if case == 11 { bind_race(); return; }
            fresh!(1, usize::from(case == 9), |storage, core, state| {
                let count = match case { 5 | 12 => 3, 6 => 65, _ => 4 };
                let mut pages = vec![[0xa7; PAGE_BYTES]; count];
                let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
                let mut operation = boxed::<OPERATION_BYTES>(0x73);
                let foreign_fence = SessionFence::new();
                let foreign_core = PageConcurrency::new(&foreign_fence).unwrap();
                let selected_core = if case == 4 { &foreign_core } else { &core };
                if case == 7 { frames[1].register(expected_request(1)).unwrap(); }
                let mut reader_root = boxed::<PAGE_BYTES>(0);
                let mut reader_scratch = boxed::<PAGE_BYTES>(0);
                if case == 9 { std::mem::forget(storage.core.open_reader(&mut reader_root, &mut reader_scratch).unwrap()); }
                if case == 12 { storage.session_fence().fence(); }
                if case == 13 {
                    assert!(catch_unwind(AssertUnwindSafe(|| {
                        let _held = storage.workspace.lock().unwrap();
                        panic!("G5 actual workspace poison");
                    })).is_err());
                }
                clear_runtime_trace(&state);
                if matches!(case, 14 | 15) {
                    install_fault(&state, "read", "d3-profile", 0, 1, if case == 14 { port::Action::BeforeError } else { port::Action::BeforePanic });
                }
                let start = io_start(&state);
                if case == 15 {
                    assert!(catch_unwind(AssertUnwindSafe(|| { let _ = storage.bind_plan4_pages(selected_core, &mut frames, &mut operation); })).is_err());
                    assert!(storage.session_fence().is_fenced());
                    assert_eq!(storage.binding.load(Ordering::Acquire), ABANDONED);
                    assert_eq!(state.lock().unwrap().fault_hits, 1);
                } else if matches!(case, 10 | 16) {
                    let bound = storage.bind_plan4_pages(selected_core, &mut frames, &mut operation).unwrap();
                    let mut pages2 = vec![[0xb7; PAGE_BYTES]; 4];
                    let mut frames2: Vec<_> = pages2.iter_mut().map(DirtyFrame::new).collect();
                    let mut operation2 = boxed::<OPERATION_BYTES>(0x87);
                    if case == 16 { drop(bound); }
                    let start = io_start(&state);
                    let e = error(storage.bind_plan4_pages(&core, &mut frames2, &mut operation2));
                    if case == 10 {
                        assert_eq!(e.effect, Effect::Unchanged);
                        assert!(matches!(e.cause, Plan4PageCause::AlreadyBound));
                        assert_eq!(storage.binding.load(Ordering::Acquire), BOUND);
                        assert!(!storage.session_fence().is_fenced());
                    } else {
                        assert_eq!(e.effect, Effect::CallerMustResolve);
                        assert!(matches!(e.cause, Plan4PageCause::Storage(inner) if matches!(inner.cause, super::super::Cause::Fenced)));
                        assert_eq!(storage.binding.load(Ordering::Acquire), ABANDONED);
                    }
                    no_io(&state, start);
                } else {
                    let e = error(storage.bind_plan4_pages(selected_core, &mut frames, &mut operation));
                    match case {
                        4 => { assert_eq!(e.effect, Effect::Unchanged); assert!(matches!(e.cause, Plan4PageCause::BindingMismatch)); }
                        5 | 6 => { assert_eq!(e.effect, Effect::Unchanged); assert!(matches!(e.cause, Plan4PageCause::Shape)); }
                        7 => { assert_eq!(e.effect, Effect::CallerMustResolve); assert!(matches!(e.cause, Plan4PageCause::Frame(inner) if inner.cause == dirty_frame::Cause::Invariant)); }
                        9 => { assert_eq!(e.effect, Effect::CallerMustResolve); assert!(matches!(e.cause, Plan4PageCause::Invariant)); }
                        12 => { assert_eq!(e.effect, Effect::CallerMustResolve); assert!(matches!(e.cause, Plan4PageCause::Storage(inner) if matches!(inner.cause, super::super::Cause::Fenced))); }
                        13 => { assert_eq!(e.effect, Effect::CallerMustResolve); assert!(matches!(e.cause, Plan4PageCause::Storage(inner) if matches!(inner.cause, super::super::Cause::Poisoned))); }
                        14 => { assert_eq!(e.effect, Effect::CallerMustResolve); assert!(matches!(e.cause, Plan4PageCause::Genesis(_))); assert_eq!(state.lock().unwrap().fault_hits, 1); }
                        _ => unreachable!(),
                    }
                    if matches!(case, 4..=7 | 12 | 13) {
                        assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
                        no_io(&state, start);
                    } else { assert_eq!(storage.binding.load(Ordering::Acquire), ABANDONED); }
                }
            });
        });
    }
    h.finish();
}
fn bind_race() {
    fresh!(1, |storage, core, state| {
        clear_runtime_trace(&state);
        let start = Arc::new(Barrier::new(3));
        let (release_tx, release_rx) = mpsc::channel();
        let release = Arc::new(Mutex::new(release_rx));
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let start = Arc::clone(&start);
                let release = Arc::clone(&release);
                let tx = tx.clone();
                let storage = &storage;
                let core = &core;
                scope.spawn(move || {
                    let mut pages = vec![[0; PAGE_BYTES]; 4];
                    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
                    let mut operation = boxed::<OPERATION_BYTES>(0);
                    start.wait();
                    match storage.bind_plan4_pages(core, &mut frames, &mut operation) {
                        Ok(bound) => {
                            tx.send(true).unwrap();
                            release
                                .lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(5))
                                .unwrap();
                            drop(bound);
                        }
                        Err(e) => {
                            assert_eq!(e.effect, Effect::Unchanged);
                            assert!(matches!(e.cause, Plan4PageCause::AlreadyBound));
                            tx.send(false).unwrap();
                        }
                    }
                });
            }
            start.wait();
            let first = rx.recv_timeout(Duration::from_secs(5));
            let second = rx.recv_timeout(Duration::from_secs(5));
            // Wake the successful bound before asserting so a failed assertion
            // does not leave the scoped winner parked forever.
            let _ = release_tx.send(());
            assert_ne!(first.unwrap(), second.unwrap());
        });
        assert_eq!(storage.binding.load(Ordering::Acquire), ABANDONED);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(
            state
                .lock()
                .unwrap()
                .events
                .iter()
                .filter(|event| event.op == "read" && event.name == DATA)
                .count(),
            130
        );
    });
}

// Actual forwarding types. These bytes are retained by every actual File, not
// a caller-supplied pretend size. ST01 chooses/finalizes B08 before effects.
struct SizedFileWriter<const N: usize> {
    inner: port::Writer,
}
struct SizedFile<const N: usize> {
    inner: port::File,
    _padding: [u8; N],
}
impl<const N: usize> SizedFile<N> {
    fn wrap(inner: port::File) -> Self {
        Self {
            inner,
            _padding: [0; N],
        }
    }
}
impl<const N: usize> foundation::io::FileIo for SizedFile<N> {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> std::io::Result<usize> {
        self.inner.read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> std::io::Result<usize> {
        self.inner.write_at(bytes, offset)
    }
    fn sync_all(&mut self) -> std::io::Result<()> {
        self.inner.sync_all()
    }
    fn set_len(&mut self, length: u64) -> std::io::Result<()> {
        self.inner.set_len(length)
    }
}
impl<const N: usize> WriterNamespace for SizedFileWriter<N> {
    type File = SizedFile<N>;
    fn random_incarnation(&mut self) -> foundation::namespace::NamespaceResult<[u8; 16]> {
        self.inner.random_incarnation()
    }
    fn create_new(
        &mut self,
        name: &foundation::namespace::FileName,
    ) -> foundation::namespace::NamespaceResult<Self::File> {
        self.inner.create_new(name).map(SizedFile::wrap)
    }
    fn open_existing(
        &mut self,
        name: &foundation::namespace::FileName,
        mode: foundation::namespace::OpenMode,
    ) -> foundation::namespace::NamespaceResult<Self::File> {
        self.inner.open_existing(name, mode).map(SizedFile::wrap)
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &foundation::namespace::FileName,
        mode: foundation::namespace::OpenMode,
    ) -> foundation::namespace::NamespaceResult<(Self::File, foundation::namespace::FileMetadata)>
    {
        self.inner
            .open_existing_with_metadata(name, mode)
            .map(|(file, metadata)| (SizedFile::wrap(file), metadata))
    }
    fn metadata(
        &self,
        name: &foundation::namespace::FileName,
    ) -> foundation::namespace::NamespaceResult<Option<foundation::namespace::FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(
        &self,
        limit: usize,
    ) -> foundation::namespace::NamespaceResult<Vec<foundation::namespace::DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> foundation::namespace::NamespaceResult<()> {
        self.inner.sync_directory()
    }
    fn publish_selector(
        &mut self,
        name: &foundation::namespace::FileName,
        selector: foundation::namespace::Selector,
    ) -> foundation::namespace::NamespaceResult<()> {
        self.inner.publish_selector(name, selector)
    }
    fn remove_retired(
        &mut self,
        name: &foundation::namespace::FileName,
    ) -> foundation::namespace::NamespaceResult<()> {
        self.inner.remove_retired(name)
    }
}
fn capacity_type<const N: usize>() -> bool {
    capacity_setup_fits::<N>() && !resources::fits::<SizedFileWriter<N>>()
}
fn capacity_setup_fits<const N: usize>() -> bool {
    super::super::super::super::source_budget_fits::<SizedFileWriter<N>>()
        && genesis::fits::<SizedFileWriter<N>>()
        && std::mem::size_of::<
            super::super::super::DoublewriteStorageSession<'_, '_, SizedFileWriter<N>>,
        >() <= 16_384
        && std::mem::size_of::<super::super::super::InstallPlan>() <= 4_096
}
fn padding_census<const N: usize>() {
    let fresh = super::super::super::super::source_budget_fits::<SizedFileWriter<N>>()
        && genesis::fits::<SizedFileWriter<N>>();
    let doublewrite_owner = std::mem::size_of::<
        super::super::super::DoublewriteStorageSession<'_, '_, SizedFileWriter<N>>,
    >();
    let doublewrite_plan = std::mem::size_of::<super::super::super::InstallPlan>();
    let c = source_census::<SizedFileWriter<N>>();
    println!(
        "P4_CENSUS\tB08-candidate\tpadding={N}\tfile={}\tfresh_fits={fresh}\tdoublewrite_owner={doublewrite_owner}\tdoublewrite_plan={doublewrite_plan}\tsetup_fits={}\tg5_fits={}\tg5_peak={}",
        std::mem::size_of::<SizedFile<N>>(),
        capacity_setup_fits::<N>(),
        resources::fits::<SizedFileWriter<N>>(),
        c.peak
    );
}
fn capacity_bind() {
    let requested: usize = std::env::var("XADB_PLAN4_G5_CAPACITY_PADDING")
        .expect("B08 requires source-bound static gate input")
        .parse()
        .unwrap();
    let first = [
        (256, capacity_type::<256>()),
        (512, capacity_type::<512>()),
        (1024, capacity_type::<1024>()),
        (2048, capacity_type::<2048>()),
        (4096, capacity_type::<4096>()),
    ]
    .into_iter()
    .find_map(|(n, fits)| fits.then_some(n))
    .expect("no legitimate B08 type; input review required before effects");
    assert_eq!(
        requested, first,
        "B08 must use the first accepted finite candidate"
    );
    match requested {
        256 => capacity_bind_type::<256>(),
        512 => capacity_bind_type::<512>(),
        1024 => capacity_bind_type::<1024>(),
        2048 => capacity_bind_type::<2048>(),
        4096 => capacity_bind_type::<4096>(),
        _ => unreachable!(),
    }
}
fn capacity_bind_type<const N: usize>() {
    assert!(capacity_type::<N>());
    let directory = port::Directory::fresh();
    let state = directory.state();
    let writer = SizedFileWriter::<N> {
        inner: directory.acquire_writer().unwrap(),
    };
    let mut slot = boxed::<MAX_FRAME_BYTES>(0);
    let mut maintenance = boxed::<12_582_912>(0);
    let mut flush = boxed::<FLUSH_BYTES>(0);
    let fresh =
        FreshStorageSession::create_retention_catalog(writer, 1, &mut slot, &mut maintenance)
            .unwrap();
    let storage = fresh
        .install_doublewrite(&mut flush)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut pages = vec![[0xa7; PAGE_BYTES]; 4];
    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<OPERATION_BYTES>(0x73);
    clear_trace(&state);
    let before = io_start(&state);
    let e = error(storage.bind_plan4_pages(&core, &mut frames, &mut operation));
    assert_eq!(
        (e.phase, e.effect),
        (Plan4PagePhase::Preflight, Effect::Unchanged)
    );
    assert!(matches!(e.cause, Plan4PageCause::Capacity));
    assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
    assert!(!storage.session_fence().is_fenced());
    no_io(&state, before);
    drop(storage);
    let state = state.lock().unwrap();
    assert_eq!((state.writers, state.handles), (0, 0));
}

#[test]
fn g5_load_io_boundaries() {
    let mut h = raw::Harness::new(raw::ids("G5-I", 1, 14));
    for case in 1..=14 {
        h.case(&format!("P4-G5-I{case:02}"), || {
            fixture!(1, 4, |bound, core, state, storage| {
                let old = bound.target(Plan4Page::CatalogAnchor).unwrap();
                bound.load_into(1, &old).unwrap();
                let target = bound.target(Plan4Page::Root).unwrap();
                let before = snapshots(&bound.pool, 4);
                clear_runtime_trace(&state);
                match case {
                    1..=6 => install_fault(
                        &state,
                        "read",
                        DATA,
                        PAGE_BYTES as u64,
                        1,
                        match case {
                            1 => port::Action::Short(17),
                            2 => port::Action::Interrupted,
                            3 => port::Action::BeforeError,
                            4 => port::Action::PrefixError,
                            5 => port::Action::Zero,
                            6 => port::Action::InvalidCount,
                            _ => unreachable!(),
                        },
                    ),
                    7 => install_fault(&state, "open", DATA, 0, 1, port::Action::OpenedIdentity),
                    8 | 9 => {
                        state.lock().unwrap().hook = Some(port::Hook {
                            key: port::Key::new("metadata", DATA, 0, 3),
                            edit: if case == 8 {
                                port::Edit::Identity(DATA.into())
                            } else {
                                port::Edit::Length(DATA.into(), 1)
                            },
                        });
                    }
                    10 => {
                        let mut bytes = state.lock().unwrap().files[DATA].bytes.clone();
                        let mut root = raw::raw("P01");
                        raw::put64(&mut root, 15_840 + 64, 2);
                        raw::resign(&mut root);
                        bytes[PAGE_BYTES..2 * PAGE_BYTES].copy_from_slice(&root);
                        state.lock().unwrap().hook = Some(port::Hook {
                            key: port::Key::new("read", DATA, PAGE_BYTES as u64, 2),
                            edit: port::Edit::Bytes(DATA.into(), bytes),
                        });
                    }
                    11 => {
                        state.lock().unwrap().hook = Some(port::Hook {
                            key: port::Key::new("read", "d3-profile", 0, 2),
                            edit: port::Edit::Bytes(
                                "d3-profile".into(),
                                raw::reference(2, "d3-profile"),
                            ),
                        });
                    }
                    12 => install_fault(&state, "read", "FILEMAP", 0, 1, port::Action::Zero),
                    13 => {
                        let mut bytes = raw::reference(1, "FILEMAP");
                        raw::put64(&mut bytes, 32, 2);
                        raw::resign(&mut bytes);
                        state.lock().unwrap().hook = Some(port::Hook {
                            key: port::Key::new("read", "FILEMAP", 0, 2),
                            edit: port::Edit::Bytes("FILEMAP".into(), bytes),
                        });
                    }
                    14 => install_fault(
                        &state,
                        "read",
                        DATA,
                        PAGE_BYTES as u64,
                        2,
                        port::Action::BeforeError,
                    ),
                    _ => unreachable!(),
                }
                let result = bound.load_into(1, &target);
                {
                    let state = state.lock().unwrap();
                    assert_eq!(state.fault_hits + state.hook_hits, 1);
                    assert!(state.fault.is_none() && state.hook.is_none());
                    if case == 4 {
                        assert!(state.events.iter().any(|event| event.op == "read"
                            && event.name == DATA
                            && event.error_prefix == 17));
                    }
                    if matches!(case, 8..=11 | 13 | 14) {
                        assert!(state.events.iter().any(|event| event.op == "read"
                            && event.name == DATA
                            && event.offset == PAGE_BYTES as u64
                            && event.returned == Some(PAGE_BYTES)));
                    }
                }
                if case <= 2 {
                    let after = result.unwrap();
                    assert_eq!(after.after.generation, before[1].2.unwrap().1 + 1);
                    assert_clean(&snapshots(&bound.pool, 4)[1], 1);
                    assert!(!storage.session_fence().is_fenced());
                } else {
                    let e = error(result);
                    assert_eq!(e.phase, Plan4PagePhase::Load);
                    let retryable = matches!(case, 3 | 4 | 14);
                    assert_eq!(
                        e.effect,
                        if retryable {
                            Effect::Unchanged
                        } else {
                            Effect::CallerMustResolve
                        }
                    );
                    assert_eq!(storage.session_fence().is_fenced(), !retryable);
                    match (case, &e.cause) {
                        (3 | 4 | 14, Plan4PageCause::PageIo(PageIoError::Read(inner))) => {
                            assert_eq!(inner.io_kind, Some(std::io::ErrorKind::Other));
                            assert_eq!(inner.effect(), Effect::Unchanged);
                            assert_eq!(inner.certainty(), foundation::error::Certainty::Known);
                        }
                        (5 | 12, Plan4PageCause::PageIo(PageIoError::Read(inner))) => {
                            assert_eq!(inner.io_kind, Some(std::io::ErrorKind::UnexpectedEof))
                        }
                        (6, Plan4PageCause::PageIo(PageIoError::Read(_))) => {}
                        (7, Plan4PageCause::PageIo(PageIoError::Namespace(inner))) => assert_eq!(
                            inner.kind,
                            foundation::namespace::NamespaceErrorKind::InvalidType
                        ),
                        (
                            8 | 9 | 13,
                            Plan4PageCause::Genesis(genesis::Plan4GenesisCause::IdentityMismatch),
                        ) => {}
                        (10, Plan4PageCause::Retention(_)) => {}
                        (11, Plan4PageCause::PageIo(PageIoError::Control(_))) => {}
                        _ => panic!("G5 I{case:02} unexpected nested cause {:?}", e.cause),
                    }
                    assert_eq!(snapshots(&bound.pool, 4), before);
                }
            });
        });
    }
    h.finish();
}

#[test]
#[ignore = "G5 S5 profile/frame expansion"]
fn g5_s5_profile_frames() {
    let cases: Vec<_> = (1..=3)
        .flat_map(|profile| [4, 5, 8, 64].into_iter().map(move |count| (profile, count)))
        .filter(|(profile, count)| !(*profile == 1 && matches!(count, 4 | 64)))
        .collect();
    assert_eq!(cases.len(), 10);
    let mut h = raw::Harness::new(
        cases
            .iter()
            .map(|(p, n)| format!("P4-G5-SL-P{p}-F{n:02}"))
            .collect(),
    );
    for (profile, count) in cases {
        h.case(&format!("P4-G5-SL-P{profile}-F{count:02}"), || {
            fixture!(profile, count, |bound, core, state, storage| {
                lifecycle(&mut bound, &core, count);
            });
        });
    }
    h.finish();
}
#[test]
#[ignore = "G5 S5 required-address positions"]
fn g5_s5_required_positions() {
    const ORDERS: [[usize; 3]; 6] = [
        [1, 2, 7],
        [1, 7, 2],
        [2, 1, 7],
        [2, 7, 1],
        [7, 1, 2],
        [7, 2, 1],
    ];
    let cases: Vec<_> = (1..=3)
        .flat_map(|p| (0..6).flat_map(move |o| (1..=3).map(move |c| (p, o, c))))
        .filter(|entry| *entry != (1, 0, 1))
        .collect();
    assert_eq!(cases.len(), 53);
    let mut h = raw::Harness::new(
        cases
            .iter()
            .map(|(p, o, c)| format!("P4-G5-SS-P{p}-O{o}-C{c}"))
            .collect(),
    );
    for (profile, order, cursor) in cases {
        h.case(&format!("P4-G5-SS-P{profile}-O{order}-C{cursor}"), || {
            fixture!(profile, 4, |bound, core, state, storage| {
                required_positions(&mut bound, &core, ORDERS[order], cursor);
            });
        });
    }
    h.finish();
}
#[test]
#[ignore = "G5 S5 full home identity expansion"]
fn g5_s5_home_identity() {
    let mut h = raw::Harness::new(
        (2..=8)
            .flat_map(|p| (1..=8).map(move |d| format!("P4-G5-SI-P{p:02}-D{d:02}")))
            .collect(),
    );
    for number in 2..=8 {
        for dimension in 1..=8 {
            h.case(&format!("P4-G5-SI-P{number:02}-D{dimension:02}"), || {
                rejected_home(number, dimension)
            });
        }
    }
    h.finish();
}
#[test]
#[ignore = "G5 S5 nonzero home LSN expansion"]
fn g5_s5_home_lsn() {
    let mut h = raw::Harness::new((2..=8).map(|p| format!("P4-G5-SH-P{p:02}")).collect());
    for number in 2..=8 {
        h.case(&format!("P4-G5-SH-P{number:02}"), || {
            rejected_home(number, 9)
        });
    }
    h.finish();
}
