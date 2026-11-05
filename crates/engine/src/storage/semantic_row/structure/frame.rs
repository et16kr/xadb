//! Scope4 production and canonical byte comparison in one borrowed call.
//! A previous Candidate is never accepted: both entry points recompute from
//! the original cut and privately borrow the resulting after images through
//! emission. No mutable output can change between those two steps.
use super::{
    Error, Operation, Result,
    candidate::{self, Candidate, Inputs},
};
use crate::storage::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    obligation::{
        self, DescriptorSpec, InventoryClaim, OwnerChain, OwnerState, RemainingBudget,
        SystemProgress, WorkPhase,
    },
    page::{DIGEST_OFFSET, PAGE_BYTES, PageHeader, PageRef},
    retention_obligation::profile_reference,
    wal_frame::{self, RedoKind, RedoTarget, WalFrameHeader},
};
use foundation::{codec::content_digest, identity::Generation};

#[derive(Clone, Copy)]
pub(crate) struct Observation {
    pub(crate) header: WalFrameHeader,
    pub(crate) operation: Operation,
    pub(crate) frame_bytes: usize,
    pub(crate) targets: [PageHeader; 5],
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
pub(super) fn put(bytes: &mut [u8], at: usize, value: u64, width: usize) {
    bytes[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}
pub(super) fn target(header: PageHeader) -> RedoTarget {
    RedoTarget {
        page: header.identity.page,
        owner: header.identity.owner,
        owner_generation: header.identity.owner_generation,
        kind: header.kind,
        level: header.level,
    }
}
fn prior(candidate: &Candidate<'_>, role: usize) -> u64 {
    candidate.before[role].map_or(0, |p| {
        u64::from_le_bytes(p[64..72].try_into().expect("page LSN"))
    })
}
fn order(candidate: &Candidate<'_>) -> Result<[usize; 5]> {
    let mut ranked = [(0u64, 0u64, 0usize); 5];
    for (role, item) in ranked.iter_mut().enumerate() {
        let mut page = [0; 24];
        candidate.pages[role].identity.page.encode(&mut page)?;
        *item = (
            u64::from_le_bytes(page[..8].try_into().expect("file")),
            candidate.pages[role].identity.page.page_no,
            role,
        );
    }
    ranked.sort_unstable();
    for pair in ranked.windows(2) {
        if pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1 {
            return Err(Error::IdentityMismatch);
        }
    }
    Ok(ranked.map(|(_, _, role)| role))
}

pub(super) enum Sink<'a> {
    Write(&'a mut [u8]),
    Compare(&'a [u8]),
}
pub(super) struct Cursor<'a> {
    pub(super) sink: Sink<'a>,
    pub(super) at: usize,
}
impl Cursor<'_> {
    pub(super) fn part(&mut self, value: &[u8]) -> Result<()> {
        let end = self.at.checked_add(value.len()).ok_or(Error::Exhausted)?;
        match &mut self.sink {
            Sink::Write(out) => out
                .get_mut(self.at..end)
                .ok_or(Error::Exhausted)?
                .copy_from_slice(value),
            Sink::Compare(input) => {
                if input.get(self.at..end) != Some(value) {
                    return Err(Error::Corrupt);
                }
            }
        }
        self.at = end;
        Ok(())
    }
    fn redo(
        &mut self,
        candidate: &Candidate<'_>,
        role: usize,
        after: &[u8; PAGE_BYTES],
    ) -> Result<()> {
        let init = candidate.operation.init(role as u8);
        let data = if init {
            after.as_slice()
        } else {
            &after[72..DIGEST_OFFSET]
        };
        let expected = target(candidate.pages[role]);
        let end = self
            .at
            .checked_add(64 + data.len())
            .ok_or(Error::Exhausted)?;
        match &mut self.sink {
            Sink::Write(out) => {
                let out = out.get_mut(self.at..end).ok_or(Error::Exhausted)?;
                if init {
                    wal_frame::encode_init_record(
                        candidate.header.stream.database,
                        candidate.header.lsn,
                        expected,
                        after,
                        out,
                    )?;
                } else {
                    wal_frame::encode_patch_record(
                        expected,
                        prior(candidate, role),
                        72,
                        data,
                        out,
                    )?;
                }
            }
            Sink::Compare(input) => {
                let bytes = input.get(self.at..end).ok_or(Error::Corrupt)?;
                let record = wal_frame::decode_redo_record(candidate.header, bytes)?;
                if record.target != expected
                    || record.kind
                        != if init {
                            RedoKind::Init
                        } else {
                            RedoKind::Patch
                        }
                    || record.prior_lsn != prior(candidate, role)
                    || record.offset != if init { 0 } else { 72 }
                    || record.data != data
                {
                    return Err(Error::Corrupt);
                }
            }
        }
        self.at = end;
        Ok(())
    }
}

impl Cursor<'_> {
    /// One full-body Patch(72, 16280) record of an existing page.
    pub(super) fn patch(
        &mut self,
        header: WalFrameHeader,
        expected: RedoTarget,
        prior: u64,
        after: &[u8; PAGE_BYTES],
    ) -> Result<()> {
        let data = &after[72..DIGEST_OFFSET];
        let end = self
            .at
            .checked_add(64 + data.len())
            .ok_or(Error::Exhausted)?;
        match &mut self.sink {
            Sink::Write(out) => {
                let out = out.get_mut(self.at..end).ok_or(Error::Exhausted)?;
                wal_frame::encode_patch_record(expected, prior, 72, data, out)?;
            }
            Sink::Compare(input) => {
                let bytes = input.get(self.at..end).ok_or(Error::Corrupt)?;
                let record = wal_frame::decode_redo_record(header, bytes)?;
                if record.target != expected
                    || record.kind != RedoKind::Patch
                    || record.prior_lsn != prior
                    || record.offset != 72
                    || record.data != data
                {
                    return Err(Error::Corrupt);
                }
            }
        }
        self.at = end;
        Ok(())
    }
}

fn descriptor(input: &Inputs<'_>, candidate: &Candidate<'_>, out: &mut [u8; 1536]) -> Result<()> {
    finished_descriptor(
        input.profile,
        candidate.header,
        candidate.operation.frame_bytes(),
        out,
    )
}

/// The new Finished/zero-residue Structural owner of one scope4 group.
pub(super) fn finished_descriptor(
    profile: crate::storage::control::Profile,
    header: WalFrameHeader,
    frame_bytes: usize,
    out: &mut [u8; 1536],
) -> Result<()> {
    let reference = SystemWorkRef::new(
        profile.spec().namespace,
        SystemWorkKind::StructuralChange,
        header.lsn,
        Generation::new(u64::from(header.stream.writer_epoch))
            .map_err(|_| Error::IdentityMismatch)?,
    )
    .map_err(|_| Error::IdentityMismatch)?;
    let spec = DescriptorSpec {
        owner: OwnerState::System(SystemProgress {
            reference,
            phase: WorkPhase::Finished,
            sequence: 1,
            cursor: 1,
            limit: 1,
        }),
        chain: OwnerChain {
            apply: header.lsn,
            previous: 0,
            birth: header.lsn,
            groups: 1,
        },
        profile: profile_reference(profile),
        inventory: InventoryClaim {
            change_lsn: header.lsn,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        },
        consumed_wal: frame_bytes as u64,
        consumed_control: 0,
        remaining: [RemainingBudget::default(); 5],
        minimum_wal_lsn: 0,
        last_mutation_sequence: 0,
    };
    obligation::encode_descriptor(&spec, out)?;
    // The new owner is not the predecessor: its chain starts here, and the
    // old bank commitment is carried only by the already recomputed core.
    obligation::decode_descriptor(
        out,
        ReservationOwner::System(reference),
        profile_reference(profile),
    )?;
    Ok(())
}

fn body(
    input: &Inputs<'_>,
    candidate: &Candidate<'_>,
    after: &[[u8; PAGE_BYTES]; 5],
    cursor: &mut Cursor<'_>,
) -> Result<()> {
    let bytes = candidate.operation.frame_bytes();
    let mut desc = [0; 1536];
    descriptor(input, candidate, &mut desc)?;
    let mut group = [0; 192];
    group[..4].copy_from_slice(b"XSG1");
    for (at, value, width) in [
        (4, 2, 2),
        (6, candidate.operation as u64, 2),
        (8, 192, 2),
        (10, 4, 2),
        (12, (bytes - 192) as u64, 4),
        (16, candidate.header.lsn, 8),
        (120, 1536, 4),
        (124, 5, 2),
        (132, 5, 2),
        (136, 640, 4),
        (152, (bytes - super::REDO_AT - 32) as u64, 4),
    ] {
        put(&mut group, at, value, width);
    }
    group[32..88].copy_from_slice(&desc[16..72]);
    cursor.part(&group)?;
    cursor.part(&desc)?;
    cursor.part(&candidate.core)?;
    let order = order(candidate)?;
    for (ordinal, role) in order.into_iter().enumerate() {
        let header = candidate.pages[role];
        let mut native = [0; 128];
        native[0] = role as u8;
        native[1] = u8::from(candidate.operation.init(role as u8));
        put(&mut native, 2, ordinal as u64, 2);
        native[4] = header.kind as u8;
        native[5] = header.level;
        header.identity.page.encode(&mut native[8..32])?;
        put(&mut native, 32, header.identity.owner.get(), 8);
        put(&mut native, 40, header.identity.owner_generation.get(), 8);
        put(&mut native, 48, prior(candidate, role), 8);
        if let Some(before) = candidate.before[role] {
            native[56..88].copy_from_slice(&content_digest(before));
        }
        native[88..120].copy_from_slice(&content_digest(&after[role]));
        cursor.part(&native)?;
    }
    for role in order {
        cursor.redo(candidate, role, &after[role])?;
    }
    if cursor.at + 32 != bytes {
        return Err(Error::Corrupt);
    }
    Ok(())
}

/// Produce the exact used prefix. Output beyond `frame_bytes` is untouched.
/// Both buffers are workspace on failure; only success permits their use.
/// The immutable borrow passed to body is private and never crosses a return
/// to the caller after preparation. This is still no WAL/install authority.
pub(crate) fn produce<'a>(
    input: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
    output: &mut [u8],
    image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Observation> {
    let candidate = candidate::prepare(input, after, image)?;
    let bytes = candidate.operation.frame_bytes();
    let out = output.get_mut(..bytes).ok_or(Error::Exhausted)?;
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
    wal_frame::finish_semantic_envelope(candidate.header, 5, out)?;
    let view = super::decode_layout(out, input.frame, input.profile)?;
    if view.header() != candidate.header || view.operation() != candidate.operation {
        return Err(Error::Corrupt);
    }
    Ok(Observation::from(&candidate))
}

/// Recompute from the original cut and compare every native/core/redo byte or
/// canonical decoded redo field. A supplied after image or old Candidate is
/// never input to this API; all five images are output workspace again.
pub(crate) fn observe<'a>(
    input: &Inputs<'_>,
    frame: &[u8],
    after: &mut [[u8; PAGE_BYTES]; 5],
    image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Observation> {
    let view = super::decode_layout(frame, input.frame, input.profile)?;
    let candidate = candidate::prepare(input, after, image)?;
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

/// Source-value bound of exact scope4 parsing. The five borrowed native/redo
/// records are scanned one at a time; INIT page/hash codecs own no page copy.
fn layout_source_bytes() -> usize {
    use std::mem::size_of;
    let own = 3 * size_of::<super::LayoutView<'_>>()
        + 3 * size_of::<Result<super::LayoutView<'_>>>()
        + 2 * size_of::<super::CoreView<'_>>()
        + 3 * size_of::<super::NativeView<'_>>()
        + 3 * size_of::<wal_frame::RedoRecordView<'_>>()
        + 3 * size_of::<wal_frame::FrameEnvelope<'_>>()
        + 3 * size_of::<WalFrameHeader>()
        + 3 * size_of::<RedoTarget>()
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, obligation::ObligationError>>()
        + 2 * size_of::<[RemainingBudget; 5]>()
        + 3 * size_of::<(SystemWorkRef, SystemProgress, OwnerChain, InventoryClaim)>()
        + 2048;
    own + obligation::descriptor_source_bytes()
        .max(crate::storage::semantic_catalog::page_read_codec_bytes() + 1536)
        .max(1536 + std::mem::size_of::<foundation::codec::ContentHasher>())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceCensus {
    pub(crate) outer: usize,
    pub(crate) layout: usize,
    pub(crate) candidate: usize,
    pub(crate) body: usize,
    pub(crate) peak: usize,
}
/// The caller separately charges its before arena, after5, WAL frame and
/// callback captures. In observe the decoded LayoutView survives prepare.
pub(crate) fn source_census() -> SourceCensus {
    use std::mem::size_of;
    // The Observation/result is constructed only after prepare/body/parser
    // returns. Charge that return separately from the nested helper phases.
    let args = size_of::<(&Inputs<'_>, &[u8], &mut [[u8; PAGE_BYTES]; 5])>();
    let calls = args
        + size_of::<Candidate<'_>>()
        + size_of::<Result<Candidate<'_>>>()
        + size_of::<super::LayoutView<'_>>()
        + size_of::<Result<super::LayoutView<'_>>>()
        + size_of::<Cursor<'_>>()
        + 2 * size_of::<Result<()>>()
        + 256;
    let returning = args
        + size_of::<Candidate<'_>>()
        + size_of::<super::LayoutView<'_>>()
        + size_of::<Observation>()
        + size_of::<Result<Observation>>()
        + 256;
    let outer = calls.max(returning);
    let descriptor = 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, obligation::ObligationError>>()
        + 2 * size_of::<SystemProgress>()
        + 2 * size_of::<SystemWorkRef>()
        + 512
        + obligation::descriptor_source_bytes();
    let redo = 2 * size_of::<wal_frame::RedoRecordView<'_>>()
        + 3 * size_of::<RedoTarget>()
        + 2 * size_of::<WalFrameHeader>()
        + 1024
        + crate::storage::semantic_catalog::page_read_codec_bytes()
        + 1536;
    let body = size_of::<[u8; 1536]>()
        + size_of::<[u8; 192]>()
        + size_of::<[u8; 128]>()
        + size_of::<[(u64, u64, usize); 5]>()
        + 2 * size_of::<[usize; 5]>()
        + 3 * size_of::<PageHeader>()
        + 2 * size_of::<Cursor<'_>>()
        + 2 * size_of::<Observation>()
        + 1024
        + descriptor.max(redo);
    let (layout, candidate) = (layout_source_bytes(), candidate::source_bytes());
    SourceCensus {
        outer,
        layout,
        candidate,
        body,
        peak: outer + layout.max(candidate).max(body),
    }
}
pub(crate) fn source_bytes() -> usize {
    source_census().peak
}

#[cfg(test)]
#[path = "frame/source_tests.rs"]
mod source_tests;
