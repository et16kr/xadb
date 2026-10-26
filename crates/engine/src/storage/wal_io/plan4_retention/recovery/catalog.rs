//! Recovery selection over the original current ATT/Job bank. Selected scalar
//! identities feed the same live producers; this dispatcher creates no grant.
use super::*;
use crate::storage::{
    obligation,
    retention_page::catalog as queue,
    semantic_catalog::{Operation, PRIVATE_BYTES},
    transaction_record::{RollbackState, TransactionState},
};

#[derive(Clone, Copy)]
pub(super) enum Selected {
    Cancel {
        ordinal: usize,
    },
    Rollback {
        actor: TxnRef,
        terminal: usize,
        root: Option<usize>,
    },
    Abort {
        actor: TxnRef,
        terminal: usize,
    },
    Retire {
        ordinal: usize,
        operation: Operation,
    },
    Complete,
}

impl<'s, 'c, W: WriterNamespace> Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    fn recovery_catalog_job(&self, ordinal: usize) -> Result<queue::JobView<'_>> {
        queue::decode_job(
            &self.control.jobs()[ordinal],
            self.inner.storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                self.inner.storage,
                RetentionPhase::Census,
                RetentionCause::Retention(e),
            )
        })
    }
    fn recovery_catalog_bad(&self) -> RetentionError {
        fatal(
            self.inner.storage,
            RetentionPhase::Census,
            RetentionCause::Invariant,
        )
    }
    /// Current graph validation has already established both directions. The
    /// selector still rejects ambiguity and never interprets old v1 Job bytes.
    pub(super) fn select_catalog_cleanup(&self) -> Result<Selected> {
        for ordinal in 0..64 {
            if self.recovery_catalog_job(ordinal)?.state() == queue::JobState::RootReserved {
                return Ok(Selected::Cancel { ordinal });
            }
        }
        for index in 0..control::ATT_SLOTS {
            let row = self.control.row(index);
            if row.iter().all(|byte| *byte == 0) {
                continue;
            }
            let actor = TxnRef::decode(&row[control::ACTOR]).map_err(|e| {
                fatal(
                    self.inner.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let status = StatusSlotRef::decode(&row[control::LOCATION]).map_err(|e| {
                fatal(
                    self.inner.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let descriptor = obligation::decode_descriptor(
                &row[..1536],
                allocation_page::ReservationOwner::Transaction(actor),
                crate::storage::retention_obligation::profile_reference(
                    self.inner.storage.profile(),
                ),
            )
            .map_err(|e| {
                fatal(
                    self.inner.storage,
                    RetentionPhase::Census,
                    RetentionCause::Base(e.into()),
                )
            })?;
            let OwnerState::Transaction(tx) = descriptor.owner else {
                return Err(self.recovery_catalog_bad());
            };
            let mut terminal = None;
            let mut root = None;
            for ordinal in 0..64 {
                let job = self.recovery_catalog_job(ordinal)?;
                if job.source() != Some(actor) {
                    continue;
                }
                if job.status() != Some(status) {
                    return Err(self.recovery_catalog_bad());
                }
                let destination = if job.state().is_terminal() {
                    if job.state() != queue::JobState::TerminalLeased {
                        return Err(self.recovery_catalog_bad());
                    }
                    &mut terminal
                } else {
                    &mut root
                };
                if destination.replace(ordinal).is_some() {
                    return Err(self.recovery_catalog_bad());
                }
            }
            let terminal = terminal.ok_or_else(|| self.recovery_catalog_bad())?;
            return match tx.state() {
                TransactionState::Active => Ok(Selected::Rollback {
                    actor,
                    terminal,
                    root,
                }),
                TransactionState::Aborting(RollbackState::Transaction { next: None }) => {
                    Ok(Selected::Abort { actor, terminal })
                }
                TransactionState::Aborting(RollbackState::Transaction { next: Some(next) }) => {
                    let ordinal = root.ok_or_else(|| self.recovery_catalog_bad())?;
                    let job = self.recovery_catalog_job(ordinal)?;
                    if job.state() != queue::JobState::RootRetained || job.undo() != Some(next) {
                        return Err(self.recovery_catalog_bad());
                    }
                    Ok(Selected::Retire {
                        ordinal,
                        operation: Operation::Clr,
                    })
                }
                _ => Err(self.recovery_catalog_bad()),
            };
        }
        // A root must relinquish its incoming references and all three roles
        // before its actor's terminal slot can be freed.
        for ordinal in 0..64 {
            let job = self.recovery_catalog_job(ordinal)?;
            let operation = match job.state() {
                queue::JobState::Free | queue::JobState::TerminalReady => continue,
                queue::JobState::RootRetained => Operation::Prune,
                queue::JobState::RootReleasingCommitted
                | queue::JobState::RootReleasingCompensated => {
                    if job.release_cursor() < 3 {
                        Operation::ReleaseRole
                    } else {
                        Operation::RootFree
                    }
                }
                _ => return Err(self.recovery_catalog_bad()),
            };
            return Ok(Selected::Retire { ordinal, operation });
        }
        for ordinal in 0..64 {
            if self.recovery_catalog_job(ordinal)?.state() == queue::JobState::TerminalReady {
                return Ok(Selected::Retire {
                    ordinal,
                    operation: Operation::TerminalFinish,
                });
            }
        }
        Ok(Selected::Complete)
    }

    /// Prepare the whole exact live operation target set through the original
    /// residency machinery. No pin spans a load/flush; the producer revalidates
    /// and pins the whole set again before its semantic change.
    fn prepare_catalog_cleanup(
        &mut self,
        owner: &Owner<'c, 's>,
        selected: Selected,
    ) -> Result<Option<RetentionRecoveryStep>> {
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let (ordinal, root_ordinal, operation) = match selected {
            Selected::Cancel { ordinal } => (ordinal, Some(ordinal), Operation::Cancel),
            Selected::Rollback { terminal, root, .. } => (terminal, root, Operation::RollbackStart),
            Selected::Abort { terminal, .. } => (terminal, None, Operation::AbortEnd),
            Selected::Retire { ordinal, operation } => (ordinal, Some(ordinal), operation),
            Selected::Complete => return Ok(None),
        };
        // Only compact references survive the Job borrow.
        let (status, undo, pair) = {
            let job = self.recovery_catalog_job(ordinal)?;
            let root = root_ordinal
                .map(|index| self.recovery_catalog_job(index))
                .transpose()?;
            (
                job.status(),
                root.and_then(|job| job.undo()),
                root.and_then(|job| job.pair()),
            )
        };
        let mut pages = [None; 9];
        pages[0] = Some(0);
        pages[1] = Some(1);
        let mut count = 2;
        let mut push = |index: usize| -> Result<()> {
            if pages[..count].contains(&Some(index)) {
                return Ok(());
            }
            if index >= 64 || count == pages.len() {
                return Err(self.recovery_catalog_bad());
            }
            pages[count] = Some(index);
            count += 1;
            Ok(())
        };
        if operation != Operation::RollbackStart {
            push(2 + ordinal / 15)?;
        }
        if matches!(
            operation,
            Operation::Cancel
                | Operation::RollbackStart
                | Operation::AbortEnd
                | Operation::Clr
                | Operation::Prune
                | Operation::TerminalFinish
        ) {
            push(
                status
                    .ok_or_else(|| self.recovery_catalog_bad())?
                    .page()
                    .page_no as usize,
            )?;
        }
        if matches!(
            operation,
            Operation::Cancel
                | Operation::Clr
                | Operation::Prune
                | Operation::ReleaseRole
                | Operation::RootFree
        ) || operation == Operation::RollbackStart && undo.is_some()
        {
            push(
                undo.ok_or_else(|| self.recovery_catalog_bad())?
                    .page()
                    .page_no as usize,
            )?;
        }
        if matches!(
            operation,
            Operation::Cancel | Operation::Clr | Operation::Prune
        ) {
            push(7)?;
            push(8)?;
        }
        if operation == Operation::Clr {
            for page in pair.ok_or_else(|| self.recovery_catalog_bad())? {
                push(page.page_no as usize)?;
            }
        }
        let first = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let mut requests = [first; 9];
        for index in 0..count {
            let page = pages[index].expect("bounded target");
            requests[index] = request_from_image(
                self.inner.storage.profile(),
                history::arena_page(self.history.exclusive()?, page),
            )
            .map_err(|e| fatal(self.inner.storage, RetentionPhase::Source, e))?;
        }
        let mut cursor = Plan4ResidencyCursor::new();
        // The authenticated allocator is permanently resident in slot zero.
        // It is protected in `required` but is not a replaceable payload load.
        for index in 1..count {
            if let RetentionResidencyStep::FlushRequired { target, rec_lsn } = self
                .prepare_required(
                    &Plan4Target {
                        request: requests[index],
                    },
                    &mut cursor,
                    &requests[..count],
                )?
            {
                return Ok(Some(RetentionRecoveryStep::FlushRequired {
                    target,
                    rec_lsn,
                }));
            }
        }
        Ok(None)
    }

    pub(in crate::storage) fn recovery_catalog_step(
        &mut self,
        owner: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<RetentionRecoveryStep> {
        self.recovery_enter(owner)?;
        if self.inner.storage.profile().format() != 2 {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        if self.control.population.reserved() {
            if !self
                .recovery_binding
                .as_ref()
                .expect("checked binding")
                .cancel_reserved_first()
            {
                return Err(self.recovery_catalog_bad());
            }
            self.allocate(owner, true)?;
            return Ok(RetentionRecoveryStep::CancelledReserved);
        }
        self.refresh_recovered()?;
        let selected = self.select_catalog_cleanup()?;
        if let Some(step) = self.prepare_catalog_cleanup(owner, selected)? {
            return Ok(step);
        }
        match selected {
            Selected::Cancel { ordinal } => {
                self.catalog_cancel_reserved(owner, ordinal, private)?;
                Ok(RetentionRecoveryStep::CancelledReserved)
            }
            Selected::Rollback { actor, .. } => {
                self.catalog_rollback_start(owner, actor, private)?;
                Ok(RetentionRecoveryStep::RollbackStarted { actor })
            }
            Selected::Abort { actor, .. } => {
                self.catalog_abort_end(owner, actor, private)?;
                Ok(RetentionRecoveryStep::Aborted { actor })
            }
            Selected::Retire { ordinal, operation } => {
                let selected_job = self.recovery_catalog_job(ordinal)?;
                let job = self.cleanup_job(ordinal, selected_job.generation())?;
                if operation == Operation::ReleaseRole {
                    self.catalog_release_role(owner, ordinal, private)?;
                } else {
                    self.catalog_retire(owner, ordinal, operation, private)?;
                }
                Ok(RetentionRecoveryStep::CatalogAdvanced { job, operation })
            }
            Selected::Complete => {
                for index in 0..64 {
                    if let Some(target) = self.recovery_flush_target_unchecked(index)?
                        && let Some(rec_lsn) = self.recovery_resident_rec_lsn(owner, target)?
                    {
                        return Ok(RetentionRecoveryStep::FlushRequired { target, rec_lsn });
                    }
                }
                self.recovery_verify(owner)?;
                Ok(RetentionRecoveryStep::Verified)
            }
        }
    }
}

pub(super) fn selection_source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<Selected>()
        + 2 * size_of::<Result<Selected>>()
        + 4 * size_of::<queue::JobView<'_>>()
        + 4 * size_of::<Result<queue::JobView<'_>>>()
        + size_of::<obligation::DescriptorSpec>()
        + size_of::<std::result::Result<obligation::DescriptorSpec, obligation::ObligationError>>()
        + size_of::<[TargetRequest; 9]>()
        + size_of::<[Option<usize>; 9]>()
        + size_of::<Plan4ResidencyCursor>()
        + 2 * size_of::<Result<RetentionRecoveryStep>>()
        + queue::source_bytes()
}
