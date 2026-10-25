//! Native AR02/AR06 workers. The Python runner owns fork, timeout and evidence.
//! Fixed fixture entropy is the only namespace substitution; all effects use Linux.
use super::*;
use foundation::{
    codec::content_digest,
    io::{FileIo, write_all_at},
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
    },
};
use platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const WAL: &str = "d3-wal-00000001";
const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocation-reservation"
);
const DB_LIMIT: u64 = 24 << 20;

#[derive(Default)]
struct Trace {
    enabled: bool,
    cut: String,
    hits: usize,
    events: Vec<String>,
    handles: usize,
    peak: usize,
}
struct Writer {
    inner: LinuxWriterLease,
    trace: Arc<Mutex<Trace>>,
}
struct File {
    inner: LinuxNamespaceFile,
    name: String,
    trace: Arc<Mutex<Trace>>,
}
impl Drop for File {
    fn drop(&mut self) {
        self.trace.lock().unwrap().handles -= 1;
    }
}
impl File {
    fn cut(&self, operation: &str) -> String {
        let trace = self.trace.lock().unwrap();
        if trace.enabled && self.name == WAL && trace.cut.starts_with(operation) {
            assert_eq!(trace.hits, 0, "fault must only execute once");
            trace.cut.clone()
        } else {
            String::new()
        }
    }
    fn event(&self, operation: &str, offset: u64, requested: usize, written: usize, cut: &str) {
        let mut trace = self.trace.lock().unwrap();
        if trace.enabled {
            assert_eq!(self.name, WAL, "allocation may only mutate WAL");
            assert!(trace.events.len() < 12);
            trace.events.push(format!(
                "{operation}\t{offset}\t{requested}\t{written}\t{cut}"
            ));
            trace.hits += usize::from(!cut.is_empty());
        }
    }
}
fn fault() -> io::Error {
    io::Error::other("fixed allocation native cut")
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.inner.read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let cut = self.cut("write-");
        if cut.is_empty() {
            let written = self.inner.write_at(bytes, offset)?;
            self.event("write", offset, bytes.len(), written, "");
            return Ok(written);
        }
        let written = match cut.as_str() {
            "write-before" => 0,
            "write-half" => bytes.len() / 2,
            "write-after" => bytes.len(),
            _ => panic!("unrecognized fixed cut"),
        };
        if written != 0 {
            write_all_at(&mut self.inner, &bytes[..written], offset).unwrap();
        }
        self.event("write", offset, bytes.len(), written, &cut);
        Err(fault())
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let cut = self.cut("sync-");
        if cut != "sync-before" {
            self.inner.sync_all()?;
        }
        self.event("sync", 0, 0, 0, &cut);
        if cut.is_empty() { Ok(()) } else { Err(fault()) }
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        assert!(
            !self.trace.lock().unwrap().enabled,
            "unexpected allocation resize"
        );
        self.inner.set_len(len)
    }
}
impl Writer {
    fn wrap(&self, file: LinuxNamespaceFile, name: &FileName) -> File {
        let mut trace = self.trace.lock().unwrap();
        trace.handles += 1;
        trace.peak = trace.peak.max(trace.handles);
        File {
            inner: file,
            name: name.to_string(),
            trace: Arc::clone(&self.trace),
        }
    }
    fn bootstrap_only(&self) {
        assert!(
            !self.trace.lock().unwrap().enabled,
            "unexpected allocation namespace effect"
        );
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.bootstrap_only();
        // Deterministic native fixture identity. Entropy generation is not tested here.
        Ok(std::array::from_fn(|i| (i + 1) as u8))
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        self.bootstrap_only();
        let file = self.inner.create_new(name)?;
        Ok(self.wrap(file, name))
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        let file = self.inner.open_existing(name, mode)?;
        Ok(self.wrap(file, name))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let (file, metadata) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.wrap(file, name), metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.bootstrap_only();
        self.inner.sync_directory()
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        self.bootstrap_only();
        self.inner.publish_selector(temporary, selected)
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        self.bootstrap_only();
        self.inner.remove_retired(name)
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn literal(profile: u32, n: usize, part: &str) -> Vec<u8> {
    fs::read(format!("{INPUT}/p{profile}-n{n}-{part}")).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn inventory(path: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut size = 0;
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        assert!(meta.file_type().is_file());
        assert_eq!(meta.nlink(), 1);
        size += meta.len();
        let name = entry.file_name().into_string().unwrap();
        let hash = hex(&content_digest(&fs::read(entry.path()).unwrap()));
        result.insert(
            name,
            format!(
                "{}\t{}\t{}\t{}\t{hash}",
                meta.len(),
                meta.dev(),
                meta.ino(),
                meta.nlink()
            ),
        );
    }
    assert!(size <= DB_LIMIT);
    result
}
fn manifest(path: &Path, files: &BTreeMap<String, String>) {
    fs::write(
        path,
        files
            .iter()
            .map(|(name, row)| format!("{name}\t{row}\n"))
            .collect::<String>(),
    )
    .unwrap();
}
fn record_step(
    owner: &AllocationSession<'_, '_, '_, '_, Writer>,
    case: &Path,
    profile: u32,
    n: usize,
    cancel: bool,
) {
    let part = if cancel { "cancel" } else { "reserve" };
    assert_eq!(
        &owner.state.frame[..],
        literal(profile, n, &format!("{part}.page"))
    );
    assert_eq!(
        &owner.state.descriptor[..],
        literal(profile, n, &format!("{part}.descriptor"))
    );
    fs::write(
        case.join(format!("n{n}-{part}.page")),
        &owner.state.frame[..],
    )
    .unwrap();
    fs::write(
        case.join(format!("n{n}-{part}.descriptor")),
        &owner.state.descriptor[..],
    )
    .unwrap();
}
fn success(
    owner: &mut AllocationSession<'_, '_, '_, '_, Writer>,
    case: &Path,
    profile: u32,
    n: usize,
    cancel: bool,
    expected: &mut Vec<u8>,
) {
    let view = if cancel {
        owner.cancel_reserved()
    } else {
        owner.reserve_pages(n)
    }
    .unwrap();
    let part = if cancel { "cancel" } else { "reserve" };
    expected.extend(literal(profile, n, &format!("{part}.frame")));
    assert_eq!(
        fs::read(case.join("database").join(WAL)).unwrap(),
        *expected
    );
    record_step(owner, case, profile, n, cancel);
    let lsn = match n {
        1 => 1,
        8 => 3,
        63 => 5,
        _ => unreachable!(),
    } + u64::from(cancel);
    assert_eq!(view.page_lsn, PageLsn(lsn));
    assert_eq!(view.rec_lsn, Some(PageLsn(1)));
    assert_eq!(view.modification_counter, lsn);
    assert_eq!(view.work_high_water, lsn.div_ceil(2));
    assert_eq!(view.reserved, if cancel { 0 } else { n });
    assert_eq!(view.wal.complete_offset, expected.len() as u64);
    assert_eq!(view.wal.synced_offset, expected.len() as u64);
    assert_eq!(
        view.protected_bytes,
        21_024_768 + expected.len() as u64 - 4096
            + 16384
            + if cancel {
                0
            } else {
                (20992 + 208 * n + 16384 * n) as u64
            }
    );
    let tokens = literal(profile, n, "items.bin");
    for ordinal in 0..n {
        if cancel {
            assert_eq!(owner.reserved_page(ordinal).unwrap(), None);
        } else {
            let mut encoded = [0; 24];
            owner
                .reserved_page(ordinal)
                .unwrap()
                .unwrap()
                .encode(&mut encoded)
                .unwrap();
            assert_eq!(&encoded, &tokens[ordinal * 96 + 8..ordinal * 96 + 32]);
        }
    }
    assert_eq!(owner.reserved_page(n).unwrap(), None);
}
fn run(profile: u32, operation: &str, cut: &str) {
    let case = PathBuf::from(
        std::env::var_os("XADB_ALLOCATION_CASE")
            .expect("run via scripts/dev3_allocation_native.py"),
    );
    assert!(case.is_absolute());
    let database = case.join("database");
    fs::create_dir(&database).unwrap();
    let directory = LinuxDirectory::open_existing(&database).unwrap();
    let trace = Arc::new(Mutex::new(Trace::default()));
    let writer = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut frame = boxed::<PAGE_BYTES>();
    let mut workspace = boxed::<OPERATION_BYTES>();
    let fresh = super::super::super::FreshStorageSession::create(
        writer,
        profile,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    let before = inventory(&database);
    manifest(&case.join("before.tsv"), &before);
    let mut owner = installed
        .into_allocation_session(&mut frame, &mut workspace)
        .unwrap();
    assert_eq!(inventory(&database), before);
    let mut expected = fs::read(database.join(WAL)).unwrap();
    fs::write(case.join("header.bin"), &expected).unwrap();
    assert_eq!(expected.len(), 4096);
    if operation == "profiles" {
        trace.lock().unwrap().enabled = true;
        for n in [1, 8, 63] {
            success(&mut owner, &case, profile, n, false, &mut expected);
            success(&mut owner, &case, profile, n, true, &mut expected);
        }
        assert_eq!(expected.len(), 160000);
        assert!(!owner.is_fenced());
    } else {
        success(&mut owner, &case, profile, 1, false, &mut expected);
        success(&mut owner, &case, profile, 1, true, &mut expected);
        let cancel = operation == "cancel";
        if cancel {
            success(&mut owner, &case, profile, 8, false, &mut expected);
        }
        {
            let mut trace = trace.lock().unwrap();
            trace.enabled = true;
            trace.cut = cut.into();
        }
        let prior = owner.observe().unwrap();
        let error = if cancel {
            owner.cancel_reserved()
        } else {
            owner.reserve_pages(8)
        }
        .unwrap_err();
        assert_eq!(error.phase, Phase::Sync);
        assert_eq!(error.effect, Effect::CallerMustResolve);
        let Cause::Wal(lower) = error.cause else {
            panic!("original WAL error missing")
        };
        assert_eq!(
            lower.phase,
            if cut.starts_with("write-") {
                wal::Phase::Write
            } else {
                wal::Phase::Sync
            }
        );
        assert_eq!(lower.effect, Effect::CallerMustResolve);
        let wal::Cause::Io(io) = lower.cause else {
            panic!("original I/O cause missing")
        };
        assert_eq!(io.code, foundation::error::ErrorCode::IoWriteUnknown);
        assert_eq!(io.effect(), Effect::CallerMustResolve);
        assert_eq!(io.io_kind, Some(io::ErrorKind::Other));
        assert_eq!(
            io.context,
            if cut.starts_with("write-") {
                "write outcome requires caller resolution"
            } else {
                "sync outcome unknown"
            }
        );
        fs::write(
            case.join("error.tsv"),
            format!(
                "Sync\tCallerMustResolve\t{:?}\tCallerMustResolve\tIoWriteUnknown\tOther\t{}\n",
                lower.phase, io.context
            ),
        )
        .unwrap();
        assert!(owner.is_fenced());
        assert!(owner.installed.is_fenced());
        assert!(owner.installed.fresh.core.wal.fenced.is_fenced());
        for error in [
            owner.observe().unwrap_err(),
            owner.reserved_page(0).unwrap_err(),
            owner.reserve_pages(1).unwrap_err(),
            owner.cancel_reserved().unwrap_err(),
        ] {
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, Cause::Fenced));
        }
        let part = if cancel { "cancel" } else { "reserve" };
        let bytes = literal(profile, 8, &format!("{part}.frame"));
        let written = match cut {
            "write-before" => 0,
            "write-half" => 11328,
            _ => 22656,
        };
        expected.extend_from_slice(&bytes[..written]);
        assert_eq!(fs::read(database.join(WAL)).unwrap(), expected);
        record_step(&owner, &case, profile, 8, cancel);
        assert_eq!(owner.state.terminal_pending, cancel);
        assert_eq!(owner.state.batch.unwrap().count(), 8);
        assert_eq!(owner.state.rec_lsn, Some(PageLsn(1)));
        let append = owner.installed.fresh.core.wal.append.lock().unwrap();
        assert_eq!(append.prefix.complete_lsn, prior.page_lsn.0 + 1);
        assert_eq!(append.prefix.synced_lsn, prior.page_lsn.0);
        assert!(matches!(append.state, wal::SlotState::IoOwned(_)));
        assert!(append.slot.is_none());
        let retained = owner.state.protection(append.prefix).unwrap();
        assert_eq!(
            retained,
            21_024_768 + append.prefix.complete_offset - 4096
                + 16384
                + 8 * 16384
                + if cancel { 0 } else { 22656 }
        );
        fs::write(
            case.join("fence.tsv"),
            format!("true\ttrue\ttrue\t4\t{retained}\n"),
        )
        .unwrap();
    }
    drop(owner);
    let after = inventory(&database);
    assert_eq!(before.len(), after.len());
    for (name, value) in &before {
        if name != WAL {
            assert_eq!(after.get(name), Some(value));
        }
    }
    manifest(&case.join("after.tsv"), &after);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.handles, 0);
    assert!(trace.peak <= 5);
    assert_eq!(trace.hits, usize::from(operation != "profiles"));
    fs::write(
        case.join("events.tsv"),
        trace
            .events
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        case.join("worker.tsv"),
        format!(
            "PASS\t{profile}\t{operation}\t{cut}\t{}\t{}\t{}\n",
            trace.hits,
            trace.peak,
            option_env!("XADB_ALLOCATION_SOURCE_SHA256").unwrap_or("MISSING_COMPILE_IDENTITY")
        ),
    )
    .unwrap();
    println!("\nXADB_ALLOCATION_NATIVE_DONE");
}

#[test]
#[ignore = "forked by scripts/dev3_allocation_native.py with retained case directory"]
fn allocation_native_profiles() {
    let profile = std::env::var("XADB_ALLOCATION_PROFILE")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!((1..=3).contains(&profile));
    run(profile, "profiles", "");
}

#[test]
#[ignore = "forked by scripts/dev3_allocation_native.py with retained case directory"]
fn allocation_native_faults() {
    let operation = std::env::var("XADB_ALLOCATION_OPERATION").unwrap();
    let cut = std::env::var("XADB_ALLOCATION_CUT").unwrap();
    assert!(matches!(operation.as_str(), "reserve" | "cancel"));
    assert!(matches!(
        cut.as_str(),
        "write-before" | "write-half" | "write-after" | "sync-before" | "sync-after"
    ));
    run(1, &operation, &cut);
}
