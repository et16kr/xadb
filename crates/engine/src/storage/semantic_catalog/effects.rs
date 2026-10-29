//! Exact edge multiset for the single Missing Catalog root. The selected
//! native indices come from the actual sorted changed page/cell set.
use super::{Error, Operation, Result, format::CatalogJob, put16, put64};
use crate::storage::retention_page::catalog::JobState;

#[derive(Clone, Copy)]
pub(crate) struct EffectIndices {
    pub(crate) catalog: u16,
    pub(crate) status: u16,
    pub(crate) undo: u16,
}

pub(crate) fn reference_changes<'a, J: CatalogJob<'a>>(
    operation: Operation,
    job: J,
    indices: EffectIndices,
    edges: &mut [u8; 384],
    counts: &mut [u8; 192],
) -> Result<()> {
    let adding = match operation {
        Operation::Forward if job.state() == JobState::RootReserved => true,
        Operation::Clr | Operation::Prune if job.state() == JobState::RootRetained => false,
        _ => return Err(Error::OrderMismatch),
    };
    let shape = operation.shape(job.catalog_bytes() as usize)?;
    if [indices.catalog, indices.status, indices.undo]
        .iter()
        .any(|index| *index >= shape.native_count)
        || indices.catalog == indices.status
        || indices.catalog == indices.undo
        || indices.status == indices.undo
    {
        return Err(Error::InvalidInput);
    }
    let undo = job.undo().ok_or(Error::IdentityMismatch)?;
    let actor = job.source().ok_or(Error::IdentityMismatch)?;
    let mut undo_target = [0; 56];
    undo_target[0] = 1;
    undo.encode(&mut undo_target[8..48])?;
    put64(&mut undo_target, 48, job.undo_sequence());
    let mut txn_target = [0; 56];
    txn_target[0] = 2;
    actor.encode(&mut txn_target[8..56])?;

    // Physical ordering may put the Status before the Catalog on future
    // layouts. Source-role order is derived, not hardcoded to native3/4.
    let mut sources = [
        (indices.catalog, 3u8, &undo_target),
        (indices.catalog, 9u8, &txn_target),
        (indices.status, 10u8, &undo_target),
    ];
    sources.sort_unstable_by_key(|(index, role, _)| (*index, *role));
    edges.fill(0);
    for (edge, (index, role, target)) in edges.chunks_exact_mut(128).zip(sources) {
        put16(edge, 0, index);
        edge[2] = role;
        let at = if adding { 64 } else { 8 };
        edge[at..at + 56].copy_from_slice(target);
    }
    counts.fill(0);
    let count = &mut counts[..96];
    count[..56].copy_from_slice(&undo_target);
    put64(count, if adding { 64 } else { 56 }, 2);
    put16(count, 72, indices.undo);
    let count = &mut counts[96..];
    count[..56].copy_from_slice(&txn_target);
    put64(count, if adding { 64 } else { 56 }, 1);
    put16(count, 72, indices.status);
    Ok(())
}
