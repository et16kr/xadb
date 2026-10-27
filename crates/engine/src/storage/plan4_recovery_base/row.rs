//! Exact-image v4 layout and historical INIT observations. Layout validation is
//! deliberately separate from the full semantic graph/credit/inventory closure
//! and from the original selected-source owner's provenance verification.

#[allow(
    unused_imports,
    reason = "pure Base4 supply awaits current-profile closure"
)]
pub(in crate::storage) use super::image_records::encode_certificate;
use super::image_records::{decode_certificate, validate_provenance};
use super::{
    AllocationEntry, BaseFormat, CERTIFICATE_BYTES, Certificate, DESCRIPTOR_BYTES, Error, Expected,
    Header, HeaderMode, PAGE_BYTES, PageKind, Result, allocation_page, content_digest, get16,
    get32, identity, match_certificate, one, put16, put32, read_header_format, write_header, zero,
};
pub(in crate::storage) use super::{INIT_BYTES, InitProvenance};
use crate::storage::retention_page::row;

pub(in crate::storage) const BASE_BYTES: usize = 2_097_152;
pub(in crate::storage) const PAGE_ARENA_BYTES: usize = 64 * PAGE_BYTES;
const TRAILER: usize = BASE_BYTES - 32;
const BANK: usize = 4096;
const CERTIFICATES: usize = 65_536;
const PROVENANCE: usize = 73_728;
const INVENTORY: usize = 98_304;
const ARENA: usize = 262_144;
const SECTIONS: [(usize, usize, usize, usize); 5] = [
    (BANK, DESCRIPTOR_BYTES, 36, 1536),
    (CERTIFICATES, 64 * CERTIFICATE_BYTES, 64, CERTIFICATE_BYTES),
    (PROVENANCE, 64 * INIT_BYTES, 64, INIT_BYTES),
    (INVENTORY, PAGE_BYTES, 1, PAGE_BYTES),
    (ARENA, PAGE_ARENA_BYTES, 64, PAGE_BYTES),
];

/// No owned Header or page array is retained by a view. The temporary decoded
/// Header must leave scope before the caller acquires physical target carriers.
#[derive(Clone, Copy)]
pub(in crate::storage) struct LayoutView<'a> {
    bytes: &'a [u8; BASE_BYTES],
    expected: Expected,
}
impl<'a> LayoutView<'a> {
    pub(in crate::storage) fn bytes(self) -> &'a [u8; BASE_BYTES] {
        self.bytes
    }
    pub(in crate::storage) fn header(self) -> Header {
        read_header_format(self.bytes, self.expected, HeaderMode::Row)
            .expect("validated immutable header")
    }
    pub(in crate::storage) fn descriptor_bank(self) -> &'a [u8; DESCRIPTOR_BYTES] {
        self.bytes[BANK..BANK + DESCRIPTOR_BYTES]
            .try_into()
            .expect("fixed section")
    }
    pub(in crate::storage) fn inventory(self) -> &'a [u8; PAGE_BYTES] {
        self.bytes[INVENTORY..INVENTORY + PAGE_BYTES]
            .try_into()
            .expect("fixed section")
    }
    pub(in crate::storage) fn certificate(self, ordinal: usize) -> Result<Option<Certificate>> {
        certificate(self.bytes, self.expected, ordinal)
    }
    pub(in crate::storage) fn provenance(
        self,
        ordinal: usize,
    ) -> Result<Option<InitProvenance<'a>>> {
        if ordinal >= 64 {
            return Err(Error::InvalidInput);
        }
        let raw =
            &self.bytes[PROVENANCE + ordinal * INIT_BYTES..PROVENANCE + (ordinal + 1) * INIT_BYTES];
        if zero(raw) {
            return Ok(None);
        }
        Ok(Some(InitProvenance {
            bytes: raw.try_into().expect("fixed record"),
        }))
    }
    pub(in crate::storage) fn image(self, ordinal: usize) -> Result<Option<&'a [u8; PAGE_BYTES]>> {
        Ok(self
            .certificate(ordinal)?
            .map(|_| image(self.bytes, ordinal)))
    }
    pub(in crate::storage) fn digest(self) -> [u8; 32] {
        content_digest(self.bytes)
    }
}

fn image(bytes: &[u8; BASE_BYTES], ordinal: usize) -> &[u8; PAGE_BYTES] {
    bytes[ARENA + ordinal * PAGE_BYTES..ARENA + (ordinal + 1) * PAGE_BYTES]
        .try_into()
        .expect("bounded ordinal")
}

fn certificate(
    bytes: &[u8; BASE_BYTES],
    e: Expected,
    ordinal: usize,
) -> Result<Option<Certificate>> {
    if ordinal >= 64 {
        return Err(Error::InvalidInput);
    }
    let raw = &bytes[CERTIFICATES + ordinal * CERTIFICATE_BYTES
        ..CERTIFICATES + (ordinal + 1) * CERTIFICATE_BYTES];
    decode_certificate(raw.try_into().expect("fixed certificate"), e, ordinal)
}

/// Raw layout validation only. In particular self-consistent hashes are not an
/// INIT proof, and this type cannot construct the selected recovery owner.
pub(in crate::storage) fn decode_layout(
    input: &[u8],
    expected: Expected,
) -> Result<LayoutView<'_>> {
    let bytes: &[u8; BASE_BYTES] = input.try_into().map_err(|_| Error::Corrupt)?;
    if expected.profile.format() != 3 || get16(bytes, 4) != 4 {
        return Err(Error::Unsupported);
    }
    if content_digest(&bytes[..TRAILER]) != bytes[TRAILER..] {
        return Err(Error::Corrupt);
    }
    let h = read_header_format(bytes, expected, HeaderMode::Row)?;
    let mut end = 4096;
    for (index, (offset, length, count, width)) in SECTIONS.iter().copied().enumerate() {
        let at = 1280 + index * 64;
        if !zero(&bytes[end..offset])
            || get16(bytes, at) != index as u16 + 1
            || get16(bytes, at + 2) != 0
            || get32(bytes, at + 4) != offset as u32
            || get32(bytes, at + 8) != length as u32
            || get32(bytes, at + 12) != count as u32
            || get32(bytes, at + 16) != width as u32
            || !zero(&bytes[at + 20..at + 32])
            || bytes[at + 32..at + 64] != content_digest(&bytes[offset..offset + length])
        {
            return Err(Error::Corrupt);
        }
        end = offset + length;
    }
    if !zero(&bytes[end..TRAILER]) {
        return Err(Error::Corrupt);
    }
    let allocator = allocation_page::decode_allocation_page(
        image(bytes, 0),
        identity(expected, 0, one(), 1),
        expected.profile.spec().namespace,
    )?;
    if allocator.header().lsn.0 > h.capture_lsn {
        return Err(Error::OrderMismatch);
    }
    let mut status_mask = 0u64;
    for ordinal in 0..64 {
        let cert = certificate(bytes, expected, ordinal)?;
        let provenance: &[u8; INIT_BYTES] = bytes
            [PROVENANCE + ordinal * INIT_BYTES..PROVENANCE + (ordinal + 1) * INIT_BYTES]
            .try_into()
            .expect("fixed record");
        match allocator.entry(ordinal)? {
            AllocationEntry::Free(_) | AllocationEntry::Reserved { .. } => {
                if ordinal < 9
                    || cert.is_some()
                    || !zero(provenance)
                    || !zero(image(bytes, ordinal))
                {
                    return Err(Error::IdentityMismatch);
                }
            }
            AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind,
            } => {
                let cert = cert.ok_or(Error::IdentityMismatch)?;
                if cert.header.identity.page.incarnation != generation
                    || cert.header.identity.owner != object
                    || cert.header.identity.owner_generation != object_generation
                    || cert.header.kind != kind
                    || cert.header.lsn.0 > h.capture_lsn
                {
                    return Err(Error::IdentityMismatch);
                }
                match_certificate(image(bytes, ordinal), cert)?;
                if ordinal < 9 {
                    if !zero(provenance) {
                        return Err(Error::ProvenanceMismatch);
                    }
                } else {
                    validate_provenance(provenance, cert, expected)?;
                }
                if kind == PageKind::TransactionStatus {
                    status_mask |= 1 << ordinal;
                }
            }
        }
    }
    if status_mask != h.status_mask {
        return Err(Error::IdentityMismatch);
    }
    let (_, root) = row::decode_root_page(image(bytes, 1), expected.profile.spec().namespace)?;
    if root.masks()[1] != h.leased
        || root.masks()[2] != h.ready
        || root.revision() != h.root_revision
        || h.aging_groups.checked_add(1) != Some(root.revision())
    {
        return Err(Error::OrderMismatch);
    }
    let mut active = 0;
    for index in 0..5 {
        let page = row::decode_jobs_page(
            image(bytes, index + 2),
            expected.profile.spec().namespace,
            index as u8,
        )?;
        if page.header().lsn.0 > root.apply() {
            return Err(Error::OrderMismatch);
        }
        for slot in 0..page.len() {
            let job = row::decode_job(
                page.cell(slot).ok_or(Error::Corrupt)?,
                expected.profile.spec().namespace,
            )?;
            active += u32::from(job.state() == row::JobState::TerminalLeased);
        }
    }
    if active != h.active_transactions {
        return Err(Error::IdentityMismatch);
    }
    let view = LayoutView { bytes, expected };
    validate_pairs(view)?;
    Ok(view)
}

fn validate_pairs(view: LayoutView<'_>) -> Result<()> {
    for ordinal in 9..64 {
        let Some(provenance) = view.provenance(ordinal)? else {
            continue;
        };
        let Some(pair) = provenance.pair() else {
            continue;
        };
        let other = pair[usize::from(provenance.role() == 3)];
        let other = view
            .provenance(other.page_no as usize)?
            .ok_or(Error::ProvenanceMismatch)?;
        if other.role() == provenance.role()
            || other.pair() != Some(pair)
            || other.source() != provenance.source()
            || other.object() != provenance.object()
            || other.init_lsn() != provenance.init_lsn()
            || other.reserve_lsn() != provenance.reserve_lsn()
            || other.writer_epoch() != provenance.writer_epoch()
            || other.frame_digest() != provenance.frame_digest()
        {
            return Err(Error::ProvenanceMismatch);
        }
    }
    Ok(())
}

/// Unpublished staging bytes. This packs exact captured sections and validates
/// only the layout; the original capture must close graph/credits/provenance
/// before any CURRENT publication. Output may change on error.
pub(in crate::storage) struct Sections<'a> {
    pub(in crate::storage) descriptors: &'a [u8; DESCRIPTOR_BYTES],
    pub(in crate::storage) certificates: &'a [[u8; CERTIFICATE_BYTES]; 64],
    pub(in crate::storage) provenance: &'a [[u8; INIT_BYTES]; 64],
    pub(in crate::storage) inventory: &'a [u8; PAGE_BYTES],
    pub(in crate::storage) images: &'a [[u8; PAGE_BYTES]; 64],
}
pub(in crate::storage) fn encode_layout<'a>(
    h: Header,
    expected: Expected,
    sections: Sections<'_>,
    output: &'a mut [u8; BASE_BYTES],
) -> Result<LayoutView<'a>> {
    if h.format != BaseFormat::V4RowMissing || expected.profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    super::validate_header(h, expected)?;
    output.fill(0);
    write_header(h, expected, &mut output[..4096])?;
    output[BANK..BANK + DESCRIPTOR_BYTES].copy_from_slice(sections.descriptors);
    for ordinal in 0..64 {
        output[CERTIFICATES + ordinal * CERTIFICATE_BYTES
            ..CERTIFICATES + (ordinal + 1) * CERTIFICATE_BYTES]
            .copy_from_slice(&sections.certificates[ordinal]);
        output[PROVENANCE + ordinal * INIT_BYTES..PROVENANCE + (ordinal + 1) * INIT_BYTES]
            .copy_from_slice(&sections.provenance[ordinal]);
        output[ARENA + ordinal * PAGE_BYTES..ARENA + (ordinal + 1) * PAGE_BYTES]
            .copy_from_slice(&sections.images[ordinal]);
    }
    output[INVENTORY..INVENTORY + PAGE_BYTES].copy_from_slice(sections.inventory);
    for (index, (offset, length, count, width)) in SECTIONS.iter().copied().enumerate() {
        let at = 1280 + index * 64;
        put16(output, at, index as u16 + 1);
        put32(output, at + 4, offset as u32);
        put32(output, at + 8, length as u32);
        put32(output, at + 12, count as u32);
        put32(output, at + 16, width as u32);
        let digest = content_digest(&output[offset..offset + length]);
        output[at + 32..at + 64].copy_from_slice(&digest);
    }
    let digest = content_digest(&output[..TRAILER]);
    output[TRAILER..].copy_from_slice(&digest);
    decode_layout(output, expected)
}

#[cfg(test)]
#[path = "row_tests.rs"]
mod tests;
