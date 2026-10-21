//! Worker-only native fault composition. Hub serialization and blocked ports
//! deliberately perturb scheduling: these runs are never PF/performance evidence.
//! Frame prefix recognition identifies a cut only; it does not decode expected
//! rows, validate durability, or call a production/reference recovery codec.
use std::{
    cell::Cell,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceError, NamespaceErrorKind,
        NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};
use xadbtest_protocol::{
    durable,
    fault::{self as wire, Boundary as B, Target as T},
    frame,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Context {
    run: [u8; 16],
    actor: u8,
    request: u64,
    phase: wire::Phase,
}
thread_local! {static CONTEXT:Cell<Option<Context>>=const{Cell::new(None)};}
pub(crate) struct Scope(Option<Context>);
impl Scope {
    pub(crate) fn enter(request: &durable::Request) -> Self {
        let phase = match request.command.action {
            durable::Action::CreateDatabase { .. } => wire::Phase::Create,
            durable::Action::OpenDatabase { .. } => wire::Phase::Open,
            durable::Action::RestoreDatabase { .. } => wire::Phase::Restore,
            durable::Action::Commit { .. } => wire::Phase::Commit,
            durable::Action::Checkpoint { .. } => wire::Phase::Checkpoint,
            durable::Action::Backup { .. } => wire::Phase::Backup,
            durable::Action::CollectGarbage { .. } => wire::Phase::GarbageCollect,
            _ => wire::Phase::Other,
        };
        Self(CONTEXT.replace(Some(Context {
            run: request.run,
            actor: request.actor,
            request: request.request,
            phase,
        })))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CONTEXT.set(self.0);
    }
}
struct Control {
    stream: UnixStream,
    run: [u8; 16],
    token: [u8; 16],
    plan: wire::Plan,
    matched: u32,
    fired: bool,
    bytes: usize,
}
struct State {
    configured: bool,
    control: Option<Control>,
}
pub(crate) struct Hub(Mutex<State>);
impl Hub {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(State {
            configured: false,
            control: None,
        })))
    }
    pub(crate) fn configure(
        &self,
        run: [u8; 16],
        token: [u8; 16],
        plan: wire::Plan,
    ) -> io::Result<()> {
        plan.validate().map_err(io::Error::other)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| io::Error::other("fault hub poisoned"))?;
        if state.configured {
            return Err(io::Error::other("fault plan already consumed"));
        }
        state.configured = true;
        // This trusted environment value is installed by the owning controller,
        // points into its fresh retained run directory, and is never a wire path.
        let path = std::env::var_os("XADBTEST_CONTROL_SOCKET")
            .ok_or_else(|| io::Error::other("missing owned control socket"))?;
        let stream = UnixStream::connect(path)?;
        let mut control = Control {
            stream,
            run,
            token,
            plan,
            matched: 0,
            fired: false,
            bytes: 0,
        };
        control.send(wire::Event::Connected {
            process_id: std::process::id(),
        })?;
        state.control = Some(control);
        Ok(())
    }
    fn partial(&self, point: Point) -> io::Result<bool> {
        let state = self
            .0
            .lock()
            .map_err(|_| io::Error::other("fault hub poisoned"))?;
        let Some(control) = &state.control else {
            return Ok(false);
        };
        Ok(!control.fired
            && control.plan.cut.boundary == B::WritePartial
            && point.matches(control)?)
    }
    fn hit(&self, point: Point) -> io::Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| io::Error::other("fault hub poisoned"))?;
        let Some(control) = &mut state.control else {
            return Ok(());
        };
        if control.fired
            || control.plan.cut.boundary != point.boundary
            || !point.matches(control)?
        {
            return Ok(());
        }
        control.matched = control
            .matched
            .checked_add(1)
            .ok_or_else(|| io::Error::other("fault occurrence overflow"))?;
        if control.matched != control.plan.occurrence {
            return Ok(());
        }
        control.fired = true;
        let context = CONTEXT
            .get()
            .ok_or_else(|| io::Error::other("unattributed fault effect"))?;
        control.send(wire::Event::Reached {
            witness: wire::Reached {
                plan: control.plan,
                matched: control.matched,
                actor: context.actor,
                request: context.request,
                directory: point.directory,
                phase: context.phase,
                observed_target: point.target,
                file: point.file.map(|name| name.as_str().to_owned()),
                offset: point.offset,
                requested_bytes: point.requested,
                completed_bytes: point.completed,
                frame_start: point.frame_start,
                frame_end: point.frame_end,
                frame_written_bytes: point.frame_written,
            },
        })?;
        let release = read_release(&mut control.stream)?;
        if release.run != control.run || release.token != control.token {
            return Err(io::Error::other("fault release identity"));
        }
        control.send(wire::Event::Released {
            decision: release.decision,
        })?;
        match release.decision {
            wire::Decision::Continue => Ok(()),
            wire::Decision::ReturnError => {
                Err(io::Error::other("injected native boundary failure"))
            }
        }
    }
}
impl Control {
    fn send(&mut self, event: wire::Event) -> io::Result<()> {
        let bytes = frame::encode(&wire::Observation {
            version: wire::VERSION,
            run: self.run,
            token: self.token,
            event,
        })
        .map_err(io::Error::other)?;
        if bytes.len() > wire::MAX_MESSAGE_BYTES
            || self
                .bytes
                .checked_add(bytes.len())
                .is_none_or(|count| count > wire::MAX_CONTROL_BYTES)
        {
            return Err(io::Error::other("fault control byte limit"));
        }
        self.stream.write_all(&bytes)?;
        self.stream.flush()?;
        self.bytes += bytes.len();
        Ok(())
    }
}
fn read_release(stream: &mut UnixStream) -> io::Result<wire::Release> {
    let mut header = [0; 4];
    stream.read_exact(&mut header)?;
    let count = u32::from_be_bytes(header) as usize;
    if count == 0 || count > wire::MAX_MESSAGE_BYTES - 4 {
        return Err(io::Error::other("fault release frame limit"));
    }
    let mut bytes = vec![0; count + 4];
    bytes[..4].copy_from_slice(&header);
    stream.read_exact(&mut bytes[4..])?;
    frame::read(&mut bytes.as_slice())
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("fault release missing"))
}
#[derive(Clone, Copy)]
struct Point {
    target: T,
    boundary: B,
    directory: u8,
    file: Option<FileName>,
    offset: Option<u64>,
    requested: Option<u64>,
    completed: Option<u64>,
    frame_complete: bool,
    frame_start: Option<u64>,
    frame_end: Option<u64>,
    frame_written: Option<u64>,
}
impl Point {
    fn new(target: T, boundary: B, directory: u8, file: Option<FileName>) -> Self {
        Self {
            target,
            boundary,
            directory,
            file,
            offset: None,
            requested: None,
            completed: None,
            frame_complete: false,
            frame_start: None,
            frame_end: None,
            frame_written: None,
        }
    }
    fn matches(self, control: &Control) -> io::Result<bool> {
        let context = CONTEXT
            .get()
            .ok_or_else(|| io::Error::other("unattributed native effect"))?;
        if context.run != control.run {
            return Err(io::Error::other("native effect run mismatch"));
        }
        Ok(
            (control.plan.cut.target == T::Any || control.plan.cut.target == self.target)
                && (control.plan.cut.target != T::WalCommit
                    || self.boundary != B::WriteAfter
                    || self.frame_complete)
                && control
                    .plan
                    .actor
                    .is_none_or(|actor| actor == context.actor)
                && control
                    .plan
                    .directory
                    .is_none_or(|directory| directory == self.directory),
        )
    }
}
fn namespace(error: io::Error, after: bool) -> NamespaceError {
    NamespaceError::io(
        NamespaceErrorKind::Io,
        if after {
            Effect::CallerMustResolve
        } else {
            Effect::Unchanged
        },
        "native fault boundary",
        error,
    )
}

/// Exact prepared/selected names identify an effect role, not valid contents.
/// Data/control payload files and similar prefixes intentionally remain untyped.
fn selector_role(name: FileName) -> Option<T> {
    match name.as_str() {
        "d3-filemap-select-stage" | "FILEMAP" => Some(T::FileMap),
        "d3-dwb-select-stage" | "DWB" => Some(T::Doublewrite),
        _ => None,
    }
}

pub(crate) struct Directory {
    inner: LinuxDirectory,
    hub: Arc<Hub>,
    slot: u8,
}
impl Directory {
    pub(crate) fn new(inner: LinuxDirectory, hub: Arc<Hub>, slot: u8) -> Self {
        Self { inner, hub, slot }
    }
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        Ok(Writer {
            inner: self.inner.acquire_writer()?,
            hub: Arc::clone(&self.hub),
            slot: self.slot,
            lineage: None,
        })
    }
}
struct Lineage {
    context: Context,
    target: T,
    file: FileName,
}
pub(crate) struct Writer {
    inner: LinuxWriterLease,
    hub: Arc<Hub>,
    slot: u8,
    lineage: Option<Lineage>,
}
impl Writer {
    fn hit(
        &self,
        target: T,
        boundary: B,
        file: Option<FileName>,
        after: bool,
    ) -> NamespaceResult<()> {
        self.hub
            .hit(Point::new(target, boundary, self.slot, file))
            .map_err(|error| namespace(error, after))
    }
    fn file(&self, inner: LinuxNamespaceFile, name: FileName, opened: bool) -> File {
        File {
            inner,
            hub: Arc::clone(&self.hub),
            slot: self.slot,
            name,
            opened,
            role: selector_role(name).unwrap_or(T::Any),
            pending: None,
        }
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.inner.random_incarnation()
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        self.hit(T::Any, B::CreateBefore, Some(*name), false)?;
        let inner = self.inner.create_new(name)?;
        self.hit(T::Any, B::CreateAfter, Some(*name), true)?;
        Ok(self.file(inner, *name, false))
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        let inner = self.inner.open_existing(name, mode)?;
        Ok(self.file(inner, *name, true))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let (inner, metadata) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.file(inner, *name, true), metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        let current = CONTEXT.get();
        let (target, file) = self
            .lineage
            .as_ref()
            .filter(|lineage| Some(lineage.context) == current)
            .map_or((T::Any, None), |lineage| {
                (lineage.target, Some(lineage.file))
            });
        self.hit(target, B::DirectorySyncBefore, file, true)?;
        self.inner.sync_directory()?;
        self.hit(target, B::DirectorySyncAfter, file, true)?;
        self.lineage = None;
        Ok(())
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        let target = match selected {
            Selector::Current => T::Current,
            Selector::Writer => T::Writer,
            Selector::FileMap => T::FileMap,
            Selector::Doublewrite => T::Doublewrite,
        };
        self.hit(target, B::RenameBefore, Some(*temporary), false)?;
        self.inner.publish_selector(temporary, selected)?;
        self.lineage = CONTEXT.get().map(|context| Lineage {
            context,
            target,
            file: *temporary,
        });
        self.hit(target, B::RenameAfter, Some(*temporary), true)
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        self.hit(T::Retirement, B::UnlinkBefore, Some(*name), false)?;
        self.inner.remove_retired(name)?;
        self.lineage = CONTEXT.get().map(|context| Lineage {
            context,
            target: T::Retirement,
            file: *name,
        });
        self.hit(T::Retirement, B::UnlinkAfter, Some(*name), true)
    }
}
#[derive(Clone, Copy)]
struct PendingFrame {
    target: T,
    start: u64,
    next: u64,
    end: u64,
}
pub(crate) struct File {
    inner: LinuxNamespaceFile,
    hub: Arc<Hub>,
    slot: u8,
    name: FileName,
    opened: bool,
    role: T,
    pending: Option<PendingFrame>,
}
impl File {
    fn point(&self, target: T, boundary: B) -> Point {
        let mut point = Point::new(target, boundary, self.slot, Some(self.name));
        if let Some(frame) = self.pending {
            point.frame_start = Some(frame.start);
            point.frame_end = Some(frame.end);
            point.frame_written = Some(frame.next - frame.start);
            point.frame_complete = frame.next == frame.end;
        }
        point
    }
    fn start_frame(&mut self, bytes: &[u8], offset: u64) -> io::Result<()> {
        if let Some(role) = selector_role(self.name) {
            // DEV3 selector cuts do not depend on a DEV2 frame prefix, even if
            // the caller writes bytes which happen to resemble that envelope.
            self.role = role;
            self.pending = None;
            return Ok(());
        }
        if self
            .pending
            .is_some_and(|frame| frame.next == offset && frame.next < frame.end)
        {
            return Ok(());
        }
        self.pending = None;
        // AXF1 envelope revision1 plus XDK1 record revision/capability1. No hash,
        // row, q or transaction result is inferred from these dispatch bytes.
        if bytes.len() < 25
            || &bytes[..6] != b"AXF1\x01\x00"
            || &bytes[10..16] != b"XDK1\x01\x00"
            || bytes[16..24] != 1u64.to_le_bytes()
        {
            return Ok(());
        }
        let payload = u32::from_le_bytes(bytes[6..10].try_into().expect("four bytes")) as usize;
        let length = payload
            .checked_add(42)
            .ok_or_else(|| io::Error::other("fault frame length overflow"))?;
        if payload > 1024 * 1024 || length != bytes.len() {
            return Ok(());
        }
        let target = match bytes[24] {
            2 if self.name.as_str().starts_with("wal-") => T::WalCommit,
            5 if self.name.as_str().starts_with("select-") => T::Writer,
            6 if self.name.as_str().starts_with("select-") => T::Current,
            _ => T::Any,
        };
        let end = offset
            .checked_add(length as u64)
            .ok_or_else(|| io::Error::other("fault frame end overflow"))?;
        self.role = if matches!(target, T::Current | T::Writer) {
            target
        } else {
            T::Any
        };
        self.pending = Some(PendingFrame {
            target,
            start: offset,
            next: offset,
            end,
        });
        Ok(())
    }
    fn sync_target(&self) -> T {
        if let Some(role) = selector_role(self.name) {
            return role;
        }
        if self.opened
            && CONTEXT
                .get()
                .is_some_and(|context| context.phase == wire::Phase::Open)
        {
            return T::RecoveryOldFile;
        }
        self.pending
            .filter(|frame| frame.next == frame.end)
            .map_or(self.role, |frame| frame.target)
    }
}

#[cfg(test)]
#[path = "fault_tests.rs"]
mod tests;
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.inner.read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        self.start_frame(bytes, offset)?;
        let pending = self.pending;
        let target = pending.map_or(self.role, |frame| frame.target);
        let mut point = self.point(target, B::WriteBefore);
        point.offset = Some(offset);
        point.requested = Some(bytes.len() as u64);
        point.completed = Some(0);
        // A WAL first-write cut is emitted only once for its complete input frame.
        if target != T::WalCommit || pending.is_some_and(|frame| frame.start == offset) {
            self.hub.hit(point)?;
        }
        point.boundary = B::WritePartial;
        let partial = bytes.len() > 1
            && (target != T::WalCommit || pending.is_some_and(|frame| frame.start == offset))
            && self.hub.partial(point)?;
        let written = self
            .inner
            .write_at(if partial { &bytes[..1] } else { bytes }, offset)?;
        if let Some(frame) = &mut self.pending {
            if frame.next != offset || written > bytes.len() {
                return Err(io::Error::other("native write sequence mismatch"));
            }
            frame.next = offset
                .checked_add(written as u64)
                .ok_or_else(|| io::Error::other("native write extent overflow"))?;
            if frame.next > frame.end {
                return Err(io::Error::other("native write exceeded frame"));
            }
            point.frame_complete = frame.next == frame.end;
            point.frame_written = Some(frame.next - frame.start);
        }
        point.completed = Some(written as u64);
        if partial && written == 1 {
            self.hub.hit(point)?;
        }
        point.boundary = B::WriteAfter;
        self.hub.hit(point)?;
        Ok(written)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        let target = self.sync_target();
        self.hub.hit(self.point(target, B::SyncBefore))?;
        self.inner.sync_all()?;
        self.hub.hit(self.point(target, B::SyncAfter))?;
        self.pending = None;
        Ok(())
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        self.inner.set_len(len)
    }
}
