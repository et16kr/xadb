//! The three original XOI1 items of one Catalog reservation. A group-local
//! ordinal is always zero; the original global inventory resolves its own
//! owner ordinal separately. These bytes are observations, never reservations.
use super::{Error, Operation, Result, format::CatalogJob, put16, put64};
use crate::storage::{obligation::InventoryClaim, retention_page::catalog::JobState};
use foundation::codec::{ContentHasher, content_digest};

pub(crate) const ITEM_BYTES: usize = 96;
pub(crate) const ITEMS_BYTES: usize = 3 * ITEM_BYTES;
pub(crate) const DELTAS_BYTES: usize = 3 * 208;

/// Each identity and its change LSN come from the same stable Reserved job.
/// The caller must additionally prove its actual allocator/Free Undo before
/// image and original inventory closure before admitting a transition.
pub(crate) fn items<'a, J: CatalogJob<'a>>(
    job: J,
    output: &mut [u8; ITEMS_BYTES],
) -> Result<InventoryClaim> {
    if job.state() != JobState::RootReserved || job.lease_lsn() != job.last_step() {
        return Err(Error::OrderMismatch);
    }
    let pair = job.pair().ok_or(Error::IdentityMismatch)?;
    let undo = job.undo().ok_or(Error::IdentityMismatch)?;
    if pair[0].file != pair[1].file
        || pair[0].page_no >= pair[1].page_no
        || pair[0].file != undo.page().file
        || pair.iter().any(|page| page.page_no == undo.page().page_no)
    {
        return Err(Error::IdentityMismatch);
    }
    output.fill(0);
    for (ordinal, item) in output.chunks_exact_mut(ITEM_BYTES).enumerate() {
        item[0] = if ordinal < 2 { 1 } else { 2 };
        item[1] = 2; // Structural, not the System count-one category.
        if ordinal < 2 {
            pair[ordinal].encode(&mut item[8..32])?;
        } else {
            undo.encode(&mut item[8..48])?;
        }
        put64(item, 64, 1);
        put64(item, 72, job.lease_lsn());
    }
    Ok(InventoryClaim {
        change_lsn: job.lease_lsn(),
        items: 3,
        digest: content_digest(output),
        growth_extents: 0,
    })
}

/// Empty is an actual zero-item digest, including after consume/release.
pub(crate) fn empty(change_lsn: u64) -> Result<InventoryClaim> {
    if change_lsn == 0 {
        return Err(Error::InvalidInput);
    }
    Ok(InventoryClaim {
        change_lsn,
        items: 0,
        digest: content_digest(&[]),
        growth_extents: 0,
    })
}

/// Reserve emits absent -> item, Forward consumes item -> absent, and Cancel
/// releases item -> absent. Bytes are reconstructed from the original job,
/// never copied from a purported candidate delta.
pub(crate) fn deltas<'a, J: CatalogJob<'a>>(
    operation: Operation,
    job: J,
    output: &mut [u8; DELTAS_BYTES],
) -> Result<InventoryClaim> {
    let action = match operation {
        Operation::Reserve => 1,
        Operation::Forward => 2,
        Operation::Cancel => 3,
        _ => return Err(Error::InvalidInput),
    };
    let mut encoded = [0; ITEMS_BYTES];
    let claim = items(job, &mut encoded)?;
    output.fill(0);
    for (delta, item) in output
        .chunks_exact_mut(208)
        .zip(encoded.chunks_exact(ITEM_BYTES))
    {
        delta[0] = action;
        delta[1] = if action == 1 { 2 } else { 1 };
        let at = if action == 1 { 112 } else { 16 };
        delta[at..at + ITEM_BYTES].copy_from_slice(item);
    }
    Ok(claim)
}

/// Rewrite only the global owner ordinal. The digest remains normalized to
/// ordinal zero, as required by the original ownership verifier.
pub(crate) fn global_items<'a, J: CatalogJob<'a>>(
    job: J,
    owner_ordinal: u16,
    output: &mut [u8; ITEMS_BYTES],
) -> Result<InventoryClaim> {
    if owner_ordinal >= 36 {
        return Err(Error::InvalidInput);
    }
    let claim = items(job, output)?;
    for item in output.chunks_exact_mut(ITEM_BYTES) {
        put16(item, 2, owner_ordinal);
    }
    Ok(claim)
}

pub(crate) fn normalized_digest(items: &[[u8; ITEM_BYTES]]) -> [u8; 32] {
    let mut hash = ContentHasher::new();
    for item in items {
        let mut normalized = *item;
        normalized[2..4].fill(0);
        hash.update(&normalized);
    }
    hash.digest()
}
