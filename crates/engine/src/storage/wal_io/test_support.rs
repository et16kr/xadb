//! Deterministic namespace effects and independent raw frame construction.
use super::*;
use foundation::{
    io::FileIo,
    namespace::{DirectoryEntry, FileMetadata, NamespaceErrorKind, NamespaceResult},
};
use std::{collections::BTreeMap, io, sync::Arc};

pub(super) const PROFILE: &[u8] =
    include_bytes!("../../../tests/fixtures/dev3-genesis-profile.bin");
pub(super) const EPOCH: &[u8] = include_bytes!("../../../tests/fixtures/dev3-genesis-epoch.bin");
pub(super) const SELECTED: &[u8] =
    include_bytes!("../../../tests/fixtures/dev3-genesis-writer.bin");

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
        }
    }
}
pub(super) struct Writer(pub Arc<Mutex<State>>);
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
    Ok(Writer(Arc::clone(state)))
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
fn read_event(state: &mut State) -> io::Result<usize> {
    let at = state.reads;
    state.reads += 1;
    if state.read_fail == Some(at) {
        state.hit += 1;
        return Err(fail());
    }
    Ok(at)
}
impl FileIo for File {
    fn read_at(&mut self, output: &mut [u8], offset: u64) -> io::Result<usize> {
        let mut s = self.state.lock().unwrap();
        read_event(&mut s)?;
        let entry = &s.files[&self.name];
        let count = entry.length.saturating_sub(offset).min(output.len() as u64) as usize;
        output[..count].fill(0);
        for (i, byte) in output[..count].iter_mut().enumerate() {
            let position = offset + i as u64;
            if position < entry.bytes.len() as u64 {
                *byte = entry.bytes[position as usize];
            }
            for (&start, data) in &entry.sparse_tail {
                if position >= start && position - start < data.len() as u64 {
                    *byte = data[(position - start) as usize];
                }
            }
        }
        Ok(count)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let (at, action) = begin(&self.state, "write", &self.name, offset, bytes.len())?;
        self.state.lock().unwrap().events[at].data = bytes.to_vec();
        let count = match action {
            Some(Action::Interrupted) => return Err(io::ErrorKind::Interrupted.into()),
            Some(Action::Zero) => return Ok(0),
            Some(Action::InvalidCount) => return Ok(bytes.len() + 1),
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
            if action == Some(Action::PrefixError) {
                s.plan = Some(Plan {
                    at: at + 1,
                    action: Action::BeforeError,
                });
            }
        }
        finish(&self.state, at, action)?;
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
        let metadata = {
            let mut s = self.0.lock().unwrap();
            let at = read_event(&mut s).map_err(|_| ns_error(Effect::Unchanged))?;
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
            m
        };
        // Keep metadata detached from a later path query, just like the real port.
        let file = self.file(name)?;
        Ok((file, metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        let mut s = self.0.lock().unwrap();
        read_event(&mut s).map_err(|_| ns_error(Effect::Unchanged))?;
        Ok(s.files.get(name.as_str()).map(Entry::metadata))
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
        assert_eq!(selected, Selector::Writer);
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
