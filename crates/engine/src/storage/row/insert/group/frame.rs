//! The scope3 XHG1 Forward/CLR frame body around already produced after
//! images and descriptors. The tag3 envelope is finished last and the whole
//! frame is reread by the shared layout decoder before success is reported.

use super::{InsertError, Observed, Result, projection, require, undo_item};
use crate::storage::{
    disk::RowPageRole,
    page::{self, PAGE_BYTES},
    semantic_row::{Operation, decode_layout},
    tree_page,
    undo_record::UndoLink,
    wal_frame::{self, FrameExpectation, RedoTarget},
};
use foundation::codec::content_digest;

const CORE_AT: usize = 3_488;
const NATIVE_AT: usize = 3_744;
/// Edge roles of this group: Row undo/stamp, U.prevTxn and Status head.
const ROW_UNDO: u8 = 1;
const UNDO_PREVIOUS_TRANSACTION: u8 = 4;
const ROW_STAMP: u8 = 7;
const TRANSACTION_HEAD: u8 = 10;

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Undo(UndoLink),
    Transaction,
}

fn encode_target(observed: &Observed<'_>, target: Target, output: &mut [u8]) -> Result<()> {
    match target {
        Target::Undo(link) => {
            output[0] = 1;
            link.reference.encode(&mut output[8..48])?;
            put64(output, 48, link.sequence);
        }
        Target::Transaction => {
            output[0] = 2;
            observed.actor.encode(&mut output[8..56])?;
        }
    }
    Ok(())
}

fn native_index(observed: &Observed<'_>, role: RowPageRole) -> u16 {
    observed
        .pages
        .iter()
        .position(|page| page.role == role)
        .expect("five distinct roles") as u16
}

/// Slot of the inserted/removed row in one RowLeaf image (slot0 is metadata).
fn leaf_slot(observed: &Observed<'_>, bytes: &[u8; PAGE_BYTES]) -> Result<u16> {
    let view = tree_page::decode_tree_page(bytes, observed.leaf)?;
    let position = view.lower_bound(&observed.row.get().to_be_bytes())?;
    require(position.equal, InsertError::IdentityMismatch)?;
    Ok((position.index + 1) as u16)
}

/// One native record: 128-byte header, then the present before/after cells.
fn native(
    observed: &Observed<'_>,
    index: usize,
    after: &[u8; PAGE_BYTES],
    output: &mut [u8],
) -> Result<usize> {
    let page = observed.pages[index];
    let (kind, before_slot, after_slot) = match page.role {
        RowPageRole::Root => (8, Some(0), Some(0)),
        RowPageRole::RootJob => {
            let slot = (observed.ordinal % 15) as u16;
            (9, Some(slot), Some(slot))
        }
        RowPageRole::Status => (5, Some(0), Some(0)),
        RowPageRole::Undo => (3, Some(0), Some(0)),
        RowPageRole::RowLeaf if observed.forward => (1, None, Some(leaf_slot(observed, after)?)),
        RowPageRole::RowLeaf => (1, Some(leaf_slot(observed, page.before)?), None),
    };
    output[0] = kind;
    put16(output, 2, index as u16);
    let mut length = 128;
    for (bit, slot_at, offset_at, length_at, digest_at, bytes, slot) in [
        (1, 4, 8, 12, 24, page.before, before_slot),
        (2, 6, 10, 16, 56, after, after_slot),
    ] {
        let Some(slot) = slot else {
            put16(output, slot_at, u16::MAX);
            continue;
        };
        let view = page::decode_page(bytes, page.identity, page.kind)?;
        let range = view
            .cell_range(usize::from(slot))
            .ok_or(InsertError::IdentityMismatch)?;
        let cell = &bytes[range.clone()];
        output[1] |= bit;
        put16(output, slot_at, slot);
        put16(output, offset_at, range.start as u16);
        put32(output, length_at, cell.len() as u32);
        output[digest_at..digest_at + 32].copy_from_slice(&content_digest(cell));
        output
            .get_mut(length..length + cell.len())
            .ok_or(InsertError::Capacity)?
            .copy_from_slice(cell);
        length += cell.len();
    }
    put32(output, 20, length as u32);
    Ok(length)
}

fn core(observed: &Observed<'_>, output: &mut [u8]) -> Result<()> {
    let job = observed.job;
    let h = observed.predecessor.is_some();
    let flags = 1 | (u16::from(h) << 1) | (u16::from(observed.target.is_some()) << 2) | 24;
    let operation = if observed.forward {
        Operation::Forward
    } else {
        Operation::Clr
    };
    put16(output, 0, operation as u16);
    put16(output, 2, flags);
    put32(output, 4, 256);
    put64(output, 8, job.statement());
    put64(output, 16, job.mutation());
    put64(output, 24, observed.undo.sequence);
    observed.undo.reference.encode(&mut output[32..72])?;
    if let Some(link) = observed.predecessor {
        link.reference.encode(&mut output[72..112])?;
    }
    if let Some(target) = observed.target {
        target.encode(&mut output[112..152])?;
    }
    let missing = projection(observed.table, observed.row, None)?;
    let present = job.projection_digest();
    let (before, after) = if observed.forward {
        (&missing, present)
    } else {
        (present, &missing)
    };
    output[176..208].copy_from_slice(before);
    output[208..240].copy_from_slice(after);
    Ok(())
}

/// Write the complete frame. `output` is exactly the zeroed frame length with
/// both descriptor after images already present.
pub(super) fn write(
    observed: &Observed<'_>,
    after: &[[u8; PAGE_BYTES]; 5],
    output: &mut [u8],
) -> Result<()> {
    let h = observed.predecessor.is_some();
    let operation = if observed.forward {
        Operation::Forward
    } else {
        Operation::Clr
    };
    let shape = operation.shape(observed.cell, h)?;
    core(observed, &mut output[CORE_AT..NATIVE_AT])?;

    let mut at = NATIVE_AT;
    for (index, image) in after.iter().enumerate() {
        at += native(observed, index, image, &mut output[at..])?;
    }
    require(
        at - NATIVE_AT == shape.native_bytes,
        InsertError::IdentityMismatch,
    )?;
    let natives = at - NATIVE_AT;

    // Edges sorted by (native index, role); counts are U then T.
    let u = Target::Undo(observed.undo);
    let previous = observed.predecessor.map(Target::Undo);
    let status = native_index(observed, RowPageRole::Status);
    let undo = native_index(observed, RowPageRole::Undo);
    let leaf = native_index(observed, RowPageRole::RowLeaf);
    let present = |value| {
        if observed.forward {
            (None, value)
        } else {
            (value, None)
        }
    };
    let mut edges = [(0u16, 0u8, None, None); 4];
    let head = if observed.forward {
        (previous, Some(u))
    } else {
        (Some(u), previous)
    };
    edges[0] = (status, TRANSACTION_HEAD, head.0, head.1);
    let (old, new) = present(Some(u));
    edges[1] = (leaf, ROW_UNDO, old, new);
    let (old, new) = present(Some(Target::Transaction));
    edges[2] = (leaf, ROW_STAMP, old, new);
    let count = if let Some(previous) = previous {
        let (old, new) = present(Some(previous));
        edges[3] = (undo, UNDO_PREVIOUS_TRANSACTION, old, new);
        4
    } else {
        3
    };
    require(
        count == usize::from(shape.edges),
        InsertError::IdentityMismatch,
    )?;
    edges[..count].sort_unstable_by_key(|edge| (edge.0, edge.1));
    let edges_at = at;
    for (index, role, old, new) in edges[..count].iter().copied() {
        let edge = &mut output[at..at + 128];
        put16(edge, 0, index);
        edge[2] = role;
        if let Some(old) = old {
            encode_target(observed, old, &mut edge[8..64])?;
        }
        if let Some(new) = new {
            encode_target(observed, new, &mut edge[64..120])?;
        }
        at += 128;
    }
    let incoming = observed.status.incoming();
    let counts = [
        (u, if observed.forward { (0, 2) } else { (2, 0) }, undo),
        (
            Target::Transaction,
            if observed.forward {
                (incoming, incoming + 1)
            } else {
                (incoming, incoming - 1)
            },
            status,
        ),
    ];
    for (target, (before, after), index) in counts {
        let count = &mut output[at..at + 96];
        encode_target(observed, target, &mut count[..56])?;
        put64(count, 56, before);
        put64(count, 64, after);
        put16(count, 72, index);
        at += 96;
    }
    if observed.forward {
        let delta = &mut output[at..at + 208];
        delta[0] = 2;
        delta[1] = 1;
        delta[16..112].copy_from_slice(&undo_item(observed.job)?);
        at += 208;
    }
    let auxiliary = at - edges_at;
    require(
        auxiliary
            == usize::from(shape.edges) * 128
                + usize::from(shape.counts) * 96
                + usize::from(shape.items) * 208,
        InsertError::IdentityMismatch,
    )?;

    let redo_at = at;
    for (page, image) in observed.pages.iter().zip(after) {
        let target = RedoTarget {
            page: page.identity.page,
            owner: page.identity.owner,
            owner_generation: page.identity.owner_generation,
            kind: page.kind,
            level: 0,
        };
        let record_bytes = page::DIGEST_OFFSET - 72;
        let written = wal_frame::encode_patch_record(
            target,
            page.lsn,
            72,
            &image[72..page::DIGEST_OFFSET],
            output
                .get_mut(at..at + 64 + record_bytes)
                .ok_or(InsertError::Capacity)?,
        )?
        .len();
        at += written;
    }
    let redo = at - redo_at;
    require(
        redo == shape.redo_bytes && at + 32 == observed.frame_len,
        InsertError::IdentityMismatch,
    )?;

    // XHG1 group header.
    let length = observed.frame_len;
    let group = &mut output[160..416];
    group[..4].copy_from_slice(b"XHG1");
    put16(group, 4, 2);
    put16(group, 6, operation as u16);
    put16(group, 8, 256);
    put16(group, 10, 3);
    put32(group, 12, (length - 192) as u32);
    put64(group, 16, observed.header.lsn);
    put64(group, 24, observed.transaction.chain.apply);
    group[32..40].copy_from_slice(&observed.aging[72..80]);
    group[152..184].copy_from_slice(&content_digest(observed.transaction_bytes));
    group[184..216].copy_from_slice(&content_digest(observed.aging));
    put32(group, 216, 1536);
    put32(group, 220, 1536);
    put32(group, 224, 256);
    put32(group, 228, natives as u32);
    put32(group, 232, auxiliary as u32);
    put16(group, 236, shape.native_count);
    put16(group, 238, shape.redo_count);
    put32(group, 240, redo as u32);
    put16(group, 244, shape.edges);
    put16(group, 246, shape.counts);
    put16(group, 248, shape.items);
    let (owners, descriptors) = output.split_at_mut(416);
    owners[200..256].copy_from_slice(&descriptors[16..72]);
    owners[256..312].copy_from_slice(&descriptors[1536 + 16..1536 + 72]);

    wal_frame::finish_retention_envelope(observed.header, shape.redo_count, output)?;
    let layout = decode_layout(
        output,
        FrameExpectation {
            stream: observed.header.stream,
            previous_lsn: observed.header.previous_lsn,
        },
        observed.profile,
    )?;
    require(
        layout.header() == observed.header && layout.operation() == operation,
        InsertError::IdentityMismatch,
    )?;
    Ok(())
}

pub(super) fn local_source_carrier_bytes() -> usize {
    use std::mem::size_of;
    size_of::<[(u16, u8, Option<Target>, Option<Target>); 4]>()
        + size_of::<[(Target, (u64, u64), u16); 2]>()
        + size_of::<crate::storage::semantic_row::Shape>()
        + size_of::<crate::storage::semantic_row::LayoutView<'_>>()
        + size_of::<page::PageView<'_>>()
        + size_of::<tree_page::TreePageView<'_>>()
        + size_of::<RedoTarget>()
        + size_of::<FrameExpectation>()
        + 3 * size_of::<[u8; 32]>()
        + 16 * size_of::<usize>()
}
