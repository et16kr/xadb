//! Exact scope4 framing for Row-tree structural groups (D3-TREE-SMO-001).
//! These immutable observations grant no allocation, INIT, stage, install or
//! replay authority. Canonical before images and split recomputation are still
//! required, including both native digests and every claimed rollback weight.

use super::{Error, Result, u16_at, u32_at, u64_at, zero};
use crate::storage::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    control::Profile,
    obligation::{self, InventoryClaim, OwnerChain, OwnerState, RemainingBudget, WorkPhase},
    page::{MAX_PAGE_SLOTS, PageKind, PageRef},
    retention_obligation::profile_reference,
    tree_page::TREE_ENTRY_CAPACITY,
    wal_frame::{
        self, FrameExpectation, FrameKind, RedoKind, RedoRecordView, RedoTarget, WalFrameHeader,
    },
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch},
};

const CORE_AT: usize = 160 + 192 + 1536;
const NATIVE_AT: usize = CORE_AT + 256;
const REDO_AT: usize = NATIVE_AT + 5 * 128;

pub(crate) mod bank;
pub(crate) mod candidate;
pub(crate) mod frame;
pub(crate) mod unlink;
pub(crate) mod weights;

/// Scope4 opcode. Splits (1, 2) and unlinks (3, 4) have disjoint frame
/// grammars: `decode_layout` accepts only splits, `unlink::decode_layout`
/// only unlinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum Operation {
    RootSplit = 1,
    ChildSplit = 2,
    /// Empty non-root leaf removed from a parent with two or more separators.
    LeafUnlink = 3,
    /// Level-1 root with one separator replaced by its other (leaf) child.
    RootCollapse = 4,
}
impl Operation {
    fn decode(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::RootSplit),
            2 => Ok(Self::ChildSplit),
            3 => Ok(Self::LeafUnlink),
            4 => Ok(Self::RootCollapse),
            _ => Err(Error::Unsupported),
        }
    }
    pub(crate) fn frame_bytes(self) -> usize {
        match self {
            Self::RootSplit => 84_744,
            Self::ChildSplit => 84_640,
            Self::LeafUnlink => 35_248,
            Self::RootCollapse => 19_032,
        }
    }
    pub(crate) fn is_split(self) -> bool {
        matches!(self, Self::RootSplit | Self::ChildSplit)
    }
    /// Split INIT roles only; an unlink initializes no page.
    fn init(self, role: u8) -> bool {
        self.is_split() && (role == 3 || role == 4 && self == Self::RootSplit)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CoreView<'a> {
    bytes: &'a [u8; 256],
}
impl<'a> CoreView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; 256] {
        self.bytes
    }
    pub(crate) fn operation(self) -> Operation {
        Operation::decode(u16_at(self.bytes, 6)).expect("validated opcode")
    }
    pub(crate) fn object(self) -> ObjectHandle {
        ObjectHandle {
            object: ObjectId::new(u64_at(self.bytes, 8)).expect("validated object"),
            generation: Generation::new(u64_at(self.bytes, 16)).expect("validated generation"),
            schema_epoch: SchemaEpoch::new(u64_at(self.bytes, 24)).expect("validated schema"),
        }
    }
    fn page(self, at: usize) -> PageRef {
        PageRef::decode(&self.bytes[at..at + 24]).expect("validated reference")
    }
    pub(crate) fn anchor(self) -> PageRef {
        self.page(32)
    }
    pub(crate) fn left(self) -> PageRef {
        self.page(56)
    }
    pub(crate) fn right(self) -> PageRef {
        self.page(80)
    }
    pub(crate) fn parent(self) -> PageRef {
        self.page(104)
    }
    pub(crate) fn before_root(self) -> PageRef {
        self.page(128)
    }
    pub(crate) fn revision(self) -> u64 {
        u64_at(self.bytes, 152)
    }
    pub(crate) fn kind(self) -> PageKind {
        PageKind::from_byte(self.bytes[160]).expect("validated kind")
    }
    pub(crate) fn level(self) -> u8 {
        self.bytes[161]
    }
    pub(crate) fn root_level(self) -> u8 {
        self.bytes[163]
    }
    pub(crate) fn boundary(self) -> u64 {
        u64_at(self.bytes, 168)
    }
    pub(crate) fn previous_high_water(self) -> u64 {
        u64_at(self.bytes, 176)
    }
    pub(crate) fn weighted_costs(self) -> [u64; 2] {
        [u64_at(self.bytes, 184), u64_at(self.bytes, 192)]
    }
    pub(crate) fn actual_costs(self) -> [u64; 2] {
        [u64_at(self.bytes, 200), u64_at(self.bytes, 208)]
    }
    pub(crate) fn previous_digest(self) -> &'a [u8; 32] {
        self.bytes[224..].try_into().expect("fixed digest")
    }
}

fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}

/// Validate local scalar topology only. The source tree supplies the actual
/// boundary, costs, parent link and root revision in a later semantic check.
pub(crate) fn decode_core(bytes: &[u8], operation: Operation) -> Result<CoreView<'_>> {
    let bytes: &[u8; 256] = bytes.try_into().map_err(|_| Error::Corrupt)?;
    if !operation.is_split()
        || &bytes[..4] != b"XSM1"
        || u16_at(bytes, 4) != 1
        || Operation::decode(u16_at(bytes, 6))? != operation
    {
        return Err(Error::Unsupported);
    }
    if !zero(&bytes[164..168]) || !zero(&bytes[216..224]) {
        return Err(Error::Corrupt);
    }
    ObjectId::new(u64_at(bytes, 8)).map_err(|_| Error::Corrupt)?;
    Generation::new(u64_at(bytes, 16)).map_err(|_| Error::Corrupt)?;
    SchemaEpoch::new(u64_at(bytes, 24)).map_err(|_| Error::Corrupt)?;
    for at in [32, 56, 80, 104, 128] {
        PageRef::decode(&bytes[at..at + 24])?;
    }
    let kind = PageKind::from_byte(bytes[160])?;
    if !matches!(kind, PageKind::RowLeaf | PageKind::RowInternal) {
        return Err(Error::Unsupported);
    }
    let v = CoreView { bytes };
    if !kind.accepts_level(v.level())
        || v.level() >= 15
        || bytes[162] != 0
        || v.root_level() > 15
        || v.revision() == 0
        || v.revision() == u64::MAX
        || v.boundary() == 0
        || v.boundary() >= MAX_PAGE_SLOTS as u64
        || (v.previous_high_water() == 0) != zero(v.previous_digest())
    {
        return Err(Error::Corrupt);
    }
    let pages = [v.anchor(), v.left(), v.right(), v.parent()];
    for (i, page) in pages.iter().enumerate() {
        if pages[..i].iter().any(|other| same_address(*page, *other)) {
            return Err(Error::IdentityMismatch);
        }
    }
    match operation {
        Operation::RootSplit if v.before_root() != v.left() || v.root_level() != v.level() => {
            return Err(Error::IdentityMismatch);
        }
        Operation::ChildSplit
            if v.root_level() <= v.level()
                || same_address(v.before_root(), v.anchor())
                || same_address(v.before_root(), v.left())
                || same_address(v.before_root(), v.right())
                || (v.root_level() == v.level() + 1) != (v.before_root() == v.parent())
                || same_address(v.before_root(), v.parent()) && v.before_root() != v.parent() =>
        {
            return Err(Error::IdentityMismatch);
        }
        _ => (),
    }
    let weighted = v.weighted_costs();
    let actual = v.actual_costs();
    if weighted
        .iter()
        .any(|cost| *cost > TREE_ENTRY_CAPACITY as u64)
        || weighted[0] + weighted[1] > TREE_ENTRY_CAPACITY as u64
        || actual
            .iter()
            .zip(weighted)
            .any(|(actual, weight)| *actual == 0 || *actual > weight)
        || kind == PageKind::RowInternal && actual != weighted
    {
        return Err(Error::BudgetMismatch);
    }
    Ok(v)
}

#[derive(Clone, Copy)]
pub(crate) struct NativeView<'a> {
    bytes: &'a [u8; 128],
}
impl<'a> NativeView<'a> {
    pub(crate) fn role(self) -> u8 {
        self.bytes[0]
    }
    pub(crate) fn is_init(self) -> bool {
        self.bytes[1] == 1
    }
    pub(crate) fn target(self) -> RedoTarget {
        RedoTarget {
            page: PageRef::decode(&self.bytes[8..32]).expect("validated native reference"),
            owner: ObjectId::new(u64_at(self.bytes, 32)).expect("validated native owner"),
            owner_generation: Generation::new(u64_at(self.bytes, 40))
                .expect("validated native generation"),
            kind: PageKind::from_byte(self.bytes[4]).expect("validated native kind"),
            level: self.bytes[5],
        }
    }
    pub(crate) fn prior_lsn(self) -> u64 {
        u64_at(self.bytes, 48)
    }
    pub(crate) fn before_digest(self) -> &'a [u8; 32] {
        self.bytes[56..88].try_into().expect("fixed digest")
    }
    pub(crate) fn after_digest(self) -> &'a [u8; 32] {
        self.bytes[88..120].try_into().expect("fixed digest")
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LayoutView<'a> {
    bytes: &'a [u8],
    header: WalFrameHeader,
    operation: Operation,
}
impl<'a> LayoutView<'a> {
    pub(crate) fn header(self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn operation(self) -> Operation {
        self.operation
    }
    pub(crate) fn core(self) -> CoreView<'a> {
        CoreView {
            bytes: self.bytes[CORE_AT..NATIVE_AT]
                .try_into()
                .expect("fixed core"),
        }
    }
    pub(crate) fn descriptor(self) -> &'a [u8; 1536] {
        self.bytes[352..CORE_AT]
            .try_into()
            .expect("fixed descriptor")
    }
    pub(crate) fn native(self, index: usize) -> Option<NativeView<'a>> {
        if index >= 5 {
            return None;
        }
        Some(NativeView {
            bytes: self.bytes[NATIVE_AT + index * 128..NATIVE_AT + (index + 1) * 128]
                .try_into()
                .expect("fixed native"),
        })
    }
    pub(crate) fn redo(self) -> RedoRecords<'a> {
        RedoRecords {
            bytes: &self.bytes[REDO_AT..self.bytes.len() - 32],
            header: self.header,
        }
    }
}
pub(crate) struct RedoRecords<'a> {
    bytes: &'a [u8],
    header: WalFrameHeader,
}
impl<'a> Iterator for RedoRecords<'a> {
    type Item = RedoRecordView<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        let len = u32_at(self.bytes, 4) as usize;
        let (bytes, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        Some(wal_frame::decode_redo_record(self.header, bytes).expect("immutable validated redo"))
    }
}

pub(crate) fn decode_layout(
    bytes: &[u8],
    expected: FrameExpectation,
    profile: Profile,
) -> Result<LayoutView<'_>> {
    if profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    if profile.spec().namespace.database != expected.stream.database
        || profile.spec().namespace.timeline != expected.stream.timeline
    {
        return Err(Error::IdentityMismatch);
    }
    if !matches!(bytes.len(), 84_744 | 84_640) {
        return Err(Error::Corrupt);
    }
    let envelope = wal_frame::decode_semantic_envelope(bytes, expected)?;
    let group = &envelope.payload[..192];
    if &group[..4] != b"XSG1"
        || u16_at(group, 4) != 2
        || u16_at(group, 10) != 4
        || envelope.header.kind != FrameKind::System
    {
        return Err(Error::Unsupported);
    }
    let operation = Operation::decode(u16_at(group, 6))?;
    if !operation.is_split() {
        return Err(Error::Unsupported);
    }
    if bytes.len() != operation.frame_bytes()
        || envelope.record_count != 5
        || u16_at(group, 8) != 192
        || u32_at(group, 12) as usize != bytes.len() - 192
        || u64_at(group, 16) != envelope.header.lsn
        || u32_at(group, 120) != 1536
        || u16_at(group, 124) != 5
        || !zero(&group[126..132])
        || u16_at(group, 132) != 5
        || !zero(&group[134..136])
        || u32_at(group, 136) != 640
        || !zero(&group[140..152])
        || u32_at(group, 152) as usize != bytes.len() - REDO_AT - 32
        || !zero(&group[156..192])
    {
        return Err(Error::Corrupt);
    }
    if envelope.header.txn.is_some()
        || envelope.header.previous_txn_lsn != 0
        || !zero(&group[24..32])
        || !zero(&group[88..120])
    {
        return Err(Error::OrderMismatch);
    }
    let core = decode_core(&bytes[CORE_AT..NATIVE_AT], operation)?;
    if core.previous_high_water() > envelope.header.previous_lsn {
        return Err(Error::OrderMismatch);
    }
    let view = LayoutView {
        bytes,
        header: envelope.header,
        operation,
    };
    validate_owner(view, group, profile)?;
    validate_records(view)?;
    Ok(view)
}

fn validate_owner(view: LayoutView<'_>, group: &[u8], profile: Profile) -> Result<()> {
    validate_finished_owner(
        view.header,
        view.descriptor(),
        view.bytes.len(),
        group,
        profile,
    )
}

/// The one Finished/zero-residue Structural owner of a scope4 group, for
/// both split and unlink frames (consumed WAL = exact frame bytes).
pub(super) fn validate_finished_owner(
    header: WalFrameHeader,
    descriptor: &[u8; 1536],
    frame_bytes: usize,
    group: &[u8],
    profile: Profile,
) -> Result<()> {
    if group[32] != 2 || !zero(&group[33..40]) {
        return Err(Error::Corrupt);
    }
    let owner = SystemWorkRef::decode(&group[40..88]).map_err(|_| Error::Corrupt)?;
    if owner.namespace() != profile.spec().namespace
        || owner.kind() != SystemWorkKind::StructuralChange
        || owner.id() != header.lsn
        || owner.generation().get() != u64::from(header.stream.writer_epoch)
    {
        return Err(Error::IdentityMismatch);
    }
    let spec = obligation::decode_descriptor(
        descriptor,
        ReservationOwner::System(owner),
        profile_reference(profile),
    )?;
    let OwnerState::System(progress) = spec.owner else {
        return Err(Error::IdentityMismatch);
    };
    if progress.phase != WorkPhase::Finished
        || progress.sequence != 1
        || progress.cursor != 1
        || progress.limit != 1
        || spec.chain
            != (OwnerChain {
                apply: header.lsn,
                previous: 0,
                birth: header.lsn,
                groups: 1,
            })
    {
        return Err(Error::OrderMismatch);
    }
    if spec.inventory
        != (InventoryClaim {
            change_lsn: header.lsn,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        })
        || spec.consumed_wal != frame_bytes as u64
        || spec.consumed_control != 0
        || spec.remaining != [RemainingBudget::default(); 5]
        || spec.minimum_wal_lsn != 0
        || spec.last_mutation_sequence != 0
    {
        return Err(Error::BudgetMismatch);
    }
    Ok(())
}

fn validate_records(view: LayoutView<'_>) -> Result<()> {
    let core = view.core();
    let mut remaining = &view.bytes[REDO_AT..view.bytes.len() - 32];
    let mut roles = 0u8;
    let mut previous = None;
    for index in 0..5 {
        let native = view.native(index).expect("five fixed native records");
        let raw = native.bytes;
        let role = native.role();
        if role > 4 || roles & (1 << role) != 0 {
            return Err(Error::IdentityMismatch);
        }
        roles |= 1 << role;
        if raw[1] != u8::from(view.operation.init(role))
            || !zero(&raw[6..8])
            || !zero(&raw[120..128])
        {
            return Err(Error::Corrupt);
        }
        if usize::from(u16_at(raw, 2)) != index {
            return Err(Error::OrderMismatch);
        }
        // Do not construct a NativeView target until its individual fields are valid.
        PageRef::decode(&raw[8..32])?;
        ObjectId::new(u64_at(raw, 32)).map_err(|_| Error::Corrupt)?;
        Generation::new(u64_at(raw, 40)).map_err(|_| Error::Corrupt)?;
        PageKind::from_byte(raw[4])?;
        let target = native.target();
        let (kind, level, page) = match role {
            0 => (PageKind::Allocation, 0, target.page),
            1 => (PageKind::TreeAnchor, 0, core.anchor()),
            2 => (core.kind(), core.level(), core.left()),
            3 => (core.kind(), core.level(), core.right()),
            4 => (PageKind::RowInternal, core.level() + 1, core.parent()),
            _ => unreachable!("checked role"),
        };
        if target.kind != kind
            || target.level != level
            || target.page != page
            || role == 0 && same_address(target.page, core.before_root())
            || role != 0
                && (target.owner != core.object().object
                    || target.owner_generation != core.object().generation)
        {
            return Err(Error::IdentityMismatch);
        }
        let rank = (u64_at(raw, 8), target.page.page_no);
        if previous.is_some_and(|old| old >= rank) {
            return Err(Error::OrderMismatch);
        }
        previous = Some(rank);
        if native.is_init() {
            if native.prior_lsn() != 0 || !zero(native.before_digest()) {
                return Err(Error::Corrupt);
            }
        } else if native.prior_lsn() >= view.header.lsn {
            return Err(Error::OrderMismatch);
        }
        if remaining.len() < 64 {
            return Err(Error::Corrupt);
        }
        let length = u32_at(remaining, 4) as usize;
        if length < 64 || length > remaining.len() {
            return Err(Error::Corrupt);
        }
        let redo = wal_frame::decode_redo_record(view.header, &remaining[..length])?;
        if redo.target != target || redo.prior_lsn != native.prior_lsn() {
            return Err(Error::IdentityMismatch);
        }
        if native.is_init() {
            if redo.kind != RedoKind::Init || native.after_digest() != &content_digest(redo.data) {
                return Err(Error::Corrupt);
            }
        } else if redo.kind != RedoKind::Patch || redo.offset != 72 || redo.data.len() != 16280 {
            return Err(Error::Corrupt);
        }
        remaining = &remaining[length..];
    }
    if roles != 31 || !remaining.is_empty() {
        return Err(Error::Corrupt);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
