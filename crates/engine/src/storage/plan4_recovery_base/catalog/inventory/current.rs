//! Canonical current XOI projection from borrowed original/projected rows. This
//! codec neither authenticates the rows nor issues inventory or INIT authority.
use super::{Mode, finish, item_slot, owner_slot};
use crate::storage::{
    allocation_page::{self, AllocationEntry, ReservationOwner, SystemWorkRef},
    control::Profile,
    obligation::{self, OwnerState, RemainingBudget, WorkPhase},
    page::{PageIdentity, PageRef},
    plan4_recovery_base::{Error, PAGE_BYTES, Result, first_file, one, put, put16},
    retention_obligation,
    retention_page::{catalog as queue, row as row_queue},
    semantic_catalog,
    transaction_record::TxnRef,
};
use foundation::identity::ObjectId;

/// Only borrowed current observations cross this boundary. A projected caller
/// supplies the exact verified successor rows; a replay caller supplies its
/// accepted canonical rows. Both retain their separate source proofs.
pub(in crate::storage) trait InventoryRows {
    fn transaction(&self, index: usize) -> Option<&[u8; 1536]>;
    fn allocation(&self) -> Option<&[u8; 1536]>;
    fn aging(&self) -> Option<&[u8; 1536]>;
    fn root(&self) -> &[u8; PAGE_BYTES];
    fn allocator(&self) -> &[u8; PAGE_BYTES];
    fn job(&self, ordinal: usize) -> Result<&[u8]>;
}

fn charge(total: &mut (u64, u64), rows: &[RemainingBudget; 5]) -> Result<()> {
    for row in rows {
        total.0 = total.0.checked_add(row.wal).ok_or(Error::BudgetMismatch)?;
        total.1 = total
            .1
            .checked_add(row.steps)
            .ok_or(Error::BudgetMismatch)?;
    }
    Ok(())
}

/// Returns current remaining WAL/control credit. Aging already owns each Job's
/// cleanup debt, so Jobs are not charged a second time. Output is discardable
/// on any error; generation/capture must come from the caller's actual source.
pub(in crate::storage) fn encode_current(
    profile: Profile,
    generation: u64,
    capture: u64,
    rows: &impl InventoryRows,
    output: &mut [u8; PAGE_BYTES],
) -> Result<(u64, u64)> {
    encode(Mode::Catalog2, profile, generation, capture, rows, output)
}

/// The exact Profile3 entry: `rows` carries a Root3 page and Job3 cells.
pub(in crate::storage) fn encode_row_current(
    profile: Profile,
    generation: u64,
    capture: u64,
    rows: &impl InventoryRows,
    output: &mut [u8; PAGE_BYTES],
) -> Result<(u64, u64)> {
    encode(Mode::Row3, profile, generation, capture, rows, output)
}

fn encode(
    mode: Mode,
    profile: Profile,
    generation: u64,
    capture: u64,
    rows: &impl InventoryRows,
    output: &mut [u8; PAGE_BYTES],
) -> Result<(u64, u64)> {
    // The exact format gate stays in `finish`, as in the original order.
    output.fill(0);
    let mut future = (0, 0);
    let mut used = 0u32;
    let mut transactions = 0usize;
    let mut previous = None;
    loop {
        let mut selected = None;
        for index in 0..32 {
            if used & (1u32 << index) != 0 {
                continue;
            }
            let Some(bytes) = rows.transaction(index) else {
                continue;
            };
            let actor = TxnRef::decode(&bytes[24..72])?;
            let key = [
                u64::from(actor.id().writer_epoch),
                actor.id().local,
                actor.generation().get(),
            ];
            if selected.is_none_or(|(_, prior)| key < prior) {
                selected = Some((index, key));
            }
        }
        let Some((index, key)) = selected else { break };
        if previous.is_some_and(|prior| prior >= key) {
            return Err(Error::IdentityMismatch);
        }
        let bytes = rows.transaction(index).ok_or(Error::Corrupt)?;
        let actor = TxnRef::decode(&bytes[24..72])?;
        let decoded = obligation::decode_descriptor(
            bytes,
            ReservationOwner::Transaction(actor),
            retention_obligation::profile_reference(profile),
        )?;
        if !matches!(decoded.owner, OwnerState::Transaction(_)) {
            return Err(Error::IdentityMismatch);
        }
        charge(&mut future, &decoded.remaining)?;
        owner_slot(output, transactions)?.copy_from_slice(&bytes[16..72]);
        used |= 1u32 << index;
        previous = Some(key);
        transactions += 1;
    }
    let mut owners = transactions;
    let mut allocation = None;
    for (is_aging, bytes) in [(false, rows.allocation()), (true, rows.aging())] {
        let Some(bytes) = bytes else { continue };
        let decoded = if is_aging {
            match mode {
                Mode::Catalog2 => {
                    retention_obligation::catalog::decode(bytes, profile, rows.root())?.descriptor()
                }
                Mode::Row3 => {
                    retention_obligation::row::decode(bytes, profile, rows.root())?.descriptor()
                }
            }
        } else {
            let owner = SystemWorkRef::decode(&bytes[24..72])?;
            obligation::decode_descriptor(
                bytes,
                ReservationOwner::System(owner),
                retention_obligation::profile_reference(profile),
            )?
        };
        let OwnerState::System(work) = decoded.owner else {
            return Err(Error::IdentityMismatch);
        };
        if work.phase == WorkPhase::Finished {
            continue;
        }
        charge(&mut future, &decoded.remaining)?;
        if !is_aging {
            allocation = Some((
                owners,
                work.reference,
                decoded.inventory.change_lsn,
                decoded.inventory.items,
            ));
        }
        owner_slot(output, owners)?.copy_from_slice(&bytes[16..72]);
        owners += 1;
    }
    let mut count = 0;
    for ordinal in 0..64 {
        if mode == Mode::Row3 {
            let job = row_queue::decode_job(rows.job(ordinal)?, profile.spec().namespace)?;
            if job.state() != row_queue::JobState::RootReserved {
                continue;
            }
            // Job3 Catalog-scope items need the Profile3 semantic projection
            // (U2-C3); a Row-scope RowReserve owns one UndoSlot item.
            if job.scope() != Some(row_queue::JobScope::Row) {
                return Err(Error::Unsupported);
            }
            if count != 0 {
                return Err(Error::IdentityMismatch);
            }
            row_undo_item(output, transactions, job)?;
            count = 1;
            continue;
        }
        let job = queue::decode_job(rows.job(ordinal)?, profile.spec().namespace)?;
        if job.state() != queue::JobState::RootReserved {
            continue;
        }
        if count != 0 {
            return Err(Error::IdentityMismatch);
        }
        let actor = job.source().ok_or(Error::IdentityMismatch)?;
        let mut owner = None;
        for index in 0..transactions {
            if TxnRef::decode(&owner_slot(output, index)?[8..56])? == actor
                && owner.replace(index).is_some()
            {
                return Err(Error::IdentityMismatch);
            }
        }
        let mut items = [0; semantic_catalog::inventory::ITEMS_BYTES];
        semantic_catalog::inventory::global_items(
            job,
            owner.ok_or(Error::IdentityMismatch)? as u16,
            &mut items,
        )?;
        for index in 0..3 {
            item_slot(output, index)?.copy_from_slice(&items[index * 96..(index + 1) * 96]);
        }
        count = 3;
    }
    let identity = PageIdentity {
        database: profile.spec().namespace.database,
        page: PageRef {
            file: first_file(),
            page_no: 0,
            incarnation: one(),
        },
        owner: ObjectId::new(1).expect("allocator object"),
        owner_generation: one(),
    };
    let allocator = allocation_page::decode_allocation_page(
        rows.allocator(),
        identity,
        profile.spec().namespace,
    )?;
    let mut system_items = 0;
    for index in 0..64 {
        if let AllocationEntry::Reserved {
            generation,
            claimant: ReservationOwner::System(claimant),
            ..
        } = allocator.entry(index)?
        {
            if count != 0 {
                return Err(Error::IdentityMismatch);
            }
            let (owner, work, change, expected_items) =
                allocation.ok_or(Error::IdentityMismatch)?;
            if claimant != work || expected_items != 1 || change == 0 {
                return Err(Error::IdentityMismatch);
            }
            let item = item_slot(output, 0)?;
            item.fill(0);
            item[0] = 1;
            item[1] = 3;
            put16(item, 2, owner as u16);
            PageRef {
                file: first_file(),
                page_no: index as u64,
                incarnation: generation,
            }
            .encode(&mut item[8..32])?;
            put(item, 64, 1);
            put(item, 72, change);
            count = 1;
            system_items = 1;
        }
    }
    if allocation.is_some_and(|(_, _, _, items)| items != system_items) {
        return Err(Error::IdentityMismatch);
    }
    finish(mode, profile, generation, capture, owners, count, output)?;
    Ok(future)
}

/// Profile3 Row mode: a RowReserve's `UndoSlot(U)` item in the Forward row's
/// category, owned by its actor's ATT row and born at the lease.
fn row_undo_item(
    output: &mut [u8; PAGE_BYTES],
    transactions: usize,
    job: row_queue::JobView<'_>,
) -> Result<()> {
    let actor = job.source().ok_or(Error::IdentityMismatch)?;
    let mut owner = None;
    for index in 0..transactions {
        if TxnRef::decode(&owner_slot(output, index)?[8..56])? == actor
            && owner.replace(index).is_some()
        {
            return Err(Error::IdentityMismatch);
        }
    }
    let owner = owner.ok_or(Error::IdentityMismatch)?;
    let undo = job.undo().ok_or(Error::IdentityMismatch)?;
    let item = item_slot(output, 0)?;
    item.fill(0);
    item[0] = 2;
    item[1] = 2;
    put16(item, 2, owner as u16);
    undo.encode(&mut item[8..48])?;
    put(item, 64, 1);
    put(item, 72, job.lease_lsn());
    Ok(())
}

/// Typed nested codec closure. Its sum is the Catalog2 mode's; the Profile3
/// Row mode has no separate bound yet. Caller-owned rows/adapter, output page and any
/// retained current/projected views are accounted by the calling phase.
pub(in crate::storage) fn current_source_bytes() -> usize {
    use foundation::codec::ContentHasher;
    use std::mem::size_of;
    let outer = semantic_catalog::inventory::ITEMS_BYTES
        + 2 * size_of::<obligation::DescriptorSpec>()
        + 2 * size_of::<std::result::Result<obligation::DescriptorSpec, obligation::ObligationError>>(
        )
        + 2 * size_of::<TxnRef>()
        + size_of::<Option<(usize, [u64; 3])>>()
        + size_of::<Option<[u64; 3]>>()
        + size_of::<Option<(usize, SystemWorkRef, u64, u64)>>()
        + size_of::<allocation_page::AllocationPage<'_>>()
        + size_of::<
            std::result::Result<
                allocation_page::AllocationPage<'_>,
                allocation_page::AllocationError,
            >,
        >()
        + 2 * size_of::<queue::JobView<'_>>()
        + 2 * size_of::<Result<queue::JobView<'_>>>()
        + 8 * size_of::<Result<()>>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>();
    outer
        + retention_obligation::catalog::source_bytes()
            .max(queue::source_bytes())
            .max(4 * size_of::<ContentHasher>() + 1536)
}
