//! Narrow native-test assembly for unfinished storage leaves, not DB readiness.
//! Available only with the explicit `test-support` feature.

/// Native assembly for the limited fresh genesis WAL session.
pub mod genesis_wal;
pub mod storage_session;

use crate::{
    api::{DatabaseId, Namespace},
    storage::{
        page::{FileId, PageIdentity, PageKind, PageRef},
        page_io::{PageFileReader, PageIoError},
    },
};
use xadb_foundation::{
    identity::{Generation, ObjectId},
    namespace::WriterNamespace,
};

/// Primitive request accepted by the native selected-map test assembly.
pub struct PageReadRequest {
    /// Expected page database identity, checked against the selected profile.
    pub database: [u8; 16],
    /// Packed file epoch and counter.
    pub file: u64,
    /// Zero-based physical page number.
    pub page: u64,
    /// Expected page incarnation.
    pub incarnation: u64,
    /// Expected object owner.
    pub owner: u64,
    /// Expected owner generation.
    pub owner_generation: u64,
    /// Envelope kind from the DEV3 page contract.
    pub kind: u8,
    /// Expected tree level.
    pub level: u8,
}

/// Stable coarse projection for native leaf assertions; no durable outcome.
#[derive(Debug, PartialEq, Eq)]
pub enum PageReadFailure {
    /// Primitive request cannot form a legal typed identity or kind.
    InvalidRequest,
    /// Namespace open/metadata failure.
    Namespace,
    /// Exact read failed.
    Read,
    /// Selected control validation failed.
    Control,
    /// Page envelope validation failed.
    Page,
    /// Profile, allocation identity or level mismatch.
    IdentityMismatch,
    /// Opened handle has a different physical length.
    LengthMismatch,
    /// Requested file is absent from the selected map.
    MissingMapping,
    /// Page offset exceeds the selected extent.
    OutOfBounds,
    /// The fresh session already has a borrowed reader.
    Busy,
    /// The shared storage session has failed permanently.
    Fenced,
    /// A namespace lock was poisoned.
    Poisoned,
}

impl From<PageIoError> for PageReadFailure {
    fn from(error: PageIoError) -> Self {
        match error {
            PageIoError::Namespace(_) => Self::Namespace,
            PageIoError::Read(_) => Self::Read,
            PageIoError::Control(_) => Self::Control,
            PageIoError::Page(_) => Self::Page,
            PageIoError::InvalidRequest => Self::InvalidRequest,
            PageIoError::IdentityMismatch => Self::IdentityMismatch,
            PageIoError::LengthMismatch => Self::LengthMismatch,
            PageIoError::MissingMapping => Self::MissingMapping,
            PageIoError::OutOfBounds => Self::OutOfBounds,
            PageIoError::Busy => Self::Busy,
            PageIoError::Fenced => Self::Fenced,
            PageIoError::Poisoned => Self::Poisoned,
        }
    }
}

/// Verify one selected map and one page through the production read leaf.
/// Returns `(selector_generation, cell_count)` and fills caller output. On error
/// output is invalid. Fixed root/scratch buffers are local to this test adapter.
/// This creates no database, repairs no bytes and exposes no READY capability.
pub fn read_selected_page<W: WriterNamespace>(
    writer: &mut W,
    expected_namespace: ([u8; 16], u64),
    request: PageReadRequest,
    output: &mut [u8; 16_384],
) -> Result<(u64, usize), PageReadFailure> {
    let invalid = || PageReadFailure::InvalidRequest;
    let namespace = Namespace {
        database: DatabaseId::new(expected_namespace.0).map_err(|_| invalid())?,
        timeline: expected_namespace.1,
    };
    let expected = PageIdentity {
        database: DatabaseId::new(request.database).map_err(|_| invalid())?,
        page: PageRef {
            file: FileId::new(request.file).map_err(|_| invalid())?,
            page_no: request.page,
            incarnation: Generation::new(request.incarnation).map_err(|_| invalid())?,
        },
        owner: ObjectId::new(request.owner).map_err(|_| invalid())?,
        owner_generation: Generation::new(request.owner_generation).map_err(|_| invalid())?,
    };
    let kind = PageKind::from_byte(request.kind).map_err(|_| invalid())?;
    let mut root = [0; 16_384];
    let mut scratch = [0; 16_384];
    let mut reader = PageFileReader::open(writer, Some(namespace), &mut root, &mut scratch)?;
    let generation = reader.selector_generation();
    let page = reader.read_page(expected, kind, request.level, output)?;
    Ok((generation, page.len()))
}

/// Fixed native clean-buffer script operation; no transaction or write semantics.
#[derive(Clone, Copy, Debug)]
pub enum BufferTraceOperation {
    /// Acquire one pin into a currently empty handle slot.
    Pin {
        /// Destination pin slot in 0..4.
        slot: usize,
        /// Index into the supplied request slice.
        request: usize,
    },
    /// Read the full page through an existing pin.
    Read {
        /// Occupied pin slot in 0..4.
        slot: usize,
    },
    /// Release an occupied pin slot.
    Release {
        /// Occupied pin slot in 0..4.
        slot: usize,
    },
}

/// Error projection preserving source errors and buffer ownership failures.
#[derive(Debug, PartialEq, Eq)]
pub enum BufferTraceFailure {
    /// Bad script length/index, malformed identity or occupied/empty handle misuse.
    InvalidInput,
    /// Source reader failure, including selected control validation.
    Source(PageReadFailure),
    /// All candidate frames are pinned or loading.
    Busy,
    /// A physical cache location has a different expected identity/kind/level.
    IdentityMismatch,
    /// Checked pin count reached its maximum.
    PinLimit,
    /// No eligible frame generation can advance.
    GenerationExhausted,
    /// An internal lock/state failure fenced this instance.
    Poisoned,
    /// The owning storage session is fenced.
    Fenced,
}
impl From<crate::storage::read_buffer::BufferError> for BufferTraceFailure {
    fn from(error: crate::storage::read_buffer::BufferError) -> Self {
        use crate::storage::read_buffer::BufferError;
        match error {
            BufferError::Read(e) => Self::Source(e.into()),
            BufferError::InvalidCapacity => Self::InvalidInput,
            BufferError::BufferBusy => Self::Busy,
            BufferError::IdentityMismatch => Self::IdentityMismatch,
            BufferError::PinLimit => Self::PinLimit,
            BufferError::GenerationExhausted => Self::GenerationExhausted,
            BufferError::Poisoned => Self::Poisoned,
            BufferError::Fenced => Self::Fenced,
        }
    }
}

/// Full-image digest and independently inspectable envelope fields for one read.
#[derive(Debug, PartialEq, Eq)]
pub struct BufferTracePage {
    /// SHA256 of every byte in the 16 KiB image.
    pub digest: [u8; 32],
    /// Full database, file, page, incarnation, owner and owner-generation identity.
    pub identity: ([u8; 16], u64, u64, u64, u64, u64),
    /// Envelope kind and level.
    pub kind_level: (u8, u8),
    /// Envelope LSN and cell count; no logical row visibility claim.
    pub lsn_cells: (u64, usize),
}

/// Concrete adapter resource size, including its supplied writer/file types.
/// This does not bound hidden allocation or arbitrary third-party ports.
pub fn selected_buffer_pool_bytes<W: WriterNamespace>() -> usize {
    std::mem::size_of::<crate::storage::read_buffer::ReadBuffer<'_, '_, W>>()
}

/// Run at most 128 operations/requests with two frames and four pin slots.
/// Outer failure is construction/input rejection; each inner result corresponds
/// to one executed operation (release returns None). Pins are released on exit.
/// This test-only assembly owns bounded Vec output, unlike the buffer itself.
/// The namespace must remain unchanged until this call has fully returned.
pub fn run_selected_buffer_trace<W: WriterNamespace>(
    writer: &mut W,
    expected_namespace: ([u8; 16], u64),
    requests: &[PageReadRequest],
    operations: &[BufferTraceOperation],
) -> Result<Vec<Result<Option<BufferTracePage>, BufferTraceFailure>>, BufferTraceFailure> {
    use crate::storage::{
        page::PAGE_BYTES,
        read_buffer::{ReadBuffer, ReadFrame},
    };
    if operations.len() > 128 || requests.len() > 128 {
        return Err(BufferTraceFailure::InvalidInput);
    }
    let namespace = Namespace {
        database: DatabaseId::new(expected_namespace.0)
            .map_err(|_| BufferTraceFailure::InvalidInput)?,
        timeline: expected_namespace.1,
    };
    let mut root = [0; PAGE_BYTES];
    let mut scratch = [0; PAGE_BYTES];
    let mut load = [0; PAGE_BYTES];
    let mut frames = [ReadFrame::new(), ReadFrame::new()];
    let reader = PageFileReader::open(writer, Some(namespace), &mut root, &mut scratch)
        .map_err(|e| BufferTraceFailure::Source(e.into()))?;
    let buffer = ReadBuffer::new(reader, &mut frames, &mut load)?;
    let mut handles = [None, None, None, None];
    let mut trace = Vec::with_capacity(operations.len());
    for operation in operations {
        let result = (|| {
            let slot = match *operation {
                BufferTraceOperation::Pin { slot, .. }
                | BufferTraceOperation::Read { slot }
                | BufferTraceOperation::Release { slot } => slot,
            };
            let handle = handles
                .get_mut(slot)
                .ok_or(BufferTraceFailure::InvalidInput)?;
            match *operation {
                BufferTraceOperation::Pin { request, .. } => {
                    if handle.is_some() {
                        return Err(BufferTraceFailure::InvalidInput);
                    }
                    let request = requests
                        .get(request)
                        .ok_or(BufferTraceFailure::InvalidInput)?;
                    let identity = PageIdentity {
                        database: DatabaseId::new(request.database)
                            .map_err(|_| BufferTraceFailure::InvalidInput)?,
                        page: PageRef {
                            file: FileId::new(request.file)
                                .map_err(|_| BufferTraceFailure::InvalidInput)?,
                            page_no: request.page,
                            incarnation: Generation::new(request.incarnation)
                                .map_err(|_| BufferTraceFailure::InvalidInput)?,
                        },
                        owner: ObjectId::new(request.owner)
                            .map_err(|_| BufferTraceFailure::InvalidInput)?,
                        owner_generation: Generation::new(request.owner_generation)
                            .map_err(|_| BufferTraceFailure::InvalidInput)?,
                    };
                    let kind = PageKind::from_byte(request.kind)
                        .map_err(|_| BufferTraceFailure::InvalidInput)?;
                    *handle = Some(buffer.pin(identity, kind, request.level)?);
                }
                BufferTraceOperation::Release { .. } => {
                    if handle.take().is_none() {
                        return Err(BufferTraceFailure::InvalidInput);
                    }
                    return Ok(None);
                }
                BufferTraceOperation::Read { .. } => {}
            }
            let guard = handle
                .as_ref()
                .ok_or(BufferTraceFailure::InvalidInput)?
                .read()?;
            let page = guard.view()?;
            let header = page.header();
            let identity = header.identity;
            let mut reference = [0; 24];
            identity
                .page
                .encode(&mut reference)
                .map_err(|_| BufferTraceFailure::InvalidInput)?;
            let file = u64::from_le_bytes(
                reference[..8]
                    .try_into()
                    .map_err(|_| BufferTraceFailure::InvalidInput)?,
            );
            Ok(Some(BufferTracePage {
                digest: xadb_foundation::codec::content_digest(guard.bytes()),
                identity: (
                    identity.database.bytes(),
                    file,
                    identity.page.page_no,
                    identity.page.incarnation.get(),
                    identity.owner.get(),
                    identity.owner_generation.get(),
                ),
                kind_level: (header.kind as u8, header.level),
                lsn_cells: (header.lsn.0, page.len()),
            }))
        })();
        trace.push(result);
    }
    Ok(trace)
}
