//! Root2-bound original Aging descriptor. Job budgets are checked separately by
//! the whole queue census; this decoder does not issue cleanup ownership.
use super::{
    AgingDescriptorView, AgingObligationError, DescriptorSpec, ObligationError, OwnerState,
    PAGE_BYTES, Profile, ReservationOwner, Result, WorkPhase, content_digest, obligation,
    profile_reference,
};
use crate::storage::retention_page::{self, catalog};

fn validate(spec: &DescriptorSpec, profile: Profile, root: catalog::RootView<'_>) -> Result<()> {
    let Some(reference) = root.aging() else {
        return Err(AgingObligationError::IdentityMismatch);
    };
    let OwnerState::System(progress) = spec.owner else {
        return Err(AgingObligationError::IdentityMismatch);
    };
    if profile.format() != 2
        || progress.reference != reference
        || progress.phase != WorkPhase::Running
        || progress.sequence != root.revision()
        || progress.cursor != root.cursor()
        || progress.limit != 64
        || spec.profile != profile_reference(profile)
    {
        return Err(AgingObligationError::IdentityMismatch);
    }
    if spec.chain.apply != root.apply()
        || spec.chain.groups.checked_add(1) != Some(root.revision())
        || spec.inventory.change_lsn != spec.chain.birth
        || spec.minimum_wal_lsn != spec.chain.birth
        || spec.last_mutation_sequence != 0
    {
        return Err(AgingObligationError::OrderMismatch);
    }
    if spec.inventory.items != 0
        || spec.inventory.growth_extents != 0
        || spec.inventory.digest != content_digest(&[])
        || spec.consumed_wal != root.consumed_wal()
        || spec.consumed_control != root.consumed_control()
        || retention_page::budget_digest(&spec.remaining) != *root.budget_digest()
    {
        return Err(AgingObligationError::BudgetMismatch);
    }
    obligation::validate_descriptor(spec)?;
    Ok(())
}

fn work_bytes(page: &[u8; PAGE_BYTES], root: catalog::RootView<'_>) -> [u8; 96] {
    let mut bytes = [0; 96];
    bytes[0] = 7;
    retention_page::put(&mut bytes, 8, root.revision());
    bytes[16..20].copy_from_slice(&(PAGE_BYTES as u32).to_le_bytes());
    bytes[32..64].copy_from_slice(&content_digest(page));
    bytes[64..96].copy_from_slice(&content_digest(root.bytes()));
    bytes
}

pub(crate) fn encode(
    spec: &DescriptorSpec,
    profile: Profile,
    root_page: &[u8; PAGE_BYTES],
    output: &mut [u8],
) -> Result<()> {
    let (_, root) = catalog::decode_root_page(root_page, profile.spec().namespace)?;
    validate(spec, profile, root)?;
    obligation::encode_aging_descriptor(spec, &work_bytes(root_page, root), output)?;
    Ok(())
}

pub(crate) fn decode<'a>(
    bytes: &'a [u8],
    profile: Profile,
    root_page: &[u8; PAGE_BYTES],
) -> Result<AgingDescriptorView<'a>> {
    let raw: &[u8; 1536] = bytes.try_into().map_err(|_| ObligationError::Corrupt)?;
    let (_, root) = catalog::decode_root_page(root_page, profile.spec().namespace)?;
    let expected =
        ReservationOwner::System(root.aging().ok_or(AgingObligationError::IdentityMismatch)?);
    let selected = profile_reference(profile);
    {
        let spec = obligation::decode_aging_descriptor(raw, expected, selected)?;
        validate(&spec, profile, root)?;
    }
    if raw[384..480] != work_bytes(root_page, root) {
        return Err(AgingObligationError::IdentityMismatch);
    }
    Ok(AgingDescriptorView {
        bytes: raw,
        expected,
        profile: selected,
    })
}

pub(crate) fn source_bytes() -> usize {
    use crate::storage::{
        allocation_page::SystemWorkRef,
        control::ControlRef,
        obligation::{RemainingBudget, SystemProgress},
        page::PageHeader,
    };
    use foundation::codec::ContentHasher;
    use std::mem::size_of;
    let parent = size_of::<(&DescriptorSpec, Profile, &[u8; PAGE_BYTES], &mut [u8])>()
        + size_of::<(&[u8], &[u8; 1536], Profile, &[u8; PAGE_BYTES])>()
        + 2 * size_of::<catalog::RootView<'_>>()
        + 2 * size_of::<PageHeader>()
        + size_of::<ReservationOwner>()
        + size_of::<ControlRef>()
        + 2 * size_of::<Result<AgingDescriptorView<'_>>>()
        + 4 * size_of::<Result<()>>();
    let hashing = 2 * size_of::<ContentHasher>() + 1536 + 2 * size_of::<[u8; 32]>();
    let work = 2 * 96 + hashing + 4 * size_of::<&()>();
    let validation = size_of::<(SystemWorkRef, SystemProgress, OwnerState)>()
        + 4 * size_of::<Result<()>>()
        // budget_digest's actual bytes and encode_budget_rows current row.
        + (640 + size_of::<[u64; 14]>() + size_of::<RemainingBudget>() + hashing)
            .max(obligation::descriptor_source_bytes());
    let encoding = 96 + obligation::descriptor_source_bytes();
    // The decoded local spec exists only in its explicit scope and is gone
    // before work_bytes. Include the callee Result and extracted value once.
    let decoding = size_of::<DescriptorSpec>()
        + size_of::<std::result::Result<DescriptorSpec, ObligationError>>()
        + obligation::descriptor_source_bytes().max(validation);
    parent
        + catalog::root_decode_source_bytes()
            .max(validation)
            .max(work)
            .max(encoding)
            .max(decoding)
}
