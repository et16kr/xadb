//! Exact-image v3 layout and historical INIT observations. Layout validation is
//! deliberately separate from the full semantic graph/credit/inventory closure
//! and from the original selected-source owner's provenance verification.

use super::{
    AllocationEntry, BaseFormat, CERTIFICATE_BYTES, Certificate, DESCRIPTOR_BYTES, Error, Expected,
    Header, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef, ReservationOwner,
    Result, SystemWorkRef, TxnRef, allocation_page, content_digest, first_file, get, get16, get32,
    hash, identity, match_certificate, one, page, put, put16, put32, read_header_format,
    write_header, zero,
};
use crate::storage::{allocation_page::SystemWorkKind, retention_page::catalog};
use foundation::identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch};

#[path = "catalog/graph.rs"]
mod graph;
#[path = "catalog/inventory.rs"]
pub(in crate::storage) mod inventory;
pub(in crate::storage) use graph::{
    CatalogBaseView, CatalogCensus, ReservedInventory, SourceCensus, decode, encode, source_bytes,
    source_census,
};

pub(in crate::storage) const BASE_BYTES: usize = 2_097_152;
pub(in crate::storage) const PAGE_ARENA_BYTES: usize = 64 * PAGE_BYTES;
pub(in crate::storage) const INIT_BYTES: usize = 352;
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
        read_header_format(self.bytes, self.expected, true).expect("validated immutable header")
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

/// Historical source information is not a usable current Txn/Undo pin. The
/// original source owner must match it to accepted base or retained WAL INIT.
#[derive(Clone, Copy)]
pub(in crate::storage) struct InitProvenance<'a> {
    bytes: &'a [u8; INIT_BYTES],
}
impl<'a> InitProvenance<'a> {
    pub(in crate::storage) fn bytes(self) -> &'a [u8; INIT_BYTES] {
        self.bytes
    }
    pub(in crate::storage) fn role(self) -> u8 {
        self.bytes[1]
    }
    pub(in crate::storage) fn page(self) -> PageRef {
        PageRef::decode(&self.bytes[8..32]).expect("validated page")
    }
    pub(in crate::storage) fn init_lsn(self) -> u64 {
        get(self.bytes, 48)
    }
    pub(in crate::storage) fn writer_epoch(self) -> u32 {
        get32(self.bytes, 56)
    }
    pub(in crate::storage) fn reserve_lsn(self) -> u64 {
        get(self.bytes, 176)
    }
    pub(in crate::storage) fn frame_digest(self) -> &'a [u8; 32] {
        self.bytes[64..96].try_into().expect("fixed field")
    }
    pub(in crate::storage) fn initial_digest(self) -> &'a [u8; 32] {
        self.bytes[96..128].try_into().expect("fixed field")
    }
    pub(in crate::storage) fn source(self) -> ReservationOwner {
        if self.bytes[4] == 1 {
            ReservationOwner::System(
                SystemWorkRef::decode(&self.bytes[128..176]).expect("validated System source"),
            )
        } else {
            ReservationOwner::Transaction(
                TxnRef::decode(&self.bytes[128..176]).expect("validated Tx source"),
            )
        }
    }
    pub(in crate::storage) fn object(self) -> Option<ObjectHandle> {
        (self.bytes[4] == 2).then(|| ObjectHandle {
            object: ObjectId::new(get(self.bytes, 184)).expect("validated object"),
            generation: Generation::new(get(self.bytes, 192)).expect("validated generation"),
            schema_epoch: SchemaEpoch::new(get(self.bytes, 200)).expect("validated schema"),
        })
    }
    pub(in crate::storage) fn pair(self) -> Option<[PageRef; 2]> {
        (self.bytes[4] == 2).then(|| {
            [
                PageRef::decode(&self.bytes[208..232]).expect("validated Anchor"),
                PageRef::decode(&self.bytes[232..256]).expect("validated RowRoot"),
            ]
        })
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

fn decode_certificate(
    raw: &[u8; CERTIFICATE_BYTES],
    e: Expected,
    ordinal: usize,
) -> Result<Option<Certificate>> {
    if ordinal >= 64 {
        return Err(Error::InvalidInput);
    }
    if zero(raw) {
        return Ok(None);
    }
    let kind = PageKind::from_byte(raw[1])?;
    let reference = PageRef::decode(&raw[8..32])?;
    if raw[0] != 1
        || raw[2] != 0
        || !zero(&raw[3..8])
        || !zero(&raw[88..96])
        || reference.file != first_file()
        || reference.page_no != ordinal as u64
        || get(raw, 40) != 1
        || (ordinal < 9 && reference.incarnation != one())
        || (ordinal >= 9 && reference.incarnation.get() < 2)
    {
        return Err(Error::IdentityMismatch);
    }
    let object = get(raw, 32);
    let valid_role = match ordinal {
        0 => kind == PageKind::Allocation && object == 1,
        1 => kind == PageKind::RetentionRoot && object == 4,
        2..=6 => kind == PageKind::RetentionJobs && object == 4,
        7 => kind == PageKind::TreeAnchor && object == 5,
        8 => kind == PageKind::CatalogLeaf && object == 5,
        _ => match kind {
            PageKind::TransactionStatus => object == 2,
            PageKind::Undo => object == 3,
            PageKind::TreeAnchor | PageKind::RowLeaf => object >= 6,
            _ => false,
        },
    };
    if !valid_role {
        return Err(Error::Unsupported);
    }
    Ok(Some(Certificate {
        header: PageHeader {
            identity: PageIdentity {
                database: e.profile.spec().namespace.database,
                page: reference,
                owner: ObjectId::new(object).map_err(|_| Error::IdentityMismatch)?,
                owner_generation: one(),
            },
            kind,
            level: 0,
            lsn: PageLsn(get(raw, 48)),
        },
        full_digest: hash(raw, 56),
    }))
}

/// Encode an exact current image certificate. This validates its fixed or
/// dynamic role, but does not establish its historical INIT provenance.
pub(in crate::storage) fn encode_certificate(
    expected: Expected,
    header: PageHeader,
    image: &[u8; PAGE_BYTES],
    output: &mut [u8; CERTIFICATE_BYTES],
) -> Result<()> {
    let ordinal = usize::try_from(header.identity.page.page_no).map_err(|_| Error::InvalidInput)?;
    let mut record = [0; CERTIFICATE_BYTES];
    record[0] = 1;
    record[1] = header.kind as u8;
    header.identity.page.encode(&mut record[8..32])?;
    put(&mut record, 32, header.identity.owner.get());
    put(&mut record, 40, header.identity.owner_generation.get());
    put(&mut record, 48, header.lsn.0);
    record[56..88].copy_from_slice(&content_digest(image));
    let cert = decode_certificate(&record, expected, ordinal)?.ok_or(Error::InvalidInput)?;
    if cert.header != header
        || page::decode_page(image, header.identity, header.kind)?.header() != header
    {
        return Err(Error::IdentityMismatch);
    }
    output.copy_from_slice(&record);
    Ok(())
}

fn validate_provenance(raw: &[u8; INIT_BYTES], cert: Certificate, e: Expected) -> Result<()> {
    let page = cert.header.identity.page;
    if raw[0] != 1
        || raw[2] != cert.header.kind as u8
        || raw[3] != 0
        || !zero(&raw[5..8])
        || !zero(&raw[60..64])
        || !zero(&raw[256..])
        || PageRef::decode(&raw[8..32])? != page
        || get(raw, 32) != cert.header.identity.owner.get()
        || get(raw, 40) != 1
        || get(raw, 48) == 0
        || get32(raw, 56) == 0
        || get(raw, 48) > cert.header.lsn.0
        || get(raw, 176) == 0
        || get(raw, 176) >= get(raw, 48)
    {
        return Err(Error::ProvenanceMismatch);
    }
    match (raw[1], raw[4]) {
        (1 | 2, 1) => {
            let source = SystemWorkRef::decode(&raw[128..176])?;
            if source.namespace() != e.profile.spec().namespace
                || source.kind() != SystemWorkKind::AllocationMaintenance
                || source.generation() != one()
                || !zero(&raw[184..256])
                || (raw[1] == 1 && cert.header.kind != PageKind::TransactionStatus)
                || (raw[1] == 2 && cert.header.kind != PageKind::Undo)
            {
                return Err(Error::ProvenanceMismatch);
            }
        }
        (3 | 4, 2) => {
            let source = TxnRef::decode(&raw[128..176])?;
            let pair = [
                PageRef::decode(&raw[208..232])?,
                PageRef::decode(&raw[232..256])?,
            ];
            if source.id().database != e.profile.spec().namespace.database
                || source.id().timeline != e.profile.spec().namespace.timeline
                || source.id().writer_epoch != get32(raw, 56)
                || source.generation().get() < 2
                || get(raw, 184) != cert.header.identity.owner.get()
                || get(raw, 192) != 1
                || get(raw, 200) != 1
                || pair.iter().any(|p| {
                    p.file != first_file()
                        || !(9..64).contains(&p.page_no)
                        || p.incarnation.get() < 2
                })
                || pair[0].page_no == pair[1].page_no
                || pair[usize::from(raw[1] - 3)] != page
                || (raw[1] == 3 && cert.header.kind != PageKind::TreeAnchor)
                || (raw[1] == 4 && cert.header.kind != PageKind::RowLeaf)
            {
                return Err(Error::ProvenanceMismatch);
            }
        }
        _ => return Err(Error::Unsupported),
    }
    Ok(())
}

/// Raw layout validation only. In particular self-consistent hashes are not an
/// INIT proof, and this type cannot construct the selected recovery owner.
pub(in crate::storage) fn decode_layout(
    input: &[u8],
    expected: Expected,
) -> Result<LayoutView<'_>> {
    let bytes: &[u8; BASE_BYTES] = input.try_into().map_err(|_| Error::Corrupt)?;
    if expected.profile.format() != 2 || get16(bytes, 4) != 3 {
        return Err(Error::Unsupported);
    }
    if content_digest(&bytes[..TRAILER]) != bytes[TRAILER..] {
        return Err(Error::Corrupt);
    }
    let h = read_header_format(bytes, expected, true)?;
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
    let (_, root) = catalog::decode_root_page(image(bytes, 1), expected.profile.spec().namespace)?;
    if root.masks()[1] != h.leased
        || root.masks()[2] != h.ready
        || root.revision() != h.root_revision
        || h.aging_groups.checked_add(1) != Some(root.revision())
    {
        return Err(Error::OrderMismatch);
    }
    let mut active = 0;
    for index in 0..5 {
        let page = catalog::decode_jobs_page(
            image(bytes, index + 2),
            expected.profile.spec().namespace,
            index as u8,
        )?;
        if page.header().lsn.0 > root.apply() {
            return Err(Error::OrderMismatch);
        }
        for slot in 0..page.len() {
            let job = catalog::decode_job(
                page.cell(slot).ok_or(Error::Corrupt)?,
                expected.profile.spec().namespace,
            )?;
            active += u32::from(job.state() == catalog::JobState::TerminalLeased);
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
    if h.format != BaseFormat::V3Catalog || expected.profile.format() != 2 {
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

/// Raw encode/decode carriers, excluding the caller's 2MiB/page loans. The
/// full graph decoder ends this phase before constructing its graph carrier.
pub(in crate::storage) fn layout_source_bytes() -> usize {
    use std::mem::size_of;
    640 + 1536
        + 4 * size_of::<Header>()
        + 4 * size_of::<Expected>()
        + 4 * size_of::<LayoutView<'_>>()
        + 4 * size_of::<Result<LayoutView<'_>>>()
        + 4 * size_of::<Certificate>()
        + 4 * size_of::<Result<Option<Certificate>>>()
        + 4 * size_of::<InitProvenance<'_>>()
        + 4 * size_of::<Result<Option<InitProvenance<'_>>>>()
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<page::PageView<'_>>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 4 * size_of::<AllocationEntry>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<Sections<'_>>()
        + 32 * size_of::<&[u8]>()
        + 32 * size_of::<u64>()
        + catalog::source_bytes()
}
