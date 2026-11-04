//! Original-source preparation for a Row split. This result grants neither
//! a claim nor staging/install rights. Those effects must recheck its cut and
//! stamps and supply I/O quiescence, outstanding credit and XOI succession.
#![allow(
    dead_code,
    reason = "original structural preparation precedes its effect consumer"
)]
use super::*;
use crate::storage::{
    page,
    retention_page::row::JobCells,
    semantic_row::{
        self as row,
        structure::{self as smo, candidate, frame as split_frame},
    },
};

/// All bytes remain immutable borrows of the caller's existing Row loan.
/// Roles index the images; physical order indexes requests, roles and stamps.
pub(in crate::storage) struct PreparedStructural<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    owner: OwnerIdentity,
    prefix: WalPrefix,
    control: ControlCut,
    observation: split_frame::Observation,
    frame: &'i [u8],
    frame_digest: [u8; 32],
    after: &'i [[u8; PAGE_BYTES]; 5],
    before: &'i [[u8; PAGE_BYTES]; 9],
    requests: [TargetRequest; 5],
    roles: [u8; 5],
    stamps: [Option<FrameStamp>; 5],
}

fn unchanged_cut(
    prefix: WalPrefix,
    current: WalPrefix,
    cut: ControlCut,
    control: &control::RetentionLedger<'_, '_, '_>,
) -> Result<()> {
    if current != prefix || !cut.matches(control) {
        return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
    }
    Ok(())
}

fn semantic(error: row::Error) -> RetentionError {
    unchanged(RetentionPhase::Prepare, RetentionCause::RowSemantic(error))
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Borrow only the original writer resources. No decoded/raw caller page
    /// becomes a before image, and no WAL or control state is advanced here.
    pub(in crate::storage) fn prepare_row_split<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        source: PageRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedStructural<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_split(owner, table, source, private)
    }

    #[cfg(test)]
    pub(in crate::storage) fn test_structural_cut_guards(&self) {
        let prefix = self.storage.wal().prefix().unwrap();
        let cut = ControlCut::capture(self.control);
        unchanged_cut(prefix, prefix, cut, self.control).unwrap();
        let later = WalPrefix {
            complete_lsn: prefix.complete_lsn + 1,
            ..prefix
        };
        assert!(unchanged_cut(prefix, later, cut, self.control).is_err());
        let mut old = cut;
        old.jobs[0] ^= 1;
        assert!(unchanged_cut(prefix, prefix, old, self.control).is_err());
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_structural_replay_returned(&self) -> bool {
        self.history.test_replay_returned()
    }

    #[cfg(test)]
    pub(in crate::storage) fn test_split_read4<'x, 'y>(
        &self,
        pool: &SharedPool<'x, 'y, '_, '_>,
        owner: &mut Owner<'x, 'y>,
        observed: &split_frame::Observation,
        before: &mut [[u8; PAGE_BYTES]; 9],
        arena: &[u8],
    ) -> Result<[Option<FrameStamp>; 5]> {
        // Physical helper fixture only; this never calls the prepared factory.
        read_split_before::<4, W>(self.storage, pool, owner, observed, before, arena)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    #[inline(never)]
    fn prepare_split<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        source: PageRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedStructural<'i, 'c, 's>> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let (owner_identity, prefix) = self.existing_entry(owner)?;
        let cut = ControlCut::capture(self.control);
        let (frame_space, rest) = private.split_at_mut(RowLoanLayout::FRAME.end);
        let (before_space, rest) = rest.split_at_mut(range_len(RowLoanLayout::BEFORE));
        let (after_space, _) = rest.split_at_mut(range_len(RowLoanLayout::AFTER));
        let before: &mut [[u8; PAGE_BYTES]; 9] = before_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("original Row before loans");
        let after: &mut [[u8; PAGE_BYTES]; 5] = after_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("original Row after loans");
        let (observation, requests, roles, stamps) = {
            let mut audit = self.history.replay_scope(owner)?;
            let mut replay = audit.checkout()?;
            // File reads finish before the first target latch. Keep the wide
            // target/result packets out of the nested canonical replay phase.
            if let Err(error) =
                history::validate_live(storage, self.control, replay.bytes_mut(), prefix)
            {
                let rejected = merge(Err(error), replay.return_checked());
                return merge(rejected, audit.finish_checked());
            }
            let prepared = (|| {
                let bank_row = self.control.row(control::STRUCTURAL_SLOT);
                let previous = control::observe_structural_bank_row(
                    storage.profile(),
                    prefix.complete_lsn,
                    prefix.stream.writer_epoch,
                    bank_row,
                )
                .map_err(|cause| fatal(storage, RetentionPhase::Census, cause))?;
                let inputs = candidate::Inputs {
                    profile: storage.profile(),
                    frame: FrameExpectation {
                        stream: prefix.stream,
                        previous_lsn: prefix.complete_lsn,
                    },
                    table,
                    source,
                    jobs: JobCells::original(self.control.jobs()),
                    previous_high_water: previous.map_or(0, |p| p.owner.id()),
                    previous: previous.map(|p| {
                        (
                            p.owner,
                            bank_row[..1536]
                                .try_into()
                                .expect("checked bank descriptor"),
                        )
                    }),
                };
                let observed = split_frame::produce(&inputs, after, frame_space, |reference| {
                    canonical_image(replay.bytes(), reference)
                        .map_err(|_| row::Error::IdentityMismatch)
                })
                .map_err(semantic)?;
                check_new_destinations(storage, &observed, replay.bytes())?;
                let mut order = [0usize, 1, 2, 3, 4];
                order.sort_unstable_by_key(|at| target_rank(request(observed.targets[*at])));
                let requests = order.map(|at| request(observed.targets[at]));
                let roles = order.map(|at| at as u8);
                let stamps = match observed.operation {
                    smo::Operation::RootSplit => read_split_before::<3, W>(
                        storage,
                        self.pool,
                        owner,
                        &observed,
                        before,
                        replay.bytes(),
                    ),
                    smo::Operation::ChildSplit => read_split_before::<4, W>(
                        storage,
                        self.pool,
                        owner,
                        &observed,
                        before,
                        replay.bytes(),
                    ),
                }?;
                // Convert role-indexed stamps to the same physical order as
                // requests. INIT roles have no invented old frame stamp.
                Ok((observed, requests, roles, order.map(|at| stamps[at])))
            })();
            let prepared = merge(prepared, replay.return_checked());
            merge(prepared, audit.finish_checked())?
        };
        let current = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        unchanged_cut(prefix, current, cut, self.control)?;
        let bytes = &frame_space[..observation.frame_bytes];
        Ok(PreparedStructural {
            core: self.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            owner: owner_identity,
            prefix,
            control: cut,
            observation,
            frame: bytes,
            frame_digest: content_digest(bytes),
            after,
            before,
            requests,
            roles,
            stamps,
        })
    }
}

/// The successful canonical candidate supplies identities, but only the
/// original pool's latched images supply physical before stamps.
#[inline(never)]
#[allow(
    clippy::too_many_arguments,
    reason = "disjoint original pool/source loans"
)]
fn read_split_before<'c, 's, const N: usize, W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    pool: &SharedPool<'c, 's, '_, '_>,
    owner: &mut Owner<'c, 's>,
    observed: &split_frame::Observation,
    before: &mut [[u8; PAGE_BYTES]; 9],
    arena: &[u8],
) -> Result<[Option<FrameStamp>; 5]> {
    const {
        assert!(N == 3 || N == 4);
    }
    let mut roles: [usize; N] = std::array::from_fn(|at| if at == 3 { 4 } else { at });
    roles.sort_unstable_by_key(|at| target_rank(request(observed.targets[*at])));
    let requests = roles.map(|at| request(observed.targets[at]));
    let read = pool
        .reserve_target_set(owner, &requests)
        .and_then(|pins| pins.try_read())
        .map_err(|e| frame(RetentionPhase::Prepare, e))?;
    let result = (|| {
        let mut stamps = [None; 5];
        for (at, role) in roles.into_iter().enumerate() {
            let target = requests[at];
            let output = &mut before[role];
            stamps[role] = Some(
                read.copy_snapshot(at, output)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
            );
            let canonical = canonical_image(arena, target.identity.page)
                .map_err(|cause| fatal(storage, RetentionPhase::Census, cause))?;
            let header = page::decode_page(output, target.identity, target.kind)
                .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?
                .header();
            if output != canonical || header.level != target.level {
                return Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
        }
        // Candidate already checked whole-tree membership; independently
        // bind these exact physical requests to the latched allocator.
        let allocator = allocation_page::decode_allocation_page(
            &before[0],
            observed.targets[0].identity,
            storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(e),
            )
        })?;
        for target in requests {
            let index = usize::try_from(target.identity.page.page_no)
                .map_err(|_| fatal(storage, RetentionPhase::Census, RetentionCause::Membership))?;
            if allocator.entry(index).map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Allocation(e),
                )
            })? != (AllocationEntry::Allocated {
                generation: target.identity.page.incarnation,
                object: target.identity.owner,
                object_generation: target.identity.owner_generation,
                kind: target.kind,
            }) {
                return Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Membership,
                ));
            }
        }
        Ok(stamps)
    })();
    merge(
        result,
        read.release_checked()
            .map_err(|e| frame(RetentionPhase::Prepare, e)),
    )
}

// The canonical cut owns the image's identity. Range-check before indexing;
// candidate/page decoders check the full PageRef, owner, kind and level.
fn canonical_image(
    arena: &[u8],
    reference: PageRef,
) -> std::result::Result<&[u8; PAGE_BYTES], RetentionCause> {
    let index = usize::try_from(reference.page_no).map_err(|_| RetentionCause::Invariant)?;
    if index >= 64 || arena.len() < history::ARENA_END {
        return Err(RetentionCause::Invariant);
    }
    Ok(history::arena_page(arena, index))
}

fn request(header: page::PageHeader) -> TargetRequest {
    TargetRequest {
        identity: header.identity,
        kind: header.kind,
        level: header.level,
    }
}

fn check_new_destinations<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    observed: &split_frame::Observation,
    replay: &[u8; RETENTION_REPLAY_BYTES],
) -> Result<()> {
    vacant_destinations(observed, replay)
        .map_err(|cause| fatal(storage, RetentionPhase::Census, cause))
}

fn vacant_destinations(
    observed: &split_frame::Observation,
    replay: &[u8; RETENTION_REPLAY_BYTES],
) -> std::result::Result<(), RetentionCause> {
    let count = if observed.operation == smo::Operation::RootSplit {
        2
    } else {
        1
    };
    for header in &observed.targets[3..3 + count] {
        let index =
            usize::try_from(header.identity.page.page_no).map_err(|_| RetentionCause::Invariant)?;
        if index >= 64
            || history::arena_page(replay, index).iter().any(|b| *b != 0)
            || replay[history::AFTER_END + index * 352..history::AFTER_END + (index + 1) * 352]
                .iter()
                .any(|b| *b != 0)
        {
            return Err(RetentionCause::Invariant);
        }
    }
    Ok(())
}

/// Source values for the original preparation only. Caller backing, resident
/// core/ports and the eventual live claim/install continuation are separate.
#[derive(Debug)]
struct SourceCensus {
    outer: usize,
    scope: usize,
    allocation: usize,
    semantic: usize,
    physical: usize,
    cleanup: usize,
    replay: usize,
    returning: usize,
    peak: usize,
}

fn source_census<W: WriterNamespace>() -> SourceCensus {
    use std::mem::size_of;
    type Packet = (
        split_frame::Observation,
        [TargetRequest; 5],
        [u8; 5],
        [Option<FrameStamp>; 5],
    );
    let outer = size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 2 * size_of::<WalPrefix>()
        + size_of::<OwnerIdentity>()
        + size_of::<ControlCut>()
        + size_of::<ObjectHandle>()
        + size_of::<PageRef>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 16 * size_of::<&[u8]>()
        + 12 * size_of::<usize>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>();
    let scope = size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>();
    let semantic = size_of::<candidate::Inputs<'_>>()
        + 2 * size_of::<Result<split_frame::Observation>>()
        + 2 * size_of::<Option<crate::storage::semantic_row::structure::bank::FinishedSplit>>()
        + 8 * size_of::<&[u8]>()
        + split_frame::source_bytes();
    let allocation = 4 * size_of::<AllocationEntry>()
        + 3 * size_of::<allocation_page::AllocationPage<'_>>()
        + 3 * size_of::<std::result::Result<AllocationEntry, allocation_page::AllocationError>>()
        + 3 * size_of::<
            std::result::Result<
                allocation_page::AllocationPage<'_>,
                allocation_page::AllocationError,
            >,
        >()
        + 2 * size_of::<page::PageView<'_>>()
        + 3 * size_of::<page::PageHeader>()
        + allocation_page::ALLOCATION_ENTRY_BYTES
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<usize>()
        + crate::storage::semantic_catalog::page_read_codec_bytes()
        + 1536; // SHA block/output/compression closure inside page decode
    let physical = size_of::<candidate::Inputs<'_>>()
        + 2 * size_of::<Option<crate::storage::semantic_row::structure::bank::FinishedSplit>>()
        + 2 * size_of::<Result<split_frame::Observation>>()
        + size_of::<split_frame::Observation>()
        + size_of::<[TargetRequest; 5]>()
        + size_of::<[usize; 5]>()
        + 3 * size_of::<Result<[Option<FrameStamp>; 5]>>()
        + size_of::<[TargetRequest; 4]>()
        + size_of::<[usize; 4]>()
        + size_of::<dirty_frame::ReadTargets<'_, '_, '_, '_, '_, '_, 4>>()
        + size_of::<[Option<FrameStamp>; 5]>()
        + 3 * size_of::<TargetRequest>()
        + size_of::<page::PageHeader>()
        + 2 * size_of::<Result<FrameStamp>>()
        + 16 * size_of::<&[u8]>()
        + 512
        + dirty_frame::target_set_call_bytes::<4>().max(allocation);
    let cleanup = size_of::<Packet>()
        + 3 * size_of::<Result<Packet>>()
        + history::runtime::lookup_source_census().returning;
    let processing = outer
        + scope
        + semantic
            .max(physical)
            .max(control::structural_bank_source_bytes())
            .max(cleanup);
    let replay = outer + scope + history::source_bytes::<W>().peak;
    let returning = outer
        + size_of::<Packet>()
        + size_of::<PreparedStructural<'_, '_, '_>>()
        + 2 * size_of::<Result<PreparedStructural<'_, '_, '_>>>()
        + 2048; // hashing closure and check/return scalars, no page copy
    // The unchanged entry is the same original entry used by Catalog. Its
    // existing conservative prepare/replay bound covers check_ledger and core.
    let peak = replay
        .max(processing)
        .max(returning)
        .max(super::catalog_live_source_bytes::<W>().prepare_replay);
    SourceCensus {
        outer,
        scope,
        allocation,
        semantic,
        physical,
        cleanup,
        replay,
        returning,
        peak,
    }
}

pub(in super::super::super) fn source_bytes<W: WriterNamespace>() -> usize {
    source_census::<W>().peak
}

#[cfg(test)]
type TestStructuralParts<'a> = (
    &'a [u8],
    &'a [[u8; PAGE_BYTES]; 5],
    &'a [TargetRequest; 5],
    &'a [u8; 5],
    &'a [Option<FrameStamp>; 5],
);

#[cfg(test)]
impl PreparedStructural<'_, '_, '_> {
    pub(in crate::storage) fn test_parts(&self) -> TestStructuralParts<'_> {
        (
            self.frame,
            self.after,
            &self.requests,
            &self.roles,
            &self.stamps,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal_frame::{FrameKind, WalFrameHeader, WalStreamIdentity};
    use std::mem::size_of;

    #[test]
    fn structural_prepare_second_destination_and_inactive_child_are_bounded() {
        // Destination-slice helper only: these observation fields do not
        // construct a canonical source, claim or prepared capability.
        let identity = crate::storage::page_io::test_support::expected();
        let header = page::PageHeader {
            identity,
            kind: PageKind::RowLeaf,
            level: 0,
            lsn: PageLsn(7),
        };
        let targets = [0, 10, 11, 17, 18].map(|n| page::PageHeader {
            identity: PageIdentity {
                page: PageRef {
                    page_no: n,
                    ..identity.page
                },
                ..identity
            },
            ..header
        });
        let mut observed = split_frame::Observation {
            header: WalFrameHeader {
                stream: WalStreamIdentity {
                    database: identity.database,
                    timeline: 1,
                    writer_epoch: 1,
                },
                kind: FrameKind::System,
                lsn: 7,
                previous_lsn: 6,
                txn: None,
                previous_txn_lsn: 0,
            },
            operation: smo::Operation::RootSplit,
            frame_bytes: 84_744,
            targets,
        };
        let mut replay: Box<[u8; RETENTION_REPLAY_BYTES]> = vec![0; RETENTION_REPLAY_BYTES]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        vacant_destinations(&observed, &replay).unwrap();
        for offset in [18 * PAGE_BYTES + 72, history::AFTER_END + 18 * 352 + 304] {
            replay[offset] = 1;
            let before = content_digest(replay.as_slice());
            assert!(vacant_destinations(&observed, &replay).is_err());
            assert_eq!(content_digest(replay.as_slice()), before);
            observed.operation = smo::Operation::ChildSplit;
            vacant_destinations(&observed, &replay).unwrap();
            observed.operation = smo::Operation::RootSplit;
            replay[offset] = 0;
        }
        observed.targets[3].identity.page.page_no = 64;
        assert!(vacant_destinations(&observed, &replay).is_err());
    }

    #[test]
    fn structural_prepare_source_inventory_covers_read_decode_and_return() {
        type W = crate::storage::wal_io::plan4_test_support::Writer;
        let c = source_census::<W>();
        // Independent inventory of simultaneous decoder values from
        // allocation_page::{decode_allocation_page,parse_inventory,entry}.
        let decoder = size_of::<allocation_page::AllocationPage<'_>>()
            + size_of::<
                std::result::Result<
                    allocation_page::AllocationPage<'_>,
                    allocation_page::AllocationError,
                >,
            >()
            + 2 * size_of::<AllocationEntry>()
            + 3 * size_of::<std::result::Result<AllocationEntry, allocation_page::AllocationError>>(
            )
            + 2 * size_of::<page::PageView<'_>>()
            + 2 * size_of::<page::PageHeader>()
            + size_of::<page::PageIdentity>()
            + size_of::<crate::api::Namespace>()
            + size_of::<[u8; 96]>()
            + 16 * size_of::<&[u8]>()
            + size_of::<[usize; 8]>()
            + semantic_catalog::page_read_codec_bytes()
            + 1536;
        assert!(decoder <= c.allocation, "decoder {decoder} > {:?}", c);
        let physical = size_of::<dirty_frame::ReadTargets<'_, '_, '_, '_, '_, '_, 4>>()
            + size_of::<[TargetRequest; 4]>()
            + size_of::<[usize; 4]>()
            + size_of::<[Option<FrameStamp>; 5]>()
            + 2 * size_of::<Result<[Option<FrameStamp>; 5]>>()
            + size_of::<split_frame::Observation>()
            + size_of::<candidate::Inputs<'_>>()
            + 16 * size_of::<&[u8]>()
            + decoder;
        assert!(physical <= c.physical, "physical {physical} > {:?}", c);
        let returned = size_of::<PreparedStructural<'_, '_, '_>>()
            + 2 * size_of::<Result<PreparedStructural<'_, '_, '_>>>()
            + 1536;
        assert!(returned <= c.returning);
        assert!(c.peak >= c.outer + c.scope + c.physical.max(c.cleanup).max(c.semantic));
        assert!(c.peak >= c.replay.max(c.returning));
        println!(
            "STRUCTURAL_PREPARE_SOURCE {c:?} independent_decoder={decoder} physical={physical} returned={returned}"
        );
    }
}
