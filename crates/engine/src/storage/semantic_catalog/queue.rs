//! Unpublished queue transitions shared by the original live and replay paths.
//! A QueueChange is a calculation request, never a source, reservation or
//! cleanup capability. Status/allocator/Undo and pin checks remain mandatory.
use super::{Error, Operation, Result, put32, put64};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{SystemWorkKind, SystemWorkRef},
        obligation::{self, RemainingBudget},
        page::PageRef,
        retention_page::{
            JOB_BYTES, JOB_COUNT, ROOT_BYTES,
            catalog::{self, JobCells, JobState, JobView, RootView},
        },
        transaction_record::{StatusSlotRef, TxnRef},
        undo_ref::UndoRef,
    },
};
use foundation::{codec::content_digest, identity::Generation};

pub(crate) struct ReservePlan {
    pub(crate) actor: TxnRef,
    pub(crate) status: StatusSlotRef,
    pub(crate) undo: UndoRef,
    pub(crate) pair: [PageRef; 2],
    pub(crate) catalog_bytes: u32,
    pub(crate) projection: [u8; 32],
}
pub(crate) enum QueueChange<'a> {
    AgingStart,
    Begin {
        actor: TxnRef,
        status: StatusSlotRef,
    },
    Reserve(&'a ReservePlan),
    Forward {
        ordinal: usize,
        sender: &'a [u8; 32],
    },
    Cancel {
        ordinal: usize,
    },
    Commit {
        ordinal: usize,
        sender: &'a [u8; 32],
    },
    AbortEnd {
        ordinal: usize,
        sender: &'a [u8; 32],
    },
    Clr {
        ordinal: usize,
    },
    Prune {
        ordinal: usize,
        q: u64,
    },
    ReleaseRole {
        ordinal: usize,
    },
    RootFree {
        ordinal: usize,
    },
    TerminalFinish {
        ordinal: usize,
    },
}
impl QueueChange<'_> {
    pub(crate) fn operation(&self) -> Operation {
        match self {
            Self::AgingStart => Operation::AgingStart,
            Self::Begin { .. } => Operation::Begin,
            Self::Reserve(_) => Operation::Reserve,
            Self::Forward { .. } => Operation::Forward,
            Self::Cancel { .. } => Operation::Cancel,
            Self::Commit { .. } => Operation::Commit,
            Self::AbortEnd { .. } => Operation::AbortEnd,
            Self::Clr { .. } => Operation::Clr,
            Self::Prune { .. } => Operation::Prune,
            Self::ReleaseRole { .. } => Operation::ReleaseRole,
            Self::RootFree { .. } => Operation::RootFree,
            Self::TerminalFinish { .. } => Operation::TerminalFinish,
        }
    }
    fn ordinal(&self) -> Option<usize> {
        match self {
            Self::AgingStart | Self::Begin { .. } | Self::Reserve(_) => None,
            Self::Forward { ordinal, .. }
            | Self::Cancel { ordinal }
            | Self::Commit { ordinal, .. }
            | Self::AbortEnd { ordinal, .. }
            | Self::Clr { ordinal }
            | Self::Prune { ordinal, .. }
            | Self::ReleaseRole { ordinal }
            | Self::RootFree { ordinal }
            | Self::TerminalFinish { ordinal } => Some(*ordinal),
        }
    }
}

fn next(value: u64) -> Result<u64> {
    value.checked_add(1).ok_or(Error::Exhausted)
}
fn require_state(job: JobView<'_>, state: JobState) -> Result<()> {
    if job.state() == state {
        Ok(())
    } else {
        Err(Error::OrderMismatch)
    }
}
fn first_free(root: RootView<'_>, cells: JobCells<'_>, namespace: Namespace) -> Result<usize> {
    for offset in 0..JOB_COUNT {
        let ordinal = (root.cursor() as usize + offset) % JOB_COUNT;
        if catalog::decode_job(cells.cell(ordinal), namespace)?.state() == JobState::Free {
            return Ok(ordinal);
        }
    }
    Err(Error::BudgetMismatch)
}
fn lease(
    bytes: &mut [u8; JOB_BYTES],
    before: JobView<'_>,
    actor: TxnRef,
    status: StatusSlotRef,
    lsn: u64,
    root: bool,
) -> Result<()> {
    require_state(before, JobState::Free)?;
    catalog::free_job(
        Generation::new(next(before.generation().get())?).map_err(|_| Error::Exhausted)?,
        bytes,
    );
    bytes[6] = 1;
    bytes[7] = if root { 2 } else { 1 };
    actor.encode(&mut bytes[16..64])?;
    status.encode(&mut bytes[80..120])?;
    put64(bytes, 64, lsn);
    put64(bytes, 376, lsn);
    Ok(())
}

/// Output cells and aggregate are caller scratch and may change on error.
/// The returned ordinal selects the one modified Job; AgingStart has none.
pub(crate) fn prepare(
    change: QueueChange<'_>,
    namespace: Namespace,
    before: RootView<'_>,
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    lsn: u64,
    root_after: &mut [u8; ROOT_BYTES],
    job_after: &mut [u8; JOB_BYTES],
    aggregate: &mut [u8; 640],
) -> Result<Option<usize>> {
    let original = JobCells::original(cells);
    catalog::verify_selected_census(before, namespace, original, aggregate)?;
    let operation = change.operation();
    if lsn <= before.apply() || (before.aging().is_none()) != (operation == Operation::AgingStart) {
        return Err(Error::OrderMismatch);
    }
    let ordinal = if matches!(operation, Operation::Begin | Operation::Reserve) {
        Some(first_free(before, original, namespace)?)
    } else {
        change.ordinal()
    };
    if ordinal.is_some_and(|value| value >= JOB_COUNT) {
        return Err(Error::InvalidInput);
    }
    root_after.copy_from_slice(before.bytes());
    put64(root_after, 64, next(before.revision())?);
    put64(root_after, 80, lsn);
    let mut c = 0usize;
    if let Some(ordinal) = ordinal {
        let prior = catalog::decode_job(original.cell(ordinal), namespace)?;
        job_after.copy_from_slice(prior.bytes());
        match change {
            QueueChange::Begin { actor, status } => {
                lease(job_after, prior, actor, status, lsn, false)?;
                put64(root_after, 88, next(before.issued())?);
            }
            QueueChange::Reserve(plan) => {
                if !(catalog::MIN_CATALOG_CELL..=catalog::MAX_CATALOG_CELL)
                    .contains(&plan.catalog_bytes)
                {
                    return Err(Error::InvalidInput);
                }
                lease(job_after, prior, plan.actor, plan.status, lsn, true)?;
                plan.undo.encode(&mut job_after[128..168])?;
                let sequence = next(before.undo_high())?;
                let object = next(before.object_high())?;
                put64(job_after, 168, sequence);
                put64(job_after, 176, object);
                put64(job_after, 184, 1);
                put64(job_after, 192, 1);
                plan.pair[0].encode(&mut job_after[200..224])?;
                plan.pair[1].encode(&mut job_after[224..248])?;
                put64(job_after, 248, 1);
                put64(job_after, 256, 1);
                put32(job_after, 280, plan.catalog_bytes);
                job_after[288..320].copy_from_slice(&plan.projection);
                put64(root_after, 88, next(before.issued())?);
                put64(root_after, 392, sequence);
                put64(root_after, 400, object);
            }
            QueueChange::Forward { sender, .. } => {
                require_state(prior, JobState::RootReserved)?;
                job_after[6] = 2;
                put64(job_after, 72, lsn);
                job_after[328..360].copy_from_slice(sender);
                c = prior.catalog_bytes() as usize;
            }
            QueueChange::Cancel { .. } => {
                require_state(prior, JobState::RootReserved)?;
                catalog::free_job(prior.generation(), job_after);
                put64(root_after, 384, next(before.canceled())?);
            }
            QueueChange::Commit { sender, .. } | QueueChange::AbortEnd { sender, .. } => {
                require_state(prior, JobState::TerminalLeased)?;
                let committed = operation == Operation::Commit;
                job_after[6] = 2;
                job_after[120] = if committed { 1 } else { 2 };
                put64(job_after, 72, lsn);
                if committed {
                    let q = next(before.commit_c())?;
                    put64(job_after, 264, q);
                    put64(root_after, 408, q);
                }
                job_after[328..360].copy_from_slice(sender);
            }
            QueueChange::Clr { .. } | QueueChange::Prune { .. } => {
                require_state(prior, JobState::RootRetained)?;
                c = prior.catalog_bytes() as usize;
                job_after[6] = if operation == Operation::Prune { 3 } else { 4 };
                job_after[200..248].fill(0);
                job_after[280..328].fill(0);
                put64(job_after, 272, lsn);
                if let QueueChange::Prune { q, .. } = change {
                    if q == 0 || q > before.commit_c() {
                        return Err(Error::OrderMismatch);
                    }
                    put64(job_after, 264, q);
                }
            }
            QueueChange::ReleaseRole { .. } => {
                if !prior.state().is_releasing() || prior.release_cursor() >= 3 {
                    return Err(Error::OrderMismatch);
                }
                job_after[121] = prior.release_cursor() + 1;
            }
            QueueChange::RootFree { .. } => {
                if !prior.state().is_releasing() || prior.release_cursor() != 3 {
                    return Err(Error::OrderMismatch);
                }
                catalog::free_job(prior.generation(), job_after);
                put64(root_after, 376, next(before.completed())?);
            }
            QueueChange::TerminalFinish { .. } => {
                require_state(prior, JobState::TerminalReady)?;
                // An outstanding RootJob owns actual cleanup after terminal.
                for other in 0..JOB_COUNT {
                    let job = catalog::decode_job(original.cell(other), namespace)?;
                    if job.state().is_root() && job.source() == prior.source() {
                        return Err(Error::OrderMismatch);
                    }
                }
                catalog::free_job(prior.generation(), job_after);
                put64(root_after, 376, next(before.completed())?);
            }
            QueueChange::AgingStart => return Err(Error::InvalidInput),
        }
        if job_after[6] != 0 {
            put64(job_after, 376, lsn);
            fill_budget(job_after)?;
        }
        catalog::decode_job(job_after, namespace)?;
    } else {
        if operation != Operation::AgingStart {
            return Err(Error::InvalidInput);
        }
        root_after[6] = 1;
        SystemWorkRef::new(
            namespace,
            SystemWorkKind::Aging,
            1,
            Generation::new(1).expect("one"),
        )
        .map_err(|_| Error::IdentityMismatch)?
        .encode(&mut root_after[16..64])
        .map_err(|_| Error::IdentityMismatch)?;
    }
    if !operation.handoff() {
        let cost = operation.frame_bytes(c)? as u64;
        put64(
            root_after,
            416,
            before
                .consumed_wal()
                .checked_add(cost)
                .ok_or(Error::Exhausted)?,
        );
        if let Some(ordinal) = ordinal {
            put64(root_after, 72, ((ordinal + 1) % JOB_COUNT) as u64);
        }
    }
    let selected = if let Some(ordinal) = ordinal {
        original.replace(ordinal, job_after)?
    } else {
        original
    };
    let (masks, digest) = catalog::summarize_jobs(selected, namespace, aggregate)?;
    for (index, mask) in masks.into_iter().enumerate() {
        put64(root_after, 96 + index * 8, mask);
    }
    root_after[128..160].copy_from_slice(&digest);
    root_after[160..192].copy_from_slice(&content_digest(aggregate));
    let root = catalog::decode_root(root_after, namespace)?;
    catalog::verify_selected_census(root, namespace, selected, aggregate)?;
    Ok(ordinal)
}

fn fill_budget(job: &mut [u8; JOB_BYTES]) -> Result<()> {
    let (steps, wal) = match (job[6], job[7]) {
        (1 | 2, 1) => (1, 55112),
        (1 | 2, 2) => (5, 311608 + 2 * u64::from(super::u32_at(job, 280))),
        (3 | 4, 2) if job[121] <= 3 => {
            let steps = 4 - u64::from(job[121]);
            (steps, steps * catalog::RELEASE_BYTES)
        }
        _ => return Err(Error::Corrupt),
    };
    let mut rows = [RemainingBudget::default(); 5];
    rows[3] = RemainingBudget {
        steps,
        wal,
        protected: wal,
        peak: catalog::CREATE_PEAK,
        ..RemainingBudget::default()
    };
    obligation::encode_budget_rows(
        &rows,
        (&mut job[384..1024]).try_into().expect("budget rows"),
    );
    Ok(())
}
