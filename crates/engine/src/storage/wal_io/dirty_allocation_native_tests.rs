//! New dirty-owner native workers; the parent runner retains three Linux DBs.
//! Fixed fixture entropy is the only namespace substitution; all effects use Linux.
use super::*;
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    io::{FileIo, write_all_at},
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

use crate::storage::{
    dirty_frame::{DirtyFrame, test_support as frame_support},
    wal_io as wal,
};
use crate::transaction::concurrency::OwnerRole;

const WAL: &str = "d3-wal-00000001";
const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
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

fn record(
    writer: &AllocationWriter<'_, '_, '_, '_, '_, '_, '_, '_, Writer>,
    case: &Path,
    cancel: bool,
) {
    let part = if cancel { "cancel" } else { "reserve" };
    let mut page = boxed::<PAGE_BYTES>();
    let snapshot = frame_support::diagnose(writer.pool, 0, &mut page);
    assert_eq!(snapshot.frame.identity, writer.control.identity);
    assert_eq!(snapshot.frame.generation, 1);
    assert_eq!(&page[..], literal(1, 1, &format!("{part}.page")));
    assert_eq!(
        &writer.control.descriptor[..],
        literal(1, 1, &format!("{part}.descriptor"))
    );
    let lsn = if cancel { 2 } else { 1 };
    assert_eq!(snapshot.frame.page_lsn, PageLsn(lsn));
    assert_eq!(snapshot.frame.modification_counter, lsn);
    assert_eq!(snapshot.frame.rec_lsn, Some(PageLsn(1)));
    assert!(snapshot.dirty);
    assert_eq!(snapshot.total, 0);
    let entry = snapshot.entry.expect("actual dirty registration");
    assert_eq!(entry.identity, snapshot.frame.identity);
    assert_eq!(entry.generation, 1);
    assert_eq!(entry.rec_lsn, PageLsn(1));
    assert_eq!(entry.modification_counter, lsn);
    fs::write(case.join(format!("{part}.page")), &page[..]).unwrap();
    fs::write(
        case.join(format!("{part}.descriptor")),
        writer.control.descriptor,
    )
    .unwrap();
    fs::write(
        case.join(format!("{part}.dirty.tsv")),
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            snapshot.frame.page_lsn.0,
            snapshot.frame.modification_counter,
            snapshot.frame.rec_lsn.unwrap().0,
            snapshot.frame.generation,
            snapshot.dirty,
            snapshot.total
        ),
    )
    .unwrap();
}

#[test]
#[ignore = "three frozen fresh DB cases selected by dev3_dirty_allocation_native.py"]
fn dirty_allocation_native_selected() {
    let case =
        PathBuf::from(std::env::var_os("XADB_DIRTY_NATIVE_CASE").expect("native case directory"));
    let kind = std::env::var("XADB_DIRTY_NATIVE_KIND").expect("frozen native case kind");
    assert!(case.is_absolute());
    assert!(matches!(
        kind.as_str(),
        "normal" | "reserve-write-half" | "cancel-sync-after"
    ));
    let database = case.join("database");
    fs::create_dir(&database).unwrap();
    let directory = LinuxDirectory::open_existing(&database).unwrap();
    let trace = Arc::new(Mutex::new(Trace::default()));
    let port = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut payload = boxed::<PAGE_BYTES>();
    let mut workspace = boxed::<OPERATION_BYTES>();
    let fresh =
        wal::FreshStorageSession::create(port, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    let before = inventory(&database);
    manifest(&case.join("before.tsv"), &before);
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(&mut payload)];
    let mut bound = storage
        .bind_allocator(&core, &mut frames, &mut workspace)
        .unwrap();
    assert_eq!(inventory(&database), before);
    let (mut writer, _reader) = bound.split();
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut expected = fs::read(database.join(WAL)).unwrap();
    fs::write(case.join("header.bin"), &expected).unwrap();
    assert_eq!(expected.len(), 4096);
    if kind == "normal" {
        trace.lock().unwrap().enabled = true;
        for cancel in [false, true] {
            let observed = if cancel {
                writer.cancel_reserved(&mut owner)
            } else {
                writer.reserve_pages(&mut owner, 1)
            }
            .unwrap();
            let part = if cancel { "cancel" } else { "reserve" };
            expected.extend(literal(1, 1, &format!("{part}.frame")));
            assert_eq!(observed.page_lsn.0, if cancel { 2 } else { 1 });
            assert_eq!(observed.wal.complete_offset, expected.len() as u64);
            assert_eq!(observed.wal.synced_offset, expected.len() as u64);
            assert_eq!(observed.rec_lsn, Some(PageLsn(1)));
            assert_eq!(observed.reserved, usize::from(!cancel));
            assert!(writer.control.installation.is_none());
            assert!(!writer.control.terminal_pending);
            assert_eq!(writer.control.batch.is_some(), !cancel);
            record(&writer, &case, cancel);
        }
        assert!(!storage.session_fence().is_fenced());
    } else {
        let cancel = kind == "cancel-sync-after";
        if cancel {
            writer.reserve_pages(&mut owner, 1).unwrap();
            expected.extend(literal(1, 1, "reserve.frame"));
            record(&writer, &case, false);
        }
        let cut = if cancel { "sync-after" } else { "write-half" };
        {
            let mut trace = trace.lock().unwrap();
            trace.enabled = true;
            trace.cut = cut.into();
        }
        let failure = if cancel {
            writer.cancel_reserved(&mut owner)
        } else {
            writer.reserve_pages(&mut owner, 1)
        }
        .unwrap_err();
        assert_eq!(failure.phase, Phase::Sync);
        assert_eq!(failure.effect, Effect::CallerMustResolve);
        let Cause::Wal(lower) = failure.cause else {
            panic!("lost original WAL cause");
        };
        assert_eq!(
            lower.phase,
            if cancel {
                wal::Phase::Sync
            } else {
                wal::Phase::Write
            }
        );
        assert_eq!(lower.effect, Effect::CallerMustResolve);
        let wal::Cause::Io(io) = lower.cause else {
            panic!("lost original I/O cause");
        };
        assert_eq!(io.code, xadb_foundation::error::ErrorCode::IoWriteUnknown);
        assert_eq!(io.effect(), Effect::CallerMustResolve);
        assert_eq!(io.io_kind, Some(io::ErrorKind::Other));
        fs::write(
            case.join("error.tsv"),
            format!(
                "Sync\tCallerMustResolve\t{:?}\tCallerMustResolve\tIoWriteUnknown\tOther\t{}\n",
                lower.phase, io.context
            ),
        )
        .unwrap();
        assert!(storage.session_fence().is_fenced());
        let rejected = writer.observe(&mut owner).unwrap_err();
        assert_eq!(rejected.effect, Effect::CallerMustResolve);
        assert!(matches!(
            rejected.cause,
            Cause::Storage(MutableStorageError {
                cause: super::super::Cause::Fenced,
                ..
            })
        ));
        let part = if cancel { "cancel" } else { "reserve" };
        let bytes = literal(1, 1, &format!("{part}.frame"));
        expected.extend_from_slice(&bytes[..if cancel { 21200 } else { 10600 }]);
        record(&writer, &case, cancel);
        assert_eq!(writer.control.batch.unwrap().count(), 1);
        assert_eq!(writer.control.terminal_pending, cancel);
        let obligation = writer
            .control
            .installation
            .as_ref()
            .expect("retained stage obligation");
        assert_eq!(obligation.phase, InstallPhase::DptInstalled);
        assert_eq!(obligation.identity, writer.control.identity);
        assert_eq!(obligation.generation, 1);
        assert!(std::ptr::eq(obligation.fence, storage.session_fence()));
        assert_eq!(obligation.cancel, cancel);
        assert_eq!(obligation.first_rec_lsn, PageLsn(1));
        assert_eq!(obligation.before_counter, u64::from(cancel));
        assert_eq!(obligation.after_counter, if cancel { 2 } else { 1 });
        let append = storage.wal().append.lock().unwrap();
        let complete = if cancel { 2 } else { 1 };
        assert_eq!(append.prefix.complete_lsn, complete);
        assert_eq!(append.prefix.synced_lsn, u64::from(cancel));
        let wal::SlotState::IoOwned(location) = append.state else {
            panic!("missing retained actual I/O ownership");
        };
        assert_eq!(obligation.location, location);
        assert_eq!(location.stream, append.prefix.stream);
        assert_eq!(location.file, append.prefix.file);
        assert_eq!(location.previous_lsn, u64::from(cancel));
        assert_eq!(location.lsn, complete);
        assert_eq!(location.start, if cancel { 25296 } else { 4096 });
        assert_eq!(location.length, 21200);
        assert_eq!(location.end, append.prefix.complete_offset);
        assert!(append.slot.is_none());
        fs::write(
            case.join("obligation.tsv"),
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                obligation.generation,
                obligation.location.previous_lsn,
                obligation.location.lsn,
                obligation.location.start,
                obligation.location.length,
                obligation.location.end,
                std::ptr::eq(obligation.fence, storage.session_fence()),
                obligation.identity == writer.control.identity
            ),
        )
        .unwrap();
        fs::write(
            case.join("fence.tsv"),
            format!(
                "true\t{complete}\t{}\t{}\t1\t{}\tDptInstalled\t1\n",
                u64::from(cancel),
                append.prefix.complete_offset,
                cancel
            ),
        )
        .unwrap();
    }
    assert_eq!(fs::read(database.join(WAL)).unwrap(), expected);
    drop(owner);
    drop(bound);
    drop(core);
    drop(storage);
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
    assert_eq!(trace.hits, usize::from(kind != "normal"));
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
            "PASS\t{kind}\t{}\t{}\t{}\n",
            trace.hits,
            trace.peak,
            option_env!("XADB_DIRTY_SOURCE_SHA256").unwrap_or("MISSING_COMPILE_IDENTITY")
        ),
    )
    .unwrap();
    println!("\nXADB_DIRTY_NATIVE_DONE");
}
