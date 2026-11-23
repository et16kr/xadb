//! Inputs and INIT provenance for overflow replay. WAL supplies the newly
//! written payload. Existing chunks are always read from the before arena.
use super::*;
use crate::storage::{
    overflow::{MAX_OVERFLOW_CHUNKS, inspect_overflow_page},
    semantic_row::transition::{ChainImages, ChunkPlan},
};

/// Collect the bounded, Job-owned suffix. The whole-group observer checks
/// its indices, owner, allocator entries and length before using any page.
pub(super) fn chain_images<'a, 's>(
    arena: &'a [u8],
    profile: Profile,
    job: job3::JobView<'_>,
    pages: &'s mut [&'a [u8; PAGE_BYTES]; MAX_OVERFLOW_CHUNKS],
) -> ObservationResult<ChainImages<'s>>
where
    'a: 's,
{
    let (mut next, count) = if let Some(pending) = job.pending() {
        (pending.front, pending.written)
    } else {
        let chain = job.chain().ok_or_else(invalid)?;
        (Some(chain.root), chain.count())
    };
    let pages = pages.get_mut(..usize::from(count)).ok_or_else(invalid)?;
    let table = job.object().ok_or_else(invalid)?;
    for slot in &mut *pages {
        let reference = next.ok_or_else(invalid)?;
        let identity = PageIdentity {
            database: profile.spec().namespace.database,
            page: reference,
            owner: table.object,
            owner_generation: table.generation,
        };
        *slot = image(arena, reference)?;
        next = inspect_overflow_page(&slot[..], identity)
            .map_err(|_| invalid())?
            .next();
    }
    if next.is_some() {
        return Err(invalid());
    }
    Ok(ChainImages {
        allocator: arena_page(arena, 0),
        pages,
    })
}

/// The candidate's INIT and entry patch must name the same page as its Job
/// after. Only the row digest and payload are adopted by the producer.
pub(super) fn chunk_plan<'a>(
    view: LayoutView<'a>,
    arena: &'a [u8],
    profile: Profile,
    job: job3::JobView<'_>,
    ordinal: usize,
) -> ObservationResult<ChunkPlan<'a>> {
    let namespace = profile.spec().namespace;
    let table = job.object().ok_or_else(invalid)?;
    let before = job.pending().ok_or_else(invalid)?;
    let after_job = job3::decode_job(native_job(view)?.1.ok_or_else(invalid)?, namespace)
        .map_err(RetentionCause::Retention)?;
    let front = after_job
        .pending()
        .and_then(|p| p.front)
        .ok_or_else(invalid)?;
    let identity = PageIdentity {
        database: namespace.database,
        page: front,
        owner: table.object,
        owner_generation: table.generation,
    };
    let mut records = view.redo();
    let mut chunk = None;
    for native in view.native_records() {
        // decode_layout validated all lengths and their pairing with natives.
        let length = u32::from_le_bytes(records[4..8].try_into().expect("redo length")) as usize;
        let (raw, rest) = records.split_at(length);
        records = rest;
        if native.kind() == 6 {
            let old = AllocationEntry::decode(native.before().ok_or_else(invalid)?, namespace)
                .map_err(RetentionCause::Allocation)?;
            let new = AllocationEntry::decode(native.after().ok_or_else(invalid)?, namespace)
                .map_err(RetentionCause::Allocation)?;
            let alloc = allocation_page::decode_allocation_page(
                arena_page(arena, 0),
                recovery::allocator_identity(profile)?,
                namespace,
            )
            .map_err(RetentionCause::Allocation)?;
            let index = usize::try_from(front.page_no).map_err(|_| invalid())?;
            if !matches!(old, AllocationEntry::Free(_))
                || alloc.entry(index).map_err(RetentionCause::Allocation)? != old
                || old
                    .next_reservation_generation()
                    .map_err(RetentionCause::Allocation)?
                    != front.incarnation
                || new
                    != (AllocationEntry::Allocated {
                        generation: front.incarnation,
                        object: table.object,
                        object_generation: table.generation,
                        kind: PageKind::Overflow,
                    })
            {
                return Err(invalid());
            }
        }
        if native.kind() == 12 {
            let redo =
                wal_frame::decode_redo_record(view.header(), raw).map_err(|e| sem(e.into()))?;
            let page = inspect_overflow_page(redo.data, identity).map_err(|_| invalid())?;
            if redo.target.page != front
                || page.next() != before.front
                || page.index() + usize::from(before.written) + 1 != usize::from(before.count())
                || page.chain().total() != before.total as usize
                || page.header().lsn.0 != view.header().lsn
            {
                return Err(invalid());
            }
            chunk = Some(ChunkPlan {
                ordinal,
                allocator: arena_page(arena, 0),
                digest: page.chain().digest(),
                payload: page.payload(),
            });
        }
    }
    chunk.ok_or_else(invalid)
}

pub(super) fn overflow_init_record(
    view: LayoutView<'_>,
    target: TargetRequest,
    initial: &[u8; PAGE_BYTES],
    job: job3::JobView<'_>,
    out: &mut [u8; 352],
) -> ObservationResult<()> {
    let frame = view.header();
    let actor = job.source().ok_or_else(invalid)?;
    let table = job.object().ok_or_else(invalid)?;
    let anchor = job.pair().ok_or_else(invalid)?[0];
    let chunk = inspect_overflow_page(initial, target.identity).map_err(|_| invalid())?;
    if actor.id().writer_epoch != frame.stream.writer_epoch
        || job.lease_lsn() >= frame.lsn
        || table.generation.get() != 1
        || table.schema_epoch.get() != 1
        || table.object.get() < 6
        || target.identity.owner != table.object
        || target.identity.owner_generation != table.generation
        || anchor.page_no == target.identity.page.page_no
    {
        return Err(invalid());
    }
    out.fill(0);
    out[..5].copy_from_slice(&[1, 7, PageKind::Overflow as u8, 0, 2]);
    target
        .identity
        .page
        .encode(&mut out[8..32])
        .map_err(RetentionCause::Page)?;
    for (at, value) in [
        (32, table.object.get()),
        (40, table.generation.get()),
        (48, frame.lsn),
        (176, job.lease_lsn()),
        (184, table.object.get()),
        (192, table.generation.get()),
        (200, table.schema_epoch.get()),
    ] {
        out[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    out[56..60].copy_from_slice(&frame.stream.writer_epoch.to_le_bytes());
    out[64..96].copy_from_slice(&content_digest(view.bytes()));
    out[96..128].copy_from_slice(&content_digest(initial));
    actor
        .encode(&mut out[128..176])
        .map_err(RetentionCause::Page)?;
    anchor
        .encode(&mut out[208..232])
        .map_err(RetentionCause::Page)?;
    out[232..234].copy_from_slice(&(chunk.index() as u16).to_le_bytes());
    out[234..236].copy_from_slice(&(chunk.chain().count() as u16).to_le_bytes());
    out[236..240].copy_from_slice(&(chunk.chain().total() as u32).to_le_bytes());
    Ok(())
}

/// Extra stack-owned selection values. Arena pages and candidate bytes stay
/// borrowed. Both selectors walk iteratively, with one decoder at a time.
pub(super) fn selection_bytes() -> usize {
    use crate::storage::{overflow::OverflowPageView, semantic_catalog::page_read_codec_bytes};
    use std::mem::size_of;
    let page = page_read_codec_bytes();
    let inspect = crate::storage::overflow::source::inspect(page) + 1536;
    let frame = size_of::<(LayoutView<'_>, Profile, usize, ObjectHandle, PageIdentity)>()
        + 2 * size_of::<job3::JobView<'_>>()
        + size_of::<(
            job3::PendingChain,
            Option<job3::ReturnChain>,
            Option<PageRef>,
        )>()
        + size_of::<Option<ChunkPlan<'_>>>()
        + size_of::<ObservationResult<ChunkPlan<'_>>>()
        + size_of::<ObservationResult<ChainImages<'_>>>()
        + size_of::<wal_frame::RedoRecordView<'_>>()
        + size_of::<OverflowPageView<'_>>()
        + 8 * size_of::<&[u8]>();
    let allocation = 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 4 * size_of::<AllocationEntry>()
        + page
        + 1536;
    frame + inspect.max(allocation).max(job3::source_bytes())
}

/// The claim output itself is charged by the caller. Hashing the WAL or INIT
/// and decoding the overflow cell take place sequentially.
pub(super) fn claim_bytes() -> usize {
    use crate::storage::{overflow::OverflowPageView, semantic_catalog::page_read_codec_bytes};
    use std::mem::size_of;
    let frame = size_of::<(LayoutView<'_>, TargetRequest, job3::JobView<'_>, TxnRef)>()
        + size_of::<(ObjectHandle, PageRef, OverflowPageView<'_>)>()
        + size_of::<[(usize, u64); 7]>()
        + size_of::<[u8; 32]>()
        + 4 * size_of::<&[u8]>();
    frame
        + 1536
        + crate::storage::overflow::source::inspect(page_read_codec_bytes())
            .max(4 * size_of::<foundation::codec::ContentHasher>())
}
