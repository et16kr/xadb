//! Current-build DEV2 records. No Rust memory layout is persisted.
use super::error::{DurableError as Error, DurableErrorCode, Result};
use crate::api::{Column, DataType, DatabaseId, TableSpec};
use foundation::{
    codec::{MAX_FRAME_PAYLOAD, decode_frame, encode_frame},
    identity::{Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch},
    value::Value,
};

pub(crate) const FRAME_OVERHEAD: usize = 42;
pub(crate) const COMMON_BYTES: usize = 43;
pub(crate) const MAX_CHANGES: usize = 256;
pub(crate) const MAX_FILES: usize = 4096;
/// Logical and observed allocation bound for one decoded record, excluding its
/// raw AXF1 bytes. Checkpoint packing observes this as well as the wire bound.
pub(crate) const MAX_DECODED_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const SEGMENT_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const WAL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub database: DatabaseId,
    pub timeline: u64,
    pub epoch: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub epoch: u32,
    pub segment: u64,
    pub end_offset: u64,
    pub file_size: u64,
    pub q: u64,
    pub prefix_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Anchor {
    pub epoch: u32,
    pub segment: u64,
    pub header_digest: [u8; 32],
    pub offset: u64,
    pub prefix_digest: [u8; 32],
    pub q: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Genesis,
    Rotation,
    Resume,
    Restore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub segment: u64,
    pub start_q: u64,
    pub origin: Origin,
    pub previous: Option<Link>,
    pub resume_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableImage {
    pub identity: ObjectHandle,
    pub spec: TableSpec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowImage {
    pub table: ObjectHandle,
    pub row: RowId,
    pub values: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    Create(TableImage),
    Drop(ObjectHandle),
    Put(RowImage),
    Delete { table: ObjectHandle, row: RowId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Batch {
    pub txn_local: u64,
    pub previous_q: u64,
    pub q: u64,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManifestRef {
    pub generation: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChunkRef {
    pub index: u32,
    pub bytes: u64,
    pub digest: [u8; 32],
    pub rows: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Provenance {
    pub database: DatabaseId,
    pub timeline: u64,
    pub generation: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub generation: u64,
    pub cut: u64,
    pub epoch_high_water: u32,
    pub anchor: Anchor,
    pub tables: Vec<TableImage>,
    pub chunks: Vec<ChunkRef>,
    pub provenance: Option<Provenance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Record {
    Header(Header),
    Batch(Batch),
    Seal {
        end_offset: u64,
        q: u64,
        prefix_digest: [u8; 32],
    },
    Resume {
        segment: u64,
        previous: Link,
    },
    Writer {
        segment: u64,
        header_digest: [u8; 32],
    },
    Current {
        current: ManifestRef,
        previous: Option<ManifestRef>,
    },
    Manifest(Manifest),
    Chunk {
        generation: u64,
        index: u32,
        rows: Vec<RowImage>,
    },
}

struct Encoder(Vec<u8>);
impl Encoder {
    fn new() -> Result<Self> {
        let mut data = Vec::new();
        data.try_reserve_exact(MAX_FRAME_PAYLOAD)
            .map_err(|_| Error::allocation())?;
        if data.capacity() > MAX_FRAME_PAYLOAD {
            return Err(Error::limit("encoder allocation capacity"));
        }
        Ok(Self(data))
    }
    fn bytes(&mut self, data: &[u8]) -> Result<()> {
        if self
            .0
            .len()
            .checked_add(data.len())
            .is_none_or(|n| n > MAX_FRAME_PAYLOAD)
        {
            return Err(Error::limit("engine payload exceeds 1 MiB"));
        }
        self.0.extend_from_slice(data);
        Ok(())
    }
    fn byte(&mut self, value: u8) -> Result<()> {
        self.bytes(&[value])
    }
    fn u16(&mut self, value: u16) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn name(&mut self, value: &str) -> Result<()> {
        if !crate::catalog::name_valid(value) {
            return Err(Error::invalid("schema name"));
        }
        self.u16(value.len() as u16)?;
        self.bytes(value.as_bytes())
    }
    fn handle(&mut self, id: ObjectHandle) -> Result<()> {
        valid_handle(id)?;
        self.u64(id.object.get())?;
        self.u64(id.generation.get())?;
        self.u64(id.schema_epoch.get())
    }
    fn table(&mut self, table: &TableImage) -> Result<()> {
        crate::catalog::validate_spec(&table.spec).map_err(|_| Error::invalid("table schema"))?;
        self.handle(table.identity)?;
        self.name(&table.spec.name)?;
        self.u16(table.spec.columns.len() as u16)?;
        for column in &table.spec.columns {
            self.name(&column.name)?;
            self.byte(match column.data_type {
                DataType::Bool => 1,
                DataType::Int64 => 2,
                DataType::Utf8 => 3,
                DataType::Binary => 4,
            })?;
            self.byte(u8::from(column.nullable))?;
        }
        Ok(())
    }
    fn row(&mut self, row: &RowImage) -> Result<()> {
        self.handle(row.table)?;
        valid_number(row.row.get())?;
        self.u64(row.row.get())?;
        let start = self.0.len();
        if !(1..=32).contains(&row.values.len()) {
            return Err(Error::invalid("row column count"));
        }
        self.u32(row.values.len() as u32)?;
        for value in &row.values {
            value.validate().map_err(Error::foundation)?;
            match value {
                Value::Null => self.byte(0)?,
                Value::Bool(value) => {
                    self.byte(1)?;
                    self.byte(u8::from(*value))?;
                }
                Value::Int64(value) => {
                    self.byte(2)?;
                    self.bytes(&value.to_le_bytes())?;
                }
                Value::Utf8(value) => {
                    self.byte(3)?;
                    self.u32(value.len() as u32)?;
                    self.bytes(value.as_bytes())?;
                }
                Value::Bytes(value) => {
                    self.byte(4)?;
                    self.u32(value.len() as u32)?;
                    self.bytes(value)?;
                }
            }
        }
        if self.0.len() - start > crate::catalog::MAX_ROW_BYTES {
            return Err(Error::limit("canonical row exceeds 128 KiB"));
        }
        Ok(())
    }
    fn link(&mut self, link: &Link) -> Result<()> {
        valid_link(link)?;
        self.u32(link.epoch)?;
        self.u64(link.segment)?;
        self.u64(link.end_offset)?;
        self.u64(link.file_size)?;
        self.u64(link.q)?;
        self.bytes(&link.prefix_digest)
    }
    fn anchor(&mut self, anchor: &Anchor) -> Result<()> {
        valid_anchor(anchor)?;
        self.u32(anchor.epoch)?;
        self.u64(anchor.segment)?;
        self.bytes(&anchor.header_digest)?;
        self.u64(anchor.offset)?;
        self.bytes(&anchor.prefix_digest)?;
        self.u64(anchor.q)
    }
    fn reference(&mut self, reference: &ManifestRef) -> Result<()> {
        if reference.generation == 0 {
            return Err(Error::invalid("zero manifest generation"));
        }
        self.u64(reference.generation)?;
        self.bytes(&reference.digest)
    }
}

fn valid_number(number: u64) -> Result<()> {
    if number >> 32 == 0 || number as u32 == 0 {
        return Err(Error::invalid("unreserved logical identity"));
    }
    Ok(())
}
fn valid_handle(handle: ObjectHandle) -> Result<()> {
    valid_number(handle.object.get())?;
    if handle.generation.get() != 1 || handle.schema_epoch.get() != 1 {
        return Err(Error::invalid("unsupported object generation"));
    }
    Ok(())
}
fn valid_link(link: &Link) -> Result<()> {
    if link.epoch == 0
        || link.segment == 0
        || link.end_offset < (COMMON_BYTES + FRAME_OVERHEAD) as u64
        || link.end_offset > link.file_size
        || link.file_size > SEGMENT_BYTES
    {
        return Err(Error::invalid("predecessor boundary"));
    }
    Ok(())
}
fn valid_anchor(anchor: &Anchor) -> Result<()> {
    if anchor.epoch == 0
        || anchor.segment == 0
        || anchor.offset < (COMMON_BYTES + FRAME_OVERHEAD) as u64
        || anchor.offset > SEGMENT_BYTES
    {
        return Err(Error::invalid("checkpoint anchor boundary"));
    }
    Ok(())
}
fn valid_batch(batch: &Batch) -> Result<()> {
    if batch.txn_local == 0
        || batch.previous_q.checked_add(1) != Some(batch.q)
        || batch.changes.is_empty()
        || batch.changes.len() > MAX_CHANGES
    {
        return Err(Error::invalid("commit sequence/count"));
    }
    for (index, change) in batch.changes.iter().enumerate() {
        let row = match change {
            Change::Create(_) | Change::Drop(_) => {
                if batch.changes.len() != 1 {
                    return Err(Error::invalid("DDL mixed with DML"));
                }
                continue;
            }
            Change::Put(row) => row.row,
            Change::Delete { row, .. } => *row,
        };
        if batch.changes[..index].iter().any(|prior| match prior {
            Change::Put(value) => value.row == row,
            Change::Delete { row: prior, .. } => *prior == row,
            _ => false,
        }) {
            return Err(Error::invalid("duplicate row after-image"));
        }
    }
    Ok(())
}

pub(crate) fn encode(identity: Identity, record: &Record) -> Result<Vec<u8>> {
    if identity.timeline == 0 || identity.epoch == 0 {
        return Err(Error::invalid("zero record namespace"));
    }
    let mut out = Encoder::new()?;
    out.bytes(b"XDK1")?;
    out.u16(1)?;
    out.u64(1)?;
    out.byte(match record {
        Record::Header(_) => 1,
        Record::Batch(_) => 2,
        Record::Seal { .. } => 3,
        Record::Resume { .. } => 4,
        Record::Writer { .. } => 5,
        Record::Current { .. } => 6,
        Record::Manifest(_) => 7,
        Record::Chunk { .. } => 8,
    })?;
    out.bytes(&identity.database.bytes())?;
    out.u64(identity.timeline)?;
    out.u32(identity.epoch)?;
    match record {
        Record::Header(header) => {
            valid_header(identity, header)?;
            out.u64(header.segment)?;
            out.u64(header.start_q)?;
            out.byte(match header.origin {
                Origin::Genesis => 0,
                Origin::Rotation => 1,
                Origin::Resume => 2,
                Origin::Restore => 3,
            })?;
            out.byte(u8::from(header.previous.is_some()))?;
            if let Some(link) = &header.previous {
                out.link(link)?;
            }
            out.bytes(&header.resume_digest)?;
        }
        Record::Batch(batch) => {
            write_batch(&mut out, batch)?;
        }
        Record::Seal {
            end_offset,
            q,
            prefix_digest,
        } => {
            valid_seal(*end_offset)?;
            out.u64(*end_offset)?;
            out.u64(*q)?;
            out.bytes(prefix_digest)?;
        }
        Record::Resume { segment, previous } => {
            if *segment == 0 || previous.epoch >= identity.epoch {
                return Err(Error::invalid("resume epoch/segment"));
            }
            out.u64(*segment)?;
            out.link(previous)?;
        }
        Record::Writer {
            segment,
            header_digest,
        } => {
            if *segment == 0 {
                return Err(Error::invalid("zero writer segment"));
            }
            out.u64(*segment)?;
            out.bytes(header_digest)?;
        }
        Record::Current { current, previous } => {
            if previous
                .as_ref()
                .is_some_and(|prior| prior.generation >= current.generation)
            {
                return Err(Error::invalid("checkpoint selection order"));
            }
            out.reference(current)?;
            out.byte(u8::from(previous.is_some()))?;
            if let Some(previous) = previous {
                out.reference(previous)?;
            }
        }
        Record::Manifest(manifest) => {
            valid_manifest(identity, manifest)?;
            out.u64(manifest.generation)?;
            out.u64(manifest.cut)?;
            out.u32(manifest.epoch_high_water)?;
            out.anchor(&manifest.anchor)?;
            out.u16(manifest.tables.len() as u16)?;
            for table in &manifest.tables {
                out.table(table)?;
            }
            out.u32(manifest.chunks.len() as u32)?;
            for chunk in &manifest.chunks {
                out.u32(chunk.index)?;
                out.u64(chunk.bytes)?;
                out.bytes(&chunk.digest)?;
                out.u32(chunk.rows)?;
            }
            out.byte(u8::from(manifest.provenance.is_some()))?;
            if let Some(source) = &manifest.provenance {
                out.bytes(&source.database.bytes())?;
                out.u64(source.timeline)?;
                out.u64(source.generation)?;
                out.bytes(&source.digest)?;
            }
        }
        Record::Chunk {
            generation,
            index,
            rows,
        } => {
            if *generation == 0
                || *index as usize >= MAX_FILES
                || rows.is_empty()
                || rows.len() > MAX_FRAME_PAYLOAD / 37
            {
                return Err(Error::invalid("checkpoint chunk bounds"));
            }
            valid_rows(rows)?;
            let mut decoded = 0_usize;
            for row in rows {
                decoded = decoded
                    .checked_add(row_decoded_bytes(row)?)
                    .ok_or(Error::limit("decoded chunk allocation overflow"))?;
                if decoded > MAX_DECODED_BYTES {
                    return Err(Error::limit("decoded checkpoint chunk exceeds 2 MiB"));
                }
            }
            out.u64(*generation)?;
            out.u32(*index)?;
            out.u32(rows.len() as u32)?;
            for row in rows {
                out.row(row)?;
            }
        }
    }
    finish(out)
}

fn finish(out: Encoder) -> Result<Vec<u8>> {
    let frame = encode_frame(&out.0).map_err(Error::foundation)?;
    if frame.capacity() > MAX_FRAME_PAYLOAD + FRAME_OVERHEAD {
        return Err(Error::limit("frame allocation capacity"));
    }
    Ok(frame)
}

fn common(identity: Identity, kind: u8) -> Result<Encoder> {
    if identity.timeline == 0 || identity.epoch == 0 {
        return Err(Error::invalid("zero record namespace"));
    }
    let mut out = Encoder::new()?;
    out.bytes(b"XDK1")?;
    out.u16(1)?;
    out.u64(1)?;
    out.byte(kind)?;
    out.bytes(&identity.database.bytes())?;
    out.u64(identity.timeline)?;
    out.u32(identity.epoch)?;
    Ok(out)
}

pub(crate) fn encode_batch(identity: Identity, batch: &Batch) -> Result<Vec<u8>> {
    let mut out = common(identity, 2)?;
    write_batch(&mut out, batch)?;
    finish(out)
}

fn write_batch(out: &mut Encoder, batch: &Batch) -> Result<()> {
    valid_batch(batch)?;
    out.u64(batch.txn_local)?;
    out.u64(batch.previous_q)?;
    out.u64(batch.q)?;
    out.u16(batch.changes.len() as u16)?;
    for change in &batch.changes {
        match change {
            Change::Create(table) => {
                out.byte(1)?;
                out.table(table)?;
            }
            Change::Drop(table) => {
                out.byte(2)?;
                out.handle(*table)?;
            }
            Change::Put(row) => {
                out.byte(3)?;
                out.row(row)?;
            }
            Change::Delete { table, row } => {
                out.byte(4)?;
                out.handle(*table)?;
                valid_number(row.get())?;
                out.u64(row.get())?;
            }
        }
    }
    let body_bytes = out.0.len() as u32;
    out.bytes(b"XDCM")?;
    out.u64(batch.q)?;
    out.u16(batch.changes.len() as u16)?;
    out.u32(body_bytes)?;
    Ok(())
}

fn valid_header(identity: Identity, header: &Header) -> Result<()> {
    if header.segment == 0 {
        return Err(Error::invalid("zero segment"));
    }
    match (header.origin, &header.previous) {
        (Origin::Genesis, None) if header.start_q == 0 && header.resume_digest == [0; 32] => Ok(()),
        (Origin::Restore, None) if header.resume_digest == [0; 32] => Ok(()),
        (Origin::Rotation, Some(link))
            if link.epoch == identity.epoch
                && link.segment < header.segment
                && link.q == header.start_q
                && link.end_offset == link.file_size
                && header.resume_digest == [0; 32] =>
        {
            valid_link(link)
        }
        (Origin::Resume, Some(link))
            if link.epoch < identity.epoch
                && link.q == header.start_q
                && header.resume_digest != [0; 32] =>
        {
            valid_link(link)
        }
        _ => Err(Error::invalid("segment origin/predecessor")),
    }
}
fn valid_seal(offset: u64) -> Result<()> {
    // A seal's digest excludes its own frame. The frame occupies 48 payload
    // bytes after the common prefix plus the outer envelope.
    if offset < (COMMON_BYTES + FRAME_OVERHEAD) as u64
        || offset > SEGMENT_BYTES - (COMMON_BYTES + 48 + FRAME_OVERHEAD) as u64
    {
        return Err(Error::invalid("seal prefix boundary"));
    }
    Ok(())
}
fn valid_manifest(identity: Identity, manifest: &Manifest) -> Result<()> {
    if manifest.generation == 0
        || manifest.anchor.q != manifest.cut
        || manifest.epoch_high_water < identity.epoch
        || manifest.anchor.epoch > manifest.epoch_high_water
        || manifest.tables.len() > crate::catalog::MAX_TABLES
        || manifest.chunks.len() >= MAX_FILES
    {
        return Err(Error::invalid("manifest bounds/anchor"));
    }
    valid_anchor(&manifest.anchor)?;
    for (index, table) in manifest.tables.iter().enumerate() {
        if manifest.tables[..index].iter().any(|prior| {
            prior.identity.object == table.identity.object || prior.spec.name == table.spec.name
        }) {
            return Err(Error::invalid("duplicate checkpoint table"));
        }
        valid_handle(table.identity)?;
    }
    for (index, chunk) in manifest.chunks.iter().enumerate() {
        if chunk.index as usize != index
            || chunk.rows == 0
            || chunk.bytes > (MAX_FRAME_PAYLOAD + FRAME_OVERHEAD) as u64
            || chunk.bytes < (COMMON_BYTES + FRAME_OVERHEAD + 16 + 37) as u64
        {
            return Err(Error::invalid("chunk inventory"));
        }
    }
    if identity.epoch != manifest.anchor.epoch {
        return Err(Error::invalid("manifest capture epoch"));
    }
    if manifest.provenance.as_ref().is_some_and(|source| {
        source.timeline == 0
            || source.generation == 0
            || source.database != identity.database
            || source.timeline == identity.timeline
    }) {
        return Err(Error::invalid("restore provenance"));
    }
    Ok(())
}
fn valid_rows(rows: &[RowImage]) -> Result<()> {
    for pair in rows.windows(2) {
        if (pair[0].table.object.get(), pair[0].row.get())
            >= (pair[1].table.object.get(), pair[1].row.get())
        {
            return Err(Error::invalid("checkpoint rows not strictly ordered"));
        }
    }
    Ok(())
}

/// Exact requested decoded storage for one row, including its enclosing Vec slot.
/// The decoder additionally accounts actual allocator capacity before retaining it.
pub(crate) fn row_decoded_bytes(row: &RowImage) -> Result<usize> {
    row_values_decoded_bytes(&row.values)
}

pub(crate) fn row_values_decoded_bytes(values: &[Value]) -> Result<usize> {
    let mut bytes = values
        .len()
        .checked_mul(std::mem::size_of::<Value>())
        .and_then(|n| n.checked_add(std::mem::size_of::<RowImage>()))
        .ok_or(Error::limit("decoded row allocation overflow"))?;
    for value in values {
        let extra = match value {
            Value::Utf8(value) => value.len(),
            Value::Bytes(value) => value.len(),
            _ => 0,
        };
        bytes = bytes
            .checked_add(extra)
            .ok_or(Error::limit("decoded row payload overflow"))?;
    }
    Ok(bytes)
}

struct Decoder<'a> {
    data: &'a [u8],
    at: usize,
    allocated: usize,
}
impl<'a> Decoder<'a> {
    fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(count)
            .ok_or(Error::invalid("record length overflow"))?;
        let value = self
            .data
            .get(self.at..end)
            .ok_or(Error::invalid("truncated record"))?;
        self.at = end;
        Ok(value)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.bytes(N)?
            .try_into()
            .map_err(|_| Error::invalid("record scalar width"))
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn boolean(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::invalid("noncanonical boolean")),
        }
    }
    fn vector<T>(&mut self, count: usize, minimum_bytes: usize, maximum: usize) -> Result<Vec<T>> {
        if count > maximum
            || count
                .checked_mul(minimum_bytes)
                .is_none_or(|n| n > self.data.len() - self.at)
        {
            return Err(Error::invalid("record count exceeds input/budget"));
        }
        let requested = count
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| n.checked_add(self.allocated))
            .ok_or(Error::limit("decoded allocation overflow"))?;
        if requested > MAX_DECODED_BYTES {
            return Err(Error::limit("decoded record exceeds 2 MiB"));
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| Error::allocation())?;
        let actual = values
            .capacity()
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| n.checked_add(self.allocated))
            .ok_or(Error::limit("decoded allocation capacity overflow"))?;
        if actual > MAX_DECODED_BYTES {
            return Err(Error::limit("decoded capacity exceeds 2 MiB"));
        }
        self.allocated = actual;
        Ok(values)
    }
    fn owned(&mut self, count: usize, maximum: usize) -> Result<Vec<u8>> {
        let mut value = self.vector(count, 1, maximum)?;
        value.extend_from_slice(self.bytes(count)?);
        Ok(value)
    }
    fn name(&mut self) -> Result<String> {
        let count = usize::from(self.u16()?);
        let name = String::from_utf8(self.owned(count, 128)?)
            .map_err(|_| Error::invalid("schema name UTF-8"))?;
        if !crate::catalog::name_valid(&name) {
            return Err(Error::invalid("schema name"));
        }
        Ok(name)
    }
    fn handle(&mut self) -> Result<ObjectHandle> {
        let object = ObjectId::new(self.u64()?).map_err(Error::foundation)?;
        let generation = Generation::new(self.u64()?).map_err(Error::foundation)?;
        let schema_epoch = SchemaEpoch::new(self.u64()?).map_err(Error::foundation)?;
        let handle = ObjectHandle {
            object,
            generation,
            schema_epoch,
        };
        valid_handle(handle)?;
        Ok(handle)
    }
    fn table(&mut self) -> Result<TableImage> {
        let identity = self.handle()?;
        let name = self.name()?;
        let count = usize::from(self.u16()?);
        let mut columns = self.vector(count, 5, 32)?;
        for _ in 0..count {
            let name = self.name()?;
            let data_type = match self.byte()? {
                1 => DataType::Bool,
                2 => DataType::Int64,
                3 => DataType::Utf8,
                4 => DataType::Binary,
                _ => return Err(Error::invalid("schema type tag")),
            };
            columns.push(Column {
                name,
                data_type,
                nullable: self.boolean()?,
            });
        }
        let spec = TableSpec { name, columns };
        crate::catalog::validate_spec(&spec).map_err(|_| Error::invalid("table schema"))?;
        Ok(TableImage { identity, spec })
    }
    fn row(&mut self) -> Result<RowImage> {
        let table = self.handle()?;
        let row = RowId::new(self.u64()?).map_err(Error::foundation)?;
        valid_number(row.get())?;
        let start = self.at;
        let count = self.u32()? as usize;
        if count == 0 {
            return Err(Error::invalid("empty row"));
        }
        let mut values = self.vector(count, 1, 32)?;
        for _ in 0..count {
            values.push(match self.byte()? {
                0 => Value::Null,
                1 => Value::Bool(self.boolean()?),
                2 => Value::Int64(i64::from_le_bytes(self.array()?)),
                3 => {
                    let len = self.u32()? as usize;
                    Value::Utf8(
                        String::from_utf8(self.owned(len, 65_536)?)
                            .map_err(|_| Error::invalid("row UTF-8"))?,
                    )
                }
                4 => {
                    let len = self.u32()? as usize;
                    Value::Bytes(self.owned(len, 65_536)?)
                }
                _ => return Err(Error::invalid("row value tag")),
            });
            if self.at - start > crate::catalog::MAX_ROW_BYTES {
                return Err(Error::limit("canonical row exceeds 128 KiB"));
            }
        }
        Ok(RowImage { table, row, values })
    }
    fn link(&mut self) -> Result<Link> {
        let link = Link {
            epoch: self.u32()?,
            segment: self.u64()?,
            end_offset: self.u64()?,
            file_size: self.u64()?,
            q: self.u64()?,
            prefix_digest: self.array()?,
        };
        valid_link(&link)?;
        Ok(link)
    }
    fn anchor(&mut self) -> Result<Anchor> {
        let anchor = Anchor {
            epoch: self.u32()?,
            segment: self.u64()?,
            header_digest: self.array()?,
            offset: self.u64()?,
            prefix_digest: self.array()?,
            q: self.u64()?,
        };
        valid_anchor(&anchor)?;
        Ok(anchor)
    }
    fn reference(&mut self) -> Result<ManifestRef> {
        let reference = ManifestRef {
            generation: self.u64()?,
            digest: self.array()?,
        };
        if reference.generation == 0 {
            return Err(Error::invalid("zero manifest generation"));
        }
        Ok(reference)
    }
}

pub(crate) fn decode(frame: &[u8]) -> Result<(Identity, Record)> {
    let payload = decode_frame(frame).map_err(Error::foundation)?;
    let mut input = Decoder {
        data: payload,
        at: 0,
        allocated: 0,
    };
    if input.bytes(4)? != b"XDK1" {
        return Err(Error::invalid("engine record magic"));
    }
    if input.u16()? != 1 || input.u64()? != 1 {
        return Err(Error::new(
            DurableErrorCode::UnsupportedFormat,
            "engine format/features",
        ));
    }
    let kind = input.byte()?;
    let database =
        DatabaseId::new(input.array()?).map_err(|_| Error::invalid("zero database identity"))?;
    let identity = Identity {
        database,
        timeline: input.u64()?,
        epoch: input.u32()?,
    };
    if identity.timeline == 0 || identity.epoch == 0 {
        return Err(Error::invalid("zero record namespace"));
    }
    let record = match kind {
        1 => {
            let segment = input.u64()?;
            let start_q = input.u64()?;
            let origin = match input.byte()? {
                0 => Origin::Genesis,
                1 => Origin::Rotation,
                2 => Origin::Resume,
                3 => Origin::Restore,
                _ => return Err(Error::invalid("segment origin")),
            };
            let previous = if input.boolean()? {
                Some(input.link()?)
            } else {
                None
            };
            let header = Header {
                segment,
                start_q,
                origin,
                previous,
                resume_digest: input.array()?,
            };
            valid_header(identity, &header)?;
            Record::Header(header)
        }
        2 => {
            let txn_local = input.u64()?;
            let previous_q = input.u64()?;
            let q = input.u64()?;
            let count = usize::from(input.u16()?);
            let mut changes = input.vector(count, 25, MAX_CHANGES)?;
            for _ in 0..count {
                changes.push(match input.byte()? {
                    1 => Change::Create(input.table()?),
                    2 => Change::Drop(input.handle()?),
                    3 => Change::Put(input.row()?),
                    4 => {
                        let table = input.handle()?;
                        let row = RowId::new(input.u64()?).map_err(Error::foundation)?;
                        valid_number(row.get())?;
                        Change::Delete { table, row }
                    }
                    _ => return Err(Error::invalid("mutation tag")),
                });
            }
            let body_bytes = input.at;
            if input.bytes(4)? != b"XDCM"
                || input.u64()? != q
                || usize::from(input.u16()?) != count
                || input.u32()? as usize != body_bytes
            {
                return Err(Error::invalid("commit trailer"));
            }
            let batch = Batch {
                txn_local,
                previous_q,
                q,
                changes,
            };
            valid_batch(&batch)?;
            Record::Batch(batch)
        }
        3 => {
            let end_offset = input.u64()?;
            valid_seal(end_offset)?;
            Record::Seal {
                end_offset,
                q: input.u64()?,
                prefix_digest: input.array()?,
            }
        }
        4 => {
            let segment = input.u64()?;
            let previous = input.link()?;
            if segment == 0 || previous.epoch >= identity.epoch {
                return Err(Error::invalid("resume epoch/segment"));
            }
            Record::Resume { segment, previous }
        }
        5 => {
            let segment = input.u64()?;
            if segment == 0 {
                return Err(Error::invalid("zero writer segment"));
            }
            Record::Writer {
                segment,
                header_digest: input.array()?,
            }
        }
        6 => {
            let current = input.reference()?;
            let previous = if input.boolean()? {
                Some(input.reference()?)
            } else {
                None
            };
            if previous
                .as_ref()
                .is_some_and(|prior| prior.generation >= current.generation)
            {
                return Err(Error::invalid("checkpoint selection order"));
            }
            Record::Current { current, previous }
        }
        7 => {
            let generation = input.u64()?;
            let cut = input.u64()?;
            let epoch_high_water = input.u32()?;
            let anchor = input.anchor()?;
            let count = usize::from(input.u16()?);
            let mut tables = input.vector(count, 33, crate::catalog::MAX_TABLES)?;
            for _ in 0..count {
                tables.push(input.table()?);
            }
            let count = input.u32()? as usize;
            let mut chunks = input.vector(count, 48, MAX_FILES - 1)?;
            for _ in 0..count {
                chunks.push(ChunkRef {
                    index: input.u32()?,
                    bytes: input.u64()?,
                    digest: input.array()?,
                    rows: input.u32()?,
                });
            }
            let provenance = if input.boolean()? {
                Some(Provenance {
                    database: DatabaseId::new(input.array()?)
                        .map_err(|_| Error::invalid("source database"))?,
                    timeline: input.u64()?,
                    generation: input.u64()?,
                    digest: input.array()?,
                })
            } else {
                None
            };
            let manifest = Manifest {
                generation,
                cut,
                epoch_high_water,
                anchor,
                tables,
                chunks,
                provenance,
            };
            valid_manifest(identity, &manifest)?;
            Record::Manifest(manifest)
        }
        8 => {
            let generation = input.u64()?;
            let index = input.u32()?;
            let count = input.u32()? as usize;
            if generation == 0 || index as usize >= MAX_FILES || count == 0 {
                return Err(Error::invalid("checkpoint chunk bounds"));
            }
            let mut rows = input.vector(count, 37, MAX_FRAME_PAYLOAD / 37)?;
            for _ in 0..count {
                rows.push(input.row()?);
            }
            valid_rows(&rows)?;
            Record::Chunk {
                generation,
                index,
                rows,
            }
        }
        _ => return Err(Error::invalid("engine record kind")),
    };
    if input.at != payload.len() {
        return Err(Error::invalid("trailing record bytes"));
    }
    Ok((identity, record))
}
