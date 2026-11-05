//! Exact Finished split descriptor observations. These are neither live work
//! authorities nor proof that the historical split actually occurred.
use super::{Error, Operation, Result};
use crate::storage::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    control::Profile,
    obligation::{
        self, DescriptorSpec, InventoryClaim, OwnerChain, OwnerState, RemainingBudget, WorkPhase,
    },
    retention_obligation::profile_reference,
};
use foundation::codec::content_digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FinishedSplit {
    pub(crate) owner: SystemWorkRef,
    pub(crate) operation: Operation,
}

/// Validate every semantic field of a decoded descriptor. The raw decoder is
/// still responsible for its exact byte grammar and reserved fields.
pub(crate) fn validate(
    profile: Profile,
    cut: u64,
    epoch: u32,
    high_water: u64,
    spec: &DescriptorSpec,
) -> Result<FinishedSplit> {
    if profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    let OwnerState::System(progress) = spec.owner else {
        return Err(Error::IdentityMismatch);
    };
    let owner = progress.reference;
    if epoch == 0
        || high_water == 0
        || owner.namespace() != profile.spec().namespace
        || owner.kind() != SystemWorkKind::StructuralChange
        || owner.id() != high_water
        || owner.id() > cut
        || owner.generation().get() > u64::from(epoch)
        || spec.profile != profile_reference(profile)
    {
        return Err(Error::IdentityMismatch);
    }
    if progress.phase != WorkPhase::Finished
        || progress.sequence != 1
        || progress.cursor != 1
        || progress.limit != 1
        || spec.chain
            != (OwnerChain {
                apply: owner.id(),
                previous: 0,
                birth: owner.id(),
                groups: 1,
            })
    {
        return Err(Error::OrderMismatch);
    }
    let operation = match spec.consumed_wal {
        84_744 => Operation::RootSplit,
        84_640 => Operation::ChildSplit,
        _ => return Err(Error::BudgetMismatch),
    };
    if spec.inventory
        != (InventoryClaim {
            change_lsn: owner.id(),
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        })
        || spec.consumed_control != 0
        || spec.remaining != [RemainingBudget::default(); 5]
        || spec.minimum_wal_lsn != 0
        || spec.last_mutation_sequence != 0
    {
        return Err(Error::BudgetMismatch);
    }
    Ok(FinishedSplit { owner, operation })
}

pub(crate) fn observe(
    profile: Profile,
    cut: u64,
    epoch: u32,
    high_water: u64,
    raw: Option<&[u8; 1536]>,
) -> Result<Option<FinishedSplit>> {
    if profile.format() != 3 || epoch == 0 {
        return Err(Error::Unsupported);
    }
    let Some(raw) = raw else {
        return if high_water == 0 {
            Ok(None)
        } else {
            Err(Error::IdentityMismatch)
        };
    };
    let owner = SystemWorkRef::decode(&raw[24..72]).map_err(|_| Error::IdentityMismatch)?;
    let spec = obligation::decode_descriptor(
        raw,
        ReservationOwner::System(owner),
        profile_reference(profile),
    )?;
    validate(profile, cut, epoch, high_water, &spec).map(Some)
}

/// Typed validation frame plus its pinned digest closure; encoded input stays
/// borrowed. The caller counts its own already-decoded DescriptorSpec, if any.
pub(crate) fn validation_source_bytes() -> usize {
    use std::mem::size_of;
    1536 + 2 * size_of::<foundation::codec::ContentHasher>()
        + size_of::<[RemainingBudget; 5]>()
        + 2 * size_of::<(Profile, u64, u32, u64, &DescriptorSpec)>()
        + 2 * size_of::<crate::storage::obligation::SystemProgress>()
        + 2 * size_of::<SystemWorkRef>()
        + 2 * size_of::<InventoryClaim>()
        + 2 * size_of::<OwnerChain>()
        + 2 * size_of::<FinishedSplit>()
        + 2 * size_of::<Result<FinishedSplit>>()
        + 4 * size_of::<[u8; 32]>()
        + size_of::<[u64; 8]>()
}

pub(crate) fn source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<(Profile, u64, u32, u64, Option<&[u8; 1536]>)>()
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, obligation::ObligationError>>()
        + 2 * size_of::<SystemWorkRef>()
        + 2 * size_of::<
            std::result::Result<SystemWorkRef, crate::storage::allocation_page::AllocationError>,
        >()
        + 2 * size_of::<Result<Option<FinishedSplit>>>()
        + validation_source_bytes().max(obligation::descriptor_source_bytes())
}

#[cfg(test)]
#[path = "bank_tests.rs"]
mod tests;
