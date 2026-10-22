//! Isolated real producer, reopened selector and lease contender subprocess modes.
use super::RecoverySelection;
use crate::{
    storage::{dirty_frame::DirtyFrame, page::PAGE_BYTES, wal_io::FreshStorageSession},
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use xadb_foundation::{
    codec::content_digest,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
        WriterNamespace,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

#[derive(Default)]
struct Trace {
    closed: bool,
    root: PathBuf,
    handles: usize,
    peak: usize,
    fd_peak: usize,
    mutations: usize,
    reads: Vec<String>,
    fd_rows: Vec<String>,
}
impl Trace {
    fn sample(&mut self, tag: &str) {
        let n = db_fds(&self.root);
        self.fd_peak = self.fd_peak.max(n);
        self.fd_rows.push(format!("{tag}\t{n}"));
    }
    fn mutation(&mut self) {
        self.mutations += 1;
        assert!(!self.closed, "closed selection attempted mutation");
    }
}
struct Directory {
    inner: LinuxDirectory,
    trace: Arc<Mutex<Trace>>,
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
fn db_fds(root: &Path) -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| fs::read_link(e.ok()?.path()).ok())
        .filter(|p| p.starts_with(root))
        .count()
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        let inner = self.inner.acquire_writer()?;
        self.trace.lock().unwrap().sample("lease");
        Ok(Writer {
            inner,
            trace: Arc::clone(&self.trace),
        })
    }
}
impl Writer {
    fn wrap(&self, inner: LinuxNamespaceFile, name: &FileName) -> File {
        let mut t = self.trace.lock().unwrap();
        t.handles += 1;
        t.peak = t.peak.max(t.handles);
        t.sample("open");
        File {
            inner: Some(inner),
            name: name.to_string(),
            trace: Arc::clone(&self.trace),
        }
    }
    fn mutation(&self) {
        self.trace.lock().unwrap().mutation();
    }
}
impl Drop for File {
    fn drop(&mut self) {
        drop(self.inner.take());
        let mut t = self.trace.lock().unwrap();
        t.handles -= 1;
        t.sample("close");
    }
}
impl FileIo for File {
    fn read_at(&mut self, output: &mut [u8], offset: u64) -> io::Result<usize> {
        let n = self.inner.as_mut().unwrap().read_at(output, offset)?;
        let mut t = self.trace.lock().unwrap();
        if t.closed {
            t.reads
                .push(format!("{}\t{offset}\t{}\t{n}", self.name, output.len()));
        }
        t.sample("read");
        Ok(n)
    }
    fn write_at(&mut self, input: &[u8], offset: u64) -> io::Result<usize> {
        self.trace.lock().unwrap().mutation();
        self.inner.as_mut().unwrap().write_at(input, offset)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.trace.lock().unwrap().mutation();
        self.inner.as_mut().unwrap().sync_all()
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        self.trace.lock().unwrap().mutation();
        self.inner.as_mut().unwrap().set_len(len)
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.mutation();
        Ok(std::array::from_fn(|i| (i + 1) as u8))
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        self.mutation();
        let f = self.inner.create_new(name)?;
        Ok(self.wrap(f, name))
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        if self.trace.lock().unwrap().closed {
            assert_eq!(mode, OpenMode::ReadOnly);
        }
        let f = self.inner.open_existing(name, mode)?;
        Ok(self.wrap(f, name))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        if self.trace.lock().unwrap().closed {
            assert_eq!(mode, OpenMode::ReadOnly);
        }
        let (f, m) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.wrap(f, name), m))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        assert!(
            !self.trace.lock().unwrap().closed,
            "selection must not list directory"
        );
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.mutation();
        self.inner.sync_directory()
    }
    fn publish_selector(&mut self, name: &FileName, selected: Selector) -> NamespaceResult<()> {
        self.mutation();
        self.inner.publish_selector(name, selected)
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        self.mutation();
        self.inner.remove_retired(name)
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
fn produce(directory: Directory, profile: u32, plan: u64, nonempty: bool) {
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut scratch = [[0; PAGE_BYTES]; 3];
    let writer = directory.acquire_writer().unwrap();
    // The acquired lease already retains its own directory and LOCK. Drop the
    // fixture caller descriptor before retained WAL + reader/data files coexist.
    drop(directory);
    let fresh = if plan == 1 {
        FreshStorageSession::create(writer, profile, &mut slot, &mut scratch)
    } else {
        FreshStorageSession::create_status(writer, profile, &mut slot, &mut scratch)
    }
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    if nonempty {
        let storage = installed.into_mutable_storage();
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut a = boxed::<PAGE_BYTES>();
        let mut s = boxed::<PAGE_BYTES>();
        let mut work = boxed::<114688>();
        let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
        let mut bound = storage
            .bind_status_bootstrap(&core, &mut frames, &mut work)
            .unwrap();
        let (mut writer, _reader) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        assert!(reserved.cancel().is_ok());
    } else {
        drop(installed);
    }
}

#[test]
#[ignore = "isolated producer/reopen/contender processes driven by recovery native runner"]
fn worker_recovery_select_native() {
    let output = PathBuf::from(std::env::var_os("XADB_RECOVERY_SELECT_NATIVE_DIR").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_RECOVERY_SELECT_NATIVE_DB").unwrap());
    assert!(output.is_absolute() && db.is_absolute());
    let mode = std::env::var("XADB_RECOVERY_SELECT_NATIVE_MODE").unwrap();
    let profile: u32 = std::env::var("XADB_RECOVERY_SELECT_NATIVE_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    let plan: u64 = std::env::var("XADB_RECOVERY_SELECT_NATIVE_PLAN")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&profile) && (1..=2).contains(&plan));
    let trace = Arc::new(Mutex::new(Trace {
        closed: !mode.starts_with("produce"),
        root: db.clone(),
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    trace.lock().unwrap().sample("directory");
    let mut outcome = String::from("produced");
    if mode.starts_with("produce") {
        assert!(mode == "produce" || mode == "produce_nonempty");
        produce(directory, profile, plan, mode == "produce_nonempty");
    } else {
        let mut scratch = [[0; PAGE_BYTES]; 3];
        let selected = RecoverySelection::acquire(&directory, None, &mut scratch);
        // A returned owner never retains the caller scratch loan.
        scratch[0].fill(0xa5);
        match selected {
            Ok(owner) => {
                assert!(mode == "select" || mode == "hold");
                let o = owner.observation();
                assert_eq!(o.profile_id(), profile);
                assert_eq!(o.bootstrap_plan(), plan);
                fs::write(
                    output.join("observation.tsv"),
                    format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                        hex(&o.namespace().database.bytes()),
                        o.namespace().timeline,
                        o.profile_id(),
                        o.bootstrap_plan(),
                        o.filemap_generation(),
                        o.doublewrite_generation(),
                        hex(o.epoch_digest())
                    ),
                )
                .unwrap();
                {
                    let mut t = trace.lock().unwrap();
                    assert_eq!(t.handles, 0);
                    t.sample("selected");
                }
                if mode == "hold" {
                    fs::write(output.join("holding"), std::process::id().to_string()).unwrap();
                    let start = Instant::now();
                    while !output.join("release").exists() {
                        assert!(
                            start.elapsed() < Duration::from_secs(20),
                            "controller release not reached"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                drop(owner);
                outcome = "selected".into();
            }
            Err(error) => {
                assert!(mode == "reject" || mode == "contend", "{error:?}");
                fs::write(output.join("error.txt"), format!("{error:?}\n")).unwrap();
                outcome = "rejected".into();
            }
        }
        drop(directory);
    }
    let mut t = trace.lock().unwrap();
    t.sample("final");
    let final_fd = db_fds(&db);
    fs::write(
        output.join("reads.tsv"),
        t.reads.iter().map(|r| format!("{r}\n")).collect::<String>(),
    )
    .unwrap();
    fs::write(
        output.join("fd.tsv"),
        t.fd_rows
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        output.join("resources.tsv"),
        format!(
            "handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\nmutations\t{}\n",
            t.peak, t.handles, t.fd_peak, final_fd, t.mutations
        ),
    )
    .unwrap();
    // Retain raw observations before assertions so a failed bound stays auditable.
    assert_eq!(t.handles, 0);
    assert_eq!(final_fd, 0);
    assert!(t.fd_peak <= 5);
    if t.closed {
        assert!(t.peak <= 2 && t.mutations == 0);
    }
    // The header digest here is evidence, never used to calculate expected values.
    let epoch = fs::read(db.join("d3-wal-00000001")).unwrap();
    fs::write(
        output.join("epoch.sha256"),
        format!("{}\n", hex(&content_digest(&epoch[..4096]))),
    )
    .unwrap();
    println!(
        "RECOVERY_SELECT_NATIVE_COMPLETE mode={mode} pid={} outcome={outcome}",
        std::process::id()
    );
}
