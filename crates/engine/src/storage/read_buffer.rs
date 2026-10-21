//! Bounded selected clean pages, frame pins and clock replacement.
//! The exclusive selected namespace stays immutable for this buffer's lifetime.
//! This is not a dirty buffer, transient undo pin or transaction reservation.
#![allow(dead_code)] // Awaiting the separately reviewed transaction consumer.

use std::sync::{
    Mutex, MutexGuard, RwLock, RwLockReadGuard,
    atomic::{AtomicBool, Ordering},
};

use super::{
    control::Profile,
    page::{PAGE_BYTES, PageIdentity, PageKind, PageView, decode_page},
    page_io::{PageFileReader, PageIoError, validate_page_request},
};
use xadb_foundation::namespace::WriterNamespace;

#[derive(Debug)]
pub(crate) enum BufferError {
    Read(PageIoError),
    InvalidCapacity,
    BufferBusy,
    IdentityMismatch,
    PinLimit,
    GenerationExhausted,
    Poisoned,
}
impl From<PageIoError> for BufferError {
    fn from(error: PageIoError) -> Self {
        Self::Read(error)
    }
}
type Result<T> = std::result::Result<T, BufferError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Request {
    identity: PageIdentity,
    kind: PageKind,
    level: u8,
}
impl Request {
    fn same_location(self, other: Self) -> bool {
        self.identity.database == other.identity.database
            && self.identity.page.file == other.identity.page.file
            && self.identity.page.page_no == other.identity.page.page_no
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Empty,
    Loading(Request),
    Clean(Request),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Header {
    state: State,
    generation: u64,
    pins: u32,
    referenced: bool,
}
impl Header {
    const fn new() -> Self {
        Self {
            state: State::Empty,
            generation: 0,
            pins: 0,
            referenced: false,
        }
    }
}

/// Caller-owned payload and metadata; its exclusive pool borrow prevents aliases.
pub(crate) struct ReadFrame {
    content: RwLock<[u8; PAGE_BYTES]>,
    header: Mutex<Header>,
}
impl ReadFrame {
    pub(crate) const fn new() -> Self {
        Self {
            content: RwLock::new([0; PAGE_BYTES]),
            header: Mutex::new(Header::new()),
        }
    }
}
struct Loader<'source, 'memory, W: WriterNamespace> {
    reader: PageFileReader<'source, W>,
    scratch: &'memory mut [u8; PAGE_BYTES],
}

/// No growing maps or per-pin allocation. W and W::File auto traits govern sharing.
pub(crate) struct ReadBuffer<'source, 'memory, W: WriterNamespace> {
    frames: &'memory [ReadFrame],
    profile: Profile,
    selector_generation: u64,
    hand: Mutex<usize>,
    loader: Mutex<Loader<'source, 'memory, W>>,
    fenced: AtomicBool,
}

impl<'source, 'memory, W: WriterNamespace> ReadBuffer<'source, 'memory, W> {
    /// Sequential workspace reuse preserves generations and clears only quiescent state.
    /// Poison, orphaned Loading claims or leaked pins reject reuse; none is repaired.
    pub(crate) fn new(
        reader: PageFileReader<'source, W>,
        frames: &'memory mut [ReadFrame],
        load_scratch: &'memory mut [u8; PAGE_BYTES],
    ) -> Result<Self> {
        let profile = reader.profile();
        let selector_generation = reader.selector_generation();
        if frames.is_empty() || frames.len() as u64 > profile.buffer_bytes() / PAGE_BYTES as u64 {
            return Err(BufferError::InvalidCapacity);
        }
        for frame in frames.iter_mut() {
            if frame.content.get_mut().is_err() {
                return Err(BufferError::Poisoned);
            }
            let header = frame.header.get_mut().map_err(|_| BufferError::Poisoned)?;
            if header.pins != 0 || matches!(header.state, State::Loading(_)) {
                return Err(BufferError::Poisoned);
            }
            header.state = State::Empty;
            header.referenced = false;
        }
        Ok(Self {
            frames,
            profile,
            selector_generation,
            hand: Mutex::new(0),
            loader: Mutex::new(Loader {
                reader,
                scratch: load_scratch,
            }),
            fenced: AtomicBool::new(false),
        })
    }

    fn live(&self) -> Result<()> {
        if self.fenced.load(Ordering::Acquire) {
            Err(BufferError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn fence(&self) -> BufferError {
        self.fenced.store(true, Ordering::Release);
        BufferError::Poisoned
    }
    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> Result<MutexGuard<'a, T>> {
        mutex.lock().map_err(|_| self.fence())
    }

    /// Busy is immediate; the caller releases partial pin sets before retrying.
    pub(crate) fn pin(
        &self,
        identity: PageIdentity,
        kind: PageKind,
        level: u8,
    ) -> Result<PagePin<'_, 'source, 'memory, W>> {
        self.live()?;
        validate_page_request(self.profile, identity, kind, level)?;
        let request = Request {
            identity,
            kind,
            level,
        };
        let mut hand = self.lock(&self.hand)?;
        self.live()?;
        for (index, frame) in self.frames.iter().enumerate() {
            let mut header = self.lock(&frame.header)?;
            let found = match header.state {
                State::Empty => continue,
                State::Loading(found) | State::Clean(found) => found,
            };
            if !request.same_location(found) {
                continue;
            }
            if request != found {
                return Err(BufferError::IdentityMismatch);
            }
            if matches!(header.state, State::Loading(_)) {
                return Err(BufferError::BufferBusy);
            }
            header.pins = header.pins.checked_add(1).ok_or(BufferError::PinLimit)?;
            header.referenced = true;
            return Ok(PagePin {
                buffer: self,
                index,
                generation: header.generation,
                request,
            });
        }
        let mut exhausted = false;
        for _ in 0..2 * self.frames.len() {
            let index = *hand;
            *hand = (index + 1) % self.frames.len();
            let mut header = self.lock(&self.frames[index].header)?;
            if matches!(header.state, State::Loading(_)) || header.pins != 0 {
                continue;
            }
            if matches!(header.state, State::Clean(_)) && header.referenced {
                header.referenced = false;
                continue;
            }
            let Some(generation) = header.generation.checked_add(1) else {
                exhausted = true;
                continue;
            };
            *header = Header {
                state: State::Loading(request),
                generation,
                pins: 0,
                referenced: false,
            };
            let mut claim = LoadClaim {
                buffer: self,
                index,
                generation,
                request,
                armed: true,
            };
            drop(header);
            drop(hand);
            let loaded = self.load(index, request);
            if let Err(error) = loaded {
                claim.finish(false)?;
                return Err(error);
            }
            claim.finish(true)?;
            return Ok(PagePin {
                buffer: self,
                index,
                generation,
                request,
            });
        }
        Err(if exhausted {
            BufferError::GenerationExhausted
        } else {
            BufferError::BufferBusy
        })
    }

    fn load(&self, index: usize, request: Request) -> Result<()> {
        let mut loader = self.lock(&self.loader)?;
        self.live()?;
        let Loader { reader, scratch } = &mut *loader;
        reader.read_page(request.identity, request.kind, request.level, scratch)?;
        let mut content = self.frames[index]
            .content
            .write()
            .map_err(|_| self.fence())?;
        content.copy_from_slice(*scratch);
        Ok(())
    }
}

struct LoadClaim<'pool, 'source, 'memory, W: WriterNamespace> {
    buffer: &'pool ReadBuffer<'source, 'memory, W>,
    index: usize,
    generation: u64,
    request: Request,
    armed: bool,
}
impl<W: WriterNamespace> LoadClaim<'_, '_, '_, W> {
    fn finish(&mut self, success: bool) -> Result<()> {
        let mut header = self.buffer.lock(&self.buffer.frames[self.index].header)?;
        if header.state != State::Loading(self.request)
            || header.generation != self.generation
            || header.pins != 0
        {
            return Err(self.buffer.fence());
        }
        self.buffer.live()?;
        header.state = if success {
            State::Clean(self.request)
        } else {
            State::Empty
        };
        header.pins = u32::from(success);
        header.referenced = success;
        self.armed = false;
        Ok(())
    }
}
impl<W: WriterNamespace> Drop for LoadClaim<'_, '_, '_, W> {
    fn drop(&mut self) {
        if self.armed {
            self.buffer.fence();
        }
    }
}

/// One replacement pin. Shared read guards borrow this pin; it cannot be cloned.
pub(crate) struct PagePin<'pool, 'source, 'memory, W: WriterNamespace> {
    buffer: &'pool ReadBuffer<'source, 'memory, W>,
    index: usize,
    generation: u64,
    request: Request,
}
impl<W: WriterNamespace> PagePin<'_, '_, '_, W> {
    pub(crate) fn read(&self) -> Result<PageReadGuard<'_>> {
        self.buffer.live()?;
        {
            let header = self.buffer.lock(&self.buffer.frames[self.index].header)?;
            if header.state != State::Clean(self.request)
                || header.generation != self.generation
                || header.pins == 0
            {
                return Err(self.buffer.fence());
            }
        }
        let content = self.buffer.frames[self.index]
            .content
            .read()
            .map_err(|_| self.buffer.fence())?;
        self.buffer.live()?;
        Ok(PageReadGuard {
            content,
            request: self.request,
            fenced: &self.buffer.fenced,
        })
    }
}
impl<W: WriterNamespace> Drop for PagePin<'_, '_, '_, W> {
    fn drop(&mut self) {
        let Ok(mut header) = self.buffer.frames[self.index].header.lock() else {
            self.buffer.fence();
            return;
        };
        if header.state != State::Clean(self.request)
            || header.generation != self.generation
            || header.pins == 0
        {
            self.buffer.fence();
            return;
        }
        header.pins -= 1;
    }
}

pub(crate) struct PageReadGuard<'pin> {
    content: RwLockReadGuard<'pin, [u8; PAGE_BYTES]>,
    request: Request,
    fenced: &'pin AtomicBool,
}
impl PageReadGuard<'_> {
    pub(crate) fn bytes(&self) -> &[u8; PAGE_BYTES] {
        &self.content
    }
    pub(crate) fn view(&self) -> Result<PageView<'_>> {
        if self.fenced.load(Ordering::Acquire) {
            return Err(BufferError::Poisoned);
        }
        let page = decode_page(&*self.content, self.request.identity, self.request.kind);
        match page {
            Ok(page) if page.header().level == self.request.level => Ok(page),
            _ => {
                self.fenced.store(true, Ordering::Release);
                Err(BufferError::Poisoned)
            }
        }
    }
}

#[cfg(test)]
#[path = "read_buffer_tests.rs"]
mod tests;
