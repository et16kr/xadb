//! Exact changed-path cases. Immutable expected files come from independent raw.
use super::*;
use crate::storage::{
    recovery_select::{
        Plan4GenesisSelection, Plan4LeaseEffect, Plan4SelectCause, Plan4SelectPhase,
    },
    retention_genesis::tests as q,
};
use foundation::{ErrorCode, namespace::DirectoryPort};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
};

#[path = "tests/support.rs"]
pub(in crate::storage) mod support;
use support::{Action, Directory, Edit, Fault, Hook, Key, Writer};
const DATA: &str = "d3-data-0000000100000001";
const MAINTENANCE: usize = 12_582_912;
fn backing<const N: usize>() -> Box<[u8; N]> {
    vec![0xa7; N].into_boxed_slice().try_into().unwrap()
}
fn scratch() -> Box<[[u8; PAGE_BYTES]; 3]> {
    vec![[0; PAGE_BYTES]; 3]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn profile(id: u32) -> Profile {
    control::decode_bootstrap_profile(&q::reference(id, "d3-profile"), Some(q::namespace()))
        .unwrap()
}
fn seed(id: u32, scratch: &mut [[u8; PAGE_BYTES]; 3]) -> Plan4GenesisSeed {
    scratch[0][..4096].copy_from_slice(&q::raw(&format!("XBS-P{id}")));
    let selected = control::decode_selector(
        &q::reference(id, "FILEMAP"),
        control::SelectorRole::FileMap,
        profile(id),
    )
    .unwrap();
    inspect_bootstrap(profile(id), selected, scratch).unwrap()
}
#[derive(Debug)]
enum AttemptError {
    Bootstrap(Plan4GenesisError),
    Doublewrite(super::super::DoublewriteInstallError),
}
fn produce(
    directory: &Directory,
    id: u32,
    complete: bool,
) -> std::result::Result<(), AttemptError> {
    let mut slot = backing::<{ super::super::super::MAX_FRAME_BYTES }>();
    let mut maintenance = backing::<MAINTENANCE>();
    let owner = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        id,
        &mut slot,
        &mut maintenance,
    )
    .map_err(AttemptError::Bootstrap)?;
    assert_eq!(owner.core.plan.kind.issued_object_high_water(), 5);
    assert_eq!(owner.core.plan.kind.version(), 4);
    let database = directory.0.lock().unwrap().entropy;
    let expected = q::files_for(id, database);
    assert_eq!(&maintenance[..IMAGE_BYTES], &expected[DATA][..IMAGE_BYTES]);
    assert!(maintenance[LOAN_BYTES..].iter().all(|byte| *byte == 0xa7));
    if complete {
        let mut flush = backing::<{ 4 << 20 }>();
        let dwb = owner
            .install_doublewrite(&mut flush)
            .map_err(AttemptError::Doublewrite)?;
        assert!(!dwb.is_fenced());
        drop(dwb);
    } else {
        drop(owner);
    }
    Ok(())
}
fn assert_released(directory: &Directory) {
    let s = directory.0.lock().unwrap();
    assert_eq!((s.writers, s.handles), (0, 0));
    assert!(s.peak <= 3);
}
fn expected_bootstrap(id: u32) -> BTreeMap<String, Vec<u8>> {
    q::files(id)
        .into_iter()
        .filter(|(name, _)| !name.starts_with("d3-dwb-") && name != "DWB")
        .collect()
}
fn assert_complete(directory: &Directory, id: u32) {
    let s = directory.0.lock().unwrap();
    assert_files(&s.images(), &q::files_for(id, s.entropy));
    assert_eq!(s.entropy_calls, 1);
}
fn assert_files(actual: &BTreeMap<String, Vec<u8>>, expected: &BTreeMap<String, Vec<u8>>) {
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in expected {
        assert_eq!(actual[name].len(), bytes.len(), "file length {name}");
        assert!(actual[name] == *bytes, "full byte mismatch {name}");
    }
}
fn partial_selection(error: crate::storage::recovery_select::Plan4SelectError, stage: bool) {
    assert_eq!(error.phase, Plan4SelectPhase::Preflight);
    assert_eq!(error.effect, Effect::CallerMustResolve);
    assert_eq!(error.database_effect, Effect::Unchanged);
    assert_eq!(error.lease_effect, Plan4LeaseEffect::AcquiredMayHaveChanged);
    if stage {
        assert!(matches!(error.cause, Plan4SelectCause::UnsupportedClosure));
    } else {
        match error.cause {
            Plan4SelectCause::Existing(inner) => match inner.cause {
                crate::storage::recovery_select::Cause::Namespace(e) => {
                    assert_eq!(e.kind, foundation::namespace::NamespaceErrorKind::NotFound)
                }
                other => panic!("unexpected partial input cause {other:?}"),
            },
            other => panic!("unexpected partial selection {other:?}"),
        }
    }
}

#[derive(Clone)]
struct Op {
    kind: &'static str,
    name: &'static str,
    offset: usize,
    length: usize,
}
fn effects() -> Vec<Op> {
    let mut out = Vec::new();
    for (name, length, stride) in [
        ("d3-profile", 4096, 4096),
        ("d3-wal-00000001", 4096, 4096),
        ("d3-writer-stage", 4096, 4096),
        ("d3-bootstrap", 4096, 4096),
        (DATA, 1_048_576, PAGE_BYTES),
        ("d3-filemap-0", 2_113_536, PAGE_BYTES),
        ("d3-filemap-1", 2_113_536, PAGE_BYTES),
        ("d3-filemap-select-stage", 4096, 4096),
        ("d3-dwb-stage", PAGE_BYTES, PAGE_BYTES),
        ("d3-dwb-images", 16_711_680, 4 << 20),
        ("d3-dwb-0", PAGE_BYTES, PAGE_BYTES),
        ("d3-dwb-1", PAGE_BYTES, PAGE_BYTES),
        ("d3-dwb-reserved", 8192, 8192),
        ("d3-dwb-select-stage", 4096, 4096),
    ] {
        out.push(Op {
            kind: "create",
            name,
            offset: 0,
            length: 0,
        });
        for offset in (0..length).step_by(stride) {
            out.push(Op {
                kind: "write",
                name,
                offset,
                length: stride.min(length - offset),
            });
        }
        out.push(Op {
            kind: "sync",
            name,
            offset: 0,
            length: 0,
        });
        if matches!(
            name,
            "d3-writer-stage" | "d3-filemap-select-stage" | "d3-dwb-select-stage"
        ) {
            out.push(Op {
                kind: "publish",
                name,
                offset: 0,
                length: 0,
            });
        }
        if matches!(
            name,
            "d3-wal-00000001"
                | "d3-writer-stage"
                | "d3-bootstrap"
                | "d3-filemap-1"
                | "d3-filemap-select-stage"
                | "d3-dwb-stage"
                | "d3-dwb-reserved"
                | "d3-dwb-select-stage"
        ) {
            out.push(Op {
                kind: "dirsync",
                name: "",
                offset: 0,
                length: 0,
            });
        }
    }
    out
}
fn source_name(name: &str) -> &str {
    match name {
        "d3-writer-stage" => "WRITER",
        "d3-filemap-select-stage" => "FILEMAP",
        "d3-dwb-select-stage" => "DWB",
        _ => name,
    }
}
/// Independent operation-prefix model. It never reads the adapter's event data.
fn prefix(id: u32, key: &Key, after: bool, partial: Option<usize>) -> BTreeMap<String, Vec<u8>> {
    let full = q::files(id);
    let mut result = BTreeMap::from([("LOCK".into(), Vec::new())]);
    let mut seen = 0;
    for op in effects() {
        let hit = op.kind == key.op && op.name == key.name && op.offset as u64 == key.offset;
        if hit {
            seen += 1;
        }
        let stop = hit && seen == key.occurrence;
        if stop && !after && partial.is_none() {
            return result;
        }
        match op.kind {
            "create" => {
                assert!(result.insert(op.name.into(), Vec::new()).is_none());
            }
            "write" => {
                let count = if stop {
                    partial.unwrap_or(op.length)
                } else {
                    op.length
                };
                let target = result.get_mut(op.name).unwrap();
                assert_eq!(target.len(), op.offset);
                target.extend_from_slice(&full[source_name(op.name)][op.offset..op.offset + count]);
            }
            "publish" => {
                let bytes = result.remove(op.name).unwrap();
                result.insert(source_name(op.name).into(), bytes);
            }
            "sync" | "dirsync" => {}
            _ => unreachable!(),
        }
        if stop {
            return result;
        }
    }
    panic!("unreachable independent prefix key")
}
fn exact_effect_order(directory: &Directory) {
    let s = directory.0.lock().unwrap();
    let actual: Vec<_> = s
        .events
        .iter()
        .filter(|e| matches!(e.op, "create" | "write" | "sync" | "publish" | "dirsync"))
        .collect();
    let expected = effects();
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert_eq!(
            (a.op, a.name.as_str(), a.offset, a.requested),
            (e.kind, e.name, e.offset as u64, e.length)
        );
        assert!(a.completed);
    }
}

#[test]
fn plan4_canonical_profiles() {
    let mut h = q::Harness::new(q::ids("C", 1, 3));
    for id in 1..=3 {
        h.case(&format!("P4-C{id:02}"), || {
            let directory = Directory::fresh();
            produce(&directory, id, true).unwrap();
            assert_complete(&directory, id);
            exact_effect_order(&directory);
            assert_released(&directory);
        });
    }
    h.finish();
}

pub(in crate::storage) fn mutate_page(case: usize) -> (usize, Vec<u8>) {
    let index = match case {
        16 | 18..=24 => 0,
        17 | 35 => 6,
        34 => 2,
        36 => 4,
        37 => 3,
        43..=45 => 7,
        46..=48 => 8,
        _ => 1,
    };
    let mut page = q::raw(&format!("P{index:02}"));
    let root = 15_840;
    let entry = |i: usize| 10_208 + 96 * i;
    match case {
        1 => page[8] ^= 1,
        2 => q::put64(&mut page, root + 208, 2),
        3 => q::put64(&mut page, 48, 5),
        4 => q::put64(&mut page, 56, 2),
        5 => q::put64(&mut page, 24, 0x100000002),
        6 => q::put64(&mut page, 32, 2),
        7 => q::put64(&mut page, 40, 2),
        8 => page[6] = 13,
        9 => page[7] = 1,
        10 => q::put64(&mut page, 64, 1),
        11 => q::put16(&mut page, 72, 0),
        12 => q::put16(&mut page, 128, 15_839),
        13 => q::put16(&mut page, 130, 511),
        14 => page[132] = 1,
        15 => page[127] = 1,
        16 | 18 => {
            let at = entry(if case == 16 { 1 } else { 8 });
            page[at..at + 96].fill(0);
            q::put64(&mut page, at + 8, 1);
        }
        17 => {
            q::put16(&mut page, 72, 3);
            q::put16(&mut page, 74, 140);
            q::put16(&mut page, 76, 13_280);
            page[140..13_280].fill(0);
        }
        19 => {
            let source = page[entry(8)..entry(8) + 96].to_vec();
            page[entry(9)..entry(9) + 96].copy_from_slice(&source);
        }
        20 => q::put64(&mut page, entry(1) + 16, 5),
        21 => page[entry(7) + 1] = 5,
        22 => q::put64(&mut page, entry(8) + 8, 2),
        23 => {
            let at = entry(0);
            page[at..at + 96].fill(0);
            page[at] = 1;
            page[at + 2] = 2;
            q::put64(&mut page, at + 8, 1);
            page[at + 32..at + 48].copy_from_slice(&q::namespace().database.bytes());
            q::put64(&mut page, at + 48, 1);
            page[at + 56] = 1;
            q::put64(&mut page, at + 64, 1);
            q::put64(&mut page, at + 72, 1);
        }
        24 => q::put64(&mut page, entry(63) + 8, 2),
        25 => q::put16(&mut page, root + 4, 2),
        26 => page[root + 8..root + 12].copy_from_slice(&511u32.to_le_bytes()),
        27 => page[root + 12..root + 16].copy_from_slice(&63u32.to_le_bytes()),
        28 => page[root + 16] = 1,
        29 => q::put64(&mut page, root + 64, 2),
        30 => q::put64(&mut page, root + 72, 1),
        31 => q::put64(&mut page, root + 96, u64::MAX >> 1),
        32 => q::put64(&mut page, root + 104, 1),
        33 => {
            let job = q::raw("JOB-FREE");
            let mut hash = ContentHasher::new();
            for _ in 0..63 {
                hash.update(&job);
            }
            page[root + 128..root + 160].copy_from_slice(&hash.digest());
        }
        34 => q::put64(&mut page, 16_352 - 1024 + 8, 0),
        35 => q::put64(&mut page, 16_352 - 4 * 1024 + 8, 2),
        36 => page[16_352 - 2 * 1024 + 6] = 1,
        37 => page[16_352 - 3 * 1024 + 1023] = 1,
        38 => page[root + 511] = 1,
        39 => page[root + 160..root + 192].copy_from_slice(&content_digest(&[0; 640])),
        40 => q::put64(&mut page, root + 256 + 3 * 24 + 8, 2),
        41 => q::put64(&mut page, root + 256 + 4 * 24 + 8, 7),
        42 => q::put64(&mut page, root + 232 + 8, 2),
        43 => q::put64(&mut page, 16_304 + 16, 9),
        44 => page[16_304 + 6] = 1,
        45 => q::put64(&mut page, 16_304 + 32, 0),
        46 => {
            q::put16(&mut page, 72, 0);
            q::put16(&mut page, 74, 128);
            q::put16(&mut page, 76, 16_352);
            page[128..16_352].fill(0);
        }
        47 => page[16_320 + 6] = 1,
        48 => {
            q::put64(&mut page, 16_320 + 8, 0x100000001);
            q::put64(&mut page, 16_320 + 16, 9);
            q::put64(&mut page, 16_320 + 24, 1);
        }
        _ => panic!("unknown N"),
    }
    q::resign(&mut page);
    (index, page)
}
#[test]
fn plan4_genesis_page_mutants() {
    let mut h = q::Harness::new(q::ids("N", 1, 48));
    for case in 1..=48 {
        h.case(&format!("P4-N{case:02}"), || {
            let (index, page) = mutate_page(case);
            if matches!(case,16|18..=24|43..=48) {
                let mut files = q::files(1);
                files.get_mut(DATA).unwrap()[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                    .copy_from_slice(&page);
                let directory = Directory::from_files(files);
                let before = directory.0.lock().unwrap().files.clone();
                let mut scratch = scratch();
                let seed = seed(1, &mut scratch);
                let error = seed
                    .verify_data(&mut directory.acquire_writer().unwrap(), &mut scratch)
                    .unwrap_err();
                match case {
                    23 => assert!(matches!(
                        error,
                        Plan4GenesisCause::Allocation(AllocationError::Corrupt)
                    )),
                    45 => assert!(matches!(error, Plan4GenesisCause::Page(PageError::Corrupt))),
                    46..=48 => {
                        assert!(matches!(error, Plan4GenesisCause::Tree(TreeError::Corrupt)))
                    }
                    _ => assert!(
                        matches!(error, Plan4GenesisCause::IdentityMismatch),
                        "{error:?}"
                    ),
                }
                let s = directory.0.lock().unwrap();
                assert!(s.files == before, "read-only mutant input changed");
                assert_eq!(s.mutations(), 0);
                drop(s);
                assert_released(&directory);
            } else {
                let expected = if index == 1 {
                    RetentionGenesisPage::Root
                } else {
                    RetentionGenesisPage::Jobs {
                        index: (index - 2) as u8,
                    }
                };
                let error =
                    retention_genesis::validate(&page, q::namespace(), expected).unwrap_err();
                let wanted = match case {
                    1 | 3..=7 => RetentionGenesisError::IdentityMismatch,
                    8 => RetentionGenesisError::Page(PageError::KindMismatch),
                    9 | 11..=15 => RetentionGenesisError::Page(PageError::Corrupt),
                    25 => RetentionGenesisError::UnsupportedFormat,
                    _ => RetentionGenesisError::Corrupt,
                };
                assert_eq!(error, wanted);
            }
        });
    }
    h.finish();
}

fn page_fault_ids(prefix: &str, actions: &[char]) -> Vec<String> {
    (0..9)
        .flat_map(|page| {
            actions
                .iter()
                .map(move |action| format!("P4-{prefix}{page:02}-{action}"))
        })
        .collect()
}
#[test]
fn plan4_prepared_page_write_boundaries() {
    let mut h = q::Harness::new(page_fault_ids("W", &['S', 'I', 'Z', 'C', 'E', 'P']));
    for page in 0..9 {
        for action in ['S', 'I', 'Z', 'C', 'E', 'P'] {
            h.case(&format!("P4-W{page:02}-{action}"), || {
                let directory = Directory::fresh();
                let key = Key::new("write", DATA, (page * PAGE_BYTES) as u64, 1);
                directory.0.lock().unwrap().fault = Some(Fault {
                    key: key.clone(),
                    action: match action {
                        'S' => Action::Short(17),
                        'I' => Action::Interrupted,
                        'Z' => Action::Zero,
                        'C' => Action::InvalidCount,
                        'E' => Action::BeforeError,
                        'P' => Action::PrefixError,
                        _ => unreachable!(),
                    },
                });
                let result = produce(&directory, 1, true);
                if matches!(action, 'S' | 'I') {
                    result.unwrap();
                    assert_complete(&directory, 1);
                } else {
                    let error = match result {
                        Err(AttemptError::Bootstrap(e)) => e,
                        other => panic!("unexpected {other:?}"),
                    };
                    assert_eq!(error.phase, Plan4GenesisPhase::Install(Phase::DataWrite));
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    match error.cause {
                        Plan4GenesisCause::Io(e) => assert_eq!(e.code, ErrorCode::IoWriteUnknown),
                        other => panic!("{other:?}"),
                    };
                    assert_files(
                        &directory.0.lock().unwrap().images(),
                        &prefix(
                            1,
                            &key,
                            false,
                            if action == 'P' { Some(8192) } else { None },
                        ),
                    );
                }
                assert_eq!(directory.0.lock().unwrap().fault_hits, 1);
                assert_released(&directory);
            });
        }
    }
    h.finish();
}

#[test]
fn plan4_readback_page_boundaries() {
    let mut h = q::Harness::new(page_fault_ids("R-P", &['S', 'Z', 'C', 'E']));
    for page in 0..9 {
        for action in ['S', 'Z', 'C', 'E'] {
            h.case(&format!("P4-R-P{page:02}-{action}"), || {
                let directory = Directory::fresh();
                directory.0.lock().unwrap().fault = Some(Fault {
                    key: Key::new(
                        "read",
                        DATA,
                        (page * PAGE_BYTES) as u64,
                        if page == 0 { 2 } else { 1 },
                    ),
                    action: match action {
                        'S' => Action::Short(17),
                        'Z' => Action::Zero,
                        'C' => Action::InvalidCount,
                        'E' => Action::PrefixError,
                        _ => unreachable!(),
                    },
                });
                let result = produce(&directory, 1, true);
                if action == 'S' {
                    result.unwrap();
                    assert_complete(&directory, 1);
                } else {
                    let error = match result {
                        Err(AttemptError::Bootstrap(e)) => e,
                        other => panic!("{other:?}"),
                    };
                    assert_eq!(error.phase, Plan4GenesisPhase::Readback);
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    match error.cause {
                        Plan4GenesisCause::Io(e) => {
                            assert_eq!(e.code, ErrorCode::IoRead);
                            assert_eq!(e.effect(), Effect::Unchanged);
                        }
                        other => panic!("{other:?}"),
                    };
                    assert_files(
                        &directory.0.lock().unwrap().images(),
                        &expected_bootstrap(1),
                    );
                }
                assert_eq!(directory.0.lock().unwrap().fault_hits, 1);
                assert_released(&directory);
            });
        }
    }
    h.finish();
}

#[test]
fn plan4_prepared_control_seals() {
    let mut h = q::Harness::new(q::ids("F", 1, 2));
    for case in 1..=2 {
        h.case(&format!("P4-F{case:02}"), || {
            let mut images: Box<[[u8; PAGE_BYTES]; 9]> = vec![[0; PAGE_BYTES]; 9]
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let mut scratch = scratch();
            let prepared = prepare(profile(1), &mut images, &mut scratch).unwrap();
            scratch[case][0] ^= 1;
            let directory = Directory::fresh();
            let error = install_prepared(
                &mut directory.acquire_writer().unwrap(),
                &prepared,
                &mut scratch,
            )
            .unwrap_err();
            assert_eq!(error.phase, Plan4GenesisPhase::Prepare);
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, Plan4GenesisCause::IdentityMismatch));
            assert!(directory.0.lock().unwrap().events.is_empty());
            assert_released(&directory);
        });
    }
    h.finish();
}

fn install_key(case: usize) -> (Key, bool) {
    match case {
        1 => (Key::new("sync", "d3-bootstrap", 0, 1), false),
        2 => (Key::new("dirsync", "", 0, 3), false),
        3 => (Key::new("sync", DATA, 0, 1), false),
        4 => (Key::new("dirsync", "", 0, 4), false),
        5 => (Key::new("publish", "d3-filemap-select-stage", 0, 1), false),
        6 => (Key::new("publish", "d3-filemap-select-stage", 0, 1), true),
        7 => (Key::new("dirsync", "", 0, 5), true),
        8 => (Key::new("sync", "d3-dwb-images", 0, 1), false),
        9 => (Key::new("publish", "d3-dwb-select-stage", 0, 1), false),
        10 => (Key::new("publish", "d3-dwb-select-stage", 0, 1), true),
        11 => (Key::new("dirsync", "", 0, 8), true),
        _ => unreachable!(),
    }
}
fn assert_install_error(error: AttemptError, case: usize) {
    match error {
        AttemptError::Bootstrap(error) => {
            assert!(case <= 7);
            assert_eq!(error.effect, Effect::CallerMustResolve);
            let expected = match case {
                1 => Phase::DescriptorSync,
                2 => Phase::DescriptorDirectory,
                3 => Phase::DataSync,
                4 => Phase::FilesDirectory,
                5 | 6 => Phase::SelectorPublish,
                7 => Phase::SelectorDirectory,
                _ => unreachable!(),
            };
            assert_eq!(error.phase, Plan4GenesisPhase::Install(expected));
            match error.cause {
                Plan4GenesisCause::Bootstrap(inner) => {
                    assert_eq!(inner.phase, expected);
                    match inner.cause {
                        super::super::Cause::Io(e) => assert_eq!(e.code, ErrorCode::IoWriteUnknown),
                        super::super::Cause::Namespace(e) => {
                            assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other))
                        }
                        other => panic!("{other:?}"),
                    }
                }
                Plan4GenesisCause::Io(e) => assert_eq!(e.code, ErrorCode::IoWriteUnknown),
                other => panic!("{other:?}"),
            }
        }
        AttemptError::Doublewrite(error) => {
            use super::super::DoublewriteInstallPhase as D;
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert_eq!(
                error.phase,
                match case {
                    8 => D::ImagesSync,
                    9 | 10 => D::SelectorPublish,
                    11 => D::SelectorDirectory,
                    _ => panic!("unexpected DWB boundary"),
                }
            );
            match error.cause {
                super::super::DoublewriteInstallCause::Io(e) => {
                    assert_eq!(e.code, ErrorCode::IoWriteUnknown)
                }
                super::super::DoublewriteInstallCause::Namespace(e) => {
                    assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other))
                }
                other => panic!("{other:?}"),
            }
        }
    }
}
fn select(
    directory: &Directory,
) -> std::result::Result<(), crate::storage::recovery_select::Plan4SelectError> {
    let mut maintenance = backing::<MAINTENANCE>();
    Plan4GenesisSelection::acquire(directory, Some(q::namespace()), &mut maintenance).map(|owner| {
        assert_eq!(owner.observation().bootstrap_plan(), 4);
        drop(owner)
    })
}
#[test]
fn plan4_install_and_final_identity_boundaries() {
    let mut h = q::Harness::new(q::ids("I", 1, 20));
    for case in 1..=20 {
        h.case(&format!("P4-I{case:02}"), || {
            if case <= 11 {
                let directory = Directory::fresh();
                let (key, after) = install_key(case);
                directory.0.lock().unwrap().fault = Some(Fault {
                    key: key.clone(),
                    action: if after {
                        Action::AfterError
                    } else {
                        Action::BeforeError
                    },
                });
                assert_install_error(produce(&directory, 1, true).unwrap_err(), case);
                assert_eq!(directory.0.lock().unwrap().fault_hits, 1);
                assert_files(
                    &directory.0.lock().unwrap().images(),
                    &prefix(1, &key, after, None),
                );
                assert_released(&directory);
                let result = select(&directory);
                if case >= 10 {
                    result.unwrap();
                } else {
                    partial_selection(result.unwrap_err(), matches!(case, 5 | 9));
                }
                assert_released(&directory);
            } else if case <= 14 || case == 19 {
                let directory = Directory::fresh();
                if case == 19 {
                    directory.0.lock().unwrap().fault = Some(Fault {
                        key: Key::new("read", DATA, 0, 1),
                        action: Action::BeforePanic,
                    });
                    let unwind = catch_unwind(AssertUnwindSafe(|| produce(&directory, 1, true)));
                    assert!(unwind.is_err());
                    assert_files(
                        &directory.0.lock().unwrap().images(),
                        &expected_bootstrap(1),
                    );
                    assert_eq!(directory.0.lock().unwrap().fault_hits, 1);
                } else {
                    let edit = match case {
                        12 => Edit::Identity(DATA.into()),
                        13 => Edit::Length(DATA.into(), 1),
                        14 => Edit::Bytes("d3-profile".into(), q::reference(2, "d3-profile")),
                        _ => unreachable!(),
                    };
                    directory.0.lock().unwrap().hook = Some(Hook {
                        key: Key::new("read", DATA, 63 * PAGE_BYTES as u64, 2),
                        edit,
                    });
                    let error = match produce(&directory, 1, true) {
                        Err(AttemptError::Bootstrap(e)) => e,
                        other => panic!("{other:?}"),
                    };
                    assert_eq!(error.phase, Plan4GenesisPhase::Readback);
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    match case {
                        12 | 13 => {
                            assert!(matches!(error.cause, Plan4GenesisCause::IdentityMismatch))
                        }
                        14 => assert!(matches!(
                            error.cause,
                            Plan4GenesisCause::PageIo(PageIoError::Control(
                                ControlError::IdentityMismatch
                            ))
                        )),
                        _ => unreachable!(),
                    };
                    let s = directory.0.lock().unwrap();
                    assert_eq!(s.hook_hits + s.fault_hits, 1);
                    assert!(!s.files.contains_key("DWB"));
                }
                assert_released(&directory);
            } else {
                let directory = Directory::from_files(q::files(1));
                if case == 20 {
                    directory.0.lock().unwrap().fault = Some(Fault {
                        key: Key::new("open", DATA, 0, 2),
                        action: Action::OpenedIdentity,
                    });
                } else {
                    let edit = match case {
                        15 => Edit::Identity(DATA.into()),
                        16 => Edit::Length(DATA.into(), 1),
                        17 => {
                            let mut bytes = q::reference(1, "FILEMAP");
                            q::put64(&mut bytes, 32, 2);
                            q::resign(&mut bytes);
                            Edit::Bytes("FILEMAP".into(), bytes)
                        }
                        18 => {
                            let mut bytes = q::reference(1, "WRITER");
                            bytes[96] ^= 1;
                            q::resign(&mut bytes);
                            Edit::Bytes("WRITER".into(), bytes)
                        }
                        _ => unreachable!(),
                    };
                    directory.0.lock().unwrap().hook = Some(Hook {
                        key: Key::new("read", DATA, 63 * PAGE_BYTES as u64, 2),
                        edit,
                    });
                }
                let error = select(&directory).unwrap_err();
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert_eq!(error.database_effect, Effect::Unchanged);
                assert_eq!(error.lease_effect, Plan4LeaseEffect::AcquiredMayHaveChanged);
                if case == 20 {
                    assert_eq!(error.phase, Plan4SelectPhase::Data);
                    match error.cause {
                        Plan4SelectCause::Genesis(Plan4GenesisCause::PageIo(
                            PageIoError::Namespace(e),
                        )) => {
                            assert_eq!(
                                e.kind,
                                foundation::namespace::NamespaceErrorKind::InvalidType
                            );
                            assert_eq!(e.effect, Effect::Unchanged);
                            assert_eq!(e.context, "plan4 opened identity mismatch");
                        }
                        other => panic!("{other:?}"),
                    }
                } else if case <= 16 {
                    assert_eq!(error.phase, Plan4SelectPhase::Data);
                    assert!(matches!(
                        error.cause,
                        Plan4SelectCause::Genesis(Plan4GenesisCause::IdentityMismatch)
                    ));
                } else {
                    assert_eq!(error.phase, Plan4SelectPhase::FinalRecheck);
                    assert!(matches!(error.cause, Plan4SelectCause::IdentityMismatch));
                }
                let s = directory.0.lock().unwrap();
                assert_eq!(s.hook_hits + s.fault_hits, 1);
                assert_eq!(s.mutations(), 0);
                if case == 20 {
                    assert_eq!(
                        s.events
                            .iter()
                            .filter(|event| event.op == "open" && event.name == DATA)
                            .count(),
                        2
                    );
                    assert_eq!(
                        s.events
                            .iter()
                            .filter(|event| event.op == "read" && event.name == DATA)
                            .count(),
                        0
                    );
                }
                drop(s);
                assert_released(&directory);
            }
        });
    }
    h.finish();
}

#[cfg(feature = "test-support")]
fn request(kind: u8) -> crate::testing::PageReadRequest {
    crate::testing::PageReadRequest {
        database: q::namespace().database.bytes(),
        file: 0x100000001,
        page: if kind == 12 { 1 } else { 2 },
        incarnation: 1,
        owner: 4,
        owner_generation: 1,
        kind,
        level: 0,
    }
}
#[cfg(feature = "test-support")]
#[test]
fn plan4_selected_page_kind_inputs() {
    let mut h = q::Harness::new(q::ids("K", 28, 29));
    for (case, kind) in [(28, 12), (29, 13)] {
        h.case(&format!("P4-K{case}"), || {
            let directory = Directory::from_files(q::files(1));
            let mut output = backing::<PAGE_BYTES>();
            let result = crate::testing::read_selected_page(
                &mut directory.acquire_writer().unwrap(),
                (q::namespace().database.bytes(), 1),
                request(kind),
                &mut output,
            )
            .unwrap();
            assert_eq!(result, (1, if kind == 12 { 1 } else { 15 }));
            assert_eq!(
                output.as_slice(),
                q::raw(if kind == 12 { "P01" } else { "P02" })
            );
            assert_eq!(directory.0.lock().unwrap().mutations(), 0);
            assert_released(&directory);
        });
    }
    h.finish();
}
#[cfg(feature = "test-support")]
#[test]
fn plan4_buffer_trace_kind_inputs() {
    use crate::testing::BufferTraceOperation as O;
    let mut h = q::Harness::new(q::ids("K", 30, 31));
    for (case, kind) in [(30, 12), (31, 13)] {
        h.case(&format!("P4-K{case}"), || {
            let directory = Directory::from_files(q::files(1));
            let trace = crate::testing::run_selected_buffer_trace(
                &mut directory.acquire_writer().unwrap(),
                (q::namespace().database.bytes(), 1),
                &[request(kind)],
                &[
                    O::Pin {
                        slot: 0,
                        request: 0,
                    },
                    O::Read { slot: 0 },
                    O::Release { slot: 0 },
                ],
            )
            .unwrap();
            assert_eq!(trace.len(), 3);
            let pinned = trace[0].as_ref().unwrap().as_ref().unwrap();
            let page = trace[1].as_ref().unwrap().as_ref().unwrap();
            assert_eq!(pinned, page);
            assert_eq!(
                page.identity,
                (
                    q::namespace().database.bytes(),
                    0x0000_0001_0000_0001,
                    if kind == 12 { 1 } else { 2 },
                    1,
                    4,
                    1,
                )
            );
            assert_eq!(
                page.digest,
                content_digest(&q::raw(if kind == 12 { "P01" } else { "P02" }))
            );
            assert_eq!(page.kind_level, (kind, 0));
            assert_eq!(page.lsn_cells, (0, if kind == 12 { 1 } else { 15 }));
            assert!(matches!(&trace[2], Ok(None)));
            assert_eq!(directory.0.lock().unwrap().mutations(), 0);
            assert_released(&directory);
        });
    }
    h.finish();
}

fn census<W: WriterNamespace>() {
    // Base39 had exactly these private fields and three payload-free variants.
    // Only type layout is compared; no old operation or old fixture is executed.
    #[allow(dead_code)]
    enum OldBootstrapPlanKind {
        AllocatorOnly,
        StatusBootstrap,
        UndoBootstrap,
    }
    #[allow(dead_code)]
    struct OldBootstrapPlan {
        kind: OldBootstrapPlanKind,
        profile: Profile,
        selected: SelectedControl,
        allocation_digest: [u8; 32],
    }
    assert_eq!(
        size_of::<BootstrapPlanKind>(),
        size_of::<OldBootstrapPlanKind>()
    );
    assert_eq!(size_of::<BootstrapPlan>(), size_of::<OldBootstrapPlan>());
    assert_eq!(size_of::<BootstrapPlanKind>(), 1);
    println!(
        "P4_LAYOUT_DELTA\tkind_old={}\tkind_new={}\tplan_old={}\tplan_new={}",
        size_of::<OldBootstrapPlanKind>(),
        size_of::<BootstrapPlanKind>(),
        size_of::<OldBootstrapPlan>(),
        size_of::<BootstrapPlan>()
    );
    let actual = source_census::<W>();
    let old = super::super::source_budget::<W>();
    let selection = crate::storage::recovery_select::plan4_selection_source_census::<W>();
    println!(
        "P4_CENSUS\twriter={}\tfile={}\tprepared={}\tseed={}\terror={}\tconstructor_result={}\tprepare={}\tinstall={}\treadback={}\treturned={}\tpeak={}\tselection_peak={}\told_prepare={}\told_readback={}",
        size_of::<W>(),
        size_of::<W::File>(),
        size_of::<PreparedPlan4<'_>>(),
        size_of::<Plan4GenesisSeed>(),
        size_of::<Plan4GenesisError>(),
        size_of::<std::result::Result<FreshStorageSession<'_, W>, Plan4GenesisError>>(),
        actual.prepare,
        actual.install,
        actual.readback,
        actual.returned,
        actual.peak,
        selection.peak,
        old.prepare_nested,
        old.readback_nested
    );
    println!(
        "P4_COMMON_LAYOUT\tbootstrap_plan={}\tbootstrap_error={}\tfresh_owner={}\tdescriptor_spec={}\tsystem_progress={}\tundo_record={}",
        size_of::<BootstrapPlan>(),
        size_of::<BootstrapError>(),
        size_of::<FreshStorageSession<'_, W>>(),
        size_of::<crate::storage::obligation::DescriptorSpec>(),
        size_of::<crate::storage::obligation::SystemProgress>(),
        size_of::<crate::storage::undo_record::UndoRecord<'_>>()
    );
    assert!(fits::<W>());
    assert!(selection.peak <= 65536);
    assert!(old.prepare_nested <= 16384 && old.readback_nested <= 16384);
    assert!(size_of::<W::File>() <= 16384);
    assert_eq!(IMAGE_BYTES, 147456);
    assert_eq!(LOAN_BYTES, 196608);
    const { assert!(LOAN_BYTES <= MAINTENANCE) };
    assert_eq!(1_048_576, IMAGE_BYTES + 55 * PAGE_BYTES);
    assert_eq!(6 * PAGE_BYTES, 98304);
    assert_eq!(2 * PAGE_BYTES, 32768);
    let source_fixed = std::hint::black_box(super::super::FIXED_BYTES + 16_777_216);
    assert_eq!(source_fixed - 4096, 22_069_248);
}
#[test]
fn plan4_source_census_memory() {
    let mut h = q::Harness::new(vec!["P4-ST01".into()]);
    h.case("P4-ST01", census::<Writer>);
    h.finish();
}
#[cfg(target_os = "linux")]
#[test]
fn plan4_source_census_native() {
    let mut h = q::Harness::new(vec!["P4-ST02".into()]);
    h.case(
        "P4-ST02",
        census::<crate::storage::recovery_select::plan4_native_support::Writer>,
    );
    h.finish();
}

#[test]
#[ignore = "DEV3 final S5 expansion"]
fn plan4_s5_crash_prefixes() {
    let events: Vec<_> = (0..9)
        .map(|p| format!("PAGE{p}"))
        .chain([
            "DATA_SYNC".into(),
            "FILEMAP_PUBLISH".into(),
            "DWB_PUBLISH".into(),
        ])
        .collect();
    let ids = events
        .iter()
        .flat_map(|event| {
            [
                format!("P4-SC{event}-BEFORE"),
                format!("P4-SC{event}-AFTER"),
            ]
        })
        .collect();
    let mut h = q::Harness::new(ids);
    for event in events {
        for after in [false, true] {
            h.case(
                &format!("P4-SC{event}-{}", if after { "AFTER" } else { "BEFORE" }),
                || {
                    let key = if let Some(page) = event.strip_prefix("PAGE") {
                        Key::new(
                            "write",
                            DATA,
                            page.parse::<u64>().unwrap() * PAGE_BYTES as u64,
                            1,
                        )
                    } else {
                        match event.as_str() {
                            "DATA_SYNC" => Key::new("sync", DATA, 0, 1),
                            "FILEMAP_PUBLISH" => {
                                Key::new("publish", "d3-filemap-select-stage", 0, 1)
                            }
                            "DWB_PUBLISH" => Key::new("publish", "d3-dwb-select-stage", 0, 1),
                            _ => unreachable!(),
                        }
                    };
                    let files = prefix(1, &key, after, None);
                    let directory = Directory::from_files(files.clone());
                    let result = select(&directory);
                    if event == "DWB_PUBLISH" && after {
                        result.unwrap();
                        assert_files(&files, &q::files(1));
                    } else {
                        partial_selection(
                            result.unwrap_err(),
                            !after && matches!(event.as_str(), "FILEMAP_PUBLISH" | "DWB_PUBLISH"),
                        );
                    }
                    assert_files(&directory.0.lock().unwrap().images(), &files);
                    assert_eq!(directory.0.lock().unwrap().mutations(), 0);
                    assert_released(&directory);
                },
            );
        }
    }
    h.finish();
}
#[test]
#[ignore = "DEV3 final S5 expansion"]
fn plan4_s5_entropy_profiles() {
    let seeds: [[u8; 16]; 8] = [
        [0xff; 16],
        std::array::from_fn(|i| if i == 0 { 1 } else { 0 }),
        std::array::from_fn(|i| if i == 15 { 1 } else { 0 }),
        std::array::from_fn(|i| if i % 2 == 0 { 0xaa } else { 0x55 }),
        std::array::from_fn(|i| i as u8),
        std::array::from_fn(|i| 255 - i as u8),
        std::array::from_fn(|i| if i == 0 { 0x80 } else { 0 }),
        std::array::from_fn(|i| if i == 15 { 0x80 } else { 0 }),
    ];
    let ids = (1..=3)
        .flat_map(|p| (1..=8).map(move |seed| format!("P4-SE{p}-{seed}")))
        .collect();
    let mut h = q::Harness::new(ids);
    for profile in 1..=3 {
        for (index, entropy) in seeds.into_iter().enumerate() {
            h.case(&format!("P4-SE{profile}-{}", index + 1), || {
                let directory = Directory::fresh();
                directory.0.lock().unwrap().entropy = entropy;
                produce(&directory, profile, true).unwrap();
                assert_complete(&directory, profile);
                exact_effect_order(&directory);
                assert_released(&directory);
            });
        }
    }
    h.finish();
}

/// Setup creates the actual requested plan4 owner; the new operation is the
/// specific legacy-role/admission boundary owned by S's single O case marker.
pub(in crate::storage) fn owner_boundary(case: usize) {
    use crate::storage::wal_io::Plan4TestMutableCause as M;
    use foundation::namespace::NamespaceErrorKind;
    assert!(matches!(case, 8..=14 | 17 | 18 | 20));
    if matches!(case, 13 | 14) {
        old_mixed_kind_boundary(case);
        return;
    }
    let directory = if case == 20 {
        Directory::from_files(BTreeMap::from([
            ("LOCK".into(), Vec::new()),
            ("d3-bootstrap".into(), q::raw("XBS-P1")),
        ]))
    } else {
        Directory::fresh()
    };
    let mut slot = backing::<{ super::super::super::MAX_FRAME_BYTES }>();
    let mut maintenance = backing::<MAINTENANCE>();
    if case == 18 {
        let writer = support::SizedWriter {
            inner: directory.acquire_writer().unwrap(),
            padding: [0; 16385],
        };
        let error =
            FreshStorageSession::create_retention_catalog(writer, 1, &mut slot, &mut maintenance)
                .err()
                .expect("oversized source must reject");
        assert_eq!(error.phase, Plan4GenesisPhase::Preflight);
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(matches!(error.cause, Plan4GenesisCause::Capacity));
        let state = directory.0.lock().unwrap();
        assert_eq!(state.entropy_calls, 0);
        assert!(state.events.is_empty());
        drop(state);
        assert_released(&directory);
        return;
    }
    let before = directory.0.lock().unwrap().files.clone();
    let result = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        if case == 17 { 0 } else { 1 },
        &mut slot,
        &mut maintenance,
    );
    if matches!(case, 17 | 20) {
        let error = result.err().expect("preflight must reject");
        assert_eq!(error.effect, Effect::Unchanged);
        if case == 17 {
            assert_eq!(error.phase, Plan4GenesisPhase::Preflight);
            assert!(matches!(error.cause, Plan4GenesisCause::InvalidInput));
        } else {
            assert_eq!(error.phase, Plan4GenesisPhase::Genesis);
            match error.cause {
                Plan4GenesisCause::Wal(error) => {
                    assert_eq!(error.phase, super::super::super::Phase::Preflight);
                    assert!(
                        matches!(error.cause,super::super::super::Cause::Namespace(ref e) if e.kind==NamespaceErrorKind::LimitExceeded)
                    );
                }
                other => panic!("{other:?}"),
            }
        }
        let state = directory.0.lock().unwrap();
        assert_eq!(state.entropy_calls, 0);
        assert_eq!(state.mutations(), 0);
        if case == 17 {
            assert!(state.events.is_empty());
        }
        if case == 20 {
            assert!(state.files == before, "residue was changed");
        }
        drop(state);
        assert_released(&directory);
        return;
    }
    let owner = result.unwrap();
    let mut flush = backing::<{ 4 << 20 }>();
    if case <= 9 {
        let effects = directory.0.lock().unwrap().events.len();
        let error = if case == 8 {
            owner.status_role().err().expect("plan4 status role")
        } else {
            owner.undo_role().err().expect("plan4 Undo role")
        };
        assert_eq!(error.phase, Phase::Role);
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(matches!(error.cause, super::super::Cause::UnsupportedPlan));
        assert!(!owner.is_fenced());
        assert_eq!(directory.0.lock().unwrap().events.len(), effects);
        drop(owner);
    } else {
        let owner = owner
            .install_doublewrite(&mut flush)
            .unwrap()
            .into_mutable_storage();
        let effects = directory.0.lock().unwrap().events.len();
        if case <= 11 {
            let error = if case == 10 {
                owner
                    .status_role()
                    .err()
                    .expect("mutable plan4 status role")
            } else {
                owner.undo_role().err().expect("mutable plan4 Undo role")
            };
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(
                matches!(error.cause,M::Bootstrap(ref inner) if inner.phase==Phase::Role && inner.effect==Effect::Unchanged && matches!(inner.cause,super::super::Cause::UnsupportedPlan))
            );
        } else {
            let core = crate::transaction::concurrency::PageConcurrency::new(owner.session_fence())
                .unwrap();
            let mut page = backing::<PAGE_BYTES>();
            let mut frames = [crate::storage::dirty_frame::DirtyFrame::new(&mut page)];
            let mut operation = backing::<98304>();
            let error = owner
                .bind_allocator(&core, &mut frames, &mut operation)
                .err()
                .expect("plan4 allocator binding");
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(error.cause, M::UnsupportedPlan));
            frames[0].check_empty().unwrap();
            assert!(operation.iter().all(|byte| *byte == 0xa7));
            assert!(page.iter().all(|byte| *byte == 0xa7));
        }
        assert!(!owner.session_fence().is_fenced());
        assert_eq!(directory.0.lock().unwrap().events.len(), effects);
        drop(owner);
    }
    assert_released(&directory);
}

/// Pinned independent input is retained with its original provenance. These
/// reads do not execute the old fixture generator or any completed old test.
fn old_mixed_raw(path: &str, length: usize, digest: &str) -> Vec<u8> {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(path),
    )
    .unwrap();
    assert_eq!(bytes.len(), length);
    assert_eq!(q::hash_hex(&bytes), digest);
    bytes
}

fn old_mixed_kind_boundary(case: usize) {
    use crate::storage::{
        wal_frame::{self, FrameExpectation, RedoKind, WalStreamIdentity},
        wal_io::{
            ClosedRecovery, Plan4TestSemanticError, RecoveryCause, RecoveryPhase,
            RecoveryPrefixCause, RecoveryPrefixPhase,
        },
    };
    let kind = if case == 13 {
        PageKind::RetentionRoot
    } else {
        PageKind::RetentionJobs
    };
    let allocator = old_mixed_raw(
        "dev3-mixed-system-init/p1/allocator.genesis",
        PAGE_BYTES,
        "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
    );
    let bootstrap = old_mixed_raw(
        "dev3-undo-bootstrap/profile1-d3-bootstrap.bin",
        4096,
        "c0566b18ff533f5d9f85b21a47ffe3777fceeb85e760d19fd46c14ea9991068b",
    );
    let reserve = old_mixed_raw(
        "dev3-mixed-system-init/p1/sequence/work1-reserve.wal",
        21200,
        "1a9c6df5ea77c6372189ade584cd7b3b2484816ad93e949c9e51f12902263b86",
    );
    let mut terminal = old_mixed_raw(
        "dev3-mixed-system-init/p1/sequence/work1-terminal.wal",
        38000,
        "2e5173c72fbc5dfc96cba834f240682081abfd41b3d425e03d3d5491407f79b5",
    );
    // This is a normal generic INIT, not a queue-body certificate. Keep the
    // independent old native/descriptor/budget unchanged and change only the
    // common target/image class. Closed Status materialization must detect the
    // target mismatch after generic frame and INIT validation succeed.
    const INIT: usize = 21520;
    const IMAGE: usize = INIT + 64;
    terminal[INIT + 1] = kind as u8;
    terminal[IMAGE + 6] = kind as u8;
    q::resign(&mut terminal[IMAGE..IMAGE + PAGE_BYTES]);
    q::resign(&mut terminal);
    let stream = WalStreamIdentity {
        database: q::namespace().database,
        timeline: 1,
        writer_epoch: 1,
    };
    let begin = wal_frame::decode_semantic_envelope(
        &reserve,
        FrameExpectation {
            stream,
            previous_lsn: 0,
        },
    )
    .unwrap();
    assert_eq!((begin.header.lsn, begin.record_count), (1, 1));
    let end = wal_frame::decode_semantic_envelope(
        &terminal,
        FrameExpectation {
            stream,
            previous_lsn: 1,
        },
    )
    .unwrap();
    assert_eq!((end.header.lsn, end.record_count), (2, 2));
    let init = wal_frame::decode_redo_record(end.header, &terminal[INIT..37968]).unwrap();
    assert_eq!(init.kind, RedoKind::Init);
    assert_eq!(init.target.kind, kind);
    assert_eq!(
        (init.target.page.page_no, init.target.page.incarnation.get()),
        (1, 2)
    );
    assert_eq!(
        (init.target.owner.get(), init.target.owner_generation.get()),
        (2, 1)
    );
    assert_eq!(init.data, &terminal[IMAGE..37968]);
    let mut files = q::files(1);
    let data = files.get_mut(DATA).unwrap();
    data.fill(0);
    data[..PAGE_BYTES].copy_from_slice(&allocator);
    files.insert("d3-bootstrap".into(), bootstrap);
    let wal = files.get_mut("d3-wal-00000001").unwrap();
    wal.extend_from_slice(&reserve);
    wal.extend_from_slice(&terminal);
    let expected = files.clone();
    let directory = Directory::from_files(files);
    let mut maintenance = backing::<MAINTENANCE>();
    let mut flush = backing::<{ 4 << 20 }>();
    let error = ClosedRecovery::acquire_mixed(
        &directory,
        Some(q::namespace()),
        &mut maintenance,
        &mut flush,
    )
    .err()
    .expect("old mixed census must reject the new generic page class");
    assert_eq!(error.error().phase, RecoveryPhase::Prefix);
    assert_eq!(error.error().effect, Effect::CallerMustResolve);
    match &error.error().cause {
        RecoveryCause::Prefix(inner) => {
            assert_eq!(inner.phase, RecoveryPrefixPhase::Semantic);
            assert_eq!(inner.effect, Effect::Unchanged);
            assert!(matches!(
                inner.cause,
                RecoveryPrefixCause::Semantic(Plan4TestSemanticError::PageMismatch)
            ));
        }
        other => panic!("unexpected old mixed rejection {other:?}"),
    }
    assert_eq!(error.effects().mutation_attempted, 0);
    let state = directory.0.lock().unwrap();
    assert_eq!((state.writers, state.handles, state.mutations()), (0, 0, 0));
    assert_files(&state.images(), &expected);
}
