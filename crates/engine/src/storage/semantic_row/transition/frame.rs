//! Scope3 XHG1/XSG1 framing of one prepared Row group (wire.rs layout,
//! create-group-format.md §body order). One emitter serves both directions:
//! the producer writes each section, the observer compares the same computed
//! section against the candidate without a second frame-sized backing.
use super::super::{Error, LayoutView, Result, decode_layout};
use super::{
    Inputs, Prepared,
    cells::{put, put16, put32},
};
use crate::storage::{
    page::{self, DIGEST_OFFSET, PAGE_BYTES, PageKind},
    wal_frame::{self, FrameExpectation, RedoKind, RedoTarget, WalFrameHeader},
};
use foundation::codec::content_digest;

/// Present roles sorted by the physical `(file, page_no)` WAL order.
pub(super) fn physical_order(prepared: &Prepared<'_>) -> Result<[Option<usize>; 5]> {
    let mut ranked = [(0u64, 0u64, 0usize); 5];
    let mut count = 0;
    for (role, page) in prepared.pages.iter().enumerate() {
        let Some(page) = page else {
            continue;
        };
        let mut reference = [0; 24];
        page.identity.page.encode(&mut reference)?;
        ranked[count] = (
            u64::from_le_bytes(reference[..8].try_into().expect("file")),
            page.identity.page.page_no,
            role,
        );
        count += 1;
    }
    ranked[..count].sort_unstable();
    let mut order = [None; 5];
    for (at, (file, page_no, role)) in ranked[..count].iter().copied().enumerate() {
        if at > 0 && (ranked[at - 1].0, ranked[at - 1].1) == (file, page_no) {
            return Err(Error::IdentityMismatch);
        }
        order[at] = Some(role);
    }
    Ok(order)
}

/// Producer: write; observer: compare each computed section in place.
enum Sink<'o> {
    Write(&'o mut [u8]),
    Compare(&'o [u8]),
}
struct Cursor<'o> {
    sink: Sink<'o>,
    at: usize,
}
impl Cursor<'_> {
    fn part(&mut self, part: &[u8]) -> Result<()> {
        let end = self.at.checked_add(part.len()).ok_or(Error::Corrupt)?;
        match &mut self.sink {
            Sink::Write(output) => output
                .get_mut(self.at..end)
                .ok_or(Error::Corrupt)?
                .copy_from_slice(part),
            Sink::Compare(bytes) => {
                if bytes.get(self.at..end) != Some(part) {
                    return Err(Error::Corrupt);
                }
            }
        }
        self.at = end;
        Ok(())
    }
    /// One Patch record; compared decoded, like the Catalog verifier.
    fn patch(
        &mut self,
        header: WalFrameHeader,
        target: RedoTarget,
        prior: u64,
        offset: u16,
        data: &[u8],
    ) -> Result<()> {
        let length = 64 + data.len();
        let end = self.at.checked_add(length).ok_or(Error::Corrupt)?;
        match &mut self.sink {
            Sink::Write(output) => {
                let written = wal_frame::encode_patch_record(
                    target,
                    prior,
                    offset,
                    data,
                    output.get_mut(self.at..end).ok_or(Error::Corrupt)?,
                )?
                .len();
                if written != length {
                    return Err(Error::Corrupt);
                }
            }
            Sink::Compare(bytes) => {
                let raw = bytes.get(self.at..end).ok_or(Error::Corrupt)?;
                if u32::from_le_bytes(raw[4..8].try_into().expect("length")) as usize != length {
                    return Err(Error::Corrupt);
                }
                let redo = wal_frame::decode_redo_record(header, raw)?;
                if redo.kind != RedoKind::Patch
                    || redo.target != target
                    || redo.prior_lsn != prior
                    || redo.offset != offset
                    || redo.data != data
                {
                    return Err(Error::Corrupt);
                }
            }
        }
        self.at = end;
        Ok(())
    }
}
fn native_kind(kind: PageKind) -> Result<u8> {
    Ok(match kind {
        PageKind::RowLeaf => 1,
        PageKind::Undo => 3,
        PageKind::TransactionStatus => 5,
        PageKind::RetentionRoot => 8,
        PageKind::RetentionJobs => 9,
        _ => return Err(Error::Unsupported),
    })
}

/// The group header fields fixed by the owners, shape and descriptors.
fn group_header(prepared: &Prepared<'_>, output: &mut [u8; 256]) -> Result<usize> {
    let op = prepared.op;
    let shape = op.shape(prepared.row_cell, prepared.predecessor)?;
    let size = if op.handoff() { 256 } else { 192 };
    output.fill(0);
    output[..4].copy_from_slice(if op.handoff() { b"XHG1" } else { b"XSG1" });
    put16(output, 4, 2);
    put16(output, 6, op as u16);
    put16(output, 8, size as u16);
    put16(output, 10, 3);
    put32(output, 12, (prepared.frame_bytes - 192) as u32);
    put(output, 16, prepared.header.lsn);
    // XHG: Tx (absent before Begin) then Aging; XSG: the single owner.
    let owners: [Option<&[u8; 1536]>; 2] = if op.handoff() {
        [prepared.transaction_before, prepared.aging_before]
    } else if op.status_patch() {
        [prepared.transaction_before, None]
    } else {
        [prepared.aging_before, None]
    };
    for ordinal in 0..if op.handoff() { 2 } else { 1 } {
        let (owner, previous, digest) = if op.handoff() {
            (40 + ordinal * 56, 24 + ordinal * 8, 152 + ordinal * 32)
        } else {
            (32, 24, 88)
        };
        output[owner..owner + 56].copy_from_slice(&prepared.descriptors[ordinal][16..72]);
        if let Some(before) = owners[ordinal] {
            output[previous..previous + 8].copy_from_slice(&before[72..80]);
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

/// Every section after the XWF1 header, in body order, through one cursor.
fn body(
    prepared: &Prepared<'_>,
    after: &[[u8; PAGE_BYTES]; 5],
    cursor: &mut Cursor<'_>,
) -> Result<()> {
    let op = prepared.op;
    let shape = op.shape(prepared.row_cell, prepared.predecessor)?;
    let mut group = [0; 256];
    let size = group_header(prepared, &mut group)?;
    cursor.part(&group[..size])?;
    for descriptor in &prepared.descriptors[..if op.handoff() { 2 } else { 1 }] {
        cursor.part(descriptor)?;
    }
    cursor.part(&prepared.core)?;
    let order = physical_order(prepared)?;
    let natives = cursor.at;
    for (index, role) in order.iter().enumerate() {
        let Some(role) = *role else {
            continue;
        };
        let page = prepared.pages[role].ok_or(Error::Corrupt)?;
        let mut header = [0u8; 128];
        header[0] = native_kind(page.kind)?;
        put16(&mut header, 2, index as u16);
        let mut cells: [&[u8]; 2] = [&[], &[]];
        for (side, (bit, slot_at, offset_at, length_at, digest_at, bytes, slot)) in [
            (1, 4, 8, 12, 24, page.before, page.before_slot),
            (2, 6, 10, 16, 56, &after[role], page.after_slot),
        ]
        .into_iter()
        .enumerate()
        {
            let Some(slot) = slot else {
                put16(&mut header, slot_at, u16::MAX);
                continue;
            };
            let view = page::decode_page(bytes, page.identity, page.kind)?;
            let range = view
                .cell_range(usize::from(slot))
                .ok_or(Error::IdentityMismatch)?;
            let cell = &bytes[range.clone()];
            header[1] |= bit;
            put16(&mut header, slot_at, slot);
            put16(&mut header, offset_at, range.start as u16);
            put32(&mut header, length_at, cell.len() as u32);
            header[digest_at..digest_at + 32].copy_from_slice(&content_digest(cell));
            cells[side] = cell;
        }
        put32(
            &mut header,
            20,
            (128 + cells[0].len() + cells[1].len()) as u32,
        );
        cursor.part(&header)?;
        cursor.part(cells[0])?;
        cursor.part(cells[1])?;
    }
    if cursor.at - natives != shape.native_bytes {
        return Err(Error::Corrupt);
    }
    if prepared.aux.len != shape.auxiliary_bytes() {
        return Err(Error::Corrupt);
    }
    cursor.part(prepared.aux.bytes())?;
    let redo = cursor.at;
    let (offset, span) = if op.status_patch() {
        (16128, 16128..DIGEST_OFFSET)
    } else {
        (72, 72..DIGEST_OFFSET)
    };
    for role in order.into_iter().flatten() {
        let page = prepared.pages[role].ok_or(Error::Corrupt)?;
        cursor.patch(
            prepared.header,
            RedoTarget {
                page: page.identity.page,
                owner: page.identity.owner,
                owner_generation: page.identity.owner_generation,
                kind: page.kind,
                level: 0,
            },
            super::cells::page_lsn(page.before),
            offset,
            &after[role][span.clone()],
        )?;
    }
    if cursor.at - redo != shape.redo_bytes || cursor.at + 32 != prepared.frame_bytes {
        return Err(Error::Corrupt);
    }
    Ok(())
}

/// Write the complete frame into `output` (exactly `frame_bytes`), finish the
/// envelope last and reread it through the shared layout decoder.
pub(super) fn encode(
    inputs: &Inputs<'_>,
    prepared: &Prepared<'_>,
    after: &[[u8; PAGE_BYTES]; 5],
    output: &mut [u8],
) -> Result<()> {
    if output.len() != prepared.frame_bytes {
        return Err(Error::InvalidInput);
    }
    output.fill(0);
    let mut cursor = Cursor {
        sink: Sink::Write(output),
        at: 160,
    };
    body(prepared, after, &mut cursor)?;
    let count = prepared
        .op
        .shape(prepared.row_cell, prepared.predecessor)?
        .redo_count;
    if prepared.op.handoff() {
        wal_frame::finish_retention_envelope(prepared.header, count, output)?;
    } else {
        wal_frame::finish_semantic_envelope(prepared.header, count, output)?;
    }
    let layout = decode_layout(output, inputs.frame, inputs.profile)?;
    if layout.header() != prepared.header || layout.operation() != prepared.op {
        return Err(Error::Corrupt);
    }
    Ok(())
}

/// Compare a decoded candidate with the computed group, section by section.
pub(super) fn verify(
    _inputs: &Inputs<'_>,
    prepared: &Prepared<'_>,
    after: &[[u8; PAGE_BYTES]; 5],
    layout: LayoutView<'_>,
) -> Result<()> {
    if layout.header() != prepared.header
        || layout.operation() != prepared.op
        || layout.bytes().len() != prepared.frame_bytes
    {
        return Err(Error::Corrupt);
    }
    let mut cursor = Cursor {
        sink: Sink::Compare(layout.bytes()),
        at: 160,
    };
    body(prepared, after, &mut cursor)
}

/// Framing's own frame: group/native scratch, ordering and the decoded views.
pub(super) fn locals_bytes() -> usize {
    use std::mem::size_of;
    256 // group header
        + 128 // native header
        + 2 * size_of::<[u8; 32]>()
        + size_of::<[&[u8]; 2]>()
        + size_of::<[Option<usize>; 5]>()
        + size_of::<[(u64, u64, usize); 5]>()
        + size_of::<[u8; 24]>()
        + 2 * size_of::<page::PageView<'_>>()
        + size_of::<Cursor<'_>>()
        + size_of::<RedoTarget>()
        + size_of::<wal_frame::RedoRecordView<'_>>()
        + size_of::<wal_frame::WalFrameHeader>()
        + size_of::<crate::storage::semantic_row::Shape>()
        + size_of::<LayoutView<'_>>()
        + size_of::<FrameExpectation>()
        + 24 * size_of::<usize>()
}

/// Framing's locals with the standard slack, its counted deepest callee
/// (layout decode, envelope finish or a native page view) and the SHA
/// closure of the frame/native digests.
pub(super) fn source_bytes() -> usize {
    locals_bytes() + super::source::slack() + super::source::frame_callee() + super::source::SHA
}
