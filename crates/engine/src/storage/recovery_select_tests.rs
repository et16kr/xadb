//! Independent literal inputs and read-only namespace model for the new SELECT.
//! Geometry-only files model length separately; native tests supply actual files.
use super::*;
use crate::api::DatabaseId;
use foundation::{io::FileIo, namespace::FileIdentity};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{Arc, Mutex},
};

const OLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-storage-session/literal"
);
const DWB: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-doublewrite-install/literal"
);
const STATUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-bootstrap"
);
const FLUSH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocator-flush-one"
);
const REQUIRED: [&str; 14] = [
    "d3-profile",
    "d3-bootstrap",
    "FILEMAP",
    "d3-filemap-0",
    "d3-filemap-1",
    "d3-data-0000000100000001",
    "d3-dwb-stage",
    "DWB",
    "d3-dwb-0",
    "d3-dwb-1",
    "d3-dwb-images",
    "d3-dwb-reserved",
    "d3-wal-00000001",
    "WRITER",
];
const FAMILIES: [&str; 10] = [
    "d3-profile",
    "FILEMAP",
    "root",
    "chunk",
    "d3-bootstrap",
    "d3-dwb-stage",
    "DWB",
    "d3-dwb-0",
    "d3-wal-00000001",
    "WRITER",
];
const BASE_OPS: usize = 56;

fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn bytes(root: &str, filename: &str) -> Vec<u8> {
    std::fs::read(format!("{root}/{filename}")).unwrap()
}
fn literal(root: &str, profile: u32, filename: &str) -> Vec<u8> {
    bytes(root, &format!("profile{profile}-{filename}.bin"))
}
fn scratch() -> Box<[[u8; PAGE_BYTES]; 3]> {
    vec![[0; PAGE_BYTES]; 3]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn ids(prefix: &str, count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("{prefix}{n:02}")).collect()
}
fn run(cases: &[String], mut case: impl FnMut(usize, &str)) {
    let chosen = match std::env::var("XADB_RECOVERY_SELECT_CASES") {
        Ok(value) => {
            assert!(!value.is_empty(), "empty selection");
            let parsed: Vec<_> = value.split(',').collect();
            let mut unique = BTreeSet::new();
            for id in &parsed {
                assert!(
                    cases.iter().any(|allowed| allowed == id),
                    "unknown case {id}"
                );
                assert!(unique.insert(*id), "duplicate case {id}");
            }
            Some(
                unique
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>(),
            )
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid selection: {error}"),
    };
    let mut completed = 0;
    for (index, id) in cases.iter().enumerate() {
        if chosen
            .as_ref()
            .is_some_and(|selection| !selection.contains(id))
        {
            continue;
        }
        println!("RECOVERY_SELECT_CASE_BEGIN\t{id}");
        case(index, id);
        println!("RECOVERY_SELECT_CASE\t{id}\tPASS");
        completed += 1;
    }
    assert!(completed > 0, "selection reached zero cases");
    println!("RECOVERY_SELECT_COMPLETE count={completed}");
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    bytes: Vec<u8>,
    length: u64,
    identity: FileIdentity,
}
#[derive(Default)]
struct State {
    entries: BTreeMap<String, Entry>,
    active: bool,
    handles: usize,
    peak: usize,
    operations: Vec<String>,
    reads: Vec<(String, u64, usize)>,
    mutation_calls: usize,
    acquire_calls: usize,
    fail_at: Option<usize>,
    panic_at: Option<usize>,
    short: Option<usize>,
    interrupted: bool,
    zero: bool,
    replacement: Option<(String, bool)>,
    acquired_effect: Option<Effect>,
}
#[derive(Clone)]
struct Directory(Arc<Mutex<State>>);
struct Writer(Arc<Mutex<State>>);
struct File {
    state: Arc<Mutex<State>>,
    name: String,
}
fn lower(kind: NamespaceErrorKind) -> NamespaceError {
    NamespaceError::new(kind, Effect::Unchanged, "fixed SELECT model fault")
}
fn step(state: &Arc<Mutex<State>>, event: String) -> io::Result<()> {
    let (fail, panic) = {
        let mut state = state.lock().unwrap();
        state.operations.push(event);
        let n = state.operations.len();
        (state.fail_at == Some(n), state.panic_at == Some(n))
    };
    assert!(!panic, "fixed SELECT model panic");
    if fail {
        Err(io::Error::other("fixed SELECT fault"))
    } else {
        Ok(())
    }
}
fn ns_step(state: &Arc<Mutex<State>>, event: String) -> NamespaceResult<()> {
    step(state, event).map_err(|e| {
        NamespaceError::io(
            NamespaceErrorKind::Io,
            Effect::Unchanged,
            "fixed SELECT model fault",
            e,
        )
    })
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        self.0.lock().unwrap().acquire_calls += 1;
        ns_step(&self.0, "lease".into())?;
        let mut state = self.0.lock().unwrap();
        if let Some(effect) = state.acquired_effect {
            return Err(NamespaceError::new(
                NamespaceErrorKind::Io,
                effect,
                "fixed acquire effect",
            ));
        }
        if state.active || state.handles != 0 {
            return Err(lower(NamespaceErrorKind::Busy));
        }
        state.active = true;
        Ok(Writer(Arc::clone(&self.0)))
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).active = false;
    }
}
impl Drop for File {
    fn drop(&mut self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).handles -= 1;
    }
}
impl FileIo for File {
    fn read_at(&mut self, out: &mut [u8], offset: u64) -> io::Result<usize> {
        step(
            &self.state,
            format!("read:{}:{offset}:{}", self.name, out.len()),
        )?;
        let mut state = self.state.lock().unwrap();
        state.reads.push((self.name.clone(), offset, out.len()));
        if state.interrupted {
            state.interrupted = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if state.zero {
            state.zero = false;
            return Ok(0);
        }
        let entry = &state.entries[&self.name];
        let start = offset as usize;
        let count = out
            .len()
            .min(entry.bytes.len().saturating_sub(start))
            .min(state.short.unwrap_or(usize::MAX));
        if count > 0 {
            out[..count].copy_from_slice(&entry.bytes[start..start + count]);
        }
        Ok(count)
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        self.state.lock().unwrap().mutation_calls += 1;
        Err(io::Error::other("forbidden write"))
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().mutation_calls += 1;
        Err(io::Error::other("forbidden sync"))
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        self.state.lock().unwrap().mutation_calls += 1;
        Err(io::Error::other("forbidden resize"))
    }
}
impl Writer {
    fn forbidden<T>(&self) -> NamespaceResult<T> {
        self.0.lock().unwrap().mutation_calls += 1;
        Err(lower(NamespaceErrorKind::InvalidName))
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.forbidden()
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<File> {
        self.forbidden()
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        self.open_existing_with_metadata(name, mode).map(|(f, _)| f)
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        assert_eq!(mode, OpenMode::ReadOnly);
        ns_step(&self.0, format!("open:{name}"))?;
        let mut state = self.0.lock().unwrap();
        let entry = state
            .entries
            .get(name.as_str())
            .ok_or_else(|| lower(NamespaceErrorKind::NotFound))?;
        let metadata = FileMetadata {
            identity: entry.identity,
            length: entry.length,
        };
        state.handles += 1;
        state.peak = state.peak.max(state.handles);
        Ok((
            File {
                state: Arc::clone(&self.0),
                name: name.to_string(),
            },
            metadata,
        ))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        ns_step(&self.0, format!("metadata:{name}"))?;
        let state = self.0.lock().unwrap();
        let Some(entry) = state.entries.get(name.as_str()) else {
            return Ok(None);
        };
        let mut metadata = FileMetadata {
            identity: entry.identity,
            length: entry.length,
        };
        if let Some((path, length)) = &state.replacement
            && path == name.as_str()
        {
            if *length {
                metadata.length += 1;
            } else {
                metadata.identity.inode += 1;
            }
        }
        Ok(Some(metadata))
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.forbidden()
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.forbidden()
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        self.forbidden()
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        self.forbidden()
    }
}

fn directory(profile: u32, plan: u64) -> Directory {
    let mut state = State::default();
    let mut put = |path: &str, raw: Vec<u8>, length: u64| {
        let inode = state.entries.len() as u64 + 2;
        state.entries.insert(
            path.into(),
            Entry {
                bytes: raw,
                length,
                identity: FileIdentity { device: 7, inode },
            },
        );
    };
    for path in ["d3-profile", "FILEMAP", "d3-wal-00000001", "WRITER"] {
        put(path, literal(OLD, profile, path), 4096);
    }
    put(
        "d3-bootstrap",
        literal(
            if plan == 1 { OLD } else { STATUS },
            profile,
            "d3-bootstrap",
        ),
        4096,
    );
    let mut map = vec![0; PAGE_BYTES + 256];
    map[..320].copy_from_slice(&literal(OLD, profile, "root"));
    map[PAGE_BYTES..].copy_from_slice(&literal(OLD, profile, "chunk"));
    put("d3-filemap-0", map, control::CONTAINER_BYTES);
    put("d3-filemap-1", vec![0xa5; 17], control::CONTAINER_BYTES);
    put(
        "d3-data-0000000100000001",
        vec![0xa5; 17],
        control::EXTENT_BYTES,
    );
    put(
        "d3-dwb-stage",
        literal(DWB, profile, "d3-dwb-stage"),
        PAGE_BYTES as u64,
    );
    put("DWB", literal(DWB, profile, "DWB"), 4096);
    put(
        "d3-dwb-0",
        literal(DWB, profile, "empty"),
        PAGE_BYTES as u64,
    );
    put("d3-dwb-1", vec![0xa5; 17], PAGE_BYTES as u64);
    put("d3-dwb-images", vec![0xa5; 17], 16_711_680);
    put("d3-dwb-reserved", vec![0xa5; 17], 8192);
    assert_eq!(state.entries.len(), 14);
    Directory(Arc::new(Mutex::new(state)))
}
fn image(directory: &Directory) -> BTreeMap<String, Entry> {
    directory.0.lock().unwrap().entries.clone()
}
fn unchanged(directory: &Directory, before: &BTreeMap<String, Entry>) {
    let state = directory.0.lock().unwrap();
    assert_eq!(&state.entries, before);
    assert_eq!(state.mutation_calls, 0);
    assert_eq!(state.handles, 0);
    assert!(!state.active);
    assert!(state.peak <= 2);
}
fn acquire(directory: &Directory) -> Result<RecoverySelection<Writer>> {
    RecoverySelection::acquire(directory, Some(ns()), &mut scratch())
}
fn rejected(directory: &Directory) -> RecoverySelectError {
    match acquire(directory) {
        Err(error) => error,
        Ok(_) => panic!("unexpected SELECT success"),
    }
}
fn reseal(raw: &mut [u8]) {
    let end = raw.len() - 32;
    let digest = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&digest);
}
fn set64(raw: &mut [u8], offset: usize, value: u64) {
    raw[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn mutate(directory: &Directory, path: &str, change: impl FnOnce(&mut Vec<u8>)) {
    change(
        &mut directory
            .0
            .lock()
            .unwrap()
            .entries
            .get_mut(path)
            .unwrap()
            .bytes,
    );
}
fn update_control_ref(directory: &Directory, path: &str, target: &[u8]) {
    mutate(directory, path, |raw| {
        raw[112..144].copy_from_slice(&content_digest(target));
        reseal(raw);
    });
}
fn assert_read_set(directory: &Directory) {
    let state = directory.0.lock().unwrap();
    let expected: BTreeSet<_> = [
        "d3-profile",
        "FILEMAP",
        "d3-filemap-0",
        "d3-bootstrap",
        "DWB",
        "d3-dwb-0",
        "d3-dwb-stage",
        "d3-wal-00000001",
        "WRITER",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(
        state
            .reads
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert!(
        state
            .reads
            .iter()
            .filter(|(name, _, _)| name == "d3-dwb-0")
            .all(|(_, offset, len)| *offset == 0 && *len == 160)
    );
}

#[test]
fn initial_profiles_and_plans_are_selected_without_home_reads() {
    run(&ids("RP", 6), |n, _| {
        let profile = n as u32 / 2 + 1;
        let plan = n as u64 % 2 + 1;
        let directory = directory(profile, plan);
        let before = image(&directory);
        let owner = acquire(&directory).unwrap();
        let facts = owner.observation();
        assert_eq!(facts.namespace(), ns());
        assert_eq!(facts.profile_id(), profile);
        assert_eq!(facts.bootstrap_plan(), plan);
        assert_eq!(facts.filemap_generation(), 1);
        assert_eq!(facts.doublewrite_generation(), 1);
        assert_eq!(
            facts.epoch_digest(),
            &content_digest(&literal(OLD, profile, "d3-wal-00000001"))
        );
        drop(owner);
        assert_read_set(&directory);
        unchanged(&directory, &before);
    });
}
#[test]
fn unsupported_closures_preserve_all_input_files() {
    run(&ids("RU", 12), |n, _| {
        let directory = directory(1, 2);
        match n {
            0 => {
                let mut state = directory.0.lock().unwrap();
                let entry = state.entries.get_mut(wal_io::WAL_NAME).unwrap();
                entry.bytes.push(0);
                entry.length += 1;
            }
            1 | 2 => {
                let label = if n == 1 { "pending" } else { "retirement" };
                let selector = bytes(FLUSH, &format!("p1-n1-reserve-{label}.selector"));
                mutate(&directory, "DWB", |raw| *raw = selector);
            }
            3 | 4 => mutate(&directory, if n == 3 { "FILEMAP" } else { "DWB" }, |raw| {
                set64(raw, 32, 2);
                reseal(raw);
            }),
            _ => {
                let mut state = directory.0.lock().unwrap();
                let entry = state.entries["WRITER"].clone();
                state.entries.insert(BLOCKED_NAMES[n - 5].into(), entry);
            }
        }
        let before = image(&directory);
        let error = rejected(&directory);
        assert!(matches!(error.cause, Cause::UnsupportedClosure));
        assert_eq!(error.effect, Effect::Unchanged);
        let state = directory.0.lock().unwrap();
        if n == 0 {
            assert!(
                !state
                    .reads
                    .iter()
                    .any(|(name, _, _)| name == wal_io::WAL_NAME)
            );
        }
        if n == 3 {
            assert!(!state.operations.iter().any(|op| op == "open:d3-filemap-0"));
        }
        if n == 1 || n == 2 || n == 4 {
            assert!(
                !state
                    .operations
                    .iter()
                    .any(|op| op == "open:d3-dwb-0" || op == "open:d3-dwb-1")
            );
        }
        drop(state);
        unchanged(&directory, &before);
    });
}
#[test]
fn bootstrap_and_epoch_support_is_explicit() {
    run(&ids("RB", 8), |n, _| {
        let directory = directory(1, 2);
        match n {
            0 => mutate(&directory, "d3-bootstrap", |raw| {
                set64(raw, 280, 3);
                reseal(raw);
            }),
            1 => mutate(&directory, "d3-profile", |raw| {
                set64(raw, 40, 2);
                reseal(raw);
            }),
            2 => mutate(&directory, "d3-profile", |raw| {
                set64(raw, 32, 2);
                reseal(raw);
            }),
            3 => mutate(&directory, wal_io::WAL_NAME, |raw| {
                raw[40..44].copy_from_slice(&2u32.to_le_bytes());
                reseal(raw);
            }),
            4 => mutate(&directory, "WRITER", |raw| {
                raw[48..52].copy_from_slice(&2u32.to_le_bytes());
                reseal(raw);
            }),
            5 => mutate(&directory, "d3-dwb-stage", |raw| {
                raw[..4].copy_from_slice(b"XDW1")
            }),
            6 => {
                mutate(&directory, "d3-dwb-0", |raw| {
                    set64(raw, 56, 1);
                    reseal(raw);
                });
                let target = image(&directory)["d3-dwb-0"].bytes.clone();
                update_control_ref(&directory, "DWB", &target);
            }
            _ => {}
        }
        let before = image(&directory);
        let error = if n == 2 {
            match RecoverySelection::acquire(&directory, None, &mut scratch()) {
                Err(error) => error,
                Ok(_) => panic!("unexpected timeline success"),
            }
        } else if n == 7 {
            match RecoverySelection::acquire(
                &directory,
                Some(Namespace {
                    timeline: 0,
                    ..ns()
                }),
                &mut scratch(),
            ) {
                Err(error) => error,
                Ok(_) => panic!("unexpected invalid namespace success"),
            }
        } else {
            rejected(&directory)
        };
        match n {
            0 => assert!(matches!(error.cause, Cause::UnsupportedBootstrapPlan)),
            1 | 2 | 5 => assert!(matches!(error.cause, Cause::UnsupportedClosure)),
            3 | 4 => assert!(matches!(
                error.cause,
                Cause::Control(ControlError::IdentityMismatch)
            )),
            6 => assert!(matches!(error.cause, Cause::Control(ControlError::Corrupt))),
            _ => {
                assert_eq!(error.phase, Phase::Preflight);
                assert!(matches!(
                    error.cause,
                    Cause::Control(ControlError::InvalidInput)
                ));
                assert_eq!(directory.0.lock().unwrap().acquire_calls, 0);
            }
        }
        unchanged(&directory, &before);
    });
}

fn corrupt(directory: &Directory, family: usize, mode: usize) {
    let path = FAMILIES[family];
    if path == "root" || path == "chunk" {
        let start = if path == "root" { 0 } else { PAGE_BYTES };
        let length = if path == "root" { 320 } else { 256 };
        mutate(directory, "d3-filemap-0", |raw| {
            let control = &mut raw[start..start + length];
            match mode {
                0 => control[127] ^= 1,
                1 => control[7] = 1,
                2 => control[16] ^= 1,
                _ => control[4] = 2,
            }
            if mode != 0 {
                reseal(control);
            }
            if start != 0 {
                let digest = content_digest(control);
                raw[224..256].copy_from_slice(&digest);
                reseal(&mut raw[..320]);
            }
        });
        let root = image(directory)["d3-filemap-0"].bytes[..320].to_vec();
        update_control_ref(directory, "FILEMAP", &root);
    } else {
        mutate(directory, path, |raw| match mode {
            0 => {
                let end = raw.len() - 1;
                raw[end] ^= 1;
            }
            1 => {
                match path {
                    "d3-profile" => raw[316] = 1,
                    "FILEMAP" | "DWB" => raw[7] = 1,
                    "d3-bootstrap" => set64(raw, 88, 1),
                    "d3-dwb-stage" => set64(raw, 88, 1),
                    "d3-dwb-0" => raw[7] = 1,
                    "d3-wal-00000001" => set64(raw, 96, 1),
                    _ => set64(raw, 136, 1),
                }
                reseal(raw);
            }
            2 => {
                let offset = if path == "d3-dwb-0" { 8 } else { 16 };
                raw[offset] ^= 1;
                reseal(raw);
            }
            _ => {
                raw[4] = 2;
                reseal(raw);
            }
        });
        if path == "d3-dwb-0" {
            let target = image(directory)[path].bytes.clone();
            update_control_ref(directory, "DWB", &target);
        }
    }
}
#[test]
fn selected_corruption_and_semantic_mismatch_are_rejected() {
    run(&ids("RC", 20), |n, _| {
        let directory = directory(1, 2);
        corrupt(&directory, n / 2, n % 2);
        let before = image(&directory);
        let error = rejected(&directory);
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(!matches!(
            error.cause,
            Cause::UnsupportedClosure | Cause::UnsupportedBootstrapPlan
        ));
        unchanged(&directory, &before);
    });
}
#[test]
fn required_lengths_missing_and_handle_identity_are_checked() {
    run(&ids("RG", 48), |n, _| {
        let directory = directory(1, 2);
        if n < 28 {
            let mut state = directory.0.lock().unwrap();
            let entry = state.entries.get_mut(REQUIRED[n / 2]).unwrap();
            if n.is_multiple_of(2) {
                entry.length -= 1;
            } else {
                entry.length += 1;
            }
        } else if n < 42 {
            directory.0.lock().unwrap().entries.remove(REQUIRED[n - 28]);
        } else if n < 44 {
            directory.0.lock().unwrap().replacement = Some(("d3-profile".into(), n == 43));
        } else if n < 46 {
            let path = if n == 44 {
                "d3-profile"
            } else {
                "d3-filemap-0"
            };
            let mut state = directory.0.lock().unwrap();
            state.entries.get_mut(path).unwrap().length -= 1;
            state.replacement = Some((path.into(), false));
        } else {
            directory.0.lock().unwrap().replacement = Some(("d3-filemap-0".into(), n == 47));
        }
        let before = image(&directory);
        let error = rejected(&directory);
        if n < 28 {
            assert!(
                matches!(
                    error.cause,
                    Cause::LengthMismatch
                        | Cause::PageIo(PageIoError::LengthMismatch)
                        | Cause::PageIo(PageIoError::Namespace(NamespaceError {
                            kind: NamespaceErrorKind::LimitExceeded,
                            ..
                        }))
                ) || (REQUIRED[n / 2] == wal_io::WAL_NAME
                    && n % 2 == 1
                    && matches!(error.cause, Cause::UnsupportedClosure))
            );
        } else if n < 42 {
            assert!(matches!(
                error.cause,
                Cause::Namespace(_) | Cause::PageIo(PageIoError::Namespace(_))
            ));
        } else if n == 42 {
            assert!(matches!(error.cause, Cause::IdentityMismatch));
        } else if n == 43 {
            assert!(matches!(error.cause, Cause::LengthMismatch));
        } else if n == 44 {
            assert!(matches!(error.cause, Cause::LengthMismatch));
            assert!(
                !directory
                    .0
                    .lock()
                    .unwrap()
                    .operations
                    .iter()
                    .any(|op| op == "metadata:d3-profile")
            );
        } else if n == 45 {
            assert!(matches!(
                error.cause,
                Cause::PageIo(PageIoError::Namespace(NamespaceError {
                    kind: NamespaceErrorKind::LimitExceeded,
                    ..
                }))
            ));
            assert!(
                !directory
                    .0
                    .lock()
                    .unwrap()
                    .operations
                    .iter()
                    .any(|op| op == "metadata:d3-filemap-0")
            );
        } else {
            let expected = if n == 46 {
                NamespaceErrorKind::InvalidType
            } else {
                NamespaceErrorKind::LimitExceeded
            };
            assert!(
                matches!(error.cause, Cause::PageIo(PageIoError::Namespace(NamespaceError { kind, .. })) if kind == expected)
            );
        }
        unchanged(&directory, &before);
    });
}
#[test]
fn selection_io_faults_release_every_capability() {
    run(&ids("RF", 61), |n, _| {
        let directory = directory(1, 2);
        let before = image(&directory);
        if n < BASE_OPS {
            directory.0.lock().unwrap().fail_at = Some(n + 1);
            let error = rejected(&directory);
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(
                error.cause,
                Cause::Namespace(_) | Cause::Io(_) | Cause::PageIo(_)
            ));
            assert_eq!(directory.0.lock().unwrap().operations.len(), n + 1);
        } else {
            match n {
                56 => {
                    directory.0.lock().unwrap().short = Some(17);
                    drop(acquire(&directory).unwrap());
                }
                57 => {
                    directory.0.lock().unwrap().interrupted = true;
                    drop(acquire(&directory).unwrap());
                }
                58 => {
                    directory.0.lock().unwrap().zero = true;
                    assert!(matches!(rejected(&directory).cause, Cause::Io(_)));
                }
                59 => {
                    directory.0.lock().unwrap().panic_at = Some(11);
                    assert!(
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| acquire(
                            &directory
                        )))
                        .is_err()
                    );
                }
                _ => {
                    directory.0.lock().unwrap().acquired_effect = Some(Effect::CallerMustResolve);
                    let error = rejected(&directory);
                    assert_eq!(error.phase, Phase::Lease);
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                }
            }
        }
        unchanged(&directory, &before);
    });
}
#[test]
fn closed_owner_lease_and_scratch_lifetimes() {
    run(&ids("RL", 5), |n, _| {
        let directory = directory(1, 2);
        let before = image(&directory);
        match n {
            0 => {
                let owner = acquire(&directory).unwrap();
                assert!(matches!(
                    rejected(&directory).cause,
                    Cause::Namespace(NamespaceError {
                        kind: NamespaceErrorKind::Busy,
                        ..
                    })
                ));
                drop(owner);
            }
            1 => {
                drop(acquire(&directory).unwrap());
                drop(acquire(&directory).unwrap());
            }
            2 => {
                directory.0.lock().unwrap().fail_at = Some(11);
                let _ = rejected(&directory);
                directory.0.lock().unwrap().fail_at = None;
                drop(acquire(&directory).unwrap());
            }
            3 => {
                let mut workspace = scratch();
                let owner =
                    RecoverySelection::acquire(&directory, Some(ns()), &mut workspace).unwrap();
                workspace[0].fill(0xaa);
                assert_eq!(owner.observation().profile_id(), 1);
                drop(owner);
            }
            _ => {
                drop(acquire(&directory).unwrap());
                assert_eq!(directory.0.lock().unwrap().mutation_calls, 0);
            }
        }
        unchanged(&directory, &before);
    });
}

struct LargeDirectory<const A: usize, const B: usize>(std::cell::Cell<usize>);
struct LargeWriter<const A: usize, const B: usize>([u8; A]);
struct LargeFile<const B: usize>([u8; B]);
impl<const B: usize> FileIo for LargeFile<B> {
    fn read_at(&mut self, _: &mut [u8], _: u64) -> io::Result<usize> {
        panic!("oversized port read")
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        panic!("oversized port write")
    }
    fn sync_all(&mut self) -> io::Result<()> {
        panic!("oversized port sync")
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        panic!("oversized port resize")
    }
}
impl<const A: usize, const B: usize> DirectoryPort for LargeDirectory<A, B> {
    type Writer = LargeWriter<A, B>;
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        self.0.set(self.0.get() + 1);
        Err(forbidden())
    }
}
impl<const A: usize, const B: usize> WriterNamespace for LargeWriter<A, B> {
    type File = LargeFile<B>;
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
fn source_owned_resource_bound_is_preflighted() {
    run(&ids("RQ", 3), |n, _| match n {
        0 => {
            let b = source_budget::<Writer>();
            println!(
                "RECOVERY_SELECT_SOURCE_BUDGET owner={} file={} outer={} preparation_nested={} reader_nested={} preparation={} reader={} io={} peak={} caller=49152",
                b.owner,
                b.file,
                b.outer,
                b.preparation_nested,
                b.reader_nested,
                b.preparation,
                b.reader,
                b.io,
                b.peak
            );
            assert!(capacity::<Writer>());
            assert!(b.peak <= 65536);
        }
        1 => {
            let directory = LargeDirectory::<65536, 8>(std::cell::Cell::new(0));
            let error = match RecoverySelection::acquire(&directory, None, &mut scratch()) {
                Err(e) => e,
                Ok(_) => panic!("oversized success"),
            };
            assert!(matches!(error.cause, Cause::Capacity));
            assert_eq!(directory.0.get(), 0);
        }
        _ => {
            let directory = LargeDirectory::<8, 65536>(std::cell::Cell::new(0));
            let error = match RecoverySelection::acquire(&directory, None, &mut scratch()) {
                Err(e) => e,
                Ok(_) => panic!("oversized success"),
            };
            assert!(matches!(error.cause, Cause::Capacity));
            assert_eq!(directory.0.get(), 0);
        }
    });
}
#[test]
fn unselected_and_home_bytes_are_never_read() {
    run(&ids("RN", 1), |_, _| {
        let directory = directory(1, 2);
        // Distinct corrupt bytes in every non-content source; arbitrary orphan
        // names remain unobserved and must not become a fallback/inventory claim.
        {
            let mut state = directory.0.lock().unwrap();
            for (n, path) in [
                "d3-data-0000000100000001",
                "d3-filemap-1",
                "d3-dwb-1",
                "d3-dwb-images",
                "d3-dwb-reserved",
            ]
            .iter()
            .enumerate()
            {
                state
                    .entries
                    .get_mut(*path)
                    .unwrap()
                    .bytes
                    .fill(n as u8 + 0xb0);
            }
            let orphan = state.entries["WRITER"].clone();
            state.entries.insert("d3-wal-00000099".into(), orphan);
        }
        let before = image(&directory);
        drop(acquire(&directory).unwrap());
        assert_read_set(&directory);
        let state = directory.0.lock().unwrap();
        assert_eq!(state.operations.len(), BASE_OPS);
        assert!(!state.operations.iter().any(|op| op.contains("00000099")));
        println!("RECOVERY_SELECT_OPERATIONS {}", state.operations.join("|"));
        drop(state);
        unchanged(&directory, &before);
    });
}
#[test]
fn shared_preparation_keeps_current_producer_bytes() {
    run(&ids("RS", 6), |n, _| {
        let profile = n as u32 / 2 + 1;
        let plan = n as u64 % 2 + 1;
        let directory = directory(profile, plan);
        let raw = image(&directory);
        let profile =
            control::decode_bootstrap_profile(&raw["d3-profile"].bytes, Some(ns())).unwrap();
        let selected =
            control::decode_selector(&raw["FILEMAP"].bytes, SelectorRole::FileMap, profile)
                .unwrap();
        let mut work = scratch();
        work[0][..4096].copy_from_slice(&raw["d3-bootstrap"].bytes);
        let expected = wal_io::inspect_initial_bootstrap(profile, selected, &mut work).unwrap();
        assert_eq!(expected.plan, plan);
        assert_eq!(expected.filemap, selected);
        assert_eq!(
            expected.stage_digest,
            content_digest(&raw["d3-dwb-stage"].bytes)
        );
        assert_eq!(
            expected.doublewrite,
            control::decode_selector(&raw["DWB"].bytes, SelectorRole::Doublewrite, profile)
                .unwrap()
        );
        assert_eq!(work[0].as_slice(), raw["d3-dwb-stage"].bytes.as_slice());
    });
}

#[test]
#[ignore = "DEV3 final S5 expanded input inventory only"]
fn s5_initial_selection_expanded_inputs() {
    let cases: Vec<_> = (0..1176).map(|n| format!("RX{:04}", n + 1)).collect();
    let mut lifecycle = BTreeMap::<usize, Directory>::new();
    run(&cases, |n, _| {
        if n < 240 {
            let seed = n / 40;
            let directory = directory(seed as u32 / 2 + 1, seed as u64 % 2 + 1);
            corrupt(&directory, (n % 40) / 4, n % 4);
            let before = image(&directory);
            let _ = rejected(&directory);
            unchanged(&directory, &before);
        } else if n < 408 {
            let n = n - 240;
            let seed = n / 28;
            let directory = directory(seed as u32 / 2 + 1, seed as u64 % 2 + 1);
            {
                let mut state = directory.0.lock().unwrap();
                let entry = state.entries.get_mut(REQUIRED[(n % 28) / 2]).unwrap();
                if n.is_multiple_of(2) {
                    entry.length -= 1;
                } else {
                    entry.length += 1;
                }
            }
            let before = image(&directory);
            let _ = rejected(&directory);
            unchanged(&directory, &before);
        } else {
            let seed = (n - 408) / 128;
            let directory = lifecycle
                .entry(seed)
                .or_insert_with(|| directory(seed as u32 / 2 + 1, seed as u64 % 2 + 1));
            let before = image(directory);
            drop(acquire(directory).unwrap());
            unchanged(directory, &before);
        }
    });
}
