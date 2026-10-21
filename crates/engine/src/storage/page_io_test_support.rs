use super::*;
use crate::api::DatabaseId;
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    identity::{Generation, ObjectId},
    io::FileIo,
    namespace::{
        DirectoryEntry, FileIdentity, FileMetadata, NamespaceErrorKind, NamespaceResult, Selector,
    },
};

pub(in crate::storage) const PROFILE: &[u8] =
    include_bytes!("../../tests/fixtures/dev3-io-profile.bin");
pub(in crate::storage) const SELECTOR: &[u8] =
    include_bytes!("../../tests/fixtures/dev3-io-selector.bin");
pub(in crate::storage) const GOLDEN: &[u8] =
    include_bytes!("../../tests/fixtures/dev3-page-v1-cells.bin");
pub(in crate::storage) const DATA: &str = "d3-data-0000000100000002";
pub(in crate::storage) const MAP: &str = "d3-filemap-0";
pub(in crate::storage) fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}
pub(in crate::storage) fn expected() -> PageIdentity {
    PageIdentity {
        database: ns().database,
        page: super::super::page::PageRef {
            file: FileId::new(0x100000002).unwrap(),
            page_no: 3,
            incarnation: Generation::new(4).unwrap(),
        },
        owner: ObjectId::new(5).unwrap(),
        owner_generation: Generation::new(6).unwrap(),
    }
}
pub(in crate::storage) fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
pub(in crate::storage) fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
pub(in crate::storage) fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
pub(in crate::storage) fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let hash = content_digest(&b[..end]);
    b[end..].copy_from_slice(&hash);
}
// Independent raw contract offsets; fixture setup never calls product encoders.
pub(in crate::storage) fn raw_control(kind: u8, index: u32, count: u32, body: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 160 + body.len()];
    let len = b.len() as u32;
    b[..4].copy_from_slice(b"XDC1");
    put16(&mut b, 4, 1);
    b[6] = kind;
    put16(&mut b, 8, 128);
    put32(&mut b, 12, len);
    b[16..32].fill(0x11);
    put64(&mut b, 32, 17);
    put64(&mut b, 40, 23);
    put32(&mut b, 48, index);
    put32(&mut b, 52, count);
    put32(&mut b, 56, body.len() as u32);
    b[128..len as usize - 32].copy_from_slice(body);
    seal(&mut b);
    b
}
pub(in crate::storage) fn raw_ref(kind: u8, index: u32, bytes: &[u8]) -> [u8; 64] {
    let mut b = [0; 64];
    b[0] = kind;
    put32(&mut b, 4, index);
    put64(&mut b, 8, 23);
    put32(&mut b, 16, bytes.len() as u32);
    b[32..].copy_from_slice(&content_digest(bytes));
    b
}

#[derive(Clone, Copy, Debug)]
pub(in crate::storage) enum Action {
    BeforeError,
    Short(usize),
    Interrupted,
    PrefixEof,
    PrefixError,
    InvalidCount,
}
#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct Plan {
    pub(in crate::storage) phase: usize,
    pub(in crate::storage) action: Action,
}
#[derive(Debug)]
pub(in crate::storage) struct Read {
    pub(in crate::storage) name: String,
    pub(in crate::storage) offset: u64,
    pub(in crate::storage) bytes: usize,
    pub(in crate::storage) calls: usize,
}
pub(in crate::storage) struct Active {
    pub(in crate::storage) phase: usize,
    pub(in crate::storage) next: u64,
    pub(in crate::storage) end: u64,
}
pub(in crate::storage) struct Pause {
    pub(in crate::storage) offset: u64,
    pub(in crate::storage) entered: std::sync::mpsc::SyncSender<()>,
    pub(in crate::storage) release: std::sync::mpsc::Receiver<()>,
}
#[derive(Default)]
pub(in crate::storage) struct State {
    pub(in crate::storage) pause: Option<Pause>,
    pub(in crate::storage) files: BTreeMap<String, Vec<u8>>,
    pub(in crate::storage) handles: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) mutations: usize,
    pub(in crate::storage) reads: Vec<Read>,
    pub(in crate::storage) active: Option<Active>,
    pub(in crate::storage) plan: Option<Plan>,
    pub(in crate::storage) hit: bool,
    pub(in crate::storage) actual_short: bool,
}
pub(in crate::storage) struct Writer(pub(in crate::storage) Arc<Mutex<State>>);
pub(in crate::storage) struct File {
    pub(in crate::storage) state: Arc<Mutex<State>>,
    pub(in crate::storage) name: String,
}
impl Drop for File {
    fn drop(&mut self) {
        self.state.lock().unwrap().handles -= 1;
    }
}
pub(in crate::storage) fn io_failure() -> io::Error {
    io::Error::other("fixed read fault")
}
pub(in crate::storage) fn unsupported() -> NamespaceError {
    NamespaceError::new(
        NamespaceErrorKind::Io,
        Effect::Unchanged,
        "unexpected mutation",
    )
}
impl FileIo for File {
    fn read_at(&mut self, output: &mut [u8], offset: u64) -> io::Result<usize> {
        let pause = {
            let mut s = self.state.lock().unwrap();
            if self.name == DATA && s.pause.as_ref().is_some_and(|p| p.offset == offset) {
                s.pause.take()
            } else {
                None
            }
        };
        if let Some(pause) = pause {
            pause.entered.send(()).unwrap();
            pause
                .release
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        }
        let mut s = self.state.lock().unwrap();
        if s.active.is_none() {
            let phase = s.reads.len();
            s.reads.push(Read {
                name: self.name.clone(),
                offset,
                bytes: output.len(),
                calls: 0,
            });
            s.active = Some(Active {
                phase,
                next: offset,
                end: offset + output.len() as u64,
            });
        }
        let a = s.active.as_ref().unwrap();
        assert_eq!(a.next, offset);
        assert_eq!(a.end - offset, output.len() as u64);
        let phase = a.phase;
        assert_eq!(s.reads[phase].name, self.name);
        s.reads[phase].calls += 1;
        let call = s.reads[phase].calls;
        let action = s.plan.filter(|p| p.phase == phase).map(|p| p.action);
        if action.is_some() {
            s.hit = true;
        }
        match action {
            Some(Action::BeforeError) => {
                s.active = None;
                return Err(io_failure());
            }
            Some(Action::Interrupted) if call == 1 => {
                return Err(io::ErrorKind::Interrupted.into());
            }
            Some(Action::PrefixEof) if call == 2 => {
                s.active = None;
                return Ok(0);
            }
            Some(Action::PrefixError) if call == 2 => {
                s.active = None;
                return Err(io_failure());
            }
            Some(Action::InvalidCount) => {
                s.active = None;
                return Ok(output.len() + 1);
            }
            _ => {}
        }
        let cap = match action {
            Some(Action::Short(cap)) if call == 1 => cap.min(output.len() - 1),
            Some(Action::Short(cap)) => cap,
            Some(Action::PrefixEof | Action::PrefixError) if call == 1 => 1,
            _ => output.len(),
        };
        let bytes = s.files.get(&self.name).unwrap();
        let start = usize::try_from(offset).unwrap();
        let count = bytes.len().saturating_sub(start).min(output.len()).min(cap);
        if count > 0 {
            output[..count].copy_from_slice(&bytes[start..start + count]);
        }
        if matches!(action, Some(Action::Short(_))) && call == 1 {
            s.actual_short = count > 0 && count < output.len();
        }
        if count == 0 || count == output.len() {
            s.active = None;
        } else {
            s.active.as_mut().unwrap().next += count as u64;
        }
        Ok(count)
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        self.state.lock().unwrap().mutations += 1;
        Err(io_failure())
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().mutations += 1;
        Err(io_failure())
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        self.state.lock().unwrap().mutations += 1;
        Err(io_failure())
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<File> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<File> {
        panic!("must bind returned handle metadata")
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        assert_eq!(mode, OpenMode::ReadOnly);
        let mut s = self.0.lock().unwrap();
        let length = s
            .files
            .get(name.as_str())
            .ok_or_else(|| {
                NamespaceError::new(
                    NamespaceErrorKind::NotFound,
                    Effect::Unchanged,
                    "absent fixture",
                )
            })?
            .len() as u64;
        s.handles += 1;
        s.peak = s.peak.max(s.handles);
        Ok((
            File {
                state: Arc::clone(&self.0),
                name: name.as_str().into(),
            },
            FileMetadata {
                identity: FileIdentity {
                    device: 1,
                    inode: 2,
                },
                length,
            },
        ))
    }
    fn metadata(&self, _: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        panic!("path stat is not handle metadata")
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        panic!("no directory scan or fallback")
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
}
pub(in crate::storage) fn fixture(count: usize, retained: bool) -> Writer {
    let chunks = count.div_ceil(64);
    let mut body = vec![0; 64 + 96 * chunks];
    put32(&mut body, 0, count as u32);
    put32(&mut body, 4, chunks as u32);
    put64(&mut body, 8, (count as u64) << 20);
    if count > 0 {
        put64(&mut body, 16, 0x100000002);
        put64(&mut body, 24, 0x100000001 + count as u64);
    }
    put16(&mut body, 32, 64);
    put16(&mut body, 34, 96);
    let mut container = vec![0; 2_113_536];
    for index in 0..chunks {
        let first = index * 64;
        let n = (count - first).min(64);
        let mut entries = vec![0; n * 96];
        for entry in 0..n {
            let file = 0x100000002 + (first + entry) as u64;
            let b = &mut entries[entry * 96..(entry + 1) * 96];
            put64(b, 0, file);
            put64(b, 8, 1 << 20);
            put64(b, 16, 5);
            put64(b, 24, 6);
            put64(b, 32, file);
            put64(b, 48, 1);
            b[56..80].copy_from_slice(format!("d3-data-{file:016x}").as_bytes());
            b[88] = if retained { 2 } else { 1 };
        }
        let chunk = raw_control(3, index as u32, n as u32, &entries);
        let offset = 16_384 * (1 + index);
        container[offset..offset + chunk.len()].copy_from_slice(&chunk);
        let b = &mut body[64 + index * 96..64 + (index + 1) * 96];
        b[..64].copy_from_slice(&raw_ref(3, index as u32, &chunk));
        put16(b, 64, n as u16);
        put64(b, 72, 0x100000002 + first as u64);
        put64(b, 80, 0x100000001 + (first + n) as u64);
    }
    let root = raw_control(2, 0, chunks as u32, &body);
    container[..root.len()].copy_from_slice(&root);
    let mut selector = SELECTOR.to_vec();
    selector[80..144].copy_from_slice(&raw_ref(2, 0, &root));
    seal(&mut selector);
    if count == 65 && !retained {
        assert_eq!(
            &root,
            include_bytes!("../../tests/fixtures/dev3-io-root.bin")
        );
        assert_eq!(&selector, SELECTOR);
        assert_eq!(
            &container[16_384..16_384 + 6304],
            include_bytes!("../../tests/fixtures/dev3-io-chunk0.bin")
        );
        assert_eq!(
            &container[32_768..32_768 + 256],
            include_bytes!("../../tests/fixtures/dev3-io-chunk1.bin")
        );
    }
    let mut data = vec![0; 1 << 20];
    data[3 * 16_384..4 * 16_384].copy_from_slice(GOLDEN);
    let files = BTreeMap::from([
        ("d3-profile".into(), PROFILE.to_vec()),
        ("FILEMAP".into(), selector),
        (MAP.into(), container),
        (DATA.into(), data),
    ]);
    Writer(Arc::new(Mutex::new(State {
        files,
        ..State::default()
    })))
}
