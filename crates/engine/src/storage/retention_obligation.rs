//! Root-bound Aging work bytes, separate from the old empty-work descriptor.
//! These borrowed observations do not select a root or restore reservations.
#![allow(dead_code)] // Awaiting the original live retention owner and recovery consumer.

use super::{
    allocation_page::ReservationOwner,
    control::{ControlKind, ControlRef, PROFILE_BYTES, Profile},
    obligation::{self, DescriptorSpec, ObligationError, OwnerState, WorkPhase},
    page::PAGE_BYTES,
    retention_page::{self, FINISH_BYTES, RetentionError, RootView},
};
use xadb_foundation::codec::content_digest;

pub(crate) const AGING_START_BYTES: usize = 19_672;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgingObligationError {
    Retention(RetentionError),
    Descriptor(ObligationError),
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
}
impl From<RetentionError> for AgingObligationError {
    fn from(value: RetentionError) -> Self {
        Self::Retention(value)
    }
}
impl From<ObligationError> for AgingObligationError {
    fn from(value: ObligationError) -> Self {
        Self::Descriptor(value)
    }
}
type Result<T> = std::result::Result<T, AgingObligationError>;

pub(crate) fn profile_reference(profile: Profile) -> ControlRef {
    ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: profile.spec().generation,
        encoded_len: PROFILE_BYTES as u32,
        digest: profile.digest(),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AgingDescriptorView<'a> {
    bytes: &'a [u8; 1536],
    expected: ReservationOwner,
    profile: ControlRef,
}
impl<'a> AgingDescriptorView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; 1536] {
        self.bytes
    }
    pub(crate) fn descriptor(self) -> DescriptorSpec {
        obligation::decode_aging_descriptor(self.bytes, self.expected, self.profile)
            .expect("immutable validated Aging descriptor")
    }
}

fn validate(spec: &DescriptorSpec, profile: Profile, root: RootView<'_>) -> Result<()> {
    let actual = root.spec();
    let Some(reference) = actual.aging else {
        return Err(AgingObligationError::IdentityMismatch);
    };
    let OwnerState::System(progress) = spec.owner else {
        return Err(AgingObligationError::IdentityMismatch);
    };
    if progress.reference != reference
        || progress.phase != WorkPhase::Running
        || progress.sequence != actual.revision
        || progress.cursor != actual.cursor
        || progress.limit != 64
        || spec.profile != profile_reference(profile)
    {
        return Err(AgingObligationError::IdentityMismatch);
    }
    if spec.chain.apply != actual.apply
        || spec.chain.groups.checked_add(1) != Some(actual.revision)
        || spec.inventory.change_lsn != spec.chain.birth
        || spec.minimum_wal_lsn != spec.chain.birth
        || spec.last_mutation_sequence != 0
    {
        return Err(AgingObligationError::OrderMismatch);
    }
    let consumed = actual
        .completed
        .checked_mul(FINISH_BYTES as u64)
        .and_then(|value| value.checked_add(AGING_START_BYTES as u64))
        .ok_or(AgingObligationError::BudgetMismatch)?;
    if spec.inventory.items != 0
        || spec.inventory.growth_extents != 0
        || spec.inventory.digest != content_digest(&[])
        || spec.consumed_wal != consumed
        || spec.consumed_control != 0
        || spec.remaining != retention_page::cleanup_rows(root.non_free())?
    {
        return Err(AgingObligationError::BudgetMismatch);
    }
    obligation::validate_descriptor(spec)?;
    Ok(())
}

fn work_bytes(root_page: &[u8; PAGE_BYTES], root: RootView<'_>) -> [u8; 96] {
    let mut bytes = [0; 96];
    bytes[0] = 7;
    retention_page::put(&mut bytes, 8, root.spec().revision);
    bytes[16..20].copy_from_slice(&(PAGE_BYTES as u32).to_le_bytes());
    bytes[32..64].copy_from_slice(&content_digest(root_page));
    bytes[64..96].copy_from_slice(&content_digest(root.bytes()));
    bytes
}

/// Encodes only after root/page/accounting validation. Caller output is unchanged
/// on error; this function does not authorize that root's current membership.
pub(crate) fn encode(
    spec: &DescriptorSpec,
    profile: Profile,
    root_page: &[u8; PAGE_BYTES],
    output: &mut [u8],
) -> Result<()> {
    let (_, root) = retention_page::decode_root_page(root_page, profile.spec().namespace)?;
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
    let (_, root) = retention_page::decode_root_page(root_page, profile.spec().namespace)?;
    let expected = ReservationOwner::System(
        root.spec()
            .aging
            .ok_or(AgingObligationError::IdentityMismatch)?,
    );
    let selected = profile_reference(profile);
    let spec = obligation::decode_aging_descriptor(raw, expected, selected)?;
    validate(&spec, profile, root)?;
    if raw[384..480] != work_bytes(root_page, root) {
        return Err(AgingObligationError::IdentityMismatch);
    }
    Ok(AgingDescriptorView {
        bytes: raw,
        expected,
        profile: selected,
    })
}

/// New-only source closure, including common wide decode/return values. The
/// actual live caller adds its retained pool, controls and other simultaneous
/// values; this is neither an ABI stack estimate nor a capacity decision.
pub(crate) fn source_bytes() -> usize {
    use std::mem::size_of;
    96 + 1536
        + 640
        + 512
        + 1024
        + 6 * size_of::<DescriptorSpec>()
        + 4 * size_of::<std::result::Result<DescriptorSpec, ObligationError>>()
        + 4 * size_of::<Result<AgingDescriptorView<'_>>>()
        + 6 * size_of::<super::retention_page::RootSpec>()
        + 4 * size_of::<super::page::PageView<'_>>()
        + 4 * size_of::<xadb_foundation::codec::ContentHasher>()
        + 1024
}

#[cfg(test)]
#[path = "retention_obligation_tests.rs"]
mod tests;
