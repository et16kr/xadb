//! Bounded scope2 serialization and comparison with computed images. The
//! producer computes every after image/descriptor first. This layer never
//! supplies a stage/install grant or accepts candidate bytes as a before state.
use super::{
    EffectIndices, Error, Operation, Result, decode_core, decode_layout, inventory, put16, put32,
    put64, reference_changes, u64_at,
};
use crate::storage::{
    control::Profile,
    page::{self, PAGE_BYTES, PageHeader, PageKind, PageView},
    retention_page::catalog::JobView,
    wal_frame::{self, FrameExpectation, RedoKind, RedoTarget, WalFrameHeader},
};
use foundation::codec::content_digest;

#[derive(Clone, Copy)]
pub(super) struct PageChange<'a> {
    pub(super) header: PageHeader,
    pub(super) before: Option<&'a [u8; PAGE_BYTES]>,
    pub(super) after: &'a [u8; PAGE_BYTES],
    pub(super) before_slot: Option<u16>,
    pub(super) after_slot: Option<u16>,
}
impl<'a> PageChange<'a> {
    fn kind(self) -> Result<u8> {
        Ok(match self.header.kind {
            PageKind::Undo => 3,
            PageKind::TransactionStatus => 5,
            PageKind::Allocation => 6,
            PageKind::CatalogLeaf => 7,
            PageKind::RetentionRoot => 8,
            PageKind::RetentionJobs => 9,
            PageKind::TreeAnchor => 10,
            PageKind::RowLeaf => 11,
            _ => return Err(Error::Unsupported),
        })
    }
    fn target(self) -> RedoTarget {
        RedoTarget {
            page: self.header.identity.page,
            owner: self.header.identity.owner,
            owner_generation: self.header.identity.owner_generation,
            kind: self.header.kind,
            level: self.header.level,
        }
    }
    fn views(self, operation: Operation) -> Result<(Option<PageView<'a>>, PageView<'a>)> {
        let after = page::decode_page(self.after, self.header.identity, self.header.kind)?;
        if after.header() != self.header || self.header.level != 0 {
            return Err(Error::IdentityMismatch);
        }
        let before = self
            .before
            .map(|bytes| page::decode_page(bytes, self.header.identity, self.header.kind))
            .transpose()?;
        if let Some(before) = before.as_ref() {
            if before.header().level != self.header.level
                || before.header().lsn.0 >= self.header.lsn.0
            {
                return Err(Error::OrderMismatch);
            }
        } else if operation != Operation::Forward
            || !matches!(self.header.kind, PageKind::TreeAnchor | PageKind::RowLeaf)
            || self.before_slot.is_some()
            || self.after_slot != Some(0)
            || after.len() != 1
        {
            return Err(Error::InvalidInput);
        }
        if self.before_slot.is_none() && self.after_slot.is_none()
            || self.before_slot.is_none()
                && self.before.is_some()
                && !(operation == Operation::Forward && self.header.kind == PageKind::CatalogLeaf)
            || self.after_slot.is_none()
                && !(operation == Operation::Clr && self.header.kind == PageKind::CatalogLeaf)
        {
            return Err(Error::InvalidInput);
        }
        Ok((before, after))
    }
}

pub(super) struct FrameSpec<'a> {
    pub(super) operation: Operation,
    pub(super) profile: Profile,
    pub(super) header: WalFrameHeader,
    pub(super) catalog_bytes: usize,
    /// Tx/Aging for XHG; Tx for RollbackStart; Aging for other XSG.
    pub(super) before: [Option<&'a [u8; 1536]>; 2],
    pub(super) after: [&'a [u8; 1536]; 2],
    pub(super) core: &'a [u8; 256],
    /// Reserved after Job for Reserve, stable before RootJob for the others.
    pub(super) root_job: Option<JobView<'a>>,
    /// Actual physical order, at most eight pages. One changed cell per page
    /// in the closed operations of this scope; proof-only pages are omitted.
    pub(super) pages: &'a [PageChange<'a>],
}

fn group_header(spec: &FrameSpec<'_>, output: &mut [u8; 256]) -> Result<usize> {
    let op = spec.operation;
    let shape = op.shape(spec.catalog_bytes)?;
    let size = if op.handoff() { 256 } else { 192 };
    if spec.profile.format() != 2
        || spec.profile.spec().namespace.database != spec.header.stream.database
        || spec.profile.spec().namespace.timeline != spec.header.stream.timeline
        || spec.header.kind != op.outer()
        || spec.pages.len() != usize::from(shape.redo_count)
        || spec.header.lsn == 0
    {
        return Err(Error::IdentityMismatch);
    }
    decode_core(spec.core, op)?;
    output.fill(0);
    output[..4].copy_from_slice(if op.handoff() { b"XHG1" } else { b"XSG1" });
    put16(output, 4, 2);
    put16(output, 6, op as u16);
    put16(output, 8, size as u16);
    put16(output, 10, 2);
    put32(
        output,
        12,
        (op.frame_bytes(spec.catalog_bytes)? - 192) as u32,
    );
    put64(output, 16, spec.header.lsn);
    for ordinal in 0..if op.handoff() { 2 } else { 1 } {
        let (owner, previous, digest) = if op.handoff() {
            (40 + ordinal * 56, 24 + ordinal * 8, 152 + ordinal * 32)
        } else {
            (32, 24, 88)
        };
        output[owner..owner + 56].copy_from_slice(&spec.after[ordinal][16..72]);
        if let Some(before) = spec.before[ordinal] {
            put64(output, previous, u64_at(before, 72));
            output[digest..digest + 32].copy_from_slice(&content_digest(before));
        }
    }
    if op.handoff() {
        for (at, value) in [
            (216, 1536),
            (220, 1536),
            (224, 256),
            (228, shape.native_bytes as u32),
            (232, shape.auxiliary_bytes() as u32),
            (240, shape.redo_bytes as u32),
        ] {
            put32(output, at, value);
        }
        for (at, value) in [
            (236, shape.native_count),
            (238, shape.redo_count),
            (244, shape.edges),
            (246, shape.counts),
            (248, shape.items),
        ] {
            put16(output, at, value);
        }
    } else {
        for (at, value) in [
            (120, 1536),
            (136, shape.native_bytes as u32),
            (140, u32::from(shape.edges) * 128),
            (144, u32::from(shape.counts) * 96),
            (148, u32::from(shape.items) * 208),
            (152, shape.redo_bytes as u32),
        ] {
            put32(output, at, value);
        }
        for (at, value) in [
            (124, shape.native_count),
            (126, shape.edges),
            (128, shape.counts),
            (130, shape.items),
            (132, shape.redo_count),
        ] {
            put16(output, at, value);
        }
    }
    Ok(size)
}

struct Native<'a> {
    header: [u8; 128],
    before: &'a [u8],
    after: &'a [u8],
    prior_lsn: u64,
}
fn native<'a>(change: PageChange<'a>, operation: Operation, index: usize) -> Result<Native<'a>> {
    let (before, after) = change.views(operation)?;
    let mut native = Native {
        header: [0; 128],
        before: &[],
        after: &[],
        prior_lsn: before.as_ref().map_or(0, |view| view.header().lsn.0),
    };
    native.header[0] = change.kind()?;
    put16(&mut native.header, 2, index as u16);
    for (flag, slot_at, offset_at, length_at, digest_at, view, slot) in [
        (1, 4, 8, 12, 24, before.as_ref(), change.before_slot),
        (2, 6, 10, 16, 56, Some(&after), change.after_slot),
    ] {
        if let Some(slot) = slot {
            let view = view.ok_or(Error::IdentityMismatch)?;
            let range = view
                .cell_range(usize::from(slot))
                .ok_or(Error::IdentityMismatch)?;
            let cell = view
                .cell(usize::from(slot))
                .ok_or(Error::IdentityMismatch)?;
            native.header[1] |= flag;
            put16(&mut native.header, slot_at, slot);
            put16(&mut native.header, offset_at, range.start as u16);
            put32(&mut native.header, length_at, cell.len() as u32);
            native.header[digest_at..digest_at + 32].copy_from_slice(&content_digest(cell));
            if flag == 1 {
                native.before = cell;
            } else {
                native.after = cell;
            }
        } else {
            put16(&mut native.header, slot_at, u16::MAX);
        }
    }
    put32(
        &mut native.header,
        20,
        (128 + native.before.len() + native.after.len()) as u32,
    );
    Ok(native)
}

fn auxiliaries(spec: &FrameSpec<'_>, output: &mut [u8; 1200]) -> Result<usize> {
    let shape = spec.operation.shape(spec.catalog_bytes)?;
    let mut at = 0;
    if shape.edges != 0 {
        let index = |kind| -> Result<u16> {
            let mut selected = None;
            for (index, page) in spec.pages.iter().enumerate() {
                if page.header.kind == kind {
                    if selected.is_some() {
                        return Err(Error::IdentityMismatch);
                    }
                    selected = Some(index as u16);
                }
            }
            selected.ok_or(Error::IdentityMismatch)
        };
        let indices = EffectIndices {
            catalog: index(PageKind::CatalogLeaf)?,
            status: index(PageKind::TransactionStatus)?,
            undo: index(PageKind::Undo)?,
        };
        let (edges, rest) = output.split_at_mut(384);
        reference_changes(
            spec.operation,
            spec.root_job.ok_or(Error::IdentityMismatch)?,
            indices,
            edges.try_into().expect("fixed edges"),
            (&mut rest[..192]).try_into().expect("fixed counts"),
        )?;
        at = 576;
    }
    if shape.items != 0 {
        inventory::deltas(
            spec.operation,
            spec.root_job.ok_or(Error::IdentityMismatch)?,
            (&mut output[at..at + 624])
                .try_into()
                .expect("fixed deltas"),
        )?;
        at += 624;
    }
    if at != shape.auxiliary_bytes() {
        return Err(Error::Corrupt);
    }
    Ok(at)
}

pub(super) fn encode(spec: &FrameSpec<'_>, output: &mut [u8]) -> Result<()> {
    if output.len() != spec.operation.frame_bytes(spec.catalog_bytes)? {
        return Err(Error::InvalidInput);
    }
    output.fill(0);
    let mut group = [0; 256];
    let h = group_header(spec, &mut group)?;
    output[160..160 + h].copy_from_slice(&group[..h]);
    let mut at = 160 + h;
    for descriptor in &spec.after[..if spec.operation.handoff() { 2 } else { 1 }] {
        output[at..at + 1536].copy_from_slice(*descriptor);
        at += 1536;
    }
    output[at..at + 256].copy_from_slice(spec.core);
    at += 256;
    let native_start = at;
    for (index, change) in spec.pages.iter().copied().enumerate() {
        let native = native(change, spec.operation, index)?;
        for part in [&native.header[..], native.before, native.after] {
            let end = at.checked_add(part.len()).ok_or(Error::Corrupt)?;
            output
                .get_mut(at..end)
                .ok_or(Error::Corrupt)?
                .copy_from_slice(part);
            at = end;
        }
    }
    if at - native_start != spec.operation.shape(spec.catalog_bytes)?.native_bytes {
        return Err(Error::Corrupt);
    }
    let mut auxiliary = [0; 1200];
    let size = auxiliaries(spec, &mut auxiliary)?;
    output[at..at + size].copy_from_slice(&auxiliary[..size]);
    at += size;
    for change in spec.pages.iter().copied() {
        let size = if let Some(before) = change.before {
            let (offset, span) = patch_span(spec.operation);
            wal_frame::encode_patch_record(
                change.target(),
                u64_at(before, 64),
                offset,
                &change.after[span],
                &mut output[at..],
            )?
            .len()
        } else {
            wal_frame::encode_init_record(
                spec.header.stream.database,
                spec.header.lsn,
                change.target(),
                change.after,
                &mut output[at..],
            )?
            .len()
        };
        at += size;
    }
    if at + 32 != output.len() {
        return Err(Error::Corrupt);
    }
    if spec.operation.handoff() {
        wal_frame::finish_retention_envelope(spec.header, spec.pages.len() as u16, output)?;
    } else {
        wal_frame::finish_semantic_envelope(spec.header, spec.pages.len() as u16, output)?;
    }
    let expectation = FrameExpectation {
        stream: spec.header.stream,
        previous_lsn: spec.header.previous_lsn,
    };
    decode_layout(output, expectation, spec.profile)?;
    Ok(())
}

fn patch_span(operation: Operation) -> (u16, std::ops::Range<usize>) {
    if operation == Operation::RollbackStart {
        (16128, 16128..16352)
    } else {
        (72, 72..16352)
    }
}

/// Compare against independently computed state without a second frame-sized
/// backing. Existing framing is decoded once; each expected cell/redo is then
/// compared with its exact computed counterpart.
pub(super) fn verify(spec: &FrameSpec<'_>, bytes: &[u8]) -> Result<()> {
    let view = decode_layout(
        bytes,
        FrameExpectation {
            stream: spec.header.stream,
            previous_lsn: spec.header.previous_lsn,
        },
        spec.profile,
    )?;
    if view.header() != spec.header || view.operation() != spec.operation {
        return Err(Error::IdentityMismatch);
    }
    let mut group = [0; 256];
    let h = group_header(spec, &mut group)?;
    if view.group() != &group[..h] || view.core().bytes() != spec.core {
        return Err(Error::Corrupt);
    }
    for ordinal in 0..if spec.operation.handoff() { 2 } else { 1 } {
        if view.descriptor(ordinal) != Some(spec.after[ordinal]) {
            return Err(Error::Corrupt);
        }
    }
    let mut observed_native = view.native();
    let mut observed_redo = view.redo();
    for (index, change) in spec.pages.iter().copied().enumerate() {
        let expected = native(change, spec.operation, index)?;
        for part in [&expected.header[..], expected.before, expected.after] {
            if !observed_native.starts_with(part) {
                return Err(Error::Corrupt);
            }
            observed_native = &observed_native[part.len()..];
        }
        let (kind, offset, data) = if change.before.is_some() {
            let (offset, span) = patch_span(spec.operation);
            (RedoKind::Patch, offset, &change.after[span])
        } else {
            (RedoKind::Init, 0, &change.after[..])
        };
        let length = 64 + data.len();
        let raw = observed_redo.get(..length).ok_or(Error::Corrupt)?;
        let redo = wal_frame::decode_redo_record(spec.header, raw)?;
        if redo.kind != kind
            || redo.target != change.target()
            || redo.prior_lsn != expected.prior_lsn
            || redo.offset != offset
            || redo.data != data
        {
            return Err(Error::Corrupt);
        }
        observed_redo = &observed_redo[length..];
    }
    if !observed_native.is_empty() || !observed_redo.is_empty() {
        return Err(Error::Corrupt);
    }
    let mut auxiliary = [0; 1200];
    let size = auxiliaries(spec, &mut auxiliary)?;
    let mut at = 0;
    for actual in [view.edges(), view.counts(), view.items()] {
        if actual != &auxiliary[at..at + actual.len()] {
            return Err(Error::Corrupt);
        }
        at += actual.len();
    }
    if at != size {
        return Err(Error::Corrupt);
    }
    Ok(())
}
