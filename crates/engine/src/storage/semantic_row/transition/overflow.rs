//! The overflow parts of the Row groups (dev3-row-overflow.md#transitions):
//! the head record and its root-free projection, ChunkWrite's allocator
//! pick, Job pending and chunk page, and the Forward's pending-chain check.
//! Each runs in its own frame below prepare's (`#[inline(never)]`), so none
//! adds to prepare's locals; their source terms join prepare's callees.
use super::super::{Error, Result};
use super::{
    ChainImages, ChunkPlan, Inputs, LEAF, MAX_RECORD, Prepared, STATUS, UNDO,
    cells::{self, FIRST_FILE, put16, require, same},
    graph::Core,
    source,
};
use crate::{
    api::{DatabaseId, Namespace},
    storage::{
        allocation_page::{
            self, ALLOCATION_ENTRY_BYTES, ALLOCATION_HEADER_BYTES, AllocationEntry,
            AllocationError, AllocationPage,
        },
        overflow::{self, OverflowChain, OverflowChainVerifier, OverflowError, OverflowHead},
        page::{self, FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
        retention_page::row::{JobState, JobView, OVERFLOW_CELL, RowEffect},
        row_record::{
            self, RecordExpectation, RecordVersion, RowError, RowSchemaInput,
            stream::CanonicalRowStream,
        },
        transaction_record::TxnRef,
    },
};
use foundation::{
    codec::ContentHasher,
    identity::{Generation, ObjectHandle, ObjectId, RowId},
};

/// The head record of an overflow row: header 168 and head 64; its cell is
/// `OVERFLOW_CELL` (dev3-row-overflow.md#overflow-row).
pub(super) const OVERFLOW_RECORD: usize = OVERFLOW_CELL as usize - 12;
/// The head root `[168,192)` the value commitment reads as zero.
const ROOT: std::ops::Range<usize> = 168..192;

/// The new chunk's page before image: there is none (presence 2, INIT redo),
/// so its `Changed::before` is this zero page, which no step reads.
pub(super) static NO_PAGE: [u8; PAGE_BYTES] = [0; PAGE_BYTES];

pub(super) fn overflow_error(error: OverflowError) -> Error {
    match error {
        OverflowError::InvalidInput => Error::InvalidInput,
        OverflowError::UnsupportedFormat => Error::Unsupported,
        OverflowError::Corrupt | OverflowError::Incomplete => Error::Corrupt,
        OverflowError::HeadMismatch => Error::IdentityMismatch,
        OverflowError::OrderMismatch => Error::OrderMismatch,
        OverflowError::Page(error) => Error::Page(error),
    }
}
fn allocation_error(error: AllocationError) -> Error {
    match error {
        AllocationError::Page(error) => Error::Page(error),
        AllocationError::Corrupt => Error::Corrupt,
        AllocationError::UnsupportedFormat => Error::Unsupported,
        _ => Error::IdentityMismatch,
    }
}

/// The first extent's allocator page (page 0, owner 1).
fn allocator_identity(database: DatabaseId) -> PageIdentity {
    PageIdentity {
        database,
        page: PageRef {
            file: FileId::new(FIRST_FILE).expect("first file"),
            page_no: 0,
            incarnation: cells::one(),
        },
        owner: ObjectId::new(1).expect("allocator owner"),
        owner_generation: cells::one(),
    }
}

/// The stable first-extent allocator before image (`bounded` like the split
/// candidate's).
fn allocator<'a>(
    bytes: &'a [u8; PAGE_BYTES],
    namespace: Namespace,
    previous_lsn: u64,
) -> Result<AllocationPage<'a>> {
    let alloc = allocation_page::decode_allocation_page(
        bytes,
        allocator_identity(namespace.database),
        namespace,
    )
    .map_err(allocation_error)?;
    require(alloc.header().lsn.0 != 0 && alloc.header().lsn.0 <= previous_lsn)?;
    Ok(alloc)
}

/// The head record of `version` with total and digest and `placeholder` as
/// its root (the first 232 bytes of `output`). Only the root-free
/// projection reads it while the root is unknown.
pub(super) fn planned_record(
    namespace: Namespace,
    version: RecordVersion,
    total: usize,
    digest: [u8; 32],
    placeholder: PageRef,
    output: &mut [u8; MAX_RECORD],
) -> Result<usize> {
    let head = OverflowHead::new(placeholder, total, digest).map_err(|_| Error::InvalidInput)?;
    let length = row_record::encode_overflow_record(
        RecordExpectation {
            namespace,
            row: version.row,
        },
        version,
        head,
        output,
    )?;
    same(length == OVERFLOW_RECORD)?;
    Ok(length)
}

/// The value commitment of an overflow row (dev3-row-overflow.md
/// #overflow-row): the XRP1 projection of its head record with the root
/// read as zero. Reserve seals it; ChunkWrite, Forward, CLR and Prune
/// compare it with the Job.
pub(super) fn sealed_projection(
    table: ObjectHandle,
    row: RowId,
    record: &[u8],
) -> Result<[u8; 32]> {
    same(record.len() == OVERFLOW_RECORD)?;
    let mut prefix = [0; 48];
    prefix[..8].copy_from_slice(b"XRP1\x01\0\x01\0");
    let target = cells::row_target(table, row);
    crate::storage::undo_record::encode_undo_target(target, target, &mut prefix[8..40])?;
    prefix[40] = 1;
    let mut hash = ContentHasher::new();
    hash.update(&prefix);
    hash.update(&record[..ROOT.start]);
    hash.update(&[0; ROOT.end - ROOT.start]);
    hash.update(&record[ROOT.end..]);
    Ok(hash.digest())
}

/// The page an INSERT RowReserve's overflow plan allows (2,049..=131,072).
pub(super) fn reserve_total(total: usize) -> Result<usize> {
    if !(overflow::INLINE_ROW_BYTES + 1..=overflow::MAX_ROW_BYTES).contains(&total) {
        return Err(Error::Unsupported);
    }
    Ok(OVERFLOW_CELL as usize)
}

/// The Job bytes of an overflow RowReserve: `@285 = 1` and the pending chain
/// (front zero, total, written 0) (dev3-row-overflow.md#job-overflow).
pub(super) fn reserve_job(job: &mut [u8], total: usize) {
    job[285] = 1;
    cells::put32(job, 352, total as u32);
}

/// ChunkWrite's checks and Job after (dev3-row-overflow.md#transitions):
/// the actor's Reserved overflow root with an unwritten chunk, the plan's
/// digest under the Job's sealed projection, its window's length, and the
/// lowest allocator-eligible Free page of an allocator with no Reserved
/// entry. The Job's pending front becomes that page and written grows by
/// one; the allocator and the new chunk become the STATUS and UNDO roles.
#[inline(never)]
pub(super) fn chunk_write<'a>(
    inputs: &Inputs<'a>,
    plan: &ChunkPlan<'a>,
    job: JobView<'_>,
    actor: TxnRef,
    out: &mut Prepared<'a, '_>,
) -> Result<Core> {
    let namespace = inputs.profile.spec().namespace;
    require(job.state() == JobState::RootReserved)?;
    same(job.effect() == RowEffect::Insert)?;
    let pending = job.pending().ok_or(Error::InvalidInput)?;
    require(pending.written < pending.count())?;
    let table = job.object().ok_or(Error::IdentityMismatch)?;
    let row = job.row_id().ok_or(Error::IdentityMismatch)?;
    // The plan's digest is the one the Reserve sealed: the Job projection
    // binds it, so no chunk is read for it.
    let record = super::prepare::record_scratch(out.descriptors);
    let length = planned_record(
        namespace,
        super::prepare::current_version(job, actor)?,
        pending.total as usize,
        plan.digest,
        job.pair().ok_or(Error::IdentityMismatch)?[0],
        record,
    )?;
    let sealed = sealed_projection(table, row, &record[..length]);
    record.fill(0);
    same(sealed? == *job.projection_digest())?;
    let chain = OverflowChain::new(pending.total as usize, plan.digest).map_err(overflow_error)?;
    let index = usize::from(pending.count() - 1 - pending.written);
    if plan.payload.len() != chain.payload_len(index).map_err(overflow_error)? {
        return Err(Error::InvalidInput);
    }
    let alloc = allocator(plan.allocator, namespace, inputs.frame.previous_lsn)?;
    // The front is this root's own chunk; the new page is the lowest
    // allocator-eligible Free one, with no System reservation in flight
    // (dev3-row-overflow.md#transitions, as the split candidate selects).
    if let Some(front) = pending.front {
        alloc
            .lookup_allocated(
                cells::table_identity(table, namespace, front),
                PageKind::Overflow,
            )
            .map_err(allocation_error)?;
    }
    let mut page = None;
    for ordinal in 0..allocation_page::EXTENT_PAGES {
        let entry = alloc.entry(ordinal).map_err(allocation_error)?;
        if matches!(entry, AllocationEntry::Reserved { .. }) {
            return Err(Error::OrderMismatch);
        }
        if page.is_none()
            && ordinal >= 9
            && let Ok(incarnation) = entry.next_reservation_generation()
        {
            page = Some(PageRef {
                file: alloc.header().identity.page.file,
                page_no: ordinal as u64,
                incarnation,
            });
        }
    }
    let page = page.ok_or(Error::Exhausted)?;
    page.encode(&mut out.job[328..352])?;
    put16(out.job, 356, pending.written + 1);
    out.pages[STATUS] = Some(super::Changed {
        identity: allocator_identity(namespace.database),
        kind: PageKind::Allocation,
        before: plan.allocator,
        before_slot: Some(0),
        after_slot: Some(0),
    });
    out.pages[UNDO] = Some(super::Changed {
        identity: cells::table_identity(table, namespace, page),
        kind: PageKind::Overflow,
        before: &NO_PAGE,
        before_slot: None,
        after_slot: Some(0),
    });
    Ok(Core {
        statement: job.statement(),
        mutation: job.mutation(),
        sequence: job.undo_sequence(),
        current: Some(job.undo().ok_or(Error::IdentityMismatch)?),
        ..Core::default()
    })
}

/// CLR of an overflow root (dev3-row-overflow.md#job-overflow): the
/// ReleasingCompensated Job keeps the chain it returns, the table anchor
/// (its pair[0]) and the row's chunk 0 at `[200,248)`, `@285 = 1` and the
/// total at `[288,292)`, over the zero Releasing fields.
pub(super) fn clr_chain(
    job: &mut [u8; crate::storage::retention_page::JOB_BYTES],
    before: JobView<'_>,
    head: OverflowHead,
) -> Result<()> {
    let anchor = before.pair().ok_or(Error::IdentityMismatch)?[0];
    anchor.encode(&mut job[200..224])?;
    head.root().encode(&mut job[224..248])?;
    job[285] = 1;
    cells::put32(job, 288, head.total() as u32);
    Ok(())
}

/// The page offset of the allocator entry of first-extent page `ordinal`
/// in the allocator image `bytes`.
pub(super) fn entry_offset(
    bytes: &[u8; PAGE_BYTES],
    database: DatabaseId,
    ordinal: u64,
) -> Result<usize> {
    let view = page::decode_page(bytes, allocator_identity(database), PageKind::Allocation)?;
    let start = view.cell_range(0).ok_or(Error::Corrupt)?.start;
    Ok(start + ALLOCATION_HEADER_BYTES + ordinal as usize * ALLOCATION_ENTRY_BYTES)
}

/// ChunkWrite's allocator and chunk after pages (STATUS, UNDO). The unused
/// LEAF loan holds each cell while its page is encoded and is zero again on
/// return, success or error.
#[inline(never)]
pub(super) fn write_chunk(
    inputs: &Inputs<'_>,
    prepared: &Prepared<'_, '_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<()> {
    let (head, tail) = after.split_at_mut(LEAF);
    let result = place_chunk(inputs, prepared, head, &mut tail[0]);
    tail[0].fill(0);
    result
}
fn place_chunk(
    inputs: &Inputs<'_>,
    prepared: &Prepared<'_, '_>,
    head: &mut [[u8; PAGE_BYTES]],
    scratch: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let super::Change::ChunkWrite(plan) = inputs.change else {
        return Err(Error::InvalidInput);
    };
    let namespace = inputs.profile.spec().namespace;
    let lsn = PageLsn(prepared.header.lsn);
    let chunk = prepared.pages[UNDO].ok_or(Error::InvalidInput)?;
    let ordinal = prepared.ordinal.ok_or(Error::InvalidInput)?;
    let before = crate::storage::retention_page::row::decode_job(&inputs.jobs[ordinal], namespace)?
        .pending()
        .ok_or(Error::InvalidInput)?;
    let identity = allocator_identity(namespace.database);
    let view = page::decode_page(plan.allocator, identity, PageKind::Allocation)?;
    let cell = view.cell(0).ok_or(Error::Corrupt)?;
    let length = cell.len();
    scratch[..length].copy_from_slice(cell);
    let at =
        ALLOCATION_HEADER_BYTES + chunk.identity.page.page_no as usize * ALLOCATION_ENTRY_BYTES;
    AllocationEntry::Allocated {
        generation: chunk.identity.page.incarnation,
        object: chunk.identity.owner,
        object_generation: chunk.identity.owner_generation,
        kind: PageKind::Overflow,
    }
    .encode(namespace, &mut scratch[at..at + ALLOCATION_ENTRY_BYTES])
    .map_err(allocation_error)?;
    allocation_page::encode_allocation_page(
        PageHeader {
            identity,
            kind: PageKind::Allocation,
            level: 0,
            lsn,
        },
        namespace,
        &scratch[..length],
        &mut head[STATUS],
    )
    .map_err(allocation_error)?;
    let chain = OverflowChain::new(before.total as usize, plan.digest).map_err(overflow_error)?;
    let index = usize::from(before.count() - 1 - before.written);
    let length = overflow::encode_overflow_cell(
        chain,
        index,
        chunk.identity.page,
        before.front,
        plan.payload,
        &mut scratch[..],
    )
    .map_err(overflow_error)?;
    overflow::encode_overflow_page(
        PageHeader {
            identity: chunk.identity,
            kind: PageKind::Overflow,
            level: 0,
            lsn,
        },
        chain,
        index,
        &scratch[..length],
        &mut head[UNDO],
    )
    .map_err(overflow_error)
}

/// The Forward of an overflow root (dev3-row-overflow.md#transitions,
/// #forward-check): every chunk written, the record's head at the pending
/// front with the pending total, and the chain read once front to back
/// through `OverflowChainVerifier` (identity, order, addresses, digest) and
/// `CanonicalRowStream` (the table schema), each chunk Allocated to the
/// table as Overflow with its incarnation and written after the lease.
#[inline(never)]
pub(super) fn forward_chain(
    inputs: &Inputs<'_>,
    job: JobView<'_>,
    head: OverflowHead,
    schema: RowSchemaInput<'_>,
) -> Result<()> {
    let super::Change::ForwardOverflow { chain, .. } = inputs.change else {
        return Err(Error::InvalidInput);
    };
    let ChainImages {
        allocator: bytes,
        pages,
    } = *chain;
    let namespace = inputs.profile.spec().namespace;
    let pending = job.pending().ok_or(Error::InvalidInput)?;
    require(pending.written == pending.count())?;
    same(Some(head.root()) == pending.front && head.total() == pending.total as usize)?;
    if pages.len() != head.count() {
        return Err(Error::InvalidInput);
    }
    let table = job.object().ok_or(Error::IdentityMismatch)?;
    let alloc = allocator(bytes, namespace, inputs.frame.previous_lsn)?;
    let root = cells::table_identity(table, namespace, head.root());
    let mut verifier = OverflowChainVerifier::new(root, head).map_err(overflow_error)?;
    let mut stream = CanonicalRowStream::new(schema, head.total())?;
    let mut next = Some(head.root());
    for (index, image) in pages.iter().copied().enumerate() {
        let identity = PageIdentity {
            page: next.ok_or(Error::Corrupt)?,
            ..root
        };
        let lsn = cells::page_lsn(image);
        require(lsn > job.lease_lsn() && lsn <= inputs.frame.previous_lsn)?;
        alloc
            .lookup_allocated(identity, PageKind::Overflow)
            .map_err(allocation_error)?;
        let view = overflow::decode_overflow_page(image, identity, head.chain(), index)
            .map_err(overflow_error)?;
        verifier.push(&view).map_err(overflow_error)?;
        stream.push(view.payload())?;
        next = view.next();
    }
    verifier.finish().map_err(overflow_error)?;
    stream.finish()?;
    Ok(())
}

/// The allocator codec: one allocation page decode or encode with its
/// entry parse over the page codec (the structure candidate's
/// `allocation_codec` inventory), the SHA closure excluded (the caller
/// charges it once).
fn allocation_codec() -> usize {
    use std::mem::size_of;
    3 * size_of::<AllocationEntry>()
        + 2 * size_of::<AllocationPage<'_>>()
        + 3 * size_of::<std::result::Result<AllocationEntry, AllocationError>>()
        + 2 * size_of::<std::result::Result<AllocationPage<'_>, AllocationError>>()
        + 2 * size_of::<page::PageView<'_>>()
        + 3 * size_of::<PageHeader>()
        + ALLOCATION_ENTRY_BYTES
        + source::slack()
        + source::page_codec()
}
/// `allocator` and an allocator lookup: arguments, the decoded view and
/// its Result, two identities, then the allocator codec.
fn allocator_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&[u8; PAGE_BYTES], Namespace, u64)>()
        + source::returned::<AllocationPage<'_>, AllocationError>()
        + 2 * size_of::<PageIdentity>()
        + allocation_codec()
}
/// `planned_record` over the head record encode, which the Row record
/// codec term covers (its Overflow arm).
fn record_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(
        Namespace,
        RecordVersion,
        usize,
        [u8; 32],
        PageRef,
        &mut [u8],
    )>() + source::returned::<OverflowHead, OverflowError>()
        + source::returned::<usize, RowError>()
        + source::row_codec()
}
/// Source term of `chunk_write`, the SHA closure excluded: its arguments
/// and result, the pending chain, handles, indexes, record reference,
/// projection, chain, length, allocator view, selected page and entry
/// beside the deepest of the head record encode, the sealed projection
/// (`source::projection` with the two extra root-zero pieces), the
/// allocator decode and lookups and the Job reference encode.
pub(super) fn chunk_write_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&Inputs<'_>, &ChunkPlan<'_>, JobView<'_>, TxnRef, &mut [u8])>()
        + source::returned::<Core, Error>()
        + size_of::<crate::storage::retention_page::row::PendingChain>()
        + size_of::<(ObjectHandle, RowId, usize, usize)>()
        + size_of::<&mut [u8; MAX_RECORD]>()
        + source::returned::<[u8; 32], Error>()
        + source::returned::<OverflowChain, OverflowError>()
        + source::returned::<usize, OverflowError>()
        + source::returned::<AllocationPage<'_>, Error>()
        + size_of::<(Option<PageRef>, AllocationEntry)>()
        + source::returned::<AllocationEntry, AllocationError>()
        + source::returned::<Generation, AllocationError>()
        + record_bytes()
            .max(source::projection() + size_of::<[u8; 24]>())
            .max(allocator_bytes())
            .max(source::job_edit())
}
/// Source term of `write_chunk` with `place_chunk`, the SHA closure
/// excluded: their arguments, the split loans, the pending chain and Job
/// decode result, positions and chain, then the Job3 decode, the
/// allocator entry and page encode, or the chunk cell and page encode
/// (`inspect`'s page codec and cell parse bound the encode's).
pub(super) fn write_chunk_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&Inputs<'_>, &Prepared<'_, '_>, &mut [[u8; PAGE_BYTES]; 5])>()
        + size_of::<(&mut [[u8; PAGE_BYTES]], &mut [[u8; PAGE_BYTES]])>()
        + 2 * size_of::<Result<()>>()
        + size_of::<(
            &Inputs<'_>,
            &Prepared<'_, '_>,
            &mut [[u8; PAGE_BYTES]],
            &mut [u8],
        )>()
        + size_of::<(
            &ChunkPlan<'_>,
            Namespace,
            PageLsn,
            super::Changed<'_>,
            usize,
        )>()
        + size_of::<crate::storage::retention_page::row::PendingChain>()
        + source::returned::<JobView<'_>, crate::storage::retention_page::RetentionError>()
        + size_of::<(PageIdentity, page::PageView<'_>, &[u8], usize, usize)>()
        + source::returned::<OverflowChain, OverflowError>()
        + source::returned::<usize, OverflowError>()
        + size_of::<(
            OverflowChain,
            usize,
            PageRef,
            Option<PageRef>,
            &[u8],
            &mut [u8],
        )>()
        + crate::storage::retention_page::row::source_bytes()
            .max(allocation_codec())
            .max(overflow::source::inspect(source::page_codec()))
}
/// Source term of `forward_chain`, the SHA closure excluded: its
/// arguments, the chain images, pending chain, identities, allocator view,
/// verifier and stream beside one chunk view, then the deepest of the
/// allocator decode and lookup, the chunk decode (`inspect` bounds it),
/// the verifier push and the stream push.
pub(super) fn forward_chain_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&Inputs<'_>, JobView<'_>, OverflowHead, RowSchemaInput<'_>)>()
        + size_of::<ChainImages<'_>>()
        + size_of::<crate::storage::retention_page::row::PendingChain>()
        + size_of::<(
            ObjectHandle,
            PageIdentity,
            PageIdentity,
            Option<PageRef>,
            u64,
        )>()
        + size_of::<AllocationPage<'_>>()
        + source::returned::<OverflowChainVerifier, OverflowError>()
        + source::returned::<CanonicalRowStream<'_>, RowError>()
        + source::returned::<overflow::OverflowPageView<'_>, OverflowError>()
        + size_of::<std::iter::Enumerate<std::slice::Iter<'_, &[u8; PAGE_BYTES]>>>()
        + 4 * size_of::<std::result::Result<(), OverflowError>>()
        + 2 * size_of::<std::result::Result<(), RowError>>()
        + allocator_bytes()
            .max(overflow::source::inspect(source::page_codec()))
            .max(overflow::source::verifier_push(0))
            .max(row_record::stream::source::push())
}
