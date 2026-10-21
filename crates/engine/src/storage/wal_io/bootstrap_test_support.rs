//! Deterministic fresh-storage effects. Expected bytes come from frozen literals.
use super::super::{FileIdentity, MAX_FRAME_BYTES};
use crate::api::{DatabaseId, Namespace};
use std::{
    cell::Cell,
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, FileMetadata, FileName, NamespaceError, NamespaceErrorKind,
        NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};

pub(super) fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
pub(super) fn raw_frame(previous: u64, lsn: u64, payload: &[u8]) -> Vec<u8> {
    // Contract offsets only; do not call the product frame encoder/decoder.
    let mut bytes = vec![0; 192 + payload.len()];
    bytes[..4].copy_from_slice(b"XWF1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6] = 3;
    bytes[8..10].copy_from_slice(&160u16.to_le_bytes());
    let length = bytes.len() as u32;
    bytes[12..16].copy_from_slice(&length.to_le_bytes());
    bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes[24..40].copy_from_slice(&namespace().database.bytes());
    bytes[40..48].copy_from_slice(&1u64.to_le_bytes());
    bytes[48..52].copy_from_slice(&1u32.to_le_bytes());
    bytes[56..64].copy_from_slice(&lsn.to_le_bytes());
    bytes[64..72].copy_from_slice(&previous.to_le_bytes());
    bytes[160..160 + payload.len()].copy_from_slice(payload);
    seal_frame(&mut bytes);
    bytes
}
pub(super) fn seal_frame(bytes: &mut [u8]) {
    let header = content_digest(&bytes[..128]);
    bytes[128..160].copy_from_slice(&header);
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
pub(super) fn seal_control(bytes: &mut [u8]) {
    let digest = content_digest(&bytes[..4064]);
    bytes[4064..].copy_from_slice(&digest);
}
pub(super) fn slot() -> Box<[u8; MAX_FRAME_BYTES]> {
    vec![0; MAX_FRAME_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
pub(super) fn fail() -> io::Error {
    io::Error::other("fixed WAL fault")
}
fn ns_error(effect: Effect) -> NamespaceError {
    NamespaceError::io(
        NamespaceErrorKind::Io,
        effect,
        "fixed WAL namespace fault",
        fail(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    PrefixEof,
    BeforeError,
    AfterError,
    BeforePanic,
    AfterPanic,
    Short(usize),
    Interrupted,
    Zero,
    InvalidCount,
    PrefixError,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Plan {
    pub at: usize,
    pub action: Action,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub kind: &'static str,
    pub name: String,
    pub offset: u64,
    pub length: usize,
    pub data: Vec<u8>,
    pub returned: Option<usize>,
}
#[derive(Clone)]
pub(super) struct Entry {
    pub bytes: Vec<u8>,
    pub length: u64,
    pub identity: FileIdentity,
    // Used only by explicitly synthetic cap arithmetic cases, never recovery.
    pub sparse_tail: BTreeMap<u64, Vec<u8>>,
}
impl Entry {
    pub(super) fn new(inode: u64) -> Self {
        Self {
            bytes: Vec::new(),
            length: 0,
            identity: FileIdentity { device: 7, inode },
            sparse_tail: BTreeMap::new(),
        }
    }
    pub fn metadata(&self) -> FileMetadata {
        FileMetadata {
            identity: self.identity,
            length: self.length,
        }
    }
}
pub(super) struct Pause {
    pub at: usize,
    pub after: bool,
    pub entered: std::sync::mpsc::SyncSender<()>,
    pub release: std::sync::mpsc::Receiver<()>,
    pub unwind: bool,
}
pub(super) struct State {
    pub files: BTreeMap<String, Entry>,
    pub writers: usize,
    pub handles: usize,
    pub peak: usize,
    pub next_inode: u64,
    pub events: Vec<Event>,
    pub plan: Option<Plan>,
    pub hit: usize,
    pub pause: Option<Pause>,
    pub entropy_zero: bool,
    pub entropy_error: bool,
    pub reads: usize,
    pub read_fail: Option<usize>,
    pub metadata_change: Option<(usize, bool)>,
    pub read_events: Vec<Event>,
    pub read_plan: Option<Plan>,
    pub read_pause: Option<Pause>,
    pub drop_pause: Option<Pause>,
    pub read_edits: Vec<(usize, String, Vec<u8>)>,
    pub length_delta: Option<(usize, i64)>,
}
impl State {
    fn new() -> Self {
        Self {
            files: BTreeMap::from([("LOCK".into(), Entry::new(1))]),
            writers: 0,
            handles: 0,
            peak: 0,
            next_inode: 2,
            events: Vec::new(),
            plan: None,
            hit: 0,
            pause: None,
            entropy_zero: false,
            entropy_error: false,
            reads: 0,
            read_fail: None,
            metadata_change: None,
            read_events: Vec::new(),
            read_plan: None,
            read_pause: None,
            drop_pause: None,
            read_edits: Vec::new(),
            length_delta: None,
        }
    }
}
pub(super) struct Writer(pub Arc<Mutex<State>>, pub Cell<()>);
pub(super) struct File {
    state: Arc<Mutex<State>>,
    name: String,
}
pub(super) fn acquire(state: &Arc<Mutex<State>>) -> NamespaceResult<Writer> {
    let mut s = state.lock().unwrap();
    if s.writers != 0 || s.handles != 0 {
        return Err(NamespaceError::new(
            NamespaceErrorKind::Busy,
            Effect::Unchanged,
            "fake lease busy",
        ));
    }
    s.writers += 1;
    Ok(Writer(Arc::clone(state), Cell::new(())))
}
pub(super) fn fresh() -> (Writer, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::new()));
    (acquire(&state).unwrap(), state)
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.0.lock().unwrap().writers -= 1;
    }
}
impl Drop for File {
    fn drop(&mut self) {
        let pause = if self.name == "d3-filemap-0" {
            self.state.lock().unwrap().drop_pause.take()
        } else {
            None
        };
        if let Some(p) = pause {
            p.entered.send(()).unwrap();
            p.release
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        }
        self.state.lock().unwrap().handles -= 1;
    }
}
fn pause(state: &Arc<Mutex<State>>, at: usize, after: bool) {
    let pause = {
        let mut s = state.lock().unwrap();
        if s.pause
            .as_ref()
            .is_some_and(|p| p.at == at && p.after == after)
        {
            s.pause.take()
        } else {
            None
        }
    };
    if let Some(p) = pause {
        p.entered.send(()).unwrap();
        p.release
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!p.unwind, "fixed paused WAL unwind");
    }
}
fn begin(
    state: &Arc<Mutex<State>>,
    kind: &'static str,
    name: &str,
    offset: u64,
    length: usize,
) -> io::Result<(usize, Option<Action>)> {
    let (at, action) = {
        let mut s = state.lock().unwrap();
        let at = s.events.len();
        s.events.push(Event {
            kind,
            name: name.into(),
            offset,
            length,
            data: Vec::new(),
            returned: None,
        });
        let action = s.plan.filter(|p| p.at == at).map(|p| p.action);
        if action.is_some() {
            s.hit += 1;
        }
        (at, action)
    };
    pause(state, at, false);
    match action {
        Some(Action::BeforeError) => Err(fail()),
        Some(Action::BeforePanic) => panic!("fixed before-effect WAL unwind"),
        _ => Ok((at, action)),
    }
}
fn finish(state: &Arc<Mutex<State>>, at: usize, action: Option<Action>) -> io::Result<()> {
    pause(state, at, true);
    match action {
        Some(Action::AfterError) => Err(fail()),
        Some(Action::AfterPanic) => panic!("fixed after-effect WAL unwind"),
        _ => Ok(()),
    }
}
fn read_begin(
    state: &Arc<Mutex<State>>,
    kind: &'static str,
    name: &str,
    offset: u64,
    length: usize,
) -> io::Result<(usize, Option<Action>)> {
    let (at, action, pause) = {
        let mut s = state.lock().unwrap();
        let at = s.reads;
        s.reads += 1;
        s.read_events.push(Event {
            kind,
            name: name.into(),
            offset,
            length,
            data: Vec::new(),
            returned: None,
        });
        let mut i = 0;
        while i < s.read_edits.len() {
            if s.read_edits[i].0 == at {
                let (_, name, bytes) = s.read_edits.remove(i);
                let entry = s.files.get_mut(&name).unwrap();
                entry.length = bytes.len() as u64;
                entry.bytes = bytes;
            } else {
                i += 1;
            }
        }
        let action = s.read_plan.filter(|p| p.at == at).map(|p| p.action);
        if action.is_some() {
            s.hit += 1;
        }
        let pause = if s.read_pause.as_ref().is_some_and(|p| p.at == at) {
            s.read_pause.take()
        } else {
            None
        };
        (at, action, pause)
    };
    if let Some(p) = pause {
        p.entered.send(()).unwrap();
        p.release
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!p.unwind, "fixed storage read unwind");
    }
    match action {
        Some(Action::BeforeError) => Err(fail()),
        Some(Action::BeforePanic) => panic!("fixed before-read unwind"),
        _ => Ok((at, action)),
    }
}
fn read_finish(action: Option<Action>) -> io::Result<()> {
    match action {
        Some(Action::AfterPanic) => panic!("fixed after-read unwind"),
        Some(Action::AfterError) => Err(fail()),
        _ => Ok(()),
    }
}
impl FileIo for File {
    fn read_at(&mut self, output: &mut [u8], offset: u64) -> io::Result<usize> {
        let (at, action) = read_begin(&self.state, "read", &self.name, offset, output.len())?;
        let count = {
            let mut s = self.state.lock().unwrap();
            let cap = match action {
                Some(Action::Interrupted) => return Err(io::ErrorKind::Interrupted.into()),
                Some(Action::Zero) => {
                    s.read_events[at].returned = Some(0);
                    return Ok(0);
                }
                Some(Action::InvalidCount) => {
                    s.read_events[at].returned = Some(output.len() + 1);
                    return Ok(output.len() + 1);
                }
                Some(Action::Short(n)) => n,
                Some(Action::PrefixError | Action::PrefixEof) => output.len() / 2,
                _ => output.len(),
            };
            let entry = &s.files[&self.name];
            let count = entry
                .length
                .saturating_sub(offset)
                .min(output.len() as u64)
                .min(cap as u64) as usize;
            output[..count].copy_from_slice(&entry.bytes[offset as usize..offset as usize + count]);
            s.read_events[at].data = output[..count].to_vec();
            if matches!(action, Some(Action::PrefixError | Action::PrefixEof)) {
                s.read_plan = Some(Plan {
                    at: at + 1,
                    action: if action == Some(Action::PrefixEof) {
                        Action::Zero
                    } else {
                        Action::BeforeError
                    },
                });
            }
            count
        };
        read_finish(action)?;
        self.state.lock().unwrap().read_events[at].returned = Some(count);
        Ok(count)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let (at, action) = begin(&self.state, "write", &self.name, offset, bytes.len())?;
        self.state.lock().unwrap().events[at].data = bytes.to_vec();
        let count = match action {
            Some(Action::Interrupted) => return Err(io::ErrorKind::Interrupted.into()),
            Some(Action::Zero) => {
                self.state.lock().unwrap().events[at].returned = Some(0);
                return Ok(0);
            }
            Some(Action::InvalidCount) => {
                self.state.lock().unwrap().events[at].returned = Some(bytes.len() + 1);
                return Ok(bytes.len() + 1);
            }
            Some(Action::Short(n)) => n.min(bytes.len()),
            Some(Action::PrefixError) => bytes.len() / 2,
            _ => bytes.len(),
        };
        {
            let mut s = self.state.lock().unwrap();
            let entry = s.files.get_mut(&self.name).unwrap();
            if offset <= entry.bytes.len() as u64 {
                let start = offset as usize;
                entry.bytes.resize(entry.bytes.len().max(start + count), 0);
                entry.bytes[start..start + count].copy_from_slice(&bytes[..count]);
            } else {
                entry.sparse_tail.insert(offset, bytes[..count].to_vec());
            }
            entry.length = entry.length.max(offset + count as u64);
        }
        if action == Some(Action::PrefixError) {
            return Err(fail());
        }
        finish(&self.state, at, action)?;
        self.state.lock().unwrap().events[at].returned = Some(count);
        Ok(count)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let (at, action) = begin(&self.state, "sync", &self.name, 0, 0)?;
        finish(&self.state, at, action)
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        panic!("unexpected WAL resize")
    }
}
impl Writer {
    fn file(&self, name: &FileName) -> NamespaceResult<File> {
        let mut s = self.0.lock().unwrap();
        if !s.files.contains_key(name.as_str()) {
            return Err(NamespaceError::new(
                NamespaceErrorKind::NotFound,
                Effect::Unchanged,
                "fake missing file",
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
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        let s = self.0.lock().unwrap();
        if s.entropy_error {
            return Err(ns_error(Effect::Unchanged));
        }
        Ok(if s.entropy_zero {
            [0; 16]
        } else {
            namespace().database.bytes()
        })
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let (at, action) = begin(&self.0, "create", name.as_str(), 0, 0)
            .map_err(|_| ns_error(Effect::Unchanged))?;
        {
            let mut s = self.0.lock().unwrap();
            if s.files.contains_key(name.as_str()) {
                return Err(NamespaceError::new(
                    NamespaceErrorKind::AlreadyExists,
                    Effect::Unchanged,
                    "fake create exists",
                ));
            }
            let id = s.next_inode;
            s.next_inode += 1;
            s.files.insert(name.as_str().into(), Entry::new(id));
        }
        finish(&self.0, at, action).map_err(|_| ns_error(Effect::CallerMustResolve))?;
        self.file(name)
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<File> {
        panic!("WAL must use handle-bound metadata")
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        _: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let (at, action) = read_begin(&self.0, "open", name.as_str(), 0, 0)
            .map_err(|_| ns_error(Effect::Unchanged))?;
        let metadata = {
            let s = self.0.lock().unwrap();
            let mut m = s
                .files
                .get(name.as_str())
                .ok_or_else(|| ns_error(Effect::Unchanged))?
                .metadata();
            if let Some((which, identity)) = s.metadata_change
                && which == at
            {
                if identity {
                    m.identity.inode += 100;
                } else {
                    m.length -= 1;
                }
            }
            if let Some((which, delta)) = s.length_delta
                && which == at
            {
                m.length = m.length.checked_add_signed(delta).unwrap();
            }
            m
        };
        let file = self.file(name)?;
        read_finish(action).map_err(|_| ns_error(Effect::Unchanged))?;
        Ok((file, metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        let (at, action) = read_begin(&self.0, "metadata", name.as_str(), 0, 0)
            .map_err(|_| ns_error(Effect::Unchanged))?;
        let metadata = {
            let s = self.0.lock().unwrap();
            let mut m = s.files.get(name.as_str()).map(Entry::metadata);
            if let Some((which, _)) = s.metadata_change
                && which == at
                && let Some(m) = &mut m
            {
                m.identity.inode += 100;
            }
            m
        };
        read_finish(action).map_err(|_| ns_error(Effect::Unchanged))?;
        Ok(metadata)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        let s = self.0.lock().unwrap();
        if s.files.len() > limit {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "fake complete inventory limit",
            ));
        }
        Ok(s.files
            .iter()
            .map(|(name, entry)| DirectoryEntry {
                name: FileName::new(name).unwrap(),
                metadata: entry.metadata(),
            })
            .collect())
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        let (at, action) =
            begin(&self.0, "dirsync", "", 0, 0).map_err(|_| ns_error(Effect::CallerMustResolve))?;
        finish(&self.0, at, action).map_err(|_| ns_error(Effect::CallerMustResolve))
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        assert!(matches!(selected, Selector::Writer | Selector::FileMap));
        let (at, action) = begin(&self.0, "publish", temporary.as_str(), 0, 0)
            .map_err(|_| ns_error(Effect::Unchanged))?;
        {
            let mut s = self.0.lock().unwrap();
            let file = s.files.remove(temporary.as_str()).unwrap();
            s.files.insert(selected.as_str().into(), file);
        }
        finish(&self.0, at, action).map_err(|_| ns_error(Effect::CallerMustResolve))
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        panic!("unexpected WAL cleanup")
    }
}

pub(super) struct SizedWriter<const OWNER: usize, const HANDLE: usize> {
    pub inner: Writer,
    pub padding: [u8; OWNER],
}
pub(super) struct SizedFile<const N: usize> {
    inner: File,
    padding: [u8; N],
}
impl<const N: usize> FileIo for SizedFile<N> {
    fn read_at(&mut self, b: &mut [u8], o: u64) -> io::Result<usize> {
        self.inner.read_at(b, o)
    }
    fn write_at(&mut self, b: &[u8], o: u64) -> io::Result<usize> {
        self.inner.write_at(b, o)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.inner.sync_all()
    }
    fn set_len(&mut self, n: u64) -> io::Result<()> {
        self.inner.set_len(n)
    }
}
impl<const O: usize, const F: usize> WriterNamespace for SizedWriter<O, F> {
    type File = SizedFile<F>;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.inner.random_incarnation()
    }
    fn create_new(&mut self, n: &FileName) -> NamespaceResult<Self::File> {
        self.inner.create_new(n).map(|inner| SizedFile {
            inner,
            padding: [0; F],
        })
    }
    fn open_existing(&mut self, n: &FileName, m: OpenMode) -> NamespaceResult<Self::File> {
        self.inner.open_existing(n, m).map(|inner| SizedFile {
            inner,
            padding: [0; F],
        })
    }
    fn open_existing_with_metadata(
        &mut self,
        n: &FileName,
        m: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        self.inner
            .open_existing_with_metadata(n, m)
            .map(|(inner, metadata)| {
                (
                    SizedFile {
                        inner,
                        padding: [0; F],
                    },
                    metadata,
                )
            })
    }
    fn metadata(&self, n: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(n)
    }
    fn list_owned(&self, n: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(n)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.inner.sync_directory()
    }
    fn publish_selector(&mut self, n: &FileName, s: Selector) -> NamespaceResult<()> {
        self.inner.publish_selector(n, s)
    }
    fn remove_retired(&mut self, n: &FileName) -> NamespaceResult<()> {
        self.inner.remove_retired(n)
    }
}
