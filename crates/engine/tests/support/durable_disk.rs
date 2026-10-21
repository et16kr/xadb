//! Explicit cache-loss model, not an OS/device qualification. File sync persists
//! one inode's bytes; directory sync persists names only. Old inodes survive
//! rename/unlink so an unsynced namespace change can revert after cache loss.
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use xadb_foundation::{
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileIdentity, FileMetadata, FileName, NamespaceError,
        NamespaceErrorKind as Kind, NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Create,
    Write,
    Sync,
    Rename,
    DirectorySync,
    Unlink,
    Resize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub operation: Operation,
    pub name: String,
    pub inode: u64,
    pub offset: u64,
    pub bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Before,
    After,
    PrefixError(usize),
    Short(usize),
}
#[derive(Clone, Debug)]
pub struct Fault {
    pub ordinal: usize,
    pub expected: Event,
    pub failure: Failure,
}
#[derive(Clone, Debug)]
pub struct Receipt {
    pub event: Event,
    pub applied: Option<Failure>,
    pub effect: bool,
    pub completed: usize,
}
#[derive(Clone, Default)]
struct Inode {
    bytes: Vec<u8>,
    synced: Vec<u8>,
}
#[derive(Clone)]
struct Image {
    names: BTreeMap<String, u64>,
    durable_names: BTreeMap<String, u64>,
    files: BTreeMap<u64, Inode>,
    next_inode: u64,
    entropy: u64,
}
struct Model {
    image: Image,
    leased: bool,
    receipts: Vec<Receipt>,
    fault: Option<Fault>,
    hit: bool,
    pause: Option<(String, Arc<PauseState>)>,
}
struct PauseState {
    state: Mutex<(Option<Event>, bool)>,
    changed: Condvar,
}
/// One real thread rendezvous at a matched file write. Dropping the controller
/// releases the worker even if a surrounding assertion unwinds.
pub struct Pause(Arc<PauseState>);
impl Pause {
    pub fn reached(&self) -> Event {
        let state = self.0.state.lock().unwrap();
        let (state, timeout) = self
            .0
            .changed
            .wait_timeout_while(state, Duration::from_secs(20), |state| state.0.is_none())
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "checkpoint did not reach its planned write"
        );
        state.0.clone().unwrap()
    }
    pub fn release(&self) {
        self.0.state.lock().unwrap().1 = true;
        self.0.changed.notify_all();
    }
}
impl Drop for Pause {
    fn drop(&mut self) {
        self.release();
    }
}
impl PauseState {
    fn wait(&self, event: Event) {
        let mut state = self.state.lock().unwrap();
        state.0 = Some(event);
        self.changed.notify_all();
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(30), |state| !state.1)
            .unwrap();
        drop(state);
        assert!(!timeout.timed_out(), "checkpoint write release deadline");
    }
}
const IMAGE_BYTES: usize = 256 * 1024 * 1024;
impl Image {
    fn admit(&self, extra: usize) {
        let used: usize = self
            .files
            .values()
            .map(|file| file.bytes.len() + file.synced.len())
            .sum();
        assert!(
            used.checked_add(extra)
                .is_some_and(|bytes| bytes <= IMAGE_BYTES),
            "cache-loss fixture byte budget exceeded"
        );
        assert!(
            self.files.len() <= 8192,
            "cache-loss fixture inode budget exceeded"
        );
    }
}
#[derive(Clone)]
pub struct Disk(Arc<Mutex<Model>>);
struct Lease(Arc<Mutex<Model>>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.lock().unwrap().leased = false;
    }
}
pub struct Writer {
    model: Arc<Mutex<Model>>,
    lease: Arc<Lease>,
}
pub struct File {
    model: Arc<Mutex<Model>>,
    _lease: Arc<Lease>,
    inode: u64,
    name: String,
    writable: bool,
}

fn namespace(kind: Kind, effect: Effect) -> NamespaceError {
    NamespaceError::new(kind, effect, "cache-loss fixture boundary")
}
fn failure() -> io::Error {
    io::Error::other("planned cache-loss fixture fault")
}
impl Disk {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(Arc::new(Mutex::new(Model {
            image: Image {
                // LOCK bootstrap is outside this model's cut profile. The real Linux
                // namespace fixture separately owns lock creation/exclusion checks.
                names: BTreeMap::from([("LOCK".into(), 1)]),
                durable_names: BTreeMap::from([("LOCK".into(), 1)]),
                files: BTreeMap::from([(1, Inode::default())]),
                next_inode: 2,
                entropy: NEXT.fetch_add(1, Ordering::Relaxed),
            },
            leased: false,
            receipts: Vec::new(),
            fault: None,
            hit: false,
            pause: None,
        })))
    }
    /// Copy the whole declared device state, preserving physical identities but
    /// neither writer capabilities nor the prior run's planned faults/receipts.
    pub fn fork(&self) -> Self {
        let model = self.0.lock().unwrap();
        assert!(
            !model.leased,
            "fork requires all process capabilities closed"
        );
        Self(Arc::new(Mutex::new(Model {
            image: model.image.clone(),
            leased: false,
            receipts: Vec::new(),
            fault: None,
            hit: false,
            pause: None,
        })))
    }
    pub fn pause_write(&self, prefix: &str) -> Pause {
        let gate = Arc::new(PauseState {
            state: Mutex::new((None, false)),
            changed: Condvar::new(),
        });
        let mut model = self.0.lock().unwrap();
        assert!(model.pause.is_none());
        model.pause = Some((prefix.into(), Arc::clone(&gate)));
        Pause(gate)
    }
    pub fn reset_trace(&self) {
        let mut model = self.0.lock().unwrap();
        model.receipts.clear();
        model.fault = None;
        model.hit = false;
    }
    pub fn inject(&self, fault: Fault) {
        assert!(fault.ordinal > 0);
        let mut model = self.0.lock().unwrap();
        model.receipts.clear();
        model.fault = Some(fault);
        model.hit = false;
    }
    pub fn receipts(&self) -> Vec<Receipt> {
        self.0.lock().unwrap().receipts.clone()
    }
    pub fn assert_reached(&self) {
        let model = self.0.lock().unwrap();
        assert!(
            model.fault.is_some() && model.hit,
            "planned cut was never reached: {:?}",
            model.fault
        );
        assert!(model.receipts.iter().any(|r| r.applied.is_some()));
    }
    pub fn clear_fault(&self) {
        self.0.lock().unwrap().fault = None;
    }
    pub fn power_loss(&self) {
        let mut model = self.0.lock().unwrap();
        assert!(
            !model.leased,
            "crashed process must drop writer and every file capability first"
        );
        model.image.names = model.image.durable_names.clone();
        for inode in model.image.files.values_mut() {
            inode.bytes.clone_from(&inode.synced);
        }
        model.fault = None;
    }
    pub fn names(&self) -> Vec<String> {
        self.0.lock().unwrap().image.names.keys().cloned().collect()
    }
    pub fn bytes(&self, name: &str) -> Vec<u8> {
        let model = self.0.lock().unwrap();
        model.image.files[&model.image.names[name]].bytes.clone()
    }
    /// External corruption is an explicit fixture effect on both cache layers.
    pub fn damage(&self, name: &str, at: usize) {
        let mut model = self.0.lock().unwrap();
        assert!(!model.leased);
        let id = model.image.names[name];
        let inode = model.image.files.get_mut(&id).unwrap();
        inode.bytes[at] ^= 0x80;
        inode.synced = inode.bytes.clone();
    }
    pub fn erase(&self, name: &str) {
        let mut model = self.0.lock().unwrap();
        assert!(!model.leased);
        assert!(model.image.names.remove(name).is_some());
        model.image.durable_names.remove(name);
    }
    pub fn export(&self, path: &std::path::Path) {
        let model = self.0.lock().unwrap();
        assert!(
            !model.leased,
            "export requires a closed immutable crash image"
        );
        std::fs::create_dir(path).unwrap();
        for (name, id) in &model.image.names {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.join(name))
                .unwrap();
            file.write_all(&model.image.files[id].bytes).unwrap();
        }
    }
}
impl Model {
    fn event(&mut self, event: Event) -> (usize, Option<Failure>) {
        assert!(
            self.receipts.len() < 8192,
            "fixture receipt budget exhausted"
        );
        let ordinal = self.receipts.len() + 1;
        let action = self
            .fault
            .as_ref()
            .filter(|fault| fault.ordinal == ordinal)
            .map(|fault| {
                assert!(!self.hit);
                assert_eq!(fault.expected, event, "cut reached a different effect");
                fault.failure
            });
        if action.is_some() {
            self.hit = true;
        }
        self.receipts.push(Receipt {
            event,
            applied: action,
            effect: false,
            completed: 0,
        });
        (ordinal - 1, action)
    }
    fn metadata(&self, id: u64) -> FileMetadata {
        FileMetadata {
            identity: FileIdentity {
                device: 1,
                inode: id,
            },
            length: self.image.files[&id].bytes.len() as u64,
        }
    }
}
impl DirectoryPort for Disk {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        let mut model = self.0.lock().unwrap();
        if model.leased {
            return Err(namespace(Kind::Busy, Effect::Unchanged));
        }
        assert!(
            model.image.names.contains_key("LOCK"),
            "preprovisioned fixture LOCK missing"
        );
        model.leased = true;
        drop(model);
        Ok(Writer {
            model: Arc::clone(&self.0),
            lease: Arc::new(Lease(Arc::clone(&self.0))),
        })
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        let mut model = self.model.lock().unwrap();
        model.image.entropy += 1;
        let mut result = [17; 16];
        result[..8].copy_from_slice(&model.image.entropy.to_le_bytes());
        Ok(result)
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let mut model = self.model.lock().unwrap();
        if matches!(name.as_str(), "LOCK" | "CURRENT" | "WRITER") {
            return Err(namespace(Kind::InvalidName, Effect::Unchanged));
        }
        if model.image.names.contains_key(name.as_str()) {
            return Err(namespace(Kind::AlreadyExists, Effect::Unchanged));
        }
        let id = model.image.next_inode;
        let (step, action) = model.event(Event {
            operation: Operation::Create,
            name: name.as_str().into(),
            inode: id,
            offset: 0,
            bytes: 0,
        });
        if action == Some(Failure::Before) {
            return Err(namespace(Kind::Io, Effect::Unchanged));
        }
        assert!(!matches!(
            action,
            Some(Failure::PrefixError(_) | Failure::Short(_))
        ));
        assert!(
            model.image.files.len() < 8192,
            "cache-loss fixture inode budget exhausted"
        );
        model.image.next_inode += 1;
        model.image.files.insert(id, Inode::default());
        model.image.names.insert(name.as_str().into(), id);
        model.receipts[step].effect = true;
        if action == Some(Failure::After) {
            return Err(namespace(Kind::Io, Effect::CallerMustResolve));
        }
        Ok(File {
            model: Arc::clone(&self.model),
            _lease: Arc::clone(&self.lease),
            inode: id,
            name: name.as_str().into(),
            writable: true,
        })
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        let model = self.model.lock().unwrap();
        let id = *model
            .image
            .names
            .get(name.as_str())
            .ok_or_else(|| namespace(Kind::NotFound, Effect::Unchanged))?;
        Ok(File {
            model: Arc::clone(&self.model),
            _lease: Arc::clone(&self.lease),
            inode: id,
            name: name.as_str().into(),
            writable: mode == OpenMode::ReadWrite,
        })
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let file = self.open_existing(name, mode)?;
        let metadata = self.model.lock().unwrap().metadata(file.inode);
        Ok((file, metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        let model = self.model.lock().unwrap();
        Ok(model
            .image
            .names
            .get(name.as_str())
            .map(|id| model.metadata(*id)))
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        let model = self.model.lock().unwrap();
        if model.image.names.len() > limit {
            return Err(namespace(Kind::LimitExceeded, Effect::Unchanged));
        }
        model
            .image
            .names
            .iter()
            .map(|(name, id)| {
                Ok(DirectoryEntry {
                    name: FileName::new(name)?,
                    metadata: model.metadata(*id),
                })
            })
            .collect()
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        let mut model = self.model.lock().unwrap();
        let (step, action) = model.event(Event {
            operation: Operation::DirectorySync,
            name: String::new(),
            inode: 0,
            offset: 0,
            bytes: 0,
        });
        if action == Some(Failure::Before) {
            return Err(namespace(Kind::Io, Effect::CallerMustResolve));
        }
        assert!(!matches!(
            action,
            Some(Failure::PrefixError(_) | Failure::Short(_))
        ));
        model.image.durable_names = model.image.names.clone();
        model.receipts[step].effect = true;
        if action == Some(Failure::After) {
            return Err(namespace(Kind::Io, Effect::CallerMustResolve));
        }
        Ok(())
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        let mut model = self.model.lock().unwrap();
        let id = *model
            .image
            .names
            .get(temporary.as_str())
            .ok_or_else(|| namespace(Kind::NotFound, Effect::Unchanged))?;
        let (step, action) = model.event(Event {
            operation: Operation::Rename,
            name: selected.as_str().into(),
            inode: id,
            offset: 0,
            bytes: 0,
        });
        if action == Some(Failure::Before) {
            return Err(namespace(Kind::Io, Effect::Unchanged));
        }
        assert!(!matches!(
            action,
            Some(Failure::PrefixError(_) | Failure::Short(_))
        ));
        model.image.names.remove(temporary.as_str());
        model.image.names.insert(selected.as_str().into(), id);
        model.receipts[step].effect = true;
        if action == Some(Failure::After) {
            return Err(namespace(Kind::Io, Effect::CallerMustResolve));
        }
        Ok(())
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        let mut model = self.model.lock().unwrap();
        if matches!(name.as_str(), "LOCK" | "CURRENT" | "WRITER") {
            return Err(namespace(Kind::InvalidName, Effect::Unchanged));
        }
        let id = *model
            .image
            .names
            .get(name.as_str())
            .ok_or_else(|| namespace(Kind::NotFound, Effect::Unchanged))?;
        let (step, action) = model.event(Event {
            operation: Operation::Unlink,
            name: name.as_str().into(),
            inode: id,
            offset: 0,
            bytes: 0,
        });
        if action == Some(Failure::Before) {
            return Err(namespace(Kind::Io, Effect::Unchanged));
        }
        assert!(!matches!(
            action,
            Some(Failure::PrefixError(_) | Failure::Short(_))
        ));
        model.image.names.remove(name.as_str());
        model.receipts[step].effect = true;
        if action == Some(Failure::After) {
            return Err(namespace(Kind::Io, Effect::CallerMustResolve));
        }
        Ok(())
    }
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        let model = self.model.lock().unwrap();
        let source = &model.image.files[&self.inode].bytes;
        let offset = usize::try_from(offset).map_err(|_| failure())?;
        if offset >= source.len() {
            return Ok(0);
        }
        let count = bytes.len().min(source.len() - offset);
        bytes[..count].copy_from_slice(&source[offset..offset + count]);
        Ok(count)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        assert!(self.writable);
        let mut model = self.model.lock().unwrap();
        let (step, action) = model.event(Event {
            operation: Operation::Write,
            name: self.name.clone(),
            inode: self.inode,
            offset,
            bytes: bytes.len(),
        });
        if action == Some(Failure::Before) {
            return Err(failure());
        }
        if model
            .pause
            .as_ref()
            .is_some_and(|(prefix, _)| self.name.starts_with(prefix))
        {
            let (_, gate) = model.pause.take().unwrap();
            let event = model.receipts[step].event.clone();
            drop(model);
            gate.wait(event);
            model = self.model.lock().unwrap();
        }
        let count = match action {
            Some(Failure::PrefixError(count) | Failure::Short(count)) => {
                assert!(count > 0 && count < bytes.len());
                count
            }
            _ => bytes.len(),
        };
        let offset = usize::try_from(offset).map_err(|_| failure())?;
        let end = offset.checked_add(count).ok_or_else(failure)?;
        assert!(end <= 16 * 1024 * 1024, "fixture file bound");
        model
            .image
            .admit(end.saturating_sub(model.image.files[&self.inode].bytes.len()));
        let target = &mut model.image.files.get_mut(&self.inode).unwrap().bytes;
        if target.len() < end {
            target.resize(end, 0);
        }
        target[offset..end].copy_from_slice(&bytes[..count]);
        model.receipts[step].effect = true;
        model.receipts[step].completed = count;
        if matches!(action, Some(Failure::After | Failure::PrefixError(_))) {
            return Err(failure());
        }
        Ok(count)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let mut model = self.model.lock().unwrap();
        let size = model.image.files[&self.inode].bytes.len();
        let (step, action) = model.event(Event {
            operation: Operation::Sync,
            name: self.name.clone(),
            inode: self.inode,
            offset: 0,
            bytes: size,
        });
        if action == Some(Failure::Before) {
            return Err(failure());
        }
        assert!(!matches!(
            action,
            Some(Failure::PrefixError(_) | Failure::Short(_))
        ));
        model
            .image
            .admit(size.saturating_sub(model.image.files[&self.inode].synced.len()));
        let inode = model.image.files.get_mut(&self.inode).unwrap();
        inode.synced.clone_from(&inode.bytes);
        model.receipts[step].effect = true;
        model.receipts[step].completed = size;
        if action == Some(Failure::After) {
            return Err(failure());
        }
        Ok(())
    }
    fn set_len(&mut self, length: u64) -> io::Result<()> {
        assert!(self.writable);
        let size = usize::try_from(length).map_err(|_| failure())?;
        assert!(size <= 16 * 1024 * 1024);
        let mut model = self.model.lock().unwrap();
        let (step, action) = model.event(Event {
            operation: Operation::Resize,
            name: self.name.clone(),
            inode: self.inode,
            offset: 0,
            bytes: size,
        });
        if action == Some(Failure::Before) {
            return Err(failure());
        }
        assert!(!matches!(
            action,
            Some(Failure::PrefixError(_) | Failure::Short(_))
        ));
        model
            .image
            .admit(size.saturating_sub(model.image.files[&self.inode].bytes.len()));
        model
            .image
            .files
            .get_mut(&self.inode)
            .unwrap()
            .bytes
            .resize(size, 0);
        model.receipts[step].effect = true;
        if action == Some(Failure::After) {
            return Err(failure());
        }
        Ok(())
    }
}
