use super::super::super::{
    ControlKind, ControlRef, INVENTORY_CHUNK_CAP, INVENTORY_ROOT_CAP, InventoryError,
    InventoryExpected, InventorySource, add_budget, ownership,
};
use super::*;

pub(super) fn raw<'a, L: SeamLayout<'a>>(layout: L, slot: usize) -> Result<&'a [u8; 1536]> {
    if slot >= 36 {
        return Err(Error::InvalidInput);
    }
    Ok(layout.descriptor_bank()[slot * 1536..(slot + 1) * 1536]
        .try_into()
        .expect("fixed descriptor"))
}
pub(in crate::storage::plan4_recovery_base) fn descriptor<'a, L: SeamLayout<'a>>(
    layout: L,
    slot: usize,
) -> Result<Option<DescriptorSpec>> {
    let bytes = raw(layout, slot)?;
    if zero(bytes) {
        return Ok(None);
    }
    if slot == 34 {
        return Ok(Some(layout.aging(bytes)?));
    }
    let owner = if slot < 32 {
        ReservationOwner::Transaction(TxnRef::decode(&bytes[24..72])?)
    } else {
        ReservationOwner::System(SystemWorkRef::decode(&bytes[24..72])?)
    };
    Ok(Some(obligation::decode_descriptor(
        bytes,
        owner,
        retention_obligation::profile_reference(layout.expected().profile),
    )?))
}
pub(in crate::storage::plan4_recovery_base) fn active_descriptor<'a, L: SeamLayout<'a>>(
    layout: L,
    ordinal: usize,
) -> Result<DescriptorSpec> {
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
    Err(Error::InvalidInput)
}

// End the owned last-descriptor lifetime before invoking the original System
// algebra. Provenance is an observation, never an allocation/INIT authority.
fn system_provenance<'a, G: Seam<'a>>(
    graph: &G,
    reserved: ReservedInventory,
) -> Result<Option<(PageRef, PageKind, u64)>> {
    let header = graph.layout().header();
    let current = descriptor(graph.layout(), 32)?;
    let mut initialized = None;
    for ordinal in 9..64 {
        let Some(claim) = graph.layout().provenance(ordinal)? else {
            continue;
        };
        let ReservationOwner::System(source) = claim.source() else {
            continue;
        };
        let spec = current.as_ref().ok_or(Error::ProvenanceMismatch)?;
        if source.id() == header.system_high_water[0] {
            if initialized.is_some()
                || claim.init_lsn() != spec.chain.apply
                || claim.reserve_lsn() != spec.chain.birth
            {
                return Err(Error::ProvenanceMismatch);
            }
            initialized = Some((
                claim.page(),
                graph
                    .layout()
                    .certificate(ordinal)?
                    .ok_or(Error::IdentityMismatch)?
                    .header
                    .kind,
                claim.init_lsn(),
            ));
        } else if source.id() > header.system_high_water[0] || claim.init_lsn() >= spec.chain.birth
        {
            return Err(Error::ProvenanceMismatch);
        }
        for previous in 9..ordinal {
            if graph
                .layout()
                .provenance(previous)?
                .is_some_and(|old| old.source() == claim.source())
            {
                return Err(Error::ProvenanceMismatch);
            }
        }
    }
    if current.as_ref().is_some_and(|spec| matches!(spec.owner, OwnerState::System(p) if (p.phase != WorkPhase::Finished) != (reserved == ReservedInventory::SystemPage))) {
        return Err(Error::OrderMismatch);
    }
    Ok(initialized)
}

fn system_allocation<'a, G: Seam<'a>>(graph: &G, reserved: ReservedInventory) -> Result<()> {
    let initialized = system_provenance(graph, reserved)?;
    let header = graph.layout().header();
    let raw = raw(graph.layout(), 32)?;
    graph.allocation(
        (!zero(raw)).then_some(raw),
        header.system_high_water[0],
        header.allocation_terminal.byte(),
        initialized,
    )
}

fn validate_rows<'a, G: Seam<'a>>(graph: &G) -> Result<(usize, usize)> {
    let header = graph.layout().header();
    if graph.aging_active() != (header.system_high_water[2] == 1)
        || graph.apply() > header.capture_lsn
    {
        return Err(Error::IdentityMismatch);
    }
    let mut transactions = 0;
    let mut systems = 0;
    let mut previous = None;
    let mut aggregate = [RemainingBudget::default(); 5];
    for slot in 0..36 {
        let Some(spec) = descriptor(graph.layout(), slot)? else {
            if slot < header.active_transactions as usize
                || slot == 32 && header.system_high_water[0] != 0
                || slot == 34 && graph.aging_active()
            {
                return Err(Error::IdentityMismatch);
            }
            continue;
        };
        if spec.chain.apply > header.capture_lsn {
            return Err(Error::OrderMismatch);
        }
        if slot < 32 {
            if slot != transactions || slot >= header.active_transactions as usize {
                return Err(Error::IdentityMismatch);
            }
            let OwnerState::Transaction(status) = spec.owner else {
                return Err(Error::IdentityMismatch);
            };
            let catalog_state = matches!(
                status.state(),
                TransactionState::Active
                    | TransactionState::Aborting(RollbackState::Transaction { .. })
            );
            // Only a base4 Row actor may also be in a statement/full rollback.
            if !catalog_state
                && !(G::ROWS && matches!(status.state(), TransactionState::Aborting(_)))
            {
                return Err(Error::Unsupported);
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
                return Err(Error::OrderMismatch);
            }
            previous = Some(key);
            let terminal = graph
                .for_actor(actor, true)?
                .ok_or(Error::IdentityMismatch)?;
            if terminal.phase() != Phase::TerminalLeased || spec.chain.birth != terminal.lease_lsn()
            {
                return Err(Error::IdentityMismatch);
            }
            // A Row actor's residual ledger is the base4 Row owner's check.
            if terminal.row_scope() {
                transactions += 1;
                add_budget(&mut aggregate, &spec.remaining)?;
                continue;
            }
            if !catalog_state {
                return Err(Error::Unsupported);
            }
            let root = graph.for_actor(actor, false)?;
            graph.catalog_transaction(raw(graph.layout(), slot)?, graph.home(terminal)?, root)?;
            if let Some(root) = root
                && (spec.chain.apply < root.last_step()
                    || root.phase() == Phase::RootReserved
                        && (spec.chain.apply != root.lease_lsn()
                            || spec.inventory.change_lsn != root.lease_lsn()))
            {
                return Err(Error::OrderMismatch);
            }
            transactions += 1;
        } else {
            let OwnerState::System(progress) = spec.owner else {
                return Err(Error::IdentityMismatch);
            };
            if !matches!(slot, 32 | 34)
                || progress.reference.kind() as usize != slot - 31
                || progress.reference.namespace()
                    != graph.layout().expected().profile.spec().namespace
                || progress.reference.generation() != one()
                || progress.reference.id() != header.system_high_water[slot - 32]
            {
                return Err(Error::IdentityMismatch);
            }
            if progress.phase == WorkPhase::Finished {
                continue;
            }
            systems += 1;
        }
        add_budget(&mut aggregate, &spec.remaining)?;
    }
    if transactions != header.active_transactions as usize
        || systems != header.active_systems as usize
        || aggregate != header.remaining
    {
        return Err(Error::BudgetMismatch);
    }
    Ok((transactions, systems))
}

fn validate_reverse<'a, G: Seam<'a>>(graph: &G, transactions: usize) -> Result<()> {
    // Reverse ATT check: every leased Terminal has exactly one active row.
    for index in 0..JOB_COUNT {
        let job = graph.job(index)?;
        if !job.phase().is_terminal() {
            continue;
        }
        let mut count = 0;
        for slot in 0..transactions {
            if descriptor(graph.layout(), slot)?.is_some_and(|d| {
                d.owner.reference()
                    == ReservationOwner::Transaction(job.source().expect("terminal"))
            }) {
                count += 1;
            }
        }
        if count != usize::from(job.phase() == Phase::TerminalLeased) {
            return Err(Error::IdentityMismatch);
        }
    }
    Ok(())
}

pub(in crate::storage::plan4_recovery_base) fn validate<'a, G: Seam<'a>>(
    graph: &G,
    reserved: ReservedInventory,
) -> Result<(usize, usize)> {
    let counts = validate_rows(graph)?;
    validate_reverse(graph, counts.0)?;
    system_allocation(graph, reserved)?;
    Ok(counts)
}

struct Embedded<'a> {
    bytes: &'a [u8; PAGE_BYTES],
    items: usize,
}
impl InventorySource for Embedded<'_> {
    fn read_chunk(
        &mut self,
        index: u64,
        output: &mut [u8; PAGE_BYTES],
    ) -> std::result::Result<usize, InventoryError> {
        if index != 0 || !matches!(self.items, 1 | 3) {
            return Err(InventoryError::Source);
        }
        let length = 160 + 96 * self.items;
        output[..length]
            .copy_from_slice(&self.bytes[INVENTORY_ROOT_CAP..INVENTORY_ROOT_CAP + length]);
        Ok(length)
    }
}

fn inventory_observation<'a, G: Seam<'a>>(
    graph: &G,
    reserved: ReservedInventory,
    owners: usize,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<InventoryObservation> {
    let header = graph.layout().header();
    let bank = graph.layout().inventory();
    let items = match reserved {
        ReservedInventory::None => 0,
        ReservedInventory::SystemPage | ReservedInventory::RowUndoSlot => 1,
        ReservedInventory::CatalogCreate => 3,
    };
    let root_length = 160 + owners * 56;
    let chunk_length = if items == 0 { 0 } else { 160 + 96 * items };
    if !zero(&bank[root_length..INVENTORY_ROOT_CAP])
        || !zero(&bank[INVENTORY_ROOT_CAP + chunk_length..INVENTORY_ROOT_CAP + INVENTORY_CHUNK_CAP])
        || !zero(&bank[INVENTORY_ROOT_CAP + INVENTORY_CHUNK_CAP..])
        || bank[7] != 2
        || get(bank, 40) != header.generation
        || get(bank, 72) != items as u64
    {
        return Err(Error::Corrupt);
    }
    let expected = InventoryExpected {
        namespace: graph.layout().expected().profile.spec().namespace,
        reference: ControlRef {
            kind: ControlKind::OwnershipRoot,
            slot: 2,
            index: 0,
            generation: header.generation,
            encoded_len: root_length as u32,
            digest: content_digest(&bank[..root_length]),
        },
        capture_lsn: header.capture_lsn,
        profile: retention_obligation::profile_reference(graph.layout().expected().profile),
        item_limit: 3,
    };
    let mut source = Embedded { bytes: bank, items };
    let observed = ownership::verify_borrowed_inventory(
        &bank[..root_length],
        expected,
        owners,
        |ordinal| active_descriptor(graph.layout(), ordinal).map_err(|_| InventoryError::Corrupt),
        &mut source,
        scratch,
    )?;
    if observed.items != items as u64 || observed.owners != owners {
        return Err(Error::IdentityMismatch);
    }
    Ok(observed)
}

fn inventory_items<'a, G: Seam<'a>>(graph: &G, reserved: ReservedInventory) -> Result<()> {
    let bank = graph.layout().inventory();
    // The base4 Row owner couples a RowUndoSlot item to its Row-scope root.
    let items = match reserved {
        ReservedInventory::None | ReservedInventory::RowUndoSlot => 0,
        ReservedInventory::SystemPage => 1,
        ReservedInventory::CatalogCreate => 3,
    };
    let allocation = allocation_page::decode_allocation_page(
        image(graph.layout().bytes(), 0),
        identity(graph.layout().expected(), 0, one(), 1),
        graph.layout().expected().profile.spec().namespace,
    )?;
    let mut pair_seen = 0u8;
    let mut undo_seen = false;
    for index in 0..items {
        let item = &bank
            [INVENTORY_ROOT_CAP + 128 + index * 96..INVENTORY_ROOT_CAP + 128 + (index + 1) * 96];
        let descriptor = active_descriptor(graph.layout(), get16(item, 2) as usize)?;
        if get(item, 72) != descriptor.inventory.change_lsn {
            return Err(Error::IdentityMismatch);
        }
        match reserved {
            ReservedInventory::SystemPage => {
                let reference = PageRef::decode(&item[8..32])?;
                if item[0] != 1
                    || item[1] != 3
                    || reference.file != allocation.header().identity.page.file
                    || !matches!(allocation.entry(reference.page_no as usize)?, AllocationEntry::Reserved { generation, claimant }
                    if generation == reference.incarnation && claimant == descriptor.owner.reference())
                    || descriptor.owner.reference() != descriptor_for_system(graph.layout())?
                {
                    return Err(Error::IdentityMismatch);
                }
            }
            ReservedInventory::CatalogCreate => {
                let OwnerState::Transaction(status) = descriptor.owner else {
                    return Err(Error::IdentityMismatch);
                };
                let job = graph
                    .for_actor(status.reference(), false)?
                    .ok_or(Error::IdentityMismatch)?;
                if job.phase() != Phase::RootReserved
                    || item[1] != 2
                    || get(item, 72) != job.lease_lsn()
                {
                    return Err(Error::IdentityMismatch);
                }
                if item[0] == 1 {
                    let reference = PageRef::decode(&item[8..32])?;
                    let pair = job.pair().ok_or(Error::IdentityMismatch)?;
                    let side = pair
                        .iter()
                        .position(|p| *p == reference)
                        .ok_or(Error::IdentityMismatch)?;
                    if pair_seen & (1 << side) != 0 {
                        return Err(Error::IdentityMismatch);
                    }
                    pair_seen |= 1 << side;
                } else if item[0] == 2 {
                    if undo_seen
                        || UndoRef::decode(&item[8..48])?
                            != job.undo().ok_or(Error::IdentityMismatch)?
                    {
                        return Err(Error::IdentityMismatch);
                    }
                    undo_seen = true;
                } else {
                    return Err(Error::Unsupported);
                }
            }
            ReservedInventory::None | ReservedInventory::RowUndoSlot => {
                return Err(Error::IdentityMismatch);
            }
        }
    }
    if reserved == ReservedInventory::CatalogCreate && (pair_seen != 3 || !undo_seen) {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}

pub(in crate::storage::plan4_recovery_base) fn inventory<'a, G: Seam<'a>>(
    graph: &G,
    reserved: ReservedInventory,
    owners: usize,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<InventoryObservation> {
    // The generic verifier's expected/source/Header packets end before the
    // physical item coupling begins. Only its small observation survives.
    let observed = inventory_observation(graph, reserved, owners, scratch)?;
    inventory_items(graph, reserved)?;
    Ok(observed)
}

fn descriptor_for_system<'a, L: SeamLayout<'a>>(layout: L) -> Result<ReservationOwner> {
    Ok(descriptor(layout, 32)?
        .ok_or(Error::IdentityMismatch)?
        .owner
        .reference())
}

/// The generic inventory checker calls back into `active_descriptor`. This
/// closure remains additive with that checker; its nested Aging decoder is not
/// replaced by a candidate descriptor or a borrowed, unchecked owner header.
fn descriptor_source_bytes<'a, L: SeamLayout<'a>>() -> usize {
    use std::mem::size_of;
    let outer = 2 * size_of::<L>()
        + 2 * size_of::<ReservationOwner>()
        + 4 * size_of::<Result<()>>()
        + 8 * size_of::<&[u8]>()
        + size_of::<[usize; 8]>();
    // The Aging bound includes the same original descriptor decoder, its
    // validation/remaining/hash temporaries and the additional Root2 check.
    // Regular Tx/System decode has no wider closure. `descriptor()` following
    // a checked AgingView runs that original decoder again, sequentially.
    let decode = L::aging_source_bytes();
    let returned = 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<Option<DescriptorSpec>>()
        + 2 * size_of::<Result<Option<DescriptorSpec>>>()
        + 2 * size_of::<Result<DescriptorSpec>>();
    outer + decode.max(returned)
}

fn active_descriptor_source_bytes<'a, L: SeamLayout<'a>>() -> usize {
    use std::mem::size_of;
    let outer = 2 * size_of::<L>() + size_of::<[usize; 8]>() + 4 * size_of::<&[u8]>();
    // Each loop iteration ends before another descriptor is decoded. At the
    // successful return, retain both extracted values and their Result slots.
    let returned = 2 * size_of::<DescriptorSpec>()
        + size_of::<Option<DescriptorSpec>>()
        + size_of::<Result<Option<DescriptorSpec>>>()
        + 2 * size_of::<Result<DescriptorSpec>>();
    outer + descriptor_source_bytes::<L>().max(returned)
}

#[derive(Clone, Copy, Debug)]
pub(in crate::storage::plan4_recovery_base) struct SourceCensus {
    pub(in crate::storage::plan4_recovery_base) descriptor: usize,
    pub(in crate::storage::plan4_recovery_base) rows: usize,
    pub(in crate::storage::plan4_recovery_base) reverse: usize,
    pub(in crate::storage::plan4_recovery_base) system: usize,
    pub(in crate::storage::plan4_recovery_base) inventory: usize,
    pub(in crate::storage::plan4_recovery_base) peak: usize,
}

pub(in crate::storage::plan4_recovery_base) fn source_census<'a, G: Seam<'a>>() -> SourceCensus {
    use std::mem::size_of;
    let descriptor = descriptor_source_bytes::<G::Layout>();
    let active = active_descriptor_source_bytes::<G::Layout>();
    let scalar =
        4 * size_of::<Result<()>>() + 16 * size_of::<u64>() + 16 * size_of::<&[u8]>() + 512; // narrow reference codecs, iterators and conversion results
    let rows_outer = size_of::<Header>()
        + size_of::<[RemainingBudget; 5]>()
        + size_of::<Option<(u32, u64, u64)>>()
        + 4 * size_of::<usize>()
        + scalar;
    let transaction = size_of::<DescriptorSpec>()
        + size_of::<Result<Option<DescriptorSpec>>>()
        + 3 * size_of::<G::Job>()
        + 2 * size_of::<Result<Option<G::Job>>>()
        + 2 * size_of::<StatusHome>()
        + size_of::<Result<StatusHome>>()
        + 2 * size_of::<crate::storage::transaction_record::TransactionStatus>()
        + 2 * size_of::<TxnRef>()
        + size_of::<(u32, u64, u64)>()
        + G::transaction_source_bytes();
    let rows = rows_outer + descriptor.max(transaction);
    let reverse = scalar
        + 2 * size_of::<G::Job>()
        + size_of::<Result<G::Job>>()
        + size_of::<Result<Option<DescriptorSpec>>>()
        + descriptor;
    let provenance = scalar
        + size_of::<Header>()
        + size_of::<Option<DescriptorSpec>>()
        + size_of::<Result<Option<DescriptorSpec>>>()
        + 4 * size_of::<Option<(PageRef, PageKind, u64)>>()
        + 4 * size_of::<InitProvenance<'_>>()
        + 4 * size_of::<Result<Option<InitProvenance<'_>>>>()
        + 2 * size_of::<Certificate>()
        + 2 * size_of::<Result<Option<Certificate>>>()
        + 2 * size_of::<PageHeader>()
        + descriptor.max(G::codec_source_bytes());
    // system_provenance returns before the old allocation algebra starts, so
    // its owned last descriptor cannot overlap that validator's descriptors.
    let system = scalar
        + size_of::<Header>()
        + 2 * size_of::<Option<(PageRef, PageKind, u64)>>()
        + provenance.max(crate::storage::wal_io::plan4_system_base_allocation_source_bytes());
    let inventory_verify = scalar
        + size_of::<Header>()
        + 2 * size_of::<InventoryExpected>()
        + size_of::<Embedded<'_>>()
        + size_of::<InventoryObservation>()
        + size_of::<Result<InventoryObservation>>()
        + ownership::borrowed_inventory_source_bytes()
        + active;
    let inventory_items = scalar
        + size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<Result<DescriptorSpec>>()
        + 2 * size_of::<G::Job>()
        + 2 * size_of::<Result<Option<G::Job>>>()
        + 4 * size_of::<PageRef>()
        + 2 * size_of::<UndoRef>()
        + 2 * size_of::<AllocationEntry>()
        + active.max(G::codec_source_bytes());
    let inventory = size_of::<InventoryObservation>()
        + size_of::<Result<InventoryObservation>>()
        + inventory_verify.max(inventory_items);
    // validate_rows, reverse and System calls have returned before inventory.
    // Keep wrapper counts/results across those function boundaries explicitly.
    let validation = size_of::<(usize, usize)>()
        + size_of::<Result<(usize, usize)>>()
        + rows.max(reverse).max(system);
    SourceCensus {
        descriptor,
        rows,
        reverse,
        system,
        inventory,
        peak: validation.max(inventory),
    }
}
