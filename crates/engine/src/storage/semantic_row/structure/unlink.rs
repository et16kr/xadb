//! Exact scope4 framing for Row-tree unlink groups: LeafUnlink (opcode 3) and
//! RootCollapse (opcode 4), dev3-tree-smo.md#empty-leaf-unlink. Like the split
//! layout, these immutable observations grant no stage, install or replay
//! authority; the canonical before images and the recomputed candidate
//! (`candidate::prepare`) are still required for every native and redo byte.

use super::{Error, Operation, Result, same_address, u16_at, u32_at, u64_at, zero};
use crate::storage::{
    control::Profile,
    page::{MAX_PAGE_SLOTS, PageKind, PageRef},
    wal_frame::{
        self, FrameExpectation, FrameKind, RedoKind, RedoRecordView, RedoTarget, WalFrameHeader,
    },
};
use foundation::identity::{Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch};

pub(crate) mod candidate;
pub(crate) mod frame;
mod source;
pub(crate) use source::{layout_source_bytes, source_bytes};
#[cfg(test)]
mod source_tests;

const CORE_AT: usize = super::CORE_AT;
const NATIVE_AT: usize = super::NATIVE_AT;
/// Redo ordinal of a native whose page the group does not change.
pub(crate) const UNCHANGED: u16 = 0xFFFF;

/// Native roles (byte 0). Not the provenance claim roles 5/6 of a split.
pub(crate) const ROLE_ANCHOR: u8 = 1;
pub(crate) const ROLE_PARENT: u8 = 4;
pub(crate) const ROLE_LEAF: u8 = 6;
pub(crate) const ROLE_SURVIVOR: u8 = 7;
pub(crate) const ROLE_RETIRED_PARENT: u8 = 8;

impl Operation {
    fn is_unlink(self) -> bool {
        matches!(self, Self::LeafUnlink | Self::RootCollapse)
    }
    /// Fixed native records: anchor, parent and leaf, plus the survivor.
    pub(crate) fn natives(self) -> usize {
        if self == Self::RootCollapse { 4 } else { 3 }
    }
    /// Changed pages, one Patch redo record each: anchor and parent, or the
    /// anchor alone. This is also the envelope record count.
    pub(crate) fn redo_records(self) -> usize {
        if self == Self::RootCollapse { 1 } else { 2 }
    }
    fn roles(self) -> u16 {
        if self == Self::RootCollapse {
            1 << ROLE_ANCHOR | 1 << ROLE_LEAF | 1 << ROLE_SURVIVOR | 1 << ROLE_RETIRED_PARENT
        } else {
            1 << ROLE_ANCHOR | 1 << ROLE_PARENT | 1 << ROLE_LEAF
        }
    }
    pub(crate) fn changes(self, role: u8) -> bool {
        role == ROLE_ANCHOR || role == ROLE_PARENT && self == Self::LeafUnlink
    }
    fn redo_at(self) -> usize {
        NATIVE_AT + self.natives() * 128
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
    pub(crate) fn leaf(self) -> PageRef {
        self.page(56)
    }
    pub(crate) fn survivor(self) -> Option<PageRef> {
        (self.operation() == Operation::RootCollapse).then(|| self.page(80))
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
    pub(crate) fn root_level(self) -> u8 {
        self.bytes[163]
    }
    pub(crate) fn separators(self) -> u16 {
        u16_at(self.bytes, 164)
    }
    pub(crate) fn removed_child(self) -> u64 {
        u64_at(self.bytes, 168)
    }
    pub(crate) fn previous_high_water(self) -> u64 {
        u64_at(self.bytes, 176)
    }
    pub(crate) fn separator(self) -> RowId {
        RowId::new(u64_at(self.bytes, 184)).expect("validated separator")
    }
    pub(crate) fn retired_after(self) -> u64 {
        u64_at(self.bytes, 192)
    }
    pub(crate) fn previous_digest(self) -> &'a [u8; 32] {
        self.bytes[224..].try_into().expect("fixed digest")
    }
}

/// Local scalar topology only. The canonical tree supplies the actual leaf,
/// parent, separator, root and list in the recomputed candidate.
pub(crate) fn decode_core(bytes: &[u8], operation: Operation) -> Result<CoreView<'_>> {
    let bytes: &[u8; 256] = bytes.try_into().map_err(|_| Error::Corrupt)?;
    if !operation.is_unlink()
        || &bytes[..4] != b"XSM1"
        || u16_at(bytes, 4) != 1
        || Operation::decode(u16_at(bytes, 6))? != operation
    {
        return Err(Error::Unsupported);
    }
    let collapse = operation == Operation::RootCollapse;
    if bytes[162] != 0
        || !zero(&bytes[166..168])
        || !zero(&bytes[200..224])
        || !collapse && !zero(&bytes[80..104])
    {
        return Err(Error::Corrupt);
    }
    ObjectId::new(u64_at(bytes, 8)).map_err(|_| Error::Corrupt)?;
    Generation::new(u64_at(bytes, 16)).map_err(|_| Error::Corrupt)?;
    SchemaEpoch::new(u64_at(bytes, 24)).map_err(|_| Error::Corrupt)?;
    for at in [32, 56, 104, 128] {
        PageRef::decode(&bytes[at..at + 24])?;
    }
    if collapse {
        PageRef::decode(&bytes[80..104])?;
    }
    let v = CoreView { bytes };
    let separators = u64::from(v.separators());
    if bytes[160] != PageKind::RowLeaf as u8
        || bytes[161] != 0
        || !(1..=15).contains(&v.root_level())
        || collapse && v.root_level() != 1
        || v.revision() == 0
        || v.revision() == u64::MAX
        || separators >= MAX_PAGE_SLOTS as u64
        || if collapse {
            separators != 1
        } else {
            separators < 2
        }
        || v.removed_child() > separators
        || RowId::new(u64_at(bytes, 184)).is_err()
        || !(u64::from(1 + u8::from(collapse))..=55).contains(&v.retired_after())
        || (v.previous_high_water() == 0) != zero(v.previous_digest())
    {
        return Err(Error::Corrupt);
    }
    let mut pages = [v.anchor(), v.leaf(), v.parent(), v.anchor()];
    let count = if collapse {
        pages[3] = v.page(80);
        4
    } else {
        3
    };
    for (i, page) in pages[..count].iter().enumerate() {
        if pages[..i].iter().any(|other| same_address(*page, *other)) {
            return Err(Error::IdentityMismatch);
        }
    }
    let root = v.before_root();
    let parent_is_root = v.root_level() == 1;
    if same_address(root, v.anchor())
        || same_address(root, v.leaf())
        || collapse && same_address(root, pages[3])
        || parent_is_root != (root == v.parent())
        || same_address(root, v.parent()) && root != v.parent()
    {
        return Err(Error::IdentityMismatch);
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
    /// Dense redo index of a changed page, or `UNCHANGED`.
    pub(crate) fn ordinal(self) -> u16 {
        u16_at(self.bytes, 2)
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
        if index >= self.operation.natives() {
            return None;
        }
        Some(NativeView {
            bytes: self.bytes[NATIVE_AT + index * 128..NATIVE_AT + (index + 1) * 128]
                .try_into()
                .expect("fixed native"),
        })
    }
    /// Patch records of the changed natives, in physical native order.
    pub(crate) fn redo(self) -> impl Iterator<Item = RedoRecordView<'a>> {
        let header = self.header;
        let mut bytes = &self.bytes[self.operation.redo_at()..self.bytes.len() - 32];
        std::iter::from_fn(move || {
            if bytes.is_empty() {
                return None;
            }
            let len = u32_at(bytes, 4) as usize;
            let (record, rest) = bytes.split_at(len);
            bytes = rest;
            Some(wal_frame::decode_redo_record(header, record).expect("immutable validated redo"))
        })
    }
}

/// Exact opcode 3/4 grammar: fixed length, record/native/redo counts and
/// bytes, the Finished owner, the core topology, the exact role set and
/// the changed/unchanged native split. Split frames are `Unsupported`.
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
    if !matches!(bytes.len(), 35_248 | 19_032) {
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
    if !operation.is_unlink() {
        return Err(Error::Unsupported);
    }
    let natives = operation.natives();
    let redo = operation.redo_records();
    if bytes.len() != operation.frame_bytes()
        || envelope.record_count as usize != redo
        || u16_at(group, 8) != 192
        || u32_at(group, 12) as usize != bytes.len() - 192
        || u64_at(group, 16) != envelope.header.lsn
        || u32_at(group, 120) != 1536
        || usize::from(u16_at(group, 124)) != natives
        || !zero(&group[126..132])
        || usize::from(u16_at(group, 132)) != redo
        || !zero(&group[134..136])
        || u32_at(group, 136) as usize != natives * 128
        || !zero(&group[140..152])
        || u32_at(group, 152) as usize != bytes.len() - operation.redo_at() - 32
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
    super::validate_finished_owner(view.header, view.descriptor(), bytes.len(), group, profile)?;
    validate_records(view)?;
    Ok(view)
}

fn validate_records(view: LayoutView<'_>) -> Result<()> {
    let core = view.core();
    let operation = view.operation;
    let mut remaining = &view.bytes[operation.redo_at()..view.bytes.len() - 32];
    let mut roles = 0u16;
    let mut previous = None;
    let mut changed = 0u16;
    for index in 0..operation.natives() {
        let native = view.native(index).expect("fixed native records");
        let raw = native.bytes;
        let role = native.role();
        if role >= 16 || operation.roles() & (1 << role) == 0 || roles & (1 << role) != 0 {
            return Err(Error::IdentityMismatch);
        }
        roles |= 1 << role;
        if raw[1] != 0 || !zero(&raw[6..8]) || !zero(&raw[120..128]) {
            return Err(Error::Corrupt);
        }
        // Do not construct a NativeView target until its individual fields are valid.
        PageRef::decode(&raw[8..32])?;
        ObjectId::new(u64_at(raw, 32)).map_err(|_| Error::Corrupt)?;
        Generation::new(u64_at(raw, 40)).map_err(|_| Error::Corrupt)?;
        PageKind::from_byte(raw[4])?;
        let target = native.target();
        let (kind, level, page) = match role {
            ROLE_ANCHOR => (PageKind::TreeAnchor, 0, core.anchor()),
            ROLE_PARENT | ROLE_RETIRED_PARENT => (PageKind::RowInternal, 1, core.parent()),
            ROLE_LEAF => (PageKind::RowLeaf, 0, core.leaf()),
            ROLE_SURVIVOR => (PageKind::RowLeaf, 0, core.survivor().expect("collapse")),
            _ => unreachable!("checked role"),
        };
        if target.kind != kind
            || target.level != level
            || target.page != page
            || target.owner != core.object().object
            || target.owner_generation != core.object().generation
        {
            return Err(Error::IdentityMismatch);
        }
        let rank = (u64_at(raw, 8), target.page.page_no);
        if previous.is_some_and(|old| old >= rank) {
            return Err(Error::OrderMismatch);
        }
        previous = Some(rank);
        if native.prior_lsn() == 0 || native.prior_lsn() >= view.header.lsn {
            return Err(Error::OrderMismatch);
        }
        if !operation.changes(role) {
            if native.ordinal() != UNCHANGED || native.after_digest() != native.before_digest() {
                return Err(Error::Corrupt);
            }
            continue;
        }
        if native.ordinal() != changed || native.after_digest() == native.before_digest() {
            return Err(Error::OrderMismatch);
        }
        changed += 1;
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
        if redo.kind != RedoKind::Patch || redo.offset != 72 || redo.data.len() != 16280 {
            return Err(Error::Corrupt);
        }
        remaining = &remaining[length..];
    }
    if roles != operation.roles()
        || usize::from(changed) != operation.redo_records()
        || !remaining.is_empty()
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}
