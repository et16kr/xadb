//! Undo records and owned before-image chunks, without allocation or I/O.
//! Contract: `devdoc/contracts/dev3-undo-format.md`. Validated views are not pins,
//! lifetime grants, reservation descriptors, or authorization to apply/reclaim.
// Remove when the first undo/rollback storage consumer connects this format.
#![allow(dead_code)]

use super::{
    key::PhysicalKey,
    page::{PageError, PageHeader, PageIdentity, PageKind, PageRef, decode_page, encode_page},
    row_record::{CanonicalRowView, RowError, decode_canonical_row},
    transaction_record::{LogicalRowVersion, TxnRef, VersionStamp, decode_txn_id, encode_txn_id},
    undo_ref::UndoRef,
};
use crate::{
    api::{Column, Namespace},
    transaction::TxnId,
};
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectId},
};

pub(crate) const UNDO_RECORD_BYTES: usize = 432;
pub(crate) const UNDO_CHUNK_HEADER_BYTES: usize = 160;
pub(crate) const UNDO_CHUNK_PAYLOAD: usize = 16_060;
pub(crate) const MAX_UNDO_VALUE: usize = 131_072;
pub(crate) const MAX_UNDO_CHUNKS: usize = 9;
pub(crate) const MAX_UNDO_SLOTS: usize = 37;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoError {
    InvalidInput,
    NoSpace,
    TooLarge,
    Corrupt,
    UnsupportedFormat,
    UnsupportedCatalogBody,
    IdentityMismatch,
    SequenceMismatch,
    StateMismatch,
    SchemaMismatch,
    Incomplete,
    Exhausted,
}

impl From<PageError> for UndoError {
    fn from(error: PageError) -> Self {
        match error {
            PageError::InvalidInput => Self::InvalidInput,
            PageError::UnsupportedFormat => Self::UnsupportedFormat,
            PageError::IdentityMismatch | PageError::KindMismatch => Self::IdentityMismatch,
            PageError::NoSpace => Self::NoSpace,
            PageError::Corrupt => Self::Corrupt,
        }
    }
}

impl From<RowError> for UndoError {
    fn from(error: RowError) -> Self {
        match error {
            RowError::InvalidInput => Self::InvalidInput,
            RowError::SchemaMismatch => Self::SchemaMismatch,
            RowError::TooLarge => Self::TooLarge,
            _ => Self::Corrupt,
        }
    }
}

fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or([0; 4]))
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap_or([0; 8]))
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}
fn in_namespace(id: TxnId, namespace: Namespace) -> bool {
    id.database == namespace.database && id.timeline == namespace.timeline
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum UndoTargetKind {
    Row = 1,
    Index = 2,
    Catalog = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoTargetView<'a> {
    pub(crate) kind: UndoTargetKind,
    pub(crate) owner: ObjectId,
    pub(crate) generation: Generation,
    pub(crate) entity: u64,
    pub(crate) arity: u8,
    pub(crate) key: &'a [u8],
}

impl<'a> UndoTargetView<'a> {
    fn validate(self) -> Result<(), UndoError> {
        if self.entity == 0 {
            return Err(UndoError::InvalidInput);
        }
        match self.kind {
            UndoTargetKind::Index => {
                let key = PhysicalKey::decode(self.key, usize::from(self.arity))
                    .map_err(|_| UndoError::InvalidInput)?;
                if key.row().get() != self.entity {
                    return Err(UndoError::IdentityMismatch);
                }
            }
            _ if self.arity != 0 || !self.key.is_empty() => {
                return Err(UndoError::InvalidInput);
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn decode(bytes: &'a [u8], expected: UndoTargetView<'_>) -> Result<Self, UndoError> {
        if bytes.len() < 32 {
            return Err(UndoError::Corrupt);
        }
        let target = Self::parse(&bytes[..32], &bytes[32..])?;
        expected.validate()?;
        if target != expected {
            return Err(UndoError::IdentityMismatch);
        }
        Ok(target)
    }

    fn parse(core: &[u8], key: &'a [u8]) -> Result<Self, UndoError> {
        if core.len() != 32 || !zero(&core[4..8]) || usize::from(u16_at(core, 2)) != key.len() {
            return Err(UndoError::Corrupt);
        }
        let target = Self {
            kind: match core[0] {
                1 => UndoTargetKind::Row,
                2 => UndoTargetKind::Index,
                3 => UndoTargetKind::Catalog,
                _ => return Err(UndoError::Corrupt),
            },
            arity: core[1],
            owner: ObjectId::new(u64_at(core, 8)).map_err(|_| UndoError::Corrupt)?,
            generation: Generation::new(u64_at(core, 16)).map_err(|_| UndoError::Corrupt)?,
            entity: u64_at(core, 24),
            key,
        };
        target.validate().map_err(|_| UndoError::Corrupt)?;
        Ok(target)
    }

    fn write_core(self, out: &mut [u8]) {
        out.fill(0);
        out[0] = self.kind as u8;
        out[1] = self.arity;
        put16(out, 2, self.key.len() as u16);
        put64(out, 8, self.owner.get());
        put64(out, 16, self.generation.get());
        put64(out, 24, self.entity);
    }
}

pub(crate) fn encode_undo_target(
    target: UndoTargetView<'_>,
    expected: UndoTargetView<'_>,
    output: &mut [u8],
) -> Result<usize, UndoError> {
    target.validate()?;
    expected.validate()?;
    if target != expected {
        return Err(UndoError::IdentityMismatch);
    }
    let len = 32 + target.key.len();
    if output.len() < len {
        return Err(UndoError::NoSpace);
    }
    target.write_core(&mut output[..32]);
    output[32..len].copy_from_slice(target.key);
    Ok(len)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoLink {
    pub(crate) reference: UndoRef,
    pub(crate) sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoEdge {
    Never,
    Linked(UndoLink),
    Cut,
    Released,
}
impl UndoEdge {
    fn tag(self) -> u8 {
        match self {
            Self::Never => 0,
            Self::Linked(_) => 1,
            Self::Cut => 2,
            Self::Released => 3,
        }
    }
    fn decode(tag: u8, bytes: &[u8]) -> Result<Self, UndoError> {
        if tag == 1 {
            return Ok(Self::Linked(UndoLink {
                reference: UndoRef::decode(&bytes[..40])?,
                sequence: u64_at(bytes, 40),
            }));
        }
        if !zero(bytes) {
            return Err(UndoError::Corrupt);
        }
        match tag {
            0 => Ok(Self::Never),
            2 => Ok(Self::Cut),
            3 => Ok(Self::Released),
            _ => Err(UndoError::Corrupt),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoEdgeRole {
    Transaction,
    Version,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoCreator {
    pub(crate) id: TxnId,
    pub(crate) status_generation: Generation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum UndoBeforeKind {
    Missing = 0,
    RowDeleted = 1,
    RowValue = 2,
    IndexAbsent = 3,
    IndexPresent = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoPayloadHead {
    root: PageRef,
    total: usize,
    digest: [u8; 32],
}
impl UndoPayloadHead {
    pub(crate) fn new(root: PageRef, total: usize, digest: [u8; 32]) -> Result<Self, UndoError> {
        if !(2049..=MAX_UNDO_VALUE).contains(&total) {
            return Err(UndoError::InvalidInput);
        }
        Ok(Self {
            root,
            total,
            digest,
        })
    }
    pub(crate) fn root(self) -> PageRef {
        self.root
    }
    pub(crate) fn total(self) -> usize {
        self.total
    }
    pub(crate) fn count(self) -> usize {
        self.total.div_ceil(UNDO_CHUNK_PAYLOAD)
    }
    pub(crate) fn digest(self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<usize, UndoError> {
        if output.len() < 64 {
            return Err(UndoError::NoSpace);
        }
        self.root.encode(&mut output[..24])?;
        put32(output, 24, self.total);
        put16(output, 28, self.count() as u16);
        output[30..32].fill(0);
        output[32..64].copy_from_slice(&self.digest);
        Ok(64)
    }
    pub(crate) fn decode(input: &[u8]) -> Result<Self, UndoError> {
        if input.len() != 64 || !zero(&input[30..32]) {
            return Err(UndoError::Corrupt);
        }
        let mut digest = [0; 32];
        digest.copy_from_slice(&input[32..64]);
        let head = Self::new(
            PageRef::decode(&input[..24])?,
            u32_at(input, 24) as usize,
            digest,
        )
        .map_err(|_| UndoError::Corrupt)?;
        if usize::from(u16_at(input, 28)) != head.count() {
            return Err(UndoError::Corrupt);
        }
        Ok(head)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoPayload<'a> {
    None,
    Inline(&'a [u8]),
    Chunks(UndoPayloadHead),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoRelease {
    pub(crate) cursor: u8,
    pub(crate) chunks_released: u16,
    pub(crate) next: Option<PageRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoRecord<'a> {
    pub(crate) creator: UndoCreator,
    pub(crate) sequence: u64,
    pub(crate) mutation_sequence: u64,
    pub(crate) incoming: u64,
    pub(crate) target: UndoTargetView<'a>,
    /// Transaction first, version second. Equal targets are two edges.
    pub(crate) edges: [UndoEdge; 2],
    pub(crate) before_kind: UndoBeforeKind,
    pub(crate) before_stamp: Option<VersionStamp>,
    pub(crate) before_logical: Option<LogicalRowVersion>,
    pub(crate) payload: UndoPayload<'a>,
    pub(crate) release: UndoRelease,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoCell<'a> {
    Free(Generation),
    Live(UndoRecord<'a>),
    Releasing(UndoRecord<'a>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoRecordExpectation<'a> {
    pub(crate) namespace: Namespace,
    pub(crate) reference: UndoRef,
    pub(crate) target: UndoTargetView<'a>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct UndoRecordView<'a> {
    cell: UndoCell<'a>,
    namespace: Namespace,
    reference: UndoRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UndoRecordState {
    Free,
    Live,
    Releasing,
}

/// Diagnostic identity and current counts. This omits the before image and
/// stamp, so observing a Releasing record cannot restore its old version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoRecordMetadata<'a> {
    pub(crate) creator: UndoCreator,
    pub(crate) sequence: u64,
    pub(crate) mutation_sequence: u64,
    pub(crate) incoming: u64,
    pub(crate) target: UndoTargetView<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoReleaseProgress {
    pub(crate) progress: UndoRelease,
    pub(crate) chunk_count: usize,
}

impl<'a> UndoRecordView<'a> {
    pub(crate) fn reference(self) -> UndoRef {
        self.reference
    }
    pub(crate) fn state(self) -> UndoRecordState {
        match self.cell {
            UndoCell::Free(_) => UndoRecordState::Free,
            UndoCell::Live(_) => UndoRecordState::Live,
            UndoCell::Releasing(_) => UndoRecordState::Releasing,
        }
    }
    /// Free retains the previous occupant's generation; observing it does not
    /// authorize slot reuse or generation advancement.
    pub(crate) fn generation(self) -> Generation {
        self.reference.generation()
    }
    pub(crate) fn metadata(self) -> Result<UndoRecordMetadata<'a>, UndoError> {
        let record = match self.cell {
            UndoCell::Free(_) => return Err(UndoError::StateMismatch),
            UndoCell::Live(record) | UndoCell::Releasing(record) => record,
        };
        Ok(UndoRecordMetadata {
            creator: record.creator,
            sequence: record.sequence,
            mutation_sequence: record.mutation_sequence,
            incoming: record.incoming,
            target: record.target,
        })
    }
    /// Queue metadata is available only during release. The saved next identity
    /// and count are not an allocator free token or an exactly-once proof.
    pub(crate) fn release_progress(self) -> Result<UndoReleaseProgress, UndoError> {
        let UndoCell::Releasing(record) = self.cell else {
            return Err(UndoError::StateMismatch);
        };
        Ok(UndoReleaseProgress {
            progress: record.release,
            chunk_count: match record.payload {
                UndoPayload::Chunks(head) => head.count(),
                _ => 0,
            },
        })
    }
    /// Releasing metadata cannot be used as a reader/rollback before image.
    pub(crate) fn live_before(self) -> Result<UndoRecord<'a>, UndoError> {
        match self.cell {
            UndoCell::Live(record) => Ok(record),
            _ => Err(UndoError::StateMismatch),
        }
    }
    pub(crate) fn linked_edges(self) -> [Option<(UndoEdgeRole, UndoLink)>; 2] {
        let record = match self.cell {
            UndoCell::Free(_) => return [None; 2],
            UndoCell::Live(r) | UndoCell::Releasing(r) => r,
        };
        [UndoEdgeRole::Transaction, UndoEdgeRole::Version].map(|role| {
            let index = usize::from(role == UndoEdgeRole::Version);
            match record.edges[index] {
                UndoEdge::Linked(link) => Some((role, link)),
                _ => None,
            }
        })
    }
    pub(crate) fn before_status_ref(self) -> Option<TxnRef> {
        let record = match self.cell {
            UndoCell::Free(_) => return None,
            UndoCell::Live(r) | UndoCell::Releasing(r) => r,
        };
        match record.before_stamp {
            Some(VersionStamp::Transaction(reference)) => Some(reference),
            _ => None,
        }
    }
}

fn expectation_valid(expected: UndoRecordExpectation<'_>) -> Result<(), UndoError> {
    if expected.namespace.timeline == 0 || usize::from(expected.reference.slot()) >= MAX_UNDO_SLOTS
    {
        return Err(UndoError::InvalidInput);
    }
    expected.target.validate()
}
fn validate_record(
    record: UndoRecord<'_>,
    releasing: bool,
    expected: UndoRecordExpectation<'_>,
    columns: &[Column],
) -> Result<usize, UndoError> {
    record.target.validate()?;
    if record.target.kind == UndoTargetKind::Catalog {
        return Err(UndoError::UnsupportedCatalogBody);
    }
    if record.target != expected.target || !in_namespace(record.creator.id, expected.namespace) {
        return Err(UndoError::IdentityMismatch);
    }
    // Validate the existing public TxnId value, not a second identity codec.
    TxnRef::new(record.creator.id, record.creator.status_generation)?;
    if record.sequence == 0 || record.mutation_sequence == 0 {
        return Err(UndoError::SequenceMismatch);
    }
    for edge in record.edges {
        if let UndoEdge::Linked(link) = edge {
            if link.sequence == 0 || link.sequence >= record.sequence {
                return Err(UndoError::SequenceMismatch);
            }
            if same_address(link.reference.page(), expected.reference.page())
                && link.reference.slot() == expected.reference.slot()
            {
                return Err(UndoError::IdentityMismatch);
            }
        }
    }
    let progress = record.release;
    let [txn, version] = record.edges;
    if releasing {
        let valid = match progress.cursor {
            0 => txn != UndoEdge::Released && version != UndoEdge::Released,
            1 => txn != UndoEdge::Released && version == UndoEdge::Released,
            2 | 3 => txn == UndoEdge::Released && version == UndoEdge::Released,
            _ => false,
        };
        if record.incoming != 0 || !valid {
            return Err(UndoError::StateMismatch);
        }
    } else if progress.cursor != 0 || txn == UndoEdge::Released || version == UndoEdge::Released {
        return Err(UndoError::StateMismatch);
    }
    let missing = record.before_kind == UndoBeforeKind::Missing;
    let matching_kind = match record.target.kind {
        UndoTargetKind::Row => matches!(
            record.before_kind,
            UndoBeforeKind::Missing | UndoBeforeKind::RowDeleted | UndoBeforeKind::RowValue
        ),
        UndoTargetKind::Index => matches!(
            record.before_kind,
            UndoBeforeKind::Missing | UndoBeforeKind::IndexAbsent | UndoBeforeKind::IndexPresent
        ),
        UndoTargetKind::Catalog => false,
    };
    if !matching_kind {
        return Err(UndoError::InvalidInput);
    }
    let stamp_released = releasing && progress.cursor == 3;
    if missing {
        let expected_edge = if releasing && progress.cursor >= 1 {
            UndoEdge::Released
        } else {
            UndoEdge::Never
        };
        if record.before_stamp.is_some()
            || record.before_logical.is_some()
            || version != expected_edge
        {
            return Err(UndoError::StateMismatch);
        }
    } else {
        let logical = record.before_logical.ok_or(UndoError::InvalidInput)?;
        if !in_namespace(logical.transaction(), expected.namespace) {
            return Err(UndoError::IdentityMismatch);
        }
        if logical.transaction() == record.creator.id
            && logical.mutation_sequence() >= record.mutation_sequence
        {
            return Err(UndoError::SequenceMismatch);
        }
        if stamp_released {
            if record.before_stamp.is_some() {
                return Err(UndoError::StateMismatch);
            }
        } else {
            match record.before_stamp.ok_or(UndoError::InvalidInput)? {
                VersionStamp::Committed(_) => {}
                VersionStamp::Transaction(reference)
                    if reference.id() == record.creator.id
                        && reference.generation() == record.creator.status_generation
                        && logical.transaction() == reference.id() => {}
                _ => return Err(UndoError::IdentityMismatch),
            }
        }
    }
    let inline = match record.payload {
        UndoPayload::None if record.before_kind != UndoBeforeKind::RowValue => 0,
        UndoPayload::Inline(bytes)
            if record.before_kind == UndoBeforeKind::RowValue
                && (5..=2048).contains(&bytes.len()) =>
        {
            decode_canonical_row(bytes, columns)?;
            bytes.len()
        }
        UndoPayload::Chunks(head) if record.before_kind == UndoBeforeKind::RowValue => {
            if same_address(head.root, expected.reference.page()) {
                return Err(UndoError::IdentityMismatch);
            }
            0
        }
        _ => return Err(UndoError::InvalidInput),
    };
    match record.payload {
        UndoPayload::Chunks(head) if stamp_released => {
            let count = usize::from(progress.chunks_released);
            if count > head.count() {
                return Err(UndoError::StateMismatch);
            }
            match progress.next {
                None if count == head.count() => {}
                Some(next)
                    if count < head.count()
                        && !same_address(next, expected.reference.page())
                        && ((count == 0 && next == head.root)
                            || (count > 0 && !same_address(next, head.root))) => {}
                _ => return Err(UndoError::StateMismatch),
            }
        }
        _ if progress.chunks_released == 0 && progress.next.is_none() => {}
        _ => return Err(UndoError::StateMismatch),
    }
    Ok(UNDO_RECORD_BYTES + record.target.key.len() + inline)
}

pub(crate) fn undo_record_encoded_len(
    cell: UndoCell<'_>,
    expected: UndoRecordExpectation<'_>,
    columns: &[Column],
) -> Result<usize, UndoError> {
    expectation_valid(expected)?;
    match cell {
        UndoCell::Free(generation) if generation == expected.reference.generation() => {
            Ok(UNDO_RECORD_BYTES)
        }
        UndoCell::Free(_) => Err(UndoError::IdentityMismatch),
        UndoCell::Live(r) => validate_record(r, false, expected, columns),
        UndoCell::Releasing(r) => validate_record(r, true, expected, columns),
    }
}

/// All input and destination bounds are checked before the first caller write.
pub(crate) fn encode_undo_record(
    cell: UndoCell<'_>,
    expected: UndoRecordExpectation<'_>,
    columns: &[Column],
    output: &mut [u8],
) -> Result<usize, UndoError> {
    let length = undo_record_encoded_len(cell, expected, columns)?;
    if output.len() < length {
        return Err(UndoError::NoSpace);
    }
    let mut header = [0; UNDO_RECORD_BYTES];
    header[..4].copy_from_slice(b"XUR1");
    put16(&mut header, 4, 1);
    put64(&mut header, 8, expected.reference.generation().get());
    let (key, inline) = match cell {
        UndoCell::Free(_) => (&[][..], &[][..]),
        UndoCell::Live(record) | UndoCell::Releasing(record) => {
            header[6] = if matches!(cell, UndoCell::Live(_)) {
                1
            } else {
                2
            };
            put64(&mut header, 16, record.sequence);
            encode_txn_id(record.creator.id, &mut header[24..64])?;
            put64(&mut header, 64, record.creator.status_generation.get());
            put64(&mut header, 72, record.mutation_sequence);
            put64(&mut header, 80, record.incoming);
            record.target.write_core(&mut header[88..120]);
            for (index, edge) in record.edges.into_iter().enumerate() {
                header[326 + index] = edge.tag();
                if let UndoEdge::Linked(link) = edge {
                    let at = 120 + index * 48;
                    link.reference.encode(&mut header[at..at + 40])?;
                    put64(&mut header, at + 40, link.sequence);
                }
            }
            if let Some(stamp) = record.before_stamp {
                stamp.encode(&mut header[216..272])?;
            }
            if let Some(logical) = record.before_logical {
                logical.encode(&mut header[272..320])?;
            }
            header[324] = record.before_kind as u8;
            let inline = match record.payload {
                UndoPayload::None => &[][..],
                UndoPayload::Inline(bytes) => {
                    header[325] = 1;
                    put32(&mut header, 320, bytes.len());
                    bytes
                }
                UndoPayload::Chunks(head) => {
                    header[325] = 2;
                    put32(&mut header, 320, head.total);
                    head.encode(&mut header[336..400])?;
                    &[][..]
                }
            };
            header[328] = record.release.cursor;
            put16(&mut header, 332, record.release.chunks_released);
            if let Some(next) = record.release.next {
                next.encode(&mut header[400..424])?;
            }
            (record.target.key, inline)
        }
    };
    output[..432].copy_from_slice(&header);
    output[432..432 + key.len()].copy_from_slice(key);
    output[432 + key.len()..length].copy_from_slice(inline);
    Ok(length)
}

pub(crate) fn decode_undo_record<'a>(
    input: &'a [u8],
    expected: UndoRecordExpectation<'_>,
    columns: &[Column],
) -> Result<UndoRecordView<'a>, UndoError> {
    expectation_valid(expected)?;
    if input.len() < 432 || &input[..4] != b"XUR1" {
        return Err(UndoError::Corrupt);
    }
    if u16_at(input, 4) != 1 {
        return Err(UndoError::UnsupportedFormat);
    }
    if input[7] != 0
        || !zero(&input[329..332])
        || !zero(&input[334..336])
        || !zero(&input[424..432])
    {
        return Err(UndoError::Corrupt);
    }
    let generation = Generation::new(u64_at(input, 8)).map_err(|_| UndoError::Corrupt)?;
    if generation != expected.reference.generation() {
        return Err(UndoError::IdentityMismatch);
    }
    let cell = if input[6] == 0 {
        if input.len() != 432 || !zero(&input[16..]) {
            return Err(UndoError::Corrupt);
        }
        UndoCell::Free(generation)
    } else {
        if input[6] != 1 && input[6] != 2 {
            return Err(UndoError::Corrupt);
        }
        let key_end = 432 + usize::from(u16_at(input, 90));
        if key_end > input.len() {
            return Err(UndoError::Corrupt);
        }
        let target = UndoTargetView::parse(&input[88..120], &input[432..key_end])?;
        let before_kind = match input[324] {
            0 => UndoBeforeKind::Missing,
            1 => UndoBeforeKind::RowDeleted,
            2 => UndoBeforeKind::RowValue,
            3 => UndoBeforeKind::IndexAbsent,
            4 => UndoBeforeKind::IndexPresent,
            _ => return Err(UndoError::Corrupt),
        };
        let total = u32_at(input, 320) as usize;
        let payload = match input[325] {
            0 if total == 0 && input.len() == key_end && zero(&input[336..400]) => {
                UndoPayload::None
            }
            1 if input.len() - key_end == total && zero(&input[336..400]) => {
                UndoPayload::Inline(&input[key_end..])
            }
            2 if input.len() == key_end => {
                let head = UndoPayloadHead::decode(&input[336..400])?;
                if total != head.total {
                    return Err(UndoError::Corrupt);
                }
                UndoPayload::Chunks(head)
            }
            _ => return Err(UndoError::Corrupt),
        };
        let record = UndoRecord {
            creator: UndoCreator {
                id: decode_txn_id(&input[24..64])?,
                status_generation: Generation::new(u64_at(input, 64))
                    .map_err(|_| UndoError::Corrupt)?,
            },
            sequence: u64_at(input, 16),
            mutation_sequence: u64_at(input, 72),
            incoming: u64_at(input, 80),
            target,
            edges: [
                UndoEdge::decode(input[326], &input[120..168])?,
                UndoEdge::decode(input[327], &input[168..216])?,
            ],
            before_kind,
            before_stamp: if zero(&input[216..272]) {
                None
            } else {
                Some(VersionStamp::decode(&input[216..272])?)
            },
            before_logical: if zero(&input[272..320]) {
                None
            } else {
                Some(LogicalRowVersion::decode(&input[272..320])?)
            },
            payload,
            release: UndoRelease {
                cursor: input[328],
                chunks_released: u16_at(input, 332),
                next: if zero(&input[400..424]) {
                    None
                } else {
                    Some(PageRef::decode(&input[400..424])?)
                },
            },
        };
        validate_record(record, input[6] == 2, expected, columns).map_err(|error| match error {
            UndoError::InvalidInput => UndoError::Corrupt,
            other => other,
        })?;
        if input[6] == 1 {
            UndoCell::Live(record)
        } else {
            UndoCell::Releasing(record)
        }
    };
    Ok(UndoRecordView {
        cell,
        namespace: expected.namespace,
        reference: expected.reference,
    })
}

/// Validate the selected root slot; other schemas and global graph integrity
/// remain the caller's responsibility. Free is inspection-only, never a lookup.
pub(crate) fn lookup_undo_record<'a>(
    input: &'a [u8],
    identity: PageIdentity,
    expected: UndoRecordExpectation<'_>,
    columns: &[Column],
) -> Result<UndoRecordView<'a>, UndoError> {
    expectation_valid(expected)?;
    if identity.database != expected.namespace.database
        || identity.page != expected.reference.page()
    {
        return Err(UndoError::IdentityMismatch);
    }
    let page = decode_page(input, identity, PageKind::Undo)?;
    if page.len() > MAX_UNDO_SLOTS {
        return Err(UndoError::Corrupt);
    }
    for slot in 0..page.len() {
        let bytes = page.cell(slot).ok_or(UndoError::Corrupt)?;
        if bytes.len() < 432 || &bytes[..4] != b"XUR1" {
            return Err(UndoError::Corrupt);
        }
    }
    let cell = page
        .cell(usize::from(expected.reference.slot()))
        .ok_or(UndoError::IdentityMismatch)?;
    let view = decode_undo_record(cell, expected, columns)?;
    if matches!(view.cell, UndoCell::Free(_)) {
        return Err(UndoError::StateMismatch);
    }
    Ok(view)
}

pub(crate) fn validate_loaded_edge(
    source: UndoRecordView<'_>,
    role: UndoEdgeRole,
    target: UndoRecordView<'_>,
) -> Result<(), UndoError> {
    let record = match source.cell {
        UndoCell::Live(r) | UndoCell::Releasing(r) => r,
        _ => return Err(UndoError::StateMismatch),
    };
    let other = target.live_before()?;
    let edge = record.edges[usize::from(role == UndoEdgeRole::Version)];
    let UndoEdge::Linked(link) = edge else {
        return Err(UndoError::StateMismatch);
    };
    if source.namespace != target.namespace || link.reference != target.reference {
        return Err(UndoError::IdentityMismatch);
    }
    if other.sequence != link.sequence || other.sequence >= record.sequence {
        return Err(UndoError::SequenceMismatch);
    }
    match role {
        UndoEdgeRole::Transaction
            if other.creator == record.creator
                && other.mutation_sequence <= record.mutation_sequence =>
        {
            Ok(())
        }
        UndoEdgeRole::Version
            if other.target == record.target
                && record.before_logical.is_some_and(|logical| {
                    logical.transaction() == other.creator.id
                        && logical.mutation_sequence() == other.mutation_sequence
                }) =>
        {
            Ok(())
        }
        _ => Err(UndoError::IdentityMismatch),
    }
}

pub(crate) fn incoming_increment(count: u64, edges: u64) -> Result<u64, UndoError> {
    count.checked_add(edges).ok_or(UndoError::Exhausted)
}
pub(crate) fn incoming_decrement(count: u64, edges: u64) -> Result<u64, UndoError> {
    count.checked_sub(edges).ok_or(UndoError::StateMismatch)
}
pub(crate) fn next_slot_generation(cell: UndoCell<'_>) -> Result<Generation, UndoError> {
    let UndoCell::Free(old) = cell else {
        return Err(UndoError::StateMismatch);
    };
    Generation::new(old.get().checked_add(1).ok_or(UndoError::Exhausted)?)
        .map_err(|_| UndoError::Exhausted)
}

/// Stable expected ownership copied from a validated root, never chunk bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoChunkContext {
    namespace: Namespace,
    record: UndoRef,
    sequence: u64,
    store: PageIdentity,
    head: UndoPayloadHead,
}
impl UndoChunkContext {
    pub(crate) fn from_record(
        view: UndoRecordView<'_>,
        store: PageIdentity,
    ) -> Result<Self, UndoError> {
        let record = match view.cell {
            UndoCell::Live(r) | UndoCell::Releasing(r) => r,
            _ => return Err(UndoError::StateMismatch),
        };
        let UndoPayload::Chunks(head) = record.payload else {
            return Err(UndoError::StateMismatch);
        };
        if store.database != view.namespace.database || store.page != view.reference.page() {
            return Err(UndoError::IdentityMismatch);
        }
        Ok(Self {
            namespace: view.namespace,
            record: view.reference,
            sequence: record.sequence,
            store,
            head,
        })
    }
    pub(crate) fn page_identity(self, page: PageRef) -> PageIdentity {
        PageIdentity { page, ..self.store }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UndoChunkView<'a> {
    context: UndoChunkContext,
    current: PageRef,
    index: usize,
    next: Option<PageRef>,
    payload: &'a [u8],
}
impl<'a> UndoChunkView<'a> {
    pub(crate) fn payload(self) -> &'a [u8] {
        self.payload
    }
    pub(crate) fn next(self) -> Option<PageRef> {
        self.next
    }
}

fn chunk_position(
    context: UndoChunkContext,
    current: PageRef,
    index: usize,
    next: Option<PageRef>,
    payload_len: usize,
) -> Result<(), UndoError> {
    let head = context.head;
    if index >= head.count()
        || payload_len != (head.total - index * UNDO_CHUNK_PAYLOAD).min(UNDO_CHUNK_PAYLOAD)
        || same_address(current, context.record.page())
        || (index == 0 && current != head.root)
        || (index > 0 && same_address(current, head.root))
    {
        return Err(UndoError::InvalidInput);
    }
    match next {
        None if index + 1 == head.count() => Ok(()),
        Some(next)
            if index + 1 < head.count()
                && !same_address(next, current)
                && !same_address(next, head.root)
                && !same_address(next, context.record.page()) =>
        {
            Ok(())
        }
        _ => Err(UndoError::InvalidInput),
    }
}

pub(crate) fn encode_undo_chunk_cell(
    context: UndoChunkContext,
    current: PageRef,
    index: usize,
    next: Option<PageRef>,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, UndoError> {
    chunk_position(context, current, index, next, payload.len())?;
    let len = UNDO_CHUNK_HEADER_BYTES + payload.len();
    if output.len() < len {
        return Err(UndoError::NoSpace);
    }
    let mut header = [0; UNDO_CHUNK_HEADER_BYTES];
    header[..4].copy_from_slice(b"XUC1");
    put16(&mut header, 4, 1);
    put16(&mut header, 6, index as u16);
    put16(&mut header, 8, context.head.count() as u16);
    put32(&mut header, 12, context.head.total);
    put32(&mut header, 16, payload.len());
    context.record.encode(&mut header[24..64])?;
    put64(&mut header, 64, context.sequence);
    context.head.root.encode(&mut header[72..96])?;
    if let Some(next) = next {
        next.encode(&mut header[96..120])?;
    }
    header[120..152].copy_from_slice(&context.head.digest);
    put64(&mut header, 152, context.namespace.timeline);
    output[..160].copy_from_slice(&header);
    output[160..len].copy_from_slice(payload);
    Ok(len)
}

pub(crate) fn decode_undo_chunk_cell(
    input: &[u8],
    context: UndoChunkContext,
    current: PageRef,
    expected_index: usize,
) -> Result<UndoChunkView<'_>, UndoError> {
    if input.len() < 160 || &input[..4] != b"XUC1" {
        return Err(UndoError::Corrupt);
    }
    if u16_at(input, 4) != 1 {
        return Err(UndoError::UnsupportedFormat);
    }
    if !zero(&input[10..12])
        || !zero(&input[20..24])
        || u32_at(input, 16) as usize != input.len() - 160
    {
        return Err(UndoError::Corrupt);
    }
    let index = usize::from(u16_at(input, 6));
    let next = if zero(&input[96..120]) {
        None
    } else {
        Some(PageRef::decode(&input[96..120])?)
    };
    if UndoRef::decode(&input[24..64])? != context.record
        || PageRef::decode(&input[72..96])? != context.head.root
        || u64_at(input, 152) != context.namespace.timeline
    {
        return Err(UndoError::IdentityMismatch);
    }
    if u64_at(input, 64) != context.sequence {
        return Err(UndoError::SequenceMismatch);
    }
    if u32_at(input, 12) as usize != context.head.total
        || usize::from(u16_at(input, 8)) != context.head.count()
        || input[120..152] != context.head.digest
    {
        return Err(UndoError::Corrupt);
    }
    if index != expected_index {
        return Err(UndoError::SequenceMismatch);
    }
    chunk_position(context, current, index, next, input.len() - 160)
        .map_err(|_| UndoError::Corrupt)?;
    Ok(UndoChunkView {
        context,
        current,
        index,
        next,
        payload: &input[160..],
    })
}

pub(crate) fn encode_undo_chunk_page(
    header: PageHeader,
    context: UndoChunkContext,
    index: usize,
    cell: &[u8],
    output: &mut [u8],
) -> Result<(), UndoError> {
    if header.kind != PageKind::Undo
        || header.level != 0
        || header.identity != context.page_identity(header.identity.page)
    {
        return Err(UndoError::InvalidInput);
    }
    decode_undo_chunk_cell(cell, context, header.identity.page, index)?;
    encode_page(header, &[cell], output)?;
    Ok(())
}
pub(crate) fn decode_undo_chunk_page(
    input: &[u8],
    context: UndoChunkContext,
    current: PageRef,
    index: usize,
) -> Result<UndoChunkView<'_>, UndoError> {
    let page = decode_page(input, context.page_identity(current), PageKind::Undo)?;
    if page.len() != 1 {
        return Err(UndoError::Corrupt);
    }
    decode_undo_chunk_cell(
        page.cell(0).ok_or(UndoError::Corrupt)?,
        context,
        current,
        index,
    )
}

/// Streaming digest and at most nine physical addresses. A failed push changes
/// neither traversal state nor hash, including final digest failures.
pub(crate) struct UndoChunkVerifier {
    context: UndoChunkContext,
    index: usize,
    next: Option<PageRef>,
    seen: [Option<PageRef>; MAX_UNDO_CHUNKS],
    hasher: ContentHasher,
}
impl UndoChunkVerifier {
    pub(crate) fn new(context: UndoChunkContext) -> Self {
        Self {
            context,
            index: 0,
            next: Some(context.head.root),
            seen: [None; MAX_UNDO_CHUNKS],
            hasher: ContentHasher::new(),
        }
    }
    pub(crate) fn push(&mut self, view: UndoChunkView<'_>) -> Result<(), UndoError> {
        if self.index >= self.context.head.count()
            || view.context != self.context
            || view.index != self.index
            || Some(view.current) != self.next
            || self
                .seen
                .iter()
                .flatten()
                .any(|old| same_address(*old, view.current))
        {
            return Err(UndoError::SequenceMismatch);
        }
        let mut hasher = self.hasher.clone();
        hasher.update(view.payload);
        if self.index + 1 == self.context.head.count()
            && hasher.digest() != self.context.head.digest
        {
            return Err(UndoError::Corrupt);
        }
        self.seen[self.index] = Some(view.current);
        self.index += 1;
        self.next = view.next;
        self.hasher = hasher;
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<(), UndoError> {
        if self.index == self.context.head.count() && self.next.is_none() {
            Ok(())
        } else {
            Err(UndoError::Incomplete)
        }
    }
}

pub(crate) fn decode_materialized_before<'a>(
    view: UndoRecordView<'_>,
    input: &'a [u8],
    columns: &[Column],
) -> Result<CanonicalRowView<'a>, UndoError> {
    let record = view.live_before()?;
    let UndoPayload::Chunks(head) = record.payload else {
        return Err(UndoError::StateMismatch);
    };
    if input.len() != head.total || content_digest(input) != head.digest {
        return Err(UndoError::Corrupt);
    }
    Ok(decode_canonical_row(input, columns)?)
}
pub(crate) fn validate_release_chunk(
    view: UndoRecordView<'_>,
    chunk: UndoChunkView<'_>,
) -> Result<(), UndoError> {
    let UndoCell::Releasing(record) = view.cell else {
        return Err(UndoError::StateMismatch);
    };
    let context = UndoChunkContext::from_record(view, chunk.context.store)?;
    if record.release.cursor != 3
        || chunk.context != context
        || chunk.index != usize::from(record.release.chunks_released)
        || Some(chunk.current) != record.release.next
    {
        return Err(UndoError::StateMismatch);
    }
    Ok(())
}

#[cfg(test)]
#[path = "undo_record_tests.rs"]
mod tests;
