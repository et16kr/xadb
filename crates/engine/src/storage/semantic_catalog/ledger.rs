//! Exact per-owner residuals for the single Missing Catalog root. These are
//! arithmetic/byte checks. Global inventory closure and admission still belong
//! to the original owner or selected recovery source.
use super::{Error, Operation, Result};
use crate::storage::{
    allocation_page::ReservationOwner,
    control::Profile,
    obligation::{self, OwnerState, RemainingBudget},
    retention_obligation::profile_reference,
    retention_page::catalog::{CREATE_PEAK, JobState, JobView},
    status_home::StatusHome,
    transaction_record::{
        RollbackState, TransactionState, TransactionStatus, TransactionStatusCell,
    },
};

fn debt(bytes: u64) -> RemainingBudget {
    RemainingBudget {
        steps: 1,
        wal: bytes,
        protected: bytes,
        peak: CREATE_PEAK,
        ..RemainingBudget::default()
    }
}

pub(crate) fn transaction_remaining(
    status: TransactionStatus,
    root: Option<JobView<'_>>,
) -> Result<[RemainingBudget; 5]> {
    if root.is_some_and(|job| !job.state().is_root() || job.source() != Some(status.reference())) {
        return Err(Error::IdentityMismatch);
    }
    let mut remaining = [RemainingBudget::default(); 5];
    let terminal = matches!(
        status.state(),
        TransactionState::Committed(_) | TransactionState::Aborted
    );
    if !terminal {
        remaining[2] = debt(56712);
    }
    if status.state() == TransactionState::Active {
        remaining[3] = debt(3040);
    }
    match root.map(|job| job.state()) {
        None => {
            if status.undo_head().is_some()
                || !matches!(
                    status.state(),
                    TransactionState::Active
                        | TransactionState::Aborting(RollbackState::Transaction { next: None })
                        | TransactionState::Committed(_)
                        | TransactionState::Aborted
                )
            {
                return Err(Error::OrderMismatch);
            }
        }
        Some(JobState::RootReserved) => {
            if status.state() != TransactionState::Active || status.undo_head().is_some() {
                return Err(Error::OrderMismatch);
            }
            let c = root.expect("selected root").catalog_bytes() as usize;
            remaining[0] = debt(Operation::Clr.frame_bytes(c)? as u64);
            remaining[1] = debt(Operation::Forward.frame_bytes(c)? as u64);
            remaining[1].pages = 2;
            remaining[1].undo_slots = 1;
        }
        Some(JobState::RootRetained) => {
            let root = root.expect("selected root");
            if status.undo_head() != root.undo() {
                return Err(Error::IdentityMismatch);
            }
            match status.state() {
                TransactionState::Active => (),
                TransactionState::Aborting(RollbackState::Transaction { next })
                    if next == root.undo() =>
                {
                    ()
                }
                TransactionState::Committed(_) => (),
                _ => return Err(Error::OrderMismatch),
            }
            if !terminal {
                remaining[0] =
                    debt(Operation::Clr.frame_bytes(root.catalog_bytes() as usize)? as u64);
            }
        }
        Some(JobState::RootReleasingCommitted) => {
            let TransactionState::Committed(q) = status.state() else {
                return Err(Error::OrderMismatch);
            };
            if status.undo_head().is_some() || q.get() != root.expect("selected root").commit_q() {
                return Err(Error::OrderMismatch);
            }
        }
        Some(JobState::RootReleasingCompensated) => {
            if status.undo_head().is_some()
                || !matches!(
                    status.state(),
                    TransactionState::Aborted
                        | TransactionState::Aborting(RollbackState::Transaction { next: None })
                )
            {
                return Err(Error::OrderMismatch);
            }
        }
        _ => return Err(Error::IdentityMismatch),
    }
    Ok(remaining)
}

/// Current closure only. Previous-owner hashes and exact consumed deltas need
/// the actual transition or accepted capture/WAL history; current bytes alone
/// cannot establish historical accounting, especially after a Root clears C.
pub(crate) fn validate_transaction(
    bytes: &[u8; 1536],
    profile: Profile,
    current: StatusHome,
    root: Option<JobView<'_>>,
) -> Result<()> {
    if profile.format() != 2 {
        return Err(Error::IdentityMismatch);
    }
    let TransactionStatusCell::Occupied(status) = current.core() else {
        return Err(Error::IdentityMismatch);
    };
    let descriptor = obligation::decode_descriptor(
        bytes,
        ReservationOwner::Transaction(status.reference()),
        profile_reference(profile),
    )?;
    if descriptor.owner != OwnerState::Transaction(status)
        || descriptor.minimum_wal_lsn != descriptor.chain.birth
        || descriptor.consumed_control != 0
        || descriptor.inventory.growth_extents != 0
        || descriptor.inventory.change_lsn < descriptor.chain.birth
        || descriptor.inventory.change_lsn > descriptor.chain.apply
    {
        return Err(Error::IdentityMismatch);
    }
    let reserved = root.is_some_and(|job| job.state() == JobState::RootReserved);
    if descriptor.inventory.items != if reserved { 3 } else { 0 }
        || descriptor.remaining != transaction_remaining(status, root)?
    {
        return Err(Error::BudgetMismatch);
    }
    let mutation = if let Some(job) = root {
        u64::from(job.state() != JobState::RootReserved)
    } else if matches!(
        status.state(),
        TransactionState::Active | TransactionState::Aborting(_)
    ) {
        0
    } else {
        if descriptor.last_mutation_sequence > 1 {
            return Err(Error::OrderMismatch);
        }
        descriptor.last_mutation_sequence
    };
    if descriptor.last_mutation_sequence != mutation
        || current.incoming()
            != u64::from(root.is_some_and(|job| job.state() == JobState::RootRetained))
    {
        return Err(Error::OrderMismatch);
    }
    Ok(())
}

/// Both live production and replay use the same explicit predecessor. A parsed
/// after descriptor cannot replace this transition or establish its own history.
pub(crate) struct TransactionTransition<'a> {
    pub(crate) operation: Operation,
    pub(crate) profile: Profile,
    pub(crate) lsn: u64,
    pub(crate) before: Option<&'a [u8; 1536]>,
    pub(crate) status: StatusHome,
    pub(crate) root: Option<JobView<'a>>,
    pub(crate) inventory: obligation::InventoryClaim,
    pub(crate) catalog_bytes: usize,
}

pub(crate) fn next_transaction(
    input: TransactionTransition<'_>,
    output: &mut [u8; 1536],
) -> Result<()> {
    use obligation::{DescriptorSpec, OwnerChain};
    let TransactionStatusCell::Occupied(status) = input.status.core() else {
        return Err(Error::IdentityMismatch);
    };
    if input.profile.format() != 2
        || input.lsn == 0
        || input.operation == Operation::AgingStart
        || !matches!(
            input.operation,
            Operation::Begin
                | Operation::Forward
                | Operation::RollbackStart
                | Operation::Clr
                | Operation::Commit
                | Operation::AbortEnd
                | Operation::Reserve
                | Operation::Cancel
        )
    {
        return Err(Error::InvalidInput);
    }
    let frame_bytes = input.operation.frame_bytes(input.catalog_bytes)? as u64;
    let mut after = if let Some(bytes) = input.before {
        if input.operation == Operation::Begin {
            return Err(Error::OrderMismatch);
        }
        let before = obligation::decode_descriptor(
            bytes,
            ReservationOwner::Transaction(status.reference()),
            profile_reference(input.profile),
        )?;
        let OwnerState::Transaction(previous) = before.owner else {
            return Err(Error::IdentityMismatch);
        };
        if matches!(
            previous.state(),
            TransactionState::Committed(_) | TransactionState::Aborted
        ) || before.chain.apply >= input.lsn
        {
            return Err(Error::OrderMismatch);
        }
        let old_inventory = before.inventory;
        let inventory_changed = matches!(
            input.operation,
            Operation::Reserve | Operation::Forward | Operation::Cancel
        );
        if inventory_changed {
            let (old_count, new_count) = if input.operation == Operation::Reserve {
                (0, 3)
            } else {
                (3, 0)
            };
            if old_inventory.items != old_count
                || input.inventory.items != new_count
                || input.inventory.change_lsn != input.lsn
                || input.inventory.growth_extents != 0
            {
                return Err(Error::BudgetMismatch);
            }
        } else if input.inventory != old_inventory {
            return Err(Error::BudgetMismatch);
        }
        let mut next = before;
        next.chain.previous = before.chain.apply;
        next.chain.apply = input.lsn;
        next.chain.groups = before.chain.groups.checked_add(1).ok_or(Error::Exhausted)?;
        next.consumed_wal = before
            .consumed_wal
            .checked_add(frame_bytes)
            .ok_or(Error::Exhausted)?;
        if input.operation == Operation::Forward {
            if before.last_mutation_sequence != 0 {
                return Err(Error::OrderMismatch);
            }
            next.last_mutation_sequence = 1;
        }
        next
    } else {
        if input.operation != Operation::Begin
            || input.inventory.items != 0
            || input.inventory.change_lsn != input.lsn
            || input.inventory.growth_extents != 0
        {
            return Err(Error::OrderMismatch);
        }
        DescriptorSpec {
            owner: OwnerState::Transaction(status),
            chain: OwnerChain {
                apply: input.lsn,
                previous: 0,
                birth: input.lsn,
                groups: 1,
            },
            profile: profile_reference(input.profile),
            inventory: input.inventory,
            consumed_wal: frame_bytes,
            consumed_control: 0,
            remaining: [RemainingBudget::default(); 5],
            minimum_wal_lsn: input.lsn,
            last_mutation_sequence: 0,
        }
    };
    after.owner = OwnerState::Transaction(status);
    after.inventory = input.inventory;
    after.remaining = transaction_remaining(status, input.root)?;
    obligation::encode_descriptor(&after, output)?;
    validate_transaction(output, input.profile, input.status, input.root)?;
    Ok(())
}

/// Root's exact aggregate already came from the stable queue plus one candidate
/// replacement. The borrowed descriptor is bound to both before and after Root.
pub(crate) fn next_aging(
    profile: Profile,
    before_descriptor: Option<&[u8; 1536]>,
    before_page: &[u8; crate::storage::page::PAGE_BYTES],
    after_page: &[u8; crate::storage::page::PAGE_BYTES],
    aggregate: &[u8; 640],
    output: &mut [u8; 1536],
) -> Result<()> {
    use crate::storage::{retention_obligation, retention_page::catalog};
    use foundation::codec::content_digest;
    use obligation::{DescriptorSpec, InventoryClaim, OwnerChain, SystemProgress, WorkPhase};
    let (_, root) = catalog::decode_root_page(after_page, profile.spec().namespace)?;
    let reference = root.aging().ok_or(Error::IdentityMismatch)?;
    let mut remaining = [RemainingBudget::default(); 5];
    let steps = super::u64_at(aggregate, 3 * 128 + 8);
    if steps != 0 {
        let wal = super::u64_at(aggregate, 3 * 128 + 16);
        remaining[3] = RemainingBudget {
            steps,
            wal,
            protected: wal,
            peak: CREATE_PEAK,
            ..RemainingBudget::default()
        };
    }
    let mut canonical = [0; 640];
    obligation::encode_budget_rows(&remaining, &mut canonical);
    if &canonical != aggregate || content_digest(aggregate) != *root.budget_digest() {
        return Err(Error::BudgetMismatch);
    }
    let (chain, inventory) = if let Some(bytes) = before_descriptor {
        let previous =
            retention_obligation::catalog::decode(bytes, profile, before_page)?.descriptor();
        if previous.chain.apply >= root.apply()
            || previous.consumed_wal > root.consumed_wal()
            || previous.consumed_control != root.consumed_control()
        {
            return Err(Error::OrderMismatch);
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
                    .ok_or(Error::Exhausted)?,
            },
            previous.inventory,
        )
    } else {
        let (_, previous) = catalog::decode_root_page(before_page, profile.spec().namespace)?;
        if previous.aging().is_some()
            || root.revision() != 2
            || root.consumed_wal() != 19672
            || root.consumed_control() != 0
        {
            return Err(Error::OrderMismatch);
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
    let after = DescriptorSpec {
        owner: OwnerState::System(SystemProgress {
            reference,
            phase: WorkPhase::Running,
            sequence: root.revision(),
            cursor: root.cursor(),
            limit: 64,
        }),
        chain,
        profile: profile_reference(profile),
        inventory,
        consumed_wal: root.consumed_wal(),
        consumed_control: root.consumed_control(),
        remaining,
        minimum_wal_lsn: chain.birth,
        last_mutation_sequence: 0,
    };
    retention_obligation::catalog::encode(&after, profile, after_page, output)?;
    Ok(())
}
