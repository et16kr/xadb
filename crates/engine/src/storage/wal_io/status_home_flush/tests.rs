//! New Status-home consumer cases. Prior setup APIs construct fresh fixtures;
//! their previously completed test selectors are never invoked here.
use super::super::super::flush_io::test_support as io_test;
use super::super::{InitAttemptError, InitializedStatusObservation};
use super::*;
use crate::{
    storage::{
        dirty_frame::{DirtyFrame, init_test_support as init_test, test_support as frame_test},
        wal_io::{FreshStorageSession, bootstrap::test_support as port},
    },
    transaction::concurrency::OwnerRole,
};
use foundation::codec::content_digest;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-home-flush"
);
const HOME: &str = "d3-data-0000000100000001";
const STEP: Duration = Duration::from_secs(5);
const INIT_OPERATION_BYTES: usize = super::super::OPERATION_BYTES;
type State = Arc<Mutex<port::State>>;
type Inventory = BTreeMap<String, (u64, [u8; 32])>;

struct AdapterPause {
    binding: usize,
    arrived: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}
static ADAPTER_PAUSE: Mutex<Option<AdapterPause>> = Mutex::new(None);
static FORCE_PAUSE: Mutex<Option<AdapterPause>> = Mutex::new(None);
pub(super) fn before_init_force(binding: &AtomicU8) {
    let pause = {
        let mut entry = FORCE_PAUSE.lock().unwrap();
        if entry
            .as_ref()
            .is_some_and(|p| p.binding == std::ptr::from_ref(binding) as usize)
        {
            entry.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        pause.arrived.try_send(()).unwrap();
        pause.release.recv_timeout(STEP).unwrap();
    }
}
pub(super) fn adapter_pre_take(binding: &AtomicU8) {
    let pause = {
        let mut entry = ADAPTER_PAUSE.lock().unwrap();
        if entry
            .as_ref()
            .is_some_and(|p| p.binding == std::ptr::from_ref(binding) as usize)
        {
            entry.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        pause.arrived.try_send(()).unwrap();
        pause.release.recv_timeout(STEP).unwrap();
    }
}
fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn ok<T, E: std::fmt::Debug>(value: std::result::Result<T, E>) -> T {
    value.unwrap()
}
fn err<T, E>(value: std::result::Result<T, E>) -> E {
    match value {
        Err(error) => error,
        Ok(_) => panic!("unexpected success"),
    }
}
fn converted<B, E>(result: std::result::Result<B, IntoFlushError<E>>) -> B {
    match result {
        Ok(bound) => bound,
        Err(IntoFlushError::Before { error, .. }) | Err(IntoFlushError::Fatal(error)) => {
            panic!("conversion {error:?}")
        }
    }
}
fn selected(id: &str) -> bool {
    let selected =
        std::env::var("XADB_STATUS_FLUSH_CASES").map_or(true, |v| v.split(',').any(|s| s == id));
    if selected {
        println!("STATUS_FLUSH_CASE_BEGIN\t{id}");
    }
    selected
}
fn variant(id: &str, variant: &str) {
    println!("STATUS_FLUSH_VARIANT\t{id}\t{variant}");
}
fn passed(id: &str, count: &mut usize) {
    *count += 1;
    println!("STATUS_FLUSH_CASE\t{id}");
}
fn complete(group: &str, count: usize) {
    assert!(count > 0);
    println!("STATUS_FLUSH_COMPLETE {group} {count}");
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
fn no_io(state: &State) {
    let s = state.lock().unwrap();
    assert_eq!((s.events.len(), s.reads), (0, 0));
}
fn inventory(state: &State) -> Inventory {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, e)| {
            assert!(e.sparse_tail.is_empty());
            (name.clone(), (e.length, content_digest(&e.bytes)))
        })
        .collect()
}
fn literal(logical: &str) -> Vec<u8> {
    let table = std::fs::read_to_string(format!("{INPUT}/literals.tsv")).unwrap();
    let rows: Vec<_> = table
        .lines()
        .skip(1)
        .filter(|r| r.split('\t').next() == Some(logical))
        .collect();
    assert_eq!(rows.len(), 1, "literal {logical}");
    let fields: Vec<_> = rows[0].split('\t').collect();
    let bytes = std::fs::read(format!("{INPUT}/{}", fields[1])).unwrap();
    assert_eq!(bytes.len(), fields[2].parse::<usize>().unwrap());
    bytes
}
fn diagnose(
    pool: &SharedPool<'_, '_, '_, '_>,
) -> (
    Box<[u8; PAGE_BYTES]>,
    Box<[u8; PAGE_BYTES]>,
    init_test::InitSnapshot,
) {
    let mut a = boxed(0);
    let mut s = boxed(0);
    let snap = init_test::init_diagnose(pool, &mut a, &mut s);
    (a, s, snap)
}
fn done(progress: FlushProgress) -> FlushObservation {
    match progress {
        FlushProgress::Complete(observed) => observed,
        other => panic!("expected complete {other:?}"),
    }
}
fn initialize<'s, 'c, W: WriterNamespace>(
    writer: &mut StatusBootstrapWriter<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    cancellations: usize,
) -> InitializedStatusObservation {
    for _ in 0..cancellations {
        match ok(writer.reserve_one(owner)).cancel() {
            Ok(_) => {}
            Err(InitAttemptError::Before(e)) => panic!("cancel before {:?}", e.error),
            Err(InitAttemptError::Fatal(e)) => panic!("cancel fatal {e:?}"),
        }
    }
    match ok(writer.reserve_one(owner)).initialize() {
        Ok(observation) => observation,
        Err(InitAttemptError::Before(e)) => panic!("INIT before {:?}", e.error),
        Err(InitAttemptError::Fatal(e)) => panic!("INIT fatal {e:?}"),
    }
}
macro_rules! storage_fixture {
    ($profile:expr, |$storage:ident, $state:ident| $body:block) => {{
        let mut slot = port::slot(); let mut workspace = boxed::<{4 << 20}>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(namespace, $profile, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage = installed.into_mutable_storage(); reset(&$state);
        $body
        drop($storage);
        let state = $state.lock().unwrap(); assert_eq!((state.handles, state.writers), (0, 0));
    }};
}
macro_rules! original_fixture {
    ($profile:expr, |$bound:ident, $core:ident, $state:ident, $storage:ident| $body:block) => {{
        storage_fixture!($profile, |$storage, $state| {
            let $core = PageConcurrency::new($storage.session_fence()).unwrap();
            let mut allocator = boxed::<PAGE_BYTES>(0x2d);
            let mut payload = boxed::<PAGE_BYTES>(0xa5);
            let mut operation = boxed::<INIT_OPERATION_BYTES>(0x73);
            let mut frames = [
                DirtyFrame::new(&mut allocator),
                DirtyFrame::new(&mut payload),
            ];
            let $bound = ok($storage.bind_status_bootstrap(&$core, &mut frames, &mut operation));
            reset(&$state);
            $body
        });
    }};
}
macro_rules! fixture {
    ($profile:expr, |$bound:ident, $core:ident, $state:ident| $body:block) => {{
        original_fixture!($profile, |original, $core, $state, storage| {
            let mut $bound = converted(original.into_with_flush());
            reset(&$state); $body
            let drop_counts = { let s = $state.lock().unwrap(); (s.events.len(), s.reads) };
            drop($bound); assert!(storage.session_fence().is_fenced());
            { let s = $state.lock().unwrap(); assert_eq!((s.events.len(), s.reads), drop_counts); }
        });
    }};
}

fn full_state(state: &State, key: &str, untouched: &Inventory) {
    let table = std::fs::read_to_string(format!("{INPUT}/states.tsv")).unwrap();
    let s = state.lock().unwrap();
    let mut regions = Vec::new();
    for row in table.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[0] != key {
            continue;
        }
        let expected = literal(fields[2]);
        let actual = match fields[1] {
            "home:page0" => &s.files[HOME].bytes[..PAGE_BYTES],
            "home:page1" => &s.files[HOME].bytes[PAGE_BYTES..2 * PAGE_BYTES],
            "d3-dwb-images:slot0" => &s.files["d3-dwb-images"].bytes[..16544],
            name => &s.files[name].bytes,
        };
        assert_eq!(actual, expected, "{key} {}", fields[1]);
        regions.push(fields[1]);
    }
    assert!(regions.len() >= 7, "missing state rows {key}");
    assert_eq!(
        s.files.contains_key("d3-dwb-select-stage"),
        regions.contains(&"d3-dwb-select-stage")
    );
    let mut expected_names: std::collections::BTreeSet<_> =
        untouched.keys().map(String::as_str).collect();
    expected_names.remove("d3-dwb-select-stage");
    if regions.contains(&"d3-dwb-select-stage") {
        expected_names.insert("d3-dwb-select-stage");
    }
    assert_eq!(
        s.files
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        expected_names
    );
    for (name, entry) in &s.files {
        assert!(entry.sparse_tail.is_empty());
        assert_eq!(
            entry.length,
            entry.bytes.len() as u64,
            "full physical length {name}"
        );
        if name == HOME {
            assert!(entry.bytes[2 * PAGE_BYTES..].iter().all(|b| *b == 0));
        } else if name == "d3-dwb-images" {
            assert!(entry.bytes[16544..].iter().all(|b| *b == 0));
        } else if ![
            "DWB",
            "d3-dwb-stage",
            "d3-dwb-0",
            "d3-dwb-1",
            "d3-dwb-select-stage",
        ]
        .contains(&name.as_str())
        {
            assert_eq!(
                (entry.length, content_digest(&entry.bytes)),
                untouched[name],
                "untouched {name}"
            );
        }
    }
}
fn normal_effects(state: &State, stem: &str, target: u8) {
    let table = std::fs::read_to_string(format!("{INPUT}/effects.tsv")).unwrap();
    let s = state.lock().unwrap();
    let rows: Vec<_> = table.lines().skip(1).collect();
    assert_eq!(s.events.len(), rows.len());
    for (event, row) in s.events.iter().zip(rows) {
        let fields: Vec<_> = row.split('\t').collect();
        let kind = if fields[2] == "directory_sync" {
            "dirsync"
        } else {
            fields[2]
        };
        assert_eq!(event.kind, kind);
        assert_eq!(event.length, fields[3].parse::<usize>().unwrap());
        let (phase, step) = fields[1].split_once('.').unwrap();
        let mut expected_bytes = None;
        let name = match phase {
            "images" => {
                if kind == "write" {
                    expected_bytes = Some(literal(&format!("{stem}/slot")));
                }
                "d3-dwb-images".to_owned()
            }
            "home" => {
                if kind == "write" {
                    expected_bytes = Some(literal(&format!("{stem}/capture")));
                }
                HOME.to_owned()
            }
            _ if step.starts_with("stage_") => {
                if kind == "write" {
                    expected_bytes = Some(literal(&format!("{stem}/{phase}.bank")));
                }
                "d3-dwb-stage".to_owned()
            }
            _ if step.starts_with("bank_") => {
                let selector = literal(&format!("{stem}/{phase}.selector"));
                if kind == "write" {
                    expected_bytes = Some(literal(&format!("{stem}/{phase}.bank")));
                }
                format!("d3-dwb-{}", selector[81])
            }
            _ if step == "directory_sync" => String::new(),
            _ => {
                if kind == "write" {
                    expected_bytes = Some(literal(&format!("{stem}/{phase}.selector")));
                }
                "d3-dwb-select-stage".to_owned()
            }
        };
        assert_eq!(event.name, name);
        assert_eq!(
            event.offset,
            if phase == "home" && kind == "write" && target == b'S' {
                PAGE_BYTES as u64
            } else {
                0
            }
        );
        if let Some(bytes) = expected_bytes {
            assert_eq!(event.data, bytes);
            assert_eq!(event.returned, Some(bytes.len()));
        }
    }
}
fn normal(profile: u32, cancellations: usize, order: &str) {
    fixture!(profile, |bound, core, state| {
        let (mut writer, reader, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        let observed = initialize(&mut writer, &mut owner, cancellations);
        assert_eq!(observed.lsn, PageLsn(2 * cancellations as u64 + 2));
        let untouched = inventory(&state);
        for (batch, target) in order.bytes().enumerate() {
            reset(&state);
            let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            let result = done(ok(if target == b'A' {
                flusher.try_flush_allocator(&mut dedicated)
            } else {
                flusher.try_flush_status(&mut dedicated)
            }));
            assert!(result.finalized.cleaned);
            assert_eq!(result.io.known.generation, 3 * (batch as u64 + 1) + 1);
            assert_eq!(result.io.last_issued, batch as u64 + 1);
            assert_eq!((result.io.attempted, result.io.completed), (31, 31));
            assert!(result.io.homes_synced && result.io.retirement_selected);
            assert_eq!(state.lock().unwrap().events.len(), 31);
            normal_effects(
                &state,
                &format!("p{profile}-c{cancellations}-{order}-b{}", batch + 1),
                target,
            );
            let (a, s, snapshot) = diagnose(flusher.pool);
            let clean = if batch == 1 {
                [true, true]
            } else if target == b'A' {
                [true, false]
            } else {
                [false, true]
            };
            assert_eq!(snapshot.dirty, clean.map(|v| !v));
            assert_eq!(snapshot.page_lsns, [observed.lsn; 2]);
            assert_eq!(snapshot.counters, [2 * cancellations as u64 + 2, 1]);
            for (i, clean) in clean.into_iter().enumerate() {
                assert_eq!(snapshot.entries[i].is_none(), clean);
                assert_eq!(snapshot.requirements[i].is_none(), clean);
                assert_eq!(snapshot.rec_lsns[i].is_none(), clean);
            }
            assert_eq!(
                &a[..],
                literal(&format!("p{profile}-c{cancellations}-AS-b1/capture"))
            );
            assert_eq!(
                &s[..],
                literal(&format!("p{profile}-c{cancellations}-SA-b1/capture"))
            );
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_none());
            full_state(
                &state,
                &format!("p{profile}-c{cancellations}-{order}-b{}/after", batch + 1),
                &untouched,
            );
        }
        let mut reader_owner = ok(core.try_owner(OwnerRole::Foreground));
        let read = ok(reader.read(&mut reader_owner));
        ok(read.release_checked());
    });
}
#[test]
fn status_flush_bn() {
    let mut count = 0;
    for profile in 1..=3 {
        for (oi, order) in ["AS", "SA"].into_iter().enumerate() {
            let id = format!("BN{:02}", (profile - 1) * 2 + oi as u32 + 1);
            if selected(&id) {
                normal(profile, 0, order);
                passed(&id, &mut count);
            }
        }
    }
    complete("BN", count);
}
#[test]
#[ignore = "DEV3 final S5 only"]
fn status_flush_s5_histories() {
    let mut count = 0;
    for profile in 1..=3 {
        for cancellations in [1, 2, 8, 31] {
            for order in ["AS", "SA"] {
                let id = format!("SH-P{profile}-C{cancellations}-{order}");
                if selected(&id) {
                    normal(profile, cancellations, order);
                    passed(&id, &mut count);
                }
            }
        }
    }
    complete("SH", count);
}

#[path = "authority_tests.rs"]
mod authority;
#[test]
fn status_flush_ba() {
    authority::run_ba();
}
#[path = "race_tests.rs"]
mod races;
#[test]
fn status_flush_br() {
    races::run_br();
}
#[path = "resource_tests.rs"]
mod resources;
#[test]
fn status_flush_rf() {
    resources::run_rf();
}

#[path = "home_tests.rs"]
mod home;
#[path = "native_tests.rs"]
mod native;
#[test]
fn status_flush_bh() {
    home::run_bh();
}
#[test]
fn status_flush_pf() {
    home::run_pf();
}
#[test]
#[ignore = "DEV3 final S5 only"]
fn status_flush_s5_faults() {
    home::run_s5_faults();
}
