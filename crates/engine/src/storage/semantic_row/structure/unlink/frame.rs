//! Scope4 opcode 3/4 production and canonical byte comparison in one
//! borrowed call, like the split frame: both entry points recompute the
//! candidate from the original cut; no previous Candidate or after image is
//! input. Unchanged natives carry before = after digests and no redo.
use super::{
    super::{
        Error, Operation, Result,
        candidate::Inputs,
        frame::{Cursor, Sink, finished_descriptor, put, target},
    },
    ROLE_ANCHOR, ROLE_LEAF, ROLE_PARENT, ROLE_RETIRED_PARENT, ROLE_SURVIVOR, UNCHANGED,
    candidate::{self, ANCHOR, Candidate, LEAF, PARENT, Prepared, SURVIVOR},
};
use crate::storage::{
    page::{PAGE_BYTES, PageHeader, PageRef},
    wal_frame::{self, WalFrameHeader},
};
use foundation::codec::content_digest;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Observation {
    pub(crate) header: WalFrameHeader,
    pub(crate) operation: Operation,
    pub(crate) frame_bytes: usize,
    /// Candidate slots ANCHOR, PARENT, LEAF, SURVIVOR (LeafUnlink repeats
    /// the leaf in SURVIVOR). Changed pages carry the new LSN.
    pub(crate) targets: [PageHeader; 4],
}
impl Observation {
    fn from(candidate: &Candidate<'_>) -> Self {
        Self {
            header: candidate.header,
            operation: candidate.operation,
            frame_bytes: candidate.operation.frame_bytes(),
            targets: candidate.pages,
        }
    }
}

/// `NotNeeded` writes nothing; the tree already is a valid state.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "the fixed source census counts the inline observation, without heap allocation"
)]
pub(crate) enum Produced {
    Frame(Observation),
    NotNeeded,
}

/// (native role, candidate slot) of the opcode, in role order.
fn roles(operation: Operation) -> &'static [(u8, usize)] {
    if operation == Operation::RootCollapse {
        &[
            (ROLE_ANCHOR, ANCHOR),
            (ROLE_RETIRED_PARENT, PARENT),
            (ROLE_LEAF, LEAF),
            (ROLE_SURVIVOR, SURVIVOR),
        ]
    } else {
        &[
            (ROLE_ANCHOR, ANCHOR),
            (ROLE_PARENT, PARENT),
            (ROLE_LEAF, LEAF),
        ]
    }
}

fn prior(before: &[u8; PAGE_BYTES]) -> u64 {
    u64::from_le_bytes(before[64..72].try_into().expect("page LSN"))
}

/// Physical (file, page_no) order of the opcode's natives.
fn order(candidate: &Candidate<'_>) -> Result<[(u8, usize); 4]> {
    let roles = roles(candidate.operation);
    let mut ranked = [(0u64, 0u64, 0u8, 0usize); 4];
    for (item, (role, slot)) in ranked.iter_mut().zip(roles) {
        let mut page = [0; 24];
        candidate.pages[*slot].identity.page.encode(&mut page)?;
        *item = (
            u64::from_le_bytes(page[..8].try_into().expect("file")),
            candidate.pages[*slot].identity.page.page_no,
            *role,
            *slot,
        );
    }
    let ranked = &mut ranked[..roles.len()];
    ranked.sort_unstable();
    for pair in ranked.windows(2) {
        if pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1 {
            return Err(Error::IdentityMismatch);
        }
    }
    let mut out = [(0, 0); 4];
    for (out, item) in out.iter_mut().zip(ranked.iter()) {
        *out = (item.2, item.3);
    }
    Ok(out)
}

fn after_of<'x>(
    candidate: &'x Candidate<'_>,
    after: &'x [[u8; PAGE_BYTES]; 2],
    role: u8,
    slot: usize,
) -> &'x [u8; PAGE_BYTES] {
    if role == ROLE_ANCHOR {
        &after[0]
    } else if role == ROLE_PARENT {
        &after[1]
    } else {
        candidate.before[slot]
    }
}

fn body(
    input: &Inputs<'_>,
    candidate: &Candidate<'_>,
    after: &[[u8; PAGE_BYTES]; 2],
    cursor: &mut Cursor<'_>,
) -> Result<()> {
    let operation = candidate.operation;
    let bytes = operation.frame_bytes();
    let natives = operation.natives();
    let mut desc = [0; 1536];
    finished_descriptor(input.profile, candidate.header, bytes, &mut desc)?;
    let mut group = [0; 192];
    group[..4].copy_from_slice(b"XSG1");
    for (at, value, width) in [
        (4, 2, 2),
        (6, operation as u64, 2),
        (8, 192, 2),
        (10, 4, 2),
        (12, (bytes - 192) as u64, 4),
        (16, candidate.header.lsn, 8),
        (120, 1536, 4),
        (124, natives as u64, 2),
        (132, operation.redo_records() as u64, 2),
        (136, (natives * 128) as u64, 4),
        (152, (bytes - operation.redo_at() - 32) as u64, 4),
    ] {
        put(&mut group, at, value, width);
    }
    group[32..88].copy_from_slice(&desc[16..72]);
    cursor.part(&group)?;
    cursor.part(&desc)?;
    cursor.part(&candidate.core)?;
    let order = order(candidate)?;
    let mut ordinal = 0u16;
    for (role, slot) in order[..natives].iter().copied() {
        let header = candidate.pages[slot];
        let before = candidate.before[slot];
        let mut native = [0; 128];
        native[0] = role;
        let changed = operation.changes(role);
        put(
            &mut native,
            2,
            u64::from(if changed { ordinal } else { UNCHANGED }),
            2,
        );
        ordinal += u16::from(changed);
        native[4] = header.kind as u8;
        native[5] = header.level;
        header.identity.page.encode(&mut native[8..32])?;
        put(&mut native, 32, header.identity.owner.get(), 8);
        put(&mut native, 40, header.identity.owner_generation.get(), 8);
        put(&mut native, 48, prior(before), 8);
        native[56..88].copy_from_slice(&content_digest(before));
        native[88..120].copy_from_slice(&content_digest(after_of(candidate, after, role, slot)));
        cursor.part(&native)?;
    }
    for (role, slot) in order[..natives].iter().copied() {
        if operation.changes(role) {
            cursor.patch(
                candidate.header,
                target(candidate.pages[slot]),
                prior(candidate.before[slot]),
                after_of(candidate, after, role, slot),
            )?;
        }
    }
    if cursor.at + 32 != bytes {
        return Err(Error::Corrupt);
    }
    Ok(())
}

/// Produce the exact used prefix of a needed unlink. Output beyond the frame
/// is untouched; both buffers are workspace on failure. This is still no
/// WAL/install authority.
pub(crate) fn produce<'a>(
    input: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 2],
    output: &mut [u8],
    image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Produced> {
    let Prepared::Unlink(candidate) = candidate::prepare(input, after, image)? else {
        return Ok(Produced::NotNeeded);
    };
    let operation = candidate.operation;
    let out = output
        .get_mut(..operation.frame_bytes())
        .ok_or(Error::Exhausted)?;
    out.fill(0);
    body(
        input,
        &candidate,
        after,
        &mut Cursor {
            sink: Sink::Write(out),
            at: 160,
        },
    )?;
    wal_frame::finish_semantic_envelope(candidate.header, operation.redo_records() as u16, out)?;
    let view = super::decode_layout(out, input.frame, input.profile)?;
    if view.header() != candidate.header || view.operation() != operation {
        return Err(Error::Corrupt);
    }
    Ok(Produced::Frame(Observation::from(&candidate)))
}

/// Recompute from the original cut and compare every group, descriptor,
/// core, native and redo byte. A frame for a tree that needs no unlink is
/// `IdentityMismatch`; the two after images are output workspace.
pub(crate) fn observe<'a>(
    input: &Inputs<'_>,
    frame: &[u8],
    after: &mut [[u8; PAGE_BYTES]; 2],
    image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Observation> {
    let view = super::decode_layout(frame, input.frame, input.profile)?;
    let Prepared::Unlink(candidate) = candidate::prepare(input, after, image)? else {
        return Err(Error::IdentityMismatch);
    };
    if view.header() != candidate.header || view.operation() != candidate.operation {
        return Err(Error::IdentityMismatch);
    }
    body(
        input,
        &candidate,
        after,
        &mut Cursor {
            sink: Sink::Compare(frame),
            at: 160,
        },
    )?;
    Ok(Observation::from(&candidate))
}
