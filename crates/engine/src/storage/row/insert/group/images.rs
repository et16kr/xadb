//! Five changed page after images and both handoff descriptors. Every input
//! here was checked by `observe`; the codecs below recheck their own grammar.

use super::{
    CLR_FIXED, Change, GroupInput, InsertError, Observed, Result, debt, encode_material, leaf,
    require,
};
use crate::storage::{
    disk::RowPageRole,
    obligation::{self, InventoryClaim, OwnerChain, OwnerState, RemainingBudget},
    page::{self, PAGE_BYTES, PageHeader, PageKind, PageLsn},
    retention_obligation::row::{self as aging, AgingTransition},
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        catalog::RELEASE_BYTES,
        row::{self, JobCells},
    },
    status_home::{STATUS_HOME_BYTES, StatusHome},
    transaction_record::{
        RollbackState, TransactionState, TransactionStatus, TransactionStatusCell,
    },
    undo_record::UNDO_RECORD_BYTES,
};
use foundation::codec::content_digest;

fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// RootJob after: Forward adopts it (Retained), CLR starts its compensated
/// release at cursor0. Unchanged fields keep the reserve-sealed bytes.
pub(super) fn job(observed: &Observed<'_>) -> Result<[u8; JOB_BYTES]> {
    let lsn = observed.header.lsn;
    let mut cell = *observed.job.bytes();
    if observed.forward {
        cell[6] = 2;
        put(&mut cell, 72, lsn);
        cell[328..360].copy_from_slice(&content_digest(observed.transaction_bytes));
    } else {
        cell[6] = 4;
        cell[200..248].fill(0);
        cell[280..320].fill(0);
        put(&mut cell, 272, lsn);
        let wal = 4 * RELEASE_BYTES;
        let mut rows = [RemainingBudget::default(); 5];
        rows[3] = RemainingBudget {
            steps: 4,
            wal,
            protected: wal,
            peak: row::ROW_PEAK,
            ..RemainingBudget::default()
        };
        let budget: &mut [u8; 640] = (&mut cell[384..1024]).try_into().expect("fixed budget");
        obligation::encode_budget_rows(&rows, budget);
    }
    put(&mut cell, 376, lsn);
    row::decode_job(&cell, observed.namespace)?;
    Ok(cell)
}

/// Status after: Forward pushes U; CLR pops it back to H (or empty).
pub(super) fn status(observed: &Observed<'_>) -> Result<StatusHome> {
    let TransactionStatusCell::Occupied(core) = observed.status.core() else {
        return Err(InsertError::OrderMismatch);
    };
    let incoming = observed.status.incoming();
    let next = observed.predecessor.map(|link| link.reference);
    let (state, head, incoming) = if observed.forward {
        (
            TransactionState::Active,
            Some(observed.undo.reference),
            incoming.checked_add(1).ok_or(InsertError::OrderMismatch)?,
        )
    } else {
        let rollback = match core.state() {
            TransactionState::Aborting(RollbackState::Statement { target, .. }) => {
                RollbackState::Statement { target, next }
            }
            TransactionState::Aborting(RollbackState::Transaction { .. }) => {
                RollbackState::Transaction { next }
            }
            _ => return Err(InsertError::OrderMismatch),
        };
        (
            TransactionState::Aborting(rollback),
            next,
            incoming.checked_sub(1).ok_or(InsertError::OrderMismatch)?,
        )
    };
    require(
        head.is_some() == (incoming != 0),
        InsertError::OrderMismatch,
    )?;
    let core = TransactionStatus::new(observed.actor, state, head)?;
    Ok(StatusHome::new(
        TransactionStatusCell::Occupied(core),
        incoming,
    )?)
}

fn header(observed: &Observed<'_>, index: usize) -> PageHeader {
    let page = observed.pages[index];
    PageHeader {
        identity: page.identity,
        kind: page.kind,
        level: 0,
        lsn: PageLsn(observed.header.lsn),
    }
}

/// Write all five after images in physical order. The frame is used only as
/// row/cell scratch here and is cleared by the caller before framing.
pub(super) fn pages(
    observed: &Observed<'_>,
    input: &GroupInput<'_>,
    job: &[u8; JOB_BYTES],
    status: StatusHome,
    scratch: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<()> {
    let namespace = observed.namespace;
    let lsn = observed.header.lsn;
    for (index, output) in after.iter_mut().enumerate() {
        let page = observed.pages[index];
        match page.role {
            RowPageRole::Root => {
                let (_, root) = row::decode_root_page(page.before, namespace)?;
                let mut cell = *root.bytes();
                put(
                    &mut cell,
                    64,
                    root.revision()
                        .checked_add(1)
                        .ok_or(InsertError::OrderMismatch)?,
                );
                put(&mut cell, 80, lsn);
                let cells = JobCells::original(observed.jobs).replace(observed.ordinal, job)?;
                let mut aggregate = [0; 640];
                let (masks, digest) = row::summarize_jobs(cells, namespace, &mut aggregate)?;
                for (at, mask) in [96, 104, 112].into_iter().zip(masks) {
                    put(&mut cell, at, mask);
                }
                cell[128..160].copy_from_slice(&digest);
                cell[160..192].copy_from_slice(&content_digest(&aggregate));
                row::encode_root_page(&cell, namespace, output)?;
            }
            RowPageRole::RootJob => {
                let base = observed.ordinal / 15 * 15;
                let count = if base == 60 { JOB_COUNT - 60 } else { 15 };
                let mut cells: [&[u8]; 15] = [&[]; 15];
                for (slot, cell) in cells[..count].iter_mut().enumerate() {
                    *cell = if base + slot == observed.ordinal {
                        job
                    } else {
                        &observed.jobs[base + slot]
                    };
                }
                row::encode_jobs_page(namespace, (base / 15) as u8, lsn, &cells[..count], output)?;
            }
            RowPageRole::Status => {
                let mut cell = [0; STATUS_HOME_BYTES];
                status.encode(&mut cell)?;
                page::encode_page(header(observed, index), &[&cell], output)?;
            }
            RowPageRole::Undo => {
                let mut cell = [0; UNDO_RECORD_BYTES];
                encode_material(observed, observed.forward, &mut cell)?;
                page::encode_page(header(observed, index), &[&cell], output)?;
            }
            RowPageRole::RowLeaf => {
                let lsn = PageLsn(lsn);
                match input.change {
                    Change::Forward { canonical } => {
                        leaf::produce(
                            leaf::LeafInput {
                                namespace,
                                expected: observed.leaf,
                                before: page.before,
                                schema: input.schema,
                                canonical,
                                version: super::current_version(
                                    observed.actor,
                                    observed.row,
                                    observed.job,
                                    observed.undo.reference,
                                )?,
                                lsn,
                            },
                            scratch,
                            output,
                        )?;
                    }
                    Change::Clr { .. } => leaf::remove(
                        namespace,
                        observed.leaf,
                        input.schema,
                        page.before,
                        observed.row,
                        lsn,
                        output,
                    )?,
                }
            }
        }
        require(
            page::decode_page(output, page.identity, page.kind)?
                .header()
                .lsn
                .0
                == lsn,
            InsertError::OrderMismatch,
        )?;
    }
    Ok(())
}

fn role_index(observed: &Observed<'_>, role: RowPageRole) -> usize {
    observed
        .pages
        .iter()
        .position(|page| page.role == role)
        .expect("five distinct roles")
}

/// Tx and Aging descriptor after images at their fixed frame positions.
pub(super) fn descriptors(
    observed: &Observed<'_>,
    job: &[u8; JOB_BYTES],
    status: StatusHome,
    after: &[[u8; PAGE_BYTES]; 5],
    output: &mut [u8],
) -> Result<()> {
    let TransactionStatusCell::Occupied(core) = status.core() else {
        return Err(InsertError::OrderMismatch);
    };
    let lsn = observed.header.lsn;
    let mut spec = observed.transaction;
    spec.owner = OwnerState::Transaction(core);
    spec.chain = OwnerChain {
        apply: lsn,
        previous: spec.chain.apply,
        birth: spec.chain.birth,
        groups: spec
            .chain
            .groups
            .checked_add(1)
            .ok_or(InsertError::OrderMismatch)?,
    };
    spec.consumed_wal = spec
        .consumed_wal
        .checked_add(observed.frame_len as u64)
        .ok_or(InsertError::BudgetMismatch)?;
    if observed.forward {
        spec.inventory = InventoryClaim {
            change_lsn: lsn,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        };
        spec.remaining[1] = RemainingBudget::default();
    } else {
        let clr = debt(CLR_FIXED, observed.cell, observed.predecessor.is_some(), 0);
        let old = spec.remaining[0];
        let steps = old.steps - 1;
        let wal = old
            .wal
            .checked_sub(clr.wal)
            .ok_or(InsertError::BudgetMismatch)?;
        spec.remaining[0] = if steps == 0 {
            require(wal == 0, InsertError::BudgetMismatch)?;
            RemainingBudget::default()
        } else {
            RemainingBudget {
                steps,
                wal,
                protected: wal,
                peak: row::ROW_PEAK,
                ..RemainingBudget::default()
            }
        };
    }
    obligation::encode_descriptor(&spec, &mut output[416..1952])?;

    let root = role_index(observed, RowPageRole::Root);
    let mut aggregate = [0; 640];
    aging::next(
        AgingTransition {
            profile: observed.profile,
            before_descriptor: Some(observed.aging),
            before_root: observed.pages[root].before,
            after_root: &after[root],
            jobs: JobCells::original(observed.jobs).replace(observed.ordinal, job)?,
        },
        &mut aggregate,
        &mut output[1952..3488],
    )?;
    Ok(())
}

pub(super) fn local_source_carrier_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<[u8; JOB_BYTES]>()
        + 2 * size_of::<[u8; 640]>()
        + size_of::<[u8; 512]>()
        + size_of::<[&[u8]; 15]>()
        + size_of::<[u8; STATUS_HOME_BYTES]>()
        + size_of::<[u8; UNDO_RECORD_BYTES]>()
        + size_of::<obligation::DescriptorSpec>()
        + size_of::<AgingTransition<'_>>()
        + 2 * size_of::<StatusHome>()
        + size_of::<leaf::LeafInput<'_>>()
        + size_of::<PageHeader>()
        + size_of::<PageKind>()
        + 8 * size_of::<u64>()
}
