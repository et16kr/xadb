//! Isolated real producer, reopened selector and lease contender subprocess modes.
use crate::storage::wal_io::RecoveryPrefix;
use crate::{
    storage::{dirty_frame::DirtyFrame, page::PAGE_BYTES, wal_io::FreshStorageSession},
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::{
    codec::content_digest,
    io::{FileIo, write_all_at},
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
        WriterNamespace,
    },
};
use platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Trace {
    closed: bool,
    root: PathBuf,
    handles: usize,
    peak: usize,
    fd_peak: usize,
    mutations: usize,
    cut: usize,
    cut_count: usize,
    cut_hits: usize,
    tail_cut: bool,
    events: Vec<String>,
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
        if self.cut != 0 {
            self.cut_count += 1;
        }
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
    fn after_namespace(&self, operation: &str) -> NamespaceResult<()> {
        let mut t = self.trace.lock().unwrap();
        let ordinal = t.cut_count;
        t.events.push(format!("{operation}\t{ordinal}"));
        if t.cut != 0 && t.cut_count == t.cut {
            t.cut_hits += 1;
            return Err(foundation::namespace::NamespaceError::new(
                foundation::namespace::NamespaceErrorKind::Io,
                foundation::error::Effect::CallerMustResolve,
                "fixed recovery-prefix producer outcome unknown",
            ));
        }
        Ok(())
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
            t.reads.push(format!(
                "{}\t{offset}\t{}\t{n}\t{}",
                self.name,
                output.len(),
                hex(&content_digest(&output[..n]))
            ));
        }
        t.sample("read");
        Ok(n)
    }
    fn write_at(&mut self, input: &[u8], offset: u64) -> io::Result<usize> {
        let tail_cut = {
            let mut t = self.trace.lock().unwrap();
            t.mutation();
            t.tail_cut && self.name == "d3-wal-00000001" && input.len() == 38000
        };
        if tail_cut {
            write_all_at(self.inner.as_mut().unwrap(), &input[..19000], offset)
                .map_err(io::Error::other)?;
            let mut t = self.trace.lock().unwrap();
            t.cut_hits += 1;
            t.tail_cut = false;
            t.events.push(format!(
                "wal-prefix-error\t{offset}\t{}\t19000",
                input.len()
            ));
            return Err(io::Error::other(
                "fixed incomplete INIT write outcome unknown",
            ));
        }
        let n = self.inner.as_mut().unwrap().write_at(input, offset)?;
        self.trace.lock().unwrap().events.push(format!(
            "write\t{}\t{offset}\t{}\t{n}",
            self.name,
            input.len()
        ));
        Ok(n)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.trace.lock().unwrap().mutation();
        self.inner.as_mut().unwrap().sync_all()?;
        let mut t = self.trace.lock().unwrap();
        t.events.push(format!("sync\t{}", self.name));
        if t.cut != 0 && t.cut_count == t.cut {
            t.cut_hits += 1;
            return Err(io::Error::other("fixed producer sync outcome unknown"));
        }
        Ok(())
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
        self.inner.sync_directory()?;
        self.after_namespace("dirsync")
    }
    fn publish_selector(&mut self, name: &FileName, selected: Selector) -> NamespaceResult<()> {
        self.mutation();
        self.inner.publish_selector(name, selected)?;
        self.after_namespace("publish")
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
// Fixture preparation owns a different session and never receives prefix authority.
fn produce(directory: Directory, profile: u32, kind: &str, context: &str, output: &Path) {
    let writer = directory.acquire_writer().unwrap();
    let trace = Arc::clone(&directory.trace);
    drop(directory); // acquired writer retains its own dir/LOCK; keep producer FD <=5.
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut scratch = [[0; PAGE_BYTES]; 3];
    let plan2 = kind.starts_with('S');
    let fresh = if plan2 {
        FreshStorageSession::create_status(writer, profile, &mut slot, &mut scratch)
    } else {
        FreshStorageSession::create(writer, profile, &mut slot, &mut scratch)
    }
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    if kind.starts_with('E') {
        let mut frame = boxed::<PAGE_BYTES>();
        let mut operation = boxed::<81920>();
        let mut session = installed
            .into_allocation_session(&mut frame, &mut operation)
            .unwrap();
        if context == "long63" {
            for _ in 0..1000 {
                session.reserve_pages(63).unwrap();
                session.cancel_reserved().unwrap();
            }
        } else {
            session.reserve_pages(1).unwrap();
            if kind == "EC" {
                session.cancel_reserved().unwrap();
            }
        }
        return;
    }
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut a = boxed::<PAGE_BYTES>();
    let mut s = boxed::<PAGE_BYTES>();
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    if kind.starts_with('D') {
        let mut operation = boxed::<98304>();
        let mut frames = [DirtyFrame::new(&mut a)];
        let mut bound = storage
            .bind_allocator(&core, &mut frames, &mut operation)
            .unwrap();
        let (mut writer, _reader, mut flusher) = bound.split_with_flush();
        writer.reserve_pages(&mut owner, 1).unwrap();
        if context == "dirty_flushed" {
            flusher.try_flush_one(&mut dedicated).unwrap();
        }
        if kind == "DC" {
            writer.cancel_reserved(&mut owner).unwrap();
        }
        if context == "dirty_flushed" {
            flusher.try_flush_one(&mut dedicated).unwrap();
        }
    } else {
        let mut operation = boxed::<114688>();
        let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
        let bound = storage
            .bind_status_bootstrap(&core, &mut frames, &mut operation)
            .unwrap();
        let mut bound = match bound.into_with_flush() {
            Ok(value) => value,
            Err(_) => panic!("fresh Status flush assembly"),
        };
        let (mut writer, _reader, mut flusher) = bound.split_with_flush();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        if kind == "SC" {
            assert!(reserved.cancel().is_ok());
        } else if kind == "SA" {
            drop(reserved);
        } else {
            if context == "tail" {
                trace.lock().unwrap().tail_cut = true;
            }
            let result = reserved.initialize();
            if context == "tail" {
                let error = match result {
                    Err(crate::storage::wal_io::InitAttemptError::Fatal(error)) => error,
                    Err(crate::storage::wal_io::InitAttemptError::Before(error)) => {
                        panic!("expected poststage failure: {:?}", error.error)
                    }
                    Ok(_) => panic!("incomplete INIT fault not reached"),
                };
                assert!(storage.session_fence().is_fenced());
                fs::write(output.join("producer-error.txt"), format!("{error:?}\n")).unwrap();
            } else {
                assert!(result.is_ok());
            }
            if matches!(context, "a_flushed" | "both_flushed" | "raw_corrupt") {
                flusher.try_flush_allocator(&mut dedicated).unwrap();
            }
            if matches!(context, "s_flushed" | "both_flushed" | "raw_corrupt") {
                flusher.try_flush_status(&mut dedicated).unwrap();
            }
            if context == "pending" || context == "stage" {
                {
                    let mut t = trace.lock().unwrap();
                    t.cut = if context == "pending" { 19 } else { 18 };
                    t.cut_count = 0;
                }
                let error = flusher.try_flush_status(&mut dedicated).unwrap_err();
                assert!(storage.session_fence().is_fenced());
                fs::write(output.join("producer-error.txt"), format!("{error:?}\n")).unwrap();
            }
        }
    }
}

#[test]
#[ignore = "new actual producers and separate closed-prefix subprocesses only"]
fn worker_recovery_prefix_native() {
    run_worker(false);
}

#[test]
#[ignore = "S5 only: 1000 actual 63-page reserve/cancel cycles, separate prefix process"]
fn worker_recovery_prefix_long_native() {
    run_worker(true);
}

fn run_worker(long: bool) {
    let output = PathBuf::from(std::env::var_os("XADB_RECOVERY_PREFIX_NATIVE_DIR").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_RECOVERY_PREFIX_NATIVE_DB").unwrap());
    let mode = std::env::var("XADB_RECOVERY_PREFIX_NATIVE_MODE").unwrap();
    let kind = std::env::var("XADB_RECOVERY_PREFIX_NATIVE_KIND").unwrap();
    let context = std::env::var("XADB_RECOVERY_PREFIX_NATIVE_CONTEXT").unwrap();
    let profile: u32 = std::env::var("XADB_RECOVERY_PREFIX_NATIVE_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    assert!(output.is_absolute() && db.is_absolute() && (1..=3).contains(&profile));
    assert!(matches!(
        kind.as_str(),
        "EA" | "EC" | "DA" | "DC" | "SA" | "SC" | "SI"
    ));
    assert_eq!(context == "long63", long);
    if long {
        assert_eq!((profile, kind.as_str()), (1, "EC"));
    }
    // Record the concrete Linux wrapper type census for every producer/analyzer/
    // contender worker before acquiring any database resources. This is source
    // ownership accounting, not a stack/RSS measurement or BOOT_RESERVE permit.
    let census = super::source_census::<Writer>();
    fs::write(
        output.join("source-census.tsv"),
        format!(
            "outer\t{}\nbootstrap\t{}\nreplay\t{}\nstream\t{}\npeak\t{}\nwriter\t{}\nfile\t{}\ncaller\t262144\n",
            census.outer,
            census.bootstrap,
            census.replay,
            census.stream,
            census.peak,
            std::mem::size_of::<Writer>(),
            std::mem::size_of::<File>(),
        ),
    )
    .unwrap();
    let trace = Arc::new(Mutex::new(Trace {
        closed: mode != "produce",
        root: db.clone(),
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    trace.lock().unwrap().sample("directory");
    println!(
        "RECOVERY_PREFIX_NATIVE_BEGIN mode={mode} pid={}",
        std::process::id()
    );
    let outcome;
    if mode == "produce" {
        produce(directory, profile, &kind, &context, &output);
        outcome = "produced";
    } else {
        let mut work = boxed::<262144>();
        match RecoveryPrefix::acquire(&directory, None, &mut work) {
            Ok(prefix) => {
                assert!(mode == "analyze" || mode == "hold");
                let o = prefix.observation();
                let facts = [
                    ("database", hex(&o.namespace().database.bytes())),
                    ("timeline", o.namespace().timeline.to_string()),
                    ("profile", o.profile_id().to_string()),
                    ("plan", o.bootstrap_plan().to_string()),
                    ("filemap_generation", o.filemap_generation().to_string()),
                    ("dwb_generation", o.doublewrite_generation().to_string()),
                    ("complete_lsn", o.complete_lsn().to_string()),
                    ("complete_offset", o.complete_offset().to_string()),
                    ("source_length", o.source_length().to_string()),
                    ("work_high_water", o.work_high_water().to_string()),
                    (
                        "active_id",
                        o.active_owner().map_or(0, |owner| owner.id()).to_string(),
                    ),
                    ("active_mask", o.active_mask().to_string()),
                    ("mode", format!("{:?}", o.mode())),
                    ("inventory_items", o.inventory_items().to_string()),
                    ("inventory_digest", hex(o.inventory_digest())),
                    ("retained_wal", o.retained_wal_bytes().to_string()),
                    ("remaining_wal", o.remaining_wal().to_string()),
                    ("remaining_pages", o.remaining_pages().to_string()),
                    ("remaining_memory", o.remaining_memory().to_string()),
                    ("remaining_frames", o.remaining_frames().to_string()),
                    ("remaining_files", o.remaining_files().to_string()),
                    ("remaining_protected", o.remaining_protected().to_string()),
                    ("protected_bytes", o.protected_bytes().to_string()),
                    (
                        "replay_page_topup_bytes",
                        o.replay_page_topup_bytes().to_string(),
                    ),
                    ("owned_bytes", o.owned_bytes().to_string()),
                    ("terminal_hold_mask", o.terminal_hold_mask().to_string()),
                    ("source_stabilized", o.source_stabilized().to_string()),
                    ("tail_present", o.tail().is_some().to_string()),
                    (
                        "tail_offset",
                        o.tail().map_or(0, |t| t.offset()).to_string(),
                    ),
                    (
                        "tail_bytes",
                        o.tail().map_or(0, |t| t.physical_bytes()).to_string(),
                    ),
                    (
                        "tail_declared",
                        o.tail()
                            .and_then(|t| t.declared_bytes())
                            .unwrap_or(0)
                            .to_string(),
                    ),
                    (
                        "tail_inspected",
                        o.tail().map_or(0, |t| t.inspected_bytes()).to_string(),
                    ),
                    ("pending_present", o.pending().is_some().to_string()),
                    (
                        "pending_slot",
                        o.pending().map_or(0, |p| p.slot).to_string(),
                    ),
                    (
                        "pending_page",
                        o.pending().map_or(0, |p| p.page.page_no).to_string(),
                    ),
                    (
                        "pending_incarnation",
                        o.pending()
                            .map_or(0, |p| p.page.incarnation.get())
                            .to_string(),
                    ),
                    (
                        "pending_owner",
                        o.pending().map_or(0, |p| p.owner.get()).to_string(),
                    ),
                    (
                        "pending_owner_generation",
                        o.pending()
                            .map_or(0, |p| p.owner_generation.get())
                            .to_string(),
                    ),
                    (
                        "pending_kind",
                        o.pending().map_or(0, |p| p.kind as u8).to_string(),
                    ),
                    (
                        "pending_level",
                        o.pending().map_or(0, |p| p.level).to_string(),
                    ),
                    ("pending_lsn", o.pending_lsn().unwrap_or(0).to_string()),
                    (
                        "pending_digest",
                        o.pending()
                            .map_or_else(String::new, |p| hex(&p.image_digest)),
                    ),
                    ("dwb_batch", o.doublewrite_batch().to_string()),
                    ("dwb_state", format!("{:?}", o.doublewrite_state())),
                    ("source_device", o.source_identity().device.to_string()),
                    ("source_inode", o.source_identity().inode.to_string()),
                ];
                fs::write(
                    output.join("observation.tsv"),
                    facts
                        .iter()
                        .map(|(k, v)| format!("{k}\t{v}\n"))
                        .collect::<String>(),
                )
                .unwrap();
                fs::write(output.join("allocator.bin"), prefix.allocator_image()).unwrap();
                if let Some(bytes) = prefix.status_image() {
                    fs::write(output.join("status.bin"), bytes).unwrap();
                }
                if let Some(bytes) = prefix.descriptor_bytes() {
                    fs::write(output.join("descriptor.bin"), bytes).unwrap();
                }
                {
                    let mut t = trace.lock().unwrap();
                    assert_eq!(t.handles, 1);
                    t.sample("inspection");
                }
                if mode == "hold" {
                    fs::write(output.join("holding"), std::process::id().to_string()).unwrap();
                    let start = Instant::now();
                    while !output.join("release").exists() {
                        assert!(start.elapsed() < Duration::from_secs(20));
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                drop(prefix);
                work[0] = 0xa5; // actual caller loan returned only after owner Drop.
                outcome = "analyzed";
            }
            Err(error) => {
                assert!(mode == "reject" || mode == "contend", "{error:?}");
                fs::write(output.join("error.txt"), format!("{error:?}\n")).unwrap();
                outcome = "rejected";
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
        output.join("events.tsv"),
        t.events
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(output.join("resources.tsv"),format!("handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\nmutations\t{}\ncut_hits\t{}\ncut_ordinal\t{}\n",t.peak,t.handles,t.fd_peak,final_fd,t.mutations,t.cut_hits,t.cut_count)).unwrap();
    assert_eq!(t.handles, 0);
    assert_eq!(final_fd, 0);
    assert!(t.fd_peak <= 5);
    if t.closed {
        assert!(t.peak <= 2 && t.mutations == 0);
    }
    if matches!(context.as_str(), "pending" | "stage" | "tail") && mode == "produce" {
        assert_eq!(t.cut_hits, 1);
    }
    println!(
        "RECOVERY_PREFIX_NATIVE_CASE mode={mode} pid={} outcome={outcome} PASS",
        std::process::id()
    );
    println!("RECOVERY_PREFIX_NATIVE_COMPLETE count=1");
}
