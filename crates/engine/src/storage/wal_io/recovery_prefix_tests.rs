//! Independent frozen WAL/page/descriptor vectors. Unread geometry files use
//! bounded representative backing; native cases verify actual complete files.
use super::bootstrap::test_support as port;
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

const GENESIS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-storage-session/literal"
);
const DWB: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-doublewrite-install/literal"
);
const STATUS_BOOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-bootstrap"
);
const EXCLUSIVE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocation-reservation"
);
const DIRTY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
);
const INIT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-init"
);
const FLUSH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocator-flush-one"
);
const BASE_READS: usize = 59;
// Fixed dirty count1 reserve+cancel read schedule, before any fault selection.
// Each PPF001..059 faults exactly this 0-based event and checks its prefix.
const BASE_EVENTS: [(&str, &str, u64, usize); 59] = [
    ("metadata", "CURRENT", 0, 0),                  // 0
    ("metadata", "d3-current-stage", 0, 0),         // 1
    ("metadata", "d3-rebase-0", 0, 0),              // 2
    ("metadata", "d3-rebase-1", 0, 0),              // 3
    ("metadata", "d3-writer-stage", 0, 0),          // 4
    ("metadata", "d3-filemap-select-stage", 0, 0),  // 5
    ("open", "d3-profile", 0, 0),                   // 6
    ("metadata", "d3-profile", 0, 0),               // 7
    ("read", "d3-profile", 0, 4096),                // 8
    ("open", "FILEMAP", 0, 0),                      // 9
    ("metadata", "FILEMAP", 0, 0),                  // 10
    ("read", "FILEMAP", 0, 4096),                   // 11
    ("open", "d3-profile", 0, 0),                   // 12
    ("metadata", "d3-profile", 0, 0),               // 13
    ("read", "d3-profile", 0, 4096),                // 14
    ("open", "FILEMAP", 0, 0),                      // 15
    ("metadata", "FILEMAP", 0, 0),                  // 16
    ("read", "FILEMAP", 0, 4096),                   // 17
    ("open", "d3-filemap-0", 0, 0),                 // 18
    ("metadata", "d3-filemap-0", 0, 0),             // 19
    ("read", "d3-filemap-0", 0, 320),               // 20
    ("read", "d3-filemap-0", 16384, 256),           // 21
    ("open", "d3-bootstrap", 0, 0),                 // 22
    ("metadata", "d3-bootstrap", 0, 0),             // 23
    ("read", "d3-bootstrap", 0, 4096),              // 24
    ("open", "DWB", 0, 0),                          // 25
    ("metadata", "DWB", 0, 0),                      // 26
    ("read", "DWB", 0, 4096),                       // 27
    ("open", "d3-dwb-0", 0, 0),                     // 28
    ("metadata", "d3-dwb-0", 0, 0),                 // 29
    ("read", "d3-dwb-0", 0, 160),                   // 30
    ("open", "d3-data-0000000100000001", 0, 0),     // 31
    ("metadata", "d3-data-0000000100000001", 0, 0), // 32
    ("open", "d3-filemap-0", 0, 0),                 // 33
    ("metadata", "d3-filemap-0", 0, 0),             // 34
    ("open", "d3-filemap-1", 0, 0),                 // 35
    ("metadata", "d3-filemap-1", 0, 0),             // 36
    ("open", "d3-dwb-0", 0, 0),                     // 37
    ("metadata", "d3-dwb-0", 0, 0),                 // 38
    ("open", "d3-dwb-1", 0, 0),                     // 39
    ("metadata", "d3-dwb-1", 0, 0),                 // 40
    ("open", "d3-dwb-stage", 0, 0),                 // 41
    ("metadata", "d3-dwb-stage", 0, 0),             // 42
    ("open", "d3-dwb-images", 0, 0),                // 43
    ("metadata", "d3-dwb-images", 0, 0),            // 44
    ("open", "d3-dwb-reserved", 0, 0),              // 45
    ("metadata", "d3-dwb-reserved", 0, 0),          // 46
    ("metadata", "d3-dwb-select-stage", 0, 0),      // 47
    ("open", "d3-wal-00000001", 0, 0),              // 48
    ("metadata", "d3-wal-00000001", 0, 0),          // 49
    ("read", "d3-wal-00000001", 0, 4096),           // 50
    ("open", "WRITER", 0, 0),                       // 51
    ("metadata", "WRITER", 0, 0),                   // 52
    ("read", "WRITER", 0, 4096),                    // 53
    ("read", "d3-wal-00000001", 4096, 160),         // 54
    ("read", "d3-wal-00000001", 4256, 21040),       // 55
    ("read", "d3-wal-00000001", 25296, 160),        // 56
    ("read", "d3-wal-00000001", 25456, 21040),      // 57
    ("metadata", "d3-wal-00000001", 0, 0),          // 58
];

type State = Arc<Mutex<port::State>>;
struct Directory(State);
impl DirectoryPort for Directory {
    type Writer = port::Writer;
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        port::acquire(&self.0)
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn read(root: &str, path: &str) -> Vec<u8> {
    std::fs::read(format!("{root}/{path}")).unwrap()
}
fn literal(root: &str, profile: u32, path: &str) -> Vec<u8> {
    read(root, &format!("profile{profile}-{path}.bin"))
}
fn vector(root: &str, profile: u32, count: usize, part: &str) -> Vec<u8> {
    read(root, &format!("p{profile}-n{count}-{part}"))
}
fn status_vector(profile: u32, cycles: u64, terminal: &str, work: u64, part: &str) -> Vec<u8> {
    read(
        INIT,
        &format!("p{profile}-cancel{cycles}-{terminal}/work{work}-{part}"),
    )
}
fn mark_run(prefix: &str, count: usize, mut case: impl FnMut(usize)) {
    let cases: Vec<_> = (1..=count).map(|i| format!("{prefix}{i:03}")).collect();
    let filter = match std::env::var("XADB_RECOVERY_PREFIX_CASES") {
        Ok(raw) => {
            assert!(!raw.is_empty(), "empty case selection");
            let mut set = BTreeSet::new();
            for id in raw.split(',') {
                assert!(cases.iter().any(|v| v == id), "unknown case {id}");
                assert!(set.insert(id.to_owned()), "duplicate case {id}");
            }
            Some(set)
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(e) => panic!("invalid case selection {e}"),
    };
    let mut done = 0;
    for (i, id) in cases.iter().enumerate() {
        if filter.as_ref().is_some_and(|set| !set.contains(id)) {
            continue;
        }
        println!("RECOVERY_PREFIX_CASE_BEGIN\t{id}");
        case(i);
        println!("RECOVERY_PREFIX_CASE\t{id}\tPASS");
        done += 1;
    }
    assert!(done > 0);
    println!("RECOVERY_PREFIX_COMPLETE count={done}");
}
fn check_schedule(directory: &Directory, count: usize) {
    let state = directory.0.lock().unwrap();
    let actual: Vec<_> = state
        .read_events
        .iter()
        .map(|e| (e.kind, e.name.as_str(), e.offset, e.length))
        .collect();
    assert_eq!(actual.as_slice(), &BASE_EVENTS[..count]);
}

fn reset(state: &State) {
    let mut s = state.lock().unwrap();
    s.events.clear();
    s.read_events.clear();
    s.read_effect_counts.clear();
    s.reads = 0;
    s.hit = 0;
    s.peak = s.handles;
    s.open_modes.clear();
}
fn fixture(profile: u32, plan: u64, frames: &[u8]) -> Directory {
    let (writer, state) = port::fresh();
    drop(writer);
    {
        let mut s = state.lock().unwrap();
        let mut put = |path: &str, bytes: Vec<u8>, length: u64| {
            let mut entry = port::Entry::new(100 + s.files.len() as u64);
            entry.bytes = bytes;
            entry.length = length;
            s.files.insert(path.into(), entry);
        };
        for path in ["d3-profile", "WRITER", "FILEMAP"] {
            put(path, literal(GENESIS, profile, path), 4096);
        }
        let mut wal = literal(GENESIS, profile, "d3-wal-00000001");
        wal.extend_from_slice(frames);
        let length = wal.len() as u64;
        put(wal::WAL_NAME, wal, length);
        put(
            "d3-bootstrap",
            literal(
                if plan == 1 { GENESIS } else { STATUS_BOOT },
                profile,
                "d3-bootstrap",
            ),
            4096,
        );
        let mut map = vec![0; PAGE_BYTES + 256];
        map[..320].copy_from_slice(&literal(GENESIS, profile, "root"));
        map[PAGE_BYTES..].copy_from_slice(&literal(GENESIS, profile, "chunk"));
        put("d3-filemap-0", map, control::CONTAINER_BYTES);
        put("d3-filemap-1", vec![0xa5; 17], control::CONTAINER_BYTES);
        put(
            "d3-data-0000000100000001",
            vec![0xa5; PAGE_BYTES],
            control::EXTENT_BYTES,
        );
        put("DWB", literal(DWB, profile, "DWB"), 4096);
        put(
            "d3-dwb-0",
            literal(DWB, profile, "empty"),
            PAGE_BYTES as u64,
        );
        put("d3-dwb-1", vec![0xa5; 17], PAGE_BYTES as u64);
        put("d3-dwb-stage", vec![0xa5; 17], PAGE_BYTES as u64);
        put("d3-dwb-images", vec![0xa5; 17], 16_711_680);
        put("d3-dwb-reserved", vec![0xa5; 17], 8192);
    }
    reset(&state);
    Directory(state)
}
fn image(directory: &Directory) -> BTreeMap<String, (Vec<u8>, u64, FileIdentity)> {
    directory
        .0
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(path, e)| (path.clone(), (e.bytes.clone(), e.length, e.identity)))
        .collect()
}
fn unchanged(directory: &Directory, before: &BTreeMap<String, (Vec<u8>, u64, FileIdentity)>) {
    assert_eq!(&image(directory), before);
    let state = directory.0.lock().unwrap();
    assert!(state.events.is_empty());
    assert_eq!(state.handles, 0);
    assert_eq!(state.writers, 0);
    assert!(state.peak <= 2);
    assert!(state.open_modes.iter().all(|m| *m == OpenMode::ReadOnly));
    assert!(!state.read_events.iter().any(|event| {
        event.kind == "read"
            && [
                "d3-data-0000000100000001",
                "d3-filemap-1",
                "d3-dwb-images",
                "d3-dwb-reserved",
                "d3-dwb-stage",
                "d3-dwb-select-stage",
            ]
            .contains(&event.name.as_str())
    }));
}
fn acquire<'a>(
    directory: &Directory,
    workspace: &'a mut [u8; WORKSPACE_BYTES],
) -> Result<RecoveryPrefix<'a, port::Writer>> {
    RecoveryPrefix::acquire(directory, Some(port::namespace()), workspace)
}
fn reject(directory: &Directory) -> RecoveryPrefixError {
    match acquire(directory, &mut boxed()) {
        Err(e) => e,
        Ok(_) => panic!("unexpected prefix success"),
    }
}
fn mutate(directory: &Directory, path: &str, edit: impl FnOnce(&mut Vec<u8>)) {
    edit(
        &mut directory
            .0
            .lock()
            .unwrap()
            .files
            .get_mut(path)
            .unwrap()
            .bytes,
    );
}
fn seal(raw: &mut [u8]) {
    let end = raw.len() - 32;
    let digest = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&digest);
}
fn seal_frame(raw: &mut [u8]) {
    let digest = content_digest(&raw[..128]);
    raw[128..160].copy_from_slice(&digest);
    seal(raw);
}
fn put64(raw: &mut [u8], offset: usize, value: u64) {
    raw[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn put32(raw: &mut [u8], offset: usize, value: u32) {
    raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn set_frames(directory: &Directory, frames: &[u8]) {
    let mut s = directory.0.lock().unwrap();
    let e = s.files.get_mut(wal::WAL_NAME).unwrap();
    e.bytes.truncate(4096);
    e.bytes.extend_from_slice(frames);
    e.length = e.bytes.len() as u64;
}
fn pair(profile: u32, mode: u64, count: usize, cancel: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let root = if mode == 81920 { EXCLUSIVE } else { DIRTY };
    let mut frames = vector(root, profile, count, "reserve.frame");
    if count > 1 {
        // These literal families were captured from one actual producer run:
        // count1 work1@1/2, count8 work2@3/4, count63 work3@5/6. Preserve
        // every predecessor byte instead of rebasing isolated identities.
        let mut predecessors = Vec::new();
        for prior in [1, 8] {
            if prior < count {
                predecessors.extend_from_slice(&vector(root, profile, prior, "reserve.frame"));
                predecessors.extend_from_slice(&vector(root, profile, prior, "cancel.frame"));
            }
        }
        predecessors.extend_from_slice(&frames);
        frames = predecessors;
    }
    if cancel {
        frames.extend_from_slice(&vector(root, profile, count, "cancel.frame"));
    }
    let terminal = if cancel { "cancel" } else { "reserve" };
    (
        frames,
        vector(root, profile, count, &format!("{terminal}.page")),
        vector(root, profile, count, &format!("{terminal}.descriptor")),
    )
}
fn status_case(
    profile: u32,
    cycles: u64,
    terminal: &str,
) -> (Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>) {
    let work = cycles + 1;
    let mut frames = read(
        INIT,
        &format!("p{profile}-cancel{cycles}-{terminal}/wal.frames-only"),
    );
    let (allocator, descriptor, status) = if terminal == "init" {
        (
            status_vector(profile, cycles, terminal, work, "allocator.after"),
            status_vector(profile, cycles, terminal, work, "init.descriptor"),
            Some(status_vector(
                profile,
                cycles,
                terminal,
                work,
                "status.after",
            )),
        )
    } else {
        (
            status_vector(profile, cycles, terminal, work, "allocator.after"),
            status_vector(profile, cycles, terminal, work, "cancel.descriptor"),
            None,
        )
    };
    frames.shrink_to_fit();
    (frames, allocator, descriptor, status)
}
#[allow(clippy::too_many_arguments)] // Parallel independent frozen image and ledger expectations.
fn expect_images(
    directory: &Directory,
    expected_allocator: &[u8],
    expected_descriptor: &[u8],
    expected_status: Option<&[u8]>,
    mode: u64,
    high: u64,
    active: bool,
    mask: u64,
    lsn: u64,
) {
    let before = image(directory);
    let mut workspace = boxed();
    let owner = acquire(directory, &mut workspace).unwrap();
    let facts = owner.observation();
    assert_eq!(owner.allocator_image().as_slice(), expected_allocator);
    assert_eq!(
        owner.descriptor_bytes().unwrap().as_slice(),
        expected_descriptor
    );
    assert_eq!(owner.status_image().map(|p| p.as_slice()), expected_status);
    assert_eq!(facts.producer_mode(), Some(mode));
    assert_eq!(facts.work_high_water(), high);
    assert_eq!(facts.complete_lsn(), lsn);
    assert_eq!(facts.complete_offset(), facts.source_length());
    assert!(facts.tail().is_none());
    assert_eq!(facts.active_owner().is_some(), active);
    assert_eq!(facts.active_mask(), if active { mask } else { 0 });
    assert_eq!(
        facts.inventory_items(),
        if active { mask.count_ones() as u64 } else { 0 }
    );
    assert!(!facts.source_stabilized());
    let predecessor_hold = match (mode, high) {
        (81920 | 98304, 2) => 2,
        (81920 | 98304, 3) => 510,
        (114688, 2..) => 2,
        _ => 0,
    };
    assert_eq!(
        facts.terminal_hold_mask(),
        if active { predecessor_hold } else { mask }
    );
    if active {
        assert_eq!(
            facts.remaining_wal(),
            if mode == 114688 {
                38000
            } else {
                operation::frame_length(mask.count_ones() as usize) as u64
            }
        );
        assert_eq!(facts.remaining_memory(), mode);
        assert_eq!(facts.remaining_frames(), if mode == 114688 { 2 } else { 1 });
    } else {
        assert_eq!(facts.remaining_protected(), 0);
    }
    let retained = facts.source_length() - 4096;
    let topup = if active && mode == 114688 {
        0
    } else if expected_status.is_some() {
        32768
    } else {
        16384
    };
    assert_eq!(facts.replay_page_topup_bytes(), topup);
    assert_eq!(facts.owned_bytes(), 22069248 + retained);
    assert_eq!(
        facts.protected_bytes(),
        21024768 + retained + facts.remaining_protected() + topup
    );
    assert_eq!(directory.0.lock().unwrap().handles, 1);
    drop(owner);
    unchanged(directory, &before);
}

#[test]
fn current_producer_literals_reconstruct_canonical_images() {
    mark_run("PPN", 29, |n| {
        if n < 21 {
            let profile = n as u32 / 7 + 1;
            match n % 7 {
                0..=3 => {
                    let mode = if n % 7 < 2 { 81920 } else { 98304 };
                    let count = if n % 7 < 2 { 1 } else { 63 };
                    let cancel = !(n % 7).is_multiple_of(2);
                    let (frames, page, desc) = pair(profile, mode, count, cancel);
                    let directory = fixture(profile, 1, &frames);
                    expect_images(
                        &directory,
                        &page,
                        &desc,
                        None,
                        mode,
                        if count == 1 { 1 } else { 3 },
                        !cancel,
                        if count == 1 { 2 } else { !1u64 },
                        if count == 1 { 2 } else { 6 } - u64::from(!cancel),
                    );
                }
                4 => {
                    let frames = status_vector(profile, 0, "init", 1, "reserve.wal");
                    let page = status_vector(profile, 0, "init", 1, "allocator.reserved");
                    let desc = status_vector(profile, 0, "init", 1, "reserve.descriptor");
                    let directory = fixture(profile, 2, &frames);
                    expect_images(&directory, &page, &desc, None, 114688, 1, true, 2, 1);
                }
                5 | 6 => {
                    let terminal = if n % 7 == 5 { "cancel" } else { "init" };
                    let (frames, page, desc, status) = status_case(profile, 0, terminal);
                    let directory = fixture(profile, 2, &frames);
                    expect_images(
                        &directory,
                        &page,
                        &desc,
                        status.as_deref(),
                        114688,
                        1,
                        false,
                        2,
                        2,
                    );
                }
                _ => unreachable!(),
            }
        } else if n < 23 {
            let profile = if n == 21 { 1 } else { 3 };
            let plan = if n == 21 { 1 } else { 2 };
            let directory = fixture(profile, plan, &[]);
            let before = image(&directory);
            let mut workspace = boxed();
            let owner = acquire(&directory, &mut workspace).unwrap();
            assert_eq!(
                owner.allocator_image().as_slice(),
                literal(GENESIS, profile, "allocation")
            );
            assert!(owner.descriptor_bytes().is_none());
            assert!(owner.status_image().is_none());
            assert_eq!(owner.observation().complete_lsn(), 0);
            assert_eq!(owner.observation().producer_mode(), None);
            drop(owner);
            unchanged(&directory, &before);
        } else if n < 27 {
            let mode = if n < 25 { 81920 } else { 98304 };
            let count = if n < 25 { 63 } else { 1 };
            let cancel = !n.is_multiple_of(2);
            let (frames, page, desc) = pair(1, mode, count, cancel);
            let directory = fixture(1, 1, &frames);
            expect_images(
                &directory,
                &page,
                &desc,
                None,
                mode,
                if count == 1 { 1 } else { 3 },
                !cancel,
                if count == 1 { 2 } else { !1u64 },
                if count == 1 { 2 } else { 6 } - u64::from(!cancel),
            );
        } else {
            let terminal = if n == 27 { "cancel" } else { "init" };
            let (frames, page, desc, status) = status_case(1, 2, terminal);
            let directory = fixture(1, 2, &frames);
            expect_images(
                &directory,
                &page,
                &desc,
                status.as_deref(),
                114688,
                3,
                false,
                2,
                6,
            );
        }
    });
}

#[test]
fn owner_descriptor_and_transition_damage_reject() {
    mark_run("PPD", 24, |n| {
        let mut frames = read(INIT, "p1-cancel0-init/wal.frames-only");
        let start = if n < 12 { 0 } else { 21200 };
        let frame = &mut frames[start..if n < 12 { 21200 } else { 59200 }];
        match n % 12 {
            0 => frame[200 + 8] ^= 1, // System identity, not the expected monotone id
            1 => put64(frame, 184, 99),
            2 => frame[248] ^= 1,
            3 => {
                put64(frame, 352 + 64, 99);
                seal(&mut frame[352..1888]);
            }
            4 => {
                frame[352 + 128] ^= 1;
                seal(&mut frame[352..1888]);
            }
            5 => {
                put64(frame, 352 + 640 + 2 * 128 + 72, 81921);
                seal(&mut frame[352..1888]);
            }
            6 => frame[2272 + 100] ^= 1,
            7 => frame[8480 + 100] ^= 1,
            8 => {
                frame[286..288].copy_from_slice(&1u16.to_le_bytes());
            }
            9 => {
                frame[288..290].copy_from_slice(&1u16.to_le_bytes());
            }
            10 => {
                let at = if n < 12 { 14688 } else { 15040 };
                frame[at + 16 + 72] ^= 1;
            }
            _ => {
                let at = if n < 12 { 14896 } else { 15248 };
                put64(frame, at + 48, 99);
            }
        }
        seal_frame(frame);
        let directory = fixture(1, 2, &frames);
        let before = image(&directory);
        let error = reject(&directory);
        assert!(matches!(error.cause, Cause::Semantic(_)));
        assert_eq!(error.effect, Effect::Unchanged);
        unchanged(&directory, &before);
    });
}
fn oversized(valid: bool, length: usize) -> Vec<u8> {
    let mut frame = status_vector(1, 0, "init", 1, "reserve.wal");
    frame.resize(length, 0);
    put32(&mut frame, 12, length as u32);
    put32(&mut frame, 16, (length - 192) as u32);
    seal_frame(&mut frame);
    if !valid {
        frame[length - 1] ^= 1;
    }
    frame
}
#[test]
fn semantic_tail_and_complete_damage_are_distinguished() {
    mark_run("PPT", 12, |n| {
        let (base, _, _) = pair(1, 98304, 1, false);
        let mut frames = base.clone();
        match n {
            0 => {}
            1 => frames.truncate(1),
            2 => frames.truncate(159),
            3 => frames.truncate(160),
            4 => frames.truncate(21199),
            5 => {
                let end = frames.len() - 1;
                frames[end] ^= 1;
            }
            6 => {
                frames[24] ^= 1;
                seal_frame(&mut frames);
            }
            7 => {
                put64(&mut frames, 56, 3);
                seal_frame(&mut frames);
            }
            8 | 9 => frames = oversized(n == 9, 131105),
            10 => {
                frames = oversized(true, 131105);
                frames.truncate(50000);
            }
            _ => {
                frames[166..168].copy_from_slice(&9u16.to_le_bytes());
                seal_frame(&mut frames);
            }
        }
        let directory = fixture(1, 1, &frames);
        let before = image(&directory);
        let mut workspace = boxed();
        match n {
            0..=4 | 10 => {
                let owner = acquire(&directory, &mut workspace).unwrap();
                if n != 0 {
                    let tail = owner.observation().tail().unwrap();
                    assert_eq!(tail.offset(), 4096);
                    assert_eq!(tail.physical_bytes(), frames.len() as u64);
                    assert_eq!(owner.observation().complete_lsn(), 0);
                    assert!(owner.descriptor_bytes().is_none());
                }
                drop(owner);
            }
            5 | 8 => assert!(matches!(
                reject(&directory).cause,
                Cause::Wal(WalError::Corrupt)
            )),
            6 => assert!(matches!(
                reject(&directory).cause,
                Cause::Wal(WalError::IdentityMismatch)
            )),
            7 => assert!(matches!(
                reject(&directory).cause,
                Cause::Semantic(SemanticError::OrderMismatch)
            )),
            9 => assert!(matches!(
                reject(&directory).cause,
                Cause::UnsupportedClosure
            )),
            _ => assert!(matches!(
                reject(&directory).cause,
                Cause::Semantic(SemanticError::UnsupportedOperation)
            )),
        }
        unchanged(&directory, &before);
    });
}
fn install_dwb(directory: &Directory, phase: &str) {
    let selector = read(FLUSH, &format!("p1-n1-reserve-{phase}.selector"));
    let selected = control::decode_selector(
        &selector,
        SelectorRole::Doublewrite,
        control::decode_bootstrap_profile(
            &image(directory)["d3-profile"].0,
            Some(port::namespace()),
        )
        .unwrap(),
    )
    .unwrap();
    mutate(directory, "DWB", |raw| *raw = selector);
    mutate(
        directory,
        if selected.reference.slot == 0 {
            "d3-dwb-0"
        } else {
            "d3-dwb-1"
        },
        |raw| *raw = read(FLUSH, &format!("p1-n1-reserve-{phase}.bank")),
    );
}
#[test]
fn selected_pending_binds_historical_canonical_image() {
    mark_run("PPW", 9, |n| {
        let (frames, _, _) = pair(1, 98304, 1, true);
        let directory = fixture(1, 1, &frames);
        install_dwb(
            &directory,
            if n == 2 {
                "retirement"
            } else if n == 3 || n == 8 {
                "issuance"
            } else {
                "pending"
            },
        );
        if n == 1 {
            mutate(&directory, wal::WAL_NAME, |raw| raw.truncate(4096));
            directory
                .0
                .lock()
                .unwrap()
                .files
                .get_mut(wal::WAL_NAME)
                .unwrap()
                .length = 4096;
        }
        if n == 4 || n == 5 || n == 6 {
            let selector = image(&directory)["DWB"].0.clone();
            let bank = if selector[81] == 0 {
                "d3-dwb-0"
            } else {
                "d3-dwb-1"
            };
            mutate(&directory, bank, |raw| {
                if n == 4 {
                    raw[128 + 56] ^= 1;
                } else if n == 5 {
                    put64(raw, 128 + 48, 99);
                    put64(raw, 56, 99);
                } else {
                    put64(raw, 128 + 32, 2); // Valid nonzero owner, distinct from allocator owner1.
                }
                let length = 248;
                seal(&mut raw[..length]);
            });
            let bank_bytes = image(&directory)[bank].0[..248].to_vec();
            mutate(&directory, "DWB", |raw| {
                raw[112..144].copy_from_slice(&content_digest(&bank_bytes));
                seal(raw);
            });
        }
        if n == 8 {
            mutate(&directory, "d3-dwb-1", |raw| {
                put64(raw, 40, 2);
                seal(&mut raw[..160]);
            });
            let bytes = image(&directory)["d3-dwb-1"].0[..160].to_vec();
            mutate(&directory, "DWB", |raw| {
                raw[112..144].copy_from_slice(&content_digest(&bytes));
                seal(raw);
            });
        }
        if n == 7 {
            let mut s = directory.0.lock().unwrap();
            let mut entry = port::Entry::new(777);
            entry.bytes = vec![0xa5; 4096];
            entry.length = 4096;
            s.files.insert("d3-dwb-select-stage".into(), entry);
        }
        let before = image(&directory);
        let mut workspace = boxed();
        match n {
            0 | 2 | 3 | 7 => {
                let owner = acquire(&directory, &mut workspace).unwrap();
                assert_eq!(owner.observation().complete_lsn(), 2);
                if n == 0 {
                    assert_eq!(owner.observation().pending_lsn(), Some(1));
                    assert_eq!(
                        owner.allocator_image().as_slice(),
                        vector(DIRTY, 1, 1, "cancel.page")
                    );
                }
                drop(owner);
            }
            1 | 5 => assert!(matches!(
                reject(&directory).cause,
                Cause::RequiredPrefixMissing
            )),
            4 => assert!(matches!(
                reject(&directory).cause,
                Cause::Doublewrite(dwb::DoublewriteError::ImageMismatch)
            )),
            8 => assert!(matches!(
                reject(&directory).cause,
                Cause::UnsupportedClosure
            )),
            _ => {
                let error = reject(&directory);
                assert!(matches!(&error.cause, Cause::IdentityMismatch), "{error:?}");
            }
        }
        unchanged(&directory, &before);
    });
}

#[test]
fn mode_and_accounting_are_preserved() {
    mark_run("PPL", 12, |n| {
        if n == 0 {
            let count = 63u64;
            let mut mask = 0u64;
            for _ in 0..1000 {
                mask |= !1u64;
            }
            assert_eq!(mask.count_ones(), 63);
            let gross_tokens = count * PAGE_BYTES as u64 * 1000;
            let retained = 1000 * 2 * (20992 + 208 * count);
            assert_eq!(gross_tokens, 1_032_192_000);
            assert_eq!(retained, 68_192_000);
            assert!(retained < wal::WAL_FILE_CAP);
            // The rejected cumulative policy includes fixed maintenance and
            // physical retained WAL, not just its historic token subtotal.
            let cumulative_protected = 21_024_768 + retained + gross_tokens;
            assert_eq!(cumulative_protected, 1_121_408_768);
            assert!(cumulative_protected > 1 << 30);
            let protected = 21_024_768 + retained + PAGE_BYTES as u64;
            assert_eq!(protected, 89_233_152);
            assert!(protected < 1 << 30);
            return;
        }
        if n == 3 {
            // Isolate the fixed mode input to the pure adoption boundary using
            // an independently valid dirty frame and genesis canonical page.
            let directory = fixture(1, 1, &[]);
            let snapshot = image(&directory);
            let mut workspace = boxed();
            let mut owner = acquire(&directory, &mut workspace).unwrap();
            owner.observation.mode = Some(ProducerMode::Exclusive);
            let raw = vector(DIRTY, 1, 1, "reserve.frame");
            let expected = FrameExpectation {
                stream: WalStreamIdentity {
                    database: port::namespace().database,
                    timeline: 1,
                    writer_epoch: 1,
                },
                previous_lsn: 0,
            };
            let header =
                match wal_frame::scan_semantic_prefix(&raw, expected, TailPolicy::ForbidIncomplete)
                    .unwrap()
                {
                    SemanticFrameScan::Complete { envelope, .. } => envelope.header,
                    _ => unreachable!(),
                };
            let mut before: Box<[u8; PAGE_BYTES]> = literal(GENESIS, 1, "allocation")
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let identity = PageIdentity {
                database: port::namespace().database,
                page: PageRef::decode(&before[24..48]).unwrap(),
                owner: ObjectId::new(1).unwrap(),
                owner_generation: Generation::new(1).unwrap(),
            };
            let mut after = boxed();
            let mut status = boxed();
            let mut next_status = boxed();
            let mut descriptor = boxed::<1536>();
            let e = adopt_frame(
                &mut owner.observation,
                identity,
                CanonicalImages {
                    before: &mut before,
                    after: &mut after,
                    status: &mut status,
                    next_status: &mut next_status,
                    descriptor: &mut descriptor[..],
                },
                header,
                &raw,
            )
            .unwrap_err();
            assert!(matches!(
                e.cause,
                Cause::Semantic(SemanticError::BudgetMismatch)
            ));
            assert_eq!(owner.observation.work_high_water(), 0);
            drop(owner);
            unchanged(&directory, &snapshot);
            return;
        }
        if n == 7 {
            let (mut frames, _, _, _) = status_case(1, 0, "init");
            frames.extend_from_slice(&status_vector(1, 1, "init", 2, "reserve.wal"));
            let directory = fixture(1, 2, &frames);
            let before = image(&directory);
            assert!(matches!(
                reject(&directory).cause,
                Cause::Semantic(SemanticError::StateMismatch)
            ));
            unchanged(&directory, &before);
            return;
        }
        if n >= 10 {
            // Scalar boundary receiver, not a claim of a physical producer log.
            let directory = fixture(1, 1, &[]);
            let before = image(&directory);
            let mut workspace = boxed();
            let mut owner = acquire(&directory, &mut workspace).unwrap();
            owner.observation.complete_offset = wal::WAL_FILE_CAP - 1 + u64::from(n == 11);
            owner.observation.remaining.wal = 1;
            owner.observation.remaining.protected = 1;
            let result = account(&mut owner.observation);
            if n == 10 {
                result.unwrap();
            } else {
                assert!(matches!(result.unwrap_err().cause, Cause::Capacity));
            }
            drop(owner);
            unchanged(&directory, &before);
            return;
        }
        if n >= 8 {
            let directory = fixture(1, 1, &[]);
            let before = image(&directory);
            let mut workspace = boxed();
            let mut owner = acquire(&directory, &mut workspace).unwrap();
            owner.observation.remaining.wal = 10;
            owner.observation.remaining.protected = if n == 8 { 9 } else { 11 };
            let e = account(&mut owner.observation).unwrap_err();
            if n == 8 {
                assert!(matches!(e.cause, Cause::ArithmeticOverflow));
            } else {
                assert!(matches!(
                    e.cause,
                    Cause::Semantic(SemanticError::BudgetMismatch)
                ));
            }
            drop(owner);
            unchanged(&directory, &before);
            return;
        }
        let mode = if n < 4 { 81920 } else { 98304 };
        let cancel = n.is_multiple_of(2);
        let (frames, _, _) = pair(1, mode, 63, cancel);
        let directory = fixture(1, 1, &frames);
        if n == 6 {
            let mut state = directory.0.lock().unwrap();
            let mut entry = port::Entry::new(801);
            entry.length = 4096;
            entry.bytes = vec![0; 4096];
            state.files.insert("d3-dwb-select-stage".into(), entry);
        }
        let before = image(&directory);
        let mut workspace = boxed();
        let owner = acquire(&directory, &mut workspace).unwrap();
        let facts = owner.observation();
        assert_eq!(facts.producer_mode(), Some(mode));
        assert_eq!(
            facts.remaining_protected(),
            if cancel {
                0
            } else {
                operation::frame_length(63) as u64 + 63 * PAGE_BYTES as u64
            }
        );
        assert_eq!(facts.work_high_water(), 3);
        assert_eq!(facts.complete_lsn(), if cancel { 6 } else { 5 });
        assert_eq!(facts.terminal_hold_mask(), if cancel { !1u64 } else { 510 });
        assert!(!facts.source_stabilized());
        assert_eq!(facts.replay_page_topup_bytes(), 16384);
        assert_eq!(
            facts.owned_bytes(),
            22069248 + frames.len() as u64 + if n == 6 { 4096 } else { 0 }
        );
        assert_eq!(
            facts.protected_bytes(),
            21024768 + frames.len() as u64 + facts.remaining_protected() + 16384
        );
        drop(owner);
        unchanged(&directory, &before);
    });
}

#[test]
fn required_metadata_and_source_snapshot_are_checked() {
    mark_run("PPG", 19, |n| {
        let (frames, _, _) = pair(1, 98304, 1, true);
        let directory = fixture(1, 1, &frames);
        if n < 4 {
            let baseline = fixture(1, 1, &frames);
            let mut work = boxed();
            drop(acquire(&baseline, &mut work).unwrap());
            check_schedule(&baseline, BASE_READS);
            let snapshot = 58;
            let source_open = 48;
            let mut state = directory.0.lock().unwrap();
            match n {
                0 => state.metadata_change = Some((snapshot, true)),
                1 => {
                    // Simulate an external actor appending immediately before the
                    // final named snapshot. Only that injected byte may change.
                    let mut bytes = state.files[wal::WAL_NAME].bytes.clone();
                    bytes.push(0xa5);
                    state.read_edits = vec![(snapshot, wal::WAL_NAME.into(), bytes)];
                }
                2 => state.metadata_change = Some((source_open, true)),
                _ => state.length_delta = Some((source_open, -1)),
            }
        } else if n < 7 {
            let mut s = directory.0.lock().unwrap();
            let mut e = port::Entry::new(700);
            e.length = if n == 4 {
                0
            } else if n == 5 {
                4096
            } else {
                4097
            };
            e.bytes = vec![0; e.length as usize];
            s.files.insert("d3-dwb-select-stage".into(), e);
        } else if n < 13 {
            let paths = [
                "d3-profile",
                "FILEMAP",
                "d3-bootstrap",
                "DWB",
                "d3-dwb-stage",
                "WRITER",
            ];
            directory
                .0
                .lock()
                .unwrap()
                .files
                .get_mut(paths[n - 7])
                .unwrap()
                .length -= 1;
        } else {
            let mut s = directory.0.lock().unwrap();
            let mut e = port::Entry::new(800);
            e.bytes = vec![];
            e.length = 0;
            s.files.insert(BLOCKED[n - 13].into(), e);
        }
        let mut before = image(&directory);
        if n == 1 {
            let source = before.get_mut(wal::WAL_NAME).unwrap();
            source.0.push(0xa5);
            source.1 += 1;
        }
        if n == 4 || n == 5 {
            let mut work = boxed();
            drop(acquire(&directory, &mut work).unwrap());
        } else {
            let e = reject(&directory);
            assert_eq!(e.effect, Effect::Unchanged);
            if n >= 13 {
                assert!(matches!(e.cause, Cause::UnsupportedClosure));
            }
        }
        unchanged(&directory, &before);
    });
}

#[test]
fn every_new_io_fault_releases_source_and_lease() {
    mark_run("PPF", 68, |n| {
        if n >= 64 {
            // Only the new streaming calls are faulted here; common control
            // calls already have independent PPF001..059 fault cases.
            let frames = oversized(true, 131105);
            let directory = fixture(1, 1, &frames);
            let before = image(&directory);
            let at = 55 + n - 64;
            directory.0.lock().unwrap().read_plan = Some(port::Plan {
                at,
                action: port::Action::BeforeError,
            });
            assert!(matches!(reject(&directory).cause, Cause::Io(_)));
            let state = directory.0.lock().unwrap();
            assert_eq!(state.configured_read_hits, 1);
            let streaming: Vec<_> = state.read_events[55..]
                .iter()
                .map(|e| (e.kind, e.name.as_str(), e.offset, e.length))
                .collect();
            const STREAM: [(&str, &str, u64, usize); 4] = [
                ("read", "d3-wal-00000001", 4096, 65536),
                ("read", "d3-wal-00000001", 69632, 65536),
                ("read", "d3-wal-00000001", 135168, 1),
                ("read", "d3-wal-00000001", 135169, 32),
            ];
            assert_eq!(streaming.as_slice(), &STREAM[..n - 63]);
            drop(state);
            unchanged(&directory, &before);
            return;
        }
        let (frames, _, _) = pair(1, 98304, 1, true);
        let directory = fixture(1, 1, &frames);
        let before = image(&directory);
        if n < BASE_READS {
            directory.0.lock().unwrap().read_plan = Some(port::Plan {
                at: n,
                action: port::Action::BeforeError,
            });
            let e = reject(&directory);
            assert_eq!(e.effect, Effect::Unchanged);
            assert!(matches!(
                e.cause,
                Cause::Namespace(_) | Cause::Io(_) | Cause::PageIo(_)
            ));
            assert_eq!(directory.0.lock().unwrap().configured_read_hits, 1);
            check_schedule(&directory, n + 1);
        } else {
            match n {
                59 | 60 => {
                    directory.0.lock().unwrap().read_plan = Some(port::Plan {
                        at: 8,
                        action: if n == 59 {
                            port::Action::Short(17)
                        } else {
                            port::Action::Interrupted
                        },
                    });
                    let mut work = boxed();
                    drop(acquire(&directory, &mut work).unwrap());
                }
                61 => {
                    directory.0.lock().unwrap().read_plan = Some(port::Plan {
                        at: 8,
                        action: port::Action::PrefixEof,
                    });
                    let _ = reject(&directory);
                }
                62 => {
                    directory.0.lock().unwrap().read_plan = Some(port::Plan {
                        at: 8,
                        action: port::Action::BeforePanic,
                    });
                    assert!(
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reject(
                            &directory
                        )))
                        .is_err()
                    );
                }
                _ => {
                    let writer = port::acquire(&directory.0).unwrap();
                    assert!(matches!(
                        reject(&directory).cause,
                        Cause::Namespace(NamespaceError {
                            kind: NamespaceErrorKind::Busy,
                            ..
                        })
                    ));
                    drop(writer);
                }
            }
        }
        unchanged(&directory, &before);
    });
}
#[test]
fn workspace_owner_and_observation_are_closed() {
    mark_run("PPB", 5, |n| {
        let (frames, _, _) = pair(1, 98304, 1, n.is_multiple_of(2));
        let directory = fixture(1, 1, &frames);
        let before = image(&directory);
        let mut work = boxed();
        {
            let owner = acquire(&directory, &mut work).unwrap();
            assert_eq!(directory.0.lock().unwrap().handles, 1);
            assert!(matches!(
                reject(&directory).cause,
                Cause::Namespace(NamespaceError {
                    kind: NamespaceErrorKind::Busy,
                    ..
                })
            ));
            if n == 1 {
                assert!(owner.observation().active_owner().is_some());
            }
            if n == 2 {
                assert_eq!(owner.observation().terminal_hold_mask(), 2);
            }
            drop(owner);
        }
        work.fill(0xa5);
        if n == 3 {
            let mut next = boxed();
            drop(acquire(&directory, &mut next).unwrap());
        } else if n == 4 {
            // Reborrow the same caller storage after overwriting all old RAM
            // bytes. Both canonical outputs must be reconstructed from source.
            let owner = acquire(&directory, &mut work).unwrap();
            assert_eq!(
                owner.allocator_image().as_slice(),
                vector(DIRTY, 1, 1, "cancel.page")
            );
            assert_eq!(
                owner.descriptor_bytes().unwrap().as_slice(),
                vector(DIRTY, 1, 1, "cancel.descriptor")
            );
            assert!(owner.status_image().is_none());
            {
                let state = directory.0.lock().unwrap();
                assert_eq!(state.handles, 1);
                assert_eq!(state.writers, 1);
            }
            drop(owner);
            {
                let state = directory.0.lock().unwrap();
                assert_eq!(state.handles, 0);
                assert_eq!(state.writers, 0);
            }
            drop(port::acquire(&directory.0).unwrap());
        }
        unchanged(&directory, &before);
    });
}

struct OversizedDirectory<const N: usize, const H: usize>(std::cell::Cell<usize>);
struct OversizedWriter<const N: usize, const H: usize>([u8; N]);
struct OversizedFile<const H: usize>([u8; H]);
impl<const H: usize> xadb_foundation::io::FileIo for OversizedFile<H> {
    fn read_at(&mut self, _: &mut [u8], _: u64) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::Other.into())
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::Other.into())
    }
    fn sync_all(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Other.into())
    }
    fn set_len(&mut self, _: u64) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Other.into())
    }
}
impl<const N: usize, const H: usize> DirectoryPort for OversizedDirectory<N, H> {
    type Writer = OversizedWriter<N, H>;
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        self.0.set(self.0.get() + 1);
        Err(forbidden())
    }
}
impl<const N: usize, const H: usize> WriterNamespace for OversizedWriter<N, H> {
    type File = OversizedFile<H>;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        Err(forbidden())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        Err(forbidden())
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<Self::File> {
        Err(forbidden())
    }
    fn open_existing_with_metadata(
        &mut self,
        _: &FileName,
        _: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        Err(forbidden())
    }
    fn metadata(&self, _: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        Err(forbidden())
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        Err(forbidden())
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        Err(forbidden())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        Err(forbidden())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        Err(forbidden())
    }
}
#[test]
fn source_census_and_oversized_ports_are_checked() {
    mark_run("PPR", 5, |n| {
        if n == 0 {
            let census = source_census::<port::Writer>();
            println!(
                "RECOVERY_PREFIX_CENSUS outer={} bootstrap={} replay={} stream={} peak={} caller=262144",
                census.outer, census.bootstrap, census.replay, census.stream, census.peak
            );
            assert!(fits::<port::Writer>());
            assert!(census.peak <= 65536);
            assert!(init::fits());
            assert!(budget::fits::<port::Writer>());
            let (frames, _, _) = pair(1, 98304, 1, true);
            let directory = fixture(1, 1, &frames);
            let before = image(&directory);
            let mut work = boxed();
            drop(acquire(&directory, &mut work).unwrap());
            check_schedule(&directory, BASE_READS);
            unchanged(&directory, &before);
        } else if n == 1 {
            check_oversized(&OversizedDirectory::<65536, 8>(std::cell::Cell::new(0)));
        } else if n == 2 {
            check_oversized(&OversizedDirectory::<8, 65536>(std::cell::Cell::new(0)));
        } else {
            let directory = fixture(1, 1, &[]);
            directory
                .0
                .lock()
                .unwrap()
                .files
                .get_mut(wal::WAL_NAME)
                .unwrap()
                .length = if n == 3 { 4095 } else { wal::WAL_FILE_CAP + 1 };
            let before = image(&directory);
            let e = reject(&directory);
            assert_eq!(e.phase, Phase::Epoch);
            if n == 3 {
                assert!(matches!(e.cause, Cause::LengthMismatch));
            } else {
                assert!(matches!(e.cause, Cause::UnsupportedClosure));
            }
            assert!(
                !directory
                    .0
                    .lock()
                    .unwrap()
                    .read_events
                    .iter()
                    .any(|event| event.kind == "read" && event.name == wal::WAL_NAME)
            );
            unchanged(&directory, &before);
        }
    });
}

fn check_oversized<const N: usize, const H: usize>(directory: &OversizedDirectory<N, H>) {
    let mut work = boxed();
    let e = match RecoveryPrefix::acquire(directory, None, &mut work) {
        Err(e) => e,
        Ok(_) => panic!("oversized success"),
    };
    assert!(matches!(e.cause, Cause::Capacity));
    assert_eq!(directory.0.get(), 0);
}

#[test]
fn shared_semantic_wrappers_keep_complete_byte_checks() {
    mark_run("PPS", 6, |n| {
        let profile = control::decode_bootstrap_profile(
            &literal(GENESIS, 1, "d3-profile"),
            Some(port::namespace()),
        )
        .unwrap();
        let mut scratch = vec![[0; PAGE_BYTES]; 3].into_boxed_slice();
        let scratch: &mut [[u8; PAGE_BYTES]; 3] = (&mut *scratch).try_into().unwrap();
        let bootstrap = bootstrap::BootstrapPlan::prepare_for_kind(
            profile,
            scratch,
            if n < 4 {
                bootstrap::BootstrapPlanKind::AllocatorOnly
            } else {
                bootstrap::BootstrapPlanKind::StatusBootstrap
            },
        )
        .unwrap();
        let allocator = bootstrap.allocation_identity().unwrap();
        if n < 4 {
            let mode = if n < 2 { 81920 } else { 98304 };
            let cancel = !n.is_multiple_of(2);
            let root = if mode == 81920 { EXCLUSIVE } else { DIRTY };
            let before: Box<[u8; PAGE_BYTES]> = if cancel {
                vector(root, 1, 1, "reserve.page")
            } else {
                literal(GENESIS, 1, "allocation")
            }
            .into_boxed_slice()
            .try_into()
            .unwrap();
            let after: Box<[u8; PAGE_BYTES]> = vector(
                root,
                1,
                1,
                if cancel {
                    "cancel.page"
                } else {
                    "reserve.page"
                },
            )
            .into_boxed_slice()
            .try_into()
            .unwrap();
            let raw = vector(
                root,
                1,
                1,
                if cancel {
                    "cancel.frame"
                } else {
                    "reserve.frame"
                },
            );
            let prior = vector(root, 1, 1, "reserve.descriptor");
            let plan = operation::Plan {
                profile,
                identity: allocator,
                prefix: wal::WalPrefix {
                    stream: WalStreamIdentity {
                        database: profile.spec().namespace.database,
                        timeline: 1,
                        writer_epoch: 1,
                    },
                    file: FileIdentity {
                        device: 7,
                        inode: 9,
                    },
                    complete_lsn: if cancel { 1 } else { 0 },
                    complete_offset: if cancel { 25296 } else { 4096 },
                    synced_lsn: if cancel { 1 } else { 0 },
                    synced_offset: if cancel { 25296 } else { 4096 },
                },
                batch: Batch {
                    owner: SystemWorkRef::new(
                        profile.spec().namespace,
                        SystemWorkKind::AllocationMaintenance,
                        1,
                        Generation::new(1).unwrap(),
                    )
                    .unwrap(),
                    mask: 2,
                    birth: 1,
                },
                cancel,
                before_descriptor: cancel.then_some(prior.as_slice()),
            };
            let budget = if mode == 81920 {
                OperationBudget::Exclusive81920
            } else {
                OperationBudget::Dirty98304
            };
            operation::validate_with_budget(plan, &before, &raw, &after, budget).unwrap();
            let mut bad = raw.clone();
            bad[2232] = 1;
            seal_frame(&mut bad);
            assert!(operation::validate_with_budget(plan, &before, &bad, &after, budget).is_err());
        } else {
            let fence = crate::storage::session_fence::SessionFence::new();
            // Private fixture-only byte receiver; native producers exercise the
            // installed live role/fence path. Recovery never constructs a role.
            let role = wal::StatusRole {
                plan: &bootstrap,
                fence: &fence,
            };
            let before: Box<[u8; PAGE_BYTES]> =
                status_vector(1, 0, "init", 1, "allocator.reserved")
                    .into_boxed_slice()
                    .try_into()
                    .unwrap();
            let after: Box<[u8; PAGE_BYTES]> = status_vector(1, 0, "init", 1, "allocator.after")
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let status: Box<[u8; PAGE_BYTES]> = status_vector(1, 0, "init", 1, "status.after")
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let prior: Box<[u8; 1536]> = status_vector(1, 0, "init", 1, "reserve.descriptor")
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let raw = status_vector(1, 0, "init", 1, "init.wal");
            let plan = init::InitPlan {
                profile,
                allocator,
                payload: PageIdentity {
                    database: allocator.database,
                    page: PageRef {
                        page_no: 1,
                        incarnation: Generation::new(2).unwrap(),
                        ..allocator.page
                    },
                    owner: ObjectId::new(2).unwrap(),
                    owner_generation: Generation::new(1).unwrap(),
                },
                prefix: wal::WalPrefix {
                    stream: WalStreamIdentity {
                        database: profile.spec().namespace.database,
                        timeline: 1,
                        writer_epoch: 1,
                    },
                    file: FileIdentity {
                        device: 7,
                        inode: 9,
                    },
                    complete_lsn: 1,
                    complete_offset: 25296,
                    synced_lsn: 1,
                    synced_offset: 25296,
                },
                batch: Batch {
                    owner: SystemWorkRef::new(
                        profile.spec().namespace,
                        SystemWorkKind::AllocationMaintenance,
                        1,
                        Generation::new(1).unwrap(),
                    )
                    .unwrap(),
                    mask: 2,
                    birth: 1,
                },
                before_descriptor: &prior,
                role: &role,
            };
            init::validate_init(&plan, &before, &raw, &after, &status).unwrap();
            let mut bad = raw.clone();
            bad[if n == 4 { 14688 + 88 } else { 15040 + 112 }] = 1;
            seal_frame(&mut bad);
            assert!(init::validate_init(&plan, &before, &bad, &after, &status).is_err());
        }
    });
}

#[test]
#[ignore = "DEV3 S5 expanded canonical prefix combinations"]
fn s5_prefix_literal_matrix() {
    mark_run("PPX", 60, |n| {
        let profile = n as u32 / 20 + 1;
        let row = n % 20;
        if row >= 12 {
            let cycles = [0, 1, 2, 8][(row - 12) / 2] as u64;
            let kind = if row.is_multiple_of(2) {
                "cancel"
            } else {
                "init"
            };
            let (frames, page, desc, status) = status_case(profile, cycles, kind);
            let directory = fixture(profile, 2, &frames);
            expect_images(
                &directory,
                &page,
                &desc,
                status.as_deref(),
                114688,
                cycles + 1,
                false,
                2,
                2 * (cycles + 1),
            );
        } else {
            let mode = if row < 6 { 81920 } else { 98304 };
            let count = [1, 8, 63][(row % 6) / 2];
            let cancel = !row.is_multiple_of(2);
            let (frames, page, desc) = pair(profile, mode, count, cancel);
            let directory = fixture(profile, 1, &frames);
            expect_images(
                &directory,
                &page,
                &desc,
                None,
                mode,
                match count {
                    1 => 1,
                    8 => 2,
                    63 => 3,
                    _ => unreachable!(),
                },
                !cancel,
                if count == 63 {
                    !1u64
                } else {
                    ((1u64 << (count + 1)) - 1) & !1
                },
                match count {
                    1 => 2,
                    8 => 4,
                    63 => 6,
                    _ => unreachable!(),
                } - u64::from(!cancel),
            );
        }
    });
}
