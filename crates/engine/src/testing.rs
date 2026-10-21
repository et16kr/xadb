//! Narrow native-test assembly for unfinished storage leaves, not DB readiness.
//! Available only with the explicit `test-support` feature.

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
