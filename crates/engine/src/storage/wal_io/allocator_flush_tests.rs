//! New flush consumer tests; expected persistent bytes come only from frozen literals.
use super::super::flush_io::test_support as io_test;
use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, test_support as frame_test},
    page::{PAGE_BYTES, PageLsn},
    wal_io::{FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use xadb_foundation::{
    codec::content_digest,
    error::{Certainty, ErrorCode},
};

const STEP: Duration = Duration::from_secs(5);
const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocator-flush-one"
);
const CASES: &str =
    include_str!("../../../../../tests/fixtures/dev3-allocator-flush-one/mandatory-cases.tsv");
const FAULTS: &str =
    include_str!("../../../../../tests/fixtures/dev3-allocator-flush-one/fault-files.tsv");
const EXPANDED: &str =
    include_str!("../../../../../tests/fixtures/dev3-allocator-flush-one/expanded-cases.tsv");
const WRITES: [usize; 11] = [1, 3, 6, 10, 12, 14, 17, 21, 23, 25, 28];
type State = Arc<Mutex<port::State>>;
type Inventory = BTreeMap<String, (u64, [u8; 32])>;

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn literal(name: &str) -> Vec<u8> {
    std::fs::read(format!("{INPUT}/{name}")).unwrap()
}
fn ok<T, E: std::fmt::Debug>(r: std::result::Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("unexpected {e:?}"))
}
fn err<T, E: std::fmt::Debug>(r: std::result::Result<T, E>) -> E {
    match r {
        Err(e) => e,
        Ok(_) => panic!("unexpected success"),
    }
}
fn reset(state: &State) {
    let mut s = state.lock().unwrap();
    s.events.clear();
    s.read_events.clear();
    s.read_effect_counts.clear();
    s.reads = 0;
    s.hit = 0;
    s.plan = None;
    s.peak = s.handles;
}
fn inventory(state: &State) -> Inventory {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(n, e)| {
            assert!(e.sparse_tail.is_empty());
            (n.clone(), (e.length, content_digest(&e.bytes)))
        })
        .collect()
}
fn selected(id: &str) -> bool {
    assert_eq!(
        CASES
            .lines()
            .skip(1)
            .filter(|r| r.split('\t').next() == Some(id))
            .count(),
        1
    );
    let use_case =
        std::env::var("XADB_FLUSH_CASES").map_or(true, |v| v.split(',').any(|s| s == id));
    if use_case {
        println!("FLUSH_CASE_BEGIN\t{id}");
    }
    use_case
}
fn passed(id: &str, count: &mut usize) {
    *count += 1;
    println!("FLUSH_CASE\t{id}");
}
fn complete(group: &str, count: usize) {
    assert!(count > 0);
    println!("FLUSH_COMPLETE {group} {count}");
}
fn request(identity: PageIdentity) -> TargetRequest {
    TargetRequest {
        identity,
        kind: PageKind::Allocation,
        level: 0,
    }
}
fn diagnose(pool: &SharedPool<'_, '_, '_, '_>) -> (Box<[u8; PAGE_BYTES]>, frame_test::Snapshot) {
    let mut page = boxed();
    let snapshot = frame_test::diagnose(pool, 0, &mut page);
    (page, snapshot)
}
fn no_io(state: &State) {
    let s = state.lock().unwrap();
    assert_eq!((s.events.len(), s.reads), (0, 0));
}
fn no_claim(pool: &SharedPool<'_, '_, '_, '_>) {
    assert!(frame_test::flush_diagnose(pool).claim.is_none());
    assert_eq!(diagnose(pool).1.total, 0);
}
fn same_registry(actual: frame_test::FlushSnapshot, expected: &frame_test::FlushSnapshot) {
    assert_eq!(actual.claim, expected.claim);
    assert_eq!(actual.last_attempt, expected.last_attempt);
    assert_eq!(actual.requirements, expected.requirements);
}
fn done(progress: FlushProgress) -> FlushObservation {
    match progress {
        FlushProgress::Complete(v) => v,
        other => panic!("{other:?}"),
    }
}

macro_rules! storage_fixture {
    ($profile:expr, |$storage:ident, $state:ident| $body:block) => {{
        let mut slot = port::slot(); let mut work = boxed::<{4 << 20}>();
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create(namespace, $profile, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut work).unwrap();
        let $storage = installed.into_mutable_storage(); reset(&$state);
        $body
        drop($storage);
        let s = $state.lock().unwrap(); assert_eq!((s.handles, s.writers), (0, 0));
    }};
}
macro_rules! fixture {
    ($profile:expr, |$bound:ident, $core:ident, $state:ident| $body:block) => {{
        storage_fixture!($profile, |storage, $state| {
            let $core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut page = boxed::<PAGE_BYTES>(); let mut operation = boxed::<98304>();
            let mut frames = [DirtyFrame::new(&mut page)];
            let mut $bound = ok(storage.bind_allocator(&$core, &mut frames, &mut operation));
            reset(&$state); $body
            drop($bound); assert!(storage.session_fence().is_fenced());
        });
    }};
}

fn home_name(state: &port::State) -> String {
    state
        .files
        .keys()
        .find(|n| n.as_str() == "d3-data-0000000100000001")
        .expect("allocator home")
        .clone()
}
fn region<'a>(s: &'a port::State, name: &str) -> &'a [u8] {
    match name {
        "d3-dwb-images:slot0" => &s.files["d3-dwb-images"].bytes[..16544],
        "home:page0" => &s.files[&home_name(s)].bytes[..PAGE_BYTES],
        _ => &s.files[name].bytes,
    }
}
fn fault_bytes(state: &State, id: &str, before: &Inventory) {
    let s = state.lock().unwrap();
    let mut expected = Vec::new();
    for row in FAULTS.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[0] != id {
            continue;
        }
        let bytes = literal(fields[2]);
        assert_eq!(bytes.len(), fields[3].parse::<usize>().unwrap());
        assert_eq!(region(&s, fields[1]), bytes, "{id} {}", fields[1]);
        expected.push(fields[1]);
    }
    assert!(expected.len() >= 6);
    assert_eq!(
        s.files.contains_key("d3-dwb-select-stage"),
        expected.contains(&"d3-dwb-select-stage")
    );
    let home = home_name(&s);
    for (name, entry) in &s.files {
        assert!(entry.sparse_tail.is_empty());
        if name == "d3-dwb-images" {
            assert!(entry.bytes[16544..].iter().all(|b| *b == 0));
        } else if name == &home {
            assert!(entry.bytes[PAGE_BYTES..].iter().all(|b| *b == 0));
        } else if !expected.contains(&name.as_str()) {
            assert_eq!(
                (entry.length, content_digest(&entry.bytes)),
                before[name],
                "untouched {name}"
            );
        }
    }
    for name in before.keys() {
        assert!(s.files.contains_key(name));
    }
}
fn success_bytes(state: &State, stem: &str, batch: u64, before: &Inventory) {
    let s = state.lock().unwrap();
    assert_eq!(
        region(&s, "d3-dwb-images:slot0"),
        literal(&format!("{stem}-slot.bin"))
    );
    assert_eq!(
        region(&s, "home:page0"),
        literal(&format!("{stem}-home.after"))
    );
    assert_eq!(
        region(&s, "d3-dwb-stage"),
        literal(&format!("{stem}-retirement.bank"))
    );
    let retired_bank = (3 * batch) % 2;
    assert_eq!(
        region(&s, &format!("d3-dwb-{retired_bank}")),
        literal(&format!("{stem}-retirement.bank"))
    );
    assert_eq!(
        region(&s, &format!("d3-dwb-{}", 1 - retired_bank)),
        literal(&format!("{stem}-pending.bank"))
    );
    assert_eq!(
        region(&s, "DWB"),
        literal(&format!("{stem}-retirement.selector"))
    );
    assert!(!s.files.contains_key("d3-dwb-select-stage"));
    let home = home_name(&s);
    for (name, entry) in &s.files {
        if name == "d3-dwb-images" {
            assert!(entry.bytes[16544..].iter().all(|b| *b == 0));
        } else if name == &home {
            assert!(entry.bytes[PAGE_BYTES..].iter().all(|b| *b == 0));
        } else if !["DWB", "d3-dwb-stage", "d3-dwb-0", "d3-dwb-1"].contains(&name.as_str()) {
            assert_eq!(
                (entry.length, content_digest(&entry.bytes)),
                before[name],
                "untouched {name}"
            );
        }
    }
}
fn selection_literal(generation: u64) -> crate::storage::control::SelectedControl {
    use crate::storage::control::{ControlKind, ControlRef, SelectedControl};
    let bytes = if generation == 1 {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/dev3-doublewrite-install/literal/profile1-DWB.bin"
        ))
        .unwrap()
    } else {
        literal(&format!(
            "p1-n1-reserve-{}.selector",
            ["issuance", "pending", "retirement"][(generation - 2) as usize]
        ))
    };
    let u64_at = |at| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(bytes[80], 4);
    SelectedControl {
        generation: u64_at(32),
        reference: ControlRef {
            kind: ControlKind::Doublewrite,
            slot: bytes[81],
            index: u32_at(84),
            generation: u64_at(88),
            encoded_len: u32_at(96),
            digest: bytes[112..144].try_into().unwrap(),
        },
    }
}
fn expected_phase(cut: usize) -> flush_io::Phase {
    use flush_io::{Phase as P, SelectionStep as S};
    match cut {
        10 => P::ImageWrite,
        11 => P::ImageSync,
        21 => P::HomeWrite,
        22 => P::HomeSync,
        _ => {
            let (index, local) = if cut <= 9 {
                (0, cut - 1)
            } else if cut <= 20 {
                (1, cut - 12)
            } else {
                (2, cut - 23)
            };
            P::Selection {
                index,
                step: [
                    S::StageWrite,
                    S::StageSync,
                    S::BankWrite,
                    S::BankSync,
                    S::SelectorCreate,
                    S::SelectorWrite,
                    S::SelectorSync,
                    S::Publish,
                    S::DirectorySync,
                ][local],
            }
        }
    }
}
fn check_clean(pool: &SharedPool<'_, '_, '_, '_>, lsn: u64, bytes: &[u8]) {
    let (actual, snap) = diagnose(pool);
    assert_eq!(&actual[..], bytes);
    assert!(!snap.dirty);
    assert!(snap.entry.is_none());
    assert_eq!(snap.frame.rec_lsn, None);
    assert_eq!(
        (snap.frame.page_lsn, snap.frame.modification_counter),
        (PageLsn(lsn), lsn)
    );
    let registry = frame_test::flush_diagnose(pool);
    assert!(registry.requirements[0].is_none());
    no_claim(pool);
}
fn normal_profile(profile: u32) {
    fixture!(profile, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flush_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        for (batch, operation) in [(1, "reserve"), (2, "cancel")] {
            if batch == 1 {
                ok(writer.reserve_pages(&mut owner, 1));
            } else {
                ok(writer.cancel_reserved(&mut owner));
            }
            let (_, before_frame) = diagnose(flusher.pool);
            assert_eq!(before_frame.frame.rec_lsn, Some(PageLsn(batch)));
            let before = inventory(&state);
            reset(&state);
            let result = done(ok(flusher.try_flush_one(&mut flush_owner)));
            assert!(result.finalized.cleaned);
            assert_eq!(result.io.last_issued, batch);
            assert_eq!((result.io.attempted, result.io.completed), (31, 31));
            assert_eq!(
                (
                    result.io.known.generation,
                    result.io.known.reference.generation
                ),
                (3 * batch + 1, 3 * batch + 1)
            );
            let stem = format!("p{profile}-n1-{operation}");
            success_bytes(&state, &stem, batch, &before);
            check_clean(flusher.pool, batch, &literal(&format!("{stem}-home.after")));
            assert_eq!(state.lock().unwrap().events.len(), 31);
        }
    });
}

fn physical_fault(id: &str, cut: usize, action: port::Action) {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut owner, 1));
        let before = inventory(&state);
        let original = diagnose(flusher.pool);
        reset(&state);
        state.lock().unwrap().plan = Some(port::Plan {
            at: cut - 1,
            action,
        });
        let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let error = err(flusher.try_flush_one(&mut owner));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        let inner = match error.cause {
            Cause::Io(e) => e,
            e => panic!("{e:?}"),
        };
        assert_eq!(inner.phase, expected_phase(cut));
        assert_eq!(inner.effect, Effect::CallerMustResolve);
        match inner.cause {
            flush_io::Cause::Io(e) => {
                assert_eq!(e.code, ErrorCode::IoWriteUnknown);
                assert_eq!(e.certainty(), Certainty::Unknown);
                assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
                assert_eq!(e.effect(), Effect::CallerMustResolve);
                assert_eq!(
                    e.context,
                    if WRITES.contains(&cut) {
                        "write outcome requires caller resolution"
                    } else {
                        "sync outcome unknown"
                    }
                );
            }
            flush_io::Cause::Namespace(e) => {
                assert_eq!(e.context, "fixed WAL namespace fault");
                assert_eq!(e.kind, xadb_foundation::namespace::NamespaceErrorKind::Io);
                assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
                let unknown =
                    matches!(action, port::Action::AfterError) || matches!(cut, 9 | 20 | 31);
                assert_eq!(
                    e.certainty(),
                    if unknown {
                        Certainty::Unknown
                    } else {
                        Certainty::Known
                    }
                );
                assert_eq!(
                    e.effect,
                    if unknown {
                        Effect::CallerMustResolve
                    } else {
                        Effect::Unchanged
                    }
                );
            }
            e => panic!("original lower fault missing: {e:?}"),
        }
        let snapshot = flusher.snapshot();
        assert!(snapshot.fenced);
        assert_eq!(
            (
                snapshot.io.attempted as usize,
                snapshot.io.completed as usize
            ),
            (cut, cut - 1)
        );
        let known_generation = if cut <= 9 {
            1
        } else if cut <= 20 {
            2
        } else {
            3
        };
        assert_eq!(snapshot.io.known.generation, known_generation);
        assert_eq!(snapshot.io.known, selection_literal(known_generation));
        assert_eq!(snapshot.io.last_issued, u64::from(cut > 9));
        assert_eq!(snapshot.io.homes_synced, cut > 22);
        assert!(!snapshot.io.retirement_selected);
        let selection_cut = matches!(cut, 8 | 9 | 19 | 20 | 30 | 31);
        assert_eq!(snapshot.io.possible[1].is_some(), selection_cut);
        assert_eq!(
            snapshot.io.possible,
            [
                Some(selection_literal(known_generation)),
                selection_cut.then(|| selection_literal(known_generation + 1))
            ]
        );
        assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
        assert_eq!(diagnose(flusher.pool).1.frame, original.1.frame);
        assert_eq!(&diagnose(flusher.pool).0[..], &original.0[..]);
        let s = state.lock().unwrap();
        assert_eq!((s.hit, s.events.len()), (1, cut));
        drop(s);
        fault_bytes(&state, id, &before);
        let after = inventory(&state);
        let counts = {
            let s = state.lock().unwrap();
            (s.events.len(), s.reads)
        };
        assert_eq!(
            err(flusher.continue_flush(&mut owner)).effect,
            Effect::CallerMustResolve
        );
        assert_eq!(inventory(&state), after);
        let s = state.lock().unwrap();
        assert_eq!((s.events.len(), s.reads), counts);
    });
}

#[test]
fn flush_fb_before_fault() {
    let mut count = 0;
    for cut in 1..=31 {
        let id = format!("FB{cut:02}");
        if selected(&id) {
            physical_fault(&id, cut, port::Action::BeforeError);
            passed(&id, &mut count);
        }
    }
    complete("FB", count);
}
#[test]
fn flush_fp_partial() {
    let mut count = 0;
    for (index, cut) in WRITES.into_iter().enumerate() {
        let id = format!("FP{:02}", index + 1);
        if selected(&id) {
            physical_fault(&id, cut, port::Action::PrefixError);
            passed(&id, &mut count);
        }
    }
    complete("FP", count);
}
#[test]
fn flush_fu_unknown() {
    let mut count = 0;
    for (index, cut) in [8, 9, 19, 20, 30, 31].into_iter().enumerate() {
        let id = format!("FU{:02}", index + 1);
        if selected(&id) {
            physical_fault(&id, cut, port::Action::AfterError);
            passed(&id, &mut count);
        }
    }
    complete("FU", count);
}

fn io_pause(state: &State, at: usize, after: bool) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (arrived, ack) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    state.lock().unwrap().pause = Some(port::Pause {
        at,
        after,
        entered: arrived,
        release,
        unwind: false,
    });
    (ack, resume)
}
fn flush_pause(
    pool: &SharedPool<'_, '_, '_, '_>,
    point: frame_test::FlushPoint,
) -> (mpsc::Receiver<()>, mpsc::SyncSender<frame_test::Resume>) {
    let (arrived, ack) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    frame_test::install_flush_pause(pool, point, arrived, release);
    (ack, resume)
}
fn frame_error(error: AllocatorFlushError, cause: dirty_frame::Cause, effect: Effect) {
    assert_eq!(error.effect, effect);
    match error.cause {
        Cause::Frame(e) => assert_eq!((e.cause, e.effect), (cause, effect)),
        e => panic!("{e:?}"),
    }
}
fn redirty_during_io() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut owner, 1));
        let before = inventory(&state);
        reset(&state);
        let (ack, resume) = io_pause(&state, 9, false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                done(ok(flusher.try_flush_one(&mut o)))
            });
            ack.recv_timeout(STEP).unwrap();
            let claim = frame_test::flush_diagnose(writer.pool).claim.unwrap();
            assert_eq!(claim.captured.unwrap().page_lsn, PageLsn(1));
            assert!(!core.diagnostic_version_busy());
            ok(writer.cancel_reserved(&mut owner));
            let (_, live) = diagnose(writer.pool);
            assert_eq!(
                (live.frame.page_lsn, live.frame.rec_lsn),
                (PageLsn(2), Some(PageLsn(1)))
            );
            resume.send(()).unwrap();
            let observed = worker.join().unwrap();
            assert!(!observed.finalized.cleaned);
        });
        // WAL changed through the actual concurrent writer; all other non-flush files stay fixed.
        let mut before = before;
        before.insert(
            crate::storage::wal_io::WAL_NAME.into(),
            inventory(&state)[crate::storage::wal_io::WAL_NAME],
        );
        success_bytes(&state, "p1-n1-reserve", 1, &before);
        let (_, live) = diagnose(flusher.pool);
        assert!(live.dirty);
        assert_eq!(live.frame.rec_lsn, Some(PageLsn(1)));
        no_claim(flusher.pool);
        let before = inventory(&state);
        reset(&state);
        let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        assert!(
            done(ok(flusher.try_flush_one(&mut owner)))
                .finalized
                .cleaned
        );
        success_bytes(&state, "p1-n1-cancel", 2, &before);
        check_clean(flusher.pool, 2, &literal("p1-n1-cancel-home.after"));
    });
}
fn retired_busy() {
    fixture!(1, |bound, core, state| {
        let identity = bound.writer.identity;
        let (mut writer, reader, mut flusher) = bound.split_with_flush();
        let mut writer_owner = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut writer_owner, 1));
        reset(&state);
        let (ack, resume) = io_pause(&state, 30, true);
        let mut reader_owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let held = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                ok(flusher.try_flush_one(&mut o))
            });
            ack.recv_timeout(STEP).unwrap();
            let held = ok(reader.read(&mut reader_owner, &request(identity)));
            resume.send(()).unwrap();
            assert_eq!(
                worker.join().unwrap(),
                FlushProgress::RetiredPendingFinalize
            );
            held
        });
        let snapshot = flusher.snapshot();
        assert_eq!(snapshot.phase, Phase::RetiredPendingFinalize);
        assert!(snapshot.io.retirement_selected);
        let claim = frame_test::flush_diagnose(flusher.pool);
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let before = inventory(&state);
        reset(&state);
        let error = err(flusher.try_flush_one(&mut o));
        assert!(matches!(error.cause, Cause::Busy));
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(
            ok(flusher.continue_flush(&mut o)),
            FlushProgress::RetiredPendingFinalize
        );
        same_registry(frame_test::flush_diagnose(flusher.pool), &claim);
        no_io(&state);
        held.release_checked().unwrap();
        assert!(done(ok(flusher.continue_flush(&mut o))).finalized.cleaned);
        no_io(&state);
        assert_eq!(inventory(&state), before);
        check_clean(flusher.pool, 1, &literal("p1-n1-reserve-home.after"));
    });
}
fn no_dirty() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let before = inventory(&state);
        let snapshot = flusher.snapshot();
        assert_eq!(
            ok(flusher.try_flush_one(&mut owner)),
            FlushProgress::NoDirty
        );
        no_claim(flusher.pool);
        no_io(&state);
        assert_eq!(flusher.snapshot().io, snapshot.io);
        assert_eq!(inventory(&state), before);
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        done(ok(flusher.try_flush_one(&mut owner)));
        let before = inventory(&state);
        let snapshot = flusher.snapshot();
        reset(&state);
        assert_eq!(
            ok(flusher.try_flush_one(&mut owner)),
            FlushProgress::NoDirty
        );
        no_claim(flusher.pool);
        no_io(&state);
        assert_eq!(flusher.snapshot().io, snapshot.io);
        assert_eq!(inventory(&state), before);
    });
}
#[test]
fn flush_fn_normal() {
    let mut count = 0;
    for profile in 1..=3 {
        let id = format!("FN{profile:02}");
        if selected(&id) {
            normal_profile(profile);
            passed(&id, &mut count);
        }
    }
    for (id, run) in [
        ("FN04", redirty_during_io as fn()),
        ("FN05", retired_busy),
        ("FN06", no_dirty),
    ] {
        if selected(id) {
            run();
            passed(id, &mut count);
        }
    }
    complete("FN", count);
}

fn awaiting_wal() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let (ack, resume) = io_pause(&state, 1, false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                ok(writer.reserve_pages(&mut o, 1))
            });
            ack.recv_timeout(STEP).unwrap();
            let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            assert_eq!(ok(flusher.try_flush_one(&mut o)), FlushProgress::WaitingWal);
            let claim = frame_test::flush_diagnose(flusher.pool);
            let snapshot = flusher.snapshot();
            let n = state.lock().unwrap().events.len();
            let error = err(flusher.try_flush_one(&mut o));
            assert!(matches!(error.cause, Cause::Busy));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(
                ok(flusher.continue_flush(&mut o)),
                FlushProgress::WaitingWal
            );
            assert_eq!(flusher.snapshot(), snapshot);
            same_registry(frame_test::flush_diagnose(flusher.pool), &claim);
            assert_eq!(state.lock().unwrap().events.len(), n);
            resume.send(()).unwrap();
            worker.join().unwrap();
            reset(&state);
            done(ok(flusher.continue_flush(&mut o)));
            assert_eq!(state.lock().unwrap().events.len(), 31);
            check_clean(flusher.pool, 1, &literal("p1-n1-reserve-home.after"));
        });
    });
}
fn retained_fence() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let (ack, resume) = io_pause(&state, 1, false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                writer.reserve_pages(&mut owner, 1)
            });
            ack.recv_timeout(STEP).unwrap();
            let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            assert_eq!(
                ok(flusher.try_flush_one(&mut owner)),
                FlushProgress::WaitingWal
            );
            let claim = frame_test::flush_diagnose(flusher.pool);
            let snapshot = flusher.snapshot();
            let before = inventory(&state);
            let counts = {
                let s = state.lock().unwrap();
                (s.events.len(), s.reads)
            };
            flusher.storage.session_fence().fence();
            assert_eq!(
                err(flusher.continue_flush(&mut owner)).effect,
                Effect::CallerMustResolve
            );
            same_registry(frame_test::flush_diagnose(flusher.pool), &claim);
            assert_eq!(flusher.snapshot().phase, snapshot.phase);
            assert_eq!(flusher.snapshot().io, snapshot.io);
            assert_eq!(inventory(&state), before);
            {
                let s = state.lock().unwrap();
                assert_eq!((s.events.len(), s.reads), counts);
            }
            resume.send(()).unwrap();
            assert_eq!(
                err(worker.join().unwrap()).effect,
                Effect::CallerMustResolve
            );
        });
    });
}
fn busy_capture() {
    fixture!(1, |bound, core, state| {
        let identity = bound.writer.identity;
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let held = ok(ok(writer.pool.reserve_targets(&mut w, &[request(identity)])).try_write());
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                err(flusher.try_flush_one(&mut o))
            });
            frame_error(
                worker.join().unwrap(),
                dirty_frame::Cause::Busy,
                Effect::Unchanged,
            );
        });
        no_io(&state);
        assert!(frame_test::flush_diagnose(flusher.pool).claim.is_none());
        assert_eq!(flusher.snapshot().io.last_issued, 0);
        held.release_checked().unwrap();
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        done(ok(flusher.try_flush_one(&mut o)));
    });
}
fn admission_simple(id: &str) {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let before = inventory(&state);
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        match id {
            "FA03" => frame_error(
                err(flusher.try_flush_one(&mut w)),
                dirty_frame::Cause::Concurrency(
                    crate::transaction::concurrency::Cause::InvalidRole,
                ),
                Effect::Unchanged,
            ),
            "FA05" | "FA06" | "FA07" => {
                io_test::seed_headroom(
                    &mut flusher.control.io,
                    if id == "FA05" { Some(u64::MAX) } else { None },
                    if id == "FA06" {
                        Some(u64::MAX - 2)
                    } else {
                        None
                    },
                    if id == "FA07" {
                        Some(u64::MAX - 2)
                    } else {
                        None
                    },
                );
                let initial = flusher.snapshot();
                let error = err(flusher.try_flush_one(&mut o));
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(matches!(
                    error.cause,
                    Cause::Io(flush_io::FlushIoError {
                        cause: flush_io::Cause::Capacity,
                        phase: flush_io::Phase::Preflight,
                        ..
                    })
                ));
                assert_eq!(flusher.snapshot(), initial);
                assert!(!initial.fenced);
            }
            "FA08" => {
                frame_test::seed_last_attempt(flusher.pool, u64::MAX);
                frame_error(
                    err(flusher.try_flush_one(&mut o)),
                    dirty_frame::Cause::Capacity,
                    Effect::Unchanged,
                );
                assert_eq!(
                    frame_test::flush_diagnose(flusher.pool).last_attempt,
                    u64::MAX
                );
            }
            "FA09" => {
                flusher.storage.session_fence().fence();
                assert_eq!(
                    err(flusher.try_flush_one(&mut o)).effect,
                    Effect::CallerMustResolve
                );
                assert_eq!(
                    err(flusher.continue_flush(&mut o)).effect,
                    Effect::CallerMustResolve
                );
            }
            "FA12" => {
                let initial = flusher.snapshot();
                let error = err(flusher.continue_flush(&mut o));
                assert!(matches!(error.cause, Cause::NoPending));
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(initial, flusher.snapshot());
                no_io(&state);
                done(ok(flusher.try_flush_one(&mut o)));
                reset(&state);
                let initial = flusher.snapshot();
                let error = err(flusher.continue_flush(&mut o));
                assert!(matches!(error.cause, Cause::NoPending));
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(initial, flusher.snapshot());
                no_io(&state);
            }
            _ => unreachable!(),
        }
        no_io(&state);
        no_claim(flusher.pool);
        if id != "FA12" {
            assert_eq!(inventory(&state), before);
        }
    });
}
fn foreign_owner() {
    storage_fixture!(1, |storage, state| {
        // Both actual cores outlive the bound pool; only their pointer differs.
        let foreign = PageConcurrency::new(storage.session_fence()).unwrap();
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut page = boxed::<PAGE_BYTES>();
        let mut work = boxed::<98304>();
        let mut frames = [DirtyFrame::new(&mut page)];
        let mut bound = ok(storage.bind_allocator(&core, &mut frames, &mut work));
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut writer_owner = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut writer_owner, 1));
        reset(&state);
        let mut other = foreign.try_owner(OwnerRole::Foreground).unwrap();
        std::mem::forget(ok(other.try_apply()));
        let before = inventory(&state);
        let original = flusher.snapshot();
        frame_error(
            err(flusher.try_flush_one(&mut other)),
            dirty_frame::Cause::BindingMismatch,
            Effect::Unchanged,
        );
        assert_eq!(flusher.snapshot(), original);
        assert!(!storage.session_fence().is_fenced());
        no_io(&state);
        no_claim(flusher.pool);
        assert_eq!(inventory(&state), before);
        drop(other);
        assert!(storage.session_fence().is_fenced());
    });
}
fn corrupt_stored_selection() {
    let mut count = 0;
    for possible in [false, true] {
        fixture!(1, |bound, core, state| {
            let (_, _, mut flusher) = bound.split_with_flush();
            io_test::corrupt_selection(&mut flusher.control.io, possible);
            let damaged = flusher.snapshot().io;
            let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let error = err(flusher.try_flush_one(&mut owner));
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(
                error.cause,
                Cause::Io(flush_io::FlushIoError {
                    cause: flush_io::Cause::Invariant,
                    ..
                })
            ));
            let failed = flusher.snapshot().io;
            assert_eq!(failed.known, damaged.known);
            assert_eq!(failed.possible, damaged.possible);
            assert_eq!(failed.last_issued, damaged.last_issued);
            assert!(flusher.snapshot().fenced);
            no_io(&state);
            no_claim(flusher.pool);
        });
        println!(
            "ADMISSION_VARIANT FA05 {}",
            if possible {
                "possible_selection"
            } else {
                "stored_selector"
            }
        );
        count += 1;
    }
    assert_eq!(count, 2);
}
fn fence_after_selection() {
    let mut count = 0;
    for retirement in [false, true] {
        fixture!(1, |bound, core, state| {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let mut writer_owner = core.try_owner(OwnerRole::Foreground).unwrap();
            ok(writer.reserve_pages(&mut writer_owner, 1));
            reset(&state);
            let storage = writer.storage;
            let (ack, resume) = io_pause(&state, if retirement { 30 } else { 8 }, true);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    err(flusher.try_flush_one(&mut owner))
                });
                ack.recv_timeout(STEP).unwrap();
                storage.session_fence().fence();
                resume.send(()).unwrap();
                let error = worker.join().unwrap();
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(matches!(
                    error.cause,
                    Cause::Io(flush_io::FlushIoError {
                        cause: flush_io::Cause::Fenced,
                        ..
                    })
                ));
            });
            let io = flusher.snapshot().io;
            assert_eq!(io.known, selection_literal(if retirement { 4 } else { 2 }));
            assert_eq!(io.possible, [Some(io.known), None]);
            assert_eq!(io.last_issued, 1);
            assert_eq!(io.retirement_selected, retirement);
            assert_eq!(io.homes_synced, retirement);
            assert_eq!(
                (io.attempted, io.completed),
                if retirement { (31, 31) } else { (9, 9) }
            );
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
            let s = state.lock().unwrap();
            assert_eq!(s.events.len(), if retirement { 31 } else { 9 });
            if !retirement {
                assert!(region(&s, "d3-dwb-images:slot0").iter().all(|b| *b == 0));
                assert_eq!(
                    region(&s, "home:page0"),
                    literal("p1-n1-reserve-home.before")
                );
            } else {
                assert_eq!(
                    region(&s, "home:page0"),
                    literal("p1-n1-reserve-home.after")
                );
                assert_eq!(
                    region(&s, "DWB"),
                    literal("p1-n1-reserve-retirement.selector")
                );
            }
        });
        println!(
            "ADMISSION_VARIANT FA09 {}",
            if retirement {
                "retirement_after_dirsync"
            } else {
                "issuance_after_dirsync"
            }
        );
        count += 1;
    }
    assert_eq!(count, 2);
}
fn clean_headroom() {
    fixture!(1, |bound, core, state| {
        let (_, _, mut flusher) = bound.split_with_flush();
        io_test::seed_headroom(&mut flusher.control.io, None, Some(u64::MAX - 2), None);
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let error = err(flusher.try_flush_one(&mut o));
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(matches!(
            error.cause,
            Cause::Io(flush_io::FlushIoError {
                cause: flush_io::Cause::Capacity,
                ..
            })
        ));
        no_io(&state);
        no_claim(flusher.pool);
    });
}
fn corrupt_capture(variant: u8) {
    fixture!(1, |bound, core, state| {
        let identity = bound.writer.identity;
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let capture = ok(flusher.pool.capture_flush(
            &mut o,
            request(identity),
            ok(flusher.control.io.image_mut()),
        ))
        .unwrap();
        flusher.control.capture = Some(capture);
        flusher.control.phase = Phase::Captured;
        if variant == 0 {
            frame_test::stale_capture_generation(flusher.control.capture.as_mut().unwrap());
        } else {
            frame_test::corrupt_flush_claim(flusher.pool, variant);
        }
        let damaged = frame_test::flush_diagnose(flusher.pool);
        frame_error(
            err(flusher.advance(&mut o)),
            dirty_frame::Cause::Invariant,
            Effect::CallerMustResolve,
        );
        same_registry(frame_test::flush_diagnose(flusher.pool), &damaged);
        assert!(flusher.snapshot().fenced);
        no_io(&state);
        if variant > 0 {
            println!("FLUSH_VARIANT\tFA13\tV{variant:02}");
        }
    });
}
fn foreign_requirement() {
    fixture!(1, |foreign_bound, foreign_core, foreign_state| {
        let (mut foreign_writer, _) = foreign_bound.split();
        let mut w = foreign_core.try_owner(OwnerRole::Foreground).unwrap();
        ok(foreign_writer.reserve_pages(&mut w, 1));
        let genuine = frame_test::actual_requirement(foreign_writer.pool, 0).unwrap();
        fixture!(1, |bound, core, state| {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
            ok(writer.reserve_pages(&mut w, 1));
            reset(&state);
            frame_test::replace_requirement(flusher.pool, 0, genuine);
            let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            frame_error(
                err(flusher.try_flush_one(&mut o)),
                dirty_frame::Cause::Invariant,
                Effect::CallerMustResolve,
            );
            assert!(flusher.snapshot().fenced);
            no_io(&state);
        });
        assert_eq!(foreign_state.lock().unwrap().hit, 0);
    });
}
#[test]
fn flush_fa_admission() {
    let mut count = 0;
    for number in 1..=13 {
        let id = format!("FA{number:02}");
        if !selected(&id) {
            continue;
        }
        match number {
            1 => awaiting_wal(),
            2 => busy_capture(),
            4 => foreign_owner(),
            5 => {
                admission_simple(&id);
                corrupt_stored_selection();
            }
            6 => {
                admission_simple(&id);
                clean_headroom();
            }
            9 => {
                admission_simple(&id);
                retained_fence();
                fence_after_selection();
            }
            10 => foreign_requirement(),
            11 => corrupt_capture(0),
            13 => {
                for variant in 1..=6 {
                    corrupt_capture(variant);
                }
            }
            _ => admission_simple(&id),
        }
        passed(&id, &mut count);
    }
    complete("FA", count);
}

fn poisoned_capture(dpt: bool) {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let before = inventory(&state);
        if dpt {
            frame_test::poison_dpt(flusher.pool);
        } else {
            frame_test::poison_content(flusher.pool, 0);
        }
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        frame_error(
            err(flusher.try_flush_one(&mut o)),
            dirty_frame::Cause::Poisoned,
            Effect::CallerMustResolve,
        );
        assert!(flusher.snapshot().fenced);
        no_io(&state);
        assert_eq!(inventory(&state), before);
        assert!(diagnose(flusher.pool).1.dirty);
    });
}
fn busy_cleanup_fatal() {
    fixture!(1, |bound, core, state| {
        let identity = bound.writer.identity;
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let pool = writer.pool;
        let held = ok(ok(pool.reserve_targets(&mut w, &[request(identity)])).try_write());
        let (arrived, ack) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        frame_test::install_release_pause(pool, arrived, release);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                err(flusher.try_flush_one(&mut o))
            });
            ack.recv_timeout(STEP).unwrap();
            frame_test::seed_pins(pool, 0, 3, 2);
            resume.send(frame_test::Resume::Continue).unwrap();
            frame_error(
                worker.join().unwrap(),
                dirty_frame::Cause::Invariant,
                Effect::CallerMustResolve,
            );
            no_io(&state);
        });
        assert!(flusher.snapshot().fenced);
        // The original damage remains; no diagnostic repair is performed.
        let _ = held.release_checked();
        let snapshot = diagnose(pool).1;
        assert_eq!((snapshot.pins[0], snapshot.total), (3, 2));
    });
}
fn poison_after_retirement() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let pool = writer.pool;
        let (ack, resume) = io_pause(&state, 30, true);
        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        std::thread::scope(|scope| {
            let injector = scope.spawn(move || {
                ack.recv_timeout(STEP).unwrap();
                frame_test::poison_dpt(pool);
                resume.send(()).unwrap();
            });
            frame_error(
                err(flusher.try_flush_one(&mut o)),
                dirty_frame::Cause::Poisoned,
                Effect::CallerMustResolve,
            );
            injector.join().unwrap();
        });
        let snapshot = flusher.snapshot();
        assert!(snapshot.io.retirement_selected && snapshot.io.homes_synced && snapshot.fenced);
        assert_eq!((snapshot.io.attempted, snapshot.io.completed), (31, 31));
        assert!(frame_test::flush_diagnose(pool).claim.is_some());
        assert!(diagnose(pool).1.dirty);
        let before = inventory(&state);
        reset(&state);
        assert_eq!(
            err(flusher.continue_flush(&mut o)).effect,
            Effect::CallerMustResolve
        );
        no_io(&state);
        assert_eq!(inventory(&state), before);
    });
}
fn unwind_at(point: frame_test::FlushPoint) {
    fixture!(1, |bound, core, state| {
        let counts = {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
            ok(writer.reserve_pages(&mut w, 1));
            reset(&state);
            let pool = writer.pool;
            let (ack, resume) = flush_pause(pool, point);
            let (arrived, fenced) = mpsc::sync_channel(1);
            let (release, receiver) = mpsc::sync_channel(1);
            frame_test::install_fence_pause(pool, arrived, receiver);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    catch_unwind(AssertUnwindSafe(|| {
                        let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                        let _ = flusher.try_flush_one(&mut o);
                    }))
                });
                ack.recv_timeout(STEP).unwrap();
                assert!(frame_test::flush_diagnose(pool).claim.is_some());
                resume.send(frame_test::Resume::Panic).unwrap();
                fenced.recv_timeout(STEP).unwrap();
                assert!(writer.storage.session_fence().is_fenced());
                assert!(frame_test::content_busy(pool, 0));
                assert_eq!(
                    state.lock().unwrap().events.len(),
                    if point == frame_test::FlushPoint::Copied {
                        0
                    } else {
                        31
                    }
                );
                release.send(frame_test::Resume::Continue).unwrap();
                assert!(worker.join().unwrap().is_err());
            });
            let snapshot = diagnose(pool).1;
            assert_eq!(snapshot.dirty, point == frame_test::FlushPoint::Copied);
            assert!(frame_test::flush_diagnose(pool).claim.is_some());
            if point == frame_test::FlushPoint::Finalized {
                assert!(flusher.snapshot().io.retirement_selected);
                assert!(snapshot.entry.is_none());
            }
            let s = state.lock().unwrap();
            (s.events.len(), s.reads)
        };
        let s = state.lock().unwrap();
        assert_eq!((s.events.len(), s.reads), counts);
    });
}
#[test]
fn flush_fc_cleanup() {
    let mut count = 0;
    for n in 1..=6 {
        let id = format!("FC{n:02}");
        if !selected(&id) {
            continue;
        }
        match n {
            1 => poisoned_capture(false),
            2 => poisoned_capture(true),
            3 => busy_cleanup_fatal(),
            4 => poison_after_retirement(),
            5 => unwind_at(frame_test::FlushPoint::Copied),
            6 => unwind_at(frame_test::FlushPoint::Finalized),
            _ => unreachable!(),
        }
        passed(&id, &mut count);
    }
    complete("FC", count);
}

fn check_witness(fields: &[&str]) -> std::result::Result<(), &'static str> {
    let events: Vec<_> = fields[2].split(',').collect();
    let at = |name| events.iter().position(|e| *e == name);
    let before = |a, b| matches!((at(a), at(b)), (Some(a), Some(b)) if a < b);
    let number = |i: usize| fields[i].parse::<u64>().unwrap();
    if !before("claim", "capture") {
        return Err("N01");
    }
    if number(3) != number(4) {
        return Err("N02");
    }
    if number(5) != 1 {
        return Err("N03");
    }
    if !before("wal_durable", "image_write") {
        return Err("N04");
    }
    if at("image_sync").is_none() || !before("image_sync", "pending_durable") {
        return Err("N05");
    }
    if !before("issuance_durable", "image_write") {
        return Err("N06");
    }
    if !before("pending_durable", "home_write") {
        return Err("N07");
    }
    if !before("home_write", "home_sync") || !before("home_sync", "retirement_durable") {
        return Err("N08");
    }
    if number(6) != number(7) {
        return Err("N09");
    }
    if !before("retirement_durable", "slot_reuse") {
        return Err("N10");
    }
    if !before("finalize", "claim_return") {
        return Err("N11");
    }
    if number(11) == 1 && number(12) != 1 {
        return Err("N12");
    }
    if number(10) != number(9) || number(10) < number(8) {
        return Err("N13");
    }
    if let Some(busy) = at("finalize_busy")
        && events.iter().skip(busy + 1).any(|event| {
            matches!(
                *event,
                "wal_io"
                    | "image_write"
                    | "image_sync"
                    | "home_write"
                    | "home_sync"
                    | "namespace_read"
                    | "file_read"
                    | "selector_publish"
            )
        })
    {
        return Err("N14");
    }
    if number(13) != 1 {
        return Err("N15");
    }
    if number(14) != 1 {
        return Err("N16");
    }
    if !before("retirement_durable", "finalize") {
        return Err("N17");
    }
    if !before("image_write", "image_sync") {
        return Err("N18");
    }
    if !before("capture", "image_write") {
        return Err("N19");
    }
    Ok(())
}
fn negative_oracle() {
    let rows = include_str!("../../../../../tests/fixtures/dev3-allocator-flush-one/known-bad.tsv");
    let mut rejected = 0;
    for row in rows.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[0] == "VALID" {
            assert_eq!(check_witness(&fields), Ok(()));
        } else {
            assert_eq!(check_witness(&fields), Err(fields[1]));
            rejected += 1;
        }
    }
    assert_eq!(rejected, 19);
    // Positive events below are collected at actual claim/copy/finalize cuts and actual file effects.
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
        ok(writer.reserve_pages(&mut w, 1));
        reset(&state);
        let pool = writer.pool;
        let (ack, resume) = flush_pause(pool, frame_test::FlushPoint::Copied);
        let mut events = Vec::new();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                done(ok(flusher.try_flush_one(&mut o)))
            });
            ack.recv_timeout(STEP).unwrap();
            let claim = frame_test::flush_diagnose(pool).claim.unwrap();
            assert_eq!(claim.captured.unwrap().page_lsn, PageLsn(1));
            events.extend(["claim", "capture"]);
            assert!(state.lock().unwrap().events.is_empty());
            let prefix = writer.storage.wal().append.lock().unwrap().prefix;
            assert_eq!(prefix.synced_lsn, 1);
            events.push("wal_durable");
            let (final_ack, final_resume) = flush_pause(pool, frame_test::FlushPoint::Finalized);
            resume.send(frame_test::Resume::Continue).unwrap();
            final_ack.recv_timeout(STEP).unwrap();
            let s = state.lock().unwrap();
            assert_eq!(s.events.len(), 31);
            for (index, label, kind) in [
                (8, "issuance_durable", "dirsync"),
                (9, "image_write", "write"),
                (10, "image_sync", "sync"),
                (19, "pending_durable", "dirsync"),
                (20, "home_write", "write"),
                (21, "home_sync", "sync"),
                (30, "retirement_durable", "dirsync"),
            ] {
                assert_eq!(s.events[index].kind, kind);
                events.push(label);
            }
            drop(s);
            events.push("finalize");
            assert!(frame_test::flush_diagnose(pool).claim.is_some());
            final_resume.send(frame_test::Resume::Continue).unwrap();
            let observed = worker.join().unwrap();
            assert!(observed.finalized.cleaned);
            no_claim(pool);
            events.extend(["claim_return", "slot_reuse"]);
        });
        let joined = events.join(",");
        let actual = [
            "ACTUAL",
            "ACCEPT",
            joined.as_str(),
            "1",
            "1",
            "1",
            "1",
            "1",
            "1",
            "1",
            "1",
            "0",
            "0",
            "1",
            "1",
        ];
        assert_eq!(check_witness(&actual), Ok(()));
    });
    println!("FLUSH_ORACLE_NEGATIVES 19");
}
fn resources() {
    use std::mem::size_of;
    let expected_caps = [
        ("Prepared", 768),
        ("PreparedResult", 768),
        ("IoObservation", 256),
        ("Selected3", 192),
        ("Selected3Result", 256),
        ("FrameStamp", 112),
        ("TargetRequest", 80),
        ("WalFrameLocation", 96),
        ("Profile", 96),
        ("FileMapRoot", 192),
        ("FileMapRootResult", 192),
        ("FileMapRootInner", 192),
        ("FileMapCursor", 224),
        ("FileMapChunk", 112),
        ("FileMapChunkResult", 112),
        ("FileMapChunkInner", 112),
        ("FileMapEntry", 96),
        ("ChunkLocation", 80),
        ("PageHeader", 96),
        ("PageView", 128),
        ("AllocationPage", 192),
        ("ControlView", 96),
        ("PendingImage", 96),
        ("ContentHasher", 128),
        ("FileName", 160),
        ("FileMetadata", 32),
        ("Cause", 96),
        ("UnitResult", 96),
        ("FlushIo", 2048),
        ("RetiredBatch", 256),
        ("FlushIoError", 128),
    ];
    let layouts = io_test::layout();
    let mut failures = Vec::new();
    if layouts.len() != expected_caps.len() {
        failures.push(format!(
            "layout count {} != {}",
            layouts.len(),
            expected_caps.len()
        ));
    }
    for (index, (name, value, reported_cap)) in layouts.into_iter().enumerate() {
        println!("RESOURCE_SIZE {name} {value} {reported_cap}");
        if let Some(&(expected_name, ceiling)) = expected_caps.get(index) {
            if (name, reported_cap) != (expected_name, ceiling) {
                failures.push(format!(
                    "layout {index}: {name}/{reported_cap} != {expected_name}/{ceiling}"
                ));
            }
            if value > ceiling {
                failures.push(format!("{name}={value}>{ceiling}"));
            }
        } else {
            failures.push(format!("unexpected layout {name}"));
        }
    }
    let unused = (4 << 20) - flush_io::WORKSPACE_USED;
    if flush_io::WORKSPACE_USED != 164000 || unused != 4030304 {
        failures.push(format!(
            "workspace used={} unused={unused}",
            flush_io::WORKSPACE_USED
        ));
    }
    let registry = dirty_frame::flush_registry_bytes();
    let control = size_of::<FlushControl<'_, '_, '_>>();
    let retained = registry + control + 1024;
    let expected = (
        retained + 34 * 1024 + 8192 + 4096 + 8192,
        retained + (5 * registry + 8192).max(3 * registry + 8 * control + 8192),
    );
    let reported = flush_metadata_peaks();
    if reported != expected || expected.0 > 65536 || expected.1 > 65536 {
        failures.push(format!(
            "metadata reported={reported:?} expected={expected:?} cap=65536"
        ));
    }
    let deep = size_of::<MutableStorageOwner<'_, '_, port::Writer>>()
        + size_of::<WriterControl<'_, '_>>()
        + control
        + 4 * size_of::<port::File>()
        + 4096
        + 8192
        + 8192;
    if deep > 65536 {
        failures.push(format!("deep={deep}>65536"));
    }
    println!("RESOURCE_LEDGER R={registry} C={control} T={retained} deep={deep} deep_cap=65536");
    if !fits::<port::Writer>() {
        failures.push("concrete port::Writer admission rejected".into());
    }
    println!(
        "RESOURCE_FLUSH hot={} cold={} metadata=262144 global=2490368 remaining=5898240 workspace=4194304 used=164000 physical_stack=NOT_MEASURED",
        expected.0, expected.1
    );
    assert!(failures.is_empty(), "resource violations: {failures:#?}");
}
fn rejected_resource() {
    fixture!(1, |bound, _core, state| {
        let before = inventory(&state);
        let original = bound.flush.io.snapshot();
        assert!(!flush_io::fits::<port::SizedWriter<0, 8192>>());
        assert!(!flush_io::fits::<port::SizedWriter<65536, 0>>());
        for wide_file in [true, false] {
            let error = if wide_file {
                err(bound.flush.io.preflight::<port::SizedWriter<0, 8192>>())
            } else {
                err(bound.flush.io.preflight::<port::SizedWriter<65536, 0>>())
            };
            assert!(matches!(error.cause, flush_io::Cause::Capacity));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(bound.flush.io.snapshot(), original);
        }
        no_claim(&bound.pool);
        no_io(&state);
        assert_eq!(inventory(&state), before);
    });
    const AT: usize = 8192 - std::mem::size_of::<port::File>();
    const ALIGN: usize = std::mem::align_of::<port::File>();
    assert_eq!(
        std::mem::size_of::<port::SizedFile<{ AT - ALIGN }>>(),
        8192 - ALIGN
    );
    assert_eq!(std::mem::size_of::<port::SizedFile<AT>>(), 8192);
    assert_eq!(
        std::mem::size_of::<port::SizedFile<{ AT + ALIGN }>>(),
        8192 + ALIGN
    );
    bind_resource::<0, { AT - ALIGN }>(true);
    bind_resource::<0, AT>(true);
    bind_resource::<0, { AT + ALIGN }>(false);
    // The file is admitted in isolation; the combined owner path is not.
    bind_resource::<4000, AT>(false);
}
fn bind_resource<const OWNER: usize, const HANDLE: usize>(accepted: bool) {
    let mut slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>();
    let mut page = boxed::<PAGE_BYTES>();
    let original_page = *page;
    let mut operation = boxed::<98304>();
    let (inner, state) = port::fresh();
    let writer = port::SizedWriter::<OWNER, HANDLE> {
        inner,
        padding: [0; OWNER],
    };
    let fresh =
        FreshStorageSession::create(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    reset(&state);
    let before = inventory(&state);
    let arena_before = content_digest(&storage.workspace.lock().unwrap().as_ref().unwrap()[..]);
    let binding_before = storage.binding.load(std::sync::atomic::Ordering::Acquire);
    {
        let mut frames = [DirtyFrame::new(&mut page)];
        let result = storage.bind_allocator(&core, &mut frames, &mut operation);
        if accepted {
            let bound = ok(result);
            assert!(!storage.session_fence().is_fenced());
            drop(bound);
        } else {
            let error = err(result);
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(error.cause, super::super::Cause::Capacity));
            assert!(!storage.session_fence().is_fenced());
            no_io(&state);
        }
    }
    if !accepted {
        assert_eq!(
            content_digest(&storage.workspace.lock().unwrap().as_ref().unwrap()[..]),
            arena_before
        );
        assert_eq!(
            storage.binding.load(std::sync::atomic::Ordering::Acquire),
            binding_before
        );
        assert_eq!(&page[..], &original_page[..]);
        assert!(operation.iter().all(|b| *b == 0));
    }
    assert_eq!(inventory(&state), before);
    println!(
        "RESOURCE_VARIANT owner={OWNER} file={} accepted={accepted}",
        std::mem::size_of::<port::SizedFile<HANDLE>>()
    );
}
#[test]
fn flush_fr_resources() {
    let mut count = 0;
    for (id, run) in [
        ("FR01", resources as fn()),
        ("FR02", rejected_resource),
        ("FR03", negative_oracle),
    ] {
        if selected(id) {
            run();
            passed(id, &mut count);
        }
    }
    complete("FR", count);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn expanded_bytes(state: &State, id: &str, before: &Inventory) {
    let rows =
        include_str!("../../../../../tests/fixtures/dev3-allocator-flush-one/expanded-files.tsv");
    let s = state.lock().unwrap();
    let mut names = Vec::new();
    for row in rows.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[0] != id {
            continue;
        }
        assert_eq!(
            hex(&content_digest(region(&s, fields[1]))),
            fields[2],
            "{id} {}",
            fields[1]
        );
        names.push(fields[1]);
    }
    assert!(names.len() >= 6);
    assert_eq!(
        s.files.contains_key("d3-dwb-select-stage"),
        names.contains(&"d3-dwb-select-stage")
    );
    let home = home_name(&s);
    for (name, file) in &s.files {
        if name == "d3-dwb-images" {
            assert!(file.bytes[16544..].iter().all(|b| *b == 0));
        } else if name == &home {
            assert!(file.bytes[PAGE_BYTES..].iter().all(|b| *b == 0));
        } else if !names.contains(&name.as_str()) {
            assert_eq!((file.length, content_digest(&file.bytes)), before[name]);
        }
    }
}
#[test]
#[ignore = "DEV3 S5 expanded flush inventory; separate from mandatory76"]
fn flush_s5_batch_one_expanded() {
    let mut count = 0;
    let selection = std::env::var("XADB_FLUSH_CASES").ok();
    if let Some(selection) = &selection {
        for id in selection.split(',') {
            assert_eq!(
                EXPANDED
                    .lines()
                    .skip(1)
                    .filter(|row| row.split('\t').next() == Some(id))
                    .count(),
                1,
                "unknown S5 ID"
            );
        }
    }
    let expected_count = EXPANDED
        .lines()
        .skip(1)
        .filter(|row| {
            selection.as_ref().is_none_or(|value| {
                value
                    .split(',')
                    .any(|id| Some(id) == row.split('\t').next())
            })
        })
        .count();
    assert!(expected_count > 0);
    for row in EXPANDED.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        let id = fields[0];
        if selection
            .as_ref()
            .is_some_and(|value| !value.split(',').any(|chosen| chosen == id))
        {
            continue;
        }
        let stem = fields[1];
        let mode = fields[2];
        let cut: usize = fields[3].parse().unwrap();
        let parts: Vec<_> = stem.split('-').collect();
        let profile: u32 = parts[0][1..].parse().unwrap();
        let target_n: usize = parts[1][1..].parse().unwrap();
        let target_cancel = parts[2] == "cancel";
        println!("FLUSH_CASE_BEGIN\t{id}");
        fixture!(profile, |bound, core, state| {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let mut w = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut o = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            'history: for n in [1, 8, 63] {
                for cancel in [false, true] {
                    if cancel {
                        ok(writer.cancel_reserved(&mut w));
                    } else {
                        ok(writer.reserve_pages(&mut w, n));
                    }
                    let target = n == target_n && cancel == target_cancel;
                    if !target {
                        done(ok(flusher.try_flush_one(&mut o)));
                        continue;
                    }
                    let before = inventory(&state);
                    reset(&state);
                    let action = match mode {
                        "normal" => None,
                        "after" => Some(port::Action::AfterError),
                        "short" => Some(port::Action::Short(1)),
                        "interrupted" => Some(port::Action::Interrupted),
                        _ => panic!("unknown S5 input"),
                    };
                    if let Some(action) = action {
                        state.lock().unwrap().plan = Some(port::Plan {
                            at: cut - 1,
                            action,
                        });
                    }
                    if mode == "after" {
                        assert_eq!(
                            err(flusher.try_flush_one(&mut o)).effect,
                            Effect::CallerMustResolve
                        );
                        assert!(flusher.snapshot().fenced);
                        assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
                    } else {
                        assert!(done(ok(flusher.try_flush_one(&mut o))).finalized.cleaned);
                        no_claim(flusher.pool);
                    }
                    let s = state.lock().unwrap();
                    assert_eq!(s.hit, usize::from(mode != "normal"));
                    assert_eq!(
                        s.events.len(),
                        if mode == "after" {
                            cut
                        } else if mode == "normal" {
                            31
                        } else {
                            32
                        }
                    );
                    drop(s);
                    expanded_bytes(&state, id, &before);
                    break 'history;
                }
            }
        });
        println!("FLUSH_CASE\t{id}");
        count += 1;
    }
    assert_eq!(count, expected_count);
    assert_eq!(EXPANDED.lines().skip(1).count(), 972);
    complete("S5", count);
}
