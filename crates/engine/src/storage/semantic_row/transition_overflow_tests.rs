//! OVF-C01 and OVF-C02 (dev3-row-overflow.md#tests, stage O2a): the C-owned
//! overflow groups over history H's state after H-506 (actor T14, its
//! statement 3 reserved and cancelled). C produces the overflow RowReserve
//! and each ChunkWrite; A's Forward and CLR of the overflow root are built
//! here through C's own prepare, run and encode with A's row cell (A's
//! overflow producer is O3's). Every group passes the whole-group observer,
//! the local observers that read it (jobs.rs always; status.rs, undo.rs and
//! leaf.rs for the Row groups) and the World's redo replay. There are no
//! literals: the expected allocator pick, chunk cells, Job bytes and ledger
//! row 1 are recomputed here from the plan and the contract's integers
//! (W = 56,976, F = 91,548 + 128h, R = 55,528, X = 59,056). Observations
//! confer no actual source, reservation, pin, staging, installation or
//! recovery authority.

use super::super::{ChainImages, ChunkPlan, OverflowReservePlan, run};
use super::*;
use crate::storage::{
    allocation_page::{
        self, ALLOCATION_CELL_BYTES, AllocationEntry, ReservationOwner, decode_allocation_page,
    },
    obligation::DescriptorSpec,
    overflow::{self as chain_format, MAX_OVERFLOW_PAYLOAD, OverflowChain, OverflowHead},
    page::PageHeader,
    retention_obligation::profile_reference,
    retention_page::row::PendingChain,
    row_record::RowPayload,
};

/// ChunkWrite's frame (dev3-row-overflow.md#opcodes).
const W: u64 = 56_976;
/// The Forward of an overflow root over an h1 predecessor, `91,548 + 128h`.
const F: u64 = 91_548 + 128;
/// One release step, and ChainRetire's largest frame (#ledger).
const R: u64 = 55_528;
const X: u64 = 59_056;
/// The overflow root's Job (H-505 and H-507 lease the same one).
const ORDINAL: usize = 17;

/// A big row of `columns()`: 4 + 1 + 8 + 1 + 4 + `text` bytes.
fn big(text: &[u8]) -> Vec<u8> {
    canonical(11, Some(text))
}
/// 40,018 bytes, so k = 3 chunks (16,092 · 2 < 40,018 ≤ 16,092 · 3).
fn value() -> Vec<u8> {
    big(&[b'x'; 40_000])
}
/// The window of chunk `index` of `value`.
fn window(value: &[u8], index: usize) -> &[u8] {
    let start = index * MAX_OVERFLOW_PAYLOAD;
    &value[start..value.len().min(start + MAX_OVERFLOW_PAYLOAD)]
}

fn reserve_pages() -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(14),
        table: true,
        ..pages()
    }
}
fn chunk_pages() -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(9),
        ..pages()
    }
}
fn row_pages() -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(14),
        previous: Some(10),
        table: true,
    }
}

/// The first extent's allocator identity (page 0, owner 1).
fn allocator_identity() -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: page_ref(0, 1),
        owner: ObjectId::new(1).unwrap(),
        owner_generation: generation(1),
    }
}
/// The World's pages as allocator entries: each page header's own
/// identity and kind Allocated, every other ordinal Free(1).
fn entries(world: &World) -> [AllocationEntry; 64] {
    let mut entries = [AllocationEntry::Free(generation(1)); 64];
    let mut present: Vec<&Page> = vec![&world.root, scope3!("anchor.bin")];
    present.extend(world.jobs.iter().flatten());
    present.extend(world.status.iter().map(|(_, page)| page));
    present.extend(world.undo.iter().map(|(_, page)| page));
    present.extend(world.chunks.iter().map(|(_, page)| page));
    present.extend(world.leaf.iter());
    for page in present {
        entries[u64_at(page, 32) as usize] = AllocationEntry::Allocated {
            generation: generation(u64_at(page, 40)),
            object: ObjectId::new(u64_at(page, 48)).unwrap(),
            object_generation: generation(u64_at(page, 56)),
            kind: PageKind::from_byte(page[6]).unwrap(),
        };
    }
    entries[0] = AllocationEntry::Allocated {
        generation: generation(1),
        object: ObjectId::new(1).unwrap(),
        object_generation: generation(1),
        kind: PageKind::Allocation,
    };
    entries
}
/// The allocator page with `entries` at `lsn`, through the production
/// encoders.
fn allocator_page(entries: &[AllocationEntry; 64], lsn: u64) -> Box<Page> {
    let header = PageHeader {
        identity: allocator_identity(),
        kind: PageKind::Allocation,
        level: 0,
        lsn: PageLsn(lsn),
    };
    let mut cell = [0; ALLOCATION_CELL_BYTES];
    allocation_page::encode_allocation_cell(header, namespace(), entries, &mut cell).unwrap();
    let mut page = Box::new([0; PAGE_BYTES]);
    allocation_page::encode_allocation_page(header, namespace(), &cell, &mut page[..]).unwrap();
    page
}
fn entry(allocator: &Page, ordinal: usize) -> AllocationEntry {
    decode_allocation_page(allocator, allocator_identity(), namespace())
        .unwrap()
        .entry(ordinal)
        .unwrap()
}
/// The table's identity at `page` (owner 6, generation 1).
fn chunk_identity(page: PageRef) -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page,
        owner: table().object,
        owner_generation: table().generation,
    }
}

/// History H to the end of H-506 (each group byte-equal to its literal),
/// with the allocator of its pages at LSN 5.
fn h_506() -> Box<World> {
    let mut world = h_501();
    h_statement_rollback(&mut world);
    World::replace(&mut world.undo, 14, &page_of(blob("H-undo14-0")));
    let plan = ReservePlan {
        terminal: 14,
        container: page_ref(14, 2),
        table: table(),
        statement: 3,
        canonical: blob("H-505-canonical"),
    };
    world.produce("H-505", Change::Reserve(&plan), reserve_pages());
    world.produce(
        "H-506",
        Change::Cancel { ordinal: ORDINAL },
        Pages {
            jobs: Some(1),
            status: Some(9),
            undo: Some(14),
            ..pages()
        },
    );
    world.allocator = Some(*allocator_page(&entries(&world), 5));
    world
}

/// C's production of `change` with no literal: produced, observed again
/// (same observation and afters), cross-checked by the local observers and
/// installed through the redo replay. Returns the frame.
fn step(world: &mut World, name: &str, change: Change<'_>, pages: Pages) -> (Observation, Vec<u8>) {
    let mut after = loans(0xa5);
    let mut frame = vec![0xa5; FRAME_BYTES];
    let observation = {
        let inputs = world.inputs(change, pages);
        let observation = produce(&inputs, &mut after, &mut frame)
            .unwrap_or_else(|error| panic!("{name}: produce {error:?}"));
        produced_twin(name, &inputs, observation, &after, &frame);
        assert!(
            frame[observation.frame_bytes..]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        let bytes = &frame[..observation.frame_bytes];
        let mut again = loans(0);
        let observed = observe(&inputs, bytes, &mut again)
            .unwrap_or_else(|error| panic!("{name}: observe {error:?}"));
        assert_eq!(observed, observation, "{name}");
        borrowed_twin(name, &inputs, bytes, 0, Ok(observed), &again);
        for &role in &observation.order[..usize::from(observation.count)] {
            same(name, &again[usize::from(role)], &after[usize::from(role)]);
        }
        world.cross_check(name, change, pages, bytes, &after, &observation);
        observation
    };
    frame.truncate(observation.frame_bytes);
    world.install(name, &observation, &after, &frame);
    println!(
        "OVF_GROUP\t{name}\t{:?}\tbytes={}\torder={:?}",
        observation.operation,
        observation.frame_bytes,
        &observation.order[..usize::from(observation.count)]
    );
    (observation, frame)
}

/// A's group for `change` built through C's own prepare, run and encode
/// with A's row cell `inserted` (test-only: A's overflow producer is O3's).
fn a_group<'a>(inputs: &Inputs<'a>, inserted: Option<&'a [u8]>) -> Result<Vec<u8>, Error> {
    let mut backing = vec![0; ROW_REPLAY_SCRATCH_BYTES];
    let backing: &mut [u8; ROW_REPLAY_SCRATCH_BYTES] = (&mut backing[..]).try_into().unwrap();
    let mut prepared = prepare::new_prepared(inputs, backing);
    prepare::prepare(inputs, inserted, &mut prepared)?;
    let mut after = loans(0xa5);
    run(inputs, &mut prepared, &mut after)?;
    let mut frame = vec![0; prepared.frame_bytes];
    frame::encode(inputs, &prepared, &after, &mut frame)?;
    Ok(frame)
}
/// A's group over `inputs` is refused with `expected` before any output,
/// and C's observer refuses the candidate `frame` (an accepted group of the
/// same before state) there the same.
fn a_refuses(
    label: &str,
    inputs: &Inputs<'_>,
    inserted: Option<&[u8]>,
    frame: Option<&[u8]>,
    expected: Error,
) {
    let mut backing = vec![0; ROW_REPLAY_SCRATCH_BYTES];
    let backing: &mut [u8; ROW_REPLAY_SCRATCH_BYTES] = (&mut backing[..]).try_into().unwrap();
    let mut prepared = prepare::new_prepared(inputs, backing);
    assert_eq!(
        prepare::prepare(inputs, inserted, &mut prepared),
        Err(expected),
        "{label}: prepare"
    );
    if let Some(frame) = frame {
        let mut after = loans(0xa5);
        assert_eq!(
            observe(inputs, frame, &mut after).map(|_| ()),
            Err(expected),
            "{label}: observe"
        );
        assert!(
            after.iter().flatten().all(|byte| *byte == 0xa5),
            "{label} after"
        );
    }
    println!("OVF_REFUSAL\t{label}\t{expected:?}");
}

/// The actor's ATT descriptor.
fn descriptor(world: &World, actor: TxnRef) -> DescriptorSpec {
    obligation::decode_descriptor(
        world.transaction.as_ref().unwrap(),
        ReservationOwner::Transaction(actor),
        profile_reference(profile()),
    )
    .unwrap()
}
fn job(world: &World) -> row::JobView<'_> {
    row::decode_job(&world.bank[ORDINAL], namespace()).unwrap()
}

/// The overflow row's head record for the Reserved root at `world`:
/// version (row, Transaction(actor), logical (actor, mutation), U) and the
/// head (root, total, digest).
fn head_record(world: &World, head: OverflowHead) -> Vec<u8> {
    let job = job(world);
    let actor = job.source().unwrap();
    let row = job.row_id().unwrap();
    let mut record = vec![0; 232];
    let length = row_record::encode_overflow_record(
        RecordExpectation {
            namespace: namespace(),
            row,
        },
        RecordVersion {
            row,
            stamp: VersionStamp::Transaction(actor),
            logical: LogicalRowVersion::new(actor.id(), job.mutation()).unwrap(),
            undo: job.undo(),
        },
        head,
        &mut record,
    )
    .unwrap();
    assert_eq!(length, 232);
    record
}
/// A's row cell for `record` (key = RowId big-endian).
fn row_cell(world: &World, record: &[u8]) -> Vec<u8> {
    let key = job(world).row_id().unwrap().get().to_be_bytes();
    let mut cell = vec![0; 12 + record.len()];
    tree_page::encode_leaf_cell(PageKind::RowLeaf, 0, &key, record, &mut cell).unwrap();
    cell
}

/// The overflow RowReserve of `value` (statement 4) and its checks: Job
/// `@285 = 1`, C = 244, pending (front 0, total, written 0), the head
/// record's root-free projection, and ledger row 1 = `(1+k, k·W + F)`.
fn reserve(world: &mut World, value: &[u8]) -> OverflowReservePlan {
    let plan = OverflowReservePlan {
        terminal: 14,
        container: page_ref(14, 2),
        table: table(),
        statement: 4,
        total: value.len(),
        digest: content_digest(value),
    };
    let (observation, frame) = step(
        world,
        "OVF-reserve",
        Change::ReserveOverflow(&plan),
        reserve_pages(),
    );
    assert_eq!(observation.job_ordinal, Some(ORDINAL));
    let row = RowId::new(11).unwrap();
    assert_eq!(
        observation.issued,
        Some(Issued {
            row,
            undo: undo_ref(14, 4),
            sequence: 4,
            mutation: 4,
        })
    );
    world.sealed.push((row, value.to_vec()));
    let job = job(world);
    let k = value.len().div_ceil(MAX_OVERFLOW_PAYLOAD) as u64;
    assert_eq!(
        (job.state(), job.payload(), job.cell_bytes()),
        (row::JobState::RootReserved, 1, 244)
    );
    assert_eq!(
        job.pending(),
        Some(PendingChain {
            front: None,
            total: value.len() as u32,
            written: 0,
        })
    );
    // The value commitment reads the head root `[168,192)` as zero
    // (dev3-row-overflow.md#overflow-row): any placeholder root gives it.
    let mut record = head_record(
        world,
        OverflowHead::new(page_ref(40, 7), value.len(), plan.digest).unwrap(),
    );
    record[168..192].fill(0);
    assert_eq!(
        *job.projection_digest(),
        super::super::row_projection(table(), row, Some(&record)).unwrap()
    );
    let ledger = descriptor(world, job.source().unwrap());
    assert_eq!(ledger.chain.apply, world.lsn);
    assert_eq!(job.last_step(), world.lsn);
    drop(frame);
    assert_eq!(
        (ledger.remaining[1].steps, ledger.remaining[1].wal),
        (1 + k, k * W + F)
    );
    plan
}

/// One ChunkWrite of chunk `k-1-written` and its checks: frame W, the
/// lowest allocator-eligible Free page allocated to the table as Overflow
/// with its next generation, the chunk INIT (index, next = old front,
/// payload window), the Job pending (front, written + 1), ledger row 1
/// one ChunkWrite smaller and `chain.apply` = its LSN.
fn chunk(world: &mut World, plan: &OverflowReservePlan, value: &[u8], expected_page: u64) {
    let before = job(world).pending().unwrap();
    let index = usize::from(before.count() - 1 - before.written);
    let allocator = Box::new(world.allocator.unwrap());
    let consumed = descriptor(world, job(world).source().unwrap()).consumed_wal;
    let chunk_plan = ChunkPlan {
        ordinal: ORDINAL,
        allocator: &allocator,
        digest: plan.digest,
        payload: window(value, index),
    };
    let name = format!("OVF-chunk{index}");
    let (observation, _) = step(world, &name, Change::ChunkWrite(&chunk_plan), chunk_pages());
    assert_eq!(observation.frame_bytes as u64, W, "{name}");
    assert_eq!(
        &observation.order[..usize::from(observation.count)],
        &[STATUS as u8, ROOT as u8, JOB as u8, UNDO as u8],
        "{name}: allocator page 0, Root, Jobs3, then the chunk"
    );
    let lsn = world.lsn;
    let page = page_ref(expected_page, 2);
    assert_eq!(
        entry(world.allocator.as_ref().unwrap(), expected_page as usize),
        AllocationEntry::Allocated {
            generation: generation(2),
            object: table().object,
            object_generation: table().generation,
            kind: PageKind::Overflow,
        },
        "{name}"
    );
    // Only that entry changed (the redo patch is the entry alone).
    for ordinal in (0..64).filter(|o| *o != expected_page as usize) {
        assert_eq!(
            entry(world.allocator.as_ref().unwrap(), ordinal),
            entry(&allocator, ordinal)
        );
    }
    let image = World::find(&world.chunks, expected_page);
    let chain = OverflowChain::new(value.len(), plan.digest).unwrap();
    let view =
        chain_format::decode_overflow_page(image, chunk_identity(page), chain, index).unwrap();
    assert_eq!(
        (
            view.index(),
            view.next(),
            view.payload(),
            view.header().lsn.0
        ),
        (index, before.front, window(value, index), lsn),
        "{name}"
    );
    let job = job(world);
    assert_eq!(
        job.pending(),
        Some(PendingChain {
            front: Some(page),
            total: value.len() as u32,
            written: before.written + 1,
        })
    );
    assert_eq!(job.last_step(), lsn);
    let ledger = descriptor(world, job.source().unwrap());
    let left = u64::from(before.count() - before.written - 1);
    assert_eq!(ledger.chain.apply, lsn, "{name}");
    assert_eq!(ledger.consumed_wal, consumed + W, "{name}");
    assert_eq!(
        (ledger.remaining[1].steps, ledger.remaining[1].wal),
        (1 + left, left * W + F),
        "{name}"
    );
}

/// The h-506 World with the overflow root of `value` reserved and its
/// chunks written (pages 15, 16, ... in write order).
fn written(value: &[u8]) -> (Box<World>, OverflowReservePlan) {
    let mut world = h_506();
    let plan = reserve(&mut world, value);
    let k = value.len().div_ceil(MAX_OVERFLOW_PAYLOAD) as u64;
    for written in 0..k {
        chunk(&mut world, &plan, value, 15 + written);
    }
    (world, plan)
}

/// The chain's pages front to back and the allocator, copied out of the
/// World so a change can borrow them while the World observes.
fn chain_pages(world: &World) -> (Box<Page>, Vec<Box<Page>>) {
    let job = job(world);
    let pending = job.pending().unwrap();
    let mut chain = Vec::new();
    let mut page_no = 15 + u64::from(pending.written);
    for _ in 0..pending.written {
        page_no -= 1;
        chain.push(Box::new(*World::find(&world.chunks, page_no)));
    }
    (Box::new(world.allocator.unwrap()), chain)
}

/// The accepted ChunkWrite `frame` (before LSN `previous_lsn`) with its
/// chunk cell header edited by `patch` in both the kind-12 native after and
/// the INIT image, the page, native body and frame digests resealed: the
/// candidate stays framing-valid, so only a semantic check refuses it.
fn rechunk(frame: &[u8], previous_lsn: u64, patch: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let layout = decode_layout(frame, expectation(previous_lsn), profile()).unwrap();
    let native = layout.native_records().find(|n| n.kind() == 12).unwrap();
    let body = native.after().unwrap();
    assert_eq!(body.len(), 128);
    let body = body.as_ptr() as usize - frame.as_ptr() as usize;
    let header = body - 128;
    let mut redo = layout.redo().as_ptr() as usize - frame.as_ptr() as usize;
    for _ in 0..native.redo_index() {
        redo += u32_at(frame, redo + 4);
    }
    let mut out = frame.to_vec();
    let image = redo + 64;
    let page: &mut Page = (&mut out[image..image + PAGE_BYTES]).try_into().unwrap();
    let start = usize::from(u16::from_le_bytes([page[128], page[129]]));
    patch(&mut page[start..start + 128]);
    let digest = content_digest(&page[..DIGEST_OFFSET]);
    page[DIGEST_OFFSET..].copy_from_slice(&digest);
    let cell: [u8; 128] = page[start..start + 128].try_into().unwrap();
    out[body..body + 128].copy_from_slice(&cell);
    out[header + 56..header + 88].copy_from_slice(&content_digest(&cell));
    let digest = content_digest(&out[..128]);
    out[128..160].copy_from_slice(&digest);
    let end = out.len() - 32;
    let digest = content_digest(&out[..end]);
    out[end..].copy_from_slice(&digest);
    out
}
/// `image` with its pageLSN `lsn` (page digest resealed).
fn restamped(image: &Page, lsn: u64) -> Box<Page> {
    let mut page = Box::new(*image);
    page[64..72].copy_from_slice(&lsn.to_le_bytes());
    let digest = content_digest(&page[..DIGEST_OFFSET]);
    page[DIGEST_OFFSET..].copy_from_slice(&digest);
    page
}

/// jobs.rs alone over a ChunkWrite of the root's next chunk at `world`
/// (dev3-row-overflow.md#job-overflow): the accepted group passes, and each
/// Job after that breaks the pending chain's step (written +2, the old
/// front kept, another total) is refused there (its rule failures are
/// IdentityMismatch), the Job native, its redo image and the frame
/// resealed, so no binding check sees the edit.
fn chunk_job_refusals(world: &World, plan: &OverflowReservePlan, value: &[u8]) {
    let pending = job(world).pending().unwrap();
    let index = usize::from(pending.count() - 1 - pending.written);
    let allocator = Box::new(world.allocator.unwrap());
    let chunk_plan = ChunkPlan {
        ordinal: ORDINAL,
        allocator: &allocator,
        digest: plan.digest,
        payload: window(value, index),
    };
    let inputs = world.inputs(Change::ChunkWrite(&chunk_plan), chunk_pages());
    let mut after = loans(0xa5);
    let mut frame = vec![0; FRAME_BYTES];
    let observation = produce(&inputs, &mut after, &mut frame).unwrap();
    frame.truncate(observation.frame_bytes);
    let context = undo::UndoContext {
        profile: profile(),
        actor: observation.actor.unwrap(),
        location: observation.status.unwrap(),
    };
    let observe_job = |frame: &[u8], job_after: &Page| {
        jobs::observe(
            decode_layout(frame, expectation(world.lsn), profile()).unwrap(),
            jobs::Inputs {
                context,
                jobs: JobCells::original(&world.bank),
                ordinal: ORDINAL,
                images: jobs::JobImages {
                    before: world.jobs[ORDINAL / 15].as_ref().unwrap(),
                    after: job_after,
                },
                reserve: None,
                sender: None,
                commit: None,
            },
        )
        .map(|_| ())
    };
    assert_eq!(observe_job(&frame, &after[JOB]), Ok(()));
    // The rebinding itself keeps the group bound: an unedited Job passes.
    let (same_frame, same_job) = rebind(&frame, world.lsn, 9, &after[JOB], |_| {});
    assert_eq!(observe_job(&same_frame, &same_job), Ok(()));
    let front = pending.front.unwrap();
    for (label, patch) in [
        (
            "written +2",
            Box::new(move |cell: &mut [u8]| {
                cell[356..358].copy_from_slice(&(pending.written + 2).to_le_bytes())
            }) as Box<dyn Fn(&mut [u8])>,
        ),
        (
            "front kept",
            Box::new(move |cell: &mut [u8]| front.encode(&mut cell[328..352]).unwrap()),
        ),
        (
            "total",
            Box::new(move |cell: &mut [u8]| {
                cell[352..356].copy_from_slice(&(pending.total - 1).to_le_bytes())
            }),
        ),
    ] {
        let (mutant, job_after) = rebind(&frame, world.lsn, 9, &after[JOB], |cell| patch(cell));
        row::decode_job(cell(&job_after[..], ORDINAL % 15), namespace())
            .unwrap_or_else(|error| panic!("{label}: canonical Job {error:?}"));
        // jobs.rs's own rule failures are IdentityMismatch.
        assert_eq!(
            observe_job(&mutant, &job_after),
            Err(Error::IdentityMismatch),
            "jobs.rs ChunkWrite {label}"
        );
        println!("OVF_REJECT\tjobs.rs ChunkWrite {label}\tIdentityMismatch");
    }
}

/// OVF-C01: ChunkWrite (dev3-row-overflow.md#transitions). The overflow
/// RowReserve seals the root-free head; three ChunkWrites write chunks 2,
/// 1, 0 back to front onto the lowest allocator-eligible Free pages 15, 16
/// and 17; and each refusal (a total outside 2,049..=131,072, payload
/// length, digest, an allocator with a Reserved entry or no eligible Free
/// page or a later LSN, an allocator whose front entry is Free or another
/// object's or kind's, an inline or Retained root, every chunk written,
/// Cancel of a written root) comes before any output. The observer refuses
/// a ChunkWrite whose chunk INIT (payload, index, next, count, total) or
/// allocator patch differs, and jobs.rs alone one whose Job after breaks
/// the pending chain's step.
#[test]
fn ovf_c01_chunk_write_allocates_and_inits_back_to_front() {
    let value = value();
    let mut world = h_506();
    // An inline Reserved root (H-507's literal at the same state) has no
    // pending chain.
    let mut inline = world.clone();
    let inline_plan = ReservePlan {
        terminal: 14,
        container: page_ref(14, 2),
        table: table(),
        statement: 4,
        canonical: blob("H-507-canonical"),
    };
    inline.produce("H-507", Change::Reserve(&inline_plan), reserve_pages());
    let allocator = Box::new(inline.allocator.unwrap());
    let plan = ChunkPlan {
        ordinal: ORDINAL,
        allocator: &allocator,
        digest: content_digest(&value),
        payload: window(&value, 2),
    };
    refuses(
        "OVF-C01 inline root",
        &inline.inputs(Change::ChunkWrite(&plan), chunk_pages()),
        FRAME_BYTES,
        Error::InvalidInput,
    );

    // A big row is 2,049..=131,072 bytes (dev3-row-overflow.md#transitions).
    for total in [
        crate::storage::overflow::INLINE_ROW_BYTES,
        crate::storage::overflow::MAX_ROW_BYTES + 1,
    ] {
        let plan = OverflowReservePlan {
            terminal: 14,
            container: page_ref(14, 2),
            table: table(),
            statement: 4,
            total,
            digest: content_digest(&value),
        };
        refuses(
            &format!("OVF-C01 Reserve total {total}"),
            &world.inputs(Change::ReserveOverflow(&plan), reserve_pages()),
            FRAME_BYTES,
            Error::Unsupported,
        );
    }

    let reserved = reserve(&mut world, &value);
    let allocator = Box::new(world.allocator.unwrap());
    let good = ChunkPlan {
        ordinal: ORDINAL,
        allocator: &allocator,
        digest: reserved.digest,
        payload: window(&value, 2),
    };
    let short = &value[2 * MAX_OVERFLOW_PAYLOAD..value.len() - 1];
    let mut reserved_entry = entries(&world);
    reserved_entry[30] = AllocationEntry::Reserved {
        generation: generation(2),
        claimant: ReservationOwner::Transaction(job(&world).source().unwrap()),
    };
    let reserved_entry = allocator_page(&reserved_entry, 5);
    let mut full = entries(&world);
    for entry in full.iter_mut().skip(9) {
        if matches!(entry, AllocationEntry::Free(_)) {
            *entry = AllocationEntry::Free(Generation::new(u64::MAX).unwrap());
        }
    }
    let full = allocator_page(&full, 5);
    let later = allocator_page(&entries(&world), world.lsn + 1);
    for (label, plan, ordinal, expected) in [
        (
            "payload length",
            ChunkPlan {
                payload: short,
                ..good
            },
            ORDINAL,
            Error::InvalidInput,
        ),
        (
            "digest",
            ChunkPlan {
                digest: [0x11; 32],
                ..good
            },
            ORDINAL,
            Error::IdentityMismatch,
        ),
        (
            "Reserved allocator entry",
            ChunkPlan {
                allocator: &reserved_entry,
                ..good
            },
            ORDINAL,
            Error::OrderMismatch,
        ),
        (
            "no eligible Free page",
            ChunkPlan {
                allocator: &full,
                ..good
            },
            ORDINAL,
            Error::Exhausted,
        ),
        (
            "allocator after the frame",
            ChunkPlan {
                allocator: &later,
                ..good
            },
            ORDINAL,
            Error::OrderMismatch,
        ),
        (
            "Retained root",
            ChunkPlan {
                ordinal: 15,
                ..good
            },
            15,
            Error::OrderMismatch,
        ),
    ] {
        assert_eq!(plan.ordinal, ordinal);
        refuses(
            &format!("OVF-C01 {label}"),
            &world.inputs(Change::ChunkWrite(&plan), chunk_pages()),
            FRAME_BYTES,
            expected,
        );
    }

    // Back to front: chunk 2 (the short tail), then 1, then 0, each on the
    // lowest Free page from ordinal 9 (15, 16, 17).
    let before = world.clone();
    for written in 0..3 {
        if written == 1 {
            chunk_job_refusals(&world, &reserved, &value);
        }
        chunk(&mut world, &reserved, &value, 15 + written);
        if written == 0 {
            // The front (page 15) is this root's own chunk: an allocator
            // where it is still Free (the image before the first
            // ChunkWrite) or Allocated to another object or as another kind
            // is refused before any output, before the pick could choose
            // page 15 again.
            let stale = Box::new(before.allocator.unwrap());
            let mut foreign = entries(&world);
            foreign[15] = AllocationEntry::Allocated {
                generation: generation(2),
                object: ObjectId::new(7).unwrap(),
                object_generation: generation(1),
                kind: PageKind::Overflow,
            };
            let foreign = allocator_page(&foreign, 5);
            let mut kind = entries(&world);
            kind[15] = AllocationEntry::Allocated {
                generation: generation(2),
                object: table().object,
                object_generation: table().generation,
                kind: PageKind::RowLeaf,
            };
            let kind = allocator_page(&kind, 5);
            assert_eq!(entry(&stale, 15), AllocationEntry::Free(generation(1)));
            for (label, allocator) in [
                ("front Free", &stale),
                ("front of another object", &foreign),
                ("front of another kind", &kind),
            ] {
                refuses(
                    &format!("OVF-C01 {label}"),
                    &world.inputs(
                        Change::ChunkWrite(&ChunkPlan {
                            allocator,
                            payload: window(&value, 1),
                            ..good
                        }),
                        chunk_pages(),
                    ),
                    FRAME_BYTES,
                    Error::IdentityMismatch,
                );
            }
            // A written root leaves only through OverflowCancel (O2b).
            refuses(
                "OVF-C01 Cancel of a written root",
                &world.inputs(
                    Change::Cancel { ordinal: ORDINAL },
                    Pages {
                        undo: Some(14),
                        ..chunk_pages()
                    },
                ),
                FRAME_BYTES,
                Error::InvalidInput,
            );
        }
    }
    let allocator = Box::new(world.allocator.unwrap());
    refuses(
        "OVF-C01 every chunk written",
        &world.inputs(
            Change::ChunkWrite(&ChunkPlan {
                allocator: &allocator,
                payload: window(&value, 0),
                ..good
            }),
            chunk_pages(),
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );

    // The observer refuses a first ChunkWrite whose chunk INIT payload or
    // allocator entry patch differs (each framing-valid: page and frame
    // digests resealed).
    let allocator = Box::new(before.allocator.unwrap());
    let plan = ChunkPlan {
        allocator: &allocator,
        ..good
    };
    let inputs = before.inputs(Change::ChunkWrite(&plan), chunk_pages());
    let mut after = loans(0xa5);
    let mut frame = vec![0; FRAME_BYTES];
    let observation = produce(&inputs, &mut after, &mut frame).unwrap();
    frame.truncate(observation.frame_bytes);
    let layout = decode_layout(&frame, inputs.frame, profile()).unwrap();
    let redo = layout.redo().as_ptr() as usize - frame.as_ptr() as usize;
    // Redo order: allocator patch, Root, Jobs3, chunk INIT.
    let mut records = Vec::new();
    let mut at = redo;
    while at < frame.len() - 32 {
        records.push(at);
        at += u32_at(&frame, at + 4);
    }
    assert_eq!(records.len(), 4);
    for (label, edit) in [
        ("chunk payload", records[3]),
        ("allocator entry", records[0]),
    ] {
        let mut mutant = frame.clone();
        if label == "chunk payload" {
            let image = edit + 64;
            let page: &mut Page = (&mut mutant[image..image + PAGE_BYTES]).try_into().unwrap();
            let start = usize::from(u16::from_le_bytes([page[128], page[129]]));
            page[start + 200] ^= 1;
            let digest = content_digest(&page[..DIGEST_OFFSET]);
            page[DIGEST_OFFSET..].copy_from_slice(&digest);
        } else {
            // The entry's generation (entry @8).
            mutant[edit + 64 + 8] ^= 1;
        }
        let end = mutant.len() - 32;
        let digest = content_digest(&mutant[..end]);
        mutant[end..].copy_from_slice(&digest);
        decode_layout(&mutant, inputs.frame, profile()).expect("framing-valid");
        let mut out = loans(0);
        assert_eq!(
            observe(&inputs, &mutant, &mut out).map(|_| ()),
            Err(Error::Corrupt),
            "{label}"
        );
        println!("OVF_REJECT\tChunkWrite {label}\tCorrupt");
    }
    // The chunk cell's index @6, next @48..72, count @8 and total @12 (the
    // tail chunk 2 of 3, no next), in the native kind-12 after and the INIT
    // image alike.
    let total = u32::try_from(value.len()).unwrap();
    for (label, patch) in [
        (
            "chunk index",
            Box::new(|cell: &mut [u8]| cell[6..8].copy_from_slice(&1u16.to_le_bytes()))
                as Box<dyn Fn(&mut [u8])>,
        ),
        (
            "chunk next",
            Box::new(|cell: &mut [u8]| page_ref(16, 2).encode(&mut cell[48..72]).unwrap()),
        ),
        (
            "chunk count",
            Box::new(|cell: &mut [u8]| cell[8..10].copy_from_slice(&4u16.to_le_bytes())),
        ),
        (
            "chunk total",
            Box::new(move |cell: &mut [u8]| {
                cell[12..16].copy_from_slice(&(total - 1).to_le_bytes())
            }),
        ),
    ] {
        let mutant = rechunk(&frame, before.lsn, |cell| patch(cell));
        decode_layout(&mutant, inputs.frame, profile()).expect("framing-valid");
        let mut out = loans(0);
        assert_eq!(
            observe(&inputs, &mutant, &mut out).map(|_| ()),
            Err(Error::Corrupt),
            "{label}"
        );
        println!("OVF_REJECT\tChunkWrite {label}\tCorrupt");
    }
}

/// OVF-C02: the Forward of an overflow root (dev3-row-overflow.md
/// #forward-check). A's head record names the pending front and total and
/// the chain is read front to back through the verifier and the schema
/// stream. Refused before output: fewer chunks written than k, a head root
/// or total other than the pending one, a damaged chunk (payload digest), a
/// canonical row the schema refuses (with its own digest), the chain pages
/// in another order or count, a chain that walks back to an address it
/// read, a chunk stamped at the lease or after the frame's previous LSN, an
/// allocator entry that is not the chunk's,
/// the plain Forward of an overflow root and an overflow Forward of an
/// inline root. Then the Forward: Retained, the sender digest over the
/// pending chain, ledger row 1 cleared; and from there Commit with Prune
/// (head kept, `@285 = 0`) and a full rollback whose CLR leaves the chain
/// on the Job (anchor, root, total; budget `(5, 4R + X)`) so that the
/// root's ReleaseRole is refused until ChainRetire (O2b) returns it.
#[test]
fn ovf_c02_forward_checks_the_chain_then_commit_or_rollback() {
    let value = value();
    let (mut world, plan) = written(&value);
    let front = page_ref(17, 2);
    let head = OverflowHead::new(front, value.len(), plan.digest).unwrap();
    let row_after = row_cell(&world, &head_record(&world, head));
    let (allocator, chain) = chain_pages(&world);
    let refs: Vec<&Page> = chain.iter().map(|page| &**page).collect();
    let images = ChainImages {
        allocator: &allocator,
        pages: &refs,
    };
    let forward = Change::ForwardOverflow {
        ordinal: ORDINAL,
        statement: boundary(4, None),
        chain: &images,
    };
    let frame =
        a_group(&world.inputs(forward, row_pages()), Some(&row_after)).expect("A's Forward");

    // Refusals at the written state.
    let other_root = row_cell(
        &world,
        &head_record(
            &world,
            OverflowHead::new(page_ref(16, 2), value.len(), plan.digest).unwrap(),
        ),
    );
    let other_total = row_cell(
        &world,
        &head_record(
            &world,
            OverflowHead::new(front, value.len() - 1, plan.digest).unwrap(),
        ),
    );
    let mut damaged = Box::new(*refs[2]);
    let start = usize::from(u16::from_le_bytes([damaged[128], damaged[129]]));
    damaged[start + 200] ^= 1;
    let digest = content_digest(&damaged[..DIGEST_OFFSET]);
    damaged[DIGEST_OFFSET..].copy_from_slice(&digest);
    let damaged_pages = [refs[0], refs[1], &*damaged];
    let reversed = [refs[2], refs[1], refs[0]];
    let mut stale = entries(&world);
    stale[16] = AllocationEntry::Allocated {
        generation: generation(2),
        object: ObjectId::new(7).unwrap(),
        object_generation: generation(1),
        kind: PageKind::Overflow,
    };
    let stale = allocator_page(&stale, 5);
    // Chunk 1 (page 16) naming the root page 17 as its next: the chain
    // walks back to an address it already read.
    let mut cycle = Box::new(*refs[1]);
    let start = usize::from(u16::from_le_bytes([cycle[128], cycle[129]]));
    front.encode(&mut cycle[start + 48..start + 72]).unwrap();
    let digest = content_digest(&cycle[..DIGEST_OFFSET]);
    cycle[DIGEST_OFFSET..].copy_from_slice(&digest);
    let duplicate = [refs[0], &*cycle, refs[0]];
    // Chunk 1 stamped at the lease (not after it) or after the frame's
    // previous LSN.
    let leased = restamped(refs[1], job(&world).lease_lsn());
    let leased = [refs[0], &*leased, refs[2]];
    let later = restamped(refs[1], world.lsn + 1);
    let later = [refs[0], &*later, refs[2]];
    // A head variant is A's cell, so only the producer side sees it; a
    // chain variant is an input, so C's observer also refuses the accepted
    // Forward over it.
    let order = Error::Page(crate::storage::page::PageError::IdentityMismatch);
    for (label, chain, allocator, row, expected) in [
        (
            "head root",
            &refs[..],
            &*allocator,
            &other_root,
            Error::IdentityMismatch,
        ),
        (
            "head total",
            &refs[..],
            &*allocator,
            &other_total,
            Error::IdentityMismatch,
        ),
        (
            "chunk digest",
            &damaged_pages[..],
            &*allocator,
            &row_after,
            Error::Corrupt,
        ),
        ("chain order", &reversed[..], &*allocator, &row_after, order),
        (
            "duplicate address",
            &duplicate[..],
            &*allocator,
            &row_after,
            Error::OrderMismatch,
        ),
        (
            "chunk at the lease LSN",
            &leased[..],
            &*allocator,
            &row_after,
            Error::OrderMismatch,
        ),
        (
            "chunk after the previous LSN",
            &later[..],
            &*allocator,
            &row_after,
            Error::OrderMismatch,
        ),
        (
            "chain count",
            &refs[..2],
            &*allocator,
            &row_after,
            Error::InvalidInput,
        ),
        (
            "allocator entry",
            &refs[..],
            &*stale,
            &row_after,
            Error::IdentityMismatch,
        ),
    ] {
        let images = ChainImages {
            allocator,
            pages: chain,
        };
        let change = Change::ForwardOverflow {
            ordinal: ORDINAL,
            statement: boundary(4, None),
            chain: &images,
        };
        a_refuses(
            &format!("OVF-C02 {label}"),
            &world.inputs(change, row_pages()),
            Some(row),
            (row == &row_after).then_some(&frame[..]),
            expected,
        );
    }
    let plain = Change::Forward {
        ordinal: ORDINAL,
        statement: boundary(4, None),
    };
    a_refuses(
        "OVF-C02 plain Forward of an overflow root",
        &world.inputs(plain, row_pages()),
        Some(&row_after),
        Some(&frame),
        Error::InvalidInput,
    );

    // Fewer chunks than k: the state after two ChunkWrites.
    let mut partial = h_506();
    let partial_plan = reserve(&mut partial, &value);
    chunk(&mut partial, &partial_plan, &value, 15);
    chunk(&mut partial, &partial_plan, &value, 16);
    let (partial_allocator, partial_pages) = chain_pages(&partial);
    let partial_refs: Vec<&Page> = partial_pages.iter().map(|page| &**page).collect();
    let partial_images = ChainImages {
        allocator: &partial_allocator,
        pages: &partial_refs,
    };
    let partial_cell = row_cell(
        &partial,
        &head_record(
            &partial,
            OverflowHead::new(page_ref(16, 2), value.len(), plan.digest).unwrap(),
        ),
    );
    a_refuses(
        "OVF-C02 written < k",
        &partial.inputs(
            Change::ForwardOverflow {
                ordinal: ORDINAL,
                statement: boundary(4, None),
                chain: &partial_images,
            },
            row_pages(),
        ),
        Some(&partial_cell),
        None,
        Error::OrderMismatch,
    );

    // A canonical row the schema refuses (invalid UTF-8), sealed and written
    // with its own digest: only the Forward's stream reads it.
    let invalid = big(&[0xff; 40_000]);
    let (bad, bad_plan) = written(&invalid);
    let (bad_allocator, bad_pages) = chain_pages(&bad);
    let bad_refs: Vec<&Page> = bad_pages.iter().map(|page| &**page).collect();
    let bad_images = ChainImages {
        allocator: &bad_allocator,
        pages: &bad_refs,
    };
    let bad_cell = row_cell(
        &bad,
        &head_record(
            &bad,
            OverflowHead::new(front, invalid.len(), bad_plan.digest).unwrap(),
        ),
    );
    let bad_inputs = bad.inputs(
        Change::ForwardOverflow {
            ordinal: ORDINAL,
            statement: boundary(4, None),
            chain: &bad_images,
        },
        row_pages(),
    );
    let refusal = a_group(&bad_inputs, Some(&bad_cell)).map(|_| ());
    assert_eq!(
        refusal,
        Err(Error::Row(row_record::RowError::Corrupt)),
        "OVF-C02 schema"
    );
    println!("OVF_REFUSAL\tOVF-C02 schema\t{refusal:?}");

    // An overflow Forward of an inline Reserved root.
    let mut inline = h_506();
    inline.produce(
        "H-507",
        Change::Reserve(&ReservePlan {
            terminal: 14,
            container: page_ref(14, 2),
            table: table(),
            statement: 4,
            canonical: blob("H-507-canonical"),
        }),
        reserve_pages(),
    );
    let inline_cell = decode_layout(blob("H-508"), expectation(inline.lsn), profile())
        .unwrap()
        .native_records()
        .find(|native| native.kind() == 1)
        .and_then(|native| native.after())
        .unwrap()
        .to_vec();
    a_refuses(
        "OVF-C02 overflow Forward of an inline root",
        &inline.inputs(forward, row_pages()),
        Some(&inline_cell),
        Some(blob("H-508")),
        Error::InvalidInput,
    );

    // The Forward: the sender digest (the prior ATT's) replaces the pending
    // chain.
    let sender = content_digest(world.transaction.as_ref().unwrap());
    world.observe("OVF-forward", &frame, forward, row_pages());
    let forward_lsn = world.lsn;
    let job_after = job(&world);
    assert_eq!(
        (job_after.state(), job_after.payload(), job_after.pending()),
        (row::JobState::RootRetained, 1, None)
    );
    assert_eq!(
        &job_after.bytes()[328..360],
        &sender[..],
        "sender digest over the pending chain"
    );
    let actor = job_after.source().unwrap();
    let ledger = descriptor(&world, actor);
    assert_eq!(ledger.chain.apply, forward_lsn);
    assert_eq!(ledger.remaining[1].steps, 0);
    let leaf = world.leaf.unwrap();
    let stored = leaf_cell(&leaf, RowId::new(11).unwrap());
    assert_eq!(stored, &row_after[..]);

    // Commit and Prune: the head is kept, restamped; the Job leaves overflow.
    let mut committed = world.clone();
    step(
        &mut committed,
        "OVF-commit",
        Change::Commit { terminal: 14 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    step(
        &mut committed,
        "OVF-prune",
        Change::Prune { ordinal: ORDINAL },
        row_pages(),
    );
    let pruned = job(&committed);
    assert_eq!(
        (pruned.state(), pruned.overflow()),
        (row::JobState::RootReleasingCommitted, false)
    );
    let leaf = committed.leaf.unwrap();
    let stored = leaf_cell(&leaf, RowId::new(11).unwrap());
    let record = row_record::decode_row_record_with_schema(
        &stored[12..],
        RecordExpectation {
            namespace: namespace(),
            row: RowId::new(11).unwrap(),
        },
        RowSchemaInput::Columns(&committed.columns),
    )
    .unwrap();
    assert_eq!(record.payload(), RowPayload::Overflow(head));
    assert!(matches!(record.version().stamp, VersionStamp::Committed(_)));
    assert_eq!(record.version().undo, None);

    // Full rollback: the overflow root's CLR keeps the chain to return.
    step(
        &mut world,
        "OVF-rollback",
        Change::RollbackStart {
            terminal: 14,
            statement: None,
        },
        Pages {
            status: Some(9),
            undo: Some(14),
            ..pages()
        },
    );
    let clr = Change::Clr {
        ordinal: ORDINAL,
        statement: None,
    };
    let frame = a_group(&world.inputs(clr, row_pages()), None).expect("A's CLR");
    world.observe("OVF-clr", &frame, clr, row_pages());
    let compensated = job(&world);
    assert_eq!(compensated.state(), row::JobState::RootReleasingCompensated);
    let chain = compensated.chain().expect("the chain to return");
    assert_eq!(
        (chain.anchor, chain.root, chain.total),
        (page_ref(12, 2), front, value.len() as u32)
    );
    let budget = compensated.cleanup();
    assert_eq!((budget.steps, budget.wal), (5, 4 * R + X));
    assert!(
        world.leaf.as_ref().map(|leaf| {
            let count = usize::from(u16::from_le_bytes([leaf[72], leaf[73]]));
            (1..count).all(|slot| cell(leaf, slot)[4..12] != 11u64.to_be_bytes())
        }) == Some(true),
        "the row is gone"
    );
    // The inline root 15 and then AbortEnd: the terminal is Ready, and the
    // overflow root's release still waits for its chain.
    let clr15 = Change::Clr {
        ordinal: 15,
        statement: None,
    };
    let pages15 = Pages {
        undo: Some(10),
        previous: None,
        ..row_pages()
    };
    let frame = a_group(&world.inputs(clr15, pages15), None).expect("A's CLR of root 15");
    world.observe("OVF-clr15", &frame, clr15, pages15);
    step(
        &mut world,
        "OVF-abort",
        Change::AbortEnd { terminal: 14 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    refuses(
        "OVF-C02 ReleaseRole with the chain",
        &world.inputs(
            Change::ReleaseRole { ordinal: ORDINAL },
            Pages {
                jobs: Some(1),
                undo: Some(14),
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::InvalidInput,
    );
    // The inline compensated root 16 releases at the same state.
    step(
        &mut world,
        "OVF-release16",
        Change::ReleaseRole { ordinal: 16 },
        Pages {
            jobs: Some(1),
            undo: Some(11),
            ..pages()
        },
    );
}

/// The head record encoder `encode_overflow_record` is the schema
/// encoder's Overflow arm, byte for byte, for the Reserved root's version
/// (dev3-row-overflow.md#overflow-row).
#[test]
fn ovf_c_head_record_is_the_schema_encoders_overflow_arm() {
    let value = value();
    let (world, plan) = written(&value);
    let head = OverflowHead::new(page_ref(17, 2), value.len(), plan.digest).unwrap();
    let job = job(&world);
    let actor = job.source().unwrap();
    let row = job.row_id().unwrap();
    let mut schema = vec![0xa5; 232];
    let length = row_record::encode_row_record_with_schema(
        RecordExpectation {
            namespace: namespace(),
            row,
        },
        RowSchemaInput::Columns(&world.columns),
        RecordVersion {
            row,
            stamp: VersionStamp::Transaction(actor),
            logical: LogicalRowVersion::new(actor.id(), job.mutation()).unwrap(),
            undo: job.undo(),
        },
        RowPayload::Overflow(head),
        &mut schema,
    )
    .unwrap();
    assert_eq!(length, 232);
    assert_eq!(schema, head_record(&world, head));
}

/// S1-a and S1-d over the overflow groups: every C production of the
/// Reserve and ChunkWrites repeated through `produce_borrowed` over a dirty
/// scratch, and every observation through `observe_borrowed` over a lent
/// tail, byte for byte (H-502..H-506 included).
#[test]
fn ovf_c_borrowed_producer_and_observer_twins() {
    PRODUCED.set(Some(0));
    let (_, accepted, refused) = with_borrowed(|| {
        written(&value());
        0
    });
    let produced = PRODUCED.take().expect("produced count");
    println!("OVF_C_BORROWED produced={produced} accepted={accepted} refused={refused}");
    // H-502, H-504, H-505, H-506, the Reserve and three ChunkWrites are
    // produced; those and A's H-503 are observed.
    assert_eq!((produced, accepted, refused), (8, 9, 0));
}

/// The three new C frames' source models (SHA excluded) are pinned and sit
/// below the census maxima they join: `chunk_write` under prepare's callee
/// maximum (the ledger term), `write_chunk` under the other write-step
/// terms, `forward_chain` inside the leaf term below the same maximum. So
/// the N1 source bound and every product census are unchanged by O2a.
#[test]
fn ovf_c_census_new_frames_stay_under_the_callee_maximum() {
    use super::super::{overflow as ovf, prepare as pre, source as src};
    let (chunk_write, write_chunk, forward_chain) = (
        ovf::chunk_write_bytes(),
        ovf::write_chunk_bytes(),
        ovf::forward_chain_bytes(),
    );
    println!(
        "OVF_C_CENSUS chunk_write={chunk_write} write_chunk={write_chunk} forward_chain={forward_chain} leaf={} ledger={} callee={}",
        src::leaf(),
        src::ledger(),
        pre::callee_bytes()
    );
    let write_others = crate::storage::retention_obligation::row::source_bytes()
        .max(row::source_bytes())
        .max(src::leaf_edit())
        .max(src::status_codec())
        .max(src::page_codec());
    assert!(chunk_write < src::ledger() && src::leaf() < src::ledger());
    assert_eq!(pre::callee_bytes(), src::ledger());
    assert!(write_chunk < write_others);
    assert!(forward_chain < src::leaf());
}
