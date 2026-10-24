//! New plan4 test adapter. Fault keys name the actual operation, not a prior run.
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileIdentity, FileMetadata, FileName, NamespaceError,
        NamespaceErrorKind, NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::storage) struct Entry {
    pub(in crate::storage) bytes: Vec<u8>,
    pub(in crate::storage) length: u64,
    pub(in crate::storage) identity: FileIdentity,
}
impl Entry {
    pub(in crate::storage) fn metadata(&self) -> FileMetadata {
        FileMetadata {
            identity: self.identity,
            length: self.length,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::storage) struct Event {
    pub(in crate::storage) op: &'static str,
    pub(in crate::storage) name: String,
    pub(in crate::storage) offset: u64,
    pub(in crate::storage) requested: usize,
    pub(in crate::storage) returned: Option<usize>,
    /// Bytes placed before an injected Err; this is not a successful return count.
    pub(in crate::storage) error_prefix: usize,
    pub(in crate::storage) completed: bool,
}
#[derive(Clone, Debug)]
pub(in crate::storage) struct Key {
    pub(in crate::storage) op: &'static str,
    pub(in crate::storage) name: String,
    pub(in crate::storage) offset: u64,
    pub(in crate::storage) occurrence: usize,
}
impl Key {
    pub(in crate::storage) fn new(
        op: &'static str,
        name: &str,
        offset: u64,
        occurrence: usize,
    ) -> Self {
        Self {
            op,
            name: name.into(),
            offset,
            occurrence,
        }
    }
    fn matches(&self, events: &[Event], event: &Event) -> bool {
        self.op == event.op
            && self.name == event.name
            && self.offset == event.offset
            && events
                .iter()
                .filter(|e| e.op == self.op && e.name == self.name && e.offset == self.offset)
                .count()
                + 1
                == self.occurrence
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum Action {
    BeforeError,
    AfterError,
    BeforePanic,
    AfterPanic,
    Short(usize),
    Interrupted,
    Zero,
    InvalidCount,
    PrefixError,
    PrefixEof,
    OpenedIdentity,
}
#[derive(Clone, Debug)]
pub(in crate::storage) struct Fault {
    pub(in crate::storage) key: Key,
    pub(in crate::storage) action: Action,
}
#[derive(Clone, Debug)]
pub(in crate::storage) enum Edit {
    Bytes(String, Vec<u8>),
    Identity(String),
    Length(String, i64),
    Remove(String),
}
#[derive(Clone, Debug)]
pub(in crate::storage) struct Hook {
    pub(in crate::storage) key: Key,
    pub(in crate::storage) edit: Edit,
}

pub(in crate::storage) struct State {
    pub(in crate::storage) files: BTreeMap<String, Entry>,
    pub(in crate::storage) events: Vec<Event>,
    pub(in crate::storage) handles: usize,
    pub(in crate::storage) writers: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) entropy: [u8; 16],
    pub(in crate::storage) entropy_calls: usize,
    pub(in crate::storage) acquire_calls: usize,
    pub(in crate::storage) fault: Option<Fault>,
    pub(in crate::storage) hook: Option<Hook>,
    pub(in crate::storage) fault_hits: usize,
    pub(in crate::storage) hook_hits: usize,
    pub(in crate::storage) lock_creates: usize,
    pub(in crate::storage) acquisition_error: Option<NamespaceError>,
    pub(in crate::storage) acquisition_error_after_lock: bool,
    /// Opt-in only for new live tests; old genesis fixtures retain their restriction.
    pub(in crate::storage) live_writes: bool,
    /// New recovery fixtures alone admit their finite publication/epoch files.
    pub(in crate::storage) recovery_writes: bool,
    next_inode: u64,
    continuation: Option<(String, u64)>,
}
impl State {
    pub(in crate::storage) fn images(&self) -> BTreeMap<String, Vec<u8>> {
        self.files
            .iter()
            .map(|(name, entry)| {
                assert_eq!(entry.length, entry.bytes.len() as u64);
                (name.clone(), entry.bytes.clone())
            })
            .collect()
    }
    pub(in crate::storage) fn mutations(&self) -> usize {
        self.events
            .iter()
            .filter(|e| {
                matches!(
                    e.op,
                    "create" | "write" | "sync" | "dirsync" | "publish" | "resize" | "remove"
                )
            })
            .count()
    }
}
#[derive(Clone)]
pub(in crate::storage) struct Directory(pub(in crate::storage) Arc<Mutex<State>>);
pub(in crate::storage) struct Writer(pub(in crate::storage) Arc<Mutex<State>>);
pub(in crate::storage) struct File {
    state: Arc<Mutex<State>>,
    name: String,
}
impl Directory {
    pub(in crate::storage) fn from_files(files: BTreeMap<String, Vec<u8>>) -> Self {
        let files: BTreeMap<_, _> = files
            .into_iter()
            .enumerate()
            .map(|(i, (name, bytes))| {
                (
                    name,
                    Entry {
                        length: bytes.len() as u64,
                        bytes,
                        identity: FileIdentity {
                            device: 7,
                            inode: i as u64 + 1,
                        },
                    },
                )
            })
            .collect();
        let next_inode = files.len() as u64 + 1;
        Self(Arc::new(Mutex::new(State {
            files,
            events: Vec::new(),
            handles: 0,
            writers: 0,
            peak: 0,
            entropy: std::array::from_fn(|i| i as u8 + 1),
            entropy_calls: 0,
            acquire_calls: 0,
            fault: None,
            hook: None,
            fault_hits: 0,
            hook_hits: 0,
            lock_creates: 0,
            acquisition_error: None,
            acquisition_error_after_lock: false,
            live_writes: false,
            recovery_writes: false,
            next_inode,
            continuation: None,
        })))
    }
    pub(in crate::storage) fn fresh() -> Self {
        Self::from_files(BTreeMap::new())
    }
    pub(in crate::storage) fn state(&self) -> Arc<Mutex<State>> {
        Arc::clone(&self.0)
    }
}
fn io_error() -> io::Error {
    io::Error::other("plan4 exact injected fault")
}
fn ns_error(effect: Effect) -> NamespaceError {
    NamespaceError::io(
        NamespaceErrorKind::Io,
        effect,
        "plan4 exact injected fault",
        io_error(),
    )
}
fn begin(
    state: &Arc<Mutex<State>>,
    op: &'static str,
    name: &str,
    offset: u64,
    requested: usize,
) -> (usize, Option<Action>) {
    let mut s = state.lock().unwrap();
    let event = Event {
        op,
        name: name.into(),
        offset,
        requested,
        returned: None,
        error_prefix: 0,
        completed: false,
    };
    if s.hook
        .as_ref()
        .is_some_and(|h| h.key.matches(&s.events, &event))
    {
        let hook = s.hook.take().unwrap();
        s.hook_hits += 1;
        match hook.edit {
            Edit::Bytes(name, bytes) => {
                let entry = s.files.get_mut(&name).unwrap();
                entry.length = bytes.len() as u64;
                entry.bytes = bytes;
            }
            Edit::Identity(name) => {
                s.files.get_mut(&name).unwrap().identity.inode += 10_000;
            }
            Edit::Length(name, delta) => {
                let entry = s.files.get_mut(&name).unwrap();
                entry.length = entry.length.checked_add_signed(delta).unwrap();
            }
            Edit::Remove(name) => {
                s.files.remove(&name).unwrap();
            }
        }
    }
    let action = if s
        .fault
        .as_ref()
        .is_some_and(|f| f.key.matches(&s.events, &event))
    {
        s.fault_hits += 1;
        Some(s.fault.take().unwrap().action)
    } else {
        None
    };
    let index = s.events.len();
    s.events.push(event);
    (index, action)
}
fn before(action: Option<Action>) -> io::Result<()> {
    match action {
        Some(Action::BeforeError) => Err(io_error()),
        Some(Action::BeforePanic) => panic!("plan4 exact before panic"),
        Some(Action::Interrupted) => Err(io::ErrorKind::Interrupted.into()),
        _ => Ok(()),
    }
}
fn after(action: Option<Action>) -> io::Result<()> {
    match action {
        Some(Action::AfterError) => Err(io_error()),
        Some(Action::AfterPanic) => panic!("plan4 exact after panic"),
        _ => Ok(()),
    }
}
fn complete(state: &Arc<Mutex<State>>, index: usize, returned: Option<usize>) {
    let mut s = state.lock().unwrap();
    s.events[index].completed = true;
    s.events[index].returned = returned;
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        let mut s = self.0.lock().unwrap();
        s.acquire_calls += 1;
        if !s.acquisition_error_after_lock
            && let Some(error) = s.acquisition_error.take()
        {
            return Err(error);
        }
        if s.writers != 0 || s.handles != 0 {
            return Err(NamespaceError::new(
                NamespaceErrorKind::Busy,
                Effect::Unchanged,
                "plan4 writer busy",
            ));
        }
        if !s.files.contains_key("LOCK") {
            let inode = s.next_inode;
            s.next_inode += 1;
            s.files.insert(
                "LOCK".into(),
                Entry {
                    bytes: Vec::new(),
                    length: 0,
                    identity: FileIdentity { device: 7, inode },
                },
            );
            s.lock_creates += 1;
        }
        if s.acquisition_error_after_lock
            && let Some(error) = s.acquisition_error.take()
        {
            return Err(error);
        }
        s.writers += 1;
        Ok(Writer(Arc::clone(&self.0)))
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.0.lock().unwrap().writers -= 1;
    }
}
impl Drop for File {
    fn drop(&mut self) {
        self.state.lock().unwrap().handles -= 1;
    }
}
impl Writer {
    fn file(&self, name: &FileName) -> NamespaceResult<File> {
        let mut s = self.0.lock().unwrap();
        if !s.files.contains_key(name.as_str()) {
            return Err(NamespaceError::new(
                NamespaceErrorKind::NotFound,
                Effect::Unchanged,
                "plan4 missing file",
            ));
        }
        s.handles += 1;
        s.peak = s.peak.max(s.handles);
        Ok(File {
            state: Arc::clone(&self.0),
            name: name.as_str().into(),
        })
    }
}
impl FileIo for File {
    fn read_at(&mut self, out: &mut [u8], offset: u64) -> io::Result<usize> {
        let (index, action) = begin(&self.state, "read", &self.name, offset, out.len());
        before(action)?;
        {
            let mut s = self.state.lock().unwrap();
            if s.continuation
                .as_ref()
                .is_some_and(|(name, next)| name == &self.name && *next == offset)
            {
                s.continuation = None;
                complete_unlocked(&mut s, index, Some(0));
                return Ok(0);
            }
        }
        if action == Some(Action::Zero) {
            complete(&self.state, index, Some(0));
            return Ok(0);
        }
        if action == Some(Action::InvalidCount) {
            complete(&self.state, index, Some(out.len() + 1));
            return Ok(out.len() + 1);
        }
        let cap = match action {
            Some(Action::Short(n)) => n,
            Some(Action::PrefixError | Action::PrefixEof) => 17,
            _ => out.len(),
        };
        let count = {
            let mut s = self.state.lock().unwrap();
            let e = s.files.get(&self.name).unwrap();
            let start = offset as usize;
            let count = e.bytes.len().saturating_sub(start).min(out.len()).min(cap);
            if count > 0 {
                out[..count].copy_from_slice(&e.bytes[start..start + count]);
            }
            if action == Some(Action::PrefixEof) {
                s.continuation = Some((self.name.clone(), offset + count as u64));
            }
            count
        };
        complete(&self.state, index, Some(count));
        if action == Some(Action::PrefixError) {
            let mut state = self.state.lock().unwrap();
            state.events[index].returned = None;
            state.events[index].error_prefix = count;
            return Err(io_error());
        }
        after(action)?;
        Ok(count)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let (index, action) = begin(&self.state, "write", &self.name, offset, bytes.len());
        before(action)?;
        let count = match action {
            Some(Action::Zero) => 0,
            Some(Action::InvalidCount) => {
                complete(&self.state, index, Some(bytes.len() + 1));
                return Ok(bytes.len() + 1);
            }
            Some(Action::Short(n)) => n.min(bytes.len()),
            Some(Action::PrefixError) => 8192.min(bytes.len()),
            _ => bytes.len(),
        };
        if count > 0 {
            let mut s = self.state.lock().unwrap();
            let e = s.files.get_mut(&self.name).unwrap();
            let end = offset as usize + count;
            if end > e.bytes.len() {
                e.bytes.resize(end, 0);
            }
            e.bytes[offset as usize..end].copy_from_slice(&bytes[..count]);
            e.length = e.bytes.len() as u64;
        }
        complete(&self.state, index, Some(count));
        if action == Some(Action::PrefixError) {
            let mut state = self.state.lock().unwrap();
            state.events[index].returned = None;
            state.events[index].error_prefix = count;
            return Err(io_error());
        }
        after(action)?;
        Ok(count)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let (i, a) = begin(&self.state, "sync", &self.name, 0, 0);
        before(a)?;
        complete(&self.state, i, None);
        after(a)
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        panic!("plan4 unexpectedly resized file")
    }
}
fn complete_unlocked(s: &mut State, index: usize, returned: Option<usize>) {
    s.events[index].completed = true;
    s.events[index].returned = returned;
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        let mut s = self.0.lock().unwrap();
        s.entropy_calls += 1;
        Ok(s.entropy)
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let (i, a) = begin(&self.0, "create", name.as_str(), 0, 0);
        before(a).map_err(|_| ns_error(Effect::Unchanged))?;
        {
            let mut s = self.0.lock().unwrap();
            if s.files.contains_key(name.as_str()) {
                return Err(NamespaceError::new(
                    NamespaceErrorKind::AlreadyExists,
                    Effect::Unchanged,
                    "plan4 duplicate file",
                ));
            }
            let inode = s.next_inode;
            s.next_inode += 1;
            s.files.insert(
                name.as_str().into(),
                Entry {
                    bytes: Vec::new(),
                    length: 0,
                    identity: FileIdentity { device: 7, inode },
                },
            );
        }
        complete(&self.0, i, None);
        after(a).map_err(|_| ns_error(Effect::CallerMustResolve))?;
        self.file(name)
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        self.open_existing_with_metadata(name, mode)
            .map(|(file, _)| file)
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        // The original genesis mode keeps data/control opens read-only.
        // Live tests opt in to the exact DWB/home write inventory.
        let (live_writes, recovery_writes) = {
            let state = self.0.lock().unwrap();
            (state.live_writes, state.recovery_writes)
        };
        assert!(
            mode == OpenMode::ReadOnly
                || (mode == OpenMode::ReadWrite
                    && (name.as_str() == crate::storage::wal_io::WAL_NAME
                        || (live_writes
                            && matches!(
                                name.as_str(),
                                "d3-data-0000000100000001"
                                    | "d3-dwb-images"
                                    | "d3-dwb-stage"
                                    | "d3-dwb-0"
                                    | "d3-dwb-1"
                            ))
                        || (recovery_writes
                            && matches!(
                                name.as_str(),
                                "d3-current-stage"
                                    | "d3-writer-stage"
                                    | "d3-dwb-select-stage"
                                    | "d3-rebase-0"
                                    | "d3-rebase-1"
                                    | "d3-wal-00000002"
                                    | "d3-wal-00000003"
                            ))))
        );
        let (i, a) = begin(&self.0, "open", name.as_str(), 0, 0);
        before(a).map_err(|_| ns_error(Effect::Unchanged))?;
        let file = self.file(name)?;
        let mut metadata = self.0.lock().unwrap().files[name.as_str()].metadata();
        if a == Some(Action::OpenedIdentity) {
            metadata.identity.inode += 10_000;
        }
        complete(&self.0, i, None);
        after(a).map_err(|_| ns_error(Effect::Unchanged))?;
        Ok((file, metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        let (i, a) = begin(&self.0, "metadata", name.as_str(), 0, 0);
        before(a).map_err(|_| ns_error(Effect::Unchanged))?;
        let value = self
            .0
            .lock()
            .unwrap()
            .files
            .get(name.as_str())
            .map(Entry::metadata);
        complete(&self.0, i, None);
        after(a).map_err(|_| ns_error(Effect::Unchanged))?;
        Ok(value)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        let (i, a) = begin(&self.0, "list", "", 0, limit);
        before(a).map_err(|_| ns_error(Effect::Unchanged))?;
        let entries = {
            let s = self.0.lock().unwrap();
            if s.files.len() > limit {
                return Err(NamespaceError::new(
                    NamespaceErrorKind::LimitExceeded,
                    Effect::Unchanged,
                    "plan4 inventory cap",
                ));
            }
            s.files
                .iter()
                .map(|(name, e)| DirectoryEntry {
                    name: FileName::new(name).unwrap(),
                    metadata: e.metadata(),
                })
                .collect()
        };
        complete(&self.0, i, None);
        after(a).map_err(|_| ns_error(Effect::Unchanged))?;
        Ok(entries)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        let (i, a) = begin(&self.0, "dirsync", "", 0, 0);
        before(a).map_err(|_| ns_error(Effect::CallerMustResolve))?;
        complete(&self.0, i, None);
        after(a).map_err(|_| ns_error(Effect::CallerMustResolve))
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        let (i, a) = begin(&self.0, "publish", temporary.as_str(), 0, 0);
        before(a).map_err(|_| ns_error(Effect::Unchanged))?;
        {
            let mut s = self.0.lock().unwrap();
            let entry = s
                .files
                .remove(temporary.as_str())
                .expect("prepared temporary selector");
            s.files.insert(selected.as_str().into(), entry);
        }
        complete(&self.0, i, None);
        after(a).map_err(|_| ns_error(Effect::CallerMustResolve))
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        assert!(
            self.0.lock().unwrap().recovery_writes
                && matches!(
                    name.as_str(),
                    "d3-wal-00000001" | "d3-wal-00000002" | "d3-wal-00000003"
                ),
            "plan4 unexpectedly retired file"
        );
        let (i, action) = begin(&self.0, "remove", name.as_str(), 0, 0);
        before(action).map_err(|_| ns_error(Effect::Unchanged))?;
        let removed = self.0.lock().unwrap().files.remove(name.as_str());
        if removed.is_none() {
            return Err(NamespaceError::new(
                NamespaceErrorKind::NotFound,
                Effect::Unchanged,
                "plan4 exact retired file absent",
            ));
        }
        complete(&self.0, i, None);
        after(action).map_err(|_| ns_error(Effect::CallerMustResolve))
    }
}

/// Actual oversized negative adapter; its embedded bytes count in size_of<W>.
pub(in crate::storage) struct SizedWriter<const N: usize> {
    pub(in crate::storage) inner: Writer,
    pub(in crate::storage) padding: [u8; N],
}
impl<const N: usize> WriterNamespace for SizedWriter<N> {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.inner.random_incarnation()
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        self.inner.create_new(name)
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        self.inner.open_existing(name, mode)
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        self.inner.open_existing_with_metadata(name, mode)
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.inner.sync_directory()
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        self.inner.publish_selector(temporary, selected)
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        self.inner.remove_retired(name)
    }
}
