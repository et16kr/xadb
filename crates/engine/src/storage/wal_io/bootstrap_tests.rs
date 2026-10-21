use super::super::{Cause as WalCause, Phase as WalPhase, SlotState};
use super::{test_support::*, *};
use crate::storage::{
    page::PageRef,
    read_buffer::{BufferError, ReadBuffer, ReadFrame},
    wal_frame::WalError,
};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const DATA: &str = "d3-data-0000000100000001";
const BANK0: &str = "d3-filemap-0";
const BANK1: &str = "d3-filemap-1";

fn literal(profile: u32, name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/dev3-storage-session/literal/profile{profile}-{name}.bin"
        )),
    )
    .unwrap()
}
fn alternate(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/dev3-storage-session/alternate/{name}.bin"
        )),
    )
    .unwrap()
}
fn sealed(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let hash = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
}
fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn images(profile: u32) -> BTreeMap<String, Vec<u8>> {
    let mut result = BTreeMap::from([("LOCK".into(), Vec::new())]);
    for name in [
        "d3-profile",
        "d3-wal-00000001",
        "WRITER",
        "d3-bootstrap",
        "FILEMAP",
    ] {
        result.insert(name.into(), literal(profile, name));
    }
    let mut data = vec![0; 1048576];
    data[..16384].copy_from_slice(&literal(profile, "allocation"));
    result.insert(DATA.into(), data);
    let mut bank = vec![0; 2113536];
    bank[..320].copy_from_slice(&literal(profile, "root"));
    bank[16384..16640].copy_from_slice(&literal(profile, "chunk"));
    result.insert(BANK0.into(), bank);
    result.insert(BANK1.into(), vec![0; 2113536]);
    result
}
fn image_hashes(images: &BTreeMap<String, Vec<u8>>) -> Vec<(String, usize, [u8; 32])> {
    images
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.len(), content_digest(bytes)))
        .collect()
}
fn observed(state: &Arc<Mutex<State>>) -> BTreeMap<String, Vec<u8>> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, entry)| {
            assert!(entry.sparse_tail.is_empty());
            assert_eq!(entry.length, entry.bytes.len() as u64);
            (name.clone(), entry.bytes.clone())
        })
        .collect()
}
fn expected_events(profile: u32) -> Vec<Event> {
    let files = images(profile);
    let mut result = Vec::new();
    let mut add = |kind, name: &str, offset, length| {
        let source = match name {
            "d3-writer-stage" => "WRITER",
            "d3-filemap-select-stage" => "FILEMAP",
            _ => name,
        };
        let data = if kind == "write" {
            files[source][offset as usize..offset as usize + length].to_vec()
        } else {
            Vec::new()
        };
        result.push(Event {
            kind,
            name: name.into(),
            offset,
            length,
            data,
            returned: if kind == "write" { Some(length) } else { None },
        });
    };
    for (name, size, stride) in [
        ("d3-profile", 4096, 4096),
        ("d3-wal-00000001", 4096, 4096),
        ("d3-writer-stage", 4096, 4096),
        ("d3-bootstrap", 4096, 4096),
        (DATA, 1048576, 16384),
        (BANK0, 2113536, 16384),
        (BANK1, 2113536, 16384),
        ("d3-filemap-select-stage", 4096, 4096),
    ] {
        add("create", name, 0, 0);
        for offset in (0..size).step_by(stride) {
            add("write", name, offset as u64, stride);
        }
        add("sync", name, 0, 0);
        if matches!(name, "d3-writer-stage" | "d3-filemap-select-stage") {
            add("publish", name, 0, 0);
        }
        if matches!(
            name,
            "d3-wal-00000001"
                | "d3-writer-stage"
                | "d3-bootstrap"
                | "d3-filemap-1"
                | "d3-filemap-select-stage"
        ) {
            add("dirsync", "", 0, 0);
        }
    }
    assert_eq!(result.len(), 350);
    result
}
fn apply(events: &[Event]) -> BTreeMap<String, Vec<u8>> {
    let mut result = BTreeMap::from([("LOCK".into(), Vec::new())]);
    for event in events {
        match event.kind {
            "create" => {
                assert!(result.insert(event.name.clone(), Vec::new()).is_none());
            }
            "write" => {
                let data = result.get_mut(&event.name).unwrap();
                assert_eq!(data.len(), event.offset as usize);
                data.extend_from_slice(&event.data);
            }
            "publish" => {
                let file = result.remove(&event.name).unwrap();
                let name = if event.name == "d3-writer-stage" {
                    "WRITER"
                } else {
                    "FILEMAP"
                };
                result.insert(name.into(), file);
            }
            "sync" | "dirsync" => {}
            _ => panic!("unknown independent event"),
        }
    }
    result
}
fn assert_trace(actual: &[Event], expected: &[Event]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            (a.kind, &a.name, a.offset, a.length),
            (b.kind, &b.name, b.offset, b.length),
            "effect {}",
            i + 1
        );
        assert!(a.data == b.data, "effect {} bytes", i + 1);
    }
}
fn phase(index: usize) -> Phase {
    match index {
        13 => Phase::DescriptorCreate,
        14 => Phase::DescriptorWrite,
        15 => Phase::DescriptorSync,
        16 => Phase::DescriptorDirectory,
        17 => Phase::DataCreate,
        18..=81 => Phase::DataWrite,
        82 => Phase::DataSync,
        83 => Phase::Bank0Create,
        84..=212 => Phase::Bank0Write,
        213 => Phase::Bank0Sync,
        214 => Phase::Bank1Create,
        215..=343 => Phase::Bank1Write,
        344 => Phase::Bank1Sync,
        345 => Phase::FilesDirectory,
        346 => Phase::SelectorCreate,
        347 => Phase::SelectorWrite,
        348 => Phase::SelectorSync,
        349 => Phase::SelectorPublish,
        350 => Phase::SelectorDirectory,
        _ => panic!("bad frozen mutation"),
    }
}
fn identity() -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: PageRef {
            file: FileId::new(0x100000001).unwrap(),
            page_no: 0,
            incarnation: Generation::new(1).unwrap(),
        },
        owner: ObjectId::new(1).unwrap(),
        owner_generation: Generation::new(1).unwrap(),
    }
}
fn prepared(profile: u32, scratch: &mut [[u8; 16384]; 3]) -> BootstrapPlan {
    let profile =
        control::decode_bootstrap_profile(&literal(profile, "d3-profile"), Some(namespace()))
            .unwrap();
    BootstrapPlan::prepare(profile, scratch).unwrap()
}

#[test]
fn bootstrap_literals_and_geometry() {
    for profile in 1..=3 {
        let mut scratch = [[0xa5; 16384]; 3];
        let plan = prepared(profile, &mut scratch);
        assert!(scratch[0].as_slice() == literal(profile, "allocation"));
        assert!(scratch[1][..320] == literal(profile, "root"));
        assert!(scratch[1][320..576] == literal(profile, "chunk"));
        assert!(scratch[2][..4096] == literal(profile, "d3-bootstrap"));
        assert!(scratch[2][4096..8192] == literal(profile, "FILEMAP"));
        let mut reference = [0; 64];
        plan.selected.reference.encode(&mut reference).unwrap();
        assert!(reference.as_slice() == literal(profile, "root-ref"));
        let allocation =
            allocation_page::decode_allocation_page(&scratch[0], identity(), namespace()).unwrap();
        let summary = allocation.summary().unwrap();
        assert_eq!((summary.allocated, summary.reusable_free), (1, 63));
        for i in 1..64 {
            assert_eq!(
                allocation
                    .entry(i)
                    .unwrap()
                    .next_reservation_generation()
                    .unwrap()
                    .get(),
                2
            );
        }
        let mut output = [0xa5; 4100];
        plan.encode_descriptor(&mut output).unwrap();
        assert_eq!(&output[4096..], &[0xa5; 4]);
        let (writer, state) = fresh();
        let mut slot = slot();
        let session =
            FreshStorageSession::create(writer, profile, &mut slot, &mut scratch).unwrap();
        assert_eq!(
            image_hashes(&observed(&state)),
            image_hashes(&images(profile))
        );
        assert_eq!(session.plan.allocation_identity().unwrap(), identity());
    }
}

#[test]
fn bootstrap_descriptor_rejects_damage() {
    let mut scratch = [[0; 16384]; 3];
    let plan = prepared(1, &mut scratch);
    let golden = literal(1, "d3-bootstrap");
    for end in 0..=4096 {
        assert_eq!(plan.decode_descriptor(&golden[..end]).is_ok(), end == 4096);
    }
    let mut trailing = golden.clone();
    trailing.push(0);
    assert!(plan.decode_descriptor(&trailing).is_err());
    for i in 0..4096 {
        let mut b = golden.clone();
        b[i] ^= 1;
        assert!(plan.decode_descriptor(&b).is_err(), "byte {i}");
    }
    let mut reserved = 0;
    for range in [7..8, 12..16, 146..148, 164..176, 288..4064] {
        for i in range {
            let mut b = golden.clone();
            b[i] = 1;
            sealed(&mut b);
            assert_eq!(plan.decode_descriptor(&b), Err(ControlError::Corrupt));
            reserved += 1;
        }
    }
    assert_eq!(reserved, 3795);
    let cases = [
        (0, 1, 1, 'c'),
        (4, 2, 2, 'u'),
        (6, 1, 2, 'u'),
        (8, 4, 4095, 'c'),
        (16, 1, 0, 'i'),
        (32, 8, 2, 'i'),
        (40, 8, 2, 'i'),
        (48, 1, 0, 'i'),
        (80, 8, 0x100000002, 'c'),
        (88, 8, 2097152, 'c'),
        (96, 8, 2, 'c'),
        (104, 8, 2, 'c'),
        (112, 8, 2, 'c'),
        (120, 8, 2, 'c'),
        (128, 8, 2, 'c'),
        (136, 8, 2, 'c'),
        (144, 1, 3, 'c'),
        (145, 1, 1, 'c'),
        (148, 4, 1, 'c'),
        (152, 8, 2, 'c'),
        (160, 4, 224, 'c'),
        (176, 1, 0, 'i'),
        (208, 1, 0, 'i'),
        (240, 8, 5296129, 'c'),
        (248, 8, 273727489, 'c'),
        (256, 8, 4247553, 'c'),
        (264, 8, 4227073, 'c'),
        (272, 8, 2, 'c'),
        (280, 8, 2, 'u'),
    ];
    for (offset, width, value, kind) in cases {
        let mut b = golden.clone();
        if matches!(offset, 0 | 16 | 48 | 176 | 208) {
            b[offset] ^= 1;
        } else {
            b[offset..offset + width].copy_from_slice(&u64::to_le_bytes(value)[..width]);
        }
        sealed(&mut b);
        let expected = match kind {
            'c' => ControlError::Corrupt,
            'u' => ControlError::UnsupportedFormat,
            _ => ControlError::IdentityMismatch,
        };
        assert_eq!(plan.decode_descriptor(&b), Err(expected), "field {offset}");
    }
    for n in [0, 1, 4095] {
        let mut b = vec![0xa5; n];
        assert!(plan.encode_descriptor(&mut b).is_err());
        assert!(b.iter().all(|&b| b == 0xa5));
    }
    let mut bad = literal(1, "d3-profile");
    put64(&mut bad, 40, 2);
    sealed(&mut bad);
    let bad_profile = control::decode_bootstrap_profile(&bad, Some(namespace())).unwrap();
    let bad_plan = BootstrapPlan {
        profile: bad_profile,
        ..plan
    };
    let mut b = [0xa5; 4097];
    assert_eq!(
        bad_plan.encode_descriptor(&mut b),
        Err(ControlError::InvalidInput)
    );
    assert_eq!(b, [0xa5; 4097]);
    assert_eq!(
        BootstrapPlan::prepare(bad_profile, &mut scratch)
            .err()
            .unwrap()
            .effect,
        Effect::CallerMustResolve
    );
}

#[test]
fn bootstrap_requires_fresh_namespace() {
    for id in [0, 4] {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut scratch = [[0; 16384]; 3];
        let e = FreshStorageSession::create(writer, id, &mut slot, &mut scratch)
            .err()
            .unwrap();
        assert_eq!((e.phase, e.effect), (Phase::Preflight, Effect::Unchanged));
        assert!(state.lock().unwrap().events.is_empty());
    }
    for name in [
        "d3-bootstrap",
        DATA,
        BANK0,
        BANK1,
        "d3-filemap-select-stage",
        "FILEMAP",
        "CURRENT",
        "DWB",
        "foreign",
        "missing-lock",
        "nonzero-lock",
    ] {
        let (writer, state) = fresh();
        {
            let mut s = state.lock().unwrap();
            match name {
                "missing-lock" => {
                    s.files.clear();
                }
                "nonzero-lock" => {
                    let f = s.files.get_mut("LOCK").unwrap();
                    f.bytes.push(0x42);
                    f.length = 1;
                }
                _ => {
                    s.files.insert(name.into(), Entry::new(77));
                }
            }
        }
        let before = image_hashes(&observed(&state));
        let mut slot = slot();
        let mut scratch = [[0; 16384]; 3];
        let e = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch)
            .err()
            .unwrap();
        assert_eq!(e.effect, Effect::Unchanged);
        assert!(state.lock().unwrap().events.is_empty());
        assert_eq!(image_hashes(&observed(&state)), before);
    }
}

#[test]
fn bootstrap_installs_exact_order() {
    let reads = [
        ("open", "d3-bootstrap", 0, 0),
        ("metadata", "d3-bootstrap", 0, 0),
        ("read", "d3-bootstrap", 0, 4096),
        ("open", BANK1, 0, 0),
        ("metadata", BANK1, 0, 0),
        ("open", "d3-profile", 0, 0),
        ("read", "d3-profile", 0, 4096),
        ("open", "FILEMAP", 0, 0),
        ("read", "FILEMAP", 0, 4096),
        ("open", BANK0, 0, 0),
        ("read", BANK0, 0, 320),
        ("read", BANK0, 16384, 256),
        ("read", BANK0, 16384, 256),
        ("open", DATA, 0, 0),
        ("read", DATA, 0, 16384),
    ];
    for profile in 1..=3 {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut scratch = [[0; 16384]; 3];
        let session =
            FreshStorageSession::create(writer, profile, &mut slot, &mut scratch).unwrap();
        let prefix = session.prefix().unwrap();
        assert_eq!(
            (
                prefix.complete_lsn,
                prefix.synced_lsn,
                prefix.complete_offset,
                prefix.synced_offset
            ),
            (0, 0, 4096, 4096)
        );
        let s = state.lock().unwrap();
        assert_trace(&s.events, &expected_events(profile));
        assert_eq!(s.read_events.len(), 24);
        for (a, b) in s.read_events[9..].iter().zip(reads) {
            assert_eq!((a.kind, a.name.as_str(), a.offset, a.length), b);
        }
        for (i, e) in s.events.iter().enumerate() {
            let hash: String = content_digest(&e.data)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            println!(
                "STORAGE_EFFECT {profile} {} {} {} {} {} {hash}",
                i + 1,
                e.kind,
                if e.name.is_empty() { "-" } else { &e.name },
                e.offset,
                e.length
            );
        }
    }
}

#[test]
fn bootstrap_faults_preserve_artifacts() {
    let expected = expected_events(1);
    for at in 12..350 {
        for action in [Action::BeforeError, Action::AfterError, Action::AfterPanic] {
            let (writer, state) = fresh();
            state.lock().unwrap().plan = Some(Plan { at, action });
            let mut slot = slot();
            let mut scratch = [[0; 16384]; 3];
            let result = catch_unwind(AssertUnwindSafe(|| {
                FreshStorageSession::create(writer, 1, &mut slot, &mut scratch).map(drop)
            }));
            if action == Action::AfterPanic {
                assert!(result.is_err());
            } else {
                let e = result.unwrap().unwrap_err();
                assert_eq!(
                    (e.phase, e.effect),
                    (phase(at + 1), Effect::CallerMustResolve)
                );
                assert!(match expected[at].kind {
                    "write" | "sync" => matches!(e.cause, Cause::Io(_)),
                    _ => matches!(e.cause, Cause::Namespace(_)),
                });
                match &e.cause {
                    Cause::Io(e) => lower_io(e, true, Some(std::io::ErrorKind::Other)),
                    Cause::Namespace(e) => lower_namespace(
                        e,
                        if action == Action::BeforeError && expected[at].kind != "dirsync" {
                            Effect::Unchanged
                        } else {
                            Effect::CallerMustResolve
                        },
                    ),
                    _ => unreachable!(),
                }
            }
            let completed = at + usize::from(action != Action::BeforeError);
            assert!(
                observed(&state) == apply(&expected[..completed]),
                "cut {} {action:?}",
                at + 1
            );
            {
                let mut s = state.lock().unwrap();
                assert_eq!(
                    (s.hit, s.events.len(), s.handles, s.writers),
                    (1, at + 1, 0, 0)
                );
                assert_trace(&s.events[..at], &expected[..at]);
                if action != Action::BeforeError {
                    assert_trace(&s.events[at..], &expected[at..at + 1]);
                }
                s.plan = None;
            }
            let before = observed(&state);
            let writer = acquire(&state).unwrap();
            let e = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch)
                .err()
                .unwrap();
            assert_eq!(e.effect, Effect::Unchanged);
            assert_eq!(state.lock().unwrap().events.len(), at + 1);
            assert!(observed(&state) == before);
        }
    }
    println!("STORAGE_FAULTS before=338 after=338 unwind=338");
}

fn lower_io(error: &xadb_foundation::Error, write: bool, kind: Option<std::io::ErrorKind>) {
    assert_eq!(
        error.code,
        if write {
            xadb_foundation::ErrorCode::IoWriteUnknown
        } else {
            xadb_foundation::ErrorCode::IoRead
        }
    );
    assert_eq!(error.io_kind, kind);
    assert_eq!(
        error.effect(),
        if write {
            Effect::CallerMustResolve
        } else {
            Effect::Unchanged
        }
    );
}
fn lower_namespace(error: &NamespaceError, effect: Effect) {
    assert_eq!(
        error.kind,
        xadb_foundation::namespace::NamespaceErrorKind::Io
    );
    assert_eq!(error.effect, effect);
    assert_eq!(error.io_kind, Some(std::io::ErrorKind::Other));
}
#[test]
fn bootstrap_short_io_is_bounded() {
    use std::io::ErrorKind;
    let expected = expected_events(1);
    for index in [14, 18, 19, 84, 85, 86, 215, 347] {
        let original = &expected[index - 1];
        for action in [
            Action::Short(1),
            Action::Short(17),
            Action::Short(4096),
            Action::Interrupted,
            Action::Zero,
            Action::InvalidCount,
            Action::BeforeError,
            Action::PrefixError,
        ] {
            let (writer, state) = fresh();
            state.lock().unwrap().plan = Some(Plan {
                at: index - 1,
                action,
            });
            let mut slot = slot();
            let mut scratch = [[0; 16384]; 3];
            let result = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch);
            let success = matches!(action, Action::Short(_) | Action::Interrupted);
            let first = match action {
                Action::Short(n) => Some(n.min(original.length)),
                Action::Zero => Some(0),
                Action::InvalidCount => Some(original.length + 1),
                _ => None,
            };
            if success {
                drop(result.unwrap());
                assert!(observed(&state) == images(1));
            } else {
                let e = result.err().unwrap();
                assert_eq!(
                    (e.phase, e.effect),
                    (phase(index), Effect::CallerMustResolve)
                );
                let Cause::Io(error) = e.cause else {
                    panic!("write cause was wrapped incorrectly")
                };
                lower_io(
                    &error,
                    true,
                    match action {
                        Action::Zero => Some(ErrorKind::WriteZero),
                        Action::InvalidCount => None,
                        _ => Some(ErrorKind::Other),
                    },
                );
                let mut image = apply(&expected[..index - 1]);
                if action == Action::PrefixError {
                    image
                        .get_mut(&original.name)
                        .unwrap()
                        .extend_from_slice(&original.data[..original.length / 2]);
                }
                assert!(
                    observed(&state) == image,
                    "exact failed image {index} {action:?}"
                );
            }
            let s = state.lock().unwrap();
            assert_eq!((s.hit, s.handles, s.writers), (1, 0, 0));
            let extra = success
                && (action == Action::Interrupted || first.is_some_and(|n| n < original.length));
            assert_eq!(
                s.events.len(),
                if success {
                    350 + usize::from(extra)
                } else {
                    index
                }
            );
            assert_trace(&s.events[..index - 1], &expected[..index - 1]);
            let first_event = &s.events[index - 1];
            assert_eq!(
                (
                    first_event.kind,
                    &first_event.name,
                    first_event.offset,
                    first_event.length,
                    first_event.returned
                ),
                (
                    original.kind,
                    &original.name,
                    original.offset,
                    original.length,
                    first
                )
            );
            if extra {
                let advanced = first.unwrap_or(0);
                let next = &s.events[index];
                assert_eq!(
                    (
                        next.kind,
                        &next.name,
                        next.offset,
                        next.length,
                        next.returned
                    ),
                    (
                        "write",
                        &original.name,
                        original.offset + advanced as u64,
                        original.length - advanced,
                        Some(original.length - advanced)
                    )
                );
                assert!(next.data == original.data[advanced..]);
                assert_trace(&s.events[index + 1..], &expected[index..]);
            } else if success {
                assert_trace(&s.events[index..], &expected[index..]);
            }
        }
    }
    for observation in [3, 7, 9, 11, 12, 13, 15] {
        for action in [Action::Short(1), Action::Interrupted, Action::PrefixEof] {
            let at = 9 + observation - 1;
            let (writer, state) = fresh();
            state.lock().unwrap().read_plan = Some(Plan { at, action });
            let mut slot = slot();
            let mut scratch = [[0; 16384]; 3];
            let result = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch);
            if action == Action::PrefixEof {
                let e = result.err().unwrap();
                assert_eq!(
                    (e.phase, e.effect),
                    (Phase::Readback, Effect::CallerMustResolve)
                );
                match e.cause {
                    Cause::Io(e) if observation == 3 => {
                        lower_io(&e, false, Some(ErrorKind::UnexpectedEof))
                    }
                    Cause::PageIo(PageIoError::Read(e)) if observation != 3 => {
                        lower_io(&e, false, Some(ErrorKind::UnexpectedEof))
                    }
                    _ => panic!("EOF cause changed"),
                }
            } else {
                drop(result.unwrap());
            }
            assert!(observed(&state) == images(1));
            let s = state.lock().unwrap();
            assert_eq!((s.events.len(), s.handles, s.writers), (350, 0, 0));
            assert_eq!(s.hit, if action == Action::PrefixEof { 2 } else { 1 });
            assert_eq!(
                s.read_events.len(),
                if action == Action::PrefixEof {
                    at + 2
                } else {
                    25
                }
            );
            let first = &s.read_events[at];
            let next = &s.read_events[at + 1];
            let count = match action {
                Action::Short(_) => 1,
                Action::Interrupted => 0,
                _ => first.length / 2,
            };
            assert_eq!(
                first.returned,
                if action == Action::Interrupted {
                    None
                } else {
                    Some(count)
                }
            );
            assert_eq!(first.data.len(), count);
            assert_eq!(
                (next.kind, &next.name, next.offset, next.length),
                (
                    "read",
                    &first.name,
                    first.offset + count as u64,
                    first.length - count
                )
            );
            assert_eq!(
                next.returned,
                Some(if action == Action::PrefixEof {
                    0
                } else {
                    first.length - count
                })
            );
            let expected = &images(1)[&first.name]
                [first.offset as usize..first.offset as usize + first.length];
            assert!(first.data == expected[..count]);
            if action == Action::PrefixEof {
                assert!(next.data.is_empty());
            } else {
                assert!(next.data == expected[count..]);
            }
        }
    }
}

fn readback_error(observation: usize, error: BootstrapError) {
    assert_eq!(
        (error.phase, error.effect),
        (Phase::Readback, Effect::CallerMustResolve)
    );
    assert!(
        match observation {
            1 | 2 | 4 | 5 => matches!(error.cause, Cause::Namespace(_)),
            3 => matches!(error.cause, Cause::Io(_)),
            6 | 8 | 10 | 14 => matches!(error.cause, Cause::PageIo(PageIoError::Namespace(_))),
            _ => matches!(error.cause, Cause::PageIo(PageIoError::Read(_))),
        },
        "observation {observation}: {:?}",
        error.cause
    );
    match &error.cause {
        Cause::Namespace(e) | Cause::PageIo(PageIoError::Namespace(e)) => {
            lower_namespace(e, Effect::Unchanged)
        }
        Cause::Io(e) | Cause::PageIo(PageIoError::Read(e)) => {
            lower_io(e, false, Some(std::io::ErrorKind::Other))
        }
        _ => unreachable!(),
    }
}

fn closed_readback(state: &Arc<Mutex<State>>, stop: usize, expected: &BTreeMap<String, Vec<u8>>) {
    let s = state.lock().unwrap();
    assert_eq!(
        (s.events.len(), s.reads, s.handles, s.writers),
        (350, 9 + stop, 0, 0)
    );
    drop(s);
    assert!(&observed(state) == expected);
}
fn changed_selection() -> Vec<(String, Vec<u8>)> {
    let mut bank = vec![0; 2113536];
    bank[..320].copy_from_slice(&alternate("root"));
    bank[16384..16640].copy_from_slice(&alternate("chunk"));
    vec![
        (BANK1.into(), bank),
        ("FILEMAP".into(), alternate("FILEMAP")),
    ]
}
#[test]
fn bootstrap_readback_binds_installed_objects() {
    for observation in 1..=15 {
        for action in [Action::BeforeError, Action::AfterPanic] {
            let (writer, state) = fresh();
            state.lock().unwrap().read_plan = Some(Plan {
                at: 9 + observation - 1,
                action,
            });
            let mut slot = slot();
            let mut scratch = [[0; 16384]; 3];
            let result = catch_unwind(AssertUnwindSafe(|| {
                FreshStorageSession::create(writer, 1, &mut slot, &mut scratch).map(drop)
            }));
            if action == Action::AfterPanic {
                assert!(result.is_err());
            } else {
                readback_error(observation, result.unwrap().unwrap_err());
            }
            let s = state.lock().unwrap();
            assert_eq!(
                (s.hit, s.handles, s.writers, s.events.len()),
                (1, 0, 0, 350)
            );
            drop(s);
            closed_readback(&state, observation, &images(1));
        }
    }
    for observation in [1, 4, 6, 8, 10, 14] {
        for delta in [-1, 1] {
            let (writer, state) = fresh();
            state.lock().unwrap().length_delta = Some((9 + observation - 1, delta));
            let mut slot = slot();
            let mut scratch = [[0; 16384]; 3];
            let e = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch)
                .err()
                .unwrap();
            assert_eq!(
                (e.phase, e.effect),
                (Phase::Readback, Effect::CallerMustResolve)
            );
            assert!(if observation == 1 || observation == 4 {
                matches!(e.cause, Cause::LengthMismatch)
            } else {
                matches!(e.cause, Cause::PageIo(PageIoError::LengthMismatch))
            });
            closed_readback(&state, observation, &images(1));
        }
    }
    for observation in [2, 5] {
        let (writer, state) = fresh();
        state.lock().unwrap().metadata_change = Some((9 + observation - 1, true));
        let mut slot = slot();
        let mut scratch = [[0; 16384]; 3];
        let e = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch)
            .err()
            .unwrap();
        assert!(matches!(e.cause, Cause::IdentityMismatch));
        assert_eq!(
            (e.phase, e.effect),
            (Phase::Readback, Effect::CallerMustResolve)
        );
        closed_readback(&state, observation, &images(1));
    }
    let mut edits = Vec::new();
    for offset in [176, 208] {
        let mut bytes = literal(1, "d3-bootstrap");
        bytes[offset] ^= 1;
        sealed(&mut bytes);
        edits.push(vec![("d3-bootstrap".into(), bytes)]);
    }
    edits.push(vec![
        ("d3-profile".into(), literal(2, "d3-profile")),
        ("FILEMAP".into(), literal(2, "FILEMAP")),
    ]);
    edits.push(changed_selection());
    let mut data = images(1).remove(DATA).unwrap();
    put64(&mut data, 10312, 2);
    sealed(&mut data[..16384]);
    allocation_page::decode_allocation_page(&data[..16384], identity(), namespace()).unwrap();
    edits.push(vec![(DATA.into(), data)]);
    for (case, changes) in edits.into_iter().enumerate() {
        let (writer, state) = fresh();
        state.lock().unwrap().read_edits = changes
            .iter()
            .map(|(n, b)| (9, n.clone(), b.clone()))
            .collect();
        let mut slot = slot();
        let mut scratch = [[0; 16384]; 3];
        let e = FreshStorageSession::create(writer, 1, &mut slot, &mut scratch)
            .err()
            .unwrap();
        assert_eq!(
            (e.phase, e.effect),
            (Phase::Readback, Effect::CallerMustResolve)
        );
        assert!(
            if case < 2 {
                matches!(e.cause, Cause::Control(ControlError::IdentityMismatch))
            } else {
                matches!(e.cause, Cause::IdentityMismatch)
            },
            "case {case}: {:?}",
            e.cause
        );
        let mut expected = images(1);
        for (n, b) in &changes {
            expected.insert(n.clone(), b.clone());
        }
        closed_readback(
            &state,
            if case < 2 {
                3
            } else if case < 4 {
                12
            } else {
                15
            },
            &expected,
        );
        if case == 2 || case == 3 {
            let mut raw = acquire(&state).unwrap();
            let [root, scratch, _] = &mut scratch;
            assert!(PageFileReader::open(&mut raw, Some(namespace()), root, scratch).is_ok());
            drop(raw);
            // Each later shared construction must independently bind the full state.
            let (writer, state) = fresh();
            let mut init = [[0; 16384]; 3];
            let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
            {
                let mut s = state.lock().unwrap();
                for (n, b) in changes {
                    let f = s.files.get_mut(&n).unwrap();
                    f.length = b.len() as u64;
                    f.bytes = b;
                }
            }
            assert!(matches!(
                session.open_reader(root, scratch),
                Err(PageIoError::IdentityMismatch)
            ));
            assert!(
                !session
                    .reader_active
                    .load(std::sync::atomic::Ordering::Acquire)
            );
            assert!(!session.is_fenced());
        }
    }
}

fn channel() -> (mpsc::SyncSender<()>, mpsc::Receiver<()>) {
    mpsc::sync_channel(0)
}
fn receive(receiver: &mpsc::Receiver<()>) {
    receiver.recv_timeout(Duration::from_secs(5)).unwrap();
}
fn pause_at(at: usize, unwind: bool) -> (Pause, mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (entered, reached) = channel();
    let (release, resume) = channel();
    (
        Pause {
            at,
            after: false,
            entered,
            release: resume,
            unwind,
        },
        reached,
        release,
    )
}
fn await_condition(mut condition: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < end,
            "bounded rendezvous timed out"
        );
        std::thread::yield_now();
    }
}

#[test]
fn bootstrap_reader_and_wal_make_progress() {
    let (writer, state) = fresh();
    let mut slot = slot();
    let mut init = [[0; 16384]; 3];
    let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
    let mut root = [0; 16384];
    let mut scratch = [0; 16384];
    let mut output = [0; 16384];
    let mut other_root = [0; 16384];
    let mut other_scratch = [0; 16384];
    let mut reader = session.open_reader(&mut root, &mut scratch).unwrap();
    let reads = state.lock().unwrap().reads;
    assert!(matches!(
        session.open_reader(&mut other_root, &mut other_scratch),
        Err(PageIoError::Busy)
    ));
    assert_eq!(state.lock().unwrap().reads, reads);
    let (pause, reached, release) = pause_at(reads + 2, false);
    state.lock().unwrap().read_pause = Some(pause);
    std::thread::scope(|scope| {
        let read = scope.spawn(|| {
            reader
                .read_page(identity(), PageKind::Allocation, 0, &mut output)
                .unwrap();
        });
        receive(&reached);
        // The actual data FileIo is paused, after the namespace mutex was released.
        assert!(session.wal._writer.try_lock().is_ok());
        let (done, completed) = channel();
        let session = &session;
        let write = scope.spawn(move || {
            session.try_stage(&raw_frame(0, 1, &[])).unwrap();
            session.sync_pending().unwrap();
            done.send(()).unwrap();
        });
        receive(&completed);
        release.send(()).unwrap();
        read.join().unwrap();
        write.join().unwrap();
    });
    let at = state.lock().unwrap().events.len();
    let (pause, reached, release) = pause_at(at, false);
    state.lock().unwrap().pause = Some(pause);
    session.try_stage(&raw_frame(1, 2, &[1])).unwrap();
    std::thread::scope(|scope| {
        let write = scope.spawn(|| session.sync_pending().unwrap());
        receive(&reached);
        assert!(session.wal.append.try_lock().is_ok());
        assert!(session.wal._writer.try_lock().is_ok());
        let (done, completed) = channel();
        let reader = &mut reader;
        let output = &mut output;
        let read = scope.spawn(move || {
            reader
                .read_page(identity(), PageKind::Allocation, 0, output)
                .unwrap();
            done.send(()).unwrap();
        });
        receive(&completed);
        release.send(()).unwrap();
        read.join().unwrap();
        write.join().unwrap();
    });
    // A paused container destructor still owns the one-reader lease and handle.
    let (pause, reached, release) = pause_at(0, false);
    state.lock().unwrap().drop_pause = Some(pause);
    std::thread::scope(|scope| {
        let closing = scope.spawn(move || drop(reader));
        receive(&reached);
        assert!(matches!(
            session.open_reader(&mut other_root, &mut other_scratch),
            Err(PageIoError::Busy)
        ));
        assert_eq!(state.lock().unwrap().handles, 2);
        release.send(()).unwrap();
        closing.join().unwrap();
    });
    drop(
        session
            .open_reader(&mut other_root, &mut other_scratch)
            .unwrap(),
    );
    assert_eq!(state.lock().unwrap().handles, 1);
    assert!(!session.is_fenced());
    assert_eq!(
        (
            session.prefix().unwrap().synced_lsn,
            state.lock().unwrap().peak
        ),
        (2, 3)
    );
}

fn typed_frame(pages: &[(u64, u64)], previous: u64, lsn: u64) -> Vec<u8> {
    let mut records = Vec::new();
    for &(file, page) in pages {
        let mut r = vec![0; 65];
        r[0] = 1;
        r[1] = if page == 0 { 10 } else { 1 };
        r[4..8].copy_from_slice(&65u32.to_le_bytes());
        put64(&mut r, 8, file);
        put64(&mut r, 16, page);
        for offset in [24, 32, 40] {
            put64(&mut r, offset, 1);
        }
        put64(&mut r, 48, previous);
        r[56..58].copy_from_slice(&72u16.to_le_bytes());
        r[58..60].copy_from_slice(&1u16.to_le_bytes());
        r[64] = 0x3a;
        records.extend(r);
    }
    let mut frame = raw_frame(previous, lsn, &records);
    frame[7] = 1;
    frame[20..24].copy_from_slice(&(pages.len() as u32).to_le_bytes());
    seal_frame(&mut frame);
    frame
}

#[test]
fn bootstrap_wal_requires_selected_mapping() {
    for pages in [
        vec![(0x100000001, 0)],
        vec![(0x100000001, 63)],
        vec![(0x100000002, 0)],
        vec![(0x100000001, 64)],
        vec![(0x100000001, u64::MAX)],
        vec![(0x100000001, 0), (0x100000001, 64)],
    ] {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let before = session.prefix().unwrap();
        let calls = state.lock().unwrap().events.len();
        let frame = typed_frame(&pages, 0, 1);
        let result = session.try_stage(&frame);
        let valid = pages.iter().all(|&(f, p)| f == 0x100000001 && p < 64);
        if valid {
            assert!(result.is_ok());
            session.sync_pending().unwrap();
        } else {
            let e = result.unwrap_err();
            assert_eq!((e.phase, e.effect), (WalPhase::Stage, Effect::Unchanged));
            assert!(matches!(e.cause, WalCause::MappingMismatch));
            assert_eq!(session.prefix().unwrap(), before);
            assert_eq!(state.lock().unwrap().events.len(), calls);
            let state = session.wal.append.lock().unwrap();
            assert_eq!(state.state, SlotState::Idle);
            assert!(state.slot.as_ref().unwrap().iter().all(|&b| b == 0));
        }
    }
    let (writer, state) = fresh();
    let mut slot = slot();
    let mut init = [[0; 16384]; 3];
    let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
    let wrong = typed_frame(&[(0x100000002, 0)], 0, 1);
    let mut bad = wrong.clone();
    bad[128] ^= 1;
    assert!(matches!(
        session.try_stage(&bad).unwrap_err().cause,
        WalCause::Frame(WalError::Corrupt)
    ));
    let mut semantic = wrong.clone();
    semantic[7] = 2;
    seal_frame(&mut semantic);
    assert!(matches!(
        session.try_stage(&semantic).unwrap_err().cause,
        WalCause::Frame(WalError::UnsupportedFormat)
    ));
    session.try_stage(&raw_frame(0, 1, &[])).unwrap();
    for frame in [&bad, &wrong, &semantic] {
        assert!(matches!(
            session.try_stage(frame).unwrap_err().cause,
            WalCause::Busy
        ));
    }
    session.sync_pending().unwrap();
    let wrong = typed_frame(&[(0x100000002, 0)], 1, 2);
    let (pause, reached, release) = pause_at(0, false);
    std::thread::scope(|scope| {
        let session = &session;
        let pending = scope.spawn(move || {
            session.wal.stage_inner(
                &wrong,
                Some((FileId::new(0x100000001).unwrap(), 64)),
                || {
                    pause.entered.send(()).unwrap();
                    pause.release.recv_timeout(Duration::from_secs(5)).unwrap();
                },
            )
        });
        receive(&reached);
        session.try_stage(&raw_frame(1, 2, &[])).unwrap();
        session.sync_pending().unwrap();
        release.send(()).unwrap();
        assert!(matches!(
            pending.join().unwrap().unwrap_err().cause,
            WalCause::MappingMismatch
        ));
    });
    assert_eq!(
        (
            session.prefix().unwrap().synced_lsn,
            state.lock().unwrap().events.len()
        ),
        (2, 354)
    );
}

#[test]
fn bootstrap_buffer_preserves_shared_concurrency() {
    let (writer, state) = fresh();
    let mut slot = slot();
    let mut init = [[0; 16384]; 3];
    let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
    let mut root = [0; 16384];
    let mut scratch = [0; 16384];
    let mut load = [0; 16384];
    let mut frames = [ReadFrame::new(), ReadFrame::new()];
    let reader = session.open_reader(&mut root, &mut scratch).unwrap();
    let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
    drop(buffer.pin(identity(), PageKind::Allocation, 0).unwrap());
    let mut free = identity();
    free.page.page_no = 1;
    let reads = state.lock().unwrap().reads;
    let (pause, reached, release) = pause_at(reads + 2, false);
    state.lock().unwrap().read_pause = Some(pause);
    std::thread::scope(|scope| {
        let loading = scope.spawn(|| buffer.pin(free, PageKind::RowLeaf, 0).map(drop));
        receive(&reached);
        let (done, completed) = channel();
        let buffer = &buffer;
        let session = &session;
        let hit = scope.spawn(move || {
            let pin = buffer.pin(identity(), PageKind::Allocation, 0).unwrap();
            let guard = pin.read().unwrap();
            assert!(guard.bytes().as_slice() == literal(1, "allocation"));
            drop(guard);
            drop(pin);
            assert!(matches!(
                buffer.pin(free, PageKind::RowLeaf, 0),
                Err(BufferError::BufferBusy)
            ));
            session.try_stage(&raw_frame(0, 1, &[])).unwrap();
            session.sync_pending().unwrap();
            done.send(()).unwrap();
        });
        receive(&completed);
        release.send(()).unwrap();
        hit.join().unwrap();
        assert!(matches!(
            loading.join().unwrap(),
            Err(BufferError::Read(PageIoError::Page(_)))
        ));
    });
    assert!(!session.is_fenced());
    assert_eq!(
        buffer.test_frame_states(),
        vec![(2, 0, true), (0, 0, false)]
    );
    // Pause after the pin increment to force the final-check cancellation path.
    let (pause, reached, release) = pause_at(0, false);
    std::thread::scope(|scope| {
        let buffer = &buffer;
        let hit = scope.spawn(move || {
            buffer.test_hit_with_pause(identity(), || {
                pause.entered.send(()).unwrap();
                pause.release.recv_timeout(Duration::from_secs(5)).unwrap();
            })
        });
        receive(&reached);
        session.wal.fenced.fence();
        release.send(()).unwrap();
        assert!(matches!(hit.join().unwrap(), Err(BufferError::Fenced)));
    });
    assert_eq!(
        buffer.test_frame_states(),
        vec![(2, 0, true), (0, 0, false)]
    );
    assert!(!buffer.test_is_locally_fenced());
    let original = images(1);
    let actual = observed(&state);
    for name in [DATA, BANK0, BANK1, "FILEMAP"] {
        assert!(actual[name] == original[name]);
    }
}

fn assert_wal_fenced(session: &FreshStorageSession<'_, Writer>, state: &Arc<Mutex<State>>) {
    assert!(session.is_fenced());
    let effects = state.lock().unwrap().events.len();
    for (error, phase) in [
        (session.try_stage(&[]).unwrap_err(), WalPhase::Stage),
        (session.sync_pending().unwrap_err(), WalPhase::Write),
        (session.prefix().unwrap_err(), WalPhase::Observe),
    ] {
        assert_eq!(
            (error.phase, error.effect),
            (phase, Effect::CallerMustResolve)
        );
        assert!(matches!(error.cause, WalCause::Fenced));
    }
    assert_eq!(state.lock().unwrap().events.len(), effects);
}

#[test]
fn bootstrap_shared_fence_cleans_claims() {
    // A pre-existing shared reader rejects before any new mapping/data read.
    {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut output = [0xa5; 16384];
        let mut reader = session.open_reader(&mut root, &mut scratch).unwrap();
        let reads = state.lock().unwrap().reads;
        session.wal.fenced.fence();
        assert!(matches!(
            reader.read_page(identity(), PageKind::Allocation, 0, &mut output),
            Err(PageIoError::Fenced)
        ));
        assert_eq!(state.lock().unwrap().reads, reads);
        assert_eq!(output, [0xa5; 16384]);
        drop(reader);
        assert!(
            !session
                .reader_active
                .load(std::sync::atomic::Ordering::Acquire)
        );
    }
    // Force local poison after successful decode, before view's final checks.
    {
        let (writer, _) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let mut frames = [ReadFrame::new()];
        let reader = session.open_reader(&mut root, &mut scratch).unwrap();
        let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
        let pin = buffer.pin(identity(), PageKind::Allocation, 0).unwrap();
        let guard = pin.read().unwrap();
        let (entered, reached) = channel();
        let (release, resume) = channel();
        std::thread::scope(|scope| {
            let buffer = &buffer;
            let poison = scope.spawn(move || {
                receive(&reached);
                buffer.test_poison_header(0);
                assert!(matches!(
                    buffer.pin(identity(), PageKind::Allocation, 0),
                    Err(BufferError::Poisoned)
                ));
                release.send(()).unwrap();
            });
            assert!(matches!(
                guard.test_view_with_pause(|| {
                    entered.send(()).unwrap();
                    receive(&resume);
                }),
                Err(BufferError::Poisoned)
            ));
            poison.join().unwrap();
        });
        assert!(session.is_fenced());
        assert!(guard.bytes().as_slice() == literal(1, "allocation"));
    }
    // Three actual port failures while a successful allocation read is in flight.
    for mode in 0..3 {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let mut frames = [ReadFrame::new()];
        let reader = session.open_reader(&mut root, &mut scratch).unwrap();
        let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
        let reads = state.lock().unwrap().reads;
        let (pause, reached, release) = pause_at(reads + 2, false);
        state.lock().unwrap().read_pause = Some(pause);
        session.try_stage(&raw_frame(0, 1, &[])).unwrap();
        let at = state.lock().unwrap().events.len();
        state.lock().unwrap().plan = Some(Plan {
            at: at + usize::from(mode == 2),
            action: if mode == 1 {
                Action::AfterPanic
            } else {
                Action::BeforeError
            },
        });
        std::thread::scope(|scope| {
            let loading = scope.spawn(|| buffer.pin(identity(), PageKind::Allocation, 0).map(drop));
            receive(&reached);
            let result = catch_unwind(AssertUnwindSafe(|| session.sync_pending()));
            if mode == 1 {
                assert!(result.is_err());
            } else {
                let e = result.unwrap().unwrap_err();
                assert_eq!(
                    e.phase,
                    if mode == 2 {
                        WalPhase::Sync
                    } else {
                        WalPhase::Write
                    }
                );
                assert!(matches!(e.cause, WalCause::Io(_)));
            }
            release.send(()).unwrap();
            assert!(matches!(loading.join().unwrap(), Err(BufferError::Fenced)));
        });
        assert_eq!(buffer.test_frame_states(), vec![(0, 0, false)]);
        assert!(!buffer.test_is_locally_fenced());
        assert_wal_fenced(&session, &state);
        let reads = state.lock().unwrap().reads;
        let mut r = [0; 16384];
        let mut s = [0; 16384];
        assert!(matches!(
            session.open_reader(&mut r, &mut s),
            Err(PageIoError::Fenced)
        ));
        assert!(matches!(
            buffer.pin(identity(), PageKind::Allocation, 0),
            Err(BufferError::Fenced)
        ));
        assert_eq!(state.lock().unwrap().reads, reads);
        drop(buffer);
        assert!(
            !session
                .reader_active
                .load(std::sync::atomic::Ordering::Acquire)
        );
        drop(session);
        let s = state.lock().unwrap();
        assert_eq!((s.writers, s.handles), (0, 0));
    }
    // Acquired immutable bytes survive, but all new authority checks reject.
    {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let mut frames = [ReadFrame::new()];
        let reader = session.open_reader(&mut root, &mut scratch).unwrap();
        let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
        let pin = buffer.pin(identity(), PageKind::Allocation, 0).unwrap();
        let guard = pin.read().unwrap();
        session.try_stage(&raw_frame(0, 1, &[])).unwrap();
        state.lock().unwrap().plan = Some(Plan {
            at: 350,
            action: Action::BeforeError,
        });
        assert!(session.sync_pending().is_err());
        let reads = state.lock().unwrap().reads;
        assert!(matches!(
            buffer.pin(identity(), PageKind::Allocation, 0),
            Err(BufferError::Fenced)
        ));
        assert!(matches!(pin.read(), Err(BufferError::Fenced)));
        assert!(matches!(guard.view(), Err(BufferError::Fenced)));
        assert!(guard.bytes().as_slice() == literal(1, "allocation"));
        drop(guard);
        drop(pin);
        assert_eq!(buffer.test_frame_states(), vec![(2, 0, true)]);
        assert_eq!(state.lock().unwrap().reads, reads);
    }
    // Two buffer failures during an already-entered write: I/O may finish, publish may not.
    for mode in 0..2 {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let mut frames = [ReadFrame::new()];
        let reader = session.open_reader(&mut root, &mut scratch).unwrap();
        let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
        let frame = session.try_stage(&raw_frame(0, 1, &[])).unwrap();
        let (pause, reached, release) = pause_at(350, false);
        state.lock().unwrap().pause = Some(pause);
        std::thread::scope(|scope| {
            let writing = scope.spawn(|| session.sync_pending());
            receive(&reached);
            if mode == 0 {
                buffer.test_poison_header(0);
                assert!(matches!(
                    buffer.pin(identity(), PageKind::Allocation, 0),
                    Err(BufferError::Poisoned)
                ));
            } else {
                let reads = state.lock().unwrap().reads;
                state.lock().unwrap().read_plan = Some(Plan {
                    at: reads + 2,
                    action: Action::AfterPanic,
                });
                assert!(
                    catch_unwind(AssertUnwindSafe(|| buffer
                        .pin(identity(), PageKind::Allocation, 0)
                        .map(drop)))
                    .is_err()
                );
                assert_eq!(buffer.test_frame_states(), vec![(1, 0, false)]);
            }
            release.send(()).unwrap();
            let e = writing.join().unwrap().unwrap_err();
            assert_eq!(e.phase, WalPhase::Publish);
            assert!(matches!(e.cause, WalCause::Fenced));
        });
        assert!(buffer.test_is_locally_fenced());
        assert_wal_fenced(&session, &state);
        let append = session.wal.append.lock().unwrap();
        assert_eq!(append.state, SlotState::IoOwned(frame));
        assert!(append.slot.is_none());
        assert_eq!(
            (append.prefix.synced_offset, append.prefix.complete_offset),
            (4096, frame.end)
        );
    }
    // Exact protected interval for Idle, Complete and both I/O ownership cuts.
    for mode in 0..4 {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let frame = if mode == 0 {
            None
        } else {
            Some(session.try_stage(&raw_frame(0, 1, &[])).unwrap())
        };
        if mode < 2 {
            session.wal.fenced.fence();
        } else if mode == 2 {
            let file = session.wal.file.lock().unwrap();
            std::thread::scope(|scope| {
                let writing = scope.spawn(|| session.sync_pending());
                await_condition(|| {
                    session
                        .wal
                        .append
                        .try_lock()
                        .is_ok_and(|a| matches!(a.state, SlotState::IoOwned(_)))
                });
                session.wal.fenced.fence();
                drop(file);
                let e = writing.join().unwrap().unwrap_err();
                assert_eq!(e.phase, WalPhase::Write);
                assert!(matches!(e.cause, WalCause::Fenced));
            });
        } else {
            let (pause, reached, release) = pause_at(350, false);
            state.lock().unwrap().pause = Some(pause);
            std::thread::scope(|scope| {
                let writing = scope.spawn(|| session.sync_pending());
                receive(&reached);
                session.wal.fenced.fence();
                release.send(()).unwrap();
                let e = writing.join().unwrap().unwrap_err();
                assert_eq!(e.phase, WalPhase::Publish);
                assert!(matches!(e.cause, WalCause::Fenced));
            });
        }
        assert_wal_fenced(&session, &state);
        assert_eq!(
            state.lock().unwrap().events.len(),
            if mode == 3 { 352 } else { 350 }
        );
        let append = session.wal.append.lock().unwrap();
        assert_eq!(append.prefix.synced_offset, 4096);
        assert_eq!(append.prefix.complete_offset, frame.map_or(4096, |f| f.end));
        assert_eq!(append.slot.is_some(), mode < 2);
        assert_eq!(
            append.state,
            match mode {
                0 => SlotState::Idle,
                1 => SlotState::Complete(frame.unwrap()),
                _ => SlotState::IoOwned(frame.unwrap()),
            }
        );
    }
    // Invalid-capacity admission releases a healthy lease; fence wins over capacity.
    for fenced in [false, true] {
        let (writer, _) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let reader = session.open_reader(&mut root, &mut scratch).unwrap();
        if fenced {
            session.wal.fenced.fence();
        }
        let error = ReadBuffer::new(reader, &mut [], &mut load).err().unwrap();
        assert!(if fenced {
            matches!(error, BufferError::Fenced)
        } else {
            matches!(error, BufferError::InvalidCapacity)
        });
        assert_eq!(session.is_fenced(), fenced);
        assert!(
            !session
                .reader_active
                .load(std::sync::atomic::Ordering::Acquire)
        );
    }
    // A poisoned workspace keeps Poisoned precedence even if the new session is fenced.
    let mut frames = [ReadFrame::new()];
    {
        let (writer, _) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut load = [0; 16384];
        let reader = session.open_reader(&mut root, &mut scratch).unwrap();
        let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
        buffer.test_poison_header(0);
    }
    let (writer, _) = fresh();
    let mut slot = slot();
    let mut init = [[0; 16384]; 3];
    let session = FreshStorageSession::create(writer, 1, &mut slot, &mut init).unwrap();
    let mut root = [0; 16384];
    let mut scratch = [0; 16384];
    let mut load = [0; 16384];
    let reader = session.open_reader(&mut root, &mut scratch).unwrap();
    session.wal.fenced.fence();
    assert!(matches!(
        ReadBuffer::new(reader, &mut frames, &mut load),
        Err(BufferError::Poisoned)
    ));
}

fn rejects_large_owner<const O: usize, const F: usize>() {
    let (writer, state) = fresh();
    let writer = SizedWriter::<O, F> {
        inner: writer,
        padding: [0; O],
    };
    let mut slot = slot();
    let mut init = [[0; 16384]; 3];
    let e = FreshStorageSession::create(writer, 1, &mut slot, &mut init)
        .err()
        .unwrap();
    assert_eq!((e.phase, e.effect), (Phase::Preflight, Effect::Unchanged));
    assert!(matches!(e.cause, Cause::InvalidInput));
    let s = state.lock().unwrap();
    assert!(s.events.is_empty());
    assert_eq!((s.reads, s.handles, s.writers), (0, 0, 0));
}

#[test]
fn bootstrap_workspace_and_handles_are_bounded() {
    use std::mem::size_of;
    let sizes = [
        ("fresh", size_of::<FreshStorageSession<'_, Writer>>()),
        ("genesis", size_of::<GenesisWalSession<'_, Writer>>()),
        ("file", size_of::<File>()),
        ("plan", size_of::<BootstrapPlan>()),
        ("names", size_of::<InstallNames>()),
        ("entries", size_of::<[AllocationEntry; 64]>()),
        ("reader", size_of::<PageFileReader<'_, Writer>>()),
        ("cursor", size_of::<control::FileMapCursor<'_>>()),
        ("page_view", size_of::<crate::storage::page::PageView<'_>>()),
        ("descriptor", size_of::<ChunkDescriptor>()),
        ("profile", size_of::<Profile>()),
        ("frame", size_of::<ReadFrame>()),
    ];
    for (name, bytes) in sizes {
        println!("STORAGE_TYPE {name} {bytes}");
    }
    assert!(size_of::<FreshStorageSession<'_, Writer>>() <= 16384);
    assert!(size_of::<[AllocationEntry; 64]>() <= 8192);
    assert!(size_of::<InstallNames>() <= 1032);
    assert_eq!(size_of::<[[u8; 16384]; 3]>(), 49152);
    assert_eq!(
        (FIXED_BYTES, MAX_OWNED_BYTES, PROTECTED_BYTES),
        (5296128, 273727488, 4247552)
    );
    rejects_large_owner::<16384, 0>();
    rejects_large_owner::<0, 16384>();
    for profile in 1..=3 {
        let (writer, state) = fresh();
        let mut slot = slot();
        let mut init = [[0; 16384]; 3];
        let session = FreshStorageSession::create(writer, profile, &mut slot, &mut init).unwrap();
        assert_eq!(
            (
                state.lock().unwrap().handles,
                session.plan.profile.spec().id
            ),
            (1, profile)
        );
        let mut root = [0; 16384];
        let mut scratch = [0; 16384];
        let mut output = [0; 16384];
        for _ in 0..128 {
            let mut reader = session.open_reader(&mut root, &mut scratch).unwrap();
            assert_eq!(state.lock().unwrap().handles, 2);
            reader
                .read_page(identity(), PageKind::Allocation, 0, &mut output)
                .unwrap();
            drop(reader);
            assert_eq!(state.lock().unwrap().handles, 1);
        }
        let s = state.lock().unwrap();
        assert_eq!(s.peak, 3);
        assert_eq!(s.events.len(), 350);
        drop(s);
        drop(session);
        let s = state.lock().unwrap();
        assert_eq!((s.handles, s.writers, s.events.len()), (0, 0, 350));
    }
}
