//! Minimal common-mode forwarding observations. Locks protect only counters;
//! no namespace/file operation executes while the observation mutex is held.
use std::{
    io,
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
use xadbtest_protocol::durable_performance::{Effect, Effects};
#[derive(Clone, Copy)]
enum Kind {
    Create,
    Open,
    Read,
    Write,
    FileSync,
    Resize,
    Rename,
    DirectorySync,
    Remove,
}
pub(super) struct Counters(Mutex<Effects>);
impl Counters {
    pub(super) fn new() -> Arc<Self> {
        let mut effects = Effects::default();
        effects.read.requested_bytes = Some(0);
        effects.read.confirmed_bytes = Some(0);
        effects.write.requested_bytes = Some(0);
        effects.write.confirmed_bytes = Some(0);
        Arc::new(Self(Mutex::new(effects)))
    }
    pub(super) fn snapshot(&self) -> Effects {
        self.0
            .lock()
            .expect("performance effect counter poisoned")
            .clone()
    }
    fn record(&self, kind: Kind, requested: Option<u64>, result: Result<Option<u64>, ()>) {
        let mut all = self.0.lock().expect("performance effect counter poisoned");
        let mut saturated = false;
        let count: &mut Effect = match kind {
            Kind::Create => &mut all.create,
            Kind::Open => &mut all.open,
            Kind::Read => &mut all.read,
            Kind::Write => &mut all.write,
            Kind::FileSync => &mut all.file_sync,
            Kind::Resize => &mut all.resize,
            Kind::Rename => &mut all.rename,
            Kind::DirectorySync => &mut all.directory_sync,
            Kind::Remove => &mut all.remove,
        };
        let mut add = |destination: &mut u64, value: u64| {
            if let Some(sum) = destination.checked_add(value) {
                *destination = sum;
            } else {
                *destination = u64::MAX;
                saturated = true;
            }
        };
        add(&mut count.attempts, 1);
        if let Some(bytes) = requested {
            add(count.requested_bytes.get_or_insert(0), bytes);
        }
        match result {
            Ok(bytes) => {
                add(&mut count.successes, 1);
                if let Some(bytes) = bytes {
                    add(count.confirmed_bytes.get_or_insert(0), bytes);
                }
            }
            Err(()) => {
                add(&mut count.failures, 1);
                if matches!(kind, Kind::Read | Kind::Write) {
                    add(&mut count.unknown_partial_calls, 1);
                }
            }
        }
        all.saturated |= saturated;
    }
}
pub(super) struct Directory {
    inner: LinuxDirectory,
    counters: Arc<Counters>,
}
impl Directory {
    pub(super) fn new(inner: LinuxDirectory, counters: Arc<Counters>) -> Self {
        Self { inner, counters }
    }
}
pub(super) struct Writer {
    inner: LinuxWriterLease,
    counters: Arc<Counters>,
}
pub(super) struct File {
    inner: LinuxNamespaceFile,
    counters: Arc<Counters>,
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        self.inner.acquire_writer().map(|inner| Writer {
            inner,
            counters: Arc::clone(&self.counters),
        })
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.inner.random_incarnation()
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let result = self.inner.create_new(name);
        self.counters.record(
            Kind::Create,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result.map(|inner| File {
            inner,
            counters: Arc::clone(&self.counters),
        })
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        let result = self.inner.open_existing(name, mode);
        self.counters.record(
            Kind::Open,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result.map(|inner| File {
            inner,
            counters: Arc::clone(&self.counters),
        })
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let result = self.inner.open_existing_with_metadata(name, mode);
        self.counters.record(
            Kind::Open,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result.map(|(inner, metadata)| {
            (
                File {
                    inner,
                    counters: Arc::clone(&self.counters),
                },
                metadata,
            )
        })
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        let result = self.inner.sync_directory();
        self.counters.record(
            Kind::DirectorySync,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        let result = self.inner.publish_selector(temporary, selected);
        self.counters.record(
            Kind::Rename,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        let result = self.inner.remove_retired(name);
        self.counters.record(
            Kind::Remove,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result
    }
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        let result = self.inner.read_at(bytes, offset);
        self.counters.record(
            Kind::Read,
            Some(bytes.len() as u64),
            result
                .as_ref()
                .map(|count| Some(*count as u64))
                .map_err(|_| ()),
        );
        result
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let result = self.inner.write_at(bytes, offset);
        self.counters.record(
            Kind::Write,
            Some(bytes.len() as u64),
            result
                .as_ref()
                .map(|count| Some(*count as u64))
                .map_err(|_| ()),
        );
        result
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let result = self.inner.sync_all();
        self.counters.record(
            Kind::FileSync,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        let result = self.inner.set_len(len);
        self.counters.record(
            Kind::Resize,
            None,
            result.as_ref().map(|_| None).map_err(|_| ()),
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::MetadataExt};
    use xadb_foundation::{
        io::read_exact_at,
        namespace::{FileIdentity, NamespaceErrorKind},
    };

    #[test]
    fn open_metadata_counts_one_success_or_failure() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join("data"), b"value").unwrap();
        let native = fs::metadata(temporary.path().join("data")).unwrap();
        let counters = Counters::new();
        let directory = Directory::new(
            LinuxDirectory::open_existing(temporary.path()).unwrap(),
            Arc::clone(&counters),
        );
        let mut writer = directory.acquire_writer().unwrap();
        let (mut file, metadata) = writer
            .open_existing_with_metadata(&FileName::new("data").unwrap(), OpenMode::ReadOnly)
            .unwrap();
        assert_eq!(metadata.length, 5);
        assert_eq!(
            metadata.identity,
            FileIdentity {
                device: native.dev(),
                inode: native.ino()
            }
        );
        assert!(
            matches!(writer.open_existing_with_metadata(&FileName::new("missing").unwrap(), OpenMode::ReadOnly), Err(error) if error.kind == NamespaceErrorKind::NotFound)
        );
        let actual = counters.snapshot();
        let mut expected = Counters::new().snapshot();
        expected.open.attempts = 2;
        expected.open.successes = 1;
        expected.open.failures = 1;
        assert_eq!(actual, expected);
        drop(writer);
        assert!(
            matches!(directory.acquire_writer(), Err(error) if error.kind == NamespaceErrorKind::Busy)
        );
        let mut bytes = [0; 5];
        read_exact_at(&mut file, &mut bytes, 0).unwrap();
        assert_eq!(bytes, *b"value");
        drop(file);
        drop(directory.acquire_writer().unwrap());
    }
}
