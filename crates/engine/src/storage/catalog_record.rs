//! Versioned table/index definitions in caller-owned bytes, without DDL effects.
//! Contract: `devdoc/contracts/dev3-catalog-format.md`.
// Remove when the disk catalog first consumes this representation.
#![allow(dead_code)]

use super::{
    page::PageRef,
    transaction_record::{LogicalRowVersion, VersionStamp},
    undo_ref::UndoRef,
};
use crate::api::{DataType, Namespace, TableSpec};
use xadb_foundation::identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch};

pub(crate) const CATALOG_HEADER_BYTES: usize = 232;
pub(crate) const MAX_CATALOG_CELL_BYTES: usize = 4588;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogError {
    InvalidInput,
    NoSpace,
    Corrupt,
    UnsupportedFormat,
    IdentityMismatch,
    DefinitionMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogExpectation {
    pub(crate) namespace: Namespace,
    pub(crate) object: ObjectId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogHeader {
    pub(crate) identity: ObjectHandle,
    pub(crate) retired: bool,
    pub(crate) stamp: VersionStamp,
    pub(crate) logical: LogicalRowVersion,
    pub(crate) undo: Option<UndoRef>,
    pub(crate) anchor: PageRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum IndexMode {
    PrimaryKey = 1,
    Unique = 2,
    Nonunique = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IndexDefinition<'a> {
    pub(crate) parent: ObjectHandle,
    pub(crate) name: &'a str,
    pub(crate) mode: IndexMode,
    pub(crate) columns: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CatalogDefinition<'a> {
    Table(&'a TableSpec),
    Index(IndexDefinition<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogColumn<'a> {
    pub(crate) name: &'a str,
    pub(crate) data_type: DataType,
    pub(crate) nullable: bool,
}

/// Validated schema bytes with fixed 66-byte offset scratch; no heap ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableDefinitionView<'a> {
    bytes: &'a [u8],
    name: &'a str,
    count: usize,
    offsets: [u16; 33],
}

impl<'a> TableDefinitionView<'a> {
    pub(crate) fn name(self) -> &'a str {
        self.name
    }

    pub(crate) fn len(self) -> usize {
        self.count
    }

    pub(crate) fn column(self, index: usize) -> Result<CatalogColumn<'a>, CatalogError> {
        if index >= self.count {
            return Err(CatalogError::InvalidInput);
        }
        let start = self.offsets[index] as usize;
        let end = self.offsets[index + 1] as usize;
        Ok(CatalogColumn {
            name: read_name(&self.bytes[start + 4..end])?,
            data_type: decode_type(self.bytes[start + 2])?,
            nullable: self.bytes[start + 3] == 1,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogDefinitionView<'a> {
    Table(TableDefinitionView<'a>),
    Index(IndexDefinition<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogRecordView<'a> {
    namespace: Namespace,
    header: CatalogHeader,
    definition: CatalogDefinitionView<'a>,
}

impl<'a> CatalogRecordView<'a> {
    pub(crate) fn header(self) -> CatalogHeader {
        self.header
    }

    pub(crate) fn definition(self) -> CatalogDefinitionView<'a> {
        self.definition
    }

    /// Compare a stabilized identity; this does not acquire a handle or pin.
    pub(crate) fn validate_handle(self, expected: ObjectHandle) -> Result<(), CatalogError> {
        if self.header.identity == expected {
            Ok(())
        } else {
            Err(CatalogError::IdentityMismatch)
        }
    }
}

fn u16_at(input: &[u8], offset: usize) -> usize {
    u16::from_le_bytes([input[offset], input[offset + 1]]) as usize
}

fn u32_at(input: &[u8], offset: usize) -> u32 {
    let mut value = [0; 4];
    value.copy_from_slice(&input[offset..offset + 4]);
    u32::from_le_bytes(value)
}

fn u64_at(input: &[u8], offset: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&input[offset..offset + 8]);
    u64::from_le_bytes(value)
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn zero(input: &[u8]) -> bool {
    input.iter().all(|value| *value == 0)
}

fn valid_name(name: &str) -> bool {
    (1..=128).contains(&name.len()) && !name.contains('\0')
}

fn read_name(input: &[u8]) -> Result<&str, CatalogError> {
    let name = std::str::from_utf8(input).map_err(|_| CatalogError::Corrupt)?;
    if !valid_name(name) {
        return Err(CatalogError::Corrupt);
    }
    Ok(name)
}

fn type_tag(data_type: DataType) -> u8 {
    match data_type {
        DataType::Bool => 1,
        DataType::Int64 => 2,
        DataType::Utf8 => 3,
        DataType::Binary => 4,
    }
}

fn decode_type(tag: u8) -> Result<DataType, CatalogError> {
    match tag {
        1 => Ok(DataType::Bool),
        2 => Ok(DataType::Int64),
        3 => Ok(DataType::Utf8),
        4 => Ok(DataType::Binary),
        _ => Err(CatalogError::Corrupt),
    }
}

fn validate_header(
    header: CatalogHeader,
    expected: CatalogExpectation,
) -> Result<(), CatalogError> {
    if expected.namespace.timeline == 0 {
        return Err(CatalogError::InvalidInput);
    }
    let transaction = header.logical.transaction();
    if header.identity.object != expected.object
        || transaction.database != expected.namespace.database
        || transaction.timeline != expected.namespace.timeline
        || matches!(header.stamp, VersionStamp::Transaction(reference) if reference.id() != transaction)
    {
        return Err(CatalogError::IdentityMismatch);
    }
    Ok(())
}

fn definition_len(
    header: CatalogHeader,
    definition: CatalogDefinition<'_>,
) -> Result<usize, CatalogError> {
    match definition {
        CatalogDefinition::Table(spec) => {
            if !valid_name(&spec.name) || !(1..=32).contains(&spec.columns.len()) {
                return Err(CatalogError::InvalidInput);
            }
            let mut length = 4 + spec.name.len();
            for (i, column) in spec.columns.iter().enumerate() {
                if !valid_name(&column.name)
                    || spec.columns[..i].iter().any(|old| old.name == column.name)
                {
                    return Err(CatalogError::InvalidInput);
                }
                length += 4 + column.name.len();
            }
            Ok(length)
        }
        CatalogDefinition::Index(index) => {
            if !valid_name(index.name)
                || index.parent.object == header.identity.object
                || !(1..=4).contains(&index.columns.len())
            {
                return Err(CatalogError::InvalidInput);
            }
            for (i, position) in index.columns.iter().enumerate() {
                if *position >= 32 || index.columns[..i].contains(position) {
                    return Err(CatalogError::InvalidInput);
                }
            }
            Ok(8 + index.name.len())
        }
    }
}

pub(crate) fn catalog_encoded_len(
    expected: CatalogExpectation,
    header: CatalogHeader,
    definition: CatalogDefinition<'_>,
) -> Result<usize, CatalogError> {
    validate_header(header, expected)?;
    Ok(CATALOG_HEADER_BYTES + definition_len(header, definition)?)
}

/// Validate every field before the first caller-buffer write. Fixed local
/// header scratch isolates child encoders; failures preserve the entire output.
pub(crate) fn encode_catalog_record(
    expected: CatalogExpectation,
    header: CatalogHeader,
    definition: CatalogDefinition<'_>,
    output: &mut [u8],
) -> Result<usize, CatalogError> {
    let length = catalog_encoded_len(expected, header, definition)?;
    if output.len() < length {
        return Err(CatalogError::NoSpace);
    }
    let mut fixed = [0; CATALOG_HEADER_BYTES];
    fixed[..4].copy_from_slice(b"XCT1");
    fixed[4..6].copy_from_slice(&1_u16.to_le_bytes());
    fixed[7] = u8::from(header.retired) | (u8::from(header.undo.is_some()) << 1);
    put_u64(&mut fixed, 8, header.identity.object.get());
    put_u64(&mut fixed, 16, header.identity.generation.get());
    put_u64(&mut fixed, 24, header.identity.schema_epoch.get());
    header
        .stamp
        .encode(&mut fixed[32..88])
        .map_err(|_| CatalogError::InvalidInput)?;
    header
        .logical
        .encode(&mut fixed[88..136])
        .map_err(|_| CatalogError::InvalidInput)?;
    if let Some(undo) = header.undo {
        undo.encode(&mut fixed[136..176])
            .map_err(|_| CatalogError::InvalidInput)?;
    }
    header
        .anchor
        .encode(&mut fixed[176..200])
        .map_err(|_| CatalogError::InvalidInput)?;
    if let CatalogDefinition::Index(index) = definition {
        fixed[6] = 2;
        put_u64(&mut fixed, 200, index.parent.object.get());
        put_u64(&mut fixed, 208, index.parent.generation.get());
        put_u64(&mut fixed, 216, index.parent.schema_epoch.get());
    } else {
        fixed[6] = 1;
    }
    fixed[224..228].copy_from_slice(&((length - CATALOG_HEADER_BYTES) as u32).to_le_bytes());
    output[..CATALOG_HEADER_BYTES].copy_from_slice(&fixed);
    let body = &mut output[CATALOG_HEADER_BYTES..length];
    match definition {
        CatalogDefinition::Table(spec) => {
            body[..2].copy_from_slice(&(spec.name.len() as u16).to_le_bytes());
            body[2] = spec.columns.len() as u8;
            body[3] = 0;
            body[4..4 + spec.name.len()].copy_from_slice(spec.name.as_bytes());
            let mut offset = 4 + spec.name.len();
            for column in &spec.columns {
                body[offset..offset + 2].copy_from_slice(&(column.name.len() as u16).to_le_bytes());
                body[offset + 2] = type_tag(column.data_type);
                body[offset + 3] = u8::from(column.nullable);
                body[offset + 4..offset + 4 + column.name.len()]
                    .copy_from_slice(column.name.as_bytes());
                offset += 4 + column.name.len();
            }
        }
        CatalogDefinition::Index(index) => {
            body[..2].copy_from_slice(&(index.name.len() as u16).to_le_bytes());
            body[2] = index.mode as u8;
            body[3] = index.columns.len() as u8;
            body[4..8].fill(0);
            body[4..4 + index.columns.len()].copy_from_slice(index.columns);
            body[8..].copy_from_slice(index.name.as_bytes());
        }
    }
    Ok(length)
}

fn decode_identity(input: &[u8], offset: usize) -> Result<ObjectHandle, CatalogError> {
    Ok(ObjectHandle {
        object: ObjectId::new(u64_at(input, offset)).map_err(|_| CatalogError::Corrupt)?,
        generation: Generation::new(u64_at(input, offset + 8))
            .map_err(|_| CatalogError::Corrupt)?,
        schema_epoch: SchemaEpoch::new(u64_at(input, offset + 16))
            .map_err(|_| CatalogError::Corrupt)?,
    })
}

fn decode_table(body: &[u8]) -> Result<TableDefinitionView<'_>, CatalogError> {
    if body.len() < 10 || body[3] != 0 || !(1..=32).contains(&body[2]) {
        return Err(CatalogError::Corrupt);
    }
    let name_end = 4 + u16_at(body, 0);
    let name = read_name(body.get(4..name_end).ok_or(CatalogError::Corrupt)?)?;
    let mut view = TableDefinitionView {
        bytes: body,
        name,
        count: body[2] as usize,
        offsets: [0; 33],
    };
    let mut offset = name_end;
    for i in 0..view.count {
        let head = body.get(offset..offset + 4).ok_or(CatalogError::Corrupt)?;
        let end = offset + 4 + u16_at(head, 0);
        let name = read_name(body.get(offset + 4..end).ok_or(CatalogError::Corrupt)?)?;
        decode_type(head[2])?;
        if head[3] > 1 {
            return Err(CatalogError::Corrupt);
        }
        for previous in 0..i {
            if view.column(previous)?.name == name {
                return Err(CatalogError::Corrupt);
            }
        }
        view.offsets[i] = offset as u16;
        view.offsets[i + 1] = end as u16;
        offset = end;
    }
    if offset != body.len() {
        return Err(CatalogError::Corrupt);
    }
    Ok(view)
}

fn decode_index(body: &[u8], parent: ObjectHandle) -> Result<IndexDefinition<'_>, CatalogError> {
    if body.len() < 9 || body.len() != 8 + u16_at(body, 0) || !(1..=4).contains(&body[3]) {
        return Err(CatalogError::Corrupt);
    }
    let mode = match body[2] {
        1 => IndexMode::PrimaryKey,
        2 => IndexMode::Unique,
        3 => IndexMode::Nonunique,
        _ => return Err(CatalogError::Corrupt),
    };
    let arity = body[3] as usize;
    if !zero(&body[4 + arity..8]) {
        return Err(CatalogError::Corrupt);
    }
    let columns = &body[4..4 + arity];
    for (i, position) in columns.iter().enumerate() {
        if *position >= 32 || columns[..i].contains(position) {
            return Err(CatalogError::Corrupt);
        }
    }
    Ok(IndexDefinition {
        parent,
        name: read_name(&body[8..])?,
        mode,
        columns,
    })
}

pub(crate) fn decode_catalog_record(
    input: &[u8],
    expected: CatalogExpectation,
) -> Result<CatalogRecordView<'_>, CatalogError> {
    if input.len() < CATALOG_HEADER_BYTES || &input[..4] != b"XCT1" {
        return Err(CatalogError::Corrupt);
    }
    if input[4..6] != 1_u16.to_le_bytes() {
        return Err(CatalogError::UnsupportedFormat);
    }
    if input[7] & !3 != 0
        || !zero(&input[228..232])
        || u32_at(input, 224) as usize != input.len() - CATALOG_HEADER_BYTES
        || input.len() > MAX_CATALOG_CELL_BYTES
    {
        return Err(CatalogError::Corrupt);
    }
    let undo = if input[7] & 2 != 0 {
        Some(UndoRef::decode(&input[136..176]).map_err(|_| CatalogError::Corrupt)?)
    } else {
        if !zero(&input[136..176]) {
            return Err(CatalogError::Corrupt);
        }
        None
    };
    let header = CatalogHeader {
        identity: decode_identity(input, 8)?,
        retired: input[7] & 1 != 0,
        stamp: VersionStamp::decode(&input[32..88]).map_err(|_| CatalogError::Corrupt)?,
        logical: LogicalRowVersion::decode(&input[88..136]).map_err(|_| CatalogError::Corrupt)?,
        undo,
        anchor: PageRef::decode(&input[176..200]).map_err(|_| CatalogError::Corrupt)?,
    };
    validate_header(header, expected)?;
    let body = &input[CATALOG_HEADER_BYTES..];
    let definition = match input[6] {
        1 if zero(&input[200..224]) => CatalogDefinitionView::Table(decode_table(body)?),
        2 => {
            let parent = decode_identity(input, 200)?;
            if parent.object == header.identity.object {
                return Err(CatalogError::Corrupt);
            }
            CatalogDefinitionView::Index(decode_index(body, parent)?)
        }
        _ => return Err(CatalogError::Corrupt),
    };
    Ok(CatalogRecordView {
        namespace: expected.namespace,
        header,
        definition,
    })
}

/// Check definitions in the same namespace. Visibility, uniqueness and object
/// lifetime require their separate owners; retired definitions remain usable.
pub(crate) fn validate_index_against_table(
    index: CatalogRecordView<'_>,
    table: CatalogRecordView<'_>,
) -> Result<(), CatalogError> {
    let (CatalogDefinitionView::Index(definition), CatalogDefinitionView::Table(schema)) =
        (index.definition, table.definition)
    else {
        return Err(CatalogError::DefinitionMismatch);
    };
    if index.namespace != table.namespace || definition.parent != table.header.identity {
        return Err(CatalogError::DefinitionMismatch);
    }
    for position in definition.columns {
        let column = schema
            .column(*position as usize)
            .map_err(|_| CatalogError::DefinitionMismatch)?;
        if definition.mode == IndexMode::PrimaryKey && column.nullable {
            return Err(CatalogError::DefinitionMismatch);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "catalog_record_tests.rs"]
mod tests;
