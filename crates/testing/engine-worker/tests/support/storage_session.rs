//! Real Linux bootstrap effects with retained native fault/hook receipts.
#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceError, NamespaceErrorKind,
        NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

pub const WAL: &str = "d3-wal-00000001";
pub const CUTS: [(&str, usize); 13] = [
    ("descriptor_sync", 14),
    ("descriptor_directory", 15),
    ("allocation_write", 17),
    ("data_sync", 81),
    ("root_write", 83),
    ("chunk_write", 84),
    ("bank0_sync", 212),
    ("bank1_first_write", 214),
    ("bank1_sync", 343),
    ("files_directory", 344),
    ("selector_sync", 347),
    ("selector_publish", 348),
    ("selector_directory", 349),
];
pub const INSTALL: [(&str, &str); 12] = [
    ("create", "d3-profile"),
    ("write", "d3-profile"),
    ("sync", "d3-profile"),
    ("create", WAL),
    ("write", WAL),
    ("sync", WAL),
    ("dirsync", ""),
    ("create", "d3-writer-stage"),
    ("write", "d3-writer-stage"),
    ("sync", "d3-writer-stage"),
    ("publish", "d3-writer-stage"),
    ("dirsync", ""),
];
pub fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
pub fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
pub fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn memory() -> Box<[u8; 1048576]> {
    vec![0; 1048576].into_boxed_slice().try_into().unwrap()
}
pub fn frame(database: [u8; 16], previous: u64, lsn: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 192 + payload.len()];
    bytes[..4].copy_from_slice(b"XWF1");
    bytes[4] = 1;
    bytes[6] = 3;
    bytes[8..10].copy_from_slice(&160u16.to_le_bytes());
    let len = bytes.len();
    put32(&mut bytes, 12, len as u32);
    put32(&mut bytes, 16, payload.len() as u32);
    bytes[24..40].copy_from_slice(&database);
    put64(&mut bytes, 40, 1);
    put32(&mut bytes, 48, 1);
    put64(&mut bytes, 56, lsn);
    put64(&mut bytes, 64, previous);
    bytes[160..160 + payload.len()].copy_from_slice(payload);
    let digest = content_digest(&bytes[..128]);
    bytes[128..160].copy_from_slice(&digest);
    seal(&mut bytes);
    bytes
}
pub fn expected_controls(database: [u8; 16], id: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let templates: [&[u8]; 3] = [
        include_bytes!("../../../../engine/tests/fixtures/dev3-ownership-profile1.bin"),
        include_bytes!("../../../../engine/tests/fixtures/dev3-ownership-profile2.bin"),
        include_bytes!("../../../../engine/tests/fixtures/dev3-ownership-profile3.bin"),
    ];
    let mut profile = templates[id as usize - 1].to_vec();
    profile[16..32].copy_from_slice(&database);
    put64(&mut profile, 32, 1);
    put64(&mut profile, 40, 1);
    seal(&mut profile);
    let mut epoch = vec![0; 4096];
    epoch[..4].copy_from_slice(b"XWE1");
    epoch[4] = 1;
    epoch[6] = 1;
    put32(&mut epoch, 8, 4096);
    epoch[16..32].copy_from_slice(&database);
    put64(&mut epoch, 32, 1);
    put32(&mut epoch, 40, 1);
    put64(&mut epoch, 48, 1);
    epoch[56..88].copy_from_slice(&content_digest(&profile));
    put64(&mut epoch, 96, 4096);
    put32(&mut epoch, 104, 1048576);
    put64(&mut epoch, 112, 268435456);
    seal(&mut epoch);
    let mut writer = vec![0; 4096];
    writer[..4].copy_from_slice(b"XWS1");
    writer[4] = 1;
    writer[6] = 1;
    put32(&mut writer, 8, 4096);
    writer[16..32].copy_from_slice(&database);
    put64(&mut writer, 32, 1);
    put64(&mut writer, 40, 1);
    put32(&mut writer, 48, 1);
    put64(&mut writer, 56, 1);
    writer[64..96].copy_from_slice(&content_digest(&profile));
    writer[96..128].copy_from_slice(&content_digest(&epoch));
    put64(&mut writer, 136, 4096);
    seal(&mut writer);
    (profile, epoch, writer)
}
pub fn artifact_root(label: &str) -> PathBuf {
    let parent = std::env::var_os("XADB_STORAGE_NATIVE_ARTIFACT_PARENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap().join("artifacts/dev3"));
    fs::create_dir_all(&parent).unwrap();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = parent.join(format!("storage-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir(&root).unwrap();
    println!("XADB_STORAGE_NATIVE_ARTIFACT {}", root.display());
    root
}
pub fn inventory(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (
                entry.file_name().to_str().unwrap().to_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}
pub fn file_manifest(path: &Path) -> Value {
    json!(inventory(path).iter().filter(|(name,_)|name.as_str()!="LOCK").map(|(name, bytes)|  { let meta=fs::symlink_metadata(path.join(name)).unwrap(); json!({"name":name,"length":bytes.len(),"sha256":hex(&content_digest(bytes)),"device":meta.dev(),"inode":meta.ino(),"links":meta.nlink()}) }).collect::<Vec<_>>())
}
pub fn write_json(path: &Path, value: &Value) {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, value).unwrap();
    file.write_all(b"\n").unwrap();
}

#[derive(Clone, Copy)]
pub enum Fault {
    Before(usize),
    After(usize),
    Prefix(usize, usize),
}
#[derive(Default)]
pub struct Trace {
    pub events: Vec<Value>,
    pub fault: Option<Fault>,
    pub hit: usize,
    pub cut: Option<String>,
    /// An explicit index for a different consuming installation's frozen cuts.
    pub cut_index: Option<usize>,
    pub hook_path: Option<PathBuf>,
    pub handles: usize,
    pub peak: usize,
    pub entropy: Option<[u8; 16]>,
}
pub struct Writer {
    inner: LinuxWriterLease,
    pub trace: Arc<Mutex<Trace>>,
}
pub struct File {
    inner: LinuxNamespaceFile,
    name: String,
    trace: Arc<Mutex<Trace>>,
}
impl Drop for File {
    fn drop(&mut self) {
        self.trace.lock().unwrap().handles -= 1;
    }
}
fn fault() -> io::Error {
    io::Error::other("fixed native genesis fault")
}
fn namespace_error(effect: Effect) -> NamespaceError {
    NamespaceError::io(
        NamespaceErrorKind::Io,
        effect,
        "fixed native genesis fault",
        fault(),
    )
}
fn begin(
    trace: &Arc<Mutex<Trace>>,
    kind: &str,
    name: &str,
    offset: u64,
    length: usize,
) -> io::Result<usize> {
    let mut t = trace.lock().unwrap();
    let at = t.events.len();
    t.events.push(json!({"at":at,"kind":kind,"name":name,"offset":offset,"length":length,"completed":false,"injected":null}));
    if matches!(t.fault, Some(Fault::Before(n)) if n == at) {
        t.hit += 1;
        t.events[at]["injected"] = json!("before");
        return Err(fault());
    }
    Ok(at)
}
fn finish(trace: &Arc<Mutex<Trace>>, at: usize, completed_bytes: Option<usize>) -> io::Result<()> {
    let mut t = trace.lock().unwrap();
    t.events[at]["completed"] = json!(true);
    t.events[at]["completed_bytes"] = json!(completed_bytes);
    if matches!(t.fault, Some(Fault::After(n)) if n == at) {
        t.hit += 1;
        t.events[at]["injected"] = json!("after");
        return Err(fault());
    }
    if let Some(cut) = &t.cut {
        let expected = t
            .cut_index
            .unwrap_or_else(|| CUTS.iter().find(|(name, _)| *name == cut).unwrap().1);
        if expected == at {
            let path = t.hook_path.as_ref().unwrap();
            write_json(
                path,
                &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"cut":cut,"after":at,"events":t.events,"entropy":t.entropy}),
            );
            println!("XADB_STORAGE_CUT|{cut}");
            io::stdout().flush().unwrap();
            drop(t);
            // The parent owns stdin and sends no continuation. Only SIGKILL ends
            // this worker at the declared after-effect hook.
            let mut line = String::new();
            let _ = io::stdin().read_line(&mut line);
            panic!("cut resumed without kill");
        }
    }
    Ok(())
}
impl Writer {
    pub fn acquire(path: &Path) -> (Self, Arc<Mutex<Trace>>) {
        let inner = LinuxDirectory::open_existing(path)
            .unwrap()
            .acquire_writer()
            .unwrap();
        let trace = Arc::new(Mutex::new(Trace::default()));
        (
            Self {
                inner,
                trace: Arc::clone(&trace),
            },
            trace,
        )
    }
    fn wrap(&self, inner: LinuxNamespaceFile, name: &FileName) -> File {
        let mut t = self.trace.lock().unwrap();
        t.handles += 1;
        t.peak = t.peak.max(t.handles);
        File {
            inner,
            name: name.as_str().to_owned(),
            trace: Arc::clone(&self.trace),
        }
    }
}
impl FileIo for File {
    fn read_at(&mut self, out: &mut [u8], offset: u64) -> io::Result<usize> {
        self.inner.read_at(out, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let at = begin(&self.trace, "write", &self.name, offset, bytes.len())?;
        let plan = self.trace.lock().unwrap().fault;
        if let Some(Fault::Prefix(n, count)) = plan
            && n == at
        {
            assert!(count > 0 && count < bytes.len());
            let completed = self.inner.write_at(&bytes[..count], offset)?;
            assert_eq!(completed, count);
            let mut t = self.trace.lock().unwrap();
            t.hit += 1;
            t.events[at]["injected"] = json!("prefix_error");
            t.events[at]["completed_bytes"] = json!(completed);
            return Err(fault());
        }
        let count = self.inner.write_at(bytes, offset)?;
        finish(&self.trace, at, Some(count))?;
        Ok(count)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let at = begin(&self.trace, "sync", &self.name, 0, 0)?;
        self.inner.sync_all()?;
        finish(&self.trace, at, None)
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        panic!("unexpected native WAL resize")
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        let entropy = self.inner.random_incarnation()?;
        assert!(
            self.trace
                .lock()
                .unwrap()
                .entropy
                .replace(entropy)
                .is_none()
        );
        Ok(entropy)
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let at = begin(&self.trace, "create", name.as_str(), 0, 0)
            .map_err(|_| namespace_error(Effect::Unchanged))?;
        let inner = self.inner.create_new(name)?;
        finish(&self.trace, at, None).map_err(|_| namespace_error(Effect::CallerMustResolve))?;
        Ok(self.wrap(inner, name))
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<File> {
        panic!("native WAL must use handle-bound metadata")
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let (inner, metadata) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.wrap(inner, name), metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        let at = begin(&self.trace, "dirsync", "", 0, 0)
            .map_err(|_| namespace_error(Effect::Unchanged))?;
        self.inner.sync_directory()?;
        finish(&self.trace, at, None).map_err(|_| namespace_error(Effect::CallerMustResolve))
    }
    fn publish_selector(&mut self, name: &FileName, selected: Selector) -> NamespaceResult<()> {
        let at = begin(&self.trace, "publish", name.as_str(), 0, 0)
            .map_err(|_| namespace_error(Effect::Unchanged))?;
        self.inner.publish_selector(name, selected)?;
        finish(&self.trace, at, None).map_err(|_| namespace_error(Effect::CallerMustResolve))
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        panic!("unexpected native WAL cleanup")
    }
}
pub fn assert_install_events(events: &[Value], count: usize) {
    assert_eq!(events.len(), count);
    for (at, event) in events.iter().enumerate() {
        let (kind, name) = if at < 12 {
            INSTALL[at]
        } else if at == 12 {
            ("write", WAL)
        } else if at == 13 {
            ("sync", WAL)
        } else {
            panic!("unexpected effect")
        };
        assert_eq!(event["at"], at);
        assert_eq!(event["kind"], kind);
        assert_eq!(event["name"], name);
    }
}

fn storage_literal(id: u32, name: &str) -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../engine/tests/fixtures/dev3-storage-session/literal/profile{id}-{name}.bin"
    )))
    .unwrap()
}
pub fn expected_storage_objects(db: [u8; 16], id: u32) -> BTreeMap<String, Vec<u8>> {
    // Independent literal offset substitution. No engine control/page encoder.
    let (profile, wal, writer) = expected_controls(db, id);
    let mut allocation = storage_literal(id, "allocation");
    allocation[8..24].copy_from_slice(&db);
    seal(&mut allocation);
    let mut chunk = storage_literal(id, "chunk");
    chunk[16..32].copy_from_slice(&db);
    seal(&mut chunk);
    let mut root = storage_literal(id, "root");
    root[16..32].copy_from_slice(&db);
    root[224..256].copy_from_slice(&content_digest(&chunk));
    seal(&mut root);
    let mut selector = storage_literal(id, "FILEMAP");
    selector[8..24].copy_from_slice(&db);
    selector[48..80].copy_from_slice(&content_digest(&profile));
    selector[112..144].copy_from_slice(&content_digest(&root));
    seal(&mut selector);
    let mut descriptor = storage_literal(id, "d3-bootstrap");
    descriptor[16..32].copy_from_slice(&db);
    descriptor[48..80].copy_from_slice(&content_digest(&profile));
    descriptor[176..208].copy_from_slice(&content_digest(&root));
    descriptor[208..240].copy_from_slice(&content_digest(&allocation));
    seal(&mut descriptor);
    let mut data = vec![0; 1048576];
    data[..16384].copy_from_slice(&allocation);
    let mut bank0 = vec![0; 2113536];
    bank0[..320].copy_from_slice(&root);
    bank0[16384..16640].copy_from_slice(&chunk);
    BTreeMap::from([
        ("LOCK".into(), Vec::new()),
        ("d3-profile".into(), profile),
        (WAL.into(), wal),
        ("WRITER".into(), writer),
        ("d3-bootstrap".into(), descriptor),
        ("d3-data-0000000100000001".into(), data),
        ("d3-filemap-0".into(), bank0),
        ("d3-filemap-1".into(), vec![0; 2113536]),
        ("FILEMAP".into(), selector),
    ])
}
