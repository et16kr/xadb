//! Bounded file effects. Callers own commit phase, quota, pins and selector order.
use super::{
    codec::{self, FRAME_OVERHEAD, Identity, Record},
    error::{DurableError as Error, DurableErrorCode, Result, TransactionOutcome},
};
use xadb_foundation::{
    codec::{ContentHasher, MAX_FRAME_PAYLOAD},
    error::Effect,
    io::{FileIo, read_exact_at, write_all_at},
    namespace::{
        FileName, NamespaceError, NamespaceErrorKind, OpenMode, Selector, WriterNamespace,
    },
};

pub(crate) fn namespace(error: NamespaceError) -> Error {
    let code = match error.kind {
        NamespaceErrorKind::Busy => DurableErrorCode::Busy,
        NamespaceErrorKind::LimitExceeded => DurableErrorCode::LimitExceeded,
        NamespaceErrorKind::InvalidName => DurableErrorCode::InvalidInput,
        NamespaceErrorKind::InvalidType => DurableErrorCode::Corruption,
        _ => DurableErrorCode::Io,
    };
    Error {
        io_kind: error.io_kind,
        fenced: error.effect == Effect::CallerMustResolve,
        ..Error::new(code, error.context)
    }
}

pub(crate) fn metadata_failure(mut error: Error) -> Error {
    // No batch byte has been attempted. Metadata effects can nevertheless be
    // uncertain, so callers must fence without inventing an unknown transaction.
    error.fenced = true;
    error.outcome = TransactionOutcome::NotAttempted;
    error
}

pub(crate) fn file_error(error: xadb_foundation::Error) -> Error {
    Error {
        io_kind: error.io_kind,
        ..Error::new(DurableErrorCode::Io, error.context)
    }
}

pub(crate) fn wal_name(epoch: u32, segment: u64) -> Result<FileName> {
    FileName::new(&format!("wal-{epoch:08x}-{segment:016x}.axf")).map_err(namespace)
}
pub(crate) fn resume_name(epoch: u32) -> Result<FileName> {
    FileName::new(&format!("resume-{epoch:08x}.axf")).map_err(namespace)
}
pub(crate) fn manifest_name(generation: u64) -> Result<FileName> {
    FileName::new(&format!("manifest-{generation:016x}.axf")).map_err(namespace)
}
pub(crate) fn chunk_name(generation: u64, index: u32) -> Result<FileName> {
    FileName::new(&format!("chunk-{generation:016x}-{index:08x}.axf")).map_err(namespace)
}
pub(crate) fn selector_name(selector: Selector) -> Result<FileName> {
    FileName::new(selector.as_str()).map_err(namespace)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedName {
    Wal { epoch: u32, segment: u64 },
    Resume { epoch: u32 },
    Manifest { generation: u64 },
    Chunk { generation: u64, index: u32 },
    Temporary { epoch: u32 },
    Lock,
    Current,
    Writer,
}

/// Recognize exact generated spelling, including zero-padded lower-case hex.
/// Partial/orphan files still reserve the epoch carried by their name.
pub(crate) fn parse_name(name: &FileName) -> Option<OwnedName> {
    let value = name.as_str();
    match value {
        "LOCK" => return Some(OwnedName::Lock),
        "CURRENT" => return Some(OwnedName::Current),
        "WRITER" => return Some(OwnedName::Writer),
        _ => {}
    }
    fn number(value: &str, width: usize) -> Option<u64> {
        if value.len() != width
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        u64::from_str_radix(value, 16).ok()
    }
    if let Some(value) = value
        .strip_prefix("wal-")
        .and_then(|s| s.strip_suffix(".axf"))
    {
        let (epoch, segment) = value.split_once('-')?;
        let epoch = number(epoch, 8)? as u32;
        let segment = number(segment, 16)?;
        return (epoch != 0 && segment != 0).then_some(OwnedName::Wal { epoch, segment });
    }
    if let Some(value) = value
        .strip_prefix("resume-")
        .and_then(|s| s.strip_suffix(".axf"))
    {
        let epoch = number(value, 8)? as u32;
        return (epoch != 0).then_some(OwnedName::Resume { epoch });
    }
    if let Some(value) = value
        .strip_prefix("manifest-")
        .and_then(|s| s.strip_suffix(".axf"))
    {
        let generation = number(value, 16)?;
        return (generation != 0).then_some(OwnedName::Manifest { generation });
    }
    if let Some(value) = value
        .strip_prefix("chunk-")
        .and_then(|s| s.strip_suffix(".axf"))
    {
        let (generation, index) = value.split_once('-')?;
        let generation = number(generation, 16)?;
        let index = number(index, 8)? as u32;
        return (generation != 0).then_some(OwnedName::Chunk { generation, index });
    }
    if let Some(value) = value
        .strip_prefix("select-")
        .and_then(|s| s.strip_suffix(".tmp"))
    {
        let (epoch, serial) = value.split_once('-')?;
        let epoch = number(epoch, 8)? as u32;
        let serial = number(serial, 16)?;
        return (epoch != 0 && serial != 0).then_some(OwnedName::Temporary { epoch });
    }
    None
}

pub(crate) struct Frame {
    pub bytes: Vec<u8>,
    pub identity: Identity,
    pub record: Record,
    pub next: u64,
}

pub(crate) enum FrameRead {
    Complete(Frame),
    /// Only a final active segment may accept this result as an incomplete tail.
    Incomplete,
    End,
}

pub(crate) fn read_frame(file: &mut impl FileIo, offset: u64, file_size: u64) -> Result<FrameRead> {
    let remaining = file_size
        .checked_sub(offset)
        .ok_or(Error::invalid("scan offset after EOF"))?;
    if remaining == 0 {
        return Ok(FrameRead::End);
    }
    let mut header = [0_u8; 10];
    let prefix = remaining.min(header.len() as u64) as usize;
    read_exact_at(file, &mut header[..prefix], offset).map_err(file_error)?;
    let magic_bytes = prefix.min(4);
    if header[..magic_bytes] != b"AXF1"[..magic_bytes] {
        return Err(Error::invalid("WAL frame magic"));
    }
    if prefix == 5 && header[4] != 1 {
        return Err(Error::new(
            DurableErrorCode::UnsupportedFormat,
            "partial WAL envelope version",
        ));
    }
    if prefix >= 6 && header[4..6] != [1, 0] {
        return Err(Error::new(
            DurableErrorCode::UnsupportedFormat,
            "WAL envelope version",
        ));
    }
    if prefix < header.len() {
        return Ok(FrameRead::Incomplete);
    }
    let payload = u32::from_le_bytes([header[6], header[7], header[8], header[9]]) as usize;
    if payload > MAX_FRAME_PAYLOAD {
        return Err(Error::limit("WAL declared payload"));
    }
    if payload < codec::COMMON_BYTES {
        return Err(Error::invalid(
            "WAL payload cannot contain an engine record",
        ));
    }
    let length = payload + FRAME_OVERHEAD;
    if remaining < length as u64 {
        return Ok(FrameRead::Incomplete);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error::allocation())?;
    if bytes.capacity() > MAX_FRAME_PAYLOAD + FRAME_OVERHEAD {
        return Err(Error::limit("raw frame allocation capacity"));
    }
    bytes.resize(length, 0);
    bytes[..10].copy_from_slice(&header);
    read_exact_at(file, &mut bytes[10..], offset + 10).map_err(file_error)?;
    let (identity, record) = codec::decode(&bytes)?;
    Ok(FrameRead::Complete(Frame {
        bytes,
        identity,
        record,
        next: offset + length as u64,
    }))
}

pub(crate) fn read_record<W: WriterNamespace>(writer: &mut W, name: &FileName) -> Result<Frame> {
    let metadata = writer
        .metadata(name)
        .map_err(namespace)?
        .ok_or(Error::invalid("required file is missing"))?;
    if metadata.length > (MAX_FRAME_PAYLOAD + FRAME_OVERHEAD) as u64 {
        return Err(Error::limit("metadata file length"));
    }
    let mut file = writer
        .open_existing(name, OpenMode::ReadOnly)
        .map_err(namespace)?;
    let frame = match read_frame(&mut file, 0, metadata.length)? {
        FrameRead::Complete(frame) if frame.next == metadata.length => frame,
        _ => return Err(Error::invalid("incomplete or trailing metadata record")),
    };
    if writer.metadata(name).map_err(namespace)? != Some(metadata) {
        return Err(Error::invalid("file identity changed while reading"));
    }
    Ok(frame)
}

pub(crate) fn hash_prefix(file: &mut impl FileIo, end: u64) -> Result<ContentHasher> {
    let mut hasher = ContentHasher::new();
    let mut scratch = [0_u8; 64 * 1024];
    let mut offset = 0_u64;
    while offset < end {
        let count = (end - offset).min(scratch.len() as u64) as usize;
        read_exact_at(file, &mut scratch[..count], offset).map_err(file_error)?;
        hasher.update(&scratch[..count]);
        offset += count as u64;
    }
    Ok(hasher)
}

/// Create and sync immutable bytes, without claiming their name is durable yet.
pub(crate) fn write_new<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    bytes: &[u8],
) -> Result<W::File> {
    let mut file = writer
        .create_new(name)
        .map_err(namespace)
        .map_err(metadata_failure)?;
    write_all_at(&mut file, bytes, 0)
        .map_err(file_error)
        .map_err(metadata_failure)?;
    file.sync_all()
        .map_err(|error| Error {
            io_kind: Some(error.kind()),
            ..Error::new(DurableErrorCode::Io, "immutable file sync")
        })
        .map_err(metadata_failure)?;
    Ok(file)
}

/// Complete temp file sync -> rename -> directory sync; callers fence on failure.
/// The serial is consumed even on failure, and is scoped to a reserved epoch.
pub(crate) fn select<W: WriterNamespace>(
    writer: &mut W,
    epoch: u32,
    serial: &mut u64,
    selector: Selector,
    bytes: &[u8],
) -> Result<()> {
    *serial = serial
        .checked_add(1)
        .ok_or(Error::limit("selector temporary identity exhausted"))?;
    let temporary =
        FileName::new(&format!("select-{epoch:08x}-{serial:016x}.tmp")).map_err(namespace)?;
    let file = write_new(writer, &temporary, bytes)?;
    drop(file);
    writer
        .publish_selector(&temporary, selector)
        .map_err(namespace)
        .map_err(metadata_failure)?;
    writer
        .sync_directory()
        .map_err(namespace)
        .map_err(metadata_failure)
}
