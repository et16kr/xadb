//! Descriptor bank, borrowed XOI1 root and the fail-closed System boundary of
//! the Base4 closure. No descriptor here is a reservation or replay authority.
use super::super::super::{
    AllocationTerminal, ControlKind, ControlRef, InventoryExpected, InventorySource, OwnerState,
    RemainingBudget, SystemWorkRef, WorkPhase, add_budget, obligation, ownership,
    retention_obligation, zero,
};
use super::*;
use crate::storage::transaction_record::TransactionState;

pub(super) fn raw(layout: LayoutView<'_>, slot: usize) -> Result<&[u8; 1536]> {
    if slot >= 36 {
        return Err(BaseError::InvalidInput.into());
    }
    Ok(layout.descriptor_bank()[slot * 1536..(slot + 1) * 1536]
        .try_into()
        .expect("fixed descriptor"))
}

pub(super) fn descriptor(layout: LayoutView<'_>, slot: usize) -> Result<Option<DescriptorSpec>> {
    let bytes = raw(layout, slot)?;
    if zero(bytes) {
        return Ok(None);
    }
    if slot == 34 {
        return Ok(Some(
            retention_obligation::row::decode(
                bytes,
                layout.expected.profile,
                image(layout.bytes, 1),
            )?
            .descriptor(),
        ));
    }
    let owner = if slot < 32 {
        ReservationOwner::Transaction(TxnRef::decode(&bytes[24..72])?)
    } else {
        ReservationOwner::System(SystemWorkRef::decode(&bytes[24..72]).map_err(BaseError::from)?)
    };
    Ok(Some(obligation::decode_descriptor(
        bytes,
        owner,
        retention_obligation::profile_reference(layout.expected.profile),
    )?))
}

pub(super) fn active_descriptor(layout: LayoutView<'_>, ordinal: usize) -> Result<DescriptorSpec> {
    let mut selected = 0;
    for slot in 0..36 {
        if let Some(spec) = descriptor(layout, slot)? {
            if matches!(spec.owner, OwnerState::System(p) if p.phase == WorkPhase::Finished) {
                continue;
            }
            if selected == ordinal {
                return Ok(spec);
            }
            selected += 1;
        }
    }
    Err(BaseError::InvalidInput.into())
}

/// Slots 0..31 are the dense ATT, 32 the last allocation owner and 34 Aging.
/// Returns (transactions, active Systems).
pub(super) fn validate(
    graph: &Graph<'_>,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<[usize; 2]> {
    let header = graph.layout.header();
    if graph.root.aging().is_some() != (header.system_high_water[2] == 1)
        || graph.root.apply() > header.capture_lsn
    {
        return Err(Error::Identity);
    }
    let mut transactions = 0;
    let mut systems = 0;
    let mut previous = None;
    let mut aggregate = [RemainingBudget::default(); 5];
    for slot in 0..36 {
        let Some(spec) = descriptor(graph.layout, slot)? else {
            if slot < header.active_transactions as usize
                || slot == 32 && header.system_high_water[0] != 0
                || slot == 34 && graph.root.aging().is_some()
            {
                return Err(Error::Identity);
            }
            continue;
        };
        if spec.chain.apply > header.capture_lsn {
            return Err(Error::Order);
        }
        if slot < 32 {
            let OwnerState::Transaction(status) = spec.owner else {
                return Err(Error::Identity);
            };
            // A terminal descriptor is removed with its Terminal adoption.
            if slot != transactions
                || !matches!(
                    status.state(),
                    TransactionState::Active | TransactionState::Aborting(_)
                )
            {
                return Err(Error::Identity);
            }
            let actor = status.reference();
            let key = (
                actor.id().writer_epoch,
                actor.id().local,
                actor.generation().get(),
            );
            if previous
                .is_some_and(|old: (u32, u64, u64)| old >= key || (old.0, old.1) == (key.0, key.1))
            {
                return Err(Error::Order);
            }
            previous = Some(key);
            let terminal = graph.terminal(actor)?.ok_or(Error::Identity)?;
            if terminal.state() != JobState::TerminalLeased
                || spec.chain.birth != terminal.lease_lsn()
            {
                return Err(Error::Identity);
            }
            let location = terminal.status().ok_or(Error::Identity)?;
            let home = graph.home(terminal)?;
            let cells = bank(scratch);
            for (ordinal, cell) in cells.iter_mut().enumerate() {
                cell.copy_from_slice(graph.job(ordinal)?.bytes());
            }
            semantic_row::ledger::validate_transaction(
                raw(graph.layout, slot)?,
                graph.layout.expected.profile,
                home,
                location,
                row::JobCells::original(cells),
            )?;
            transactions += 1;
        } else {
            let OwnerState::System(progress) = spec.owner else {
                return Err(Error::Identity);
            };
            if !matches!(slot, 32 | 34)
                || progress.reference.kind() as usize != slot - 31
                || progress.reference.namespace() != graph.namespace()
                || progress.reference.generation() != one()
                || progress.reference.id() != header.system_high_water[slot - 32]
            {
                return Err(Error::Identity);
            }
            if progress.phase == WorkPhase::Finished {
                continue;
            }
            systems += 1;
        }
        add_budget(&mut aggregate, &spec.remaining)?;
    }
    // Distinct actors, each bound to its own TerminalLeased, and the layout's
    // leased count make the reverse ATT relation (Ready has none) exact.
    if transactions != header.active_transactions as usize
        || systems != header.active_systems as usize
        || aggregate != header.remaining
    {
        return Err(Error::Budget);
    }
    Ok([transactions, systems])
}

/// The first Profile3 closure admits no inventory item, so no chunk is read.
struct NoChunks;
impl InventorySource for NoChunks {
    fn read_chunk(
        &mut self,
        _: u64,
        _: &mut [u8; PAGE_BYTES],
    ) -> std::result::Result<usize, InventoryError> {
        Err(InventoryError::Source)
    }
}

pub(super) fn inventory(
    graph: &Graph<'_>,
    owners: usize,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<InventoryObservation> {
    let header = graph.layout.header();
    let bank = graph.layout.inventory();
    let root_length = 160 + owners * 56;
    if !zero(&bank[root_length..])
        || bank[7] != 2
        || get(bank, 40) != header.generation
        || get(bank, 72) != 0
    {
        return Err(Error::Corrupt);
    }
    let expected = InventoryExpected {
        namespace: graph.namespace(),
        reference: ControlRef {
            kind: ControlKind::OwnershipRoot,
            slot: 2,
            index: 0,
            generation: header.generation,
            encoded_len: root_length as u32,
            digest: content_digest(&bank[..root_length]),
        },
        capture_lsn: header.capture_lsn,
        profile: retention_obligation::profile_reference(graph.layout.expected.profile),
        item_limit: 0,
    };
    let observed = ownership::verify_borrowed_inventory(
        &bank[..root_length],
        expected,
        owners,
        |ordinal| active_descriptor(graph.layout, ordinal).map_err(|_| InventoryError::Corrupt),
        &mut NoChunks,
        scratch,
    )?;
    if observed.items != 0 || observed.owners != owners {
        return Err(Error::Identity);
    }
    Ok(observed)
}

/// Only the never-allocated System state is closed here. Every other state
/// needs the Profile3 count-one algebra and fails closed.
pub(super) fn system_allocation(graph: &Graph<'_>) -> Result<()> {
    let header = graph.layout.header();
    if header.system_high_water[0] != 0 {
        return Err(Error::AllocationUnwired);
    }
    if !zero(raw(graph.layout, 32)?) || header.allocation_terminal != AllocationTerminal::None {
        return Err(Error::Identity);
    }
    Ok(())
}
