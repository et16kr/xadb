//! Complete stable Job3 cell census, using caller scratch. This does not
//! prove the external Status/Undo/head/allocator/descriptor relationships.
use super::{
    ContentHasher, JOB_BYTES, JOB_COUNT, JobScope, JobState, JobView, Namespace, Result,
    RetentionError, RootView, content_digest, decode_job, get, put, same_address,
};

/// The one accumulator lives in caller scratch. Additive debt is checked sum;
/// one Aging executor's peak is max. It is not global concurrent Tx admission.
fn add_budget(output: &mut [u8; 640], job: JobView<'_>) -> Result<()> {
    if job.state() == JobState::Free {
        return Ok(());
    }
    for row in 0..5 {
        for at in (8..120).step_by(8) {
            let offset = row * 128 + at;
            let value = get(job.budget(), offset);
            let current = get(output, offset);
            put(
                output,
                offset,
                if (64..112).contains(&at) {
                    current.max(value)
                } else {
                    current
                        .checked_add(value)
                        .ok_or(RetentionError::BudgetMismatch)?
                },
            );
        }
    }
    Ok(())
}

/// One Row or Catalog group changes at most one Job cell. This view retains the
/// original stable census and substitutes caller scratch without copying 64KiB.
#[derive(Clone, Copy)]
pub(crate) struct JobCells<'a> {
    original: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    replacement: Option<(usize, &'a [u8; JOB_BYTES])>,
}
impl<'a> JobCells<'a> {
    pub(crate) fn original(original: &'a [[u8; JOB_BYTES]; JOB_COUNT]) -> Self {
        Self {
            original,
            replacement: None,
        }
    }
    pub(crate) fn replace(self, ordinal: usize, cell: &'a [u8; JOB_BYTES]) -> Result<Self> {
        if ordinal >= JOB_COUNT || self.replacement.is_some() {
            return Err(RetentionError::InvalidInput);
        }
        Ok(Self {
            original: self.original,
            replacement: Some((ordinal, cell)),
        })
    }
    pub(crate) fn cell(self, ordinal: usize) -> &'a [u8; JOB_BYTES] {
        if let Some((selected, cell)) = self.replacement
            && selected == ordinal
        {
            return cell;
        }
        &self.original[ordinal]
    }
}

/// Canonical sums used while constructing an unpublished Root. Full census
/// still checks actor, identity, high-water and terminal relationships below.
pub(crate) fn summarize_jobs(
    cells: JobCells<'_>,
    namespace: Namespace,
    aggregate: &mut [u8; 640],
) -> Result<([u64; 3], [u8; 32])> {
    aggregate.fill(0);
    for row in 0..5 {
        aggregate[row * 128] = row as u8 + 1;
    }
    let mut hash = ContentHasher::new();
    let mut masks = [0u64; 3];
    for ordinal in 0..JOB_COUNT {
        let cell = cells.cell(ordinal);
        let job = decode_job(cell, namespace)?;
        masks[job.state().mask_index()] |= 1u64 << ordinal;
        add_budget(aggregate, job)?;
        hash.update(cell);
    }
    Ok((masks, hash.digest()))
}

/// Full stable64-cell census. Actual Status/Undo/allocator/ATT relations remain
/// the original semantic caller's separate bidirectional obligation.
pub(crate) fn verify_census(
    root: RootView<'_>,
    namespace: Namespace,
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    aggregate: &mut [u8; 640],
) -> Result<()> {
    verify_selected_census(root, namespace, JobCells::original(cells), aggregate)
}

pub(crate) fn verify_selected_census(
    root: RootView<'_>,
    namespace: Namespace,
    cells: JobCells<'_>,
    aggregate: &mut [u8; 640],
) -> Result<()> {
    // A RootView and a namespace may each be valid but belong to different
    // databases. Bind them before accepting the independently supplied bank.
    if root.bytes()[192..208] != namespace.database.bytes()
        || get(root.bytes(), 208) != namespace.timeline
    {
        return Err(RetentionError::IdentityMismatch);
    }
    aggregate.fill(0);
    for row in 0..5 {
        aggregate[row * 128] = row as u8 + 1;
    }
    let mut hash = ContentHasher::new();
    let mut masks = [0u64; 3];
    let mut active = 0usize;
    for ordinal in 0..JOB_COUNT {
        let cell = cells.cell(ordinal);
        let job = decode_job(cell, namespace)?;
        masks[job.state().mask_index()] |= 1u64 << ordinal;
        if job.last_step() > root.apply()
            || (root.aging().is_none() && job.generation().get() != 1)
            || job.commit_q() > root.commit_c()
            || job.undo_sequence() > root.undo_high()
            || job.row_id().is_some_and(|id| id.get() > root.row_high())
            || job
                .object()
                .is_some_and(|object| object.object.get() > root.object_high())
        {
            return Err(RetentionError::OrderMismatch);
        }
        if job.state() == JobState::TerminalLeased {
            active += 1;
        }
        if job.state().is_root() {
            verify_terminal(job, namespace, cells)?;
        }
        for previous in 0..ordinal {
            verify_pair(
                job,
                decode_job(cells.cell(previous), namespace)?,
                namespace,
                cells,
            )?;
        }
        add_budget(aggregate, job)?;
        hash.update(cell);
    }
    if active > 32
        || masks != root.masks()
        || hash.digest() != *root.job_digest()
        || content_digest(aggregate) != *root.budget_digest()
    {
        return Err(RetentionError::Corrupt);
    }
    Ok(())
}

fn verify_terminal(root: JobView<'_>, namespace: Namespace, cells: JobCells<'_>) -> Result<()> {
    let mut found = false;
    for ordinal in 0..JOB_COUNT {
        let terminal = decode_job(cells.cell(ordinal), namespace)?;
        if !terminal.state().is_terminal() || terminal.source() != root.source() {
            continue;
        }
        if found
            || terminal.status() != root.status()
            || terminal.scope() != root.scope()
            || terminal.lease_lsn() >= root.lease_lsn()
        {
            return Err(RetentionError::IdentityMismatch);
        }
        found = true;
        let valid = match root.state() {
            JobState::RootReserved => terminal.state() == JobState::TerminalLeased,
            JobState::RootRetained => {
                terminal.state() == JobState::TerminalLeased || terminal.outcome() == 1
            }
            JobState::RootReleasingCommitted => {
                terminal.outcome() == 1 && terminal.commit_q() == root.commit_q()
            }
            JobState::RootReleasingCompensated => {
                (terminal.outcome() == 2
                    || root.scope() == Some(JobScope::Row) && terminal.outcome() == 1)
                    || root.release_cursor() == 0 && terminal.state() == JobState::TerminalLeased
            }
            _ => false,
        };
        if !valid {
            return Err(RetentionError::OrderMismatch);
        }
        if terminal.state() == JobState::TerminalReady {
            let terminal_lsn = terminal.adoption_lsn();
            let ordered = match root.state() {
                JobState::RootRetained => root.adoption_lsn() < terminal_lsn,
                JobState::RootReleasingCommitted => terminal_lsn < root.release_start(),
                JobState::RootReleasingCompensated => {
                    root.release_start() < terminal_lsn
                        && (root.release_cursor() == 0 || terminal_lsn < root.last_step())
                }
                _ => false,
            };
            if !ordered {
                return Err(RetentionError::OrderMismatch);
            }
        }
    }
    if !found {
        return Err(RetentionError::IdentityMismatch);
    }
    Ok(())
}

fn verify_pair(
    a: JobView<'_>,
    b: JobView<'_>,
    namespace: Namespace,
    cells: JobCells<'_>,
) -> Result<()> {
    if a.state() == JobState::Free || b.state() == JobState::Free {
        return Ok(());
    }
    let (actor_a, actor_b) = (a.source().expect("occupied"), b.source().expect("occupied"));
    let (status_a, status_b) = (a.status().expect("occupied"), b.status().expect("occupied"));
    if actor_a.id() == actor_b.id() {
        if actor_a != actor_b
            || status_a != status_b
            || a.scope() != b.scope()
            || a.state().is_terminal() && b.state().is_terminal()
            || a.state().is_root()
                && b.state().is_root()
                && (a.scope() != Some(JobScope::Row)
                    || a.state() == JobState::RootReserved && b.state() == JobState::RootReserved
                    || a.mutation() == b.mutation())
        {
            return Err(RetentionError::IdentityMismatch);
        }
    } else if same_address(status_a.page(), status_b.page()) {
        return Err(RetentionError::IdentityMismatch);
    }
    if a.undo()
        .is_some_and(|u| same_address(u.page(), status_b.page()))
        || b.undo()
            .is_some_and(|u| same_address(u.page(), status_a.page()))
        || a.undo()
            .zip(b.undo())
            .is_some_and(|(u, v)| same_address(u.page(), v.page()))
        || a.state().is_root() && b.state().is_root() && a.undo_sequence() == b.undo_sequence()
        || a.row_id().zip(b.row_id()).is_some_and(|(u, v)| u == v)
    {
        return Err(RetentionError::IdentityMismatch);
    }
    let same_object = a
        .object()
        .zip(b.object())
        .is_some_and(|(a, b)| a.object == b.object);
    if same_object {
        if a.object() != b.object()
            || a.scope() == Some(JobScope::Catalog) && b.scope() == Some(JobScope::Catalog)
            || a.pair().zip(b.pair()).is_some_and(|(a, b)| a != b)
        {
            return Err(RetentionError::IdentityMismatch);
        }
        // A retained CREATE root may still describe an uncommitted table.
        // Only a committed Catalog creator can precede another actor's Row.
        for (catalog, row) in [(a, b), (b, a)] {
            if catalog.scope() == Some(JobScope::Catalog) {
                if !matches!(
                    catalog.state(),
                    JobState::RootRetained | JobState::RootReleasingCommitted
                ) {
                    return Err(RetentionError::OrderMismatch);
                }
                let mut committed = false;
                for ordinal in 0..JOB_COUNT {
                    let terminal = decode_job(cells.cell(ordinal), namespace)?;
                    if terminal.state() == JobState::TerminalReady
                        && terminal.source() == catalog.source()
                        && terminal.outcome() == 1
                        && terminal.adoption_lsn() < row.lease_lsn()
                    {
                        committed = true;
                    }
                }
                if !committed {
                    return Err(RetentionError::OrderMismatch);
                }
            }
        }
    }
    for (left, right) in [(a, b), (b, a)] {
        if let Some(pair) = left.pair() {
            for (role, candidate) in pair.into_iter().enumerate() {
                if same_address(candidate, right.status().expect("occupied").page())
                    || right
                        .undo()
                        .is_some_and(|u| same_address(candidate, u.page()))
                    || right.pair().is_some_and(|other| {
                        other.iter().enumerate().any(|(other_role, p)| {
                            same_address(candidate, *p)
                                && (!same_object || role != other_role || candidate != *p)
                        })
                    })
                {
                    return Err(RetentionError::IdentityMismatch);
                }
            }
        }
    }
    Ok(())
}
