//! Canonical scope4 replay on the original accepted arena. The raw core only
//! selects arguments; fresh comparison precedes every historical INIT record.
//! This module issues neither dirty INIT nor pool installation authority.
use super::{catalog, control, recovery, *};
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    page::{PageHeader, PageRef},
    retention_page::row::JobCells,
    semantic_row::structure::{self as semantic, LayoutView, Operation, candidate, frame, unlink},
};

type Observed<T> = std::result::Result<T, RetentionCause>;

fn put(bytes: &mut [u8], at: usize, n: u64) {
    bytes[at..at + 8].copy_from_slice(&n.to_le_bytes());
}

/// Only called after same-call canonical comparison, with its fresh target
/// and image. Both root claims are prepared before publishing either one.
fn initial_record(
    view: LayoutView<'_>,
    role: usize,
    header: PageHeader,
    image: &[u8; PAGE_BYTES],
    frame_digest: &[u8; 32],
    out: &mut [u8; 352],
) -> Observed<()> {
    let core = view.core();
    let object = core.object();
    let frame = view.header();
    let expected_page = match role {
        3 => core.right(),
        4 if view.operation() == Operation::RootSplit => core.parent(),
        _ => return Err(RetentionCause::Invariant),
    };
    if header.identity.page != expected_page
        || header.identity.owner != object.object
        || header.identity.owner_generation != object.generation
        || object.generation.get() != 1
        || object.schema_epoch.get() != 1
        || header.lsn.0 != frame.lsn
        || page::decode_page(image, header.identity, header.kind)
            .map_err(RetentionCause::Page)?
            .header()
            != header
    {
        return Err(RetentionCause::Invariant);
    }
    let owner = SystemWorkRef::new(
        crate::api::Namespace {
            database: frame.stream.database,
            timeline: frame.stream.timeline,
        },
        SystemWorkKind::StructuralChange,
        frame.lsn,
        Generation::new(u64::from(frame.stream.writer_epoch))
            .map_err(|_| RetentionCause::Invariant)?,
    )
    .map_err(RetentionCause::Allocation)?;
    out.fill(0);
    out[..5].copy_from_slice(&[
        1,
        if role == 3 { 5 } else { 6 },
        header.kind as u8,
        header.level,
        1,
    ]);
    header
        .identity
        .page
        .encode(&mut out[8..32])
        .map_err(RetentionCause::Page)?;
    for (at, value) in [
        (32, object.object.get()),
        (40, object.generation.get()),
        (48, frame.lsn),
        (176, frame.lsn),
        (184, object.object.get()),
        (192, object.generation.get()),
        (200, object.schema_epoch.get()),
        (280, view.operation() as u64),
        (288, core.revision()),
        (296, core.revision() + 1),
    ] {
        put(out, at, value);
    }
    out[56..60].copy_from_slice(&frame.stream.writer_epoch.to_le_bytes());
    out[64..96].copy_from_slice(frame_digest);
    out[96..128].copy_from_slice(&content_digest(image));
    owner
        .encode(&mut out[128..176])
        .map_err(RetentionCause::Allocation)?;
    core.anchor()
        .encode(&mut out[208..232])
        .map_err(RetentionCause::Page)?;
    core.left()
        .encode(&mut out[232..256])
        .map_err(RetentionCause::Page)?;
    if view.operation() == Operation::RootSplit {
        let peer = if role == 3 {
            core.parent()
        } else {
            core.right()
        };
        peer.encode(&mut out[256..280])
            .map_err(RetentionCause::Page)?;
    }
    out[304..336].copy_from_slice(&content_digest(core.bytes()));
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "disjoint original canonical replay loans"
)]
pub(in super::super) fn apply(
    profile: Profile,
    expected: FrameExpectation,
    raw: &[u8],
    arena: &mut [u8],
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    bank: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 8],
    claims: &mut [[u8; 352]; 64],
) -> Observed<recovery::RetentionFrameObservation> {
    if arena.len() != ARENA_END || bank.len() != BANK_END - JOB_END {
        return Err(RetentionCause::Invariant);
    }
    // The layout is moved out of its decode Result once; the bank row's and
    // the candidate's Results are held as returned and borrowed, so no
    // second copy sits beside the comparison.
    let view = match semantic::decode_layout(raw, expected, profile) {
        Ok(view) => view,
        Err(error) => return Err(RetentionCause::RowSemantic(error)),
    };
    let observation = {
        let prior_row = control::row(bank, control::STRUCTURAL_SLOT);
        let observed_prior = control::observe_structural_bank_row(
            profile,
            expected.previous_lsn,
            expected.stream.writer_epoch,
            prior_row,
        );
        let prior = match &observed_prior {
            Ok(prior) => prior,
            Err(_) => return observed_prior.map(|_| unreachable!("the bank row was refused")),
        };
        let inputs = candidate::Inputs {
            profile,
            frame: expected,
            table: view.core().object(),
            source: view.core().left(),
            jobs: JobCells::original(cells),
            previous_high_water: prior.as_ref().map_or(0, |p| p.owner.id()),
            previous: prior.as_ref().map(|p| {
                (
                    p.owner,
                    catalog::raw_descriptor(prior_row).expect("checked bank"),
                )
            }),
        };
        frame::observe(
            &inputs,
            raw,
            (&mut after[..5]).try_into().expect("five after pages"),
            |reference| {
                catalog::image(arena, reference)
                    .map_err(|_| crate::storage::semantic_row::Error::IdentityMismatch)
            },
        )
    };
    let observed = match &observation {
        Ok(observed) => observed,
        Err(error) => return Err(RetentionCause::RowSemantic(*error)),
    };
    let mut result = recovery::RetentionFrameObservation {
        header: observed.header,
        changed: [None; 8],
        initialized: [None; 2],
        retired: [None; 2],
    };
    let mut new_claims = [[0; 352]; 2];
    let digest = content_digest(raw);
    for ordinal in 0..5 {
        let role = usize::from(view.native(ordinal).expect("validated native").role());
        let header = observed.targets[role];
        let index =
            usize::try_from(header.identity.page.page_no).map_err(|_| RetentionCause::Invariant)?;
        let request = recovery::request_from_image(profile, &after[role])?;
        if index >= 64
            || request.identity != header.identity
            || request.kind != header.kind
            || request.level != header.level
            || result.changed[..ordinal]
                .iter()
                .flatten()
                .any(|r| r.identity.page.page_no == header.identity.page.page_no)
        {
            return Err(RetentionCause::Invariant);
        }
        result.changed[ordinal] = Some(request);
        if role == 3 || role == 4 && observed.operation == Operation::RootSplit {
            if arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                .iter()
                .any(|b| *b != 0)
                || claims[index].iter().any(|b| *b != 0)
            {
                return Err(RetentionCause::Invariant);
            }
            initial_record(
                view,
                role,
                header,
                &after[role],
                &digest,
                &mut new_claims[role - 3],
            )?;
            result.initialized[role - 3] = Some(request);
        }
    }
    // No fallible operation follows. Keep the old arena/claims/bank intact
    // until all five targets and both possible INIT records have been checked.
    for (role, header) in observed.targets.iter().enumerate() {
        let at = header.identity.page.page_no as usize * PAGE_BYTES;
        arena[at..at + PAGE_BYTES].copy_from_slice(&after[role]);
    }
    for (side, request) in result.initialized.iter().enumerate() {
        if let Some(request) = request {
            claims[request.identity.page.page_no as usize] = new_claims[side];
        }
    }
    let row = control::row_mut(bank, control::STRUCTURAL_SLOT);
    row.fill(0);
    row[..1536].copy_from_slice(view.descriptor());
    put(row, 1536, observed.header.lsn);
    row[control::PHASE] = control::STABLE;
    Ok(result)
}

/// The scope4 dispatch: split and unlink frame lengths are disjoint
/// (84,744/84,640 against 35,248/19,032), and the chosen decoder checks the
/// opcode again (dev3-tree-smo.md#unlink-u1). The caller picks `apply` or
/// `apply_unlink` so the two arms never share a frame.
pub(in super::super) fn is_unlink(raw: &[u8]) -> bool {
    raw.len() == Operation::LeafUnlink.frame_bytes()
        || raw.len() == Operation::RootCollapse.frame_bytes()
}

/// Canonical LeafUnlink/RootCollapse replay (dev3-tree-smo.md#준비효과복구):
/// the same-cut unlink observer recomputes and byte-compares the frame,
/// then only the changed anchor (and LeafUnlink parent) are published.
/// Retired pages stay Allocated with their claims, so the observation's
/// `retired` (which means Free) and `initialized` stay empty.
#[inline(never)]
pub(in super::super) fn apply_unlink(
    profile: Profile,
    expected: FrameExpectation,
    raw: &[u8],
    arena: &mut [u8],
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    bank: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 2],
) -> Observed<recovery::RetentionFrameObservation> {
    if arena.len() != ARENA_END || bank.len() != BANK_END - JOB_END {
        return Err(RetentionCause::Invariant);
    }
    let view = match unlink::decode_layout(raw, expected, profile) {
        Ok(view) => view,
        Err(error) => return Err(RetentionCause::RowSemantic(error)),
    };
    let observation = {
        let prior_row = control::row(bank, control::STRUCTURAL_SLOT);
        let observed_prior = control::observe_structural_bank_row(
            profile,
            expected.previous_lsn,
            expected.stream.writer_epoch,
            prior_row,
        );
        let prior = match &observed_prior {
            Ok(prior) => prior,
            Err(_) => return observed_prior.map(|_| unreachable!("the bank row was refused")),
        };
        let inputs = candidate::Inputs {
            profile,
            frame: expected,
            table: view.core().object(),
            source: view.core().leaf(),
            jobs: JobCells::original(cells),
            previous_high_water: prior.as_ref().map_or(0, |p| p.owner.id()),
            previous: prior.as_ref().map(|p| {
                (
                    p.owner,
                    catalog::raw_descriptor(prior_row).expect("checked bank"),
                )
            }),
        };
        unlink::frame::observe(&inputs, raw, after, |reference| {
            catalog::image(arena, reference)
                .map_err(|_| crate::storage::semantic_row::Error::IdentityMismatch)
        })
    };
    let observed = match &observation {
        Ok(observed) => observed,
        Err(error) => return Err(RetentionCause::RowSemantic(*error)),
    };
    let mut result = recovery::RetentionFrameObservation {
        header: observed.header,
        changed: [None; 8],
        initialized: [None; 2],
        retired: [None; 2],
    };
    // after[0] is the anchor, after[1] the LeafUnlink parent (candidate
    // slots ANCHOR and PARENT); RootCollapse changes the anchor only.
    let changed = observed.operation.redo_records();
    for (slot, image) in after[..changed].iter().enumerate() {
        let header = observed.targets[slot];
        let request = recovery::request_from_image(profile, image)?;
        if header.identity.page.page_no >= 64
            || header.lsn.0 != observed.header.lsn
            || request.identity != header.identity
            || request.kind != header.kind
            || request.level != header.level
        {
            return Err(RetentionCause::Invariant);
        }
        result.changed[slot] = Some(request);
    }
    // No fallible operation follows: the arena and bank stay intact until
    // every changed target has been checked.
    for (slot, image) in after[..changed].iter().enumerate() {
        let at = observed.targets[slot].identity.page.page_no as usize * PAGE_BYTES;
        arena[at..at + PAGE_BYTES].copy_from_slice(image);
    }
    let row = control::row_mut(bank, control::STRUCTURAL_SLOT);
    row.fill(0);
    row[..1536].copy_from_slice(view.descriptor());
    put(row, 1536, observed.header.lsn);
    row[control::PHASE] = control::STABLE;
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
pub(in super::super) struct SourceCensus {
    pub(in super::super) semantic: usize,
    pub(in super::super) finishing: usize,
}

/// The two phases are sequential in apply: new_claims/result are declared
/// after the candidate returns. Original arena/after/frame loans are external.
/// The candidate's Observation is held once, in its returned Result, and
/// borrowed from there (dev3-tree-smo.md#current-leaf-consumers).
pub(in super::super) fn source_bytes() -> SourceCensus {
    use std::mem::size_of;
    let args = size_of::<(
        Profile,
        FrameExpectation,
        &[u8],
        &mut [u8],
        &[[u8; JOB_BYTES]; JOB_COUNT],
        &mut [u8],
        &mut [[u8; PAGE_BYTES]; 8],
        &mut [[u8; 352]; 64],
    )>();
    let compare = args
        + size_of::<LayoutView<'_>>()
        + size_of::<std::result::Result<LayoutView<'_>, crate::storage::semantic_row::Error>>()
        + size_of::<candidate::Inputs<'_>>()
        + size_of::<Observed<Option<semantic::bank::FinishedStructural>>>()
        + size_of::<&Option<semantic::bank::FinishedStructural>>()
        + size_of::<Option<(SystemWorkRef, &[u8; 1536])>>()
        + size_of::<std::result::Result<frame::Observation, crate::storage::semantic_row::Error>>()
        + size_of::<&frame::Observation>()
        + size_of::<(&[u8], &mut [[u8; PAGE_BYTES]; 5], PageRef)>()
        + 512;
    let split = compare + frame::source_bytes().max(control::structural_bank_source_bytes());
    let unlink = unlink_source_bytes();
    let dispatch = unlink_dispatch_bytes();
    let semantic = split.max(dispatch + unlink.semantic);
    let finishing = args
        + 2 * size_of::<LayoutView<'_>>()
        + 2 * size_of::<frame::Observation>()
        + 2 * size_of::<recovery::RetentionFrameObservation>()
        + 2 * size_of::<Observed<recovery::RetentionFrameObservation>>()
        + size_of::<[[u8; 352]; 2]>()
        + 3 * size_of::<TargetRequest>()
        + 3 * size_of::<Observed<TargetRequest>>()
        + 3 * size_of::<PageHeader>()
        + 3 * size_of::<SystemWorkRef>()
        + size_of::<semantic::CoreView<'_>>()
        + 4 * size_of::<[u8; 32]>()
        + 1024
        + crate::storage::semantic_catalog::page_read_codec_bytes()
        + size_of::<ContentHasher>()
        + 1536;
    SourceCensus {
        semantic,
        finishing: finishing.max(dispatch + unlink.finishing),
    }
}

/// The scope4 dispatch in `recovery::apply_frame` (UNL-C02): `is_unlink`
/// with its length tests, and the after2 prefix borrow with its `try_into`
/// Result. They exist only on the unlink path, for the whole `apply_unlink`
/// call (both phases); the split path keeps its arguments as before.
pub(in super::super) fn unlink_dispatch_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&[u8], bool, bool)>()
        + size_of::<&mut [[u8; PAGE_BYTES]; 2]>()
        + 2 * size_of::<
            std::result::Result<&mut [[u8; PAGE_BYTES]; 2], std::array::TryFromSliceError>,
        >()
}

/// `apply_unlink` (dev3-tree-smo.md#empty-leaf-unlink): a sibling frame of
/// `apply` under the scope4 dispatch, so the two never coexist. Its phases
/// follow `apply`'s rule: the compare values precede the observer's return;
/// `result`, the checked targets and the publication are declared after it,
/// beside the borrowed layout and observation. after2 is the caller's after
/// prefix; the returned observation is charged by the history frame return.
pub(in super::super) fn unlink_source_bytes() -> SourceCensus {
    use std::mem::size_of;
    let args = size_of::<(
        Profile,
        FrameExpectation,
        &[u8],
        &mut [u8],
        &[[u8; JOB_BYTES]; JOB_COUNT],
        &mut [u8],
        &mut [[u8; PAGE_BYTES]; 2],
    )>();
    let compare = args
        + size_of::<unlink::LayoutView<'_>>()
        + size_of::<std::result::Result<unlink::LayoutView<'_>, crate::storage::semantic_row::Error>>(
        )
        + size_of::<&[u8]>()
        + size_of::<Observed<Option<semantic::bank::FinishedStructural>>>()
        + size_of::<&Option<semantic::bank::FinishedStructural>>()
        + size_of::<candidate::Inputs<'_>>()
        + size_of::<Option<(SystemWorkRef, &[u8; 1536])>>()
        + size_of::<
            std::result::Result<unlink::frame::Observation, crate::storage::semantic_row::Error>,
        >()
        + size_of::<&unlink::frame::Observation>()
        + size_of::<&[u8]>()
        + 512;
    let finishing = args
        + size_of::<unlink::LayoutView<'_>>()
        + size_of::<
            std::result::Result<unlink::frame::Observation, crate::storage::semantic_row::Error>,
        >()
        + size_of::<&unlink::frame::Observation>()
        + size_of::<recovery::RetentionFrameObservation>()
        + size_of::<Observed<recovery::RetentionFrameObservation>>()
        + 2 * size_of::<(usize, PageHeader, TargetRequest, Observed<TargetRequest>)>()
        + 512
        + crate::storage::semantic_catalog::page_read_codec_bytes()
        + size_of::<ContentHasher>()
        + 1536;
    SourceCensus {
        semantic: compare + unlink::source_bytes().max(control::structural_bank_source_bytes()),
        finishing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn structural_replay_source_inventory_covers_compare_and_claim_mint_phases() {
        let args = size_of::<(
            Profile,
            FrameExpectation,
            &[u8],
            &mut [u8],
            &[[u8; JOB_BYTES]; JOB_COUNT],
            &mut [u8],
            &mut [[u8; PAGE_BYTES]; 8],
            &mut [[u8; 352]; 64],
        )>();
        let compare = args
            + size_of::<LayoutView<'_>>()
            + size_of::<Observed<LayoutView<'_>>>()
            + size_of::<&[u8]>()
            + size_of::<Option<semantic::bank::FinishedStructural>>()
            + size_of::<Observed<Option<semantic::bank::FinishedStructural>>>()
            + size_of::<candidate::Inputs<'_>>()
            + size_of::<std::result::Result<frame::Observation, crate::storage::semantic_row::Error>>(
            )
            + size_of::<&frame::Observation>()
            + size_of::<(&[u8], PageRef)>()
            + 256
            + frame::source_bytes().max(control::structural_bank_source_bytes());
        let retained = args
            + size_of::<LayoutView<'_>>()
            + size_of::<frame::Observation>()
            + size_of::<recovery::RetentionFrameObservation>()
            + size_of::<[[u8; 352]; 2]>()
            + size_of::<([u8; 32], usize, usize, PageHeader, TargetRequest)>()
            + size_of::<Observed<TargetRequest>>()
            + 256;
        let mint = size_of::<(
            LayoutView<'_>,
            usize,
            PageHeader,
            &[u8; PAGE_BYTES],
            &[u8; 32],
            &mut [u8; 352],
        )>() + size_of::<(
            semantic::CoreView<'_>,
            foundation::identity::ObjectHandle,
            crate::storage::wal_frame::WalFrameHeader,
            PageRef,
            SystemWorkRef,
        )>() + size_of::<
            std::result::Result<SystemWorkRef, crate::storage::allocation_page::AllocationError>,
        >() + size_of::<crate::storage::page::PageView<'_>>()
            + size_of::<
                std::result::Result<
                    crate::storage::page::PageView<'_>,
                    crate::storage::page::PageError,
                >,
            >()
            + size_of::<ContentHasher>()
            + 1536
            + 256
            + crate::storage::semantic_catalog::page_read_codec_bytes();
        let returning = args
            + size_of::<LayoutView<'_>>()
            + size_of::<frame::Observation>()
            + size_of::<[[u8; 352]; 2]>()
            + size_of::<recovery::RetentionFrameObservation>()
            + size_of::<Observed<recovery::RetentionFrameObservation>>()
            + 256;
        let c = source_bytes();
        println!(
            "structural replay compare {compare}/{} finishing {}/{} return {returning}",
            c.semantic,
            retained + mint,
            c.finishing
        );
        assert!(compare <= c.semantic);
        assert!(retained + mint <= c.finishing && returning <= c.finishing);
    }

    /// UNL-R04 (dev3-tree-smo.md#unlink-u2): the unlink arm's own values
    /// across its nested observer and bank reads, then across its image
    /// reads after the observer returns; the scope4 census is the larger of
    /// the split and unlink arms in each phase.
    #[test]
    fn unl_r04_unlink_replay_arm_inventory_and_scope4_maximum() {
        type Unlinked =
            std::result::Result<unlink::frame::Observation, crate::storage::semantic_row::Error>;
        let args = size_of::<(
            Profile,
            FrameExpectation,
            &[u8],
            &mut [u8],
            &[[u8; JOB_BYTES]; JOB_COUNT],
            &mut [u8],
            &mut [[u8; PAGE_BYTES]; 2],
        )>();
        let compare = args
            + size_of::<(unlink::LayoutView<'_>, &[u8], candidate::Inputs<'_>)>()
            + size_of::<Observed<Option<semantic::bank::FinishedStructural>>>()
            + size_of::<Unlinked>()
            + 256
            + unlink::source_bytes().max(control::structural_bank_source_bytes());
        let finishing = args
            + size_of::<(unlink::LayoutView<'_>, Unlinked)>()
            + size_of::<recovery::RetentionFrameObservation>()
            + size_of::<(TargetRequest, Observed<TargetRequest>, PageHeader, usize)>()
            + 256
            + size_of::<crate::storage::page::PageView<'_>>()
            + crate::storage::semantic_catalog::page_read_codec_bytes()
            + size_of::<ContentHasher>()
            + 1536;
        let (arm, all) = (unlink_source_bytes(), source_bytes());
        println!(
            "unlink replay compare {compare}/{} finishing {finishing}/{} scope4 {}/{}",
            arm.semantic, arm.finishing, all.semantic, all.finishing
        );
        assert!(compare <= arm.semantic && finishing <= arm.finishing);
        assert!(arm.semantic <= all.semantic && arm.finishing <= all.finishing);
    }

    /// UNL-C02 (dev3-tree-smo.md#unlink-u2-census): the scope4 dispatch's own
    /// values (the frame length tests and the after2 prefix borrow) sit
    /// beside the whole unlink arm in both phases, and the scope4 census is
    /// still the split arm, so the history frame does not change.
    #[test]
    fn unl_c02_scope4_dispatch_beside_the_unlink_arm() {
        type Prefix<'a> = &'a mut [[u8; PAGE_BYTES]; 2];
        let dispatch = size_of::<(&[u8], bool, bool)>()
            + size_of::<Prefix<'_>>()
            + 2 * size_of::<std::result::Result<Prefix<'_>, std::array::TryFromSliceError>>();
        let (arm, all) = (unlink_source_bytes(), source_bytes());
        println!(
            "UNL_C02 dispatch {dispatch}/{} unlink {}+{dispatch}/{} finishing {}+{dispatch}/{}",
            unlink_dispatch_bytes(),
            arm.semantic,
            all.semantic,
            arm.finishing,
            all.finishing
        );
        assert!(dispatch <= unlink_dispatch_bytes());
        // Strictly below: the split arm stays the scope4 maximum.
        assert!(arm.semantic + unlink_dispatch_bytes() < all.semantic);
        assert!(arm.finishing + unlink_dispatch_bytes() < all.finishing);
    }
}
