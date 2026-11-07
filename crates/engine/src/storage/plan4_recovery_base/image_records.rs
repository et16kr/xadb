//! Shared exact-image certificate and historical INIT record grammar for
//! Base3/Base4. These observations confer no original INIT or selection authority.
use super::{
    CERTIFICATE_BYTES, Certificate, Error, Expected, INIT_BYTES, InitProvenance, PAGE_BYTES,
    PageHeader, PageIdentity, PageKind, PageLsn, PageRef, ReservationOwner, Result, SystemWorkRef,
    TxnRef, content_digest, first_file, get, get32, hash, one, page, put, zero,
};
use crate::storage::allocation_page::SystemWorkKind;
use foundation::identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch};

/// Historical birth shape only, never a current root or installation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum StructuralOperation {
    RootSplit,
    ChildSplit,
}

/// A role 7 claim's chain position (dev3-row-overflow.md#base4): the
/// table anchor, the chunk index, the chain's count and its total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) struct ChunkInit {
    pub(in crate::storage) anchor: PageRef,
    pub(in crate::storage) index: u16,
    pub(in crate::storage) count: u16,
    pub(in crate::storage) total: u32,
}

/// The overflow row bounds of a role 7 claim and its chunk payload.
const CHUNK_TOTAL: std::ops::RangeInclusive<u32> = 2_049..=131_072;
const CHUNK_PAYLOAD: u32 = 16_092;

#[derive(Clone, Copy)]
pub(in crate::storage) struct StructuralInit<'a> {
    bytes: &'a [u8; INIT_BYTES],
}
impl<'a> StructuralInit<'a> {
    pub(in crate::storage) fn level(self) -> u8 {
        self.bytes[3]
    }
    pub(in crate::storage) fn anchor(self) -> PageRef {
        PageRef::decode(&self.bytes[208..232]).expect("validated anchor")
    }
    pub(in crate::storage) fn left(self) -> PageRef {
        PageRef::decode(&self.bytes[232..256]).expect("validated historical left")
    }
    pub(in crate::storage) fn peer(self) -> Option<PageRef> {
        (self.operation() == StructuralOperation::RootSplit)
            .then(|| PageRef::decode(&self.bytes[256..280]).expect("validated historical peer"))
    }
    pub(in crate::storage) fn operation(self) -> StructuralOperation {
        match get(self.bytes, 280) {
            1 => StructuralOperation::RootSplit,
            2 => StructuralOperation::ChildSplit,
            _ => unreachable!("validated structural operation"),
        }
    }
    pub(in crate::storage) fn revisions(self) -> [u64; 2] {
        [get(self.bytes, 288), get(self.bytes, 296)]
    }
    pub(in crate::storage) fn core_digest(self) -> &'a [u8; 32] {
        self.bytes[304..336].try_into().expect("fixed field")
    }
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
        (self.bytes[4] == 2 || matches!(self.role(), 5 | 6)).then(|| ObjectHandle {
            object: ObjectId::new(get(self.bytes, 184)).expect("validated object"),
            generation: Generation::new(get(self.bytes, 192)).expect("validated generation"),
            schema_epoch: SchemaEpoch::new(get(self.bytes, 200)).expect("validated schema"),
        })
    }
    /// A CREATE claim's (anchor, root) pair: roles 3 and 4 only. A role 7
    /// claim is also a Transaction source but names a chain position
    /// (`chunk`) in these bytes (dev3-row-overflow.md#base4).
    pub(in crate::storage) fn pair(self) -> Option<[PageRef; 2]> {
        matches!(self.role(), 3 | 4).then(|| {
            [
                PageRef::decode(&self.bytes[208..232]).expect("validated Anchor"),
                PageRef::decode(&self.bytes[232..256]).expect("validated RowRoot"),
            ]
        })
    }
    pub(in crate::storage) fn chunk(self) -> Option<ChunkInit> {
        (self.role() == 7).then(|| ChunkInit {
            anchor: PageRef::decode(&self.bytes[208..232]).expect("validated anchor"),
            index: u16::from_le_bytes([self.bytes[232], self.bytes[233]]),
            count: u16::from_le_bytes([self.bytes[234], self.bytes[235]]),
            total: get32(self.bytes, 236),
        })
    }
    pub(in crate::storage) fn structural(self) -> Option<StructuralInit<'a>> {
        matches!(self.role(), 5 | 6).then_some(StructuralInit { bytes: self.bytes })
    }
}

pub(super) fn decode_certificate(
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
    if (kind == PageKind::RowInternal && (e.profile.format() != 3 || !(1..=15).contains(&raw[2])))
        || (kind != PageKind::RowInternal && raw[2] != 0)
        || (kind == PageKind::Overflow && e.profile.format() != 3)
    {
        return Err(Error::Unsupported);
    }
    let valid_role = match ordinal {
        0 => kind == PageKind::Allocation && object == 1,
        1 => kind == PageKind::RetentionRoot && object == 4,
        2..=6 => kind == PageKind::RetentionJobs && object == 4,
        7 => kind == PageKind::TreeAnchor && object == 5,
        8 => kind == PageKind::CatalogLeaf && object == 5,
        _ => match kind {
            PageKind::TransactionStatus => object == 2,
            PageKind::Undo => object == 3,
            // A table's overflow chunk (dev3-row-overflow.md#base4).
            PageKind::TreeAnchor
            | PageKind::RowLeaf
            | PageKind::RowInternal
            | PageKind::Overflow => object >= 6,
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
            level: raw[2],
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
    record[2] = header.level;
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

pub(super) fn validate_provenance(
    raw: &[u8; INIT_BYTES],
    cert: Certificate,
    e: Expected,
) -> Result<()> {
    let page = cert.header.identity.page;
    if raw[0] != 1
        || raw[2] != cert.header.kind as u8
        || raw[3] != cert.header.level
        || !zero(&raw[5..8])
        || !zero(&raw[60..64])
        || PageRef::decode(&raw[8..32])? != page
        || get(raw, 32) != cert.header.identity.owner.get()
        || get(raw, 40) != 1
        || get(raw, 48) == 0
        || get32(raw, 56) == 0
        || get(raw, 48) > cert.header.lsn.0
        || get(raw, 176) == 0
    {
        return Err(Error::ProvenanceMismatch);
    }
    if matches!(raw[1], 5 | 6) {
        return validate_structural(raw, cert, e);
    }
    if raw[3] != 0 || !zero(&raw[256..]) || get(raw, 176) >= get(raw, 48) {
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
        // A ChunkWrite's chunk (dev3-row-overflow.md#base4): the inserting
        // transaction, the table, its anchor and the chain position. That
        // index, count and total equal the chunk's cell is the layout's check.
        (7, 2) => {
            let source = TxnRef::decode(&raw[128..176])?;
            let anchor = PageRef::decode(&raw[208..232])?;
            let index = u16::from_le_bytes([raw[232], raw[233]]);
            let count = u16::from_le_bytes([raw[234], raw[235]]);
            let total = get32(raw, 236);
            if e.profile.format() != 3
                || source.id().database != e.profile.spec().namespace.database
                || source.id().timeline != e.profile.spec().namespace.timeline
                || source.id().writer_epoch != get32(raw, 56)
                || source.generation().get() < 2
                || get(raw, 184) != cert.header.identity.owner.get()
                || get(raw, 184) < 6
                || get(raw, 192) != 1
                || get(raw, 200) != 1
                || !dynamic(anchor)
                || anchor.page_no == page.page_no
                || !CHUNK_TOTAL.contains(&total)
                || u32::from(count) != total.div_ceil(CHUNK_PAYLOAD)
                || index >= count
                || !zero(&raw[240..256])
                || cert.header.kind != PageKind::Overflow
            {
                return Err(Error::ProvenanceMismatch);
            }
        }
        _ => return Err(Error::Unsupported),
    }
    Ok(())
}

fn dynamic(reference: PageRef) -> bool {
    reference.file == first_file()
        && (9..64).contains(&reference.page_no)
        && reference.incarnation.get() >= 2
}

fn validate_structural(raw: &[u8; INIT_BYTES], cert: Certificate, e: Expected) -> Result<()> {
    if e.profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    let source = SystemWorkRef::decode(&raw[128..176])?;
    let anchor = PageRef::decode(&raw[208..232])?;
    let left = PageRef::decode(&raw[232..256])?;
    let operation = get(raw, 280);
    let peer = match operation {
        1 => Some(PageRef::decode(&raw[256..280])?),
        2 if raw[1] == 5 && zero(&raw[256..280]) => None,
        _ => return Err(Error::ProvenanceMismatch),
    };
    if raw[4] != 1
        || source.namespace() != e.profile.spec().namespace
        || source.kind() != SystemWorkKind::StructuralChange
        || source.id() != get(raw, 48)
        || source.generation().get() != u64::from(get32(raw, 56))
        || get(raw, 176) != get(raw, 48)
        || get(raw, 184) != cert.header.identity.owner.get()
        || get(raw, 184) < 6
        || get(raw, 192) != 1
        || get(raw, 200) != 1
        || get(raw, 288) == 0
        || get(raw, 288).checked_add(1) != Some(get(raw, 296))
        || !zero(&raw[336..])
        || (raw[1] == 5
            && !((cert.header.kind == PageKind::RowLeaf && raw[3] == 0)
                || (cert.header.kind == PageKind::RowInternal && (1..=14).contains(&raw[3]))))
        || (raw[1] == 6
            && (cert.header.kind != PageKind::RowInternal || !(1..=15).contains(&raw[3])))
    {
        return Err(Error::ProvenanceMismatch);
    }
    // Aliasing is physical: changing only an incarnation must not hide it.
    let references = [
        Some(cert.header.identity.page),
        Some(anchor),
        Some(left),
        peer,
    ];
    for (index, reference) in references.iter().copied().enumerate() {
        let Some(reference) = reference else { continue };
        if !dynamic(reference)
            || references[..index]
                .iter()
                .flatten()
                .any(|other| other.file == reference.file && other.page_no == reference.page_no)
        {
            return Err(Error::ProvenanceMismatch);
        }
    }
    Ok(())
}

/// Shared provenance grammar plus its borrowed accessors. This is an explicit
/// sum of both exclusive branches' typed carriers and codec results, not an ABI
/// stack sample. The caller accounts for its own layout/group loop frames.
pub(super) fn provenance_source_bytes() -> usize {
    use crate::storage::{allocation_page::AllocationError, page::PageError};
    use std::mem::size_of;
    2 * size_of::<(&[u8; INIT_BYTES], Certificate, Expected)>()
        + 2 * size_of::<SystemWorkRef>()
        + 2 * size_of::<std::result::Result<SystemWorkRef, AllocationError>>()
        + size_of::<TxnRef>()
        + size_of::<std::result::Result<TxnRef, PageError>>()
        + size_of::<[PageRef; 2]>()
        + size_of::<[Option<PageRef>; 4]>()
        + 8 * size_of::<PageRef>()
        + 8 * size_of::<std::result::Result<PageRef, PageError>>()
        + 4 * size_of::<Option<PageRef>>()
        + 4 * size_of::<StructuralInit<'_>>()
        + 4 * size_of::<InitProvenance<'_>>()
        + 4 * size_of::<ReservationOwner>()
        + 4 * size_of::<Option<ObjectHandle>>()
        + 4 * size_of::<Result<()>>()
        + 16 * size_of::<&[u8]>()
        + 16 * size_of::<u64>()
}
