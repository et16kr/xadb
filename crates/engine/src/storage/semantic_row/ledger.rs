//! Current actor/descriptor residuals for multi-statement Missing Row INSERT.
//! The caller separately closes the whole Root/Job census, actual page graph,
//! statement mark identity, predecessor history and reservation admission.
//! Inventory count/token checks do not prove the normalized UndoSlot Item96
//! digest, the original Free U, its allocator mapping or physical ownership.
use super::{Error, Result};
use crate::storage::{
    allocation_page::ReservationOwner,
    control::Profile,
    obligation::{self, OwnerState, RemainingBudget},
    retention_obligation::profile_reference,
    retention_page::{
        JOB_COUNT,
        row::{self, JobCells, JobScope, JobState, JobView},
    },
    status_home::StatusHome,
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell,
    },
};

struct ActorSummary<'a> {
    terminal: JobView<'a>,
    reserved: Option<JobView<'a>>,
    maximum_mutation: u64,
    clr_steps: u64,
    clr_wal: u64,
    latest_step: u64,
}
fn is_terminal(status: TransactionStatus) -> bool {
    matches!(
        status.state(),
        TransactionState::Committed(_) | TransactionState::Aborted
    )
}

fn observe<'a>(
    profile: Profile,
    home: StatusHome,
    location: StatusSlotRef,
    jobs: JobCells<'a>,
) -> Result<(TransactionStatus, ActorSummary<'a>)> {
    let TransactionStatusCell::Occupied(status) = home.core() else {
        return Err(Error::IdentityMismatch);
    };
    let actor = status.reference();
    let namespace = profile.spec().namespace;
    if profile.format() != 3
        || actor.id().database != namespace.database
        || actor.id().timeline != namespace.timeline
        || actor.generation() != location.generation()
    {
        return Err(Error::IdentityMismatch);
    }
    let mut terminal = None;
    let mut reserved = None;
    let mut head = None;
    let mut retained = 0u64;
    let mut maximum_sequence = 0;
    let mut maximum_mutation = 0;
    let mut clr_steps = 0u64;
    let mut clr_wal = 0u64;
    let mut latest_step = 0;
    let mut target_found = false;
    let target = match status.state() {
        TransactionState::Aborting(RollbackState::Statement { target, .. }) => target,
        _ => None,
    };
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(jobs.cell(ordinal), namespace)?;
        if job
            .source()
            .is_some_and(|source| source.id() == actor.id() && source != actor)
        {
            return Err(Error::IdentityMismatch);
        }
        if job.source() != Some(actor) {
            continue;
        }
        if job.scope() != Some(JobScope::Row) || job.status() != Some(location) {
            return Err(Error::IdentityMismatch);
        }
        latest_step = latest_step.max(job.last_step());
        if job.state().is_terminal() {
            if terminal.replace(job).is_some() {
                return Err(Error::IdentityMismatch);
            }
            continue;
        }
        maximum_mutation = maximum_mutation.max(job.mutation());
        let mut earlier_retained = false;
        for other_index in 0..JOB_COUNT {
            if other_index == ordinal {
                continue;
            }
            let other = row::decode_job(jobs.cell(other_index), namespace)?;
            if other.source() != Some(actor) || !other.state().is_root() {
                continue;
            }
            // Sequence/mutation advance together; statement identities may have
            // gaps after cancellation but cannot move backwards within an actor.
            if job.undo_sequence() == other.undo_sequence()
                || job.row_id() == other.row_id()
                || job.undo().is_some_and(|a| {
                    other.undo().is_some_and(|b| {
                        a.page().file == b.page().file && a.page().page_no == b.page().page_no
                    })
                })
            {
                return Err(Error::IdentityMismatch);
            }
            if job.undo_sequence() > other.undo_sequence() {
                if job.mutation() <= other.mutation() || job.statement() < other.statement() {
                    return Err(Error::OrderMismatch);
                }
                earlier_retained |= other.state() == JobState::RootRetained;
            } else if job.mutation() >= other.mutation() || job.statement() > other.statement() {
                return Err(Error::OrderMismatch);
            }
            if job.state() == JobState::RootReserved && job.undo_sequence() <= other.undo_sequence()
            {
                return Err(Error::OrderMismatch);
            }
        }
        match job.state() {
            JobState::RootReserved => {
                if reserved.replace(job).is_some()
                    || status.state() != TransactionState::Active
                    || job.has_previous_transaction() != Some(status.undo_head().is_some())
                {
                    return Err(Error::OrderMismatch);
                }
            }
            JobState::RootRetained => {
                if status.state() == TransactionState::Aborted
                    || job.has_previous_transaction() != Some(earlier_retained)
                {
                    return Err(Error::OrderMismatch);
                }
                retained += 1;
                if job.undo_sequence() > maximum_sequence {
                    maximum_sequence = job.undo_sequence();
                    head = job.undo();
                }
                target_found |= target.is_some() && job.undo() == target;
            }
            JobState::RootReleasingCommitted => {
                let TransactionState::Committed(q) = status.state() else {
                    return Err(Error::OrderMismatch);
                };
                if job.commit_q() != q.get() {
                    return Err(Error::OrderMismatch);
                }
            }
            JobState::RootReleasingCompensated => {
                if job.release_cursor() != 0 && !is_terminal(status) {
                    return Err(Error::OrderMismatch);
                }
            }
            _ => return Err(Error::IdentityMismatch),
        }
        if !is_terminal(status)
            && matches!(job.state(), JobState::RootReserved | JobState::RootRetained)
        {
            let wal = 91_096
                + u64::from(job.cell_bytes())
                + 128
                    * u64::from(
                        job.has_previous_transaction()
                            .ok_or(Error::IdentityMismatch)?,
                    );
            clr_steps = clr_steps.checked_add(1).ok_or(Error::Exhausted)?;
            clr_wal = clr_wal.checked_add(wal).ok_or(Error::Exhausted)?;
        }
    }
    let terminal = terminal.ok_or(Error::IdentityMismatch)?;
    let terminal_matches = match status.state() {
        TransactionState::Active | TransactionState::Aborting(_) => {
            terminal.state() == JobState::TerminalLeased
        }
        TransactionState::Committed(q) => {
            terminal.state() == JobState::TerminalReady
                && terminal.outcome() == 1
                && terminal.commit_q() == q.get()
        }
        TransactionState::Aborted => {
            terminal.state() == JobState::TerminalReady && terminal.outcome() == 2
        }
    };
    if !terminal_matches || home.incoming() != retained || status.undo_head() != head {
        return Err(Error::IdentityMismatch);
    }
    match status.state() {
        TransactionState::Aborting(RollbackState::Statement { target, next })
            if next != head || target.is_some() && !target_found =>
        {
            return Err(Error::OrderMismatch);
        }
        TransactionState::Aborting(RollbackState::Transaction { next }) if next != head => {
            return Err(Error::OrderMismatch);
        }
        _ => (),
    }
    Ok((
        status,
        ActorSummary {
            terminal,
            reserved,
            maximum_mutation,
            clr_steps,
            clr_wal,
            latest_step,
        },
    ))
}

fn debt(steps: u64, wal: u64) -> RemainingBudget {
    if steps == 0 {
        return RemainingBudget::default();
    }
    RemainingBudget {
        steps,
        wal,
        protected: wal,
        peak: row::ROW_PEAK,
        ..RemainingBudget::default()
    }
}
fn remaining(
    status: TransactionStatus,
    observed: &ActorSummary<'_>,
    statement_credit_available: bool,
) -> Result<[RemainingBudget; 5]> {
    if statement_credit_available && status.state() != TransactionState::Active
        || observed.reserved.is_some() && !statement_credit_available
    {
        return Err(Error::BudgetMismatch);
    }
    let mut rows = [RemainingBudget::default(); 5];
    rows[0] = debt(observed.clr_steps, observed.clr_wal);
    if let Some(job) = observed.reserved {
        rows[1] = debt(
            1,
            91_304
                + u64::from(job.cell_bytes())
                + 128
                    * u64::from(
                        job.has_previous_transaction()
                            .ok_or(Error::IdentityMismatch)?,
                    ),
        );
        rows[1].undo_slots = 1;
    }
    if !is_terminal(status) {
        rows[2] = debt(1, 56_712);
    }
    let control_steps = match status.state() {
        TransactionState::Active => {
            if statement_credit_available {
                3
            } else {
                1
            }
        }
        TransactionState::Aborting(RollbackState::Statement { .. }) => 2,
        _ => 0,
    };
    rows[3] = debt(control_steps, control_steps * 3_040);
    Ok(rows)
}

/// Canonical residual observation, also usable after a terminal descriptor has
/// left ATT and Aging changes Status.head. The boolean observes available
/// statement credit; it does not reserve it or authenticate a live mark.
pub(crate) fn transaction_remaining(
    profile: Profile,
    home: StatusHome,
    location: StatusSlotRef,
    jobs: JobCells<'_>,
    statement_credit_available: bool,
) -> Result<[RemainingBudget; 5]> {
    let (status, observed) = observe(profile, home, location, jobs)?;
    remaining(status, &observed, statement_credit_available)
}

/// Validate a current ATT or just-produced terminal descriptor. A descriptor
/// removed at terminal must not be reused against later Aging-mutated Status.
pub(crate) fn validate_transaction(
    bytes: &[u8; 1536],
    profile: Profile,
    home: StatusHome,
    location: StatusSlotRef,
    jobs: JobCells<'_>,
) -> Result<()> {
    let (status, observed) = observe(profile, home, location, jobs)?;
    let descriptor = obligation::decode_descriptor(
        bytes,
        ReservationOwner::Transaction(status.reference()),
        profile_reference(profile),
    )?;
    if descriptor.owner != OwnerState::Transaction(status)
        || descriptor.minimum_wal_lsn != descriptor.chain.birth
        || descriptor.chain.birth != observed.terminal.lease_lsn()
        || descriptor.consumed_control != 0
        || descriptor.inventory.growth_extents != 0
        || descriptor.inventory.change_lsn < descriptor.chain.birth
        || descriptor.inventory.change_lsn > descriptor.chain.apply
    {
        return Err(Error::IdentityMismatch);
    }
    if descriptor.chain.apply < observed.latest_step
        || descriptor.last_mutation_sequence < observed.maximum_mutation
        || is_terminal(status) && descriptor.chain.apply != observed.terminal.adoption_lsn()
    {
        return Err(Error::OrderMismatch);
    }
    if let Some(job) = observed.reserved
        && (descriptor.chain.apply != job.lease_lsn()
            || descriptor.inventory.change_lsn != job.lease_lsn()
            || descriptor.last_mutation_sequence != job.mutation())
    {
        return Err(Error::OrderMismatch);
    }
    let statement_credit_available =
        status.state() == TransactionState::Active && descriptor.remaining[3].steps == 3;
    if descriptor.inventory.items != u64::from(observed.reserved.is_some())
        || descriptor.remaining != remaining(status, &observed, statement_credit_available)?
    {
        return Err(Error::BudgetMismatch);
    }
    Ok(())
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
