//! Root3-bound original Aging descriptor. Job budgets are checked separately by
//! the whole queue census; this decoder does not issue cleanup ownership.
use super::{
    AgingDescriptorView, AgingObligationError, DescriptorSpec, ObligationError, OwnerState,
    PAGE_BYTES, Profile, ReservationOwner, Result, WorkPhase, content_digest, obligation,
    profile_reference,
};
use crate::storage::retention_page::{self, row};

fn validate(spec: &DescriptorSpec, profile: Profile, root: row::RootView<'_>) -> Result<()> {
    let Some(reference) = root.aging() else {
        return Err(AgingObligationError::IdentityMismatch);
    };
    let OwnerState::System(progress) = spec.owner else {
        return Err(AgingObligationError::IdentityMismatch);
    };
    if profile.format() != 3
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

fn work_bytes(page: &[u8; PAGE_BYTES], root: row::RootView<'_>) -> [u8; 96] {
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
    let (_, root) = row::decode_root_page(root_page, profile.spec().namespace)?;
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
    let (_, root) = row::decode_root_page(root_page, profile.spec().namespace)?;
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

/// Unpublished candidate over a caller-supplied predecessor and stable Job3
/// census. Original history and each operation's exact consumed delta are
/// checked by the semantic owner; these byte observations confer no authority.
pub(crate) struct AgingTransition<'a> {
    pub(crate) profile: Profile,
    pub(crate) before_descriptor: Option<&'a [u8; 1536]>,
    pub(crate) before_root: &'a [u8; PAGE_BYTES],
    pub(crate) after_root: &'a [u8; PAGE_BYTES],
    pub(crate) jobs: row::JobCells<'a>,
}

/// Validate the complete candidate queue before preparing its bound descriptor.
/// Aggregate is caller scratch and may change on error; output remains unchanged.
pub(crate) fn next(
    input: AgingTransition<'_>,
    aggregate: &mut [u8; 640],
    output: &mut [u8],
) -> Result<()> {
    use obligation::{InventoryClaim, OwnerChain, RemainingBudget, SystemProgress};
    if input.profile.format() != 3 {
        return Err(AgingObligationError::IdentityMismatch);
    }
    let namespace = input.profile.spec().namespace;
    let (_, root) = row::decode_root_page(input.after_root, namespace)?;
    let reference = root.aging().ok_or(AgingObligationError::IdentityMismatch)?;
    row::verify_selected_census(root, namespace, input.jobs, aggregate)?;
    let (chain, inventory) = if let Some(bytes) = input.before_descriptor {
        let previous = decode(bytes, input.profile, input.before_root)?.descriptor();
        if previous.chain.apply >= root.apply()
            || previous.consumed_wal > root.consumed_wal()
            || previous.consumed_control != root.consumed_control()
        {
            return Err(AgingObligationError::OrderMismatch);
        }
        (
            OwnerChain {
                apply: root.apply(),
                previous: previous.chain.apply,
                birth: previous.chain.birth,
                groups: previous
                    .chain
                    .groups
                    .checked_add(1)
                    .ok_or(AgingObligationError::OrderMismatch)?,
            },
            previous.inventory,
        )
    } else {
        let (_, previous) = row::decode_root_page(input.before_root, namespace)?;
        if previous.aging().is_some()
            || root.revision() != 2
            || root.consumed_wal() != super::AGING_START_BYTES as u64
            || root.consumed_control() != 0
            || root.cursor() != previous.cursor()
            || root.issued() != previous.issued()
            || root.masks() != previous.masks()
            || root.job_digest() != previous.job_digest()
            || root.budget_digest() != previous.budget_digest()
            || root.completed() != previous.completed()
            || root.canceled() != previous.canceled()
            || root.undo_high() != previous.undo_high()
            || root.object_high() != previous.object_high()
            || root.commit_c() != previous.commit_c()
            || root.row_high() != previous.row_high()
        {
            return Err(AgingObligationError::OrderMismatch);
        }
        (
            OwnerChain {
                apply: root.apply(),
                previous: 0,
                birth: root.apply(),
                groups: 1,
            },
            InventoryClaim {
                change_lsn: root.apply(),
                items: 0,
                digest: content_digest(&[]),
                growth_extents: 0,
            },
        )
    };
    // The full Job3 census above emits only canonical row3 cleanup debt.
    // Peak is max across that one Aging executor, including mixed scopes.
    let steps = retention_page::get(aggregate, 3 * 128 + 8);
    let mut remaining = [RemainingBudget::default(); 5];
    if steps != 0 {
        let wal = retention_page::get(aggregate, 3 * 128 + 16);
        let peak = match retention_page::get(aggregate, 3 * 128 + 88) {
            9 => retention_page::catalog::CREATE_PEAK,
            16 => row::ROW_PEAK,
            _ => return Err(AgingObligationError::BudgetMismatch),
        };
        remaining[3] = RemainingBudget {
            steps,
            wal,
            protected: wal,
            peak,
            ..RemainingBudget::default()
        };
    }
    let candidate = DescriptorSpec {
        owner: OwnerState::System(SystemProgress {
            reference,
            phase: WorkPhase::Running,
            sequence: root.revision(),
            cursor: root.cursor(),
            limit: 64,
        }),
        chain,
        profile: profile_reference(input.profile),
        inventory,
        consumed_wal: root.consumed_wal(),
        consumed_control: root.consumed_control(),
        remaining,
        minimum_wal_lsn: chain.birth,
        last_mutation_sequence: 0,
    };
    // encode checks the candidate's canonical budget digest against this Root.
    encode(&candidate, input.profile, input.after_root, output)
}

#[cfg(test)]
#[path = "row_tests.rs"]
mod tests;
