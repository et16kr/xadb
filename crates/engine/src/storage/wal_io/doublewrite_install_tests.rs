use super::super::test_support::{
    self as port, Action, Entry, Event, Plan, SizedWriter, State, Writer,
};
use super::*;
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-doublewrite-install"
);
fn input(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}
fn literal(profile: u32, name: &str) -> Vec<u8> {
    input(&format!("literal/profile{profile}-{name}.bin"))
}
fn workspace() -> Box<[u8; WORKSPACE_BYTES]> {
    vec![0xa5; WORKSPACE_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn new_session<'a>(
    profile: u32,
    slot: &'a mut [u8; 1048576],
) -> (FreshStorageSession<'a, Writer>, Arc<Mutex<State>>) {
    let (writer, state) = port::fresh();
    let session =
        FreshStorageSession::create(writer, profile, slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
    reset(&state);
    (session, state)
}
fn reset(state: &Arc<Mutex<State>>) {
    let mut state = state.lock().unwrap();
    state.events.clear();
    state.read_events.clear();
    state.read_effect_counts.clear();
    state.reads = 0;
    state.open_modes.clear();
    state.peak = state.handles;
    state.hit = 0;
}
fn observed(state: &Arc<Mutex<State>>) -> BTreeMap<String, Vec<u8>> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, entry)| {
            assert!(entry.sparse_tail.is_empty());
            assert_eq!(entry.bytes.len() as u64, entry.length);
            (name.clone(), entry.bytes.clone())
        })
        .collect()
}
fn assert_files(state: &Arc<Mutex<State>>, expected: &BTreeMap<String, Vec<u8>>) {
    let state = state.lock().unwrap();
    assert_eq!(state.files.len(), expected.len());
    for (name, bytes) in expected {
        let actual = &state.files[name];
        assert_eq!(actual.length, bytes.len() as u64, "{name}");
        assert!(actual.sparse_tail.is_empty());
        assert!(actual.bytes == *bytes, "file bytes differ: {name}");
    }
}
fn installed(profile: u32, old: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let mut files = old.clone();
    files.insert("DWB".into(), literal(profile, "DWB"));
    files.insert("d3-dwb-stage".into(), literal(profile, "d3-dwb-stage"));
    let mut bank0 = vec![0; 16384];
    bank0[..160].copy_from_slice(&literal(profile, "empty"));
    files.insert("d3-dwb-0".into(), bank0);
    files.insert("d3-dwb-1".into(), vec![0; 16384]);
    files.insert("d3-dwb-reserved".into(), vec![0; 8192]);
    files.insert("d3-dwb-images".into(), vec![0; 16711680]);
    files
}
fn expected_events(profile: u32) -> Vec<Event> {
    let files = installed(profile, &BTreeMap::new());
    String::from_utf8(input("effects.tsv"))
        .unwrap()
        .lines()
        .enumerate()
        .map(|(i, row)| {
            let v: Vec<_> = row.split('\t').collect();
            assert_eq!(v[0].parse::<usize>().unwrap(), i + 1);
            let kind = match v[1] {
                "create" => "create",
                "write" => "write",
                "sync" => "sync",
                "directory_sync" => "dirsync",
                "publish" => "publish",
                _ => panic!("unknown effect"),
            };
            let offset = v[3].parse::<u64>().unwrap();
            let length = v[4].parse::<usize>().unwrap();
            let source = if v[2] == "d3-dwb-select-stage" {
                "DWB"
            } else {
                v[2]
            };
            Event {
                kind,
                name: v[2].into(),
                offset,
                length,
                data: if kind == "write" {
                    files[source][offset as usize..offset as usize + length].to_vec()
                } else {
                    Vec::new()
                },
                returned: if kind == "write" { Some(length) } else { None },
            }
        })
        .collect()
}
fn observations() -> Vec<(String, String, u64, usize)> {
    String::from_utf8(input("observations.tsv"))
        .unwrap()
        .lines()
        .enumerate()
        .map(|(i, row)| {
            let v: Vec<_> = row.split('\t').collect();
            assert_eq!(v[0].parse::<usize>().unwrap(), i + 1);
            (
                v[1].into(),
                v[2].into(),
                v[3].parse().unwrap(),
                v[4].parse().unwrap(),
            )
        })
        .collect()
}
fn phase(index: usize) -> Phase {
    use Phase::*;
    [
        DescriptorCreate,
        DescriptorWrite,
        DescriptorSync,
        DescriptorDirectory,
        ImagesCreate,
        ImagesWrite,
        ImagesWrite,
        ImagesWrite,
        ImagesWrite,
        ImagesSync,
        Bank0Create,
        Bank0Write,
        Bank0Sync,
        Bank1Create,
        Bank1Write,
        Bank1Sync,
        ReservedCreate,
        ReservedWrite,
        ReservedSync,
        FilesDirectory,
        SelectorCreate,
        SelectorWrite,
        SelectorSync,
        SelectorPublish,
        SelectorDirectory,
    ][index]
}
fn expected_reads(profile: u32, old: &BTreeMap<String, Vec<u8>>) -> Vec<Event> {
    let files = installed(profile, old);
    observations()
        .into_iter()
        .map(|(kind, name, offset, length)| {
            let kind = match kind.as_str() {
                "read" => "read",
                "metadata" => "metadata",
                "open" => "open",
                _ => panic!("unknown observation"),
            };
            let data = if kind == "read" {
                files[&name][offset as usize..offset as usize + length].to_vec()
            } else {
                Vec::new()
            };
            Event {
                kind,
                name,
                offset,
                length,
                data,
                returned: if kind == "read" { Some(length) } else { None },
            }
        })
        .collect()
}
fn assert_trace(actual: &[Event], expected: &[Event]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            actual == expected,
            "event {index} differs in identity, request, bytes or return"
        );
    }
}
fn prefix(
    old: &BTreeMap<String, Vec<u8>>,
    events: &[Event],
    count: usize,
) -> BTreeMap<String, Vec<u8>> {
    let mut result = old.clone();
    for e in &events[..count] {
        match e.kind {
            "create" => {
                assert!(result.insert(e.name.clone(), Vec::new()).is_none());
            }
            "write" => {
                let file = result.get_mut(&e.name).unwrap();
                let offset = e.offset as usize;
                file.resize(offset + e.length, 0);
                file[offset..].copy_from_slice(&e.data);
            }
            "publish" => {
                let bytes = result.remove(&e.name).unwrap();
                assert!(result.insert("DWB".into(), bytes).is_none());
            }
            "sync" | "dirsync" => {}
            _ => panic!("unknown effect"),
        }
    }
    result
}
fn assert_header(error: &DoublewriteInstallError, phase: Phase, attempted: u8, completed: u8) {
    assert_eq!(
        (error.phase, error.effect, error.attempted, error.completed),
        (phase, Effect::CallerMustResolve, attempted, completed)
    );
}
fn assert_closed(state: &Arc<Mutex<State>>) {
    let s = state.lock().unwrap();
    assert_eq!((s.writers, s.handles), (0, 0));
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn doublewrite_install_literal_profiles() {
    for profile in 1..=3 {
        let mut slot = port::slot();
        let mut work = workspace();
        let (fresh, state) = new_session(profile, &mut slot);
        let old = observed(&state);
        let old_ids: Vec<_> = state
            .lock()
            .unwrap()
            .files
            .iter()
            .map(|(n, e)| (n.clone(), e.identity))
            .collect();
        let expected = expected_events(profile);
        let owner = fresh.install_doublewrite(&mut work).unwrap();
        assert_files(&state, &installed(profile, &old));
        {
            let s = state.lock().unwrap();
            assert_eq!(s.events, expected);
            assert_eq!(s.events.len(), 25);
            let reads: Vec<_> = s
                .read_events
                .iter()
                .map(|e| (e.kind.to_string(), e.name.clone(), e.offset, e.length))
                .collect();
            assert_eq!(reads, observations());
            assert_eq!(reads.len(), 32);
            assert!(s.open_modes.iter().all(|mode| *mode == OpenMode::ReadOnly));
            assert_eq!((s.handles, s.peak), (1, 2));
            for (name, id) in old_ids {
                assert_eq!(s.files[&name].identity, id);
            }
            for (i, e) in s.events.iter().enumerate() {
                println!(
                    "DWB_INSTALL_EFFECT profile={profile} index={} op={} name={} offset={} length={} sha256={}",
                    i + 1,
                    e.kind,
                    if e.name.is_empty() { "-" } else { &e.name },
                    e.offset,
                    e.length,
                    hex(&content_digest(&e.data))
                );
            }
            for (i, e) in s.read_events.iter().enumerate() {
                let phase = match s.read_effect_counts[i] {
                    0 => "namespace_preflight",
                    25 => "readback",
                    _ => panic!("observation during unexpected mutation prefix"),
                };
                println!(
                    "DWB_INSTALL_OBSERVATION profile={profile} index={} phase={phase} op={} name={} offset={} length={}",
                    i + 1,
                    e.kind,
                    e.name,
                    e.offset,
                    e.length
                );
            }
        }
        let prefix = owner.fresh.prefix().unwrap();
        assert_eq!(
            (
                prefix.complete_lsn,
                prefix.synced_lsn,
                prefix.complete_offset,
                prefix.synced_offset
            ),
            (0, 0, 4096, 4096)
        );
        assert!(!owner.is_fenced());
        let mut root = [0; PAGE_BYTES];
        let mut scratch = [0; PAGE_BYTES];
        let mut second_root = [0; PAGE_BYTES];
        let mut second_scratch = [0; PAGE_BYTES];
        let reader = owner.open_reader(&mut root, &mut scratch).unwrap();
        assert!(matches!(
            owner.open_reader(&mut second_root, &mut second_scratch),
            Err(PageIoError::Busy)
        ));
        drop(reader);
        drop(owner.open_reader(&mut root, &mut scratch).unwrap());
        owner.fresh.core.wal.fenced.fence();
        let reads = state.lock().unwrap().reads;
        assert!(owner.is_fenced());
        assert!(matches!(
            owner.open_reader(&mut root, &mut scratch),
            Err(PageIoError::Fenced)
        ));
        assert_eq!(state.lock().unwrap().reads, reads);
        drop(owner);
        assert_closed(&state);
    }
}

#[test]
fn doublewrite_install_pristine_preflight() {
    for case in 0..13 {
        let mut slot = port::slot();
        let mut work = workspace();
        let (mut fresh, state) = new_session(1, &mut slot);
        let mut root = [0; PAGE_BYTES];
        let mut scratch = [0; PAGE_BYTES];
        match case {
            0 | 1 => {
                fresh.try_stage(&port::raw_frame(0, 1, &[])).unwrap();
                if case == 1 {
                    fresh.sync_pending().unwrap();
                }
            }
            2 => {
                fresh.core.wal.append.get_mut().unwrap().slot.take();
            }
            3 => fresh.core.wal.append.get_mut().unwrap().prefix.complete_lsn = 1,
            4 => fresh.core.wal.append.get_mut().unwrap().prefix.synced_lsn = 1,
            5 => {
                fresh
                    .core
                    .wal
                    .append
                    .get_mut()
                    .unwrap()
                    .prefix
                    .complete_offset = 4097
            }
            6 => {
                fresh
                    .core
                    .wal
                    .append
                    .get_mut()
                    .unwrap()
                    .prefix
                    .synced_offset = 4097
            }
            7 => {
                fresh
                    .core
                    .wal
                    .append
                    .get_mut()
                    .unwrap()
                    .prefix
                    .stream
                    .writer_epoch = 2
            }
            8 => fresh.core.wal.fenced.fence(),
            9 => {
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _lock = fresh.core.wal.append.lock().unwrap();
                        panic!("poison append");
                    }))
                    .is_err()
                );
            }
            10 => {
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _lock = fresh.core.wal.file.lock().unwrap();
                        panic!("poison file");
                    }))
                    .is_err()
                );
            }
            11 => {
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _lock = fresh.core.wal._writer.lock().unwrap();
                        panic!("poison writer");
                    }))
                    .is_err()
                );
            }
            12 => std::mem::forget(fresh.open_reader(&mut root, &mut scratch).unwrap()),
            _ => unreachable!(),
        }
        reset(&state);
        let old = observed(&state);
        let error = fresh.install_doublewrite(&mut work).err().unwrap();
        assert_header(&error, Phase::StatePreflight, 0, 0);
        assert!(match case {
            0..=7 => matches!(error.cause, Cause::InvalidState),
            8 => matches!(error.cause, Cause::Fenced),
            9..=11 => matches!(error.cause, Cause::Poisoned),
            12 => matches!(error.cause, Cause::Busy),
            _ => false,
        });
        assert_files(&state, &old);
        let s = state.lock().unwrap();
        assert!(s.events.is_empty() && s.read_events.is_empty());
        assert_eq!(s.writers, 0);
        assert_eq!(s.handles, usize::from(case == 12));
    }
    // This port genuinely constructs Fresh below the cap; only the new returned
    // owner crosses it. Rejecting Fresh itself would not exercise this preflight.
    const PAD: usize = 16384 - std::mem::size_of::<FreshStorageSession<'static, Writer>>() - 64;
    type Large = SizedWriter<PAD, 0>;
    assert!(std::mem::size_of::<FreshStorageSession<'_, Large>>() <= 16384);
    assert!(std::mem::size_of::<DoublewriteStorageSession<'_, '_, Large>>() > 16384);
    let (writer, state) = port::fresh();
    let mut slot = port::slot();
    let mut work = workspace();
    let fresh = FreshStorageSession::create(
        Large {
            inner: writer,
            padding: [0; PAD],
        },
        1,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    reset(&state);
    let old = observed(&state);
    let error = fresh.install_doublewrite(&mut work).err().unwrap();
    assert_header(&error, Phase::StatePreflight, 0, 0);
    assert!(matches!(error.cause, Cause::Capacity));
    assert_files(&state, &old);
    assert!(state.lock().unwrap().events.is_empty());
    assert!(state.lock().unwrap().read_events.is_empty());
    assert_closed(&state);
}

#[test]
fn doublewrite_install_namespace_preflight() {
    for (i, name) in [
        "DWB",
        "d3-dwb-stage",
        "d3-dwb-images",
        "d3-dwb-0",
        "d3-dwb-1",
        "d3-dwb-reserved",
        "d3-dwb-select-stage",
    ]
    .iter()
    .enumerate()
    {
        let mut slot = port::slot();
        let mut work = workspace();
        let (fresh, state) = new_session(1, &mut slot);
        let mut entry = Entry::new(999);
        entry.bytes = vec![0x5a];
        entry.length = 1;
        state.lock().unwrap().files.insert((*name).into(), entry);
        let old = observed(&state);
        let error = fresh.install_doublewrite(&mut work).err().unwrap();
        assert_header(&error, Phase::NamespacePreflight, 0, 0);
        assert!(matches!(error.cause, Cause::Residue));
        assert_files(&state, &old);
        assert!(state.lock().unwrap().events.is_empty());
        assert_eq!(state.lock().unwrap().reads, i + 1);
        assert_closed(&state);
    }
}

#[test]
fn doublewrite_install_effect_failures() {
    let expected = expected_events(1);
    for at in 0..25 {
        for after in [false, true] {
            let mut slot = port::slot();
            let mut work = workspace();
            let (fresh, state) = new_session(1, &mut slot);
            let old = observed(&state);
            state.lock().unwrap().plan = Some(Plan {
                at,
                action: if after {
                    Action::AfterError
                } else {
                    Action::BeforeError
                },
            });
            let error = fresh.install_doublewrite(&mut work).err().unwrap();
            assert_header(&error, phase(at), (at + 1) as u8, at as u8);
            match (&error.cause, expected[at].kind) {
                (Cause::Io(e), "write" | "sync") => {
                    assert_eq!(e.code, foundation::ErrorCode::IoWriteUnknown);
                    assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
                    assert_eq!(e.effect(), Effect::CallerMustResolve);
                }
                (Cause::Namespace(e), _) => {
                    assert_eq!(e.kind, foundation::namespace::NamespaceErrorKind::Io);
                    assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
                    assert_eq!(
                        e.effect,
                        if after || expected[at].kind == "dirsync" {
                            Effect::CallerMustResolve
                        } else {
                            Effect::Unchanged
                        }
                    );
                }
                _ => panic!("wrong lower cause {error:?}"),
            }
            let s = state.lock().unwrap();
            assert_eq!(s.hit, 1);
            assert_eq!(s.events.len(), at + 1);
            assert_eq!(s.reads, 7);
            for (a, e) in s.events.iter().zip(&expected) {
                assert_eq!(
                    (a.kind, &a.name, a.offset, a.length),
                    (e.kind, &e.name, e.offset, e.length)
                );
            }
            drop(s);
            assert_files(&state, &prefix(&old, &expected, at + usize::from(after)));
            assert_closed(&state);
        }
    }
}

#[test]
fn doublewrite_install_effect_unwinds() {
    let expected = expected_events(1);
    for at in 0..25 {
        for after in [false, true] {
            let mut slot = port::slot();
            let mut work = workspace();
            let (fresh, state) = new_session(1, &mut slot);
            let old = observed(&state);
            state.lock().unwrap().plan = Some(Plan {
                at,
                action: if after {
                    Action::AfterPanic
                } else {
                    Action::BeforePanic
                },
            });
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = fresh.install_doublewrite(&mut work);
                }))
                .is_err()
            );
            assert_eq!(state.lock().unwrap().hit, 1);
            assert_eq!(state.lock().unwrap().events.len(), at + 1);
            assert_files(&state, &prefix(&old, &expected, at + usize::from(after)));
            assert_closed(&state);
        }
    }
}

#[test]
fn doublewrite_install_short_io() {
    let effects = expected_events(1);
    let reads = observations();
    let mut counts = [0, 0];
    for reading in [false, true] {
        let targets: Vec<_> = if reading {
            reads
                .iter()
                .enumerate()
                .filter(|(_, r)| r.0 == "read")
                .map(|(i, r)| (i, r.2, r.3))
                .collect()
        } else {
            effects
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == "write")
                .map(|(i, e)| (i, e.offset, e.length))
                .collect()
        };
        for (at, offset, length) in targets {
            for kind in 0..if reading { 7 } else { 6 } {
                counts[usize::from(reading)] += 1;
                let action = match kind {
                    0 => Action::Short(1),
                    1 => Action::Short(length / 2),
                    2 => Action::Interrupted,
                    3 => Action::Zero,
                    4 => Action::InvalidCount,
                    5 => Action::PrefixError,
                    6 => Action::PrefixEof,
                    _ => unreachable!(),
                };
                let mut slot = port::slot();
                let mut work = workspace();
                let (fresh, state) = new_session(1, &mut slot);
                let old = observed(&state);
                if reading {
                    state.lock().unwrap().read_plan = Some(Plan { at, action });
                } else {
                    state.lock().unwrap().plan = Some(Plan { at, action });
                }
                let result = fresh.install_doublewrite(&mut work);
                if kind < 3 {
                    drop(result.unwrap());
                    assert_files(&state, &installed(1, &old));
                } else {
                    let error = result.err().unwrap();
                    assert_header(
                        &error,
                        if reading { Phase::Readback } else { phase(at) },
                        if reading { 25 } else { (at + 1) as u8 },
                        if reading { 25 } else { at as u8 },
                    );
                    let Cause::Io(io) = error.cause else {
                        panic!("wrong short-I/O cause")
                    };
                    assert_eq!(
                        io.code,
                        if reading {
                            foundation::ErrorCode::IoRead
                        } else {
                            foundation::ErrorCode::IoWriteUnknown
                        }
                    );
                    assert_eq!(
                        io.effect(),
                        if reading {
                            Effect::Unchanged
                        } else {
                            Effect::CallerMustResolve
                        }
                    );
                    assert_eq!(
                        io.io_kind,
                        match kind {
                            3 | 6 => Some(if reading {
                                std::io::ErrorKind::UnexpectedEof
                            } else {
                                std::io::ErrorKind::WriteZero
                            }),
                            4 => None,
                            5 => Some(std::io::ErrorKind::Other),
                            _ => unreachable!(),
                        }
                    );
                    let mut expected = if reading {
                        installed(1, &old)
                    } else {
                        prefix(&old, &effects, at)
                    };
                    if !reading && kind == 5 {
                        let e = &effects[at];
                        let file = expected.get_mut(&e.name).unwrap();
                        file.resize(e.offset as usize + length / 2, 0);
                        file[e.offset as usize..].copy_from_slice(&e.data[..length / 2]);
                    }
                    assert_files(&state, &expected);
                }
                let s = state.lock().unwrap();
                let trace = if reading { &s.read_events } else { &s.events };
                if reading {
                    assert_eq!(s.configured_read_hits, 1);
                    assert_eq!(s.continuation_read_hits, usize::from(kind >= 5));
                }
                if kind < 3 {
                    let mut complete = if reading {
                        expected_reads(1, &old)
                    } else {
                        effects.clone()
                    };
                    let original = complete.remove(at);
                    let advance = match kind {
                        0 => 1,
                        1 => length / 2,
                        2 => 0,
                        _ => unreachable!(),
                    };
                    let mut first = original.clone();
                    first.returned = if kind == 2 { None } else { Some(advance) };
                    if reading {
                        first.data.truncate(advance);
                    }
                    let mut second = original;
                    second.offset += advance as u64;
                    second.length -= advance;
                    second.data.drain(..advance);
                    second.returned = Some(second.length);
                    complete.insert(at, second);
                    complete.insert(at, first);
                    assert_trace(trace, &complete);
                    assert_eq!(trace.len(), if reading { 33 } else { 26 });
                }
                assert_eq!(s.hit, if reading && kind >= 5 { 2 } else { 1 });
                if kind < 3 || reading && kind >= 5 {
                    let advance = match kind {
                        0 => 1,
                        1 | 5 | 6 => length / 2,
                        _ => 0,
                    };
                    assert_eq!(
                        (trace[at + 1].offset, trace[at + 1].length),
                        (offset + advance as u64, length - advance)
                    );
                    assert_eq!(trace[at + 1].name, trace[at].name);
                    if kind != 2 {
                        assert_eq!(trace[at].returned, Some(advance));
                    }
                }
                if kind >= 3 {
                    assert_eq!(trace.len(), at + 1 + usize::from(reading && kind >= 5));
                }
                drop(s);
                assert_closed(&state);
            }
        }
    }
    assert_eq!(counts, [54, 70]);
}

#[test]
fn doublewrite_install_observation_failures() {
    let expected = observations();
    for (at, row) in expected.iter().enumerate() {
        for panic in [false, true] {
            let mut slot = port::slot();
            let mut work = workspace();
            let (fresh, state) = new_session(1, &mut slot);
            let old = observed(&state);
            state.lock().unwrap().read_plan = Some(Plan {
                at,
                action: if panic {
                    Action::BeforePanic
                } else {
                    Action::BeforeError
                },
            });
            if panic {
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _ = fresh.install_doublewrite(&mut work);
                    }))
                    .is_err()
                );
            } else {
                let e = fresh.install_doublewrite(&mut work).err().unwrap();
                let count = if at < 7 { 0 } else { 25 };
                assert_header(
                    &e,
                    if at < 7 {
                        Phase::NamespacePreflight
                    } else {
                        Phase::Readback
                    },
                    count,
                    count,
                );
                match e.cause {
                    Cause::Io(e) => {
                        assert_eq!(row.0, "read");
                        assert_eq!(e.code, foundation::ErrorCode::IoRead);
                        assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
                        assert_eq!(e.effect(), Effect::Unchanged);
                    }
                    Cause::Namespace(e) => {
                        assert_ne!(row.0, "read");
                        assert_eq!(e.kind, foundation::namespace::NamespaceErrorKind::Io);
                        assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
                        assert_eq!(e.effect, Effect::Unchanged);
                    }
                    _ => panic!("wrong observation failure"),
                }
            }
            assert_eq!(state.lock().unwrap().hit, 1);
            assert_eq!(state.lock().unwrap().reads, at + 1);
            assert_eq!(
                state.lock().unwrap().events.len(),
                if at < 7 { 0 } else { 25 }
            );
            assert_files(
                &state,
                &if at < 7 {
                    old.clone()
                } else {
                    installed(1, &old)
                },
            );
            assert_closed(&state);
        }
    }
}

fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn category(cause: &Cause) -> String {
    match cause {
        Cause::Control(e) => format!("Control({e:?})"),
        other => format!("{other:?}"),
    }
}

#[test]
fn doublewrite_install_readback_rejects() {
    let rows = String::from_utf8(input("semantic-expectations.tsv")).unwrap();
    let mut count = 0;
    for line in rows.lines() {
        count += 1;
        let row: Vec<_> = line.split('\t').collect();
        let action = row[0];
        let name = row[1];
        let offset: usize = row[2].parse().unwrap();
        let at: usize = row[3].parse::<usize>().unwrap() - 1;
        let mut slot = port::slot();
        let mut work = workspace();
        let (fresh, state) = new_session(1, &mut slot);
        let old = observed(&state);
        let mut expected = installed(1, &old);
        match action {
            "open_length_minus1" | "open_length_plus1" => {
                state.lock().unwrap().length_delta =
                    Some((at, if action.ends_with("minus1") { -1 } else { 1 }))
            }
            "named_inode_changed" => state.lock().unwrap().metadata_change = Some((at, true)),
            "late_stage_residue" => {
                state.lock().unwrap().read_insert = Some((at, name.into(), Entry::new(999)));
                expected.insert(name.into(), Vec::new());
            }
            _ => {
                let bytes = expected.get_mut(name).unwrap();
                match action {
                    "xor_1_before_corresponding_read" => bytes[offset] ^= 1,
                    "profile2_with_same_namespace" => {
                        *bytes = std::fs::read(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/tests/fixtures/dev3-storage-session/literal/profile2-d3-profile.bin"
                        ))
                        .unwrap()
                    }
                    "descriptor_other_namespace_resealed" => {
                        bytes[16] ^= 1;
                        seal(bytes);
                    }
                    "initial_empty_batch1_resealed" => {
                        bytes[40..48].copy_from_slice(&1u64.to_le_bytes());
                        seal(&mut bytes[..160]);
                    }
                    "selector_other_generation_resealed" => {
                        bytes[32..40].copy_from_slice(&2u64.to_le_bytes());
                        seal(bytes);
                    }
                    _ => panic!("unknown fixed semantic case"),
                }
                if action != "xor_1_before_corresponding_read" {
                    let logical = if name == "d3-dwb-0" { 160 } else { bytes.len() };
                    assert_eq!(
                        &bytes[logical - 32..logical],
                        content_digest(&bytes[..logical - 32])
                    );
                }
                state
                    .lock()
                    .unwrap()
                    .read_edits
                    .push((at, name.into(), bytes.clone()));
            }
        }
        let error = fresh.install_doublewrite(&mut work).err().unwrap();
        assert_header(&error, Phase::Readback, 25, 25);
        assert_eq!(category(&error.cause), row[4], "{action} {name} {offset}");
        assert_eq!(state.lock().unwrap().reads, at + 1);
        assert_eq!(state.lock().unwrap().events.len(), 25);
        assert_files(&state, &expected);
        assert_closed(&state);
    }
    assert_eq!(count, 47);
}

#[test]
fn doublewrite_install_descriptor_and_workspace() {
    for profile in 1..=3 {
        let mut slot = port::slot();
        let mut work = workspace();
        let (fresh, state) = new_session(profile, &mut slot);
        let frozen = literal(profile, "d3-dwb-stage");
        let plan = InstallPlan::prepare(&fresh.core.plan, &mut work).unwrap();
        assert_eq!(&work[..16384], frozen);
        assert_eq!(plan.descriptor_digest, content_digest(&frozen));
        for length in [0, 16383] {
            let mut short = vec![0xa5; length];
            assert!(matches!(
                plan.encode_descriptor(&mut short),
                Err(Cause::Capacity)
            ));
            assert!(short.iter().all(|b| *b == 0xa5));
        }
        let mut tail = vec![0xa5; 16385];
        assert_eq!(plan.encode_descriptor(&mut tail).unwrap(), frozen);
        assert_eq!(tail[16384], 0xa5);
        work.fill(0);
        let empty = encode_empty(plan.profile, &mut work[..]).unwrap();
        assert_eq!(empty, literal(profile, "empty"));
        assert_eq!(content_digest(empty), plan.selected.reference.digest);
        let selector = control::encode_selector(
            SelectorRole::Doublewrite,
            plan.selected,
            plan.profile,
            &mut work[..],
        )
        .unwrap();
        assert_eq!(selector, literal(profile, "DWB"));
        assert_eq!(content_digest(selector), plan.selector_digest);
        let mut reference = [0; 64];
        plan.selected.reference.encode(&mut reference).unwrap();
        assert_eq!(reference.to_vec(), literal(profile, "reference"));
        assert_eq!(literal(profile, "d3-dwb-stage"), frozen);
        assert!(state.lock().unwrap().events.is_empty());
        drop(fresh);
        assert_closed(&state);
    }
}

#[test]
fn doublewrite_install_resource_and_drop() {
    let owner_bytes = std::mem::size_of::<DoublewriteStorageSession<'_, '_, Writer>>();
    let plan_bytes = std::mem::size_of::<InstallPlan>();
    let file_bytes = std::mem::size_of::<port::File>();
    let names_bytes = std::mem::size_of::<Names>();
    assert!(
        owner_bytes <= 16384 && plan_bytes <= 4096 && file_bytes <= 16384 && names_bytes <= 16384
    );
    println!(
        "DWB_INSTALL_TYPES owner={owner_bytes} plan={plan_bytes} file={file_bytes} names={names_bytes} component_reserve=65536 workspace=4194304"
    );
    for profile in 1..=3 {
        let mut slot = port::slot();
        let mut work = workspace();
        let (fresh, state) = new_session(profile, &mut slot);
        let owner = fresh.install_doublewrite(&mut work).unwrap();
        assert_eq!(state.lock().unwrap().peak, 2);
        assert_eq!(state.lock().unwrap().handles, 1);
        let mut root = [0; PAGE_BYTES];
        let mut scratch = [0; PAGE_BYTES];
        for _ in 0..128 {
            let reader = owner.open_reader(&mut root, &mut scratch).unwrap();
            assert_eq!(state.lock().unwrap().handles, 2);
            drop(reader);
            assert_eq!(state.lock().unwrap().handles, 1);
        }
        assert_eq!(state.lock().unwrap().peak, 2);
        let before = {
            let state = state.lock().unwrap();
            (state.events.len(), state.reads)
        };
        drop(owner);
        assert_closed(&state);
        assert_eq!(
            {
                let state = state.lock().unwrap();
                (state.events.len(), state.reads)
            },
            before
        );
        work[0] = 42;
        slot[0] = 42;
        assert_eq!((work[0], slot[0]), (42, 42));
    }
}
