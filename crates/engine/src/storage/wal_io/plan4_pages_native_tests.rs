//! Actual native adapter for G5's single-process clean live consumer.
use super::*;
use crate::storage::dirty_frame::DirtyFrame;
use crate::storage::{
    retention_genesis::tests::{Harness, hash_hex, namespace},
    wal_frame::MAX_FRAME_BYTES,
    wal_io::FreshStorageSession,
};
use crate::transaction::concurrency::PageConcurrency;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
        WriterNamespace,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

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
        let result = self.inner.as_mut().unwrap().write_at(bytes, offset);
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
        let trace = Arc::new(Mutex::new(Trace { root: root.clone(), output: trace_file, entropy_output: entropy_file, readonly: false, handles: 0, peak: 0, fd_peak: 0, events: 0, mutations: 0, entropy: 0, log_bytes: 0 }));
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
