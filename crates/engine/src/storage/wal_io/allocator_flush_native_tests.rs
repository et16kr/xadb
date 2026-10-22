//! Three new Linux DB workers for the actual flush consumer. No simulated I/O.
use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, test_support as frame_test},
    page::{PAGE_BYTES, PageLsn},
    wal_io as wal,
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocator-flush-one"
);
const DIRTY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
);
const HOME: &str = "d3-data-0000000100000001";
#[derive(Default)]
struct Trace {
    enabled: bool,
    events: Vec<String>,
    handles: usize,
    peak: usize,
    root: PathBuf,
    fd_baseline: usize,
    fd_peak: usize,
    fd_events: Vec<String>,
}
struct Writer {
    inner: LinuxWriterLease,
    trace: Arc<Mutex<Trace>>,
}
struct File {
    inner: Option<LinuxNamespaceFile>,
    name: String,
    trace: Arc<Mutex<Trace>>,
}
impl Drop for File {
    fn drop(&mut self) {
        drop(self.inner.take());
        let mut trace = self.trace.lock().unwrap();
        trace.handles -= 1;
        if trace.enabled {
            sample_fd(&mut trace, "close");
        }
    }
}
fn db_fds(root: &Path) -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .filter(|path| path.starts_with(root))
        .count()
}
fn sample_fd(trace: &mut Trace, tag: &str) {
    let count = db_fds(&trace.root);
    trace.fd_peak = trace.fd_peak.max(count);
    trace.fd_events.push(format!("{tag}\t{count}"));
}
fn event(
    trace: &Arc<Mutex<Trace>>,
    op: &str,
    name: &str,
    offset: u64,
    requested: usize,
    returned: usize,
) {
    let mut trace = trace.lock().unwrap();
    if trace.enabled {
        sample_fd(&mut trace, op);
        assert!(trace.events.len() < 64);
        trace
            .events
            .push(format!("{op}\t{name}\t{offset}\t{requested}\t{returned}"));
    }
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        let result = self.inner.as_mut().unwrap().read_at(bytes, offset);
        let mut trace = self.trace.lock().unwrap();
        if trace.enabled {
            sample_fd(&mut trace, "read");
        }
        result
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let count = self.inner.as_mut().unwrap().write_at(bytes, offset)?;
        event(&self.trace, "write", &self.name, offset, bytes.len(), count);
        Ok(count)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.inner.as_mut().unwrap().sync_all()?;
        event(&self.trace, "sync", &self.name, 0, 0, 0);
        Ok(())
    }
    fn set_len(&mut self, n: u64) -> io::Result<()> {
        assert!(!self.trace.lock().unwrap().enabled, "flush cannot resize");
        self.inner.as_mut().unwrap().set_len(n)
    }
}
impl Writer {
    fn wrap(&self, inner: LinuxNamespaceFile, name: &FileName) -> File {
        let mut t = self.trace.lock().unwrap();
        t.handles += 1;
        t.peak = t.peak.max(t.handles);
        if t.enabled {
            sample_fd(&mut t, "open");
        }
        File {
            inner: Some(inner),
            name: name.to_string(),
            trace: Arc::clone(&self.trace),
        }
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        assert!(!self.trace.lock().unwrap().enabled);
        Ok(std::array::from_fn(|n| (n + 1) as u8))
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let file = self.inner.create_new(name)?;
        event(&self.trace, "create", name.as_str(), 0, 0, 0);
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
        self.inner.sync_directory()?;
        event(&self.trace, "dirsync", "", 0, 0, 0);
        Ok(())
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        self.inner.publish_selector(temporary, selected)?;
        event(&self.trace, "publish", temporary.as_str(), 0, 0, 0);
        Ok(())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        panic!("flush1 never removes an owned file")
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn literal(name: &str) -> Vec<u8> {
    fs::read(format!("{INPUT}/{name}")).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn inventory(path: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut total = 0;
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        assert!(meta.file_type().is_file());
        assert_eq!(meta.nlink(), 1);
        total += meta.len();
        result.insert(
            entry.file_name().into_string().unwrap(),
            format!(
                "{}\t{}\t{}\t{}\t{}",
                meta.len(),
                meta.dev(),
                meta.ino(),
                meta.nlink(),
                hex(&content_digest(&fs::read(entry.path()).unwrap()))
            ),
        );
    }
    assert!(total <= 24 << 20);
    result
}
fn save_inventory(path: &Path, files: &BTreeMap<String, String>) {
    fs::write(
        path,
        files
            .iter()
            .map(|(n, v)| format!("{n}\t{v}\n"))
            .collect::<String>(),
    )
    .unwrap();
}
fn validate_disk(db: &Path, stem: &str, batch: u64) {
    let images = fs::read(db.join("d3-dwb-images")).unwrap();
    assert_eq!(&images[..16544], literal(&format!("{stem}-slot.bin")));
    assert!(images[16544..].iter().all(|b| *b == 0));
    let home = fs::read(db.join(HOME)).unwrap();
    assert_eq!(&home[..PAGE_BYTES], literal(&format!("{stem}-home.after")));
    assert!(home[PAGE_BYTES..].iter().all(|b| *b == 0));
    let bank = 3 * batch % 2;
    for (name, suffix) in [
        ("d3-dwb-stage".to_owned(), "retirement.bank"),
        (format!("d3-dwb-{bank}"), "retirement.bank"),
        (format!("d3-dwb-{}", 1 - bank), "pending.bank"),
        ("DWB".to_owned(), "retirement.selector"),
    ] {
        assert_eq!(
            fs::read(db.join(name)).unwrap(),
            literal(&format!("{stem}-{suffix}"))
        );
    }
    assert!(!db.join("d3-dwb-select-stage").exists());
}

#[test]
#[ignore = "three new frozen Linux DB cases selected by the flush native runner"]
fn flush_native_worker() {
    let output =
        PathBuf::from(std::env::var_os("XADB_FLUSH_NATIVE_DIR").expect("native output directory"));
    assert!(output.is_absolute());
    let profile: u32 = std::env::var("XADB_FLUSH_NATIVE_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&profile));
    let db = output.join("database");
    fs::create_dir(&db).unwrap();
    let directory = LinuxDirectory::open_existing(&db).unwrap();
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        ..Trace::default()
    }));
    let port = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    // The lease owns its independent directory descriptor and lock.
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut page = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<98304>();
    let fresh =
        wal::FreshStorageSession::create(port, profile, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    let before = inventory(&db);
    save_inventory(&output.join("before.tsv"), &before);
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(&mut page)];
    let mut bound = storage
        .bind_allocator(&core, &mut frames, &mut operation)
        .unwrap();
    assert_eq!(inventory(&db), before);
    {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut writer_owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flush_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut expected_wal = fs::read(db.join(wal::WAL_NAME)).unwrap();
        assert_eq!(expected_wal.len(), 4096);
        fs::write(output.join("header.bin"), &expected_wal).unwrap();
        {
            let mut t = trace.lock().unwrap();
            t.peak = t.handles;
            t.fd_baseline = db_fds(&db);
            t.fd_peak = t.fd_baseline;
            sample_fd(&mut t, "baseline");
        }
        for (batch, part) in [(1, "reserve"), (2, "cancel")] {
            let observation = if batch == 1 {
                writer.reserve_pages(&mut writer_owner, 1)
            } else {
                writer.cancel_reserved(&mut writer_owner)
            }
            .unwrap();
            assert_eq!(observation.rec_lsn, Some(PageLsn(batch)));
            expected_wal.extend(fs::read(format!("{DIRTY}/p{profile}-n1-{part}.frame")).unwrap());
            assert_eq!(fs::read(db.join(wal::WAL_NAME)).unwrap(), expected_wal);
            {
                let mut t = trace.lock().unwrap();
                t.events.clear();
                t.enabled = true;
            }
            let FlushProgress::Complete(observed) =
                flusher.try_flush_one(&mut flush_owner).unwrap()
            else {
                panic!("expected actual Complete");
            };
            {
                let mut t = trace.lock().unwrap();
                t.enabled = false;
                assert_eq!(t.events.len(), 31);
                assert!(t.peak <= 2);
                assert!(t.fd_peak <= 5);
                fs::write(
                    output.join(format!("{part}.events.tsv")),
                    t.events
                        .iter()
                        .map(|e| format!("{e}\n"))
                        .collect::<String>(),
                )
                .unwrap();
            }
            assert!(observed.finalized.cleaned);
            assert_eq!(
                (
                    observed.io.last_issued,
                    observed.io.known.generation,
                    observed.io.known.reference.generation
                ),
                (batch, 3 * batch + 1, 3 * batch + 1)
            );
            assert!(observed.io.retirement_selected && observed.io.homes_synced);
            assert_eq!((observed.io.attempted, observed.io.completed), (31, 31));
            let stem = format!("p{profile}-n1-{part}");
            validate_disk(&db, &stem, batch);
            save_inventory(&output.join(format!("{part}.after.tsv")), &inventory(&db));
            let mut actual = boxed::<PAGE_BYTES>();
            let snap = frame_test::diagnose(flusher.pool, 0, &mut actual);
            let registry = frame_test::flush_diagnose(flusher.pool);
            assert_eq!(&actual[..], literal(&format!("{stem}-home.after")));
            assert!(!snap.dirty);
            assert!(
                snap.entry.is_none()
                    && snap.frame.rec_lsn.is_none()
                    && registry.claim.is_none()
                    && registry.requirements[0].is_none()
            );
            assert_eq!(
                (
                    snap.frame.page_lsn.0,
                    snap.frame.modification_counter,
                    snap.total
                ),
                (batch, batch, 0)
            );
            fs::write(
                output.join(format!("{part}.clean.tsv")),
                format!(
                    "{batch}\t{batch}\ttrue\ttrue\ttrue\t{batch}\t{}\n",
                    3 * batch + 1
                ),
            )
            .unwrap();
            assert!(!storage.session_fence().is_fenced());
        }
        drop(flush_owner);
        drop(writer_owner);
    }
    drop(bound);
    drop(core);
    drop(storage);
    {
        let mut t = trace.lock().unwrap();
        sample_fd(&mut t, "final");
        assert_eq!(db_fds(&db), 0);
        fs::write(
            output.join("fd.tsv"),
            format!("{}\t{}\t0\n", t.fd_baseline, t.fd_peak),
        )
        .unwrap();
        fs::write(
            output.join("fd-events.tsv"),
            t.fd_events
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
    }
    let after = inventory(&db);
    assert_eq!(before.len(), after.len());
    for (name, value) in &before {
        if ![
            wal::WAL_NAME,
            HOME,
            "DWB",
            "d3-dwb-stage",
            "d3-dwb-0",
            "d3-dwb-1",
            "d3-dwb-images",
        ]
        .contains(&name.as_str())
        {
            assert_eq!(after.get(name), Some(value));
        }
    }
    save_inventory(&output.join("after.tsv"), &after);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.handles, 0);
    let bytes: u64 = fs::read_dir(&db)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    println!(
        "FLUSH_NATIVE_COMPLETE profile={profile} batches=2 effects=62 bytes={bytes} peak_files={}",
        trace.peak
    );
}
