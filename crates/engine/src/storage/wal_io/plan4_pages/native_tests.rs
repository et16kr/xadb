//! Actual native adapter for G5's single-process clean live consumer.
use super::*;
use crate::storage::dirty_frame::DirtyFrame;
use crate::storage::{
    retention_genesis::tests::{Harness, hash_hex, namespace},
    wal_frame::MAX_FRAME_BYTES,
    wal_io::FreshStorageSession,
};
use crate::transaction::concurrency::PageConcurrency;
use foundation::{
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
        WriterNamespace,
    },
};
use platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(in crate::storage) struct Trace {
    root: PathBuf,
    output: fs::File,
    entropy_output: fs::File,
    readonly: bool,
    handles: usize,
    peak: usize,
    fd_peak: usize,
    events: usize,
    mutations: usize,
    entropy: usize,
    log_bytes: usize,
    undo_home_cut: Option<UndoHomeCut>,
    catalog_home_cut: Option<CatalogHomeCut>,
    catalog_sync_after_error: Option<u64>,
    catalog_wal_write_end: u64,
}
impl Trace {
    fn event(&mut self, operation: &str, name: &str, offset: u64, length: usize, outcome: &str) {
        let row = format!(
            "{}\t{operation}\t{name}\t{offset}\t{length}\t{outcome}\n",
            self.events
        );
        self.log_bytes += row.len();
        assert!(self.log_bytes <= 4 * 1024 * 1024, "native trace bound");
        self.output.write_all(row.as_bytes()).unwrap();
        self.output.flush().unwrap();
        self.events += 1;
        self.fd_peak = self.fd_peak.max(db_fds(&self.root));
    }
    fn mutation(&mut self, operation: &str, name: &str, offset: u64, length: usize) {
        self.event(operation, name, offset, length, "ATTEMPT");
        self.mutations += 1;
        assert!(!self.readonly, "read-only phase attempted {operation}");
    }
}
fn db_fds(root: &Path) -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .filter(|path| path.starts_with(root))
        .count()
}
pub(in crate::storage) struct Directory {
    inner: Option<LinuxDirectory>,
    trace: Arc<Mutex<Trace>>,
}
pub(in crate::storage) struct Writer {
    inner: Option<LinuxWriterLease>,
    trace: Arc<Mutex<Trace>>,
}
pub(in crate::storage) struct File {
    inner: Option<LinuxNamespaceFile>,
    name: FileName,
    trace: Arc<Mutex<Trace>>,
}
impl Drop for Directory {
    fn drop(&mut self) {
        drop(self.inner.take());
        self.trace
            .lock()
            .unwrap()
            .event("drop_directory", "namespace", 0, 0, "OK");
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        drop(self.inner.take());
        self.trace
            .lock()
            .unwrap()
            .event("drop_lease", "LOCK", 0, 0, "OK");
    }
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        let before = self.trace.lock().unwrap().root.join("LOCK").exists();
        self.trace
            .lock()
            .unwrap()
            .event("acquire", "LOCK", 0, 0, "ATTEMPT");
        let result = self.inner.as_ref().unwrap().acquire_writer();
        match result {
            Ok(inner) => {
                self.trace.lock().unwrap().event(
                    "acquire",
                    "LOCK",
                    0,
                    0,
                    if before { "EXISTING" } else { "CREATED" },
                );
                Ok(Writer {
                    inner: Some(inner),
                    trace: Arc::clone(&self.trace),
                })
            }
            Err(error) => {
                self.trace.lock().unwrap().event(
                    "acquire",
                    "LOCK",
                    0,
                    0,
                    &format!("ERR:{error:?}"),
                );
                Err(error)
            }
        }
    }
}
impl Writer {
    fn wrap(&self, inner: LinuxNamespaceFile, name: FileName) -> File {
        let mut trace = self.trace.lock().unwrap();
        trace.handles += 1;
        trace.peak = trace.peak.max(trace.handles);
        trace.event("retain_file", name.as_str(), 0, 0, "OK");
        File {
            inner: Some(inner),
            name,
            trace: Arc::clone(&self.trace),
        }
    }
}
impl Drop for File {
    fn drop(&mut self) {
        drop(self.inner.take());
        let mut trace = self.trace.lock().unwrap();
        trace.handles -= 1;
        trace.event("close", self.name.as_str(), 0, 0, "OK");
    }
}
impl FileIo for File {
    fn read_at(&mut self, output: &mut [u8], offset: u64) -> io::Result<usize> {
        self.trace.lock().unwrap().event(
            "read",
            self.name.as_str(),
            offset,
            output.len(),
            "ATTEMPT",
        );
        let result = self.inner.as_mut().unwrap().read_at(output, offset);
        let outcome = match &result {
            Ok(n) => format!("OK:{n}:{}", hash_hex(&output[..*n])),
            Err(e) => format!("ERR:{e:?}"),
        };
        self.trace.lock().unwrap().event(
            "read",
            self.name.as_str(),
            offset,
            output.len(),
            &outcome,
        );
        result
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        self.trace
            .lock()
            .unwrap()
            .mutation("write", self.name.as_str(), offset, bytes.len());
        let cut = {
            let mut trace = self.trace.lock().unwrap();
            if self.name.as_str() == "d3-data-0000000100000001"
                && offset == 11 * PAGE_BYTES as u64
                && trace.undo_home_cut.is_some()
            {
                let cut = trace.undo_home_cut.take().expect("armed one-shot cut");
                assert_eq!(bytes.len(), PAGE_BYTES);
                assert_eq!(&bytes[..8], b"XDP1\x01\x00\x08\x00");
                for (at, expected) in [
                    (24, 0x100000001u64),
                    (32, 11),
                    (40, 2),
                    (48, 3),
                    (56, 1),
                    (64, 6),
                ] {
                    assert_eq!(
                        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()),
                        expected
                    );
                }
                assert_eq!(foundation::codec::content_digest(bytes), cut.digest);
                trace.event(
                    "cut",
                    "A_FU_BEFORE_UNDO_HOME",
                    offset,
                    bytes.len(),
                    "REACHED:1",
                );
                Some(cut)
            } else {
                None
            }
        };
        if let Some(cut) = cut {
            crash_barrier(&self.trace, &cut.marker, "A_FU_BEFORE_UNDO_HOME");
        }
        let catalog_cut = {
            let mut trace = self.trace.lock().unwrap();
            if self.name.as_str() == "d3-data-0000000100000001"
                && offset == 8 * PAGE_BYTES as u64
                && trace.catalog_home_cut.is_some()
            {
                let cut = trace.catalog_home_cut.take().expect("armed Catalog cut");
                assert_eq!(bytes.len(), PAGE_BYTES);
                assert_eq!(&bytes[..4], b"XDP1");
                for (at, expected) in [
                    (24, 0x100000001u64),
                    (32, 8),
                    (40, 1),
                    (48, 5),
                    (56, 1),
                    (64, 8),
                ] {
                    assert_eq!(
                        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()),
                        expected
                    );
                }
                assert_eq!(foundation::codec::content_digest(bytes), cut.digest);
                trace.event(
                    "cut",
                    "A_CREATE_BEFORE_CATALOG_HOME",
                    offset,
                    bytes.len(),
                    "REACHED:1",
                );
                Some(cut)
            } else {
                None
            }
        };
        if let Some(cut) = catalog_cut {
            crash_barrier(&self.trace, &cut.marker, "A_CREATE_BEFORE_CATALOG_HOME");
        }
        let result = self.inner.as_mut().unwrap().write_at(bytes, offset);
        if self.name.as_str() == "d3-wal-00000001" {
            if let Ok(written) = result {
                self.trace.lock().unwrap().catalog_wal_write_end = offset
                    .checked_add(written as u64)
                    .expect("native WAL write end");
            }
        }
        let outcome = match &result {
            Ok(n) => format!("OK:{n}:{}", hash_hex(&bytes[..*n])),
            Err(e) => format!("ERR:{e:?}"),
        };
        self.trace.lock().unwrap().event(
            "write",
            self.name.as_str(),
            offset,
            bytes.len(),
            &outcome,
        );
        result
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.trace
            .lock()
            .unwrap()
            .mutation("sync", self.name.as_str(), 0, 0);
        let result = self.inner.as_mut().unwrap().sync_all();
        self.trace
            .lock()
            .unwrap()
            .event("sync", self.name.as_str(), 0, 0, &format!("{result:?}"));
        let inject = {
            let mut trace = self.trace.lock().unwrap();
            if result.is_ok()
                && self.name.as_str() == "d3-wal-00000001"
                && trace.catalog_sync_after_error == Some(trace.catalog_wal_write_end)
            {
                let end = trace
                    .catalog_sync_after_error
                    .take()
                    .expect("armed one-shot sync error");
                trace.event(
                    "fault",
                    "A_CREATE_COMMIT_SYNC_AFTER_ERROR",
                    end,
                    0,
                    "AFTER_SUCCESS:1",
                );
                true
            } else {
                false
            }
        };
        if inject {
            return Err(io::Error::other("Catalog Commit sync AfterError"));
        }
        result
    }
    fn set_len(&mut self, length: u64) -> io::Result<()> {
        self.trace
            .lock()
            .unwrap()
            .mutation("resize", self.name.as_str(), length, 0);
        let result = self.inner.as_mut().unwrap().set_len(length);
        self.trace.lock().unwrap().event(
            "resize",
            self.name.as_str(),
            length,
            0,
            &format!("{result:?}"),
        );
        result
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        let mut trace = self.trace.lock().unwrap();
        assert!(!trace.readonly);
        trace.entropy += 1;
        let bytes = namespace().database.bytes();
        let ordinal = trace.entropy;
        writeln!(
            trace.entropy_output,
            "{ordinal}\t{}\t0102030405060708090a0b0c0d0e0f10",
            std::process::id()
        )
        .unwrap();
        trace.entropy_output.flush().unwrap();
        trace.event(
            "entropy",
            "namespace",
            0,
            16,
            "0102030405060708090a0b0c0d0e0f10",
        );
        Ok(bytes)
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        self.trace
            .lock()
            .unwrap()
            .mutation("create", name.as_str(), 0, 0);
        let result = self.inner.as_mut().unwrap().create_new(name);
        self.trace.lock().unwrap().event(
            "create",
            name.as_str(),
            0,
            0,
            if result.is_ok() { "OK" } else { "ERR" },
        );
        result.map(|file| self.wrap(file, *name))
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        self.trace
            .lock()
            .unwrap()
            .event("open", name.as_str(), 0, 0, "ATTEMPT");
        let result = self.inner.as_mut().unwrap().open_existing(name, mode);
        self.trace.lock().unwrap().event(
            "open",
            name.as_str(),
            0,
            0,
            if result.is_ok() { "OK" } else { "ERR" },
        );
        result.map(|file| self.wrap(file, *name))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        self.trace
            .lock()
            .unwrap()
            .event("open_meta", name.as_str(), 0, 0, "ATTEMPT");
        let result = self
            .inner
            .as_mut()
            .unwrap()
            .open_existing_with_metadata(name, mode);
        match result {
            Ok((file, metadata)) => {
                self.trace.lock().unwrap().event(
                    "open_meta",
                    name.as_str(),
                    0,
                    0,
                    &format!(
                        "{}:{}:{}",
                        metadata.identity.device, metadata.identity.inode, metadata.length
                    ),
                );
                Ok((self.wrap(file, *name), metadata))
            }
            Err(error) => {
                self.trace.lock().unwrap().event(
                    "open_meta",
                    name.as_str(),
                    0,
                    0,
                    &format!("ERR:{error:?}"),
                );
                Err(error)
            }
        }
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        let result = self.inner.as_ref().unwrap().metadata(name);
        self.trace
            .lock()
            .unwrap()
            .event("metadata", name.as_str(), 0, 0, &format!("{result:?}"));
        result
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        let result = self.inner.as_ref().unwrap().list_owned(limit);
        self.trace.lock().unwrap().event(
            "list",
            "namespace",
            0,
            limit,
            &format!("{:?}", result.as_ref().map(Vec::len)),
        );
        result
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.trace
            .lock()
            .unwrap()
            .mutation("dirsync", "namespace", 0, 0);
        let result = self.inner.as_mut().unwrap().sync_directory();
        self.trace
            .lock()
            .unwrap()
            .event("dirsync", "namespace", 0, 0, &format!("{result:?}"));
        result
    }
    fn publish_selector(&mut self, name: &FileName, selector: Selector) -> NamespaceResult<()> {
        self.trace
            .lock()
            .unwrap()
            .mutation("publish", name.as_str(), 0, 0);
        let result = self
            .inner
            .as_mut()
            .unwrap()
            .publish_selector(name, selector);
        self.trace.lock().unwrap().event(
            "publish",
            name.as_str(),
            0,
            0,
            &format!("{}:{result:?}", selector.as_str()),
        );
        result
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        self.trace
            .lock()
            .unwrap()
            .mutation("remove", name.as_str(), 0, 0);
        let result = self.inner.as_mut().unwrap().remove_retired(name);
        self.trace
            .lock()
            .unwrap()
            .event("remove", name.as_str(), 0, 0, &format!("{result:?}"));
        result
    }
}
fn fresh_write(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}

fn file_index(root: &Path) -> String {
    use std::os::unix::fs::MetadataExt;
    let mut entries: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    let mut result = String::from("name\tbytes\tdevice\tinode\tsha256\n");
    assert_eq!(entries.len(), 15);
    let mut total = 0;
    for entry in entries {
        let metadata = entry.metadata().unwrap();
        assert!(metadata.is_file());
        let bytes = fs::read(entry.path()).unwrap();
        assert_eq!(bytes.len() as u64, metadata.len());
        total += bytes.len();
        result.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            entry.file_name().to_str().unwrap(),
            bytes.len(),
            metadata.dev(),
            metadata.ino(),
            hash_hex(&bytes)
        ));
    }
    assert_eq!(total, 22_069_248);
    result
}

#[test]
#[ignore = "explicit G5 native consumer; one new process per selected profile"]
fn worker_g5_clean() {
    let profile: u32 = std::env::var("XADB_PLAN4_CLEAN_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&profile));
    let id = format!("P4-G5-NATIVE-P{profile}");
    let mut harness = Harness::new(vec![id.clone()]);
    harness.case(&id, || {
        let root = PathBuf::from(std::env::var("XADB_PLAN4_CLEAN_DB").unwrap());
        let output = PathBuf::from(std::env::var("XADB_PLAN4_CLEAN_OUT").unwrap());
        assert!(root.is_absolute() && output.is_absolute());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        let trace_file = OpenOptions::new().create_new(true).write(true).open(output.join("effects.tsv")).unwrap();
        let entropy_file = OpenOptions::new().create_new(true).write(true).open(output.join("entropy.tsv")).unwrap();
        let trace = Arc::new(Mutex::new(Trace { root: root.clone(), output: trace_file, entropy_output: entropy_file, readonly: false, handles: 0, peak: 0, fd_peak: 0, events: 0, mutations: 0, entropy: 0, log_bytes: 0, undo_home_cut: None, catalog_home_cut: None, catalog_sync_after_error: None, catalog_wal_write_end: 0 }));
        let directory = Directory { inner: Some(LinuxDirectory::open_existing(&root).unwrap()), trace: Arc::clone(&trace) };
        let writer = directory.acquire_writer().unwrap();
        drop(directory);
        let mut maintenance = boxed::<12_582_912>();
        let mut slot = boxed::<MAX_FRAME_BYTES>();
        let mut flush = boxed::<4_194_304>();
        let fresh = FreshStorageSession::create_retention_catalog(writer, profile, &mut slot, &mut maintenance).unwrap();
        let storage = fresh.install_doublewrite(&mut flush).unwrap().into_mutable_storage();
        let before = file_index(&root);
        fresh_write(&output.join("before.tsv"), before.as_bytes());
        let (boundary, setup_mutations, setup_entropy) = {
            let mut t = trace.lock().unwrap();
            t.event("g5_begin", "namespace", 0, 0, "OK");
            t.readonly = true;
            (t.events, t.mutations, t.entropy)
        };
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<{ 7 * PAGE_BYTES }>();
        let mut bound = storage.bind_plan4_pages(&core, &mut frames, &mut operation).unwrap();
        super::tests::lifecycle(&mut bound, &core, 4);
        drop(bound);
        assert!(storage.session_fence().is_fenced());
        drop(storage);
        let after = file_index(&root);
        assert_eq!(after, before);
        fresh_write(&output.join("after.tsv"), after.as_bytes());
        let t = trace.lock().unwrap();
        assert_eq!(t.handles, 0);
        assert_eq!(db_fds(&root), 0);
        assert_eq!(t.mutations, setup_mutations);
        assert_eq!(t.entropy, setup_entropy);
        assert_eq!(setup_entropy, 1);
        assert!(t.peak <= 3 && t.fd_peak <= 5);
        fresh_write(&output.join("resources.tsv"), format!("writer_bytes\t{}\nfile_bytes\t{}\nhandles_peak\t{}\nhandles_final\t0\nfd_peak\t{}\nfd_final\t0\nsetup_mutations\t{}\ng5_mutations\t0\nsetup_entropy\t{}\ng5_entropy\t0\ng5_event_begin\t{}\nevents\t{}\n", std::mem::size_of::<Writer>(), std::mem::size_of::<File>(), t.peak, t.fd_peak, setup_mutations, setup_entropy, boundary, t.events).as_bytes());
        fresh_write(&output.join("execution.tsv"), format!("id\t{id}\npid\t{}\nprofile\t{profile}\nsource_sha256\t{}\nelf_sha256\t{}\ninput_sha256\t{}\n", std::process::id(), std::env::var("XADB_PLAN4_GENESIS_SOURCE_SHA256").unwrap(), std::env::var("XADB_PLAN4_GENESIS_ELF_SHA256").unwrap(), std::env::var("XADB_PLAN4_GENESIS_INPUT_SHA256").unwrap()).as_bytes());
        println!("P4_NATIVE_COMPLETE\t{id}\t{}", std::process::id());
    });
    harness.finish();
}

/// New recovery workers use the same measured native Writer/File types.
/// Construction creates only caller-selected trace files; the namespace root
/// must already exist. No completed G5 test function is invoked.
pub(in crate::storage) fn open_native(
    root: &Path,
    output: &Path,
    entropy_output: &Path,
    readonly: bool,
) -> (Directory, Arc<Mutex<Trace>>) {
    let trace = Arc::new(Mutex::new(Trace {
        root: root.to_path_buf(),
        output: OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap(),
        entropy_output: OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(entropy_output)
            .unwrap(),
        readonly,
        handles: 0,
        peak: 0,
        fd_peak: 0,
        events: 0,
        mutations: 0,
        entropy: 0,
        log_bytes: 0,
        undo_home_cut: None,
        catalog_home_cut: None,
        catalog_sync_after_error: None,
        catalog_wal_write_end: 0,
    }));
    let directory = Directory {
        inner: Some(LinuxDirectory::open_existing(root).unwrap()),
        trace: Arc::clone(&trace),
    };
    (directory, trace)
}
#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct NativeTraceReceipt {
    pub(in crate::storage) handles: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) fd_peak: usize,
    pub(in crate::storage) events: usize,
    pub(in crate::storage) mutations: usize,
    pub(in crate::storage) entropy: usize,
    pub(in crate::storage) log_bytes: usize,
}
pub(in crate::storage) fn native_receipt(trace: &Arc<Mutex<Trace>>) -> NativeTraceReceipt {
    let trace = trace.lock().unwrap();
    NativeTraceReceipt {
        handles: trace.handles,
        peak: trace.peak,
        fd_peak: trace.fd_peak,
        events: trace.events,
        mutations: trace.mutations,
        entropy: trace.entropy,
        log_bytes: trace.log_bytes,
    }
}

/// One exact before-syscall cut for the new Free Undo integration worker. The
/// expected digest is an independent literal; every existing worker is unarmed.
struct UndoHomeCut {
    marker: PathBuf,
    digest: [u8; 32],
}
pub(in crate::storage) fn arm_undo_home_cut(
    trace: &Arc<Mutex<Trace>>,
    marker: &Path,
    digest: [u8; 32],
) {
    assert!(marker.is_absolute() && !marker.exists());
    let mut trace = trace.lock().unwrap();
    assert!(trace.undo_home_cut.is_none());
    trace.undo_home_cut = Some(UndoHomeCut {
        marker: marker.to_path_buf(),
        digest,
    });
    trace.event(
        "arm",
        "A_FU_BEFORE_UNDO_HOME",
        11 * PAGE_BYTES as u64,
        PAGE_BYTES,
        "ARMED:1",
    );
}
struct CatalogHomeCut {
    marker: PathBuf,
    digest: [u8; 32],
}

pub(in crate::storage) fn arm_catalog_home_cut(
    trace: &Arc<Mutex<Trace>>,
    marker: &Path,
    digest: [u8; 32],
) {
    assert!(marker.is_absolute() && !marker.exists());
    let mut trace = trace.lock().unwrap();
    assert!(trace.catalog_home_cut.is_none() && trace.undo_home_cut.is_none());
    trace.catalog_home_cut = Some(CatalogHomeCut {
        marker: marker.to_path_buf(),
        digest,
    });
    trace.event(
        "arm",
        "A_CREATE_BEFORE_CATALOG_HOME",
        8 * PAGE_BYTES as u64,
        PAGE_BYTES,
        "ARMED:1",
    );
}

pub(in crate::storage) fn arm_catalog_sync_after_error(
    trace: &Arc<Mutex<Trace>>,
    expected_epoch1_end_offset: u64,
) {
    let mut trace = trace.lock().unwrap();
    assert!(trace.catalog_sync_after_error.is_none());
    assert!(expected_epoch1_end_offset > trace.catalog_wal_write_end);
    trace.catalog_sync_after_error = Some(expected_epoch1_end_offset);
    trace.event(
        "arm",
        "A_CREATE_COMMIT_SYNC_AFTER_ERROR",
        expected_epoch1_end_offset,
        0,
        "ARMED:1",
    );
}

/// Flushes a reached marker, releases the trace mutex and awaits an external
/// SIGKILL for at most five seconds. Returning is a failed crash injection.
pub(in crate::storage) fn crash_barrier(trace: &Arc<Mutex<Trace>>, marker: &Path, name: &str) -> ! {
    assert!(matches!(
        name,
        "A_FU_BEFORE_UNDO_HOME"
            | "A_FU_AFTER_UNDO_DURABLE"
            | "A_CREATE_ACK_T"
            | "A_CREATE_ACK_U"
            | "A_CREATE_BEFORE_CATALOG_HOME"
            | "A_CREATE_AFTER_CLR_DURABLE"
            | "A_CREATE_UNKNOWN_AFTER_SYNC"
    ));
    let peaks = {
        let mut trace = trace.lock().unwrap();
        trace.event("barrier", name, 0, 1, "REACHED");
        trace.output.sync_all().unwrap();
        assert!(trace.peak <= 3 && trace.fd_peak <= 6);
        (trace.handles, trace.peak, trace.fd_peak)
    };
    let staged = marker.with_extension("staged");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .unwrap();
    writeln!(
        output,
        "{name}\t{}\t1\t{}\t{}\t{}",
        std::process::id(),
        peaks.0,
        peaks.1,
        peaks.2
    )
    .unwrap();
    output.sync_all().unwrap();
    drop(output);
    fs::rename(staged, marker).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("external controller failed to SIGKILL reached cut {name}");
}
