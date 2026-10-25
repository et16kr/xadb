//! Canonical values and versioned row/index payloads without storage effects.
//! Contract: `devdoc/contracts/dev3-row-format.md`.
// Remove when the first row/index storage consumer connects this representation.
#![allow(dead_code)]

use super::{
    overflow::{INLINE_ROW_BYTES, MAX_ROW_BYTES, OVERFLOW_HEAD_BYTES, OverflowHead},
    transaction_record::{LogicalRowVersion, VersionStamp},
    undo_ref::UndoRef,
};
use crate::api::{Column, DataType, Namespace};
use foundation::{
    codec::content_digest,
    identity::RowId,
    value::{MAX_VALUE_BYTES, Value},
};

pub(crate) const MAX_ROW_COLUMNS: usize = 32;
pub(crate) const ROW_HEADER_BYTES: usize = 168;
pub(crate) const INDEX_RECORD_BYTES: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowError {
    InvalidInput,
    Corrupt,
    UnsupportedFormat,
    SchemaMismatch,
    IdentityMismatch,
    TooLarge,
    NoSpace,
    OverflowMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueRef<'a> {
    Null,
    Bool(bool),
    Int64(i64),
    Utf8(&'a str),
    Binary(&'a [u8]),
}

fn schema_valid(columns: &[Column]) -> Result<(), RowError> {
    if !(1..=MAX_ROW_COLUMNS).contains(&columns.len()) {
        return Err(RowError::InvalidInput);
    }
    Ok(())
}

fn matches(column: &Column, value: ValueRef<'_>) -> bool {
    match (column.data_type, value) {
        (_, ValueRef::Null) => column.nullable,
        (DataType::Bool, ValueRef::Bool(_))
        | (DataType::Int64, ValueRef::Int64(_))
        | (DataType::Utf8, ValueRef::Utf8(_))
        | (DataType::Binary, ValueRef::Binary(_)) => true,
        _ => false,
    }
}

fn borrowed(value: &Value) -> ValueRef<'_> {
    match value {
        Value::Null => ValueRef::Null,
        Value::Bool(value) => ValueRef::Bool(*value),
        Value::Int64(value) => ValueRef::Int64(*value),
        Value::Utf8(value) => ValueRef::Utf8(value),
        Value::Bytes(value) => ValueRef::Binary(value),
    }
}

fn u32_at(input: &[u8], offset: usize) -> u32 {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(&input[offset..offset + 4]);
    u32::from_le_bytes(bytes)
}

fn u64_at(input: &[u8], offset: usize) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&input[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}

fn put_u32(output: &mut [u8], offset: usize, value: usize) {
    output[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
}

fn zero(input: &[u8]) -> bool {
    input.iter().all(|byte| *byte == 0)
}

/// Parse the wire value before considering its schema. This rejects malformed
/// Bool/UTF-8 even if a changed tag would also produce a schema mismatch.
fn parse_value(input: &[u8]) -> Result<(ValueRef<'_>, usize), RowError> {
    let tag = *input.first().ok_or(RowError::Corrupt)?;
    match tag {
        0 => Ok((ValueRef::Null, 1)),
        1 => match input.get(1) {
            Some(0) => Ok((ValueRef::Bool(false), 2)),
            Some(1) => Ok((ValueRef::Bool(true), 2)),
            _ => Err(RowError::Corrupt),
        },
        2 if input.len() >= 9 => {
            let mut bytes = [0; 8];
            bytes.copy_from_slice(&input[1..9]);
            Ok((ValueRef::Int64(i64::from_le_bytes(bytes)), 9))
        }
        3 | 4 if input.len() >= 5 => {
            let length = u32_at(input, 1) as usize;
            if length > MAX_VALUE_BYTES {
                return Err(RowError::TooLarge);
            }
            let end = 5 + length;
            let payload = input.get(5..end).ok_or(RowError::Corrupt)?;
            let value = if tag == 3 {
                ValueRef::Utf8(std::str::from_utf8(payload).map_err(|_| RowError::Corrupt)?)
            } else {
                ValueRef::Binary(payload)
            };
            Ok((value, end))
        }
        _ => Err(RowError::Corrupt),
    }
}

/// Validated immutable row bytes. Offsets use a fixed 132-byte array; values
/// borrow input and never allocate. This is not a page pin or observation token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalRowView<'a> {
    bytes: &'a [u8],
    columns: usize,
    offsets: [u32; MAX_ROW_COLUMNS + 1],
}

impl<'a> CanonicalRowView<'a> {
    pub(crate) fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub(crate) fn len(self) -> usize {
        self.columns
    }

    pub(crate) fn value(&self, index: usize) -> Result<ValueRef<'a>, RowError> {
        if index >= self.columns {
            return Err(RowError::InvalidInput);
        }
        let bytes = &self.bytes[self.offsets[index] as usize..self.offsets[index + 1] as usize];
        parse_value(bytes).map(|(value, _)| value)
    }
}

pub(crate) fn canonical_row_len(columns: &[Column], values: &[Value]) -> Result<usize, RowError> {
    schema_valid(columns)?;
    if columns.len() != values.len() {
        return Err(RowError::SchemaMismatch);
    }
    let mut total = 4_usize;
    for (column, value) in columns.iter().zip(values) {
        value.validate().map_err(|_| RowError::TooLarge)?;
        if !matches(column, borrowed(value)) {
            return Err(RowError::SchemaMismatch);
        }
        let length = match value {
            Value::Null => 1,
            Value::Bool(_) => 2,
            Value::Int64(_) => 9,
            Value::Utf8(value) => 5 + value.len(),
            Value::Bytes(value) => 5 + value.len(),
        };
        total = total.checked_add(length).ok_or(RowError::TooLarge)?;
        if total > MAX_ROW_BYTES {
            return Err(RowError::TooLarge);
        }
    }
    Ok(total)
}

/// All validation precedes writing; errors preserve output and success its tail.
pub(crate) fn encode_canonical_row(
    columns: &[Column],
    values: &[Value],
    output: &mut [u8],
) -> Result<usize, RowError> {
    let total = canonical_row_len(columns, values)?;
    if output.len() < total {
        return Err(RowError::NoSpace);
    }
    put_u32(output, 0, values.len());
    let mut offset = 4;
    for value in values {
        match value {
            Value::Null => {
                output[offset] = 0;
                offset += 1;
            }
            Value::Bool(value) => {
                output[offset] = 1;
                output[offset + 1] = u8::from(*value);
                offset += 2;
            }
            Value::Int64(value) => {
                output[offset] = 2;
                output[offset + 1..offset + 9].copy_from_slice(&value.to_le_bytes());
                offset += 9;
            }
            Value::Utf8(value) => {
                let bytes = value.as_bytes();
                output[offset] = 3;
                put_u32(output, offset + 1, bytes.len());
                output[offset + 5..offset + 5 + bytes.len()].copy_from_slice(bytes);
                offset += 5 + bytes.len();
            }
            Value::Bytes(bytes) => {
                output[offset] = 4;
                put_u32(output, offset + 1, bytes.len());
                output[offset + 5..offset + 5 + bytes.len()].copy_from_slice(bytes);
                offset += 5 + bytes.len();
            }
        }
    }
    Ok(total)
}

pub(crate) fn decode_canonical_row<'a>(
    input: &'a [u8],
    columns: &[Column],
) -> Result<CanonicalRowView<'a>, RowError> {
    schema_valid(columns)?;
    if input.len() > MAX_ROW_BYTES {
        return Err(RowError::TooLarge);
    }
    if input.len() < 4 {
        return Err(RowError::Corrupt);
    }
    let count = u32_at(input, 0) as usize;
    if !(1..=MAX_ROW_COLUMNS).contains(&count) {
        return Err(RowError::Corrupt);
    }
    if count != columns.len() {
        return Err(RowError::SchemaMismatch);
    }
    let mut offsets = [0; MAX_ROW_COLUMNS + 1];
    let mut offset = 4;
    offsets[0] = offset as u32;
    for (index, column) in columns.iter().enumerate() {
        let (value, consumed) = parse_value(&input[offset..])?;
        if !matches(column, value) {
            return Err(RowError::SchemaMismatch);
        }
        offset += consumed;
        offsets[index + 1] = offset as u32;
    }
    if offset != input.len() {
        return Err(RowError::Corrupt);
    }
    Ok(CanonicalRowView {
        bytes: input,
        columns: count,
        offsets,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordExpectation {
    pub(crate) namespace: Namespace,
    pub(crate) row: RowId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordVersion {
    pub(crate) row: RowId,
    pub(crate) stamp: VersionStamp,
    pub(crate) logical: LogicalRowVersion,
    pub(crate) undo: Option<UndoRef>,
}

impl RecordVersion {
    fn validate(self, expected: RecordExpectation) -> Result<(), RowError> {
        if expected.namespace.timeline == 0 {
            return Err(RowError::InvalidInput);
        }
        let id = self.logical.transaction();
        if self.row != expected.row
            || id.database != expected.namespace.database
            || id.timeline != expected.namespace.timeline
        {
            return Err(RowError::IdentityMismatch);
        }
        if let VersionStamp::Transaction(reference) = self.stamp
            && reference.id() != id
        {
            return Err(RowError::IdentityMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowPayload<'a> {
    Deleted,
    Inline(CanonicalRowView<'a>),
    Overflow(OverflowHead),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowRecordView<'a> {
    version: RecordVersion,
    payload: RowPayload<'a>,
}

impl<'a> RowRecordView<'a> {
    pub(crate) fn version(self) -> RecordVersion {
        self.version
    }

    pub(crate) fn payload(self) -> RowPayload<'a> {
        self.payload
    }
}

/// Prepare fixed metadata in bounded local scratch. Child encoder failures
/// cannot partially overwrite the caller's output.
fn version_bytes(version: RecordVersion, output: &mut [u8]) -> Result<(), RowError> {
    output[8..16].copy_from_slice(&version.row.get().to_le_bytes());
    version
        .stamp
        .encode(&mut output[16..72])
        .map_err(|_| RowError::InvalidInput)?;
    version
        .logical
        .encode(&mut output[72..120])
        .map_err(|_| RowError::InvalidInput)?;
    if let Some(undo) = version.undo {
        undo.encode(&mut output[120..160])
            .map_err(|_| RowError::InvalidInput)?;
    }
    Ok(())
}

fn decode_version(
    input: &[u8],
    has_undo: bool,
    expected: RecordExpectation,
) -> Result<RecordVersion, RowError> {
    let row = RowId::new(u64_at(input, 8)).map_err(|_| RowError::Corrupt)?;
    let stamp = VersionStamp::decode(&input[16..72]).map_err(|_| RowError::Corrupt)?;
    let logical = LogicalRowVersion::decode(&input[72..120]).map_err(|_| RowError::Corrupt)?;
    let undo = if has_undo {
        Some(UndoRef::decode(&input[120..160]).map_err(|_| RowError::Corrupt)?)
    } else {
        if !zero(&input[120..160]) {
            return Err(RowError::Corrupt);
        }
        None
    };
    let version = RecordVersion {
        row,
        stamp,
        logical,
        undo,
    };
    version.validate(expected)?;
    Ok(version)
}

pub(crate) fn encode_row_record(
    expected: RecordExpectation,
    columns: &[Column],
    version: RecordVersion,
    payload: RowPayload<'_>,
    output: &mut [u8],
) -> Result<usize, RowError> {
    schema_valid(columns)?;
    version.validate(expected)?;
    let mut header = [0; ROW_HEADER_BYTES];
    header[..4].copy_from_slice(b"XRW1");
    header[4..6].copy_from_slice(&1_u16.to_le_bytes());
    header[6] = u8::from(version.undo.is_some()) << 1;
    version_bytes(version, &mut header)?;
    let mut overflow_bytes = [0; OVERFLOW_HEAD_BYTES];
    let body = match payload {
        RowPayload::Deleted => {
            header[6] |= 1;
            &[][..]
        }
        RowPayload::Inline(row) => {
            if row.bytes.len() > INLINE_ROW_BYTES {
                return Err(RowError::TooLarge);
            }
            decode_canonical_row(row.bytes, columns)?;
            header[7] = 1;
            row.bytes
        }
        RowPayload::Overflow(head) => {
            header[7] = 2;
            head.encode(&mut overflow_bytes)
                .map_err(|_| RowError::InvalidInput)?;
            &overflow_bytes[..]
        }
    };
    let total = ROW_HEADER_BYTES + body.len();
    if output.len() < total {
        return Err(RowError::NoSpace);
    }
    put_u32(&mut header, 160, body.len());
    output[..ROW_HEADER_BYTES].copy_from_slice(&header);
    output[ROW_HEADER_BYTES..total].copy_from_slice(body);
    Ok(total)
}

pub(crate) fn decode_row_record<'a>(
    input: &'a [u8],
    expected: RecordExpectation,
    columns: &[Column],
) -> Result<RowRecordView<'a>, RowError> {
    schema_valid(columns)?;
    if input.len() < ROW_HEADER_BYTES || &input[..4] != b"XRW1" {
        return Err(RowError::Corrupt);
    }
    if input[4..6] != 1_u16.to_le_bytes() {
        return Err(RowError::UnsupportedFormat);
    }
    if input[6] & !3 != 0
        || !zero(&input[164..168])
        || u32_at(input, 160) as usize != input.len() - ROW_HEADER_BYTES
    {
        return Err(RowError::Corrupt);
    }
    let version = decode_version(input, input[6] & 2 != 0, expected)?;
    let body = &input[ROW_HEADER_BYTES..];
    let payload = match (input[6] & 1 != 0, input[7]) {
        (true, 0) if body.is_empty() => RowPayload::Deleted,
        (false, 1) => {
            if body.len() > INLINE_ROW_BYTES {
                return Err(RowError::TooLarge);
            }
            RowPayload::Inline(decode_canonical_row(body, columns)?)
        }
        (false, 2) => {
            RowPayload::Overflow(OverflowHead::decode(body).map_err(|_| RowError::Corrupt)?)
        }
        _ => return Err(RowError::Corrupt),
    };
    Ok(RowRecordView { version, payload })
}

/// This validates caller-materialized bytes, not the source pages or their pins.
pub(crate) fn decode_materialized_row<'a>(
    head: OverflowHead,
    bytes: &'a [u8],
    columns: &[Column],
) -> Result<CanonicalRowView<'a>, RowError> {
    if bytes.len() != head.total() || content_digest(bytes) != head.digest() {
        return Err(RowError::OverflowMismatch);
    }
    decode_canonical_row(bytes, columns)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IndexRecord {
    pub(crate) version: RecordVersion,
    pub(crate) present: bool,
}

pub(crate) fn encode_index_record(
    expected: RecordExpectation,
    record: IndexRecord,
    output: &mut [u8],
) -> Result<usize, RowError> {
    record.version.validate(expected)?;
    if output.len() < INDEX_RECORD_BYTES {
        return Err(RowError::NoSpace);
    }
    let mut bytes = [0; INDEX_RECORD_BYTES];
    bytes[..4].copy_from_slice(b"XIX1");
    bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
    bytes[6] = u8::from(record.present);
    bytes[7] = u8::from(record.version.undo.is_some());
    version_bytes(record.version, &mut bytes)?;
    output[..INDEX_RECORD_BYTES].copy_from_slice(&bytes);
    Ok(INDEX_RECORD_BYTES)
}

pub(crate) fn decode_index_record(
    input: &[u8],
    expected: RecordExpectation,
) -> Result<IndexRecord, RowError> {
    if input.len() != INDEX_RECORD_BYTES || &input[..4] != b"XIX1" {
        return Err(RowError::Corrupt);
    }
    if input[4..6] != 1_u16.to_le_bytes() {
        return Err(RowError::UnsupportedFormat);
    }
    if input[6] > 1 || input[7] > 1 {
        return Err(RowError::Corrupt);
    }
    Ok(IndexRecord {
        version: decode_version(input, input[7] == 1, expected)?,
        present: input[6] == 1,
    })
}

#[cfg(test)]
#[path = "row_record_tests.rs"]
mod tests;
