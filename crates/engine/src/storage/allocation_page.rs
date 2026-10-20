//! One fixed extent's allocation representation, without allocator effects.
//! See `devdoc/contracts/dev3-allocation-format.md`. No token, pin, quota,
//! file length, owner-directory resolution, or durability is granted here.
// Remove when the first actual allocation consumer connects this format.
#![allow(dead_code)]

use super::{
    page::{
        FileId, PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind, PageRef, PageView,
        decode_page, encode_page,
    },
    transaction_record::TxnRef,
};
use crate::api::{DatabaseId, Namespace};
use xadb_foundation::identity::{Generation, ObjectId};

pub(crate) const EXTENT_PAGES: usize = 64;
pub(crate) const ALLOCATION_HEADER_BYTES: usize = 64;
pub(crate) const ALLOCATION_ENTRY_BYTES: usize = 96;
pub(crate) const ALLOCATION_CELL_BYTES: usize =
    ALLOCATION_HEADER_BYTES + EXTENT_PAGES * ALLOCATION_ENTRY_BYTES;
pub(crate) const SYSTEM_WORK_REF_BYTES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AllocationError {
    InvalidInput,
    UnsupportedFormat,
    Corrupt,
    IdentityMismatch,
    Exhausted,
    Page(PageError),
}

impl From<PageError> for AllocationError {
    fn from(error: PageError) -> Self {
        Self::Page(error)
    }
}

/// Arithmetic geometry, not proof that a file or extent exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExtentAddress {
    metadata: PageRef,
}

impl ExtentAddress {
    pub(crate) fn new(file: FileId, base: u64) -> Result<Self, AllocationError> {
        if !base.is_multiple_of(EXTENT_PAGES as u64)
            || base
                .checked_add(EXTENT_PAGES as u64)
                .and_then(|end| end.checked_mul(PAGE_BYTES as u64))
                .is_none()
        {
            return Err(AllocationError::InvalidInput);
        }
        Ok(Self {
            metadata: PageRef {
                file,
                page_no: base,
                incarnation: Generation::new(1).map_err(|_| AllocationError::InvalidInput)?,
            },
        })
    }

    pub(crate) fn containing(page: PageRef) -> Result<Self, AllocationError> {
        Self::new(
            page.file,
            page.page_no / EXTENT_PAGES as u64 * EXTENT_PAGES as u64,
        )
    }

    pub(crate) fn metadata_ref(self) -> PageRef {
        self.metadata
    }

    pub(crate) fn page_ref(
        self,
        index: usize,
        generation: Generation,
    ) -> Result<PageRef, AllocationError> {
        if index >= EXTENT_PAGES || (index == 0 && generation.get() != 1) {
            return Err(AllocationError::InvalidInput);
        }
        Ok(PageRef {
            page_no: self.metadata.page_no + index as u64,
            incarnation: generation,
            ..self.metadata
        })
    }

    fn index(self, page: PageRef) -> Result<usize, AllocationError> {
        if page.file != self.metadata.file {
            return Err(AllocationError::IdentityMismatch);
        }
        let index = page
            .page_no
            .checked_sub(self.metadata.page_no)
            .filter(|index| *index < EXTENT_PAGES as u64)
            .ok_or(AllocationError::IdentityMismatch)?;
        Ok(index as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SystemWorkKind {
    AllocationMaintenance = 1,
    StructuralChange = 2,
    Aging = 3,
    Checkpoint = 4,
}

impl SystemWorkKind {
    pub(crate) fn from_byte(value: u8) -> Result<Self, AllocationError> {
        match value {
            1 => Ok(Self::AllocationMaintenance),
            2 => Ok(Self::StructuralChange),
            3 => Ok(Self::Aging),
            4 => Ok(Self::Checkpoint),
            _ => Err(AllocationError::Corrupt),
        }
    }
}

/// Durable logical job identity. This codec does not resolve a job descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemWorkRef {
    namespace: Namespace,
    kind: SystemWorkKind,
    id: u64,
    generation: Generation,
}

impl SystemWorkRef {
    pub(crate) fn new(
        namespace: Namespace,
        kind: SystemWorkKind,
        id: u64,
        generation: Generation,
    ) -> Result<Self, AllocationError> {
        if namespace.timeline == 0 || id == 0 {
            return Err(AllocationError::InvalidInput);
        }
        Ok(Self {
            namespace,
            kind,
            id,
            generation,
        })
    }

    pub(crate) fn namespace(self) -> Namespace {
        self.namespace
    }

    pub(crate) fn kind(self) -> SystemWorkKind {
        self.kind
    }

    pub(crate) fn id(self) -> u64 {
        self.id
    }

    pub(crate) fn generation(self) -> Generation {
        self.generation
    }

    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), AllocationError> {
        if output.len() != SYSTEM_WORK_REF_BYTES {
            return Err(AllocationError::InvalidInput);
        }
        output.fill(0);
        output[..16].copy_from_slice(&self.namespace.database.bytes());
        put_u64(output, 16, self.namespace.timeline);
        output[24] = self.kind as u8;
        put_u64(output, 32, self.id);
        put_u64(output, 40, self.generation.get());
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, AllocationError> {
        if input.len() != SYSTEM_WORK_REF_BYTES || !zero(&input[25..32]) {
            return Err(AllocationError::Corrupt);
        }
        let mut database = [0; 16];
        database.copy_from_slice(&input[..16]);
        Self::new(
            Namespace {
                database: DatabaseId::new(database).map_err(|_| AllocationError::Corrupt)?,
                timeline: read_u64(input, 16),
            },
            SystemWorkKind::from_byte(input[24])?,
            read_u64(input, 32),
            generation(read_u64(input, 40))?,
        )
        .map_err(|_| AllocationError::Corrupt)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservationOwner {
    Transaction(TxnRef),
    System(SystemWorkRef),
}

impl ReservationOwner {
    fn namespace(self) -> Namespace {
        match self {
            Self::Transaction(reference) => Namespace {
                database: reference.id().database,
                timeline: reference.id().timeline,
            },
            Self::System(reference) => reference.namespace,
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Transaction(_) => 1,
            Self::System(_) => 2,
        }
    }

    fn encode(self, output: &mut [u8]) -> Result<(), AllocationError> {
        match self {
            Self::Transaction(reference) => reference.encode(output).map_err(AllocationError::Page),
            Self::System(reference) => reference.encode(output),
        }
    }
}

/// Nonzero identities are checked value types. Reservation and object ownership
/// are separate variants, so an allocated page cannot retain a claimant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AllocationEntry {
    Free(Generation),
    Reserved {
        generation: Generation,
        claimant: ReservationOwner,
    },
    Allocated {
        generation: Generation,
        object: ObjectId,
        object_generation: Generation,
        kind: PageKind,
    },
}

impl AllocationEntry {
    pub(crate) fn incarnation(self) -> Generation {
        match self {
            Self::Free(generation)
            | Self::Reserved { generation, .. }
            | Self::Allocated { generation, .. } => generation,
        }
    }

    /// Computes a value only. Does not reserve a page or persist a generation.
    pub(crate) fn next_reservation_generation(self) -> Result<Generation, AllocationError> {
        let Self::Free(generation) = self else {
            return Err(AllocationError::InvalidInput);
        };
        generation.next().map_err(|_| AllocationError::Exhausted)
    }

    fn validate(self, namespace: Namespace) -> Result<(), AllocationError> {
        if namespace.timeline == 0 {
            return Err(AllocationError::InvalidInput);
        }
        if let Self::Reserved { claimant, .. } = self {
            if claimant.namespace() != namespace {
                return Err(AllocationError::IdentityMismatch);
            }
            // Preflight the existing TxnRef encoder; fields are private and
            // immutable, and the exact same 48-byte input is used when writing.
            claimant.encode(&mut [0; 48])?;
        }
        Ok(())
    }

    pub(crate) fn encode(
        self,
        namespace: Namespace,
        output: &mut [u8],
    ) -> Result<(), AllocationError> {
        if output.len() != ALLOCATION_ENTRY_BYTES {
            return Err(AllocationError::InvalidInput);
        }
        self.validate(namespace)?;
        let mut bytes = [0; ALLOCATION_ENTRY_BYTES];
        put_u64(&mut bytes, 8, self.incarnation().get());
        match self {
            Self::Free(_) => {}
            Self::Reserved { claimant, .. } => {
                bytes[0] = 1;
                bytes[2] = claimant.tag();
                claimant.encode(&mut bytes[32..80])?;
            }
            Self::Allocated {
                object,
                object_generation,
                kind,
                ..
            } => {
                bytes[0] = 2;
                bytes[1] = kind as u8;
                put_u64(&mut bytes, 16, object.get());
                put_u64(&mut bytes, 24, object_generation.get());
            }
        }
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8], namespace: Namespace) -> Result<Self, AllocationError> {
        if input.len() != ALLOCATION_ENTRY_BYTES || !zero(&input[3..8]) || !zero(&input[80..96]) {
            return Err(AllocationError::Corrupt);
        }
        let generation = generation(read_u64(input, 8))?;
        let entry = match input[0] {
            0 if input[1] == 0 && input[2] == 0 && zero(&input[16..80]) => Self::Free(generation),
            1 if input[1] == 0 && zero(&input[16..32]) => {
                let claimant = match input[2] {
                    1 => ReservationOwner::Transaction(
                        TxnRef::decode(&input[32..80]).map_err(|_| AllocationError::Corrupt)?,
                    ),
                    2 => ReservationOwner::System(SystemWorkRef::decode(&input[32..80])?),
                    _ => return Err(AllocationError::Corrupt),
                };
                Self::Reserved {
                    generation,
                    claimant,
                }
            }
            2 if input[2] == 0 && zero(&input[32..80]) => Self::Allocated {
                generation,
                object: ObjectId::new(read_u64(input, 16)).map_err(|_| AllocationError::Corrupt)?,
                object_generation: generation_from_field(input, 24)?,
                kind: PageKind::from_byte(input[1]).map_err(|_| AllocationError::Corrupt)?,
            },
            _ => return Err(AllocationError::Corrupt),
        };
        entry.validate(namespace)?;
        Ok(entry)
    }
}

fn checked_header(
    header: PageHeader,
    namespace: Namespace,
) -> Result<ExtentAddress, AllocationError> {
    if header.kind != PageKind::Allocation || header.level != 0 || namespace.timeline == 0 {
        return Err(AllocationError::InvalidInput);
    }
    if namespace.database != header.identity.database {
        return Err(AllocationError::IdentityMismatch);
    }
    let extent = ExtentAddress::new(header.identity.page.file, header.identity.page.page_no)?;
    if header.identity.page != extent.metadata_ref() {
        return Err(AllocationError::InvalidInput);
    }
    Ok(extent)
}

fn check_entry(
    header: PageHeader,
    index: usize,
    entry: AllocationEntry,
) -> Result<(), AllocationError> {
    if index == 0 {
        if entry
            != (AllocationEntry::Allocated {
                generation: header.identity.page.incarnation,
                object: header.identity.owner,
                object_generation: header.identity.owner_generation,
                kind: PageKind::Allocation,
            })
        {
            return Err(AllocationError::Corrupt);
        }
    } else if matches!(
        entry,
        AllocationEntry::Allocated {
            kind: PageKind::Allocation,
            ..
        }
    ) {
        return Err(AllocationError::Corrupt);
    }
    Ok(())
}

fn parse_inventory(
    cell: &[u8],
    header: PageHeader,
    namespace: Namespace,
) -> Result<ExtentAddress, AllocationError> {
    let extent = checked_header(header, namespace)?;
    if cell.len() != ALLOCATION_CELL_BYTES || &cell[..4] != b"XAL1" {
        return Err(AllocationError::Corrupt);
    }
    if read_u16(cell, 4) != 1 {
        return Err(AllocationError::UnsupportedFormat);
    }
    if read_u16(cell, 6) != EXTENT_PAGES as u16
        || read_u64(cell, 8) != header.identity.page.page_no
        || read_u16(cell, 16) != EXTENT_PAGES as u16
        || read_u16(cell, 18) != ALLOCATION_ENTRY_BYTES as u16
        || !zero(&cell[20..32])
        || !zero(&cell[56..64])
        || PageRef::decode(&cell[32..56]).map_err(|_| AllocationError::Corrupt)?
            != header.identity.page
    {
        return Err(AllocationError::Corrupt);
    }
    for index in 0..EXTENT_PAGES {
        let start = ALLOCATION_HEADER_BYTES + index * ALLOCATION_ENTRY_BYTES;
        let entry =
            AllocationEntry::decode(&cell[start..start + ALLOCATION_ENTRY_BYTES], namespace)?;
        check_entry(header, index, entry)?;
    }
    Ok(extent)
}

/// Validate all 64 caller-owned entries before writing any output byte.
pub(crate) fn encode_allocation_cell(
    header: PageHeader,
    namespace: Namespace,
    entries: &[AllocationEntry],
    output: &mut [u8],
) -> Result<usize, AllocationError> {
    checked_header(header, namespace)?;
    if entries.len() != EXTENT_PAGES || output.len() < ALLOCATION_CELL_BYTES {
        return Err(AllocationError::InvalidInput);
    }
    for (index, entry) in entries.iter().copied().enumerate() {
        entry.validate(namespace)?;
        check_entry(header, index, entry)?;
    }
    let mut reference = [0; 24];
    header.identity.page.encode(&mut reference)?;
    output[..ALLOCATION_HEADER_BYTES].fill(0);
    output[..4].copy_from_slice(b"XAL1");
    output[4..6].copy_from_slice(&1u16.to_le_bytes());
    output[6..8].copy_from_slice(&(EXTENT_PAGES as u16).to_le_bytes());
    put_u64(output, 8, header.identity.page.page_no);
    output[16..18].copy_from_slice(&(EXTENT_PAGES as u16).to_le_bytes());
    output[18..20].copy_from_slice(&(ALLOCATION_ENTRY_BYTES as u16).to_le_bytes());
    output[32..56].copy_from_slice(&reference);
    for (index, entry) in entries.iter().copied().enumerate() {
        let start = ALLOCATION_HEADER_BYTES + index * ALLOCATION_ENTRY_BYTES;
        // Every immutable entry and namespace passed the same preflight; each
        // exact 96-byte destination is in bounds. No new failure can occur.
        entry.encode(
            namespace,
            &mut output[start..start + ALLOCATION_ENTRY_BYTES],
        )?;
    }
    Ok(ALLOCATION_CELL_BYTES)
}

pub(crate) fn encode_allocation_page(
    header: PageHeader,
    namespace: Namespace,
    cell: &[u8],
    output: &mut [u8],
) -> Result<(), AllocationError> {
    if output.len() != PAGE_BYTES {
        return Err(AllocationError::InvalidInput);
    }
    parse_inventory(cell, header, namespace)?;
    encode_page(header, &[cell], output)?;
    Ok(())
}

pub(crate) struct AllocationPage<'a> {
    page: PageView<'a>,
    extent: ExtentAddress,
    namespace: Namespace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AllocationSummary {
    pub(crate) reusable_free: usize,
    pub(crate) exhausted_free: usize,
    pub(crate) reserved_transaction: usize,
    pub(crate) reserved_system: usize,
    pub(crate) allocated: usize,
}

impl AllocationPage<'_> {
    pub(crate) fn header(&self) -> PageHeader {
        self.page.header()
    }

    pub(crate) fn entry(&self, index: usize) -> Result<AllocationEntry, AllocationError> {
        if index >= EXTENT_PAGES {
            return Err(AllocationError::InvalidInput);
        }
        let cell = self.page.cell(0).ok_or(AllocationError::Corrupt)?;
        let start = ALLOCATION_HEADER_BYTES + index * ALLOCATION_ENTRY_BYTES;
        AllocationEntry::decode(&cell[start..start + ALLOCATION_ENTRY_BYTES], self.namespace)
    }

    pub(crate) fn summary(&self) -> Result<AllocationSummary, AllocationError> {
        let mut summary = AllocationSummary::default();
        for index in 0..EXTENT_PAGES {
            match self.entry(index)? {
                AllocationEntry::Free(generation) if generation.get() == u64::MAX => {
                    summary.exhausted_free += 1;
                }
                AllocationEntry::Free(_) => summary.reusable_free += 1,
                AllocationEntry::Reserved {
                    claimant: ReservationOwner::Transaction(_),
                    ..
                } => summary.reserved_transaction += 1,
                AllocationEntry::Reserved {
                    claimant: ReservationOwner::System(_),
                    ..
                } => summary.reserved_system += 1,
                AllocationEntry::Allocated { .. } => summary.allocated += 1,
            }
        }
        Ok(summary)
    }

    pub(crate) fn lookup_allocated(
        &self,
        expected: PageIdentity,
        expected_kind: PageKind,
    ) -> Result<(), AllocationError> {
        if expected.database != self.header().identity.database {
            return Err(AllocationError::IdentityMismatch);
        }
        let index = self.extent.index(expected.page)?;
        if self.entry(index)?
            != (AllocationEntry::Allocated {
                generation: expected.page.incarnation,
                object: expected.owner,
                object_generation: expected.owner_generation,
                kind: expected_kind,
            })
        {
            return Err(AllocationError::IdentityMismatch);
        }
        Ok(())
    }

    pub(crate) fn lookup_reserved(
        &self,
        expected: PageRef,
        expected_claimant: ReservationOwner,
    ) -> Result<(), AllocationError> {
        let index = self.extent.index(expected)?;
        if self.entry(index)?
            != (AllocationEntry::Reserved {
                generation: expected.incarnation,
                claimant: expected_claimant,
            })
        {
            return Err(AllocationError::IdentityMismatch);
        }
        Ok(())
    }
}

/// Expected metadata identity comes from trusted control and fixed geometry.
/// Every entry is validated, including entries the caller will not look up.
pub(crate) fn decode_allocation_page(
    input: &[u8],
    expected: PageIdentity,
    namespace: Namespace,
) -> Result<AllocationPage<'_>, AllocationError> {
    let page = decode_page(input, expected, PageKind::Allocation)?;
    if page.len() != 1 {
        return Err(AllocationError::Corrupt);
    }
    let extent = parse_inventory(
        page.cell(0).ok_or(AllocationError::Corrupt)?,
        page.header(),
        namespace,
    )?;
    Ok(AllocationPage {
        page,
        extent,
        namespace,
    })
}

fn generation(value: u64) -> Result<Generation, AllocationError> {
    Generation::new(value).map_err(|_| AllocationError::Corrupt)
}

fn generation_from_field(bytes: &[u8], offset: usize) -> Result<Generation, AllocationError> {
    generation(read_u64(bytes, offset))
}

fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut number = [0; 8];
    number.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(number)
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
#[path = "allocation_page_tests.rs"]
mod tests;
