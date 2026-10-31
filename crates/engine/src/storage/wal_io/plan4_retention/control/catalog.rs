//! Current admission totals from the original live mixed bank. A caller that
//! runs `history::validate_live` first proves that bank against installed
//! history and actual Status pages; admission-only callers (empty-actor and
//! Catalog terminal entries) rely on the checks here alone, including the Row
//! ledger's Status-free subset for a Row ATT descriptor. This arithmetic check
//! neither creates a status nor admits an operation.
//! Catalog per-actor selection and semantics skip Row-scope cells; the
//! Terminal/ATT correspondence and every debt sum read all cells through the
//! Row codec (XADB-MODULE-ROW-BEGIN-001, "replay와 분석 gate").
use super::*;
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    retention_page::{
        catalog3::{self as catalog, JobCells, JobState, JobView},
        row as job3,
    },
    semantic_catalog::{inventory, ledger},
};
use foundation::{codec::content_digest, identity::Generation};

fn bad() -> RetentionError {
    unchanged(RetentionPhase::Census, RetentionCause::Invariant)
}
fn semantic(error: crate::storage::semantic_catalog::Error) -> RetentionError {
    unchanged(
        RetentionPhase::Census,
        RetentionCause::CatalogSemantic(error),
    )
}
/// Exact Job3 decode of one cell of either scope (the Row codec).
fn job(bytes: &[u8], profile: crate::storage::control::Profile) -> Result<job3::JobView<'_>> {
    job3::decode_job(bytes, profile.spec().namespace)
        .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Retention(e)))
}

pub(super) fn check(
    bank: &RetentionLedger<'_, '_, '_>,
    profile: crate::storage::control::Profile,
    prefix: WalPrefix,
) -> Result<(u64, u64)> {
    if profile.format() != 3 || bank.pending.is_some() || bank.adopted_local > bank.issued_local {
        return Err(bad());
    }
    let mut wal = 0u64;
    let mut steps = 0u64;
    for i in 0..ATT_SLOTS {
        let row = bank.row(i);
        if row.iter().all(|byte| *byte == 0) {
            continue;
        }
        let actor = TxnRef::decode(&row[ACTOR]).map_err(|_| bad())?;
        let status = StatusSlotRef::decode(&row[LOCATION]).map_err(|_| bad())?;
        if row[PHASE] != STABLE
            || row[PHASE + 1..].iter().any(|byte| *byte != 0)
            || !bank
                .source
                .allows_actor(actor, prefix.stream.writer_epoch, bank.adopted_local)
            || actor.generation() != status.generation()
            || status.slot() != 0
        {
            return Err(bad());
        }
        for previous in 0..i {
            let other = bank.row(previous);
            if other[PHASE] == STABLE
                && (TxnRef::decode(&other[ACTOR]).map_err(|_| bad())?.id() == actor.id()
                    || StatusSlotRef::decode(&other[LOCATION])
                        .map_err(|_| bad())?
                        .page()
                        == status.page())
            {
                return Err(bad());
            }
        }
        // Catalog selection skips Row-scope cells; a Row actor's cells are
        // matched only to prove its one TerminalLeased at this Status.
        let mut root = None;
        let mut terminal = None;
        let mut row_terminal = None;
        let mut row_reserved = 0u64;
        let mut row_cells = false;
        for cell in bank.jobs() {
            let candidate = job(cell, profile)?;
            if candidate.source() != Some(actor) {
                continue;
            }
            if candidate.status() != Some(status) {
                return Err(bad());
            }
            let Some(candidate) = JobView::catalog(candidate) else {
                row_cells = true;
                if candidate.state().is_terminal() && row_terminal.replace(candidate).is_some() {
                    return Err(bad());
                }
                row_reserved += u64::from(candidate.state() == job3::JobState::RootReserved);
                continue;
            };
            let selected = if candidate.state().is_terminal() {
                &mut terminal
            } else {
                &mut root
            };
            if selected.replace(candidate).is_some() {
                return Err(bad());
            }
        }
        // One actor never owns Jobs of both scopes.
        let row_scope = match (terminal, row_cells) {
            (Some(selected), false) if selected.state() == JobState::TerminalLeased => false,
            (None, true)
                if root.is_none()
                    && row_terminal.map(|job| job.state())
                        == Some(job3::JobState::TerminalLeased) =>
            {
                true
            }
            _ => return Err(bad()),
        };
        let descriptor = obligation::decode_descriptor(
            &row[..1536],
            ReservationOwner::Transaction(actor),
            crate::storage::retention_obligation::profile_reference(profile),
        )
        .map_err(|_| bad())?;
        let OwnerState::Transaction(transaction) = descriptor.owner else {
            return Err(bad());
        };
        if !matches!(
            transaction.state(),
            TransactionState::Active | TransactionState::Aborting(_)
        ) || descriptor.chain.apply > prefix.complete_lsn
        {
            return Err(bad());
        }
        // A Row descriptor gets the Row ledger's Status-free subset here:
        // birth, reserved-root items, the terminal row and the Row recipe of
        // every debt row. Its Status-dependent rows need the actual Status
        // page; only callers that run `history::validate_live` (its combined
        // closure, `history::row::validate_rows`) prove those.
        if row_scope {
            crate::storage::semantic_row::ledger::validate_admission(
                &descriptor,
                row_terminal.ok_or_else(bad)?,
                row_reserved,
            )
            .map_err(|_| bad())?;
        } else {
            if descriptor.remaining
                != ledger::transaction_remaining(transaction, root).map_err(semantic)?
            {
                return Err(bad());
            }
            let expected =
                if let Some(root) = root.filter(|root| root.state() == JobState::RootReserved) {
                    inventory::items(root, &mut [0; inventory::ITEMS_BYTES]).map_err(semantic)?
                } else {
                    inventory::empty(descriptor.inventory.change_lsn).map_err(semantic)?
                };
            if descriptor.inventory != expected {
                return Err(bad());
            }
        }
        for remaining in descriptor.remaining {
            wal = wal.checked_add(remaining.wal).ok_or_else(bad)?;
            steps = steps.checked_add(remaining.steps).ok_or_else(bad)?;
        }
    }
    // The reverse direction rejects a leased terminal without its original ATT
    // and a finished terminal whose transaction still occupies an ATT slot.
    // Both, and the cleanup debt sum, cover Terminals and roots of either
    // scope through the Row codec.
    for cell in bank.jobs() {
        let selected = job(cell, profile)?;
        if selected.state().is_terminal() {
            let present = bank.slot(selected.source().ok_or_else(bad)?).is_some();
            if present != (selected.state() == job3::JobState::TerminalLeased) {
                return Err(bad());
            }
        }
        let remaining = selected.cleanup();
        wal = wal.checked_add(remaining.wal).ok_or_else(bad)?;
        steps = steps.checked_add(remaining.steps).ok_or_else(bad)?;
    }
    let mut aggregate = [0; 640];
    let (masks, _) = catalog::summarize_jobs(
        JobCells::original(bank.jobs()),
        profile.spec().namespace,
        &mut aggregate,
    )
    .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Retention(e)))?;
    let aging = bank.row(AGING_SLOT);
    if aging.iter().all(|byte| *byte == 0) {
        if masks[0] != u64::MAX || (0..ATT_SLOTS).any(|i| bank.row(i).iter().any(|byte| *byte != 0))
        {
            return Err(bad());
        }
    } else {
        let reference = SystemWorkRef::new(
            profile.spec().namespace,
            SystemWorkKind::Aging,
            1,
            Generation::new(1).expect("Aging generation"),
        )
        .map_err(|_| bad())?;
        let descriptor = obligation::decode_aging_descriptor(
            &aging[..1536],
            ReservationOwner::System(reference),
            crate::storage::retention_obligation::profile_reference(profile),
        )
        .map_err(|_| bad())?;
        if aging[PHASE] != STABLE
            || aging[1536..PHASE].iter().any(|byte| *byte != 0)
            || aging[PHASE + 1..].iter().any(|byte| *byte != 0)
            || descriptor.chain.apply > prefix.complete_lsn
            || retention_page::budget_digest(&descriptor.remaining) != content_digest(&aggregate)
        {
            return Err(bad());
        }
        // Its debt is exactly the Job sum already added above, not another WAL charge.
    }
    Ok((wal, steps))
}
