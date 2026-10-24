//! Actual Linux files for the explicit plan4 three-process campaign only.
use super::Plan4GenesisSelection;
use crate::storage::{
    retention_genesis::tests::{Harness, hash_hex, namespace},
    wal_frame::MAX_FRAME_BYTES,
    wal_io::FreshStorageSession,
};
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
        assert!(self.log_bytes <= 8 * 1024 * 1024, "native trace bound");
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

#[test]
#[ignore = "explicit native process phase only"]
fn worker_plan4_genesis() {
    let phase = std::env::var("XADB_PLAN4_GENESIS_PHASE").unwrap();
    assert!(matches!(phase.as_str(), "produce" | "select" | "reentry"));
    let profile: u32 = std::env::var("XADB_PLAN4_GENESIS_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&profile));
    let id = format!("P4-NATIVE-P{profile}-{}", phase.to_ascii_uppercase());
    let mut harness = Harness::new(vec![id.clone()]);
    harness.case(&id, || {
        let root = PathBuf::from(std::env::var("XADB_PLAN4_GENESIS_DB").unwrap());
        let output = PathBuf::from(std::env::var("XADB_PLAN4_GENESIS_OUT").unwrap());
        assert!(root.is_absolute() && output.is_absolute());
        let readonly = phase != "produce";
        if readonly { assert!(root.join("LOCK").is_file()); }
        let trace_file = OpenOptions::new().create_new(true).write(true).open(output.join("effects.tsv")).unwrap();
        let entropy_file = OpenOptions::new().create_new(true).write(true).open(output.join("entropy.tsv")).unwrap();
        let trace = Arc::new(Mutex::new(Trace { root: root.clone(), output: trace_file, entropy_output: entropy_file, readonly, handles: 0, peak: 0, fd_peak: 0, events: 0, mutations: 0, entropy: 0, log_bytes: 0 }));
        let directory = Directory { inner: Some(LinuxDirectory::open_existing(&root).unwrap()), trace: Arc::clone(&trace) };
        let mut maintenance = boxed::<12_582_912>();
        if phase == "produce" {
            let writer = directory.acquire_writer().unwrap();
            drop(directory);
            let mut slot = boxed::<MAX_FRAME_BYTES>();
            let mut flush = boxed::<4_194_304>();
            let fresh = FreshStorageSession::create_retention_catalog(writer, profile, &mut slot, &mut maintenance).unwrap();
            let installed = fresh.install_doublewrite(&mut flush).unwrap();
            drop(installed);
        } else {
            let owner = Plan4GenesisSelection::<Writer>::acquire(&directory, Some(namespace()), &mut maintenance).unwrap();
            let observation = owner.observation();
            assert_eq!(observation.namespace(), namespace());
            assert_eq!(observation.profile_id(), profile);
            assert_eq!(observation.bootstrap_plan(), 4);
            assert_eq!(observation.issued_object_high_water(), 5);
            assert_eq!((observation.allocated_pages(), observation.free_pages(), observation.free_jobs(), observation.catalog_entries()), (9, 55, 64, 0));
            assert_eq!((observation.catalog_anchor().page_no, observation.catalog_root().page_no), (7, 8));
            assert_eq!((observation.filemap_generation(), observation.doublewrite_generation()), (1, 1));
            fresh_write(&output.join("observation.tsv"), format!("plan\t4\nprofile\t{profile}\nhigh_water\t5\nallocated\t9\nfree\t55\njobs_free\t64\nanchor\t7\nroot\t8\nentries\t0\nepoch_digest\t{}\n", observation.epoch_digest().iter().map(|b| format!("{b:02x}")).collect::<String>()).as_bytes());
            drop(owner);
            drop(directory);
        }
        let trace = trace.lock().unwrap();
        assert_eq!(trace.handles, 0);
        assert_eq!(db_fds(&root), 0);
        assert_eq!(trace.entropy, usize::from(!readonly));
        if readonly { assert_eq!(trace.mutations, 0); }
        assert!(trace.peak <= if readonly { 2 } else { 3 });
        assert!(trace.fd_peak <= 5);
        fresh_write(&output.join("resources.tsv"), format!("writer_bytes\t{}\nfile_bytes\t{}\nhandles_peak\t{}\nhandles_final\t0\nfd_peak\t{}\nfd_final\t0\nmutations\t{}\nentropy_calls\t{}\nevents\t{}\n", std::mem::size_of::<Writer>(), std::mem::size_of::<File>(), trace.peak, trace.fd_peak, trace.mutations, trace.entropy, trace.events).as_bytes());
        fresh_write(&output.join("execution.tsv"), format!("id\t{id}\nphase\t{phase}\npid\t{}\nprofile\t{profile}\nsource_sha256\t{}\nelf_sha256\t{}\ninput_sha256\t{}\n", std::process::id(), std::env::var("XADB_PLAN4_GENESIS_SOURCE_SHA256").unwrap(), std::env::var("XADB_PLAN4_GENESIS_ELF_SHA256").unwrap(), std::env::var("XADB_PLAN4_GENESIS_INPUT_SHA256").unwrap()).as_bytes());
        println!("P4_NATIVE_COMPLETE\t{id}\t{}", std::process::id());
    });
    harness.finish();
}
