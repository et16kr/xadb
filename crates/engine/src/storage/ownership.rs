//! Bounded inventory byte/accounting/physical-overlap observations.
//! Contract: `devdoc/contracts/dev3-ownership-format.md`.
//! The profile-bound entry also checks current owner-local growth intents.
//! Neither entry proves historical nonreuse or reservation restoration.
#![allow(dead_code)] // Awaiting the distinct selected-source and restore consumers.

use super::{
    allocation_page::{ExtentAddress, ReservationOwner, SystemWorkRef},
    control::{ControlKind, ControlRef, EXTENT_BYTES, PROFILE_BYTES, Profile},
    obligation::{DescriptorSpec, OwnerState, WorkPhase, validate_descriptor},
    page::{FileId, PAGE_BYTES, PageRef},
    status_home::MAX_HOME_SLOTS,
    transaction_record::{StatusSlotRef, TransactionState, TxnRef},
    undo_record::MAX_UNDO_SLOTS,
    undo_ref::UndoRef,
};
use crate::api::{DatabaseId, Namespace};
use xadb_foundation::codec::{ContentHasher, content_digest};

pub(crate) const MAX_OWNERS: usize = 36;
pub(crate) const MAX_GROWTH_ITEMS: usize = 8192;
/// Caller-provided storage; no hidden large constructor or inventory-sized heap.
pub(crate) type IntentWorkspace = [[u64; 2]; MAX_GROWTH_ITEMS];
const ITEM_BYTES: usize = 96;
const CHUNK_ITEMS: u64 = 169;
type Result<T> = std::result::Result<T, InventoryError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InventoryError {
    InvalidInput,
    Corrupt,
    UnsupportedFormat,
    IdentityMismatch,
    LimitExceeded,
    Source,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InventoryExpected {
    pub(crate) namespace: Namespace,
    pub(crate) reference: ControlRef,
    pub(crate) capture_lsn: u64,
    pub(crate) profile: ControlRef,
    /// A caller's validation work limit, not an admitted storage capacity.
    pub(crate) item_limit: u64,
}

/// The caller owns source lifetime, I/O cancellation and any callback resources.
pub(crate) trait InventorySource {
    fn read_chunk(&mut self, index: u64, output: &mut [u8; PAGE_BYTES]) -> Result<usize>;
}

/// Observed bytes, owner totals and physical overlap only. This is not a
/// reservation-restored capability: owner-local intent uniqueness is unchecked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InventoryObservation {
    pub(crate) owners: usize,
    pub(crate) items: u64,
    pub(crate) chunks: u64,
    pub(crate) read_calls: u64,
}

/// Complete observed inventory with a matching fixed profile and unique current
/// owner-local growth intents. No source pin, admission or restoration authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProfileInventoryObservation {
    pub(crate) inventory: InventoryObservation,
    pub(crate) growth_items: u64,
    pub(crate) growth_bytes: u64,
}

struct IntentCollector<'a> {
    keys: &'a mut IntentWorkspace,
    len: usize,
}
impl IntentCollector<'_> {
    fn accept(&mut self, item: Item) -> Result<()> {
        if item.kind() == 4 {
            let key = self.keys.get_mut(self.len).ok_or(InventoryError::Corrupt)?;
            *key = [item.owner() as u64, u64_at(&item.bytes, 32)];
            self.len += 1;
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<u64> {
        let keys = &mut self.keys[..self.len];
        sort_intents(keys);
        if keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(InventoryError::Corrupt);
        }
        Ok(self.len as u64)
    }
}

/// Iterative heapsort keeps sorting scratch constant as well as heap-free.
/// Only the initialized prefix is passed; keys remain bound to this root's
/// validated owner ordinals. No truncation of the full u64 intent is permitted.
fn sort_intents(keys: &mut [[u64; 2]]) {
    fn sift_down(keys: &mut [[u64; 2]], mut root: usize) {
        while root < keys.len() / 2 {
            let mut child = root * 2 + 1;
            if child + 1 < keys.len() && keys[child] < keys[child + 1] {
                child += 1;
            }
            if keys[root] >= keys[child] {
                break;
            }
            keys.swap(root, child);
            root = child;
        }
    }
    for root in (0..keys.len() / 2).rev() {
        sift_down(keys, root);
    }
    for end in (1..keys.len()).rev() {
        keys.swap(0, end);
        sift_down(&mut keys[..end], 0);
    }
}

/// Validate the real fixed profile before any source read, then collect intents
/// during the same first pass used by the complete byte/accounting observation.
/// Workspace may change on failure; a new call always starts with an empty set.
pub(crate) fn verify_profile_inventory(
    root_bytes: &[u8],
    expected: InventoryExpected,
    descriptors: &[DescriptorSpec],
    profile: Profile,
    source: &mut impl InventorySource,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
    intents: &mut IntentWorkspace,
) -> Result<ProfileInventoryObservation> {
    validate_expected(expected, descriptors)?;
    let spec = profile.spec();
    if spec.namespace != expected.namespace
        || spec.generation != expected.profile.generation
        || profile.digest() != expected.profile.digest
        || expected.profile.encoded_len as usize != PROFILE_BYTES
    {
        return Err(InventoryError::InvalidInput);
    }
    // Each descriptor has a checked extent count. The bounded 36-owner sum and
    // byte conversion fit u128 even when the corresponding u64 byte sum does not.
    let growth_bytes = descriptors
        .iter()
        .map(|descriptor| u128::from(descriptor.inventory.growth_extents))
        .sum::<u128>()
        * u128::from(EXTENT_BYTES);
    if growth_bytes > u128::from(profile.owned_bytes())
        || growth_bytes / u128::from(EXTENT_BYTES) > MAX_GROWTH_ITEMS as u128
    {
        return Err(InventoryError::LimitExceeded);
    }
    let mut collector = IntentCollector {
        keys: intents,
        len: 0,
    };
    let inventory = observe_inventory(
        root_bytes,
        expected,
        descriptors,
        source,
        scratch,
        Some(&mut collector),
    )?;
    let growth_items = collector.finish()?;
    Ok(ProfileInventoryObservation {
        inventory,
        growth_items,
        growth_bytes: growth_bytes as u64, // Checked against the real 8 GiB profile.
    })
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked wire length"),
    )
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("checked wire length"),
    )
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}
fn namespace(bytes: &[u8]) -> Result<Namespace> {
    let database = DatabaseId::new(bytes[16..32].try_into().expect("checked wire length"))
        .map_err(|_| InventoryError::Corrupt)?;
    let timeline = u64_at(bytes, 32);
    if timeline == 0 {
        return Err(InventoryError::Corrupt);
    }
    Ok(Namespace { database, timeline })
}
fn owner_namespace(owner: ReservationOwner) -> Namespace {
    match owner {
        ReservationOwner::Transaction(reference) => Namespace {
            database: reference.id().database,
            timeline: reference.id().timeline,
        },
        ReservationOwner::System(reference) => reference.namespace(),
    }
}
fn owner_key(owner: ReservationOwner) -> [u64; 4] {
    match owner {
        ReservationOwner::Transaction(reference) => [
            1,
            u64::from(reference.id().writer_epoch),
            reference.id().local,
            reference.generation().get(),
        ],
        ReservationOwner::System(reference) => [
            2,
            reference.kind() as u64,
            reference.id(),
            reference.generation().get(),
        ],
    }
}
fn decode_owner(bytes: &[u8]) -> Result<ReservationOwner> {
    if !zero(&bytes[1..8]) {
        return Err(InventoryError::Corrupt);
    }
    match bytes[0] {
        1 => TxnRef::decode(&bytes[8..])
            .map(ReservationOwner::Transaction)
            .map_err(|_| InventoryError::Corrupt),
        2 => SystemWorkRef::decode(&bytes[8..])
            .map(ReservationOwner::System)
            .map_err(|_| InventoryError::Corrupt),
        _ => Err(InventoryError::Corrupt),
    }
}

#[derive(Default)]
struct OwnerOrder {
    previous: Option<[u64; 4]>,
    transactions: usize,
    system_kinds: u8,
}
impl OwnerOrder {
    fn accept(&mut self, owner: ReservationOwner) -> Result<()> {
        let key = owner_key(owner);
        if self
            .previous
            .is_some_and(|previous| previous >= key || previous[..3] == key[..3])
        {
            return Err(InventoryError::Corrupt);
        }
        match owner {
            ReservationOwner::Transaction(_) => {
                self.transactions += 1;
                if self.transactions > 32 {
                    return Err(InventoryError::Corrupt);
                }
            }
            ReservationOwner::System(reference) => {
                let bit = 1 << (reference.kind() as u8 - 1);
                if self.system_kinds & bit != 0 {
                    return Err(InventoryError::Corrupt);
                }
                self.system_kinds |= bit;
            }
        }
        self.previous = Some(key);
        Ok(())
    }
}

fn validate_expected(expected: InventoryExpected, descriptors: &[DescriptorSpec]) -> Result<()> {
    if expected.namespace.timeline == 0
        || expected.capture_lsn == 0
        || descriptors.len() > MAX_OWNERS
        || expected.reference.kind != ControlKind::OwnershipRoot
        || expected.reference.encode(&mut [0; 64]).is_err()
        || expected.reference.encoded_len as usize != 160 + 56 * descriptors.len()
        || expected.profile.kind != ControlKind::Profile
        || expected.profile.encode(&mut [0; 64]).is_err()
    {
        return Err(InventoryError::InvalidInput);
    }
    let mut order = OwnerOrder::default();
    for descriptor in descriptors {
        validate_descriptor(descriptor).map_err(|_| InventoryError::InvalidInput)?;
        let active = match descriptor.owner {
            OwnerState::Transaction(status) => matches!(
                status.state(),
                TransactionState::Active | TransactionState::Aborting(_)
            ),
            OwnerState::System(progress) => progress.phase != WorkPhase::Finished,
        };
        let owner = descriptor.owner.reference();
        if !active
            || owner_namespace(owner) != expected.namespace
            || descriptor.profile != expected.profile
            || descriptor.chain.apply > expected.capture_lsn
        {
            return Err(InventoryError::InvalidInput);
        }
        order
            .accept(owner)
            .map_err(|_| InventoryError::InvalidInput)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Root {
    namespace: Namespace,
    bank: u8,
    generation: u64,
    capture: u64,
    owners: usize,
    chunks: u64,
    items: u64,
    aggregate: [u8; 32],
    owner_digest: [u8; 32],
}

fn envelope(bytes: &[u8], kind: u8) -> Result<()> {
    if bytes.len() < 160
        || bytes[..4] != *b"XOI1"
        || bytes[6] != kind
        || bytes[7] > 2
        || u16_at(bytes, 8) != 128
        || u16_at(bytes, 10) != ITEM_BYTES as u16
        || u32_at(bytes, 12) as usize != bytes.len()
        || u64_at(bytes, 40) == 0
        || u64_at(bytes, 48) == 0
    {
        return Err(InventoryError::Corrupt);
    }
    if u16_at(bytes, 4) != 1 {
        return Err(InventoryError::UnsupportedFormat);
    }
    let end = bytes.len() - 32;
    if content_digest(&bytes[..end]) != bytes[end..] {
        return Err(InventoryError::Corrupt);
    }
    Ok(())
}
fn decode_root(
    bytes: &[u8],
    expected: InventoryExpected,
    descriptors: &[DescriptorSpec],
) -> Result<Root> {
    if bytes.len() > 2176 {
        return Err(InventoryError::Corrupt);
    }
    envelope(bytes, 1)?;
    let owners = usize::from(u16_at(bytes, 56));
    let items = u64_at(bytes, 72);
    let chunks = items / CHUNK_ITEMS + u64::from(!items.is_multiple_of(CHUNK_ITEMS));
    let total_bytes = chunks
        .checked_mul(160)
        .and_then(|n| items.checked_mul(96).and_then(|m| n.checked_add(m)))
        .ok_or(InventoryError::Corrupt)?;
    if owners > MAX_OWNERS
        || bytes.len() != 160 + 56 * owners
        || !zero(&bytes[58..64])
        || !zero(&bytes[120..128])
        || chunks != u64_at(bytes, 64)
        || chunks > u64::from(u32::MAX) + 1
        || total_bytes != u64_at(bytes, 80)
        || (owners == 0 && items != 0)
        || (items == 0 && bytes[88..120] != content_digest(&[]))
    {
        return Err(InventoryError::Corrupt);
    }
    let root = Root {
        namespace: namespace(bytes)?,
        bank: bytes[7],
        generation: u64_at(bytes, 40),
        capture: u64_at(bytes, 48),
        owners,
        chunks,
        items,
        aggregate: bytes[88..120].try_into().expect("checked wire length"),
        owner_digest: content_digest(&bytes[128..128 + 56 * owners]),
    };
    let mut order = OwnerOrder::default();
    for encoded in bytes[128..128 + 56 * owners].chunks_exact(56) {
        let owner = decode_owner(encoded)?;
        if owner_namespace(owner) != root.namespace {
            return Err(InventoryError::Corrupt);
        }
        order.accept(owner)?;
    }
    if root.namespace != expected.namespace
        || root.bank != expected.reference.slot
        || root.generation != expected.reference.generation
        || root.capture != expected.capture_lsn
        || bytes.len() != expected.reference.encoded_len as usize
        || content_digest(bytes) != expected.reference.digest
        || owners != descriptors.len()
    {
        return Err(InventoryError::IdentityMismatch);
    }
    for (encoded, descriptor) in bytes[128..128 + 56 * owners]
        .chunks_exact(56)
        .zip(descriptors)
    {
        if decode_owner(encoded)? != descriptor.owner.reference() {
            return Err(InventoryError::IdentityMismatch);
        }
    }
    if items > expected.item_limit {
        return Err(InventoryError::LimitExceeded);
    }
    Ok(root)
}

#[derive(Clone, Copy)]
struct Item {
    bytes: [u8; ITEM_BYTES],
}
impl Item {
    fn decode(bytes: &[u8], root: Root, descriptors: &[DescriptorSpec]) -> Result<Self> {
        let owner = usize::from(u16_at(bytes, 2));
        if !(1..=4).contains(&bytes[0])
            || !(1..=5).contains(&bytes[1])
            || owner >= root.owners
            || !zero(&bytes[4..8])
            || !zero(&bytes[80..96])
            || u64_at(bytes, 72) == 0
            || u64_at(bytes, 72) > root.capture
            || u64_at(bytes, 72) > descriptors[owner].inventory.change_lsn
        {
            return Err(InventoryError::Corrupt);
        }
        if bytes[0] == 4 {
            FileId::new(u64_at(bytes, 8)).map_err(|_| InventoryError::Corrupt)?;
            let base = u64_at(bytes, 16);
            let count = u64_at(bytes, 24);
            let end = count
                .checked_mul(64)
                .and_then(|n| base.checked_add(n))
                .and_then(|n| n.checked_mul(PAGE_BYTES as u64));
            if !base.is_multiple_of(64)
                || count == 0
                || u64_at(bytes, 32) == 0
                || !zero(&bytes[40..64])
                || end.is_none()
                || count.checked_mul(1 << 20) != Some(u64_at(bytes, 64))
            {
                return Err(InventoryError::Corrupt);
            }
        } else {
            let page = PageRef::decode(&bytes[8..32]).map_err(|_| InventoryError::Corrupt)?;
            if page.page_no.is_multiple_of(64)
                || ExtentAddress::containing(page).is_err()
                || u64_at(bytes, 64) != 1
            {
                return Err(InventoryError::Corrupt);
            }
            match bytes[0] {
                1 if !zero(&bytes[32..64]) => return Err(InventoryError::Corrupt),
                2 => {
                    let r = UndoRef::decode(&bytes[8..48]).map_err(|_| InventoryError::Corrupt)?;
                    if usize::from(r.slot()) >= MAX_UNDO_SLOTS || !zero(&bytes[48..64]) {
                        return Err(InventoryError::Corrupt);
                    }
                }
                3 => {
                    let r = StatusSlotRef::decode(&bytes[8..48])
                        .map_err(|_| InventoryError::Corrupt)?;
                    if usize::from(r.slot()) >= MAX_HOME_SLOTS || !zero(&bytes[48..64]) {
                        return Err(InventoryError::Corrupt);
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            bytes: bytes.try_into().expect("exact item slice"),
        })
    }
    fn kind(self) -> u8 {
        self.bytes[0]
    }
    fn owner(self) -> usize {
        usize::from(u16_at(&self.bytes, 2))
    }
    fn category(self) -> usize {
        usize::from(self.bytes[1] - 1)
    }
    fn file(self) -> u64 {
        u64_at(&self.bytes, 8)
    }
    fn page(self) -> u64 {
        u64_at(&self.bytes, 16)
    }
    fn incarnation(self) -> u64 {
        if self.kind() == 4 {
            0
        } else {
            u64_at(&self.bytes, 24)
        }
    }
    fn slot(self) -> u64 {
        if matches!(self.kind(), 2 | 3) {
            u64::from(u16_at(&self.bytes, 32))
        } else {
            0
        }
    }
    fn key(self) -> [u64; 9] {
        [
            u64::from(self.kind()),
            self.file(),
            self.page(),
            self.slot(),
            self.incarnation(),
            if matches!(self.kind(), 2 | 3) {
                u64_at(&self.bytes, 40)
            } else {
                0
            },
            if self.kind() == 4 {
                u64_at(&self.bytes, 32)
            } else {
                0
            },
            self.owner() as u64,
            self.category() as u64,
        ]
    }
    fn physical_key(self) -> [u64; 9] {
        let mut key = self.key();
        key[..3].rotate_left(1);
        key
    }
}

#[derive(Default)]
struct Cursor {
    next_chunk: u64,
    next_slot: usize,
    slots: usize,
    visited: u64,
    previous: Option<[u64; 9]>,
    digest: ContentHasher,
}
impl Cursor {
    fn next(
        &mut self,
        root: Root,
        descriptors: &[DescriptorSpec],
        source: &mut impl InventorySource,
        buffer: &mut [u8; PAGE_BYTES],
        calls: &mut u64,
    ) -> Result<Option<Item>> {
        if self.next_slot == self.slots {
            if self.next_chunk == root.chunks {
                if self.visited != root.items || self.digest.digest() != root.aggregate {
                    return Err(InventoryError::Corrupt);
                }
                return Ok(None);
            }
            let index = self.next_chunk;
            let count = (root.items - index * CHUNK_ITEMS).min(CHUNK_ITEMS) as usize;
            *calls += 1; // At most five times the bounded u32 chunk namespace.
            let length = source
                .read_chunk(index, buffer)
                .map_err(|_| InventoryError::Source)?;
            if length != 160 + count * ITEM_BYTES {
                return Err(InventoryError::Corrupt);
            }
            let bytes = &buffer[..length];
            envelope(bytes, 2)?;
            let declared_count = u32_at(bytes, 80) as usize;
            if !(1..=CHUNK_ITEMS as usize).contains(&declared_count)
                || length != 160 + ITEM_BYTES * declared_count
                || !zero(&bytes[84..88])
                || !zero(&bytes[120..128])
            {
                return Err(InventoryError::Corrupt);
            }
            if namespace(bytes)? != root.namespace
                || bytes[7] != root.bank
                || u64_at(bytes, 40) != root.generation
                || u64_at(bytes, 48) != root.capture
                || u64_at(bytes, 56) != index
                || u64_at(bytes, 64) != root.chunks
                || u64_at(bytes, 72) != index * CHUNK_ITEMS
                || declared_count != count
                || bytes[88..120] != root.owner_digest
            {
                return Err(InventoryError::IdentityMismatch);
            }
            self.digest.update(bytes);
            self.next_chunk += 1;
            self.next_slot = 0;
            self.slots = count;
        }
        let start = 128 + self.next_slot * ITEM_BYTES;
        let item = Item::decode(&buffer[start..start + ITEM_BYTES], root, descriptors)?;
        let key = item.key();
        if self.previous.is_some_and(|previous| previous >= key) {
            return Err(InventoryError::Corrupt);
        }
        self.previous = Some(key);
        self.next_slot += 1;
        self.visited += 1;
        Ok(Some(item))
    }

    fn next_kind(
        &mut self,
        kind: u8,
        root: Root,
        descriptors: &[DescriptorSpec],
        source: &mut impl InventorySource,
        buffer: &mut [u8; PAGE_BYTES],
        calls: &mut u64,
    ) -> Result<Option<Item>> {
        while let Some(item) = self.next(root, descriptors, source, buffer, calls)? {
            if item.kind() == kind {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }
}

#[derive(Default)]
struct OwnerTotals {
    digest: ContentHasher,
    count: u64,
    categories: [[u64; 4]; 5],
}
struct VerifierState {
    totals: [OwnerTotals; MAX_OWNERS],
    cursors: [Cursor; 4],
    heads: [Option<Item>; 4],
}

#[derive(Default)]
struct PhysicalOrder {
    growth_end: Option<(u64, u64)>,
    page: Option<(u64, u64, u8, u64, u64)>,
}
impl PhysicalOrder {
    fn accept(&mut self, item: Item) -> Result<()> {
        if let Some((file, end)) = self.growth_end {
            if file == item.file() && item.page() < end {
                return Err(InventoryError::Corrupt);
            }
            self.growth_end = None;
        }
        let same_page = self
            .page
            .filter(|p| (p.0, p.1) == (item.file(), item.page()));
        if item.kind() == 4 {
            if same_page.is_some() {
                return Err(InventoryError::Corrupt);
            }
            // Item decoding checked this exact multiplication and end.
            self.growth_end = Some((item.file(), item.page() + 64 * u64_at(&item.bytes, 24)));
            self.page = None;
        } else {
            if let Some((_, _, kind, incarnation, slot)) = same_page
                && (kind == 1
                    || item.kind() == 1
                    || kind != item.kind()
                    || incarnation != item.incarnation()
                    || slot >= item.slot())
            {
                return Err(InventoryError::Corrupt);
            }
            self.page = Some((
                item.file(),
                item.page(),
                item.kind(),
                item.incarnation(),
                item.slot(),
            ));
        }
        Ok(())
    }
}

/// Validate every observed chunk in five passes, retaining no inventory-sized
/// collection. Scratch may change on error; no source writes are requested.
pub(crate) fn verify_inventory(
    root_bytes: &[u8],
    expected: InventoryExpected,
    descriptors: &[DescriptorSpec],
    source: &mut impl InventorySource,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<InventoryObservation> {
    validate_expected(expected, descriptors)?;
    observe_inventory(root_bytes, expected, descriptors, source, scratch, None)
}

fn observe_inventory(
    root_bytes: &[u8],
    expected: InventoryExpected,
    descriptors: &[DescriptorSpec],
    source: &mut impl InventorySource,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
    mut intents: Option<&mut IntentCollector<'_>>,
) -> Result<InventoryObservation> {
    let root = decode_root(root_bytes, expected, descriptors)?;
    let mut state = VerifierState {
        totals: std::array::from_fn(|_| OwnerTotals::default()),
        cursors: std::array::from_fn(|_| Cursor::default()),
        heads: [None; 4],
    };
    let mut calls = 0;
    while let Some(item) =
        state.cursors[0].next(root, descriptors, source, &mut scratch[0], &mut calls)?
    {
        if let Some(collector) = intents.as_mut() {
            collector.accept(item)?;
        }
        let totals = &mut state.totals[item.owner()];
        let mut normalized = item.bytes;
        normalized[2..4].fill(0);
        totals.digest.update(&normalized);
        totals.count = totals.count.checked_add(1).ok_or(InventoryError::Corrupt)?;
        let counter = &mut totals.categories[item.category()][usize::from(item.kind() - 1)];
        *counter = counter
            .checked_add(u64_at(&item.bytes, 64))
            .ok_or(InventoryError::Corrupt)?;
    }
    for (totals, descriptor) in state.totals.iter().zip(descriptors) {
        if totals.count != descriptor.inventory.items
            || totals.digest.digest() != descriptor.inventory.digest
        {
            return Err(InventoryError::Corrupt);
        }
        for (actual, budget) in totals.categories.iter().zip(descriptor.remaining) {
            if *actual
                != [
                    budget.pages,
                    budget.undo_slots,
                    budget.status_slots,
                    budget.growth,
                ]
            {
                return Err(InventoryError::Corrupt);
            }
        }
    }
    for (lane, buffer) in scratch.iter_mut().enumerate() {
        state.cursors[lane] = Cursor::default();
        state.heads[lane] = state.cursors[lane].next_kind(
            lane as u8 + 1,
            root,
            descriptors,
            source,
            buffer,
            &mut calls,
        )?;
    }
    let mut physical = PhysicalOrder::default();
    loop {
        let next = state
            .heads
            .iter()
            .enumerate()
            .filter_map(|(lane, head)| head.map(|item| (lane, item)))
            .min_by_key(|(_, item)| item.physical_key());
        let Some((lane, item)) = next else {
            break;
        };
        physical.accept(item)?;
        state.heads[lane] = state.cursors[lane].next_kind(
            lane as u8 + 1,
            root,
            descriptors,
            source,
            &mut scratch[lane],
            &mut calls,
        )?;
    }
    debug_assert_eq!(calls, 5 * root.chunks);
    Ok(InventoryObservation {
        owners: root.owners,
        items: root.items,
        chunks: root.chunks,
        read_calls: calls,
    })
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
