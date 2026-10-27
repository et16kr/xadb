//! Shared exact-image certificate and historical INIT record grammar for
//! Base3/Base4. These observations confer no original INIT or selection authority.
use super::{
    CERTIFICATE_BYTES, Certificate, Error, Expected, INIT_BYTES, InitProvenance, PAGE_BYTES,
    PageHeader, PageIdentity, PageKind, PageLsn, PageRef, ReservationOwner, Result, SystemWorkRef,
    TxnRef, content_digest, first_file, get, get32, hash, one, page, put, zero,
};
use crate::storage::allocation_page::SystemWorkKind;
use foundation::identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch};

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

pub(super) fn validate_provenance(
    raw: &[u8; INIT_BYTES],
    cert: Certificate,
    e: Expected,
) -> Result<()> {
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
