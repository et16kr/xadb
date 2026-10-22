//! Actual Linux plan2 creator. External checker audits every file and effect.
use super::*;
use std::{fs, io, path::PathBuf};
use xadb_foundation::{
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

#[derive(Default)]
struct Trace {
    events: Vec<String>,
    handles: usize,
    peak: usize,
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
fn record(
    trace: &Arc<Mutex<Trace>>,
    kind: &str,
    name: &str,
    offset: u64,
    len: usize,
    bytes: &[u8],
) {
    let mut trace = trace.lock().unwrap();
    assert!(trace.events.len() < 400);
    trace.events.push(format!(
        "{}\t{}\t{}\t{}\t{}",
        kind,
        if name.is_empty() { "-" } else { name },
        offset,
        len,
        hex(&content_digest(bytes))
    ));
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
impl Drop for File {
    fn drop(&mut self) {
        drop(self.inner.take());
        self.trace.lock().unwrap().handles -= 1;
    }
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.inner.as_mut().unwrap().read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let n = self.inner.as_mut().unwrap().write_at(bytes, offset)?;
        record(&self.trace, "write", &self.name, offset, n, &bytes[..n]);
        Ok(n)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.inner.as_mut().unwrap().sync_all()?;
        record(&self.trace, "sync", &self.name, 0, 0, &[]);
        Ok(())
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        panic!("bootstrap does not resize")
    }
}
impl Writer {
    fn wrap(&self, inner: LinuxNamespaceFile, name: &FileName) -> File {
        let mut trace = self.trace.lock().unwrap();
        trace.handles += 1;
        trace.peak = trace.peak.max(trace.handles);
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
        self.inner.random_incarnation()
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let file = self.inner.create_new(name)?;
        record(&self.trace, "create", name.as_str(), 0, 0, &[]);
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
        let (file, meta) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.wrap(file, name), meta))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.inner.sync_directory()?;
        record(&self.trace, "directory_sync", "", 0, 0, &[]);
        Ok(())
    }
    fn publish_selector(&mut self, temp: &FileName, selector: Selector) -> NamespaceResult<()> {
        self.inner.publish_selector(temp, selector)?;
        record(&self.trace, "publish", temp.as_str(), 0, 0, &[]);
        Ok(())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        panic!("bootstrap cannot remove")
    }
}
fn db_fds(path: &std::path::Path) -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .filter(|target| target.starts_with(path))
        .count()
}

#[test]
#[ignore = "explicit native P1 runner with fresh output directory"]
fn status_bootstrap_native_p1() {
    let output = PathBuf::from(std::env::var("XADB_STATUS_BOOTSTRAP_NATIVE_DIR").unwrap());
    assert!(output.is_absolute());
    let database = output.join("database");
    fs::create_dir(&database).unwrap();
    let directory = LinuxDirectory::open_existing(&database).unwrap();
    let trace = Arc::new(Mutex::new(Trace::default()));
    let writer = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    drop(directory);
    assert_eq!(db_fds(&database), 2);
    let mut slot = port::slot();
    let fresh =
        boot::FreshStorageSession::create_status(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let identity = {
        let role = fresh.status_role().unwrap();
        assert_eq!(role.object().get(), 2);
        assert_eq!(role.generation().get(), 1);
        assert_eq!(role.kind(), PageKind::TransactionStatus);
        assert_eq!(role.level(), 0);
        assert_eq!(role.issued_object_high_water(), 2);
        assert!(role.uses_fence(&fresh.wal.fenced));
        assert_eq!(role.namespace().timeline, 1);
        hex(&role.namespace().database.bytes())
    };
    assert_eq!(trace.lock().unwrap().handles, 1);
    let live_fds = db_fds(&database);
    assert_eq!(live_fds, 3);
    drop(fresh);
    assert_eq!(db_fds(&database), 0);
    let trace = trace.lock().unwrap();
    assert_eq!((trace.events.len(), trace.handles, trace.peak), (350, 0, 3));
    fs::write(output.join("effects.tsv"), trace.events.join("\n") + "\n").unwrap();
    fs::write(
        output.join("role.tsv"),
        format!("P1\t1\t{identity}\t1\t2\t1\t9\t0\t2\t350\t3\t0\t{live_fds}\t0\n"),
    )
    .unwrap();
    println!("\nXADB_STATUS_BOOTSTRAP_NATIVE P1");
}
