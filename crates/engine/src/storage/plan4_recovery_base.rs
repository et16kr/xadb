//! Self-contained XRB1 N=0 snapshot bytes, borrowed from caller maintenance.
//!
//! This module observes bytes and cross-section consistency. It does not select
//! CURRENT, prove an original INIT, restore credits, or issue VerifiedBase. The
//! selected-source owner must bind the complete digest and INIT provenance.
//!
//! The indexed ownership verifier reuses the original XOI1 checker, while the
//! Catalog and allocator bridges call the original canonical producers and
//! descriptor validators. Integration/selected-source ownership stays separate.
#![allow(dead_code)] // The selected recovery owner is being connected.

use super::{
    allocation_page::{self, AllocationEntry, AllocationError, ReservationOwner, SystemWorkRef},
    control::{ControlError, ControlKind, ControlRef, Profile, SelectedControl},
    obligation::{
        self, DescriptorSpec, ObligationError, OwnerState, RemainingBudget, ResourcePeak, WorkPhase,
    },
    ownership::{self, InventoryError, InventoryExpected, InventoryObservation, InventorySource},
    page::{
        self, FileId, PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind, PageLsn, PageRef,
    },
    retention_obligation::{self, AgingObligationError},
    retention_page::{self, JobState, JobView, RetentionError, RootView},
    semantic_retention::{self, HANDOFF_BYTES, ROLLBACK_BYTES},
    status_home::StatusHome,
    transaction_record::{RollbackState, TransactionState, TransactionStatusCell, TxnRef},
};
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectId},
};

pub(super) const BASE_BYTES: usize = 1 << 20;
pub(super) const DESCRIPTOR_COUNT: usize = 36;
pub(super) const DESCRIPTOR_BYTES: usize = DESCRIPTOR_COUNT * 1536;
pub(super) const STATUS_BYTES: usize = 352;
pub(super) const CERTIFICATE_BYTES: usize = 96;
const TRAILER: usize = BASE_BYTES - 32;
const BANK: usize = 4096;
const ALLOCATOR: usize = 65536;
const QUEUE: usize = 81920;
const STATUS: usize = 180224;
const CERTIFICATES: usize = 202752;
const INVENTORY: usize = 208896;
const INVENTORY_ROOT_CAP: usize = 2176;
const INVENTORY_CHUNK_CAP: usize = 6304;
const SECTIONS: [(usize, usize, usize, usize); 6] = [
    (BANK, DESCRIPTOR_BYTES, 36, 1536),
    (ALLOCATOR, PAGE_BYTES, 1, PAGE_BYTES),
    (QUEUE, 6 * PAGE_BYTES, 6, PAGE_BYTES),
    (STATUS, 64 * STATUS_BYTES, 64, STATUS_BYTES),
    (CERTIFICATES, 64 * CERTIFICATE_BYTES, 64, CERTIFICATE_BYTES),
    (INVENTORY, PAGE_BYTES, 1, PAGE_BYTES),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    InvalidInput,
    Unsupported,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
    ProvenanceMismatch,
    Page(PageError),
    Allocation(AllocationError),
    Control(ControlError),
    Descriptor(ObligationError),
    Retention(RetentionError),
    Aging(AgingObligationError),
    Inventory(InventoryError),
}
macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for Error {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}
from_error!(PageError, Page);
from_error!(AllocationError, Allocation);
from_error!(ControlError, Control);
from_error!(ObligationError, Descriptor);
from_error!(RetentionError, Retention);
from_error!(AgingObligationError, Aging);
from_error!(InventoryError, Inventory);
type Result<T> = std::result::Result<T, Error>;

/// Actual selected immutable geometry. These values alone confer no selection.
#[derive(Clone, Copy, Debug)]
pub(super) struct Expected {
    pub(super) profile: Profile,
    pub(super) filemap: SelectedControl,
    pub(super) bootstrap_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AllocationTerminal {
    None,
    Cancelled,
    InitializedStatus,
}
impl AllocationTerminal {
    fn byte(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Cancelled => 1,
            Self::InitializedStatus => 2,
        }
    }
}

/// Scalar capture observations. Physical charge is rechecked by the namespace
/// owner; neither it nor the DWB floor grants reservation or deletion authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Header {
    pub(super) generation: u64,
    pub(super) capture_lsn: u64,
    pub(super) previous_generation: u64,
    pub(super) previous_digest: [u8; 32],
    pub(super) source_epoch: u32,
    pub(super) source_offset: u64,
    pub(super) source_prefix_digest: [u8; 32],
    pub(super) source_local: u64,
    pub(super) system_high_water: [u64; 4],
    pub(super) active_transactions: u32,
    pub(super) active_systems: u32,
    pub(super) status_mask: u64,
    pub(super) leased: u64,
    pub(super) ready: u64,
    pub(super) root_revision: u64,
    pub(super) aging_groups: u64,
    pub(super) allocation_terminal: AllocationTerminal,
    pub(super) retired_batch: u64,
    pub(super) doublewrite_generation: u64,
    pub(super) remaining: [RemainingBudget; 5],
    pub(super) physical_bytes: u64,
    pub(super) issued_epoch: u32,
    pub(super) source_header_digest: [u8; 32],
}

/// Whole large sections remain borrowed, including all 36 encoded descriptors.
pub(super) struct Sections<'a> {
    pub(super) descriptors: &'a [u8; DESCRIPTOR_BYTES],
    pub(super) allocator: &'a [u8; PAGE_BYTES],
    pub(super) queue: &'a [[u8; PAGE_BYTES]; 6],
    pub(super) statuses: &'a [[u8; STATUS_BYTES]; 64],
    pub(super) certificates: &'a [[u8; CERTIFICATE_BYTES]; 64],
    pub(super) inventory: &'a [u8; PAGE_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Census {
    pub(super) transactions: usize,
    pub(super) systems: usize,
    pub(super) statuses: usize,
    pub(super) leased: usize,
    pub(super) ready: usize,
    pub(super) reserved: Option<PageRef>,
    pub(super) inventory: InventoryObservation,
}

#[derive(Clone, Copy)]
pub(super) struct BaseView<'a> {
    bytes: &'a [u8; BASE_BYTES],
    expected: Expected,
    header: Header,
    census: Census,
}

impl<'a> BaseView<'a> {
    pub(super) fn bytes(self) -> &'a [u8; BASE_BYTES] {
        self.bytes
    }
    pub(super) fn header(self) -> Header {
        self.header
    }
    pub(super) fn census(self) -> Census {
        self.census
    }
    /// Full-file SHA, as used by CURRENT, differs from the internal trailer SHA.
    pub(super) fn digest(self) -> [u8; 32] {
        content_digest(self.bytes)
    }
    pub(super) fn descriptor_bank(self) -> &'a [u8; DESCRIPTOR_BYTES] {
        self.bytes[BANK..BANK + DESCRIPTOR_BYTES]
            .try_into()
            .expect("fixed section")
    }
    pub(super) fn descriptor(self, slot: usize) -> Result<Option<DescriptorSpec>> {
        decode_descriptor(self.bytes, self.expected, slot)
    }
    pub(super) fn active_descriptor(self, ordinal: usize) -> Result<DescriptorSpec> {
        active_descriptor(self.bytes, self.expected, ordinal)
    }
    pub(super) fn allocator(self) -> &'a [u8; PAGE_BYTES] {
        page_at(self.bytes, ALLOCATOR)
    }
    pub(super) fn queue_page(self, index: usize) -> Result<&'a [u8; PAGE_BYTES]> {
        if index >= 6 {
            return Err(Error::InvalidInput);
        }
        Ok(page_at(self.bytes, QUEUE + index * PAGE_BYTES))
    }
    pub(super) fn status_record(self, ordinal: usize) -> Result<&'a [u8; STATUS_BYTES]> {
        status_record(self.bytes, ordinal)
    }
    pub(super) fn certificate(self, ordinal: usize) -> Result<Option<Certificate>> {
        certificate(self.bytes, self.expected, ordinal)
    }
    /// Reconstructs one exact canonical image into caller scratch. Free and
    /// Reserved pages have no image; this does not authorize a home write.
    pub(super) fn image(self, ordinal: usize, output: &mut [u8; PAGE_BYTES]) -> Result<bool> {
        let Some(cert) = self.certificate(ordinal)? else {
            return Ok(false);
        };
        match ordinal {
            0 => output.copy_from_slice(self.allocator()),
            1..=6 => output.copy_from_slice(self.queue_page(ordinal - 1)?),
            7..=8 => super::wal_io::encode_plan4_catalog_genesis(
                self.expected.profile.spec().namespace,
                ordinal,
                output,
            )
            .map_err(|_| Error::IdentityMismatch)?,
            9..=63 => {
                reconstruct_status(self.status_record(ordinal)?, self.expected, ordinal, output)?;
            }
            _ => return Err(Error::InvalidInput),
        }
        match_certificate(output, cert)?;
        Ok(true)
    }
    /// The original selected-prefix or accepted-base owner must check each
    /// claim. A raw base cannot prove that an INIT ever occurred. This callback
    /// result is deliberately (), not a VerifiedBase or a restored owner.
    pub(super) fn check_init_provenance(
        self,
        scratch: &mut [u8; PAGE_BYTES],
        mut check: impl FnMut(InitClaim) -> Result<()>,
    ) -> Result<()> {
        for ordinal in 9..64 {
            let raw = self.status_record(ordinal)?;
            if raw[0] == 0 {
                continue;
            }
            let mut header = status_header(raw, self.expected, ordinal)?;
            header.lsn = PageLsn(get(raw, 56));
            let home = StatusHome::new(TransactionStatusCell::Free(one()), 0)?;
            let mut cell = [0; 224];
            home.encode(&mut cell)?;
            page::encode_page(header, &[&cell], scratch)?;
            check(InitClaim {
                header,
                full_digest: content_digest(scratch),
            })?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InitClaim {
    pub(super) header: PageHeader,
    pub(super) full_digest: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Certificate {
    pub(super) header: PageHeader,
    pub(super) full_digest: [u8; 32],
}

fn get(bytes: &[u8], at: usize) -> u64 {
    retention_page::get(bytes, at)
}
fn get32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("checked field"))
}
fn get16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("checked field"))
}
fn put(bytes: &mut [u8], at: usize, value: u64) {
    retention_page::put(bytes, at, value);
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn zero(bytes: &[u8]) -> bool {
    retention_page::zero(bytes)
}
fn hash(bytes: &[u8], at: usize) -> [u8; 32] {
    bytes[at..at + 32].try_into().expect("checked digest")
}
fn one() -> Generation {
    Generation::new(1).expect("one")
}
fn first_file() -> FileId {
    FileId::new(0x0000_0001_0000_0001).expect("first file")
}
fn page_at(bytes: &[u8; BASE_BYTES], at: usize) -> &[u8; PAGE_BYTES] {
    bytes[at..at + PAGE_BYTES]
        .try_into()
        .expect("fixed page section")
}
fn identity(
    expected: Expected,
    ordinal: usize,
    incarnation: Generation,
    object: u64,
) -> PageIdentity {
    PageIdentity {
        database: expected.profile.spec().namespace.database,
        page: PageRef {
            file: first_file(),
            page_no: ordinal as u64,
            incarnation,
        },
        owner: ObjectId::new(object).expect("fixed object"),
        owner_generation: one(),
    }
}
fn fixed_kind(ordinal: usize) -> Result<(PageKind, u64)> {
    Ok(match ordinal {
        0 => (PageKind::Allocation, 1),
        1 => (PageKind::RetentionRoot, 4),
        2..=6 => (PageKind::RetentionJobs, 4),
        7 => (PageKind::TreeAnchor, 5),
        8 => (PageKind::CatalogLeaf, 5),
        9..=63 => (PageKind::TransactionStatus, 2),
        _ => return Err(Error::InvalidInput),
    })
}

fn validate_header(h: Header, e: Expected) -> Result<()> {
    if e.profile.spec().namespace.timeline != 1
        || e.filemap.reference.kind != ControlKind::FileMapRoot
        || e.filemap.generation == 0
        || h.generation == 0
        || h.capture_lsn == 0
        || h.source_epoch == 0
        || h.source_epoch > h.issued_epoch
        || !(4096..=256 << 20).contains(&h.source_offset)
        || h.active_transactions > 32
        || h.active_systems > 2
        || h.status_mask & 0x1ff != 0
        || h.leased & h.ready != 0
        || h.leased.count_ones() != h.active_transactions
        || (h.leased | h.ready).count_ones() > 55
        || h.root_revision == 0
        || h.doublewrite_generation == 0
        || h.physical_bytes > e.profile.owned_bytes()
        || h.system_high_water[1] != 0
        || h.system_high_water[3] != 0
        || h.system_high_water[2] > 1
    {
        return Err(Error::InvalidInput);
    }
    if (h.previous_generation == 0) != (h.previous_digest == [0; 32])
        || h.previous_generation >= h.generation
    {
        return Err(Error::OrderMismatch);
    }
    let mut reference = [0; 64];
    e.filemap.reference.encode(&mut reference)?;
    Ok(())
}

fn budget(bytes: &[u8]) -> [RemainingBudget; 5] {
    std::array::from_fn(|index| {
        let row = &bytes[index * 128..(index + 1) * 128];
        RemainingBudget {
            steps: get(row, 8),
            wal: get(row, 16),
            control: get(row, 24),
            growth: get(row, 32),
            pages: get(row, 40),
            undo_slots: get(row, 48),
            status_slots: get(row, 56),
            peak: ResourcePeak {
                frames: get(row, 64),
                memory: get(row, 72),
                locks: get(row, 80),
                registry: get(row, 88),
                files: get(row, 96),
                doublewrite: get(row, 104),
            },
            protected: get(row, 112),
        }
    })
}

fn write_header(h: Header, e: Expected, output: &mut [u8]) -> Result<()> {
    validate_header(h, e)?;
    output.fill(0);
    output[..4].copy_from_slice(b"XRB1");
    put16(output, 4, 1);
    put16(output, 6, 1);
    put32(output, 8, BASE_BYTES as u32);
    put32(output, 12, 4096);
    output[16..32].copy_from_slice(&e.profile.spec().namespace.database.bytes());
    put(output, 32, e.profile.spec().namespace.timeline);
    put(output, 40, e.profile.spec().generation);
    output[48..80].copy_from_slice(&e.profile.digest());
    for (at, value) in [
        (80, 4),
        (88, h.generation),
        (96, h.capture_lsn),
        (104, h.previous_generation),
        (152, h.source_offset),
        (192, h.source_local),
        (240, 5),
        (248, h.capture_lsn),
        (264, h.status_mask),
        (272, h.leased),
        (280, h.ready),
        (288, h.root_revision),
        (296, h.aging_groups),
        (448, e.filemap.generation),
        (496, h.retired_batch),
        (504, h.doublewrite_generation),
        (1152, h.physical_bytes),
    ] {
        put(output, at, value);
    }
    output[112..144].copy_from_slice(&h.previous_digest);
    put32(output, 144, h.source_epoch);
    put32(output, 148, 1);
    output[160..192].copy_from_slice(&h.source_prefix_digest);
    for (index, high) in h.system_high_water.iter().enumerate() {
        put(output, 208 + index * 8, *high);
    }
    put32(output, 256, h.active_transactions);
    put32(output, 260, h.active_systems);
    output[304] = h.allocation_terminal.byte();
    retention_obligation::profile_reference(e.profile).encode(&mut output[320..384])?;
    e.filemap.reference.encode(&mut output[384..448])?;
    output[464..496].copy_from_slice(&e.bootstrap_digest);
    obligation::encode_budget_rows(
        &h.remaining,
        (&mut output[512..1152]).try_into().expect("budget span"),
    );
    put32(output, 1184, h.issued_epoch);
    output[1192..1224].copy_from_slice(&h.source_header_digest);
    Ok(())
}

fn read_header(bytes: &[u8; BASE_BYTES], e: Expected) -> Result<Header> {
    if &bytes[..4] != b"XRB1" {
        return Err(Error::Corrupt);
    }
    if get16(bytes, 4) != 1 || get16(bytes, 6) != 1 {
        return Err(Error::Unsupported);
    }
    if get32(bytes, 8) != BASE_BYTES as u32
        || get32(bytes, 12) != 4096
        || bytes[16..32] != e.profile.spec().namespace.database.bytes()
        || get(bytes, 32) != e.profile.spec().namespace.timeline
        || get(bytes, 40) != e.profile.spec().generation
        || bytes[48..80] != e.profile.digest()
        || get(bytes, 80) != 4
        || get32(bytes, 148) != 1
        || get(bytes, 200) != 0
        || get(bytes, 240) != 5
        || get(bytes, 248) != get(bytes, 96)
        || !zero(&bytes[305..320])
        || !zero(&bytes[456..464])
        || ControlRef::decode(&bytes[320..384])?
            != retention_obligation::profile_reference(e.profile)
        || ControlRef::decode(&bytes[384..448])? != e.filemap.reference
        || get(bytes, 448) != e.filemap.generation
        || bytes[464..496] != e.bootstrap_digest
        || !zero(&bytes[1160..1184])
        || !zero(&bytes[1188..1192])
        || !zero(&bytes[1224..1280])
        || !zero(&bytes[1664..4096])
    {
        return Err(Error::IdentityMismatch);
    }
    let h = Header {
        generation: get(bytes, 88),
        capture_lsn: get(bytes, 96),
        previous_generation: get(bytes, 104),
        previous_digest: hash(bytes, 112),
        source_epoch: get32(bytes, 144),
        source_offset: get(bytes, 152),
        source_prefix_digest: hash(bytes, 160),
        source_local: get(bytes, 192),
        system_high_water: std::array::from_fn(|index| get(bytes, 208 + index * 8)),
        active_transactions: get32(bytes, 256),
        active_systems: get32(bytes, 260),
        status_mask: get(bytes, 264),
        leased: get(bytes, 272),
        ready: get(bytes, 280),
        root_revision: get(bytes, 288),
        aging_groups: get(bytes, 296),
        allocation_terminal: match bytes[304] {
            0 => AllocationTerminal::None,
            1 => AllocationTerminal::Cancelled,
            2 => AllocationTerminal::InitializedStatus,
            _ => return Err(Error::Unsupported),
        },
        retired_batch: get(bytes, 496),
        doublewrite_generation: get(bytes, 504),
        remaining: budget(&bytes[512..1152]),
        physical_bytes: get(bytes, 1152),
        issued_epoch: get32(bytes, 1184),
        source_header_digest: hash(bytes, 1192),
    };
    validate_header(h, e)?;
    let mut canonical = [0; 640];
    obligation::encode_budget_rows(&h.remaining, &mut canonical);
    if bytes[512..1152] != canonical {
        return Err(Error::Corrupt);
    }
    Ok(h)
}

fn status_record(bytes: &[u8; BASE_BYTES], ordinal: usize) -> Result<&[u8; STATUS_BYTES]> {
    if ordinal >= 64 {
        return Err(Error::InvalidInput);
    }
    let at = STATUS + ordinal * STATUS_BYTES;
    Ok(bytes[at..at + STATUS_BYTES]
        .try_into()
        .expect("fixed record"))
}
fn status_header(raw: &[u8; STATUS_BYTES], e: Expected, ordinal: usize) -> Result<PageHeader> {
    if ordinal < 9
        || raw[..4] != [1, PageKind::TransactionStatus as u8, 0, 1]
        || !zero(&raw[4..8])
        || get(raw, 32) != 2
        || get(raw, 40) != 1
        || !zero(&raw[96..128])
        || get(raw, 56) == 0
        || get(raw, 56) > get(raw, 48)
    {
        return Err(Error::Corrupt);
    }
    let reference = PageRef::decode(&raw[8..32])?;
    if reference.file != first_file()
        || reference.page_no != ordinal as u64
        || reference.incarnation.get() < 2
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(PageHeader {
        identity: identity(e, ordinal, reference.incarnation, 2),
        kind: PageKind::TransactionStatus,
        level: 0,
        lsn: PageLsn(get(raw, 48)),
    })
}
fn reconstruct_status(
    raw: &[u8; STATUS_BYTES],
    e: Expected,
    ordinal: usize,
    scratch: &mut [u8; PAGE_BYTES],
) -> Result<StatusHome> {
    let header = status_header(raw, e, ordinal)?;
    let home = StatusHome::decode(&raw[128..])?;
    validate_home(home, e, header.lsn.0, get(raw, 56))?;
    page::encode_page(header, &[&raw[128..]], scratch)?;
    if content_digest(scratch) != raw[64..96] {
        return Err(Error::Corrupt);
    }
    Ok(home)
}
fn validate_home(home: StatusHome, e: Expected, lsn: u64, init: u64) -> Result<()> {
    if home.incoming() != 0 {
        return Err(Error::Unsupported);
    }
    if let TransactionStatusCell::Occupied(status) = home.core() {
        if status.reference().id().database != e.profile.spec().namespace.database
            || status.reference().id().timeline != e.profile.spec().namespace.timeline
            || status.undo_head().is_some()
            || !matches!(
                status.state(),
                TransactionState::Active
                    | TransactionState::Aborting(RollbackState::Transaction { next: None })
                    | TransactionState::Aborted
            )
        {
            return Err(Error::Unsupported);
        }
        if status.reference().generation().get() < 2 || lsn <= init {
            return Err(Error::OrderMismatch);
        }
    } else if let TransactionStatusCell::Free(generation) = home.core()
        && ((generation == one() && lsn != init) || (generation != one() && lsn <= init))
    {
        return Err(Error::OrderMismatch);
    }
    Ok(())
}
fn certificate(
    bytes: &[u8; BASE_BYTES],
    e: Expected,
    ordinal: usize,
) -> Result<Option<Certificate>> {
    if ordinal >= 64 {
        return Err(Error::InvalidInput);
    }
    let at = CERTIFICATES + ordinal * CERTIFICATE_BYTES;
    let raw = &bytes[at..at + CERTIFICATE_BYTES];
    if raw[0] == 0 {
        if !zero(raw) {
            return Err(Error::Corrupt);
        }
        return Ok(None);
    }
    let (kind, object) = fixed_kind(ordinal)?;
    let reference = PageRef::decode(&raw[8..32])?;
    if raw[0] != 1
        || raw[1] != kind as u8
        || raw[2] != 0
        || !zero(&raw[3..8])
        || !zero(&raw[88..96])
        || reference.file != first_file()
        || reference.page_no != ordinal as u64
        || get(raw, 32) != object
        || get(raw, 40) != 1
        || (ordinal < 9 && reference.incarnation != one())
        || (ordinal >= 9 && reference.incarnation.get() < 2)
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(Some(Certificate {
        header: PageHeader {
            identity: identity(e, ordinal, reference.incarnation, object),
            kind,
            level: 0,
            lsn: PageLsn(get(raw, 48)),
        },
        full_digest: hash(raw, 56),
    }))
}
fn match_certificate(bytes: &[u8; PAGE_BYTES], cert: Certificate) -> Result<()> {
    let page = page::decode_page(bytes, cert.header.identity, cert.header.kind)?;
    if page.header() != cert.header || content_digest(bytes) != cert.full_digest {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}

/// Compresses a selected one-cell Status image. The INIT LSN remains a claim
/// until check_init_provenance binds it to an original source or accepted base.
/// Complete output is preserved on error; this does not capture a live page.
pub(super) fn encode_status_record(
    expected: Expected,
    header: PageHeader,
    init_lsn: u64,
    image: &[u8; PAGE_BYTES],
    output: &mut [u8; STATUS_BYTES],
) -> Result<()> {
    let ordinal = usize::try_from(header.identity.page.page_no).map_err(|_| Error::InvalidInput)?;
    if !(9..64).contains(&ordinal)
        || init_lsn == 0
        || init_lsn > header.lsn.0
        || header.identity != identity(expected, ordinal, header.identity.page.incarnation, 2)
        || header.identity.page.incarnation.get() < 2
        || header.kind != PageKind::TransactionStatus
        || header.level != 0
    {
        return Err(Error::InvalidInput);
    }
    let page = page::decode_page(image, header.identity, header.kind)?;
    if page.header() != header || page.len() != 1 {
        return Err(Error::IdentityMismatch);
    }
    let cell = page.cell(0).ok_or(Error::Corrupt)?;
    let home = StatusHome::decode(cell)?;
    validate_home(home, expected, header.lsn.0, init_lsn)?;
    let mut record = [0; STATUS_BYTES];
    record[..4].copy_from_slice(&[1, PageKind::TransactionStatus as u8, 0, 1]);
    header.identity.page.encode(&mut record[8..32])?;
    put(&mut record, 32, 2);
    put(&mut record, 40, 1);
    put(&mut record, 48, header.lsn.0);
    put(&mut record, 56, init_lsn);
    record[64..96].copy_from_slice(&content_digest(image));
    record[128..].copy_from_slice(cell);
    output.copy_from_slice(&record);
    Ok(())
}

/// Observes a supported allocated image. Free/Reserved entries must instead
/// have an all-zero certificate; decode checks that allocator bijection.
pub(super) fn encode_certificate(
    expected: Expected,
    header: PageHeader,
    image: &[u8; PAGE_BYTES],
    output: &mut [u8; CERTIFICATE_BYTES],
) -> Result<()> {
    let ordinal = usize::try_from(header.identity.page.page_no).map_err(|_| Error::InvalidInput)?;
    let (kind, object) = fixed_kind(ordinal)?;
    if header.identity != identity(expected, ordinal, header.identity.page.incarnation, object)
        || header.kind != kind
        || header.level != 0
        || (ordinal < 9 && header.identity.page.incarnation != one())
        || (ordinal >= 9 && header.identity.page.incarnation.get() < 2)
    {
        return Err(Error::InvalidInput);
    }
    let page = page::decode_page(image, header.identity, kind)?;
    if page.header() != header {
        return Err(Error::IdentityMismatch);
    }
    let mut record = [0; CERTIFICATE_BYTES];
    record[0] = 1;
    record[1] = kind as u8;
    header.identity.page.encode(&mut record[8..32])?;
    put(&mut record, 32, object);
    put(&mut record, 40, 1);
    put(&mut record, 48, header.lsn.0);
    record[56..88].copy_from_slice(&content_digest(image));
    output.copy_from_slice(&record);
    Ok(())
}

fn root(bytes: &[u8; BASE_BYTES], e: Expected) -> Result<RootView<'_>> {
    Ok(retention_page::decode_root_page(page_at(bytes, QUEUE), e.profile.spec().namespace)?.1)
}
// Five small borrowed views, not five owned page arrays. Decode envelopes once
// before the bounded pairwise census instead of rehashing a full page per edge.
struct JobPages<'a> {
    pages: [page::PageView<'a>; 5],
    expected: Expected,
}
impl<'a> JobPages<'a> {
    fn new(bytes: &'a [u8; BASE_BYTES], expected: Expected, root_lsn: u64) -> Result<Self> {
        let decode = |index: usize| {
            let view = retention_page::decode_jobs_page(
                page_at(bytes, QUEUE + (index + 1) * PAGE_BYTES),
                expected.profile.spec().namespace,
                index as u8,
            )?;
            if view.header().lsn.0 > root_lsn {
                return Err(Error::OrderMismatch);
            }
            Ok(view)
        };
        Ok(Self {
            pages: [decode(0)?, decode(1)?, decode(2)?, decode(3)?, decode(4)?],
            expected,
        })
    }
    fn job(&self, ordinal: usize) -> Result<JobView<'a>> {
        if ordinal >= 64 {
            return Err(Error::InvalidInput);
        }
        Ok(retention_page::decode_job(
            self.pages[ordinal / 15]
                .cell(ordinal % 15)
                .ok_or(Error::Corrupt)?,
            self.expected.profile.spec().namespace,
        )?)
    }
}
fn raw_descriptor(bytes: &[u8; BASE_BYTES], slot: usize) -> Result<&[u8; 1536]> {
    if slot >= 36 {
        return Err(Error::InvalidInput);
    }
    let at = BANK + slot * 1536;
    Ok(bytes[at..at + 1536].try_into().expect("fixed descriptor"))
}
fn decode_descriptor(
    bytes: &[u8; BASE_BYTES],
    e: Expected,
    slot: usize,
) -> Result<Option<DescriptorSpec>> {
    let raw = raw_descriptor(bytes, slot)?;
    if zero(raw) {
        return Ok(None);
    }
    if slot == 34 {
        return Ok(Some(
            retention_obligation::decode(raw, e.profile, page_at(bytes, QUEUE))?.descriptor(),
        ));
    }
    let owner = if slot < 32 {
        ReservationOwner::Transaction(TxnRef::decode(&raw[24..72])?)
    } else {
        ReservationOwner::System(SystemWorkRef::decode(&raw[24..72])?)
    };
    Ok(Some(obligation::decode_descriptor(
        raw,
        owner,
        retention_obligation::profile_reference(e.profile),
    )?))
}
fn active_descriptor(
    bytes: &[u8; BASE_BYTES],
    e: Expected,
    ordinal: usize,
) -> Result<DescriptorSpec> {
    let mut active = 0;
    for slot in 0..36 {
        if let Some(spec) = decode_descriptor(bytes, e, slot)? {
            if matches!(spec.owner, OwnerState::System(progress) if progress.phase == WorkPhase::Finished)
            {
                continue;
            }
            if active == ordinal {
                return Ok(spec);
            }
            active += 1;
        }
    }
    Err(Error::InvalidInput)
}
fn txn_key(reference: TxnRef) -> [u64; 3] {
    [
        u64::from(reference.id().writer_epoch),
        reference.id().local,
        reference.generation().get(),
    ]
}
fn add_budget(total: &mut [RemainingBudget; 5], next: &[RemainingBudget; 5]) -> Result<()> {
    for (total, next) in total.iter_mut().zip(next) {
        macro_rules! add {
            ($field:ident) => {
                total.$field = total
                    .$field
                    .checked_add(next.$field)
                    .ok_or(Error::BudgetMismatch)?;
            };
        }
        add!(steps);
        add!(wal);
        add!(control);
        add!(growth);
        add!(pages);
        add!(undo_slots);
        add!(status_slots);
        add!(protected);
        macro_rules! peak {
            ($field:ident) => {
                total.peak.$field = total.peak.$field.max(next.peak.$field);
            };
        }
        peak!(frames);
        peak!(memory);
        peak!(locks);
        peak!(registry);
        peak!(files);
        peak!(doublewrite);
    }
    Ok(())
}

fn verify_transaction(spec: &DescriptorSpec, h: Header) -> Result<TxnRef> {
    let OwnerState::Transaction(status) = spec.owner else {
        return Err(Error::IdentityMismatch);
    };
    let reference = status.reference();
    if reference.id().writer_epoch > h.source_epoch
        || (reference.id().writer_epoch == h.source_epoch && reference.id().local > h.source_local)
        || status.undo_head().is_some()
        || spec.inventory.items != 0
        || spec.inventory.growth_extents != 0
        || spec.inventory.change_lsn != spec.chain.birth
        || spec.inventory.digest != content_digest(&[])
        || spec.minimum_wal_lsn != spec.chain.birth
        || spec.last_mutation_sequence != 0
        || spec.consumed_control != 0
    {
        return Err(Error::IdentityMismatch);
    }
    let mut remaining = semantic_retention::begin_remaining();
    let (groups, consumed) = match status.state() {
        TransactionState::Active => {
            if spec.chain.previous != 0 || spec.chain.apply != spec.chain.birth {
                return Err(Error::OrderMismatch);
            }
            (1, HANDOFF_BYTES)
        }
        TransactionState::Aborting(RollbackState::Transaction { next: None }) => {
            if spec.chain.previous != spec.chain.birth {
                return Err(Error::OrderMismatch);
            }
            remaining[3] = RemainingBudget::default();
            (2, HANDOFF_BYTES + ROLLBACK_BYTES)
        }
        _ => return Err(Error::Unsupported),
    };
    if spec.chain.groups != groups
        || spec.consumed_wal != consumed as u64
        || spec.remaining != remaining
    {
        return Err(Error::BudgetMismatch);
    }
    Ok(reference)
}

/// Each root/job/status/ATT relation is checked in both directions. At most one
/// wide descriptor is decoded at a time; repeated reads borrow immutable bytes.
fn verify_graph(bytes: &[u8; BASE_BYTES], e: Expected, h: Header) -> Result<(usize, usize)> {
    let r = root(bytes, e)?.spec();
    if r.apply > h.capture_lsn
        || r.revision != h.root_revision
        || r.leased != h.leased
        || r.ready != h.ready
        || r.aging.is_some() != (h.system_high_water[2] == 1)
        || h.aging_groups != if r.aging.is_some() { r.revision - 1 } else { 0 }
    {
        return Err(Error::IdentityMismatch);
    }
    let jobs = JobPages::new(bytes, e, r.apply)?;
    let mut digest = ContentHasher::new();
    let mut masks = [0u64; 3];
    for ordinal in 0..64 {
        let current = jobs.job(ordinal)?;
        digest.update(current.bytes());
        if current.last_step() > r.apply {
            return Err(Error::OrderMismatch);
        }
        if current.spec().generation.get() - 1 > r.issued {
            return Err(Error::OrderMismatch);
        }
        let (source, status, leased, lease) = match current.spec().state {
            JobState::Free => {
                if r.aging.is_none() && current.spec().generation != one() {
                    return Err(Error::OrderMismatch);
                }
                masks[0] |= 1 << ordinal;
                continue;
            }
            JobState::Leased {
                source,
                status,
                lease_lsn,
            } => (source, status, true, lease_lsn),
            JobState::Ready {
                source,
                status,
                lease_lsn,
                handoff_lsn,
                ..
            } => {
                if handoff_lsn > h.capture_lsn {
                    return Err(Error::OrderMismatch);
                }
                (source, status, false, lease_lsn)
            }
        };
        masks[if leased { 1 } else { 2 }] |= 1 << ordinal;
        if source.id().writer_epoch > h.source_epoch
            || (source.id().writer_epoch == h.source_epoch && source.id().local > h.source_local)
        {
            return Err(Error::IdentityMismatch);
        }
        for previous in 0..ordinal {
            let other = jobs.job(previous)?.spec().state;
            if let JobState::Leased {
                source: os,
                status: ot,
                ..
            }
            | JobState::Ready {
                source: os,
                status: ot,
                ..
            } = other
                && (source.id() == os.id() || status.page().page_no == ot.page().page_no)
            {
                return Err(Error::IdentityMismatch);
            }
        }
        let raw = status_record(bytes, status.page().page_no as usize)?;
        if raw[0] == 0 || PageRef::decode(&raw[8..32])? != status.page() || lease <= get(raw, 56) {
            return Err(Error::IdentityMismatch);
        }
        let home = StatusHome::decode(&raw[128..])?;
        let TransactionStatusCell::Occupied(actual) = home.core() else {
            return Err(Error::IdentityMismatch);
        };
        if actual.reference() != source || status.generation() != source.generation() {
            return Err(Error::IdentityMismatch);
        }
        let mut matches = 0;
        for slot in 0..h.active_transactions as usize {
            let spec = decode_descriptor(bytes, e, slot)?.ok_or(Error::Corrupt)?;
            if let OwnerState::Transaction(tx) = spec.owner
                && tx.reference() == source
            {
                if !leased
                    || tx != actual
                    || spec.chain.birth != lease
                    || get(raw, 48) != spec.chain.apply
                {
                    return Err(Error::IdentityMismatch);
                }
                matches += 1;
            }
        }
        if (leased && matches != 1)
            || (!leased && (matches != 0 || actual.state() != TransactionState::Aborted))
        {
            return Err(Error::IdentityMismatch);
        }
        if !leased && get(raw, 48) != current.last_step() {
            return Err(Error::OrderMismatch);
        }
    }
    if masks != [r.free, r.leased, r.ready] || digest.digest() != r.job_digest {
        return Err(Error::Corrupt);
    }
    let mut occupied = 0;
    for ordinal in 9..64 {
        let raw = status_record(bytes, ordinal)?;
        if raw[0] == 0 {
            continue;
        }
        let core = StatusHome::decode(&raw[128..])?.core();
        let mut matches = 0;
        for index in 0..64 {
            if let JobState::Leased { status, .. } | JobState::Ready { status, .. } =
                jobs.job(index)?.spec().state
                && status.page().page_no == ordinal as u64
            {
                matches += 1;
            }
        }
        if matches != usize::from(matches!(core, TransactionStatusCell::Occupied(_))) {
            return Err(Error::IdentityMismatch);
        }
        if matches!(core, TransactionStatusCell::Occupied(_)) {
            occupied += 1;
        }
    }
    for slot in 0..h.active_transactions as usize {
        let spec = decode_descriptor(bytes, e, slot)?.ok_or(Error::Corrupt)?;
        let reference = verify_transaction(&spec, h)?;
        let mut matches = 0;
        for ordinal in 0..64 {
            if let JobState::Leased { source, .. } = jobs.job(ordinal)?.spec().state
                && source == reference
            {
                matches += 1;
            }
        }
        if matches != 1 {
            return Err(Error::IdentityMismatch);
        }
    }
    Ok((occupied, (r.leased | r.ready).count_ones() as usize))
}

struct EmbeddedInventory<'a> {
    bytes: &'a [u8; PAGE_BYTES],
    count: usize,
}
impl InventorySource for EmbeddedInventory<'_> {
    fn read_chunk(
        &mut self,
        index: u64,
        output: &mut [u8; PAGE_BYTES],
    ) -> std::result::Result<usize, InventoryError> {
        if index != 0 || self.count != 1 {
            return Err(InventoryError::Source);
        }
        let length = 256;
        output[..length]
            .copy_from_slice(&self.bytes[INVENTORY_ROOT_CAP..INVENTORY_ROOT_CAP + length]);
        Ok(length)
    }
}

fn verify_inventory(
    bytes: &[u8; BASE_BYTES],
    e: Expected,
    h: Header,
    reserved: Option<PageRef>,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<InventoryObservation> {
    let bank = page_at(bytes, INVENTORY);
    let owners = h.active_transactions as usize + h.active_systems as usize;
    let root_length = 160 + owners * 56;
    let items = usize::from(reserved.is_some());
    let chunk_length = if items == 0 { 0 } else { 256 };
    if !zero(&bank[root_length..INVENTORY_ROOT_CAP])
        || !zero(&bank[INVENTORY_ROOT_CAP + chunk_length..INVENTORY_ROOT_CAP + INVENTORY_CHUNK_CAP])
        || !zero(&bank[INVENTORY_ROOT_CAP + INVENTORY_CHUNK_CAP..])
        || bank[7] != 2
        || get(bank, 40) != h.generation
        || get(bank, 72) != items as u64
    {
        return Err(Error::Corrupt);
    }
    let reference = ControlRef {
        kind: ControlKind::OwnershipRoot,
        slot: 2,
        index: 0,
        generation: h.generation,
        encoded_len: root_length as u32,
        digest: content_digest(&bank[..root_length]),
    };
    let expected = InventoryExpected {
        namespace: e.profile.spec().namespace,
        reference,
        capture_lsn: h.capture_lsn,
        profile: retention_obligation::profile_reference(e.profile),
        item_limit: 1,
    };
    let mut source = EmbeddedInventory {
        bytes: bank,
        count: items,
    };
    // Parent-owned extraction of the original root/item/budget/order/physical
    // verifier. No 36-wide-descriptor array or alternate XOI1 parser is used.
    let observation = ownership::verify_borrowed_inventory(
        &bank[..root_length],
        expected,
        owners,
        |ordinal| active_descriptor(bytes, e, ordinal).map_err(|_| InventoryError::Corrupt),
        &mut source,
        scratch,
    )?;
    if observation.items != items as u64 || observation.owners != owners {
        return Err(Error::IdentityMismatch);
    }
    if let Some(reference) = reserved {
        let item = &bank[INVENTORY_ROOT_CAP + 128..INVENTORY_ROOT_CAP + 224];
        let owner = active_descriptor(bytes, e, get16(item, 2) as usize)?;
        let allocation = decode_descriptor(bytes, e, 32)?.ok_or(Error::IdentityMismatch)?;
        if item[0] != 1
            || item[1] != 3
            || PageRef::decode(&item[8..32])? != reference
            || owner.owner.reference() != allocation.owner.reference()
            || get(item, 72) != allocation.inventory.change_lsn
        {
            return Err(Error::IdentityMismatch);
        }
    }
    Ok(observation)
}

/// Decode and check all sections using caller scratch. Success is a raw byte
/// observation only. In particular check_init_provenance and selected closure
/// I/O are separate mandatory owner duties, not implied by these hashes.
pub(super) fn decode<'a>(
    input: &'a [u8],
    expected: Expected,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<BaseView<'a>> {
    let bytes: &[u8; BASE_BYTES] = input.try_into().map_err(|_| Error::Corrupt)?;
    if content_digest(&bytes[..TRAILER]) != bytes[TRAILER..] {
        return Err(Error::Corrupt);
    }
    let h = read_header(bytes, expected)?;
    if !zero(&bytes[BANK + DESCRIPTOR_BYTES..ALLOCATOR])
        || !zero(&bytes[INVENTORY + PAGE_BYTES..TRAILER])
    {
        return Err(Error::Corrupt);
    }
    for (index, (offset, length, count, entry)) in SECTIONS.iter().copied().enumerate() {
        let at = 1280 + index * 64;
        if get16(bytes, at) != index as u16 + 1
            || get16(bytes, at + 2) != 0
            || get32(bytes, at + 4) != offset as u32
            || get32(bytes, at + 8) != length as u32
            || get32(bytes, at + 12) != count as u32
            || get32(bytes, at + 16) != entry as u32
            || !zero(&bytes[at + 20..at + 32])
            || bytes[at + 32..at + 64] != content_digest(&bytes[offset..offset + length])
        {
            return Err(Error::Corrupt);
        }
    }
    let allocation = allocation_page::decode_allocation_page(
        page_at(bytes, ALLOCATOR),
        identity(expected, 0, one(), 1),
        expected.profile.spec().namespace,
    )?;
    if allocation.header().level != 0 || allocation.header().lsn.0 > h.capture_lsn {
        return Err(Error::OrderMismatch);
    }
    let mut statuses = 0u64;
    let mut reserved = None;
    for ordinal in 0..64 {
        let entry = allocation.entry(ordinal)?;
        let cert = certificate(bytes, expected, ordinal)?;
        let status = status_record(bytes, ordinal)?;
        match entry {
            AllocationEntry::Free(_) => {
                if ordinal < 9 || cert.is_some() || !zero(status) {
                    return Err(Error::IdentityMismatch);
                }
            }
            AllocationEntry::Reserved { claimant, .. } => {
                if ordinal < 9
                    || entry.incarnation().get() < 2
                    || cert.is_some()
                    || !zero(status)
                    || reserved.is_some()
                {
                    return Err(Error::IdentityMismatch);
                }
                let spec =
                    decode_descriptor(bytes, expected, 32)?.ok_or(Error::IdentityMismatch)?;
                if spec.owner.reference() != claimant {
                    return Err(Error::IdentityMismatch);
                }
                reserved = Some(PageRef {
                    file: first_file(),
                    page_no: ordinal as u64,
                    incarnation: entry.incarnation(),
                });
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
                match ordinal {
                    0 => match_certificate(page_at(bytes, ALLOCATOR), cert)?,
                    1..=6 => {
                        match_certificate(page_at(bytes, QUEUE + (ordinal - 1) * PAGE_BYTES), cert)?
                    }
                    7..=8 => {
                        super::wal_io::encode_plan4_catalog_genesis(
                            expected.profile.spec().namespace,
                            ordinal,
                            &mut scratch[0],
                        )
                        .map_err(|_| Error::IdentityMismatch)?;
                        match_certificate(&scratch[0], cert)?;
                    }
                    9..=63 => {
                        reconstruct_status(status, expected, ordinal, &mut scratch[0])?;
                        match_certificate(&scratch[0], cert)?;
                        statuses |= 1 << ordinal;
                    }
                    _ => unreachable!("bounded extent"),
                }
                if ordinal < 9 && !zero(status) {
                    return Err(Error::IdentityMismatch);
                }
            }
        }
    }
    // A linear reserve blocks every other append until INIT/cancel. Recovery
    // cancels it first, so its terminal remains exactly birth + 1 even across
    // a new epoch. A later capture cannot represent this closed owner scope.
    if reserved.is_some() && h.capture_lsn != allocation.header().lsn.0 {
        return Err(Error::OrderMismatch);
    }
    if statuses != h.status_mask {
        return Err(Error::IdentityMismatch);
    }
    let mut transactions = 0;
    let mut systems = 0;
    let mut previous = None;
    let mut aggregate = [RemainingBudget::default(); 5];
    for slot in 0..36 {
        let Some(spec) = decode_descriptor(bytes, expected, slot)? else {
            if slot < h.active_transactions as usize
                || (slot == 32 && h.system_high_water[0] != 0)
                || (slot == 34 && h.system_high_water[2] != 0)
            {
                return Err(Error::IdentityMismatch);
            }
            continue;
        };
        if spec.chain.apply > h.capture_lsn {
            return Err(Error::OrderMismatch);
        }
        if slot < 32 {
            if slot != transactions || slot >= h.active_transactions as usize {
                return Err(Error::IdentityMismatch);
            }
            let reference = verify_transaction(&spec, h)?;
            if reference.id().database != expected.profile.spec().namespace.database
                || reference.id().timeline != expected.profile.spec().namespace.timeline
                || previous.is_some_and(|old: TxnRef| {
                    txn_key(old) >= txn_key(reference) || old.id() == reference.id()
                })
            {
                return Err(Error::IdentityMismatch);
            }
            previous = Some(reference);
            transactions += 1;
        } else {
            let OwnerState::System(progress) = spec.owner else {
                return Err(Error::IdentityMismatch);
            };
            if !matches!(slot, 32 | 34)
                || progress.reference.kind() as usize != slot - 31
                || progress.reference.namespace() != expected.profile.spec().namespace
                || progress.reference.generation() != one()
                || progress.reference.id() != h.system_high_water[slot - 32]
            {
                return Err(Error::Unsupported);
            }
            if progress.phase == WorkPhase::Finished {
                continue;
            }
            systems += 1;
        }
        add_budget(&mut aggregate, &spec.remaining)?;
    }
    if transactions != h.active_transactions as usize
        || systems != h.active_systems as usize
        || aggregate != h.remaining
    {
        return Err(Error::BudgetMismatch);
    }
    // Parent-owned exact count1 Prepared/Finished validator; the existing
    // original allocation descriptor algebra remains the single source.
    let allocation_raw = if zero(raw_descriptor(bytes, 32)?) {
        None
    } else {
        Some(raw_descriptor(bytes, 32)?)
    };
    super::wal_io::validate_plan4_base_allocation(
        page_at(bytes, ALLOCATOR),
        expected.profile,
        allocation_raw,
        h.system_high_water[0],
        h.allocation_terminal.byte(),
    )
    .map_err(|_| Error::IdentityMismatch)?;
    let (occupied, non_free) = verify_graph(bytes, expected, h)?;
    if occupied != non_free {
        return Err(Error::IdentityMismatch);
    }
    let inventory = verify_inventory(bytes, expected, h, reserved, scratch)?;
    Ok(BaseView {
        bytes,
        expected,
        header: h,
        census: Census {
            transactions,
            systems,
            statuses: statuses.count_ones() as usize,
            leased: h.leased.count_ones() as usize,
            ready: h.ready.count_ones() as usize,
            reserved,
            inventory,
        },
    })
}

#[cfg(test)]
#[path = "plan4_recovery_base_tests.rs"]
mod tests;

/// Encodes into unpublished caller backing and performs the same full census.
/// Output and scratch may change on error; no persistent bytes are written.
pub(super) fn encode<'a>(
    h: Header,
    expected: Expected,
    sections: Sections<'_>,
    output: &'a mut [u8; BASE_BYTES],
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<BaseView<'a>> {
    validate_header(h, expected)?;
    output.fill(0);
    write_header(h, expected, &mut output[..4096])?;
    output[BANK..BANK + DESCRIPTOR_BYTES].copy_from_slice(sections.descriptors);
    output[ALLOCATOR..ALLOCATOR + PAGE_BYTES].copy_from_slice(sections.allocator);
    for (index, page) in sections.queue.iter().enumerate() {
        output[QUEUE + index * PAGE_BYTES..QUEUE + (index + 1) * PAGE_BYTES].copy_from_slice(page);
    }
    for (index, record) in sections.statuses.iter().enumerate() {
        output[STATUS + index * STATUS_BYTES..STATUS + (index + 1) * STATUS_BYTES]
            .copy_from_slice(record);
    }
    for (index, record) in sections.certificates.iter().enumerate() {
        output[CERTIFICATES + index * CERTIFICATE_BYTES
            ..CERTIFICATES + (index + 1) * CERTIFICATE_BYTES]
            .copy_from_slice(record);
    }
    output[INVENTORY..INVENTORY + PAGE_BYTES].copy_from_slice(sections.inventory);
    for (index, (offset, length, count, entry)) in SECTIONS.iter().copied().enumerate() {
        let at = 1280 + index * 64;
        put16(output, at, index as u16 + 1);
        put32(output, at + 4, offset as u32);
        put32(output, at + 8, length as u32);
        put32(output, at + 12, count as u32);
        put32(output, at + 16, entry as u32);
        let digest = content_digest(&output[offset..offset + length]);
        output[at + 32..at + 64].copy_from_slice(&digest);
    }
    let digest = content_digest(&output[..TRAILER]);
    output[TRAILER..].copy_from_slice(&digest);
    decode(output, expected, scratch)
}

/// New-only typed source census, including all three common bridge closures.
/// Add the parent caller and original owners before comparing any F05 cap.
/// Caller 1MiB/four-page loans are separately charged; this is not ABI stack.
pub(super) fn source_bytes() -> usize {
    use std::mem::size_of;
    640 + 224
        + STATUS_BYTES
        + CERTIFICATE_BYTES
        + 3 * size_of::<Header>()
        + 6 * size_of::<Expected>()
        + 2 * size_of::<BaseView<'_>>()
        + 3 * size_of::<Result<BaseView<'_>>>()
        + 3 * size_of::<DescriptorSpec>()
        + 3 * size_of::<Result<Option<DescriptorSpec>>>()
        + 2 * size_of::<[RemainingBudget; 5]>()
        + 3 * size_of::<ContentHasher>()
        + 2 * size_of::<JobPages<'_>>()
        + 2 * size_of::<Result<JobPages<'_>>>()
        + 4 * size_of::<JobView<'_>>()
        + 4 * size_of::<Certificate>()
        + 4 * size_of::<Result<Certificate>>()
        + 2 * size_of::<Sections<'_>>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<InventoryExpected>()
        + 2 * size_of::<InventoryObservation>()
        + 2 * size_of::<InitClaim>()
        + 2048
        + retention_obligation::source_bytes()
        // The inventory callback borrows bytes and Expected, with no I/O or
        // mutable registry. Its decode/return values and nested Aging decoder
        // are included above; the verifier's simultaneous values are separate.
        + 2 * size_of::<&[u8; BASE_BYTES]>()
        + ownership::borrowed_inventory_source_bytes()
            .max(super::wal_io::plan4_base_allocation_source_bytes())
            .max(super::wal_io::plan4_catalog_genesis_source_bytes())
}
