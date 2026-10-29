//! New actual dirty-consumer cases. Oracle bytes are frozen independent fixtures.
use super::super::{BoundAllocation, DirtyReader};
use super::test_support::{Capture, Hook, Point, Resume, STEP};
use super::*;
use crate::storage::{
    dirty_frame::{
        self, DirtyEntry, DirtyFrame, ReadTargets, WriteTargets, test_support as frame_test,
    },
    session_fence::SessionFence,
    wal_io::{self as wal, FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use foundation::codec::content_digest;
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, mpsc},
};

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
);
fn literal(profile: u32, count: usize, part: &str) -> Vec<u8> {
    std::fs::read(format!("{INPUT}/p{profile}-n{count}-{part}")).unwrap()
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn reset(state: &Arc<Mutex<port::State>>) {
    let mut state = state.lock().unwrap();
    state.events.clear();
    state.read_events.clear();
    state.reads = 0;
    state.hit = 0;
    state.peak = state.handles;
}
fn files(state: &Arc<Mutex<port::State>>) -> BTreeMap<String, (u64, [u8; 32])> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, entry)| {
            assert!(entry.sparse_tail.is_empty());
            (name.clone(), (entry.length, content_digest(&entry.bytes)))
        })
        .collect()
}
fn fixed_files(state: &Arc<Mutex<port::State>>) -> BTreeMap<String, (u64, [u8; 32])> {
    let mut files = files(state);
    files.retain(|name, _| !name.starts_with("d3-wal-"));
    files
}
fn wal_bytes(state: &Arc<Mutex<port::State>>) -> Vec<u8> {
    state.lock().unwrap().files[wal::WAL_NAME].bytes.clone()
}
fn raw_prefix(storage: &MutableStorageOwner<'_, '_, port::Writer>) -> WalPrefix {
    storage
        .wal()
        .append
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .prefix
}
fn request(identity: PageIdentity) -> TargetRequest {
    TargetRequest {
        identity,
        kind: PageKind::Allocation,
        level: 0,
    }
}
fn case(group: &str, number: usize) {
    let id = format!("{group}-{number:02}");
    let fixture =
        include_str!("../../../../../../tests/fixtures/dev3-dirty-allocation/normal-cases.tsv");
    assert_eq!(
        fixture
            .lines()
            .skip(1)
            .filter(|line| line.split('\t').next() == Some(&id))
            .count(),
        1
    );
    println!("DIRTY_CASE\t{id}");
}
fn complete(group: &str, count: usize) {
    println!("DIRTY_COMPLETE {group} {count}");
}
fn ok<T, E: std::fmt::Debug>(result: std::result::Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected error: {error:?}"))
}
fn err<T, E: std::fmt::Debug>(result: std::result::Result<T, E>) -> E {
    match result {
        Err(error) => error,
        Ok(_) => panic!("unexpected success"),
    }
}
fn dirty_error(error: dirty_frame::DirtyError, cause: dirty_frame::Cause, effect: Effect) {
    assert_eq!((error.cause, error.effect), (cause, effect));
}
fn consumer_frame_error(
    error: DirtyAllocationError,
    phase: Phase,
    cause: dirty_frame::Cause,
    effect: Effect,
) {
    assert_eq!((error.phase, error.effect), (phase, effect));
    match error.cause {
        Cause::Frame(inner) => dirty_error(inner, cause, effect),
        other => panic!("unexpected {other:?}"),
    }
}
macro_rules! storage_fixture {
    ($profile:expr, |$storage:ident, $state:ident| $body:block) => {{
        let mut slot = port::slot();
        let mut flush = boxed::<{4 << 20}>();
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create(namespace, $profile, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut flush).unwrap();
        let $storage = installed.into_mutable_storage();
        reset(&$state);
        $body
        drop($storage);
        let state = $state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
    }};
}
macro_rules! fixture {
    ($profile:expr, |$bound:ident, $core:ident, $state:ident| $body:block) => {{
        storage_fixture!($profile, |storage, $state| {
            let $core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut payload = boxed::<PAGE_BYTES>();
            let mut operation = boxed::<OPERATION_BYTES>();
            let mut frames = [DirtyFrame::new(&mut payload)];
            let mut $bound = ok(storage.bind_allocator(&$core, &mut frames, &mut operation));
            let _ = &mut $bound;
            reset(&$state);
            $body
            drop($bound);
            assert!(storage.session_fence().is_fenced());
        });
    }};
}
fn diagnose(pool: &SharedPool<'_, '_, '_, '_>) -> (Box<[u8; PAGE_BYTES]>, frame_test::Snapshot) {
    let mut bytes = boxed::<PAGE_BYTES>();
    let snapshot = frame_test::diagnose(pool, 0, &mut bytes);
    (bytes, snapshot)
}
fn assert_idle(pool: &SharedPool<'_, '_, '_, '_>) {
    let (_, snapshot) = diagnose(pool);
    assert_eq!(snapshot.total, 0);
    assert!(snapshot.pins.iter().all(|count| *count == 0));
}
fn assert_installed(
    writer: &AllocationWriter<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>,
    profile: u32,
    count: usize,
    lsn: u64,
    cancel: bool,
) {
    let (bytes, snapshot) = diagnose(writer.pool);
    let kind = if cancel { "cancel" } else { "reserve" };
    assert_eq!(&bytes[..], literal(profile, count, &format!("{kind}.page")));
    assert_eq!(
        &writer.control.descriptor[..],
        literal(profile, count, &format!("{kind}.descriptor"))
    );
    assert_eq!(snapshot.frame.identity, writer.control.identity);
    assert_eq!(snapshot.frame.generation, 1);
    assert_eq!(snapshot.frame.page_lsn, PageLsn(lsn));
    assert_eq!(snapshot.frame.modification_counter, lsn);
    assert_eq!(snapshot.frame.rec_lsn, Some(PageLsn(1)));
    assert!(snapshot.dirty);
    assert_eq!(
        snapshot.entry,
        Some(DirtyEntry {
            identity: writer.control.identity,
            generation: 1,
            rec_lsn: PageLsn(1),
            modification_counter: lsn
        })
    );
}

fn post_durable_snapshot(cancel: bool, contended: bool) {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        if cancel {
            let mut owner = ok(core.try_owner(OwnerRole::Foreground));
            ok(writer.reserve_pages(&mut owner, 1));
        }
        let mut retry_owner = ok(core.try_owner(OwnerRole::Foreground));
        let pause = BeforeSyncPause::install(writer.control);
        let (arrived, ack) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        frame_test::install_flush_pause(
            writer.pool,
            frame_test::FlushPoint::Finalized,
            arrived,
            release,
        );
        let storage = writer.storage;
        let (result, flushed, fenced_at_return) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut owner = ok(core.try_owner(OwnerRole::Foreground));
                if cancel {
                    writer.cancel_reserved(&mut owner)
                } else {
                    writer.reserve_pages(&mut owner, 1)
                }
            });
            pause.arrived();
            let flush = scope.spawn(|| {
                let mut owner = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                flusher.try_flush_one(&mut owner)
            });
            ack.recv_timeout(STEP).unwrap();
            if contended {
                pause.resume();
                let result = worker.join().unwrap();
                let fenced = storage.session_fence().is_fenced();
                resume.send(frame_test::Resume::Continue).unwrap();
                (result, flush.join().unwrap(), fenced)
            } else {
                resume.send(frame_test::Resume::Continue).unwrap();
                let flushed = flush.join().unwrap();
                pause.resume();
                let result = worker.join().unwrap();
                (result, flushed, storage.session_fence().is_fenced())
            }
        });
        let lsn = if cancel { 2 } else { 1 };
        let prefix = raw_prefix(storage);
        assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (lsn, lsn));
        let (bytes, snapshot) = diagnose(writer.pool);
        let kind = if cancel { "cancel" } else { "reserve" };
        assert_eq!(&bytes[..], literal(1, 1, &format!("{kind}.page")));
        assert_eq!(
            &writer.control.descriptor[..],
            literal(1, 1, &format!("{kind}.descriptor"))
        );
        assert_eq!(snapshot.frame.page_lsn, PageLsn(lsn));
        assert!(!snapshot.dirty);
        assert_idle(writer.pool);
        if contended {
            let failure = err(result);
            assert_eq!(
                (failure.phase, failure.effect),
                (Phase::Snapshot, Effect::CallerMustResolve)
            );
            match failure.cause {
                Cause::Frame(inner) => {
                    dirty_error(inner, dirty_frame::Cause::Busy, Effect::Unchanged);
                }
                other => panic!("unexpected {other:?}"),
            }
            assert!(fenced_at_return);
            assert_eq!(err(flushed).effect, Effect::CallerMustResolve);
            let installation = writer.control.installation.as_ref().unwrap();
            assert_eq!(installation.phase, InstallPhase::DptInstalled);
            assert_eq!(installation.requirement.location().lsn, lsn);
            assert_eq!(installation.cancel, cancel);
            assert!(writer.control.batch.is_some());
            assert_eq!(writer.control.terminal_pending, cancel);
            reset(&state);
            let before = files(&state);
            assert_eq!(
                err(writer.observe(&mut retry_owner)).effect,
                Effect::CallerMustResolve
            );
            assert_eq!(files(&state), before);
            assert!(state.lock().unwrap().events.is_empty());
            assert_eq!(state.lock().unwrap().reads, 0);
        } else {
            let observed = ok(result);
            assert_eq!(observed.page_lsn, PageLsn(lsn));
            assert!(matches!(
                ok(flushed),
                super::super::allocator_flush::FlushProgress::Complete(_)
            ));
            assert!(!fenced_at_return);
            assert!(writer.control.installation.is_none());
            assert_eq!(writer.control.batch.is_none(), cancel);
            assert!(!writer.control.terminal_pending);
        }
    });
}

#[test]
fn post_durable_snapshot_reserve_busy() {
    post_durable_snapshot(false, true);
}
#[test]
fn post_durable_snapshot_cancel_busy() {
    post_durable_snapshot(true, true);
}
#[test]
fn post_durable_snapshot_reserve_released() {
    post_durable_snapshot(false, false);
}
#[test]
fn post_durable_snapshot_cancel_released() {
    post_durable_snapshot(true, false);
}

#[test]
fn dirty_da01_binding_authority() {
    case("DA01", 1);
    storage_fixture!(1, |storage, state| {
        let original = files(&state);
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut page = boxed::<PAGE_BYTES>();
        let mut work = boxed::<OPERATION_BYTES>();
        let mut frames = [DirtyFrame::new(&mut page)];
        let mut bound = ok(storage.bind_allocator(&core, &mut frames, &mut work));
        let identity = bound.writer.identity;
        let (mut writer, reader) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let observed = writer.observe(&mut owner).unwrap();
        assert_eq!(
            (
                observed.page_lsn,
                observed.rec_lsn,
                observed.modification_counter
            ),
            (PageLsn(0), None, 0)
        );
        let read = ok(reader.read(&mut owner, &request(identity)));
        let initial = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/dev3-storage-session/literal/profile1-allocation.bin"
        ))
        .unwrap();
        assert_eq!(&read.bytes(0).unwrap()[..], initial);
        read.release_checked().unwrap();
        assert_eq!(files(&state), original);
        assert!(state.lock().unwrap().events.is_empty());
    });
    case("DA01", 2);
    storage_fixture!(1, |storage, state| {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut page = boxed::<PAGE_BYTES>();
        let mut work = boxed::<OPERATION_BYTES>();
        let mut frames = [DirtyFrame::new(&mut page)];
        let error = err(storage.bind_allocator(&core, &mut frames, &mut work));
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(matches!(error.cause, super::super::Cause::BindingMismatch));
        assert_eq!(state.lock().unwrap().reads, 0);
        assert!(!storage.session_fence().is_fenced());
        assert!(!fence.is_fenced());
    });
    case("DA01", 3);
    fixture!(1, |bound, core, state| {
        let other = PageConcurrency::new(bound.storage.session_fence()).unwrap();
        let mut wrong = other.try_owner(OwnerRole::Foreground).unwrap();
        let before = files(&state);
        let (mut writer, _) = bound.split();
        consumer_frame_error(
            err(writer.reserve_pages(&mut wrong, 1)),
            Phase::Snapshot,
            dirty_frame::Cause::BindingMismatch,
            Effect::Unchanged,
        );
        assert_idle(writer.pool);
        assert_eq!(files(&state), before);
        assert!(!writer.storage.session_fence().is_fenced());
        let mut proper = core.try_owner(OwnerRole::Foreground).unwrap();
        assert_eq!(writer.observe(&mut proper).unwrap().page_lsn, PageLsn(0));
        // Same-fence wrong-core precedence also applies before idle checking.
        std::mem::forget(wrong.try_apply().unwrap());
        consumer_frame_error(
            err(writer.reserve_pages(&mut wrong, 1)),
            Phase::Snapshot,
            dirty_frame::Cause::BindingMismatch,
            Effect::Unchanged,
        );
        assert!(!writer.storage.session_fence().is_fenced());
        assert_idle(writer.pool);
        assert_eq!(files(&state), before);
        // The origin owner's later Drop diagnoses its forgotten admission. That
        // teardown fence is separate from the unchanged wrong-core rejection.
    });
    for (number, count) in [(4, 0), (5, 65)] {
        case("DA01", number);
        storage_fixture!(1, |storage, state| {
            let core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut pages: Vec<_> = (0..count).map(|_| boxed::<PAGE_BYTES>()).collect();
            let mut frames: Vec<_> = pages.iter_mut().map(|page| DirtyFrame::new(page)).collect();
            let mut work = boxed::<OPERATION_BYTES>();
            let error = err(storage.bind_allocator(&core, &mut frames, &mut work));
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(error.cause, super::super::Cause::Shape));
            assert_eq!(state.lock().unwrap().reads, 0);
            assert!(!storage.session_fence().is_fenced());
        });
    }
    case("DA01", 6);
    fixture!(1, |bound, core, state| {
        let mut page = boxed::<PAGE_BYTES>();
        let mut work = boxed::<OPERATION_BYTES>();
        let mut frames = [DirtyFrame::new(&mut page)];
        let error = err(bound.storage.bind_allocator(&core, &mut frames, &mut work));
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(matches!(error.cause, super::super::Cause::AlreadyBound));
        assert_eq!(state.lock().unwrap().reads, 0);
        assert!(!bound.storage.session_fence().is_fenced());
    });
    claimed_binding_loser();
    case("DA01", 7);
    binding_readback_failure();
    case("DA01", 8);
    binding_resplit_then_drop();
    complete("DA01", 8);
}

fn binding_readback_failure() {
    storage_fixture!(1, |storage, state| {
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut page = boxed::<PAGE_BYTES>();
        let mut work = boxed::<OPERATION_BYTES>();
        let mut frames = [DirtyFrame::new(&mut page)];
        let before = files(&state);
        state.lock().unwrap().read_plan = Some(port::Plan {
            at: 0,
            action: port::Action::BeforeError,
        });
        let error = err(storage.bind_allocator(&core, &mut frames, &mut work));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(matches!(error.cause, super::super::Cause::PageIo(_)));
        assert!(storage.session_fence().is_fenced());
        assert_eq!(
            storage.binding.load(std::sync::atomic::Ordering::Acquire),
            super::super::ABANDONED
        );
        assert_eq!(files(&state), before);
        let state = state.lock().unwrap();
        assert!(state.events.is_empty());
        assert_eq!(
            (
                state.configured_read_hits,
                state.continuation_read_hits,
                state.hit
            ),
            (1, 0, 1)
        );
        assert_eq!((state.reads, state.read_events.len()), (1, 1));
        let event = &state.read_events[0];
        assert_eq!(
            (event.kind, event.name.as_str(), event.offset, event.length),
            ("open", "d3-profile", 0, 0)
        );
        assert!(event.data.is_empty());
        assert_eq!(event.returned, None);
    });
}
fn binding_resplit_then_drop() {
    fixture!(1, |bound, core, state| {
        let identity = bound.writer.identity;
        let original = files(&state);
        for _ in 0..2 {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            assert_eq!(writer.observe(&mut owner).unwrap().page_lsn, PageLsn(0));
            assert_eq!(diagnose(writer.pool).1.frame.identity, identity);
            assert_eq!(diagnose(writer.pool).1.frame.generation, 1);
        }
        assert_eq!(files(&state), original);
        // fixture teardown checks the final common fence; no new bind is claimed.
    });
}
#[test]
#[ignore = "narrow correction run; full DA01 owns S5 inventory"]
fn dirty_da01_readback_and_resplit_repair() {
    case("DA01", 7);
    binding_readback_failure();
    case("DA01", 8);
    binding_resplit_then_drop();
    println!("DIRTY_REPAIR_COMPLETE DA01 2");
}

fn claimed_binding_loser() {
    storage_fixture!(1, |storage, state| {
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let (entered, arrival) = channel();
        let (release, resume) = channel();
        state.lock().unwrap().read_pause = Some(port::Pause {
            at: 0,
            after: false,
            entered,
            release: resume,
            unwind: false,
        });
        let storage_ref = &storage;
        let core_ref = &core;
        let state_ref = &state;
        std::thread::scope(|scope| {
            let loser = scope.spawn(move || {
                arrival
                    .recv_timeout(STEP)
                    .expect("claim winner readback not reached");
                assert_eq!(
                    storage_ref
                        .binding
                        .load(std::sync::atomic::Ordering::Acquire),
                    super::super::CLAIMED
                );
                let reads = state_ref.lock().unwrap().reads;
                let before = files(state_ref);
                let mut page = boxed::<PAGE_BYTES>();
                page.fill(0xa5);
                let mut work = boxed::<OPERATION_BYTES>();
                work.fill(0x5a);
                {
                    let mut frames = [DirtyFrame::new(&mut page)];
                    let error = err(storage_ref.bind_allocator(core_ref, &mut frames, &mut work));
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(matches!(error.cause, super::super::Cause::AlreadyBound));
                    frames[0].check_empty().unwrap();
                }
                assert!(page.iter().all(|byte| *byte == 0xa5));
                assert!(work.iter().all(|byte| *byte == 0x5a));
                assert_eq!(state_ref.lock().unwrap().reads, reads);
                assert_eq!(files(state_ref), before);
                assert!(!storage_ref.session_fence().is_fenced());
                release.try_send(()).unwrap();
            });
            let mut page = boxed::<PAGE_BYTES>();
            let mut work = boxed::<OPERATION_BYTES>();
            let mut frames = [DirtyFrame::new(&mut page)];
            let bound = ok(storage.bind_allocator(&core, &mut frames, &mut work));
            loser.join().unwrap();
            assert_eq!(
                storage.binding.load(std::sync::atomic::Ordering::Acquire),
                super::super::BOUND
            );
            drop(bound);
        });
    });
}

fn normal_history(profile: u32, expanded: bool) {
    fixture!(profile, |bound, core, state| {
        let fixed = fixed_files(&state);
        let mut expected_wal = wal_bytes(&state);
        let (mut writer, reader) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut read_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let request = request(writer.control.identity);
        let mut lsn = 0;
        for count in if expanded { &[1, 8, 63][..] } else { &[1][..] } {
            for cancel in [false, true] {
                if !expanded {
                    case("DA02", if cancel { 2 } else { 1 });
                }
                lsn += 1;
                let observation = if cancel {
                    writer.cancel_reserved(&mut owner)
                } else {
                    writer.reserve_pages(&mut owner, *count)
                }
                .unwrap();
                let kind = if cancel { "cancel" } else { "reserve" };
                expected_wal.extend_from_slice(&literal(profile, *count, &format!("{kind}.frame")));
                assert_eq!(wal_bytes(&state), expected_wal);
                assert_installed(&writer, profile, *count, lsn, cancel);
                assert_eq!(
                    (
                        observation.page_lsn.0,
                        observation.modification_counter,
                        observation.rec_lsn
                    ),
                    (lsn, lsn, Some(PageLsn(1)))
                );
                assert_eq!(observation.reserved, if cancel { 0 } else { *count });
                assert_eq!(
                    (observation.wal.complete_lsn, observation.wal.synced_lsn),
                    (lsn, lsn)
                );
                assert_eq!(
                    (
                        observation.wal.complete_offset,
                        observation.wal.synced_offset
                    ),
                    (expected_wal.len() as u64, expected_wal.len() as u64)
                );
                assert!(writer.control.installation.is_none());
                assert!(!writer.control.terminal_pending);
                assert_eq!(writer.control.batch.is_none(), cancel);
                assert_eq!(
                    writer.reserved_page(&mut owner, 0).unwrap().is_none(),
                    cancel
                );
                assert!(writer.reserved_page(&mut owner, *count).unwrap().is_none());
                let before_reads = state.lock().unwrap().reads;
                let read = ok(reader.read(&mut read_owner, &request));
                assert_eq!(
                    &read.bytes(0).unwrap()[..],
                    literal(profile, *count, &format!("{kind}.page"))
                );
                assert_eq!(read.observe(0).unwrap().entry.unwrap().rec_lsn, PageLsn(1));
                read.release_checked().unwrap();
                assert_eq!(state.lock().unwrap().reads, before_reads);
                assert_idle(writer.pool);
                assert!(!core.diagnostic_version_busy());
                assert_eq!(fixed_files(&state), fixed);
                if expanded {
                    println!("DIRTY_EXPANDED P{profile} N{count} {kind}");
                }
            }
        }
    });
}
#[test]
fn dirty_da02_allocator_consumer() {
    normal_history(1, false);
    complete("DA02", 2);
}
#[test]
#[ignore = "DEV3 S5 expanded 3 profiles x 1->8->63 only"]
fn dirty_s5_expanded_history() {
    let mut rows =
        include_str!("../../../../../../tests/fixtures/dev3-dirty-allocation/expanded-history.tsv")
            .lines()
            .skip(1);
    for profile in 1..=3 {
        for (sequence, (count, operation)) in [1, 8, 63]
            .into_iter()
            .flat_map(|count| [(count, "reserve"), (count, "cancel")])
            .enumerate()
        {
            let sequence = sequence + 1;
            let expected = format!(
                "DA-S5-P{profile}-{sequence}\t{profile}\t{sequence}\t{count}\t{operation}\t{sequence}\t1\tp{profile}-n{count}-{operation}.frame"
            );
            assert_eq!(rows.next(), Some(expected.as_str()));
        }
    }
    assert!(rows.next().is_none());
    for profile in 1..=3 {
        normal_history(profile, true);
    }
    println!("DIRTY_EXPANDED_COMPLETE 18");
}

macro_rules! mechanical_fixture {
    ($identity:expr, |$pool:ident, $core:ident, $requests:ident| $body:block) => {{
        let identity = $identity;
        let fence = SessionFence::new();
        let binding = std::sync::atomic::AtomicU8::new(2);
        let $core = PageConcurrency::new(&fence).unwrap();
        let mut first = boxed::<PAGE_BYTES>();
        let mut second = boxed::<PAGE_BYTES>();
        let mut frames = [DirtyFrame::new(&mut first), DirtyFrame::new(&mut second)];
        let mut second_identity = identity;
        second_identity.page.page_no += 1;
        let $requests = [request(identity), request(second_identity)];
        frame_test::register(&mut frames[0], $requests[0]);
        frame_test::register(&mut frames[1], $requests[1]);
        let $pool = ok(SharedPool::new(&$core, &fence, &binding, &frames));
        $body
    }};
}
fn channel<T>() -> (mpsc::SyncSender<T>, mpsc::Receiver<T>) {
    mpsc::sync_channel(1)
}

fn reader_first(readers: usize) {
    fixture!(1, |bound, core, state| {
        let fixed = files(&state);
        let (mut writer, reader) = bound.split();
        let request = request(writer.control.identity);
        let pool = writer.pool;
        let core_ref = &core;
        std::thread::scope(|scope| {
            let mut commands = Vec::new();
            let mut workers = Vec::new();
            for index in 0..readers {
                let (arrived, arrival) = channel();
                let (release, resume) = channel();
                let reader = &reader;
                workers.push(scope.spawn(move || {
                    let role = if index == 0 {
                        OwnerRole::DedicatedFlusher
                    } else {
                        OwnerRole::Foreground
                    };
                    let mut owner = core_ref.try_owner(role).unwrap();
                    let held = ok(reader.read(&mut owner, &request));
                    let bytes = held.bytes(0).unwrap().to_vec();
                    arrived.try_send(()).unwrap();
                    resume.recv_timeout(STEP).unwrap();
                    assert_eq!(&held.bytes(0).unwrap()[..], bytes);
                    held.release_checked().unwrap();
                }));
                arrival.recv_timeout(STEP).unwrap();
                commands.push(release);
            }
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            consumer_frame_error(
                err(writer.reserve_pages(&mut owner, 1)),
                Phase::Revalidate,
                dirty_frame::Cause::Busy,
                Effect::Unchanged,
            );
            assert_eq!(files(&state), fixed);
            assert!(!core.diagnostic_version_busy());
            commands.remove(0).try_send(()).unwrap();
            workers.remove(0).join().unwrap();
            if readers == 2 {
                consumer_frame_error(
                    err(writer.reserve_pages(&mut owner, 1)),
                    Phase::Revalidate,
                    dirty_frame::Cause::Busy,
                    Effect::Unchanged,
                );
                assert_eq!(files(&state), fixed);
                commands.remove(0).try_send(()).unwrap();
                workers.remove(0).join().unwrap();
            }
            writer.reserve_pages(&mut owner, 1).unwrap();
            assert_installed(&writer, 1, 1, 1, false);
            assert_idle(pool);
        });
    });
}
#[test]
fn dirty_da03_content_exclusion() {
    case("DA03", 1);
    reader_first(1);
    case("DA03", 2);
    reader_first(2);
    case("DA03", 3);
    fixture!(1, |bound, core, state| {
        let (hook, controller) = Hook::at(Point::AfterStageOwned);
        bound.writer.hook = Some(Arc::clone(&hook));
        let (mut writer, reader) = bound.split();
        let request = request(writer.control.identity);
        let core_ref = &core;
        let pool = writer.pool;
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let capture = controller.arrived();
                assert_eq!(
                    capture.installation.unwrap().phase,
                    InstallPhase::StageOwned
                );
                assert!(core_ref.diagnostic_version_busy());
                assert!(frame_test::content_busy(pool, 0));
                let mut owner = core_ref.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                dirty_error(
                    err(reader.read(&mut owner, &request)),
                    dirty_frame::Cause::Busy,
                    Effect::Unchanged,
                );
                controller.resume(Resume::Continue);
            });
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            writer.reserve_pages(&mut owner, 1).unwrap();
            worker.join().unwrap();
        });
        assert_eq!(hook.hits(), 1);
        assert_installed(&writer, 1, 1, 1, false);
        assert_idle(writer.pool);
        assert_eq!(state.lock().unwrap().events.len(), 2);
    });
    case("DA03", 4);
    fixture!(1, |bound, core, state| {
        let _ = (&core, &state);
        mechanical_fixture!(bound.writer.identity, |pool, mechanical, requests| {
            let (arrived, arrival) = channel();
            let (release, resume) = channel();
            let pool_ref = &pool;
            let core_ref = &mechanical;
            std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let mut owner = core_ref.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    let held =
                        ok(ok(pool_ref.reserve_targets(&mut owner, &requests[1..])).try_read());
                    arrived.try_send(()).unwrap();
                    resume.recv_timeout(STEP).unwrap();
                    held.release_checked().unwrap();
                });
                arrival.recv_timeout(STEP).unwrap();
                let mut owner = mechanical.try_owner(OwnerRole::Maintenance).unwrap();
                dirty_error(
                    err(
                        ok(pool.reserve_targets(&mut owner, &[requests[1], requests[0]]))
                            .try_write(),
                    ),
                    dirty_frame::Cause::Busy,
                    Effect::Unchanged,
                );
                assert!(!frame_test::content_busy(&pool, 0));
                let (_, snapshot) = diagnose(&pool);
                assert_eq!(snapshot.total, 1);
                assert_eq!(snapshot.pins[0], 0);
                release.try_send(()).unwrap();
                worker.join().unwrap();
                let write = ok(
                    ok(pool.reserve_targets(&mut owner, &[requests[1], requests[0]])).try_write(),
                );
                write.release_checked().unwrap();
                assert_idle(&pool);
            });
        });
    });
    case("DA03", 5);
    fixture!(1, |bound, core, state| {
        let _ = (&core, &state);
        mechanical_fixture!(bound.writer.identity, |pool, mechanical, requests| {
            let mut owner = mechanical.try_owner(OwnerRole::Foreground).unwrap();
            let mut duplicate = requests[0];
            duplicate.identity.page.incarnation = Generation::new(2).unwrap();
            dirty_error(
                err(pool.reserve_targets(&mut owner, &[requests[0], duplicate])),
                dirty_frame::Cause::DuplicateTarget,
                Effect::Unchanged,
            );
            dirty_error(
                err(pool.reserve_targets(&mut owner, &[duplicate])),
                dirty_frame::Cause::MissingTarget,
                Effect::Unchanged,
            );
            assert_idle(&pool);
        });
    });
    complete("DA03", 5);
}

#[test]
fn dirty_da04_prepared_stamps() {
    case("DA04", 1);
    fixture!(1, |bound, core, state| {
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        writer.reserve_pages(&mut owner, 1).unwrap();
        reset(&state);
        let before_files = files(&state);
        let original = diagnose(writer.pool).0;
        let (hook, controller) = Hook::at(Point::Snapshot);
        writer.control.hook = Some(Arc::clone(&hook));
        let pool = writer.pool;
        let identity = writer.control.identity;
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                assert!(controller.arrived().installation.is_none());
                // Test-only coherent metadata mutation, never a second product writer.
                frame_test::seed_counter(pool, 0, 2);
                frame_test::seed_dpt(
                    pool,
                    0,
                    Some(DirtyEntry {
                        identity,
                        generation: 1,
                        rec_lsn: PageLsn(1),
                        modification_counter: 2,
                    }),
                );
                controller.resume(Resume::Continue);
            });
            consumer_frame_error(
                err(writer.cancel_reserved(&mut owner)),
                Phase::Revalidate,
                dirty_frame::Cause::Stale,
                Effect::Unchanged,
            );
            worker.join().unwrap();
        });
        assert_eq!(hook.hits(), 1);
        assert_eq!(files(&state), before_files);
        assert_eq!(diagnose(writer.pool).0, original);
        assert_eq!(diagnose(writer.pool).1.frame.modification_counter, 2);
        assert!(!writer.storage.session_fence().is_fenced());
        assert_idle(writer.pool);
    });
    for number in 2..=5 {
        case("DA04", number);
        fixture!(1, |bound, core, state| {
            let request = request(bound.writer.identity);
            let before_files = files(&state);
            let original = diagnose(&bound.pool).0;
            let mut expected = diagnose(&bound.pool).1.frame;
            match number {
                2 => expected.identity.page.page_no += 1,
                3 => expected.generation += 1,
                4 => expected.page_lsn = PageLsn(1),
                5 => expected.modification_counter = 1,
                _ => unreachable!(),
            }
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let write = ok(ok(bound.pool.reserve_targets(&mut owner, &[request])).try_write());
            dirty_error(
                err(write.matches_stamp(0, &expected)),
                dirty_frame::Cause::Stale,
                Effect::Unchanged,
            );
            write.release_checked().unwrap();
            assert_eq!(files(&state), before_files);
            assert_eq!(diagnose(&bound.pool).0, original);
            assert_idle(&bound.pool);
            assert!(!bound.storage.session_fence().is_fenced());
        });
    }
    complete("DA04", 5);
}

#[allow(
    clippy::too_many_arguments,
    reason = "Independent partial-state oracle compares three before images, operation cut, captured control and live storage separately"
)]
fn assert_partial(
    writer: &AllocationWriter<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>,
    state: &Arc<Mutex<port::State>>,
    before_page: &[u8],
    before_descriptor: &[u8],
    before_wal: &[u8],
    cancel: bool,
    cut: usize,
    capture: &Capture,
) {
    let opcode = if cancel { "cancel" } else { "reserve" };
    let lsn = if cancel { 2 } else { 1 };
    let before_counter = u64::from(cancel);
    let frame = literal(1, 1, &format!("{opcode}.frame"));
    let page = literal(1, 1, &format!("{opcode}.page"));
    let descriptor = literal(1, 1, &format!("{opcode}.descriptor"));
    let (actual_page, snapshot) = diagnose(writer.pool);
    assert_eq!(
        &actual_page[..],
        if cut >= 2 { &page[..] } else { before_page }
    );
    assert_eq!(
        &writer.control.descriptor[..],
        if cut >= 3 {
            &descriptor[..]
        } else {
            before_descriptor
        }
    );
    assert_eq!(capture.descriptor, writer.control.descriptor);
    assert_eq!(
        snapshot.frame.page_lsn.0,
        if cut >= 2 { lsn } else { before_counter }
    );
    assert_eq!(
        snapshot.frame.modification_counter,
        if cut >= 3 { lsn } else { before_counter }
    );
    assert_eq!(snapshot.dirty, cancel || cut >= 3);
    assert_eq!(
        snapshot.frame.rec_lsn,
        (cancel || cut >= 3).then_some(PageLsn(1))
    );
    let expected_dpt_counter = if cut >= 4 {
        Some(lsn)
    } else if cancel {
        Some(1)
    } else {
        None
    };
    assert_eq!(
        snapshot.entry,
        expected_dpt_counter.map(|counter| DirtyEntry {
            identity: writer.control.identity,
            generation: 1,
            rec_lsn: PageLsn(1),
            modification_counter: counter,
        })
    );
    assert_eq!(snapshot.total, 0);
    assert!(snapshot.pins.iter().all(|pins| *pins == 0));
    let batch = writer.control.batch.unwrap();
    assert_eq!((batch.owner.id(), batch.mask, batch.birth), (1, 2, 1));
    assert_eq!(capture.batch, Some(batch));
    assert_eq!(
        (writer.control.work_high_water, capture.work_high_water),
        (1, 1)
    );
    assert_eq!(
        (writer.control.terminal_pending, capture.terminal_pending),
        (cancel, cancel)
    );
    let installation = writer.control.installation.as_ref().unwrap();
    let captured = capture.installation.unwrap();
    let phase = [
        InstallPhase::StageOwned,
        InstallPhase::PageInstalled,
        InstallPhase::DescriptorInstalled,
        InstallPhase::DptInstalled,
    ][cut - 1];
    assert_eq!((installation.phase, captured.phase), (phase, phase));
    assert_eq!(
        (
            captured.generation,
            captured.first_rec_lsn,
            captured.before_counter,
            captured.after_counter,
            captured.cancel
        ),
        (1, 1, before_counter, lsn, cancel)
    );
    assert_eq!(installation.requirement.location(), captured.location);
    assert_eq!(
        (
            captured.location.lsn,
            captured.location.start,
            captured.location.end
        ),
        (
            lsn,
            before_wal.len() as u64,
            before_wal.len() as u64 + frame.len() as u64
        )
    );
    let prefix = raw_prefix(writer.storage);
    check_witness(Witness {
        cut,
        cancel,
        page_lsn: snapshot.frame.page_lsn.0,
        counter: snapshot.frame.modification_counter,
        rec: snapshot.frame.rec_lsn.map(|lsn| lsn.0),
        dirty: snapshot.dirty,
        dpt_counter: snapshot.entry.map(|entry| entry.modification_counter),
        complete: prefix.complete_lsn,
        synced: prefix.synced_lsn,
        obligation: writer.control.installation.is_some(),
        batch: writer.control.batch.is_some(),
        terminal: writer.control.terminal_pending,
    })
    .unwrap();
    assert_eq!(
        (prefix.complete_lsn, prefix.synced_lsn),
        (lsn, before_counter)
    );
    assert_eq!(
        (prefix.complete_offset, prefix.synced_offset),
        (
            before_wal.len() as u64 + frame.len() as u64,
            before_wal.len() as u64
        )
    );
    assert_eq!(wal_bytes(state), before_wal);
    let append = writer.storage.wal().append.lock().unwrap();
    assert!(matches!(append.state, wal::SlotState::Complete(_)));
    assert_eq!(&append.slot.as_ref().unwrap()[..frame.len()], frame);
}
#[test]
fn dirty_da05_staged_obligations() {
    let points = [
        Point::AfterStageOwned,
        Point::AfterPageInstalled,
        Point::AfterDescriptorInstalled,
        Point::AfterDptInstalled,
    ];
    for (operation_index, cancel) in [false, true].into_iter().enumerate() {
        for (cut_index, point) in points.into_iter().enumerate() {
            case("DA05", operation_index * 4 + cut_index + 1);
            fixture!(1, |bound, core, state| {
                let (mut writer, reader) = bound.split();
                let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
                if cancel {
                    writer.reserve_pages(&mut owner, 1).unwrap();
                }
                reset(&state);
                let fixed = fixed_files(&state);
                let before_wal = wal_bytes(&state);
                let before_page = diagnose(writer.pool).0;
                let before_descriptor = writer.control.descriptor.to_vec();
                let (hook, controller) = Hook::at(point);
                writer.control.hook = Some(Arc::clone(&hook));
                let (fenced, fence_arrived) = channel();
                let (release, resume) = channel();
                frame_test::install_fence_pause(writer.pool, fenced, resume);
                let pool = writer.pool;
                let core_ref = &core;
                let fence = writer.storage.session_fence();
                let request = request(writer.control.identity);
                let (capture, mut observed_edges) = std::thread::scope(|scope| {
                    let worker = scope.spawn(move || {
                        let capture = controller.arrived();
                        let mut observed_edges = vec![Edge::StageOwned];
                        assert!(!fence.is_fenced());
                        assert!(frame_test::content_busy(pool, 0));
                        assert!(core_ref.diagnostic_version_busy());
                        controller.resume(Resume::Panic);
                        fence_arrived
                            .recv_timeout(STEP)
                            .expect("unwind fence cut missed");
                        assert!(fence.is_fenced());
                        observed_edges.push(Edge::Fence);
                        assert!(
                            frame_test::content_busy(pool, 0),
                            "content unlocked before observed fence"
                        );
                        assert!(
                            core_ref.diagnostic_version_busy(),
                            "version unlocked before observed fence"
                        );
                        release.try_send(frame_test::Resume::Continue).unwrap();
                        (capture, observed_edges)
                    });
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            let _ = if cancel {
                                writer.cancel_reserved(&mut owner)
                            } else {
                                writer.reserve_pages(&mut owner, 1)
                            };
                        }))
                        .is_err()
                    );
                    worker.join().unwrap()
                });
                assert_eq!(hook.hits(), 1);
                assert!(writer.storage.session_fence().is_fenced());
                assert_partial(
                    &writer,
                    &state,
                    &before_page[..],
                    &before_descriptor,
                    &before_wal,
                    cancel,
                    cut_index + 1,
                    &capture,
                );
                assert!(!frame_test::content_busy(writer.pool, 0));
                assert!(!core.diagnostic_version_busy());
                observed_edges.extend([
                    Edge::ContentReturned,
                    Edge::VersionReturned,
                    Edge::PinsReturned,
                ]);
                check_edges(&observed_edges, true).unwrap();
                assert_eq!(fixed_files(&state), fixed);
                assert!(state.lock().unwrap().events.is_empty());
                // Core entry itself is fenced. Existing owner must also reject fresh service.
                assert_eq!(
                    err(core.try_owner(OwnerRole::DedicatedFlusher)).effect,
                    Effect::CallerMustResolve
                );
                assert_eq!(
                    err(reader.read(&mut owner, &request)).effect,
                    Effect::CallerMustResolve
                );
            });
        }
    }
    complete("DA05", 8);
}

fn lower_error(error: DirtyAllocationError, sync: bool) {
    assert_eq!(
        (error.phase, error.effect),
        (Phase::Sync, Effect::CallerMustResolve)
    );
    let Cause::Wal(error) = error.cause else {
        panic!("original WAL cause lost");
    };
    assert_eq!(
        error.phase,
        if sync {
            wal::Phase::Sync
        } else {
            wal::Phase::Write
        }
    );
    assert_eq!(error.effect, Effect::CallerMustResolve);
    let wal::Cause::Io(error) = error.cause else {
        panic!("original lower I/O cause lost");
    };
    assert_eq!(error.code, foundation::error::ErrorCode::IoWriteUnknown);
    assert_eq!(error.io_kind, Some(std::io::ErrorKind::Other));
}
fn fault_case(cancel: bool, fault: usize) {
    fixture!(1, |bound, core, state| {
        let (mut writer, reader) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        if cancel {
            writer.reserve_pages(&mut owner, 1).unwrap();
        }
        reset(&state);
        let fixed = fixed_files(&state);
        let before_wal = wal_bytes(&state);
        let frame = literal(
            1,
            1,
            if cancel {
                "cancel.frame"
            } else {
                "reserve.frame"
            },
        );
        let (at, action, written) = match fault {
            0 => (0, port::Action::BeforeError, 0),
            1 => (0, port::Action::PrefixError, frame.len() / 2),
            2 => (0, port::Action::AfterError, frame.len()),
            3 => (1, port::Action::BeforeError, frame.len()),
            4 => (1, port::Action::AfterError, frame.len()),
            _ => unreachable!(),
        };
        state.lock().unwrap().plan = Some(port::Plan { at, action });
        let result = if cancel {
            writer.cancel_reserved(&mut owner)
        } else {
            writer.reserve_pages(&mut owner, 1)
        };
        lower_error(err(result), fault >= 3);
        let mut expected = before_wal.clone();
        expected.extend_from_slice(&frame[..written]);
        assert_eq!(wal_bytes(&state), expected);
        assert_installed(&writer, 1, 1, if cancel { 2 } else { 1 }, cancel);
        assert_idle(writer.pool);
        assert!(!core.diagnostic_version_busy());
        assert!(writer.storage.session_fence().is_fenced());
        assert!(writer.control.installation.is_some());
        assert!(writer.control.batch.is_some());
        assert_eq!(writer.control.terminal_pending, cancel);
        let prefix = raw_prefix(writer.storage);
        assert_eq!(
            (prefix.complete_lsn, prefix.synced_lsn),
            if cancel { (2, 1) } else { (1, 0) }
        );
        assert_eq!(
            (prefix.complete_offset, prefix.synced_offset),
            (
                before_wal.len() as u64 + frame.len() as u64,
                before_wal.len() as u64
            )
        );
        assert!(matches!(
            writer.storage.wal().append.lock().unwrap().state,
            wal::SlotState::IoOwned(_)
        ));
        assert_eq!(fixed_files(&state), fixed);
        assert_eq!(state.lock().unwrap().hit, 1);
        assert_eq!(
            state.lock().unwrap().events.len(),
            if fault >= 3 { 2 } else { 1 }
        );
        assert_eq!(
            err(reader.read(&mut owner, &request(writer.control.identity))).effect,
            Effect::CallerMustResolve
        );
    });
}
fn io_pause_case(failure: bool) {
    fixture!(1, |bound, core, state| {
        let fixed = fixed_files(&state);
        let (mut writer, reader) = bound.split();
        let request = request(writer.control.identity);
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (entered, arrived) = channel();
        let (release, resume) = channel();
        {
            let mut state = state.lock().unwrap();
            state.pause = Some(port::Pause {
                at: 0,
                after: false,
                entered,
                release: resume,
                unwind: false,
            });
            if failure {
                state.plan = Some(port::Plan {
                    at: 0,
                    action: port::Action::BeforeError,
                });
            }
        }
        let pool = writer.pool;
        let core_ref = &core;
        let fence = writer.storage.session_fence();
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                arrived
                    .recv_timeout(STEP)
                    .expect("actual lower write cut missed");
                check_io_witness(
                    core_ref.diagnostic_version_busy(),
                    frame_test::content_busy(pool, 0),
                    diagnose(pool).1.total,
                )
                .unwrap();
                assert!(!fence.is_fenced());
                let mut owner = core_ref.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let read = ok(reader.read(&mut owner, &request));
                assert_eq!(&read.bytes(0).unwrap()[..], literal(1, 1, "reserve.page"));
                assert_eq!(
                    read.observe(0).unwrap().entry.unwrap().modification_counter,
                    1
                );
                read.release_checked().unwrap();
                release.try_send(()).unwrap();
            });
            let result = writer.reserve_pages(&mut owner, 1);
            if failure {
                lower_error(err(result), false);
            } else {
                result.unwrap();
            }
            worker.join().unwrap();
        });
        assert_installed(&writer, 1, 1, 1, false);
        assert_idle(writer.pool);
        assert_eq!(fixed_files(&state), fixed);
        if failure {
            assert!(writer.storage.session_fence().is_fenced());
            let reader = DirtyReader { pool: writer.pool };
            assert_eq!(
                err(reader.read(&mut owner, &request)).effect,
                Effect::CallerMustResolve
            );
        }
    });
}
#[test]
fn dirty_da06_sync_and_errors() {
    for (operation_index, cancel) in [false, true].into_iter().enumerate() {
        for fault in 0..5 {
            case("DA06", operation_index * 5 + fault + 1);
            fault_case(cancel, fault);
        }
    }
    case("DA06", 11);
    io_pause_case(false);
    case("DA06", 12);
    io_pause_case(true);
    complete("DA06", 12);
}

fn owners<'c, 's>(core: &'c PageConcurrency<'s>) -> Vec<Owner<'c, 's>> {
    let mut owners: Vec<_> = (0..32)
        .map(|_| core.try_owner(OwnerRole::Foreground).unwrap())
        .collect();
    owners.push(core.try_owner(OwnerRole::Maintenance).unwrap());
    owners.push(core.try_owner(OwnerRole::DedicatedFlusher).unwrap());
    owners
}
fn cleanup_case(first_fatal: bool, poison: bool) {
    fixture!(1, |bound, core, state| {
        let before = files(&state);
        let (mut writer, reader) = bound.split();
        let (hook, controller) = Hook::at(Point::Snapshot);
        writer.control.hook = Some(Arc::clone(&hook));
        let pool = writer.pool;
        let request = request(writer.control.identity);
        let core_ref = &core;
        let (ready, ready_rx) = channel();
        let (cleanup, cleanup_rx) = channel();
        let (release, release_rx) = channel();
        let (finished, finished_rx) = channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let mut owner = core_ref.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let held = if first_fatal {
                    None
                } else {
                    Some(ok(reader.read(&mut owner, &request)))
                };
                ready.try_send(()).unwrap();
                controller.arrived();
                frame_test::install_release_pause(pool, cleanup, release_rx);
                if first_fatal {
                    frame_test::poison_content(pool, 0);
                }
                controller.resume(Resume::Continue);
                cleanup_rx
                    .recv_timeout(STEP)
                    .expect("checked cleanup cut not reached");
                if poison {
                    frame_test::poison_pins(pool);
                } else {
                    frame_test::seed_pins(pool, 0, 35, 35);
                }
                release.try_send(frame_test::Resume::Continue).unwrap();
                finished_rx.recv_timeout(STEP).unwrap();
                if let Some(held) = held {
                    assert_eq!(
                        err(held.release_checked()).effect,
                        Effect::CallerMustResolve
                    );
                }
            });
            ready_rx.recv_timeout(STEP).unwrap();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let error = err(writer.reserve_pages(&mut owner, 1));
            assert_eq!(error.effect, Effect::CallerMustResolve);
            let Cause::Frame(inner) = error.cause else {
                panic!("cleanup frame cause lost");
            };
            dirty_error(
                inner,
                if first_fatal || poison {
                    dirty_frame::Cause::Poisoned
                } else {
                    dirty_frame::Cause::Invariant
                },
                Effect::CallerMustResolve,
            );
            finished.try_send(()).unwrap();
            worker.join().unwrap();
        });
        assert_eq!(hook.hits(), 1);
        assert_eq!(files(&state), before);
        assert!(writer.storage.session_fence().is_fenced());
        assert_eq!(raw_prefix(writer.storage).complete_lsn, 0);
        let (_, snapshot) = diagnose(writer.pool);
        if !poison {
            assert_eq!((snapshot.pins[0], snapshot.total), (35, 35));
        } else {
            assert_eq!(snapshot.total, 0);
        }
    });
}

#[derive(Clone, Copy, Debug)]
struct Witness {
    cut: usize,
    cancel: bool,
    page_lsn: u64,
    counter: u64,
    rec: Option<u64>,
    dirty: bool,
    dpt_counter: Option<u64>,
    complete: u64,
    synced: u64,
    obligation: bool,
    batch: bool,
    terminal: bool,
}
fn check_witness(w: Witness) -> std::result::Result<(), &'static str> {
    if !(1..=4).contains(&w.cut) {
        return Err("cut");
    }
    let before = u64::from(w.cancel);
    let after = before + 1;
    if w.complete != after || w.synced != before {
        return Err("physical frontier");
    }
    if !w.obligation || !w.batch || w.terminal != w.cancel {
        return Err("retained obligation");
    }
    if w.page_lsn != if w.cut >= 2 { after } else { before } {
        return Err("page cut");
    }
    if w.counter != if w.cut >= 3 { after } else { before } {
        return Err("counter cut");
    }
    let dirty = w.cancel || w.cut >= 3;
    if w.dirty != dirty || w.rec != dirty.then_some(1) {
        return Err("dirty first rec");
    }
    let dpt = if w.cut >= 4 {
        Some(after)
    } else if w.cancel {
        Some(1)
    } else {
        None
    };
    if w.dpt_counter != dpt {
        return Err("DPT cut");
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    StageOwned,
    Fence,
    ContentReturned,
    VersionReturned,
    PinsReturned,
    IoAttempt,
}
fn check_io_witness(
    version_busy: bool,
    content_busy: bool,
    pins: u16,
) -> std::result::Result<(), &'static str> {
    if version_busy || content_busy || pins != 0 {
        Err("I/O while writer guards remain")
    } else {
        Ok(())
    }
}
fn check_edges(edges: &[Edge], unwind: bool) -> std::result::Result<(), &'static str> {
    let at = |wanted| edges.iter().position(|edge| *edge == wanted);
    if at(Edge::StageOwned).is_none() {
        return Err("missing stage");
    }
    if unwind
        && !matches!((at(Edge::Fence), at(Edge::ContentReturned)), (Some(f), Some(c)) if f < c)
    {
        return Err("unlock before fence");
    }
    if let Some(io) = at(Edge::IoAttempt) {
        for required in [
            Edge::ContentReturned,
            Edge::VersionReturned,
            Edge::PinsReturned,
        ] {
            if at(required).is_none_or(|release| release >= io) {
                return Err("I/O before guards returned");
            }
        }
    }
    Ok(())
}
fn negative_oracle() {
    let mut rejected = 0;
    for cancel in [false, true] {
        for cut in 1..=4 {
            let before = u64::from(cancel);
            let after = before + 1;
            let dirty = cancel || cut >= 3;
            let good = Witness {
                cut,
                cancel,
                page_lsn: if cut >= 2 { after } else { before },
                counter: if cut >= 3 { after } else { before },
                rec: dirty.then_some(1),
                dirty,
                dpt_counter: if cut >= 4 {
                    Some(after)
                } else if cancel {
                    Some(1)
                } else {
                    None
                },
                complete: after,
                synced: before,
                obligation: true,
                batch: true,
                terminal: cancel,
            };
            check_witness(good).unwrap();
            let mut bad = good;
            bad.synced = after;
            assert!(check_witness(bad).is_err());
            rejected += 1;
            let mut bad = good;
            bad.obligation = false;
            assert!(check_witness(bad).is_err());
            rejected += 1;
            let mut bad = good;
            bad.page_lsn = 99;
            assert!(check_witness(bad).is_err());
            rejected += 1;
            let mut bad = good;
            bad.counter = 99;
            assert!(check_witness(bad).is_err());
            rejected += 1;
            let mut bad = good;
            bad.rec = Some(99);
            assert!(check_witness(bad).is_err());
            rejected += 1;
            let mut bad = good;
            bad.dpt_counter = Some(99);
            assert!(check_witness(bad).is_err());
            rejected += 1;
        }
    }
    let unwind = [
        Edge::StageOwned,
        Edge::Fence,
        Edge::ContentReturned,
        Edge::VersionReturned,
        Edge::PinsReturned,
    ];
    check_edges(&unwind, true).unwrap();
    let mut bad = unwind;
    bad.swap(1, 2);
    assert!(check_edges(&bad, true).is_err());
    rejected += 1;
    let io = [
        Edge::StageOwned,
        Edge::ContentReturned,
        Edge::VersionReturned,
        Edge::PinsReturned,
        Edge::IoAttempt,
    ];
    check_edges(&io, false).unwrap();
    for missing in [
        Edge::ContentReturned,
        Edge::VersionReturned,
        Edge::PinsReturned,
    ] {
        let bad: Vec<_> = io.iter().copied().filter(|edge| *edge != missing).collect();
        assert!(check_edges(&bad, false).is_err());
        rejected += 1;
    }
    check_io_witness(false, false, 0).unwrap();
    for bad in [(true, false, 0), (false, true, 0), (false, false, 1)] {
        assert!(check_io_witness(bad.0, bad.1, bad.2).is_err());
        rejected += 1;
    }
    assert_eq!(rejected, 55);
    println!("DIRTY_ORACLE_NEGATIVES {rejected}");
}
fn type_ceilings() {
    use std::mem::size_of;
    let layout = frame_test::layout();
    for (name, actual, limit) in [
        ("registration_option", layout.registration_option, 128),
        ("pin_accounting", layout.accounting, 512),
        ("pin_set", layout.pin_set, 64),
        ("registration_result", layout.registration_result, 128),
        ("pin_guard", layout.pin_guard, 32),
        ("pin_lock_result", layout.pin_lock_result, 32),
        (
            "pin_guard_and_result",
            layout.pin_guard + layout.pin_lock_result,
            64,
        ),
        ("read_guard", layout.read_guard, 32),
        ("write_guard", layout.write_guard, 32),
        ("pool", layout.pool, 25072),
    ] {
        assert!(actual <= limit, "{name}: {actual} > {limit}");
        println!("DIRTY_TYPE {name} {actual} {limit}");
    }
    macro_rules! ceiling {
        ($name:literal, $ty:ty, $limit:expr) => {{
            let actual = size_of::<$ty>();
            assert!(actual <= $limit, "{}: {} > {}", $name, actual, $limit);
            println!("DIRTY_TYPE {} {} {}", $name, actual, $limit);
        }};
    }
    ceiling!("frame", DirtyFrame<'_>, 256);
    ceiling!("entry", DirtyEntry, 96);
    ceiling!("optional_entry", Option<DirtyEntry>, 96);
    ceiling!("stamp", FrameStamp, 112);
    ceiling!("stamp_result", dirty_frame::Result<FrameStamp>, 112);
    ceiling!("observation", dirty_frame::DirtyObservation, 192);
    ceiling!(
        "observation_result",
        dirty_frame::Result<dirty_frame::DirtyObservation>,
        192
    );
    ceiling!("read_targets", ReadTargets<'_, '_, '_, '_, '_, '_>, 256);
    ceiling!("write_targets", WriteTargets<'_, '_, '_, '_, '_, '_>, 256);
    ceiling!("dpt_stamp", dirty_frame::DptStamp<'_>, 128);
    ceiling!("dirty_error", dirty_frame::DirtyError, 32);
    ceiling!("small_result", dirty_frame::Result<()>, 32);
    ceiling!("installation", InstallObligation<'_>, 192);
    ceiling!("optional_installation", Option<InstallObligation<'_>>, 192);
    ceiling!("install_permit", InstallPermit<'_, '_, '_>, 384);
    ceiling!("writer_control", WriterControl<'_, '_>, 4096);
    ceiling!(
        "bound",
        BoundAllocation<'_, '_, '_, '_, '_, '_, '_, port::Writer>,
        25136
    );
    ceiling!(
        "bound_result",
        super::super::Result<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, port::Writer>>,
        25152
    );
    ceiling!("storage", MutableStorageOwner<'_, '_, port::Writer>, 32768);
    assert_eq!(OPERATION_BYTES, 98304);
    assert_eq!(65536 + 2 * PAGE_BYTES, OPERATION_BYTES);
    assert!(dirty_frame::fits()); // The independent concrete ceilings above remain the oracle.
    use super::super::super::allocation_reservation::budget;
    let owner = size_of::<MutableStorageOwner<'_, '_, port::Writer>>();
    let control = size_of::<WriterControl<'_, '_>>();
    let deep =
        owner + control + budget::PLANS + budget::TEMPORARIES + dirty_frame::CALL_BYTES + 2048;
    assert!(deep <= 65536);
    let generic = 4 * 192 + 4 * 112 + 2 * 96 + 6 * 128 + 2 * 32 + 4 * 32 + 32 * 8 + 64;
    assert_eq!(generic, 2688);
    assert!(generic <= dirty_frame::CALL_BYTES);
    let pool_delta = dirty_frame::init_pool_delta_bytes();
    let control_delta = base_delta();
    let retained = 60672 + pool_delta + control_delta;
    let active = retained + 34 * dirty_frame::CALL_BYTES;
    let cold =
        retained + 90112 + (5 * pool_delta + 2048).max(3 * pool_delta + 8 * control_delta + 2048);
    assert_eq!((active, cold), super::super::shared_dirty_peaks());
    assert!(pool_delta <= dirty_frame::INIT_POOL_DELTA_LIMIT && control_delta <= 64);
    assert!(active <= 196608 && cold <= 196608);
    println!(
        "DIRTY_DEEP_SOURCE owner={owner} control={control} plans={} temporaries={} generic=3072 moved=2048 total={deep} limit=65536",
        budget::PLANS,
        budget::TEMPORARIES
    );
    println!(
        "DIRTY_SOURCE_LEDGER generic=3072 retained={retained} active={active} cold={cold} control_limit=196608 constructor_limit=90112 physical_stack=NOT_MEASURED allocations=NOT_MEASURED"
    );
}
#[test]
fn dirty_da07_resource_cleanup() {
    case("DA07", 1);
    fixture!(1, |bound, core, state| {
        let before = files(&state);
        let request = request(bound.writer.identity);
        let mut owners = owners(&core);
        let pins: Vec<_> = owners
            .iter_mut()
            .map(|owner| ok(bound.pool.reserve_targets(owner, &[request])))
            .collect();
        let (_, snapshot) = diagnose(&bound.pool);
        assert_eq!((snapshot.total, snapshot.pins[0]), (34, 34));
        for pins in pins {
            pins.release_checked().unwrap();
        }
        assert_idle(&bound.pool);
        assert_eq!(files(&state), before);
    });
    case("DA07", 2);
    fixture!(1, |bound, core, state| {
        let _ = (&core, &state);
        mechanical_fixture!(bound.writer.identity, |pool, mechanical, requests| {
            let mut owners = owners(&mechanical);
            let pins: Vec<_> = owners
                .iter_mut()
                .map(|owner| ok(pool.reserve_targets(owner, &requests)))
                .collect();
            let (_, snapshot) = diagnose(&pool);
            assert_eq!(
                (snapshot.total, snapshot.pins[0], snapshot.pins[1]),
                (68, 34, 34)
            );
            for pins in pins {
                pins.release_checked().unwrap();
            }
            assert_idle(&pool);
        });
    });
    case("DA07", 3);
    fixture!(1, |bound, core, state| {
        let _ = (&core, &state);
        mechanical_fixture!(bound.writer.identity, |pool, mechanical, requests| {
            // Explicit synthetic census: no 35th live owner is claimed.
            frame_test::seed_pins(&pool, 0, 34, 34);
            frame_test::seed_pins(&pool, 1, 34, 68);
            let mut owner = mechanical.try_owner(OwnerRole::Foreground).unwrap();
            dirty_error(
                err(pool.reserve_targets(&mut owner, &requests[..1])),
                dirty_frame::Cause::Capacity,
                Effect::Unchanged,
            );
            let (_, snapshot) = diagnose(&pool);
            assert_eq!(
                (snapshot.total, snapshot.pins[0], snapshot.pins[1]),
                (68, 34, 34)
            );
        });
    });
    for (number, generation) in [(4, 0), (5, u64::MAX)] {
        case("DA07", number);
        fixture!(1, |bound, core, state| {
            let _ = (&core, &state);
            let fence = SessionFence::new();
            let binding = std::sync::atomic::AtomicU8::new(2);
            let mechanical = PageConcurrency::new(&fence).unwrap();
            let mut page = boxed::<PAGE_BYTES>();
            let mut frames = [DirtyFrame::new(&mut page)];
            frame_test::register(&mut frames[0], request(bound.writer.identity));
            frame_test::seed_generation(&mut frames[0], generation);
            dirty_error(
                err(SharedPool::new(&mechanical, &fence, &binding, &frames)),
                dirty_frame::Cause::Invariant,
                Effect::CallerMustResolve,
            );
            assert!(fence.is_fenced());
        });
    }
    case("DA07", 6);
    fixture!(1, |bound, core, state| {
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        writer.reserve_pages(&mut owner, 1).unwrap();
        reset(&state);
        frame_test::seed_counter(writer.pool, 0, u64::MAX);
        frame_test::seed_dpt(
            writer.pool,
            0,
            Some(DirtyEntry {
                identity: writer.control.identity,
                generation: 1,
                rec_lsn: PageLsn(1),
                modification_counter: u64::MAX,
            }),
        );
        let before = files(&state);
        let error = err(writer.cancel_reserved(&mut owner));
        assert_eq!(
            (error.phase, error.effect),
            (Phase::Preflight, Effect::Unchanged)
        );
        assert!(matches!(error.cause, Cause::CounterExhausted));
        assert_eq!(files(&state), before);
        assert!(!writer.storage.session_fence().is_fenced());
        assert_idle(writer.pool);
    });
    for number in 7..=9 {
        case("DA07", number);
        fixture!(1, |bound, core, state| {
            match number {
                7 => frame_test::poison_content(&bound.pool, 0),
                8 => frame_test::poison_dpt(&bound.pool),
                9 => frame_test::poison_pins(&bound.pool),
                _ => unreachable!(),
            }
            let before = files(&state);
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            consumer_frame_error(
                err(writer.reserve_pages(&mut owner, 1)),
                Phase::Snapshot,
                dirty_frame::Cause::Poisoned,
                Effect::CallerMustResolve,
            );
            assert_eq!(files(&state), before);
            assert!(writer.storage.session_fence().is_fenced());
        });
    }
    case("DA07", 10);
    cleanup_case(false, true);
    cleanup_case(false, false);
    case("DA07", 11);
    cleanup_case(true, false);
    case("DA07", 12);
    type_ceilings();
    case("DA07", 13);
    negative_oracle();
    complete("DA07", 13);
}

// Only the changed shared layout/budget boundary; old DA07 cases are not rerun.
#[test]
fn dirty_shared_resource_after_closed_init() {
    type_ceilings();
}
