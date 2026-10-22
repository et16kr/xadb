//! Linux DB workers for actual reserve/INIT, RAM publication and WAL durability.
use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, init_test_support as frame_init},
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
        panic!("Status INIT never removes an owned file")
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
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

#[test]
#[ignore = "three isolated Linux Status INIT databases selected by the INIT runner"]
fn status_init_native_worker() {
    let output = PathBuf::from(
        std::env::var_os("XADB_STATUS_INIT_NATIVE_DIR").expect("isolated output directory"),
    );
    assert!(output.is_absolute());
    let profile: u32 = std::env::var("XADB_STATUS_INIT_NATIVE_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&profile));
    let cancellations = usize::from(profile == 3);
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
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut allocator = boxed::<PAGE_BYTES>();
    let mut payload = boxed::<PAGE_BYTES>();
    payload.fill(0xa5);
    let mut work = boxed::<OPERATION_BYTES>();
    let fresh = wal::FreshStorageSession::create_status(
        port,
        profile,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    let before = inventory(&db);
    save_inventory(&output.join("before.tsv"), &before);
    let header = fs::read(db.join(wal::WAL_NAME)).unwrap();
    assert_eq!(header.len(), 4096);
    fs::write(output.join("wal-header.bin"), &header).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [
        DirtyFrame::new(&mut allocator),
        DirtyFrame::new(&mut payload),
    ];
    let mut bound = storage
        .bind_status_bootstrap(&core, &mut frames, &mut work)
        .unwrap();
    assert_eq!(inventory(&db), before);
    {
        let mut trace = trace.lock().unwrap();
        trace.enabled = true;
        trace.peak = trace.handles;
        trace.fd_baseline = db_fds(&db);
        trace.fd_peak = trace.fd_baseline;
        sample_fd(&mut trace, "baseline");
    }
    {
        let (mut writer, reader) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        if cancellations == 1 {
            cancel(writer.reserve_one(&mut owner).unwrap());
        }
        let observed = init(writer.reserve_one(&mut owner).unwrap());
        assert_eq!(observed.lsn, PageLsn(2 * (cancellations + 1) as u64));
        let guard = reader.read(&mut owner).unwrap();
        let expected = literal(
            profile,
            cancellations,
            "init",
            &format!("work{}-status.after", cancellations + 1),
        );
        assert_eq!(&guard.bytes().unwrap()[..], expected);
        fs::write(output.join("status.after"), guard.bytes().unwrap()).unwrap();
        guard.release_checked().unwrap();
        let mut actual = boxed::<PAGE_BYTES>();
        let snap = frame_init::snapshot(writer.pool, 0, &mut actual);
        assert_eq!(
            &actual[..],
            literal(
                profile,
                cancellations,
                "init",
                &format!("work{}-allocator.after", cancellations + 1)
            )
        );
        assert_eq!(snap.total, 0);
        assert!(!snap.claim_present);
        assert!(snap.dirty);
        fs::write(output.join("allocator.after"), &actual[..]).unwrap();
        fs::write(output.join("owner.descriptor"), writer.base.descriptor).unwrap();
        let expected = literal(profile, cancellations, "init", "wal.frames-only");
        let actual = fs::read(db.join(wal::WAL_NAME)).unwrap();
        assert_eq!(&actual[..4096], header);
        assert_eq!(&actual[4096..], expected);
        fs::write(output.join("wal.complete"), actual).unwrap();
        let after = inventory(&db);
        for (name, entry) in &before {
            if name != wal::WAL_NAME {
                assert_eq!(after.get(name), Some(entry));
            }
        }
        assert_eq!(after.len(), before.len());
        save_inventory(&output.join("after.tsv"), &after);
        let trace = trace.lock().unwrap();
        assert_eq!(trace.events.len(), 4 * (cancellations + 1));
        assert!(trace.peak <= 2);
        assert!(trace.fd_peak <= 5);
        fs::write(
            output.join("events.tsv"),
            trace
                .events
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
    }
    drop(bound);
    drop(storage);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.handles, 0);
    assert_eq!(db_fds(&db), 0);
    fs::write(
        output.join("fd-events.tsv"),
        trace
            .fd_events
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        output.join("resources.tsv"),
        format!(
            "wrapped_peak\t{}\nwrapped_final\t{}\nobserved_fd_peak\t{}\nobserved_fd_final\t0\n",
            trace.peak, trace.handles, trace.fd_peak
        ),
    )
    .unwrap();
    println!(
        "XADB_STATUS_INIT_NATIVE_COMPLETE profile={profile} cancellations={cancellations} homes=unchanged"
    );
}
