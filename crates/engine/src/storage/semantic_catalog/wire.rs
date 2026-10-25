//! Exact bounded scope2 section layout. This validates framing and individual
//! record shapes, not the stable native graph or owner accounting transition.
use super::{CoreView, Error, Operation, Result, Shape, decode_core, u16_at, u32_at, u64_at, zero};
use crate::storage::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation, TableDefinitionView},
    control::Profile,
    transaction_record::TxnRef,
    wal_frame::{self, FrameExpectation, RedoKind, WalFrameHeader},
};
use foundation::{codec::content_digest, identity::Generation};

#[derive(Clone, Copy)]
pub(crate) struct LayoutView<'a> {
    bytes: &'a [u8],
    header: WalFrameHeader,
    operation: Operation,
    shape: Shape,
    native_at: usize,
}
impl<'a> LayoutView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub(crate) fn header(self) -> WalFrameHeader {
        self.header
    }
    pub(crate) fn operation(self) -> Operation {
        self.operation
    }
    pub(crate) fn shape(self) -> Shape {
        self.shape
    }
    pub(crate) fn group(self) -> &'a [u8] {
        &self.bytes[160..160 + if self.operation.handoff() { 256 } else { 192 }]
    }
    pub(crate) fn owner(self, ordinal: usize) -> Option<ReservationOwner> {
        let at = if self.operation.handoff() {
            if ordinal >= 2 {
                return None;
            }
            40 + ordinal * 56
        } else {
            if ordinal != 0 {
                return None;
            }
            32
        };
        Some(decode_owner(&self.group()[at..at + 56]).expect("validated owner"))
    }
    pub(crate) fn prior(self, ordinal: usize) -> Option<(u64, &'a [u8; 32])> {
        if ordinal >= if self.operation.handoff() { 2 } else { 1 } {
            return None;
        }
        let group = self.group();
        let (lsn, digest) = if self.operation.handoff() {
            (24 + ordinal * 8, 152 + ordinal * 32)
        } else {
            (24, 88)
        };
        Some((
            u64_at(group, lsn),
            group[digest..digest + 32].try_into().expect("digest"),
        ))
    }
    pub(crate) fn descriptor(self, ordinal: usize) -> Option<&'a [u8; 1536]> {
        if ordinal >= if self.operation.handoff() { 2 } else { 1 } {
            return None;
        }
        let at = 160 + self.group().len() + ordinal * 1536;
        Some(self.bytes[at..at + 1536].try_into().expect("descriptor"))
    }
    pub(crate) fn core(self) -> CoreView<'a> {
        decode_core(
            &self.bytes[self.native_at - 256..self.native_at],
            self.operation,
        )
        .expect("validated core")
    }
    pub(crate) fn native(self) -> &'a [u8] {
        &self.bytes[self.native_at..self.native_at + self.shape.native_bytes]
    }
    /// Framing-validated records only; neither native bytes nor schemas grant
    /// admission, page installation, or stable-before authority.
    pub(crate) fn native_records(self) -> NativeRecords<'a> {
        NativeRecords {
            bytes: self.native(),
        }
    }
    pub(crate) fn forward_table(
        self,
        expected: CatalogExpectation,
    ) -> Result<TableDefinitionView<'a>> {
        if self.operation != Operation::Forward {
            return Err(Error::InvalidInput);
        }
        let mut catalog = None;
        for native in self.native_records() {
            if native.kind() == 7 {
                if catalog.is_some() || native.before().is_some() {
                    return Err(Error::Corrupt);
                }
                catalog = Some(native.after().ok_or(Error::Corrupt)?);
            }
        }
        let cell = catalog.ok_or(Error::Corrupt)?;
        // Closed Catalog leaf cell: u16 key/value lengths, the stable object's
        // eight-byte big-endian key, then one independently decoded XCT1 value.
        if cell.len() < 12
            || u16_at(cell, 0) != 8
            || usize::from(u16_at(cell, 2)) != cell.len() - 12
            || cell[4..12] != expected.object.get().to_be_bytes()
        {
            return Err(Error::IdentityMismatch);
        }
        match catalog_record::decode_catalog_record(&cell[12..], expected)?.definition() {
            CatalogDefinitionView::Table(table) => Ok(table),
            CatalogDefinitionView::Index(_) => Err(Error::Unsupported),
        }
    }
    pub(crate) fn edges(self) -> &'a [u8] {
        let at = self.native_at + self.shape.native_bytes;
        &self.bytes[at..at + usize::from(self.shape.edges) * 128]
    }
    pub(crate) fn counts(self) -> &'a [u8] {
        let at = self.native_at + self.shape.native_bytes + self.edges().len();
        &self.bytes[at..at + usize::from(self.shape.counts) * 96]
    }
    pub(crate) fn items(self) -> &'a [u8] {
        let at =
            self.native_at + self.shape.native_bytes + self.edges().len() + self.counts().len();
        &self.bytes[at..at + usize::from(self.shape.items) * 208]
    }
    pub(crate) fn redo(self) -> &'a [u8] {
        &self.bytes[self.native_at + self.shape.native_bytes + self.shape.auxiliary_bytes()
            ..self.bytes.len() - 32]
    }
}

/// A borrowed record from a complete, already shape-validated LayoutView.
#[derive(Clone, Copy)]
pub(crate) struct NativeView<'a> {
    bytes: &'a [u8],
}
impl<'a> NativeView<'a> {
    pub(crate) fn kind(self) -> u8 {
        self.bytes[0]
    }
    pub(crate) fn redo_index(self) -> u16 {
        u16_at(self.bytes, 2)
    }
    pub(crate) fn before_slot(self) -> Option<u16> {
        (self.bytes[1] & 1 != 0).then(|| u16_at(self.bytes, 4))
    }
    pub(crate) fn after_slot(self) -> Option<u16> {
        (self.bytes[1] & 2 != 0).then(|| u16_at(self.bytes, 6))
    }
    pub(crate) fn before_offset(self) -> Option<u16> {
        (self.bytes[1] & 1 != 0).then(|| u16_at(self.bytes, 8))
    }
    pub(crate) fn after_offset(self) -> Option<u16> {
        (self.bytes[1] & 2 != 0).then(|| u16_at(self.bytes, 10))
    }
    pub(crate) fn before(self) -> Option<&'a [u8]> {
        (self.bytes[1] & 1 != 0).then(|| &self.bytes[128..128 + u32_at(self.bytes, 12) as usize])
    }
    pub(crate) fn after(self) -> Option<&'a [u8]> {
        (self.bytes[1] & 2 != 0).then(|| &self.bytes[128 + u32_at(self.bytes, 12) as usize..])
    }
}
pub(crate) struct NativeRecords<'a> {
    bytes: &'a [u8],
}
impl<'a> Iterator for NativeRecords<'a> {
    type Item = NativeView<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        let length = u32_at(self.bytes, 20) as usize;
        let (record, remaining) = self.bytes.split_at(length);
        self.bytes = remaining;
        Some(NativeView { bytes: record })
    }
}

fn decode_owner(bytes: &[u8]) -> Result<ReservationOwner> {
    if !zero(&bytes[1..8]) {
        return Err(Error::Corrupt);
    }
    match bytes[0] {
        1 => Ok(ReservationOwner::Transaction(TxnRef::decode(
            &bytes[8..56],
        )?)),
        2 => Ok(ReservationOwner::System(
            SystemWorkRef::decode(&bytes[8..56]).map_err(|_| Error::Corrupt)?,
        )),
        _ => Err(Error::Unsupported),
    }
}

pub(crate) fn decode_layout(
    bytes: &[u8],
    expected: FrameExpectation,
    profile: Profile,
) -> Result<LayoutView<'_>> {
    if profile.format() != 2
        || profile.spec().namespace.database != expected.stream.database
        || profile.spec().namespace.timeline != expected.stream.timeline
    {
        return Err(Error::IdentityMismatch);
    }
    if bytes.len() < 352 || bytes.len() > super::FRAME_BYTES {
        return Err(Error::Corrupt);
    }
    let envelope = match bytes[7] {
        2 => wal_frame::decode_semantic_envelope(bytes, expected)?,
        3 => wal_frame::decode_retention_envelope(bytes, expected)?,
        _ => return Err(Error::Unsupported),
    };
    let payload = envelope.payload;
    if payload.len() < 192 || u16_at(payload, 4) != 2 || u16_at(payload, 10) != 2 {
        return Err(Error::Unsupported);
    }
    let operation = Operation::decode(u16_at(payload, 6))?;
    let h = if operation.handoff() { 256 } else { 192 };
    if payload.len() < h
        || payload[..4]
            != *if operation.handoff() {
                b"XHG1"
            } else {
                b"XSG1"
            }
        || bytes[7] != if operation.handoff() { 3 } else { 2 }
        || usize::from(u16_at(payload, 8)) != h
        || u32_at(payload, 12) as usize != payload.len()
        || u64_at(payload, 16) != envelope.header.lsn
        || envelope.header.kind != operation.outer()
    {
        return Err(Error::Corrupt);
    }
    let native_bytes = u32_at(payload, if operation.handoff() { 228 } else { 136 }) as usize;
    let c = match operation {
        Operation::Forward => native_bytes.checked_sub(17904).ok_or(Error::Corrupt)?,
        Operation::Clr => native_bytes.checked_sub(17568).ok_or(Error::Corrupt)?,
        Operation::Prune => {
            let twice = native_bytes.checked_sub(5024).ok_or(Error::Corrupt)?;
            if twice % 2 != 0 {
                return Err(Error::Corrupt);
            }
            twice / 2
        }
        _ => 0,
    };
    let shape = operation.shape(c).map_err(|_| Error::Corrupt)?;
    if operation.frame_bytes(c)? != bytes.len() || envelope.record_count != shape.redo_count {
        return Err(Error::Corrupt);
    }
    if operation.handoff() {
        if u32_at(payload, 216) != 1536
            || u32_at(payload, 220) != 1536
            || u32_at(payload, 224) != 256
            || u32_at(payload, 228) as usize != shape.native_bytes
            || u32_at(payload, 232) as usize != shape.auxiliary_bytes()
            || u16_at(payload, 236) != shape.native_count
            || u16_at(payload, 238) != shape.redo_count
            || u32_at(payload, 240) as usize != shape.redo_bytes
            || u16_at(payload, 244) != shape.edges
            || u16_at(payload, 246) != shape.counts
            || u16_at(payload, 248) != shape.items
            || !zero(&payload[250..256])
        {
            return Err(Error::Corrupt);
        }
    } else if u32_at(payload, 120) != 1536
        || u16_at(payload, 124) != shape.native_count
        || u16_at(payload, 126) != shape.edges
        || u16_at(payload, 128) != shape.counts
        || u16_at(payload, 130) != shape.items
        || u16_at(payload, 132) != shape.redo_count
        || !zero(&payload[134..136])
        || u32_at(payload, 136) as usize != shape.native_bytes
        || u32_at(payload, 140) != u32::from(shape.edges) * 128
        || u32_at(payload, 144) != u32::from(shape.counts) * 96
        || u32_at(payload, 148) != u32::from(shape.items) * 208
        || u32_at(payload, 152) as usize != shape.redo_bytes
        || !zero(&payload[156..192])
    {
        return Err(Error::Corrupt);
    }
    let native_at = 160 + h + if operation.handoff() { 3072 } else { 1536 } + 256;
    let view = LayoutView {
        bytes,
        header: envelope.header,
        operation,
        shape,
        native_at,
    };
    decode_core(&bytes[native_at - 256..native_at], operation)?;
    validate_owners(view, profile)?;
    validate_native_shape(view)?;
    validate_redo_shape(view)?;
    Ok(view)
}

fn validate_owners(view: LayoutView<'_>, profile: Profile) -> Result<()> {
    let namespace = profile.spec().namespace;
    let aging = ReservationOwner::System(
        SystemWorkRef::new(
            namespace,
            SystemWorkKind::Aging,
            1,
            Generation::new(1).expect("one"),
        )
        .map_err(|_| Error::IdentityMismatch)?,
    );
    for ordinal in 0..if view.operation.handoff() { 2 } else { 1 } {
        let group = view.group();
        let at = if view.operation.handoff() {
            40 + ordinal * 56
        } else {
            32
        };
        let owner = decode_owner(&group[at..at + 56])?;
        let (previous, digest) = view.prior(ordinal).expect("existing ordinal");
        let fresh = view.operation == Operation::AgingStart
            || view.operation == Operation::Begin && ordinal == 0;
        if previous > view.header.previous_lsn || fresh != (previous == 0) || fresh && !zero(digest)
        {
            return Err(Error::OrderMismatch);
        }
        if ordinal == 0 && (view.operation.handoff() || view.operation == Operation::RollbackStart)
        {
            let ReservationOwner::Transaction(actor) = owner else {
                return Err(Error::IdentityMismatch);
            };
            if Some(actor.id()) != view.header.txn
                || actor.id().database != namespace.database
                || actor.id().timeline != namespace.timeline
                || actor.generation().get() < 2
                || previous != view.header.previous_txn_lsn
                || view.operation == Operation::Begin
                    && actor.id().writer_epoch != view.header.stream.writer_epoch
            {
                return Err(Error::IdentityMismatch);
            }
        } else if owner != aging {
            return Err(Error::IdentityMismatch);
        }
    }
    Ok(())
}

fn validate_native_shape(view: LayoutView<'_>) -> Result<()> {
    let mut bytes = view.native();
    let mut previous = None;
    for _ in 0..view.shape.native_count {
        if bytes.len() < 128 {
            return Err(Error::Corrupt);
        }
        let kind = bytes[0];
        if !matches!(kind, 3 | 5..=11) || !matches!(bytes[1], 1..=3) || !zero(&bytes[88..128]) {
            return Err(Error::Corrupt);
        }
        let redo = u16_at(bytes, 2);
        if redo >= view.shape.redo_count {
            return Err(Error::Corrupt);
        }
        let before = u32_at(bytes, 12) as usize;
        let after = u32_at(bytes, 16) as usize;
        let length = 128usize
            .checked_add(before)
            .and_then(|v| v.checked_add(after))
            .ok_or(Error::Corrupt)?;
        if length != u32_at(bytes, 20) as usize || length > bytes.len() {
            return Err(Error::Corrupt);
        }
        for (present, slot, offset, size, digest, data) in [
            (
                bytes[1] & 1 != 0,
                4,
                8,
                before,
                24,
                &bytes[128..128 + before],
            ),
            (
                bytes[1] & 2 != 0,
                6,
                10,
                after,
                56,
                &bytes[128 + before..length],
            ),
        ] {
            if present {
                if size == 0
                    || u16_at(bytes, slot) == u16::MAX
                    || usize::from(u16_at(bytes, offset)) < 72
                    || usize::from(u16_at(bytes, offset))
                        .checked_add(size)
                        .is_none_or(|end| end > 16352)
                    || bytes[digest..digest + 32] != content_digest(data)
                {
                    return Err(Error::Corrupt);
                }
            } else if u16_at(bytes, slot) != u16::MAX
                || u16_at(bytes, offset) != 0
                || size != 0
                || !zero(&bytes[digest..digest + 32])
            {
                return Err(Error::Corrupt);
            }
        }
        let rank = (
            redo,
            u16_at(bytes, if bytes[1] & 2 != 0 { 6 } else { 4 }),
            kind,
        );
        if previous
            .is_some_and(|old: (u16, u16, u8)| old >= rank || (old.0, old.1) == (rank.0, rank.1))
        {
            return Err(Error::OrderMismatch);
        }
        previous = Some(rank);
        bytes = &bytes[length..];
    }
    if !bytes.is_empty() {
        return Err(Error::Corrupt);
    }
    Ok(())
}

fn validate_redo_shape(view: LayoutView<'_>) -> Result<()> {
    let mut bytes = view.redo();
    let mut previous = None;
    let mut inits = 0;
    for _ in 0..view.shape.redo_count {
        if bytes.len() < 64 {
            return Err(Error::Corrupt);
        }
        let length = u32_at(bytes, 4) as usize;
        if length < 64 || length > bytes.len() {
            return Err(Error::Corrupt);
        }
        let redo = wal_frame::decode_redo_record(view.header, &bytes[..length])?;
        let mut reference = [0; 24];
        redo.target.page.encode(&mut reference)?;
        let rank = (u64_at(&reference, 0), redo.target.page.page_no);
        if previous.is_some_and(|old| old >= rank) {
            return Err(Error::OrderMismatch);
        }
        previous = Some(rank);
        match redo.kind {
            RedoKind::Init => {
                inits += 1;
                if view.operation != Operation::Forward || length != 16448 {
                    return Err(Error::Corrupt);
                }
            }
            RedoKind::Patch => {
                let (offset, data) = if view.operation == Operation::RollbackStart {
                    (16128, 224)
                } else {
                    (72, 16280)
                };
                if redo.offset != offset || redo.data.len() != data {
                    return Err(Error::Corrupt);
                }
            }
        }
        bytes = &bytes[length..];
    }
    if !bytes.is_empty()
        || inits
            != if view.operation == Operation::Forward {
                2
            } else {
                0
            }
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}
