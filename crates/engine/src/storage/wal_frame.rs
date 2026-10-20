//! DEV3 byte frames and page redo records, without append, allocation or recovery.
//! `PageLsn`'s complete-group end position is a logical LSN, never a byte offset;
//! frame lengths must not be used to issue or advance it. Valid bytes do not
//! establish the semantic atomic group, reservation, undo, or durability contract.
//! See `devdoc/contracts/dev3-wal-format.md`.
// Remove when the first interleaved WAL consumer is connected.
#![allow(dead_code)]

use super::{
    page::{DIGEST_OFFSET, PAGE_BYTES, PageIdentity, PageKind, PageRef, decode_page},
    transaction_record::{decode_txn_id, encode_txn_id},
};
use crate::{api::DatabaseId, transaction::TxnId};
use xadb_foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

pub(crate) const FRAME_HEADER_BYTES: usize = 160;
pub(crate) const FRAME_OVERHEAD_BYTES: usize = 192;
pub(crate) const MAX_FRAME_BYTES: usize = 1_048_576;
pub(crate) const REDO_HEADER_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WalError {
    InvalidInput,
    NoSpace,
    TooLarge,
    UnsupportedFormat,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    Truncated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FrameKind {
    Begin = 1,
    Update = 2,
    System = 3,
    Clr = 4,
    Commit = 5,
    AbortEnd = 6,
    Checkpoint = 7,
}
impl FrameKind {
    fn from_byte(value: u8) -> Result<Self, WalError> {
        match value {
            1 => Ok(Self::Begin),
            2 => Ok(Self::Update),
            3 => Ok(Self::System),
            4 => Ok(Self::Clr),
            5 => Ok(Self::Commit),
            6 => Ok(Self::AbortEnd),
            7 => Ok(Self::Checkpoint),
            _ => Err(WalError::UnsupportedFormat),
        }
    }
    fn needs_txn(self) -> bool {
        !matches!(self, Self::System | Self::Checkpoint)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalStreamIdentity {
    pub(crate) database: DatabaseId,
    pub(crate) timeline: u64,
    pub(crate) writer_epoch: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalFrameHeader {
    pub(crate) stream: WalStreamIdentity,
    pub(crate) kind: FrameKind,
    pub(crate) lsn: u64,
    pub(crate) previous_lsn: u64,
    pub(crate) txn: Option<TxnId>,
    pub(crate) previous_txn_lsn: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameExpectation {
    pub(crate) stream: WalStreamIdentity,
    pub(crate) previous_lsn: u64,
}
#[derive(Clone, Copy)]
pub(crate) enum FramePayload<'a> {
    Opaque(&'a [u8]),
    PageRedo(&'a [&'a [u8]]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TailPolicy {
    ForbidIncomplete,
    ActiveUnrequiredEof,
}
pub(crate) enum FrameScan<'a> {
    End,
    Incomplete,
    Complete {
        frame: FrameView<'a>,
        consumed: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RedoTarget {
    pub(crate) page: PageRef,
    pub(crate) owner: ObjectId,
    pub(crate) owner_generation: Generation,
    pub(crate) kind: PageKind,
    pub(crate) level: u8,
}
impl RedoTarget {
    fn identity(self, database: DatabaseId) -> PageIdentity {
        PageIdentity {
            database,
            page: self.page,
            owner: self.owner,
            owner_generation: self.owner_generation,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RedoKind {
    Patch,
    Init,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct RedoRecordView<'a> {
    pub(crate) kind: RedoKind,
    pub(crate) target: RedoTarget,
    pub(crate) prior_lsn: u64,
    pub(crate) offset: u16,
    pub(crate) data: &'a [u8],
}

pub(crate) struct FrameView<'a> {
    header: WalFrameHeader,
    payload: &'a [u8],
    redo_count: u16,
}
impl<'a> FrameView<'a> {
    pub(crate) fn header(&self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn payload(&self) -> &'a [u8] {
        self.payload
    }
    /// All records were validated before this immutable view was returned.
    pub(crate) fn redo_records(&self) -> RedoRecords<'a> {
        RedoRecords {
            remaining: if self.redo_count == 0 {
                &[]
            } else {
                self.payload
            },
        }
    }
}
pub(crate) struct RedoRecords<'a> {
    remaining: &'a [u8],
}
impl<'a> Iterator for RedoRecords<'a> {
    type Item = RedoRecordView<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let size = read_u32(self.remaining, 4) as usize;
        let (record, remaining) = self.remaining.split_at(size);
        self.remaining = remaining;
        Some(parse_record(record).expect("immutable records were validated before publication"))
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("checked length"),
    )
}
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked length"),
    )
}
fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("checked length"),
    )
}
fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
fn valid_stream(stream: WalStreamIdentity) -> bool {
    stream.timeline != 0 && stream.writer_epoch != 0
}
fn validate_header(header: WalFrameHeader) -> Result<(), WalError> {
    if !valid_stream(header.stream) || header.lsn == 0 || header.previous_lsn >= header.lsn {
        return Err(WalError::InvalidInput);
    }
    if header.kind.needs_txn() != header.txn.is_some() {
        return Err(WalError::InvalidInput);
    }
    if let Some(txn) = header.txn {
        let mut scratch = [0; 40];
        encode_txn_id(txn, &mut scratch).map_err(|_| WalError::InvalidInput)?;
        if txn.database != header.stream.database || txn.timeline != header.stream.timeline {
            return Err(WalError::InvalidInput);
        }
    }
    if header.kind == FrameKind::Begin || !header.kind.needs_txn() {
        if header.previous_txn_lsn != 0 {
            return Err(WalError::InvalidInput);
        }
    } else if header.previous_txn_lsn == 0 || header.previous_txn_lsn > header.previous_lsn {
        return Err(WalError::InvalidInput);
    }
    Ok(())
}
fn patch_shape(offset: u16, length: usize) -> Result<(), WalError> {
    if length > DIGEST_OFFSET - 72 {
        return Err(WalError::TooLarge);
    }
    if offset < 72
        || length == 0
        || usize::from(offset)
            .checked_add(length)
            .is_none_or(|end| end > DIGEST_OFFSET)
    {
        return Err(WalError::InvalidInput);
    }
    Ok(())
}
fn record_header(
    target: RedoTarget,
    kind: u8,
    prior_lsn: u64,
    offset: u16,
    length: usize,
) -> Result<[u8; REDO_HEADER_BYTES], WalError> {
    if !target.kind.accepts_level(target.level) {
        return Err(WalError::InvalidInput);
    }
    let mut bytes = [0; REDO_HEADER_BYTES];
    bytes[0] = kind;
    bytes[1] = target.kind as u8;
    bytes[2] = target.level;
    bytes[4..8].copy_from_slice(&((REDO_HEADER_BYTES + length) as u32).to_le_bytes());
    target
        .page
        .encode(&mut bytes[8..32])
        .map_err(|_| WalError::InvalidInput)?;
    put_u64(&mut bytes, 32, target.owner.get());
    put_u64(&mut bytes, 40, target.owner_generation.get());
    put_u64(&mut bytes, 48, prior_lsn);
    bytes[56..58].copy_from_slice(&offset.to_le_bytes());
    bytes[58..60].copy_from_slice(&(length as u16).to_le_bytes());
    Ok(bytes)
}
/// Write one after-byte record only after complete local validation.
pub(crate) fn encode_patch_record<'a>(
    target: RedoTarget,
    prior_lsn: u64,
    offset: u16,
    data: &[u8],
    output: &'a mut [u8],
) -> Result<&'a [u8], WalError> {
    patch_shape(offset, data.len())?;
    let header = record_header(target, 1, prior_lsn, offset, data.len())?;
    write_record(&header, data, output)
}
/// Validate the full INIT envelope before writing; this does not allocate a page.
pub(crate) fn encode_init_record<'a>(
    database: DatabaseId,
    lsn: u64,
    target: RedoTarget,
    image: &[u8],
    output: &'a mut [u8],
) -> Result<&'a [u8], WalError> {
    if lsn == 0 {
        return Err(WalError::InvalidInput);
    }
    validate_init(database, lsn, target, image).map_err(|_| WalError::InvalidInput)?;
    let header = record_header(target, 2, 0, 0, PAGE_BYTES)?;
    write_record(&header, image, output)
}
fn write_record<'a>(
    header: &[u8; REDO_HEADER_BYTES],
    data: &[u8],
    output: &'a mut [u8],
) -> Result<&'a [u8], WalError> {
    let length = REDO_HEADER_BYTES + data.len();
    if output.len() < length {
        return Err(WalError::NoSpace);
    }
    output[..REDO_HEADER_BYTES].copy_from_slice(header);
    output[REDO_HEADER_BYTES..length].copy_from_slice(data);
    Ok(&output[..length])
}
fn parse_record(bytes: &[u8]) -> Result<RedoRecordView<'_>, WalError> {
    if bytes.len() < REDO_HEADER_BYTES
        || read_u32(bytes, 4) as usize != bytes.len()
        || usize::from(read_u16(bytes, 58)) + REDO_HEADER_BYTES != bytes.len()
        || bytes[3] != 0
        || !zero(&bytes[60..64])
    {
        return Err(WalError::Corrupt);
    }
    let kind = match bytes[0] {
        1 => RedoKind::Patch,
        2 => RedoKind::Init,
        _ => return Err(WalError::UnsupportedFormat),
    };
    let target = RedoTarget {
        page: PageRef::decode(&bytes[8..32]).map_err(|_| WalError::Corrupt)?,
        owner: ObjectId::new(read_u64(bytes, 32)).map_err(|_| WalError::Corrupt)?,
        owner_generation: Generation::new(read_u64(bytes, 40)).map_err(|_| WalError::Corrupt)?,
        kind: PageKind::from_byte(bytes[1]).map_err(|_| WalError::Corrupt)?,
        level: bytes[2],
    };
    if !target.kind.accepts_level(target.level) {
        return Err(WalError::Corrupt);
    }
    let record = RedoRecordView {
        kind,
        target,
        prior_lsn: read_u64(bytes, 48),
        offset: read_u16(bytes, 56),
        data: &bytes[REDO_HEADER_BYTES..],
    };
    match kind {
        RedoKind::Patch => {
            patch_shape(record.offset, record.data.len()).map_err(|_| WalError::Corrupt)?
        }
        RedoKind::Init
            if record.prior_lsn != 0 || record.offset != 0 || record.data.len() != PAGE_BYTES =>
        {
            return Err(WalError::Corrupt);
        }
        RedoKind::Init => (),
    }
    Ok(record)
}
fn validate_init(
    database: DatabaseId,
    lsn: u64,
    target: RedoTarget,
    image: &[u8],
) -> Result<(), WalError> {
    let view = decode_page(image, target.identity(database), target.kind)
        .map_err(|_| WalError::Corrupt)?;
    if view.header().level != target.level || view.header().lsn.0 != lsn {
        return Err(WalError::Corrupt);
    }
    Ok(())
}
fn validate_frame_record(
    header: WalFrameHeader,
    bytes: &[u8],
    previous: &mut Option<(u64, u64)>,
) -> Result<(), WalError> {
    let record = parse_record(bytes)?;
    let address = (read_u64(bytes, 8), record.target.page.page_no);
    if previous.is_some_and(|last| last >= address) {
        return Err(WalError::Corrupt);
    }
    *previous = Some(address);
    match record.kind {
        RedoKind::Patch if record.prior_lsn >= header.lsn => Err(WalError::Corrupt),
        RedoKind::Patch => Ok(()),
        RedoKind::Init => validate_init(
            header.stream.database,
            header.lsn,
            record.target,
            record.data,
        ),
    }
}

/// Validate every input, including the final record, before returning exact bytes.
pub(crate) fn frame_encoded_len(
    header: WalFrameHeader,
    payload: FramePayload<'_>,
) -> Result<usize, WalError> {
    validate_header(header)?;
    let payload_bytes = match payload {
        FramePayload::Opaque(bytes) => bytes.len(),
        FramePayload::PageRedo(records) => {
            if records.is_empty() {
                return Err(WalError::InvalidInput);
            }
            if records.len() > usize::from(u16::MAX) {
                return Err(WalError::TooLarge);
            }
            let mut previous = None;
            let mut length = 0usize;
            for bytes in records {
                length = length
                    .checked_add(bytes.len())
                    .filter(|length| *length <= MAX_FRAME_BYTES - FRAME_OVERHEAD_BYTES)
                    .ok_or(WalError::TooLarge)?;
                validate_frame_record(header, bytes, &mut previous)
                    .map_err(|_| WalError::InvalidInput)?;
            }
            length
        }
    };
    payload_bytes
        .checked_add(FRAME_OVERHEAD_BYTES)
        .filter(|length| *length <= MAX_FRAME_BYTES)
        .ok_or(WalError::TooLarge)
}
/// No output byte changes on failure; bytes beyond the returned prefix survive.
pub(crate) fn encode_frame<'a>(
    header: WalFrameHeader,
    payload: FramePayload<'_>,
    output: &'a mut [u8],
) -> Result<&'a [u8], WalError> {
    let length = frame_encoded_len(header, payload)?;
    if output.len() < length {
        return Err(WalError::NoSpace);
    }
    let mut bytes = [0; FRAME_HEADER_BYTES];
    bytes[..4].copy_from_slice(b"XWF1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6] = header.kind as u8;
    bytes[8..10].copy_from_slice(&(FRAME_HEADER_BYTES as u16).to_le_bytes());
    bytes[12..16].copy_from_slice(&(length as u32).to_le_bytes());
    bytes[16..20].copy_from_slice(&((length - FRAME_OVERHEAD_BYTES) as u32).to_le_bytes());
    if let FramePayload::PageRedo(records) = payload {
        bytes[7] = 1;
        bytes[20..22].copy_from_slice(&(records.len() as u16).to_le_bytes());
    }
    bytes[24..40].copy_from_slice(&header.stream.database.bytes());
    put_u64(&mut bytes, 40, header.stream.timeline);
    bytes[48..52].copy_from_slice(&header.stream.writer_epoch.to_le_bytes());
    put_u64(&mut bytes, 56, header.lsn);
    put_u64(&mut bytes, 64, header.previous_lsn);
    if let Some(txn) = header.txn {
        encode_txn_id(txn, &mut bytes[72..112]).map_err(|_| WalError::InvalidInput)?;
    }
    put_u64(&mut bytes, 112, header.previous_txn_lsn);
    let digest = content_digest(&bytes[..128]);
    bytes[128..].copy_from_slice(&digest);
    output[..FRAME_HEADER_BYTES].copy_from_slice(&bytes);
    let mut cursor = FRAME_HEADER_BYTES;
    match payload {
        FramePayload::Opaque(data) => output[cursor..length - 32].copy_from_slice(data),
        FramePayload::PageRedo(records) => {
            for data in records {
                output[cursor..cursor + data.len()].copy_from_slice(data);
                cursor += data.len();
            }
        }
    }
    let digest = content_digest(&output[..length - 32]);
    output[length - 32..length].copy_from_slice(&digest);
    Ok(&output[..length])
}

/// Compare even a partially visible fixed field without reading beyond EOF.
fn visible_equals(
    bytes: &[u8],
    offset: usize,
    value: &[u8],
    error: WalError,
) -> Result<(), WalError> {
    if bytes.len() > offset {
        let visible = (bytes.len() - offset).min(value.len());
        if bytes[offset..offset + visible] != value[..visible] {
            return Err(error);
        }
    }
    Ok(())
}
fn validate_header_prefix(bytes: &[u8], expected: FrameExpectation) -> Result<(), WalError> {
    if !valid_stream(expected.stream) {
        return Err(WalError::InvalidInput);
    }
    visible_equals(bytes, 0, b"XWF1", WalError::Corrupt)?;
    visible_equals(bytes, 4, &1u16.to_le_bytes(), WalError::UnsupportedFormat)?;
    visible_equals(
        bytes,
        8,
        &(FRAME_HEADER_BYTES as u16).to_le_bytes(),
        WalError::Corrupt,
    )?;
    for (offset, length) in [(10, 2), (22, 2), (52, 4), (120, 8)] {
        visible_equals(bytes, offset, &[0; 8][..length], WalError::Corrupt)?;
    }
    visible_equals(
        bytes,
        24,
        &expected.stream.database.bytes(),
        WalError::IdentityMismatch,
    )?;
    visible_equals(
        bytes,
        40,
        &expected.stream.timeline.to_le_bytes(),
        WalError::IdentityMismatch,
    )?;
    visible_equals(
        bytes,
        48,
        &expected.stream.writer_epoch.to_le_bytes(),
        WalError::IdentityMismatch,
    )?;
    visible_equals(
        bytes,
        64,
        &expected.previous_lsn.to_le_bytes(),
        WalError::OrderMismatch,
    )?;
    if bytes.len() >= 8 && bytes[7] > 1 {
        return Err(WalError::UnsupportedFormat);
    }
    if bytes.len() >= 16
        && !(FRAME_OVERHEAD_BYTES..=MAX_FRAME_BYTES).contains(&(read_u32(bytes, 12) as usize))
    {
        return Err(WalError::Corrupt);
    }
    if bytes.len() >= 20
        && (read_u32(bytes, 16) as usize > MAX_FRAME_BYTES - FRAME_OVERHEAD_BYTES
            || read_u32(bytes, 16) as usize + FRAME_OVERHEAD_BYTES != read_u32(bytes, 12) as usize)
    {
        return Err(WalError::Corrupt);
    }
    if bytes.len() >= 22
        && ((bytes[7] == 0 && read_u16(bytes, 20) != 0)
            || (bytes[7] == 1 && read_u16(bytes, 20) == 0))
    {
        return Err(WalError::Corrupt);
    }
    if bytes.len() >= 64
        && (read_u64(bytes, 56) == 0 || read_u64(bytes, 56) <= expected.previous_lsn)
    {
        return Err(WalError::OrderMismatch);
    }
    if bytes.len() >= 7 {
        let kind = FrameKind::from_byte(bytes[6])?;
        if !kind.needs_txn() {
            visible_equals(bytes, 72, &[0; 40], WalError::Corrupt)?;
        } else {
            visible_equals(
                bytes,
                72,
                &expected.stream.database.bytes(),
                WalError::IdentityMismatch,
            )?;
            visible_equals(
                bytes,
                88,
                &expected.stream.timeline.to_le_bytes(),
                WalError::IdentityMismatch,
            )?;
            visible_equals(bytes, 100, &[0; 4], WalError::Corrupt)?;
            if bytes.len() >= 100 && read_u32(bytes, 96) == 0 {
                return Err(WalError::Corrupt);
            }
        }
        if kind == FrameKind::Begin || !kind.needs_txn() {
            visible_equals(bytes, 112, &[0; 8], WalError::Corrupt)?;
        } else if bytes.len() >= 120
            && (read_u64(bytes, 112) == 0 || read_u64(bytes, 112) > expected.previous_lsn)
        {
            return Err(WalError::Corrupt);
        }
        if bytes.len() >= 112 && kind.needs_txn() {
            let txn = decode_txn_id(&bytes[72..112]).map_err(|_| WalError::Corrupt)?;
            if txn.database != expected.stream.database || txn.timeline != expected.stream.timeline
            {
                return Err(WalError::IdentityMismatch);
            }
        }
    }
    if bytes.len() >= 128 {
        visible_equals(
            bytes,
            128,
            &content_digest(&bytes[..128]),
            WalError::Corrupt,
        )?;
    }
    Ok(())
}
fn parsed_header(bytes: &[u8], expected: FrameExpectation) -> WalFrameHeader {
    let kind = FrameKind::from_byte(bytes[6]).expect("validated header kind");
    WalFrameHeader {
        stream: expected.stream,
        kind,
        lsn: read_u64(bytes, 56),
        previous_lsn: expected.previous_lsn,
        txn: if kind.needs_txn() {
            Some(decode_txn_id(&bytes[72..112]).expect("validated transaction"))
        } else {
            None
        },
        previous_txn_lsn: read_u64(bytes, 112),
    }
}
fn complete_frame<'a>(
    bytes: &'a [u8],
    expected: FrameExpectation,
) -> Result<FrameView<'a>, WalError> {
    if content_digest(&bytes[..bytes.len() - 32]) != bytes[bytes.len() - 32..] {
        return Err(WalError::Corrupt);
    }
    let header = parsed_header(bytes, expected);
    let payload = &bytes[FRAME_HEADER_BYTES..bytes.len() - 32];
    let count = read_u16(bytes, 20);
    if bytes[7] == 1 {
        let mut previous = None;
        let mut cursor = 0usize;
        for _ in 0..count {
            if payload.len() - cursor < REDO_HEADER_BYTES {
                return Err(WalError::Corrupt);
            }
            let size = read_u32(payload, cursor + 4) as usize;
            let end = cursor
                .checked_add(size)
                .filter(|end| size >= REDO_HEADER_BYTES && *end <= payload.len())
                .ok_or(WalError::Corrupt)?;
            validate_frame_record(header, &payload[cursor..end], &mut previous)?;
            cursor = end;
        }
        if cursor != payload.len() {
            return Err(WalError::Corrupt);
        }
    }
    Ok(FrameView {
        header,
        payload,
        redo_count: count,
    })
}
/// Inspect the selected active EOF only when the caller has established that the
/// missing suffix is unrequired. This parser never truncates or resynchronizes.
pub(crate) fn scan_frame_prefix(
    input: &[u8],
    expected: FrameExpectation,
    policy: TailPolicy,
) -> Result<FrameScan<'_>, WalError> {
    if !valid_stream(expected.stream) {
        return Err(WalError::InvalidInput);
    }
    if input.is_empty() {
        return Ok(FrameScan::End);
    }
    validate_header_prefix(input, expected)?;
    if input.len() < FRAME_HEADER_BYTES || input.len() < read_u32(input, 12) as usize {
        return if policy == TailPolicy::ActiveUnrequiredEof {
            Ok(FrameScan::Incomplete)
        } else {
            Err(WalError::Truncated)
        };
    }
    let consumed = read_u32(input, 12) as usize;
    Ok(FrameScan::Complete {
        frame: complete_frame(&input[..consumed], expected)?,
        consumed,
    })
}
/// Exact one-frame input; following bytes are corruption, not silently ignored.
pub(crate) fn decode_frame(
    input: &[u8],
    expected: FrameExpectation,
) -> Result<FrameView<'_>, WalError> {
    match scan_frame_prefix(input, expected, TailPolicy::ForbidIncomplete)? {
        FrameScan::Complete { frame, consumed } if consumed == input.len() => Ok(frame),
        FrameScan::Complete { .. } => Err(WalError::Corrupt),
        FrameScan::End | FrameScan::Incomplete => Err(WalError::Truncated),
    }
}

#[cfg(test)]
#[path = "wal_frame_tests.rs"]
mod tests;
