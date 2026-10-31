//! Five changed page after images and both handoff descriptors. `prepare`
//! computes every fallible after value before the first output byte, the
//! Root after page (in the A scratch) and the Aging transition bound to it
//! included; `pages` then only places them and re-encodes images whose
//! inputs `observe` checked.

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
        JOB_BYTES, JOB_COUNT, ROOT_BYTES,
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

/// RootJob after, written into `cell` (an A scratch region, M2): Forward
/// adopts it (Retained), CLR starts its compensated release at cursor0.
/// Unchanged fields keep the reserve-sealed bytes.
pub(super) fn job(observed: &Observed<'_>, cell: &mut [u8; JOB_BYTES]) -> Result<()> {
    let lsn = observed.header.lsn;
    cell.copy_from_slice(observed.job.bytes());
    if observed.forward {
        cell[6] = 2;
        put(cell, 72, lsn);
        cell[328..360].copy_from_slice(&content_digest(observed.transaction_bytes));
    } else {
        cell[6] = 4;
        cell[200..248].fill(0);
        cell[280..320].fill(0);
        put(cell, 272, lsn);
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
    put(cell, 376, lsn);
    row::decode_job(cell, observed.namespace)?;
    Ok(())
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

/// Tx then Aging descriptor after, frame bytes [416..3488).
pub(super) const DESCRIPTORS_BYTES: usize = 2 * 1536;

/// After values computed from the observation before any output byte. Every
/// byte buffer is an A scratch region (M2); only `status` is a stack value.
pub(super) struct Prepared<'s> {
    pub(super) job: &'s mut [u8; JOB_BYTES],
    pub(super) status: StatusHome,
    /// Root after cell, encoded into `page`.
    pub(super) root: &'s mut [u8; ROOT_BYTES],
    pub(super) undo: &'s mut [u8; UNDO_RECORD_BYTES],
    pub(super) descriptors: &'s mut [u8; DESCRIPTORS_BYTES],
    /// Status after cell that `pages` encodes.
    pub(super) status_cell: &'s mut [u8; STATUS_HOME_BYTES],
    /// Job census aggregate of `root` and of the Aging transition.
    pub(super) aggregate: &'s mut [u8; 640],
    /// Root after page: encoded here, bound to the Aging after, and copied
    /// into its `after` slot by `pages` (M1 keeps no Root page on the stack).
    pub(super) page: &'s mut [u8; PAGE_BYTES],
}

/// Root after cell and page, U after cell, the Tx descriptor after and the
/// whole Aging transition bound to that page. Root revision, the after Job
/// census summary, Tx chain/consumed arithmetic, the CLR residual step and
/// the Aging after census (`aging::next`: high-waters, terminal and pair
/// relations of the changed Job) are all checked here, before the write pass
/// writes any output byte; the Aging before was checked by `observe`.
pub(super) fn prepare(observed: &Observed<'_>, prepared: &mut Prepared<'_>) -> Result<()> {
    root(observed, prepared.job, prepared.root, prepared.aggregate)?;
    row::encode_root_page(prepared.root, observed.namespace, prepared.page)?;
    encode_material(observed, observed.forward, prepared.undo)?;
    let (transaction, aging_after) = prepared.descriptors.split_at_mut(1536);
    descriptor(observed, prepared.status, transaction)?;
    #[cfg(test)]
    super::super::group_tests::before_aging(prepared.job);
    let index = role_index(observed, RowPageRole::Root);
    aging::next(
        AgingTransition {
            profile: observed.profile,
            before_descriptor: Some(observed.aging),
            before_root: observed.pages[index].before,
            after_root: prepared.page,
            jobs: JobCells::original(observed.jobs).replace(observed.ordinal, prepared.job)?,
        },
        prepared.aggregate,
        aging_after,
    )?;
    Ok(())
}

/// Root after cell: revision+1, apply LSN, and the after Job census summary.
fn root(
    observed: &Observed<'_>,
    job: &[u8; JOB_BYTES],
    cell: &mut [u8; ROOT_BYTES],
    aggregate: &mut [u8; 640],
) -> Result<()> {
    let namespace = observed.namespace;
    let page = observed.pages[role_index(observed, RowPageRole::Root)];
    let (_, root) = row::decode_root_page(page.before, namespace)?;
    cell.copy_from_slice(root.bytes());
    put(
        cell,
        64,
        root.revision()
            .checked_add(1)
            .ok_or(InsertError::OrderMismatch)?,
    );
    put(cell, 80, observed.header.lsn);
    let cells = JobCells::original(observed.jobs).replace(observed.ordinal, job)?;
    let (masks, digest) = row::summarize_jobs(cells, namespace, aggregate)?;
    for (at, mask) in [96, 104, 112].into_iter().zip(masks) {
        put(cell, at, mask);
    }
    cell[128..160].copy_from_slice(&digest);
    cell[160..192].copy_from_slice(&content_digest(&aggregate[..]));
    Ok(())
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

/// Write the five after images in physical order, the Root's as a copy of
/// the page `prepare` checked. The frame is used only as row/cell scratch here and is cleared
/// by the caller before framing. Every input refusal already returned from
/// `observe` or `prepare`.
pub(super) fn pages(
    observed: &Observed<'_>,
    input: &GroupInput<'_>,
    prepared: &mut Prepared<'_>,
    scratch: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<()> {
    let namespace = observed.namespace;
    let lsn = observed.header.lsn;
    let job: &[u8] = &prepared.job[..];
    for (index, output) in after.iter_mut().enumerate() {
        let page = observed.pages[index];
        match page.role {
            RowPageRole::Root => output.copy_from_slice(&prepared.page[..]),
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
                prepared.status.encode(&mut prepared.status_cell[..])?;
                page::encode_page(
                    header(observed, index),
                    &[&prepared.status_cell[..]],
                    output,
                )?;
            }
            RowPageRole::Undo => {
                page::encode_page(header(observed, index), &[&prepared.undo[..]], output)?;
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
                        Some(output),
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

/// Tx descriptor after. `observe` proved the CLR residual exact, so its one
/// step and C/h debt are removed without underflow; checked anyway.
fn descriptor(observed: &Observed<'_>, status: StatusHome, output: &mut [u8]) -> Result<()> {
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
        let steps = old
            .steps
            .checked_sub(1)
            .ok_or(InsertError::BudgetMismatch)?;
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
    obligation::encode_descriptor(&spec, output)?;
    Ok(())
}

/// Locals of this module; the caller's `Prepared`, which holds the Root
/// after cell, is counted by its owner. The Job, aggregate and Status cell
/// buffers are A scratch regions (M2), so only the four reference parameters
/// that reach them count. `job`'s CLR budget rows stay on the stack.
#[allow(
    clippy::manual_bits,
    reason = "eight u64 scalar slots, counted in bytes"
)]
pub(super) fn local_source_carrier_bytes() -> usize {
    use std::mem::size_of;
    size_of::<Result<()>>()
        + 4 * size_of::<&mut [u8; 640]>()
        + size_of::<[&[u8]; 15]>()
        + size_of::<[RemainingBudget; 5]>()
        + size_of::<obligation::DescriptorSpec>()
        + size_of::<AgingTransition<'_>>()
        + 2 * size_of::<StatusHome>()
        + size_of::<leaf::LeafInput<'_>>()
        + size_of::<PageHeader>()
        + size_of::<PageKind>()
        + 8 * size_of::<u64>()
}
