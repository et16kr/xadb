//! Selected DWB control and slot bytes, without file, selector or repair effects.
//! A validated view is not proof of durable selection or a live slot capability.
//! Expected slot identities come from that view, never from the slot being read.
//! See `devdoc/contracts/dev3-doublewrite-format.md`.
// Remove when the first actual doublewrite consumer is connected.
#![allow(dead_code)]

use super::page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef, decode_page};
use crate::api::{DatabaseId, Namespace};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

pub(crate) const POOL_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const CONTROL_RESERVE_BYTES: usize = 64 * 1024;
pub(crate) const SLOT_HEADER_BYTES: usize = 128;
pub(crate) const SLOT_BYTES: usize = SLOT_HEADER_BYTES + PAGE_BYTES + 32;
pub(crate) const POOL_SLOTS: usize = (POOL_BYTES - CONTROL_RESERVE_BYTES) / SLOT_BYTES;
pub(crate) const MAX_PENDING_IMAGES: usize = 64;
pub(crate) const CONTROL_HEADER_BYTES: usize = 128;
pub(crate) const ENTRY_BYTES: usize = 88;
pub(crate) const MAX_CONTROL_BYTES: usize =
    CONTROL_HEADER_BYTES + ENTRY_BYTES * MAX_PENDING_IMAGES + 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DoublewriteError {
    InvalidInput,
    NoSpace,
    Corrupt,
    UnsupportedFormat,
    IdentityMismatch,
    ImageMismatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ControlState {
    Empty = 0,
    Pending = 1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ControlHeader {
    pub(crate) namespace: Namespace,
    pub(crate) generation: u64,
    pub(crate) batch_id: u64,
    pub(crate) state: ControlState,
    pub(crate) max_lsn: PageLsn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ControlExpectation {
    pub(crate) namespace: Namespace,
    pub(crate) generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingImage {
    pub(crate) slot: u16,
    pub(crate) page: PageRef,
    pub(crate) owner: ObjectId,
    pub(crate) owner_generation: Generation,
    pub(crate) kind: PageKind,
    pub(crate) level: u8,
    pub(crate) lsn: PageLsn,
    pub(crate) image_digest: [u8; 32],
}
impl PendingImage {
    fn identity(self, namespace: Namespace) -> PageIdentity {
        PageIdentity {
            database: namespace.database,
            page: self.page,
            owner: self.owner,
            owner_generation: self.owner_generation,
        }
    }
    fn validate(self) -> Result<(), DoublewriteError> {
        if usize::from(self.slot) >= POOL_SLOTS
            || !self.kind.accepts_level(self.level)
            || self.lsn.0 == 0
        {
            return Err(DoublewriteError::InvalidInput);
        }
        Ok(())
    }
}

/// Immutable verified metadata; selection, image existence and durability remain
/// caller obligations. No mutable alias can change an entry after verification.
pub(crate) struct ControlView<'a> {
    bytes: &'a [u8],
    header: ControlHeader,
    count: usize,
}
impl ControlView<'_> {
    pub(crate) fn header(&self) -> ControlHeader {
        self.header
    }
    pub(crate) fn len(&self) -> usize {
        self.count
    }
    pub(crate) fn entry(&self, index: usize) -> Option<PendingImage> {
        if index >= self.count {
            return None;
        }
        let start = CONTROL_HEADER_BYTES + ENTRY_BYTES * index;
        Some(
            decode_entry(&self.bytes[start..start + ENTRY_BYTES])
                .expect("immutable validated entry"),
        )
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
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
fn put_u64(bytes: &mut [u8], offset: usize, number: u64) {
    bytes[offset..offset + 8].copy_from_slice(&number.to_le_bytes());
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
fn reference_bytes(reference: PageRef) -> [u8; 24] {
    let mut bytes = [0; 24];
    reference.encode(&mut bytes).expect("exact reference width");
    bytes
}
fn validate_header(header: ControlHeader, count: usize) -> Result<(), DoublewriteError> {
    if header.namespace.timeline == 0 || header.generation == 0 || count > MAX_PENDING_IMAGES {
        return Err(DoublewriteError::InvalidInput);
    }
    match header.state {
        ControlState::Empty if count != 0 || header.max_lsn.0 != 0 => {
            Err(DoublewriteError::InvalidInput)
        }
        ControlState::Pending if count == 0 || header.batch_id == 0 || header.max_lsn.0 == 0 => {
            Err(DoublewriteError::InvalidInput)
        }
        _ => Ok(()),
    }
}
/// A fixed bitset and one predecessor detect duplicates in the entire inventory.
fn validate_inventory(
    header: ControlHeader,
    count: usize,
    entries: impl Iterator<Item = Result<PendingImage, DoublewriteError>>,
) -> Result<(), DoublewriteError> {
    validate_header(header, count)?;
    let mut slots = [0u8; 128];
    let mut previous = None;
    let mut max_lsn = 0;
    let mut actual = 0;
    for entry in entries {
        let entry = entry?;
        entry.validate()?;
        let slot = usize::from(entry.slot);
        let mask = 1u8 << (slot % 8);
        if slots[slot / 8] & mask != 0 {
            return Err(DoublewriteError::InvalidInput);
        }
        slots[slot / 8] |= mask;
        let address = (
            read_u64(&reference_bytes(entry.page), 0),
            entry.page.page_no,
        );
        if previous.is_some_and(|last| last >= address) {
            return Err(DoublewriteError::InvalidInput);
        }
        previous = Some(address);
        max_lsn = max_lsn.max(entry.lsn.0);
        actual += 1;
    }
    if actual != count || max_lsn != header.max_lsn.0 {
        return Err(DoublewriteError::InvalidInput);
    }
    Ok(())
}
fn entry_bytes(entry: PendingImage) -> [u8; ENTRY_BYTES] {
    let mut bytes = [0; ENTRY_BYTES];
    bytes[..2].copy_from_slice(&entry.slot.to_le_bytes());
    bytes[2] = entry.kind as u8;
    bytes[3] = entry.level;
    bytes[8..32].copy_from_slice(&reference_bytes(entry.page));
    put_u64(&mut bytes, 32, entry.owner.get());
    put_u64(&mut bytes, 40, entry.owner_generation.get());
    put_u64(&mut bytes, 48, entry.lsn.0);
    bytes[56..88].copy_from_slice(&entry.image_digest);
    bytes
}
fn decode_entry(bytes: &[u8]) -> Result<PendingImage, DoublewriteError> {
    if bytes.len() != ENTRY_BYTES || !zero(&bytes[4..8]) {
        return Err(DoublewriteError::Corrupt);
    }
    let entry = PendingImage {
        slot: read_u16(bytes, 0),
        kind: PageKind::from_byte(bytes[2]).map_err(|_| DoublewriteError::Corrupt)?,
        level: bytes[3],
        page: PageRef::decode(&bytes[8..32]).map_err(|_| DoublewriteError::Corrupt)?,
        owner: ObjectId::new(read_u64(bytes, 32)).map_err(|_| DoublewriteError::Corrupt)?,
        owner_generation: Generation::new(read_u64(bytes, 40))
            .map_err(|_| DoublewriteError::Corrupt)?,
        lsn: PageLsn(read_u64(bytes, 48)),
        image_digest: bytes[56..88].try_into().expect("fixed digest width"),
    };
    entry.validate().map_err(|_| DoublewriteError::Corrupt)?;
    Ok(entry)
}

/// Preflight all entries and exact wire length; no reservation is made.
pub(crate) fn control_encoded_len(
    header: ControlHeader,
    entries: &[PendingImage],
) -> Result<usize, DoublewriteError> {
    validate_inventory(header, entries.len(), entries.iter().copied().map(Ok))?;
    // The validated count is at most 64, so this expression cannot overflow.
    Ok(CONTROL_HEADER_BYTES + ENTRY_BYTES * entries.len() + 32)
}
/// All failures preserve the complete output. Successful encoding writes a prefix.
pub(crate) fn encode_control<'a>(
    header: ControlHeader,
    entries: &[PendingImage],
    output: &'a mut [u8],
) -> Result<&'a [u8], DoublewriteError> {
    let length = control_encoded_len(header, entries)?;
    if output.len() < length {
        return Err(DoublewriteError::NoSpace);
    }
    let mut bytes = [0; CONTROL_HEADER_BYTES];
    bytes[..4].copy_from_slice(b"XDW1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6] = header.state as u8;
    bytes[8..24].copy_from_slice(&header.namespace.database.bytes());
    put_u64(&mut bytes, 24, header.namespace.timeline);
    put_u64(&mut bytes, 32, header.generation);
    put_u64(&mut bytes, 40, header.batch_id);
    bytes[48..50].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    put_u64(&mut bytes, 56, header.max_lsn.0);
    output[..CONTROL_HEADER_BYTES].copy_from_slice(&bytes);
    for (index, entry) in entries.iter().copied().enumerate() {
        let start = CONTROL_HEADER_BYTES + ENTRY_BYTES * index;
        output[start..start + ENTRY_BYTES].copy_from_slice(&entry_bytes(entry));
    }
    let digest = content_digest(&output[..length - 32]);
    output[length - 32..length].copy_from_slice(&digest);
    Ok(&output[..length])
}
/// Require one exact control, its selected generation, namespace and every entry.
pub(crate) fn decode_control(
    input: &[u8],
    expected: ControlExpectation,
) -> Result<ControlView<'_>, DoublewriteError> {
    if expected.namespace.timeline == 0 || expected.generation == 0 {
        return Err(DoublewriteError::InvalidInput);
    }
    if input.len() < CONTROL_HEADER_BYTES + 32
        || input.len() > MAX_CONTROL_BYTES
        || &input[..4] != b"XDW1"
    {
        return Err(DoublewriteError::Corrupt);
    }
    if read_u16(input, 4) != 1 {
        return Err(DoublewriteError::UnsupportedFormat);
    }
    let state = match input[6] {
        0 => ControlState::Empty,
        1 => ControlState::Pending,
        _ => return Err(DoublewriteError::Corrupt),
    };
    let count = usize::from(read_u16(input, 48));
    if count > MAX_PENDING_IMAGES
        || input.len() != CONTROL_HEADER_BYTES + ENTRY_BYTES * count + 32
        || input[7] != 0
        || !zero(&input[50..56])
        || !zero(&input[64..128])
    {
        return Err(DoublewriteError::Corrupt);
    }
    if content_digest(&input[..input.len() - 32]) != input[input.len() - 32..] {
        return Err(DoublewriteError::Corrupt);
    }
    let database = DatabaseId::new(input[8..24].try_into().expect("checked length"))
        .map_err(|_| DoublewriteError::Corrupt)?;
    let header = ControlHeader {
        namespace: Namespace {
            database,
            timeline: read_u64(input, 24),
        },
        generation: read_u64(input, 32),
        batch_id: read_u64(input, 40),
        state,
        max_lsn: PageLsn(read_u64(input, 56)),
    };
    validate_header(header, count).map_err(|_| DoublewriteError::Corrupt)?;
    if header.namespace != expected.namespace || header.generation != expected.generation {
        return Err(DoublewriteError::IdentityMismatch);
    }
    let entries = input[CONTROL_HEADER_BYTES..input.len() - 32]
        .chunks_exact(ENTRY_BYTES)
        .map(decode_entry);
    validate_inventory(header, count, entries).map_err(|_| DoublewriteError::Corrupt)?;
    Ok(ControlView {
        bytes: input,
        header,
        count,
    })
}

/// Check a complete page image against a caller's trusted inventory entry.
pub(crate) fn validate_image(
    entry: PendingImage,
    namespace: Namespace,
    image: &[u8],
) -> Result<(), DoublewriteError> {
    if namespace.timeline == 0 {
        return Err(DoublewriteError::InvalidInput);
    }
    entry.validate()?;
    if image.len() != PAGE_BYTES || content_digest(image) != entry.image_digest {
        return Err(DoublewriteError::ImageMismatch);
    }
    let page = decode_page(image, entry.identity(namespace), entry.kind)
        .map_err(|_| DoublewriteError::ImageMismatch)?;
    if page.header().level != entry.level || page.header().lsn != entry.lsn {
        return Err(DoublewriteError::ImageMismatch);
    }
    Ok(())
}
fn selected_entry(
    control: &ControlView<'_>,
    index: usize,
) -> Result<PendingImage, DoublewriteError> {
    if control.header.state != ControlState::Pending {
        return Err(DoublewriteError::InvalidInput);
    }
    control.entry(index).ok_or(DoublewriteError::InvalidInput)
}
fn slot_header(control: &ControlView<'_>, entry: PendingImage) -> [u8; SLOT_HEADER_BYTES] {
    let mut bytes = [0; SLOT_HEADER_BYTES];
    bytes[..4].copy_from_slice(b"XDS1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&entry.slot.to_le_bytes());
    bytes[8..24].copy_from_slice(&control.header.namespace.database.bytes());
    put_u64(&mut bytes, 24, control.header.namespace.timeline);
    put_u64(&mut bytes, 32, control.header.batch_id);
    bytes[40..64].copy_from_slice(&reference_bytes(entry.page));
    put_u64(&mut bytes, 64, entry.owner.get());
    put_u64(&mut bytes, 72, entry.owner_generation.get());
    put_u64(&mut bytes, 80, entry.lsn.0);
    bytes[88] = entry.kind as u8;
    bytes[89] = entry.level;
    bytes[96..128].copy_from_slice(&entry.image_digest);
    bytes
}
/// Validate image and all selected metadata before the first output write.
pub(crate) fn encode_slot<'a>(
    control: &ControlView<'_>,
    index: usize,
    image: &[u8],
    output: &'a mut [u8],
) -> Result<&'a [u8], DoublewriteError> {
    let entry = selected_entry(control, index)?;
    validate_image(entry, control.header.namespace, image)?;
    if output.len() < SLOT_BYTES {
        return Err(DoublewriteError::NoSpace);
    }
    let header = slot_header(control, entry);
    output[..SLOT_HEADER_BYTES].copy_from_slice(&header);
    output[SLOT_HEADER_BYTES..SLOT_BYTES - 32].copy_from_slice(image);
    let digest = content_digest(&output[..SLOT_BYTES - 32]);
    output[SLOT_BYTES - 32..SLOT_BYTES].copy_from_slice(&digest);
    Ok(&output[..SLOT_BYTES])
}
/// A slot has no independent authority: every expected field comes from control.
pub(crate) fn decode_slot<'a>(
    control: &ControlView<'_>,
    index: usize,
    input: &'a [u8],
) -> Result<&'a [u8], DoublewriteError> {
    let entry = selected_entry(control, index)?;
    if input.len() != SLOT_BYTES || &input[..4] != b"XDS1" {
        return Err(DoublewriteError::Corrupt);
    }
    if read_u16(input, 4) != 1 {
        return Err(DoublewriteError::UnsupportedFormat);
    }
    if !zero(&input[90..96])
        || content_digest(&input[..SLOT_BYTES - 32]) != input[SLOT_BYTES - 32..]
    {
        return Err(DoublewriteError::Corrupt);
    }
    // Equality includes slot index, namespace, sequence, complete target identity,
    // page kind/level/LSN and full-image digest, not just the slot checksum.
    if input[..SLOT_HEADER_BYTES] != slot_header(control, entry) {
        return Err(DoublewriteError::IdentityMismatch);
    }
    let image = &input[SLOT_HEADER_BYTES..SLOT_BYTES - 32];
    validate_image(entry, control.header.namespace, image)?;
    Ok(image)
}

#[cfg(test)]
#[path = "doublewrite_control_tests.rs"]
mod tests;
