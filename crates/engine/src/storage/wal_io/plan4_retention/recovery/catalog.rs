//! Recovery selection over the original current ATT/Job bank. Selected scalar
//! identities feed the same live producers; this dispatcher creates no grant.
//! Row cells are selected only under an admitted Row policy (boundaries
//! `#row-recovery-s6`); the legacy binding keeps them Unsupported.
use super::super::super::super::plan4_recovery::RowRecoveryPolicy;
use super::*;
use crate::storage::{
    obligation,
    retention_page::{catalog3 as queue, row as job3},
    semantic_catalog::{Operation, PRIVATE_BYTES},
    semantic_row::Operation as RowOperation,
    transaction_record::{RollbackState, TransactionState},
    undo_ref::UndoRef,
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
    /// A Row RootReserved: C's Row Cancel.
    RowCancel {
        ordinal: usize,
    },
    /// A Row actor that is Active or AbortingStatement: the full
    /// RollbackStart (consumers `#row-states`, recovered AbortingStatement)
    /// over its Status and, when the ATT names a head, that U.
    RowRollback {
        actor: TxnRef,
        status: StatusSlotRef,
        head: Option<UndoRef>,
    },
    /// AbortingFull with a next U: the CLR candidate is A's (RowClr action).
    RowClr {
        actor: TxnRef,
    },
    /// AbortingFull with no next: AbortEnd of the TerminalLeased Job.
    RowAbort {
        actor: TxnRef,
        terminal: usize,
        status: StatusSlotRef,
    },
    /// Committed Prune (head root only), ReleaseRole, RootFree and
    /// TerminalFinish of a Row cell.
    RowRetire {
        ordinal: usize,
        operation: RowOperation,
    },
    Complete,
}

impl<'s, 'c, W: WriterNamespace> Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    /// A Catalog-selected ordinal: a Row cell there is a broken selection.
    /// Same call depth as the whole-bank scan below (no nested wrapper).
    fn recovery_catalog_job(&self, ordinal: usize) -> Result<queue::JobView<'_>> {
        queue::decode_catalog_cell(
            &self.control.ledger.jobs()[ordinal],
            self.inner.storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                self.inner.storage,
                RetentionPhase::Census,
                RetentionCause::Retention(e),
            )
        })?
        .ok_or_else(|| self.recovery_catalog_bad())
    }
    /// Whole-bank Catalog scans skip a valid Row cell (`None`); a corrupt
    /// cell is still a census failure.
    fn recovery_catalog_cell(&self, ordinal: usize) -> Result<Option<queue::JobView<'_>>> {
        queue::decode_catalog_cell(
            &self.control.ledger.jobs()[ordinal],
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
    /// A Row cell under the legacy binding: its analysis entry already
    /// refused every Row state, so this is never reached by READY.
    fn recovery_row_unsupported(&self) -> RetentionError {
        fatal(
            self.inner.storage,
            RetentionPhase::Census,
            RetentionCause::Unsupported,
        )
    }
    /// A whole-bank cell that is not Catalog's, under the Row codec.
    /// Under the legacy binding every Row cell stays Unsupported before any
    /// Row decode, the refusal cause the pre-C4 scan returned.
    pub(super) fn recovery_row_job(&self, ordinal: usize) -> Result<job3::JobView<'_>> {
        if self
            .recovery_binding
            .as_ref()
            .is_some_and(|binding| binding.row_policy().is_none())
        {
            return Err(self.recovery_row_unsupported());
        }
        let job = job3::decode_job(
            &self.control.ledger.jobs()[ordinal],
            self.inner.storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                self.inner.storage,
                RetentionPhase::Census,
                RetentionCause::Retention(e),
            )
        })?;
        if job.state() != job3::JobState::Free && job.scope() != Some(job3::JobScope::Row) {
            return Err(self.recovery_catalog_bad());
        }
        Ok(job)
    }
    /// The admitted Row policy of this recovered binding. Legacy has none and
    /// keeps Row work Unsupported. The admission's digests were compared
    /// when the binding's RestoredRetention was built.
    pub(super) fn recovery_rows(&self) -> Result<RowRecoveryPolicy> {
        self.recovery_binding
            .as_ref()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?
            .row_policy()
            .ok_or_else(|| self.recovery_row_unsupported())
    }
    /// A Row loser step needs WithRowClr: RejectInitialLosers admitted no
    /// loser, and recovery creates none.
    fn recovery_row_losers(&self) -> Result<()> {
        match self.recovery_rows()? {
            RowRecoveryPolicy::WithRowClr => Ok(()),
            RowRecoveryPolicy::RejectInitialLosers => Err(self.recovery_catalog_bad()),
        }
    }
    /// A Row RootRetained is the Prune candidate only when its U is the
    /// actor's current head: the actor's RootRetained root of the largest Undo
    /// sequence (consumers `#row-states`: head-order Prune). The producer
    /// checks the Status head again.
    fn recovery_row_head(&self, ordinal: usize) -> Result<bool> {
        let job = self.recovery_row_job(ordinal)?;
        let actor = job.source().ok_or_else(|| self.recovery_catalog_bad())?;
        for other in 0..64 {
            if other == ordinal || self.recovery_catalog_cell(other)?.is_some() {
                continue;
            }
            let peer = self.recovery_row_job(other)?;
            if peer.source() == Some(actor)
                && peer.state() == job3::JobState::RootRetained
                && peer.undo_sequence() >= job.undo_sequence()
            {
                if peer.undo_sequence() == job.undo_sequence() {
                    return Err(self.recovery_catalog_bad());
                }
                return Ok(false);
            }
        }
        Ok(true)
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
            match self.recovery_catalog_cell(ordinal)? {
                Some(job) if job.state() == queue::JobState::RootReserved => {
                    return Ok(Selected::Cancel { ordinal });
                }
                Some(_) => {}
                None => {
                    if self.recovery_row_job(ordinal)?.state() == job3::JobState::RootReserved {
                        self.recovery_row_losers()?;
                        return Ok(Selected::RowCancel { ordinal });
                    }
                }
            }
        }
        for index in 0..control::ATT_SLOTS {
            let row = self.control.ledger.row(index);
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
            let mut row_terminal = None;
            for ordinal in 0..64 {
                let Some(job) = self.recovery_catalog_cell(ordinal)? else {
                    let job = self.recovery_row_job(ordinal)?;
                    if job.source() == Some(actor) && job.state() == job3::JobState::TerminalLeased
                    {
                        if job.status() != Some(status) || row_terminal.is_some() {
                            return Err(self.recovery_catalog_bad());
                        }
                        row_terminal = Some(ordinal);
                    }
                    continue;
                };
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
            if let Some(row_terminal) = row_terminal {
                // One actor has one scope (consumers `#row-states`).
                if terminal.is_some() || root.is_some() {
                    return Err(self.recovery_catalog_bad());
                }
                self.recovery_row_losers()?;
                return match tx.state() {
                    TransactionState::Active
                    | TransactionState::Aborting(RollbackState::Statement { .. }) => {
                        Ok(Selected::RowRollback {
                            actor,
                            status,
                            head: tx.undo_head(),
                        })
                    }
                    TransactionState::Aborting(RollbackState::Transaction { next: Some(_) }) => {
                        Ok(Selected::RowClr { actor })
                    }
                    TransactionState::Aborting(RollbackState::Transaction { next: None }) => {
                        Ok(Selected::RowAbort {
                            actor,
                            terminal: row_terminal,
                            status,
                        })
                    }
                    _ => Err(self.recovery_catalog_bad()),
                };
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
        let mut behind_head = false;
        for ordinal in 0..64 {
            let Some(job) = self.recovery_catalog_cell(ordinal)? else {
                let operation = match self.recovery_row_job(ordinal)?.state() {
                    job3::JobState::Free | job3::JobState::TerminalReady => continue,
                    job3::JobState::RootRetained => {
                        if !self.recovery_row_head(ordinal)? {
                            behind_head = true;
                            continue;
                        }
                        RowOperation::Prune
                    }
                    job3::JobState::RootReleasingCommitted
                    | job3::JobState::RootReleasingCompensated => {
                        if self.recovery_row_job(ordinal)?.release_cursor() < 3 {
                            RowOperation::ReleaseRole
                        } else {
                            RowOperation::RootFree
                        }
                    }
                    _ => return Err(self.recovery_catalog_bad()),
                };
                self.recovery_rows()?;
                return Ok(Selected::RowRetire { ordinal, operation });
            };
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
        // Every actor's head root is eligible, so a Retained root behind a
        // head can be left only by a broken chain.
        if behind_head {
            return Err(self.recovery_catalog_bad());
        }
        for ordinal in 0..64 {
            match self.recovery_catalog_cell(ordinal)? {
                Some(job) if job.state() == queue::JobState::TerminalReady => {
                    return Ok(Selected::Retire {
                        ordinal,
                        operation: Operation::TerminalFinish,
                    });
                }
                Some(_) => {}
                None => {
                    if self.recovery_row_job(ordinal)?.state() == job3::JobState::TerminalReady {
                        self.recovery_rows()?;
                        return Ok(Selected::RowRetire {
                            ordinal,
                            operation: RowOperation::TerminalFinish,
                        });
                    }
                }
            }
        }
        // No Row cell is left behind a Complete (no READY with Row state).
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
        let mut pages = [None; 9];
        pages[0] = Some(0);
        pages[1] = Some(1);
        let mut count = 2;
        let (ordinal, root_ordinal, operation) = match selected {
            Selected::Cancel { ordinal } => (ordinal, Some(ordinal), Operation::Cancel),
            Selected::Rollback { terminal, root, .. } => (terminal, root, Operation::RollbackStart),
            Selected::Abort { terminal, .. } => (terminal, None, Operation::AbortEnd),
            Selected::Retire { ordinal, operation } => (ordinal, Some(ordinal), operation),
            // A's CLR handler makes its own sources resident.
            Selected::Complete | Selected::RowClr { .. } => return Ok(None),
            Selected::RowCancel { .. }
            | Selected::RowRollback { .. }
            | Selected::RowAbort { .. }
            | Selected::RowRetire { .. } => {
                self.recovery_row_pages(selected, &mut pages, &mut count)?;
                return self.recovery_resident(&pages, count);
            }
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
        let mut push =
            |index: usize| -> Result<()> { self.recovery_push_page(&mut pages, &mut count, index) };
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
        self.recovery_resident(&pages, count)
    }

    fn recovery_push_page(
        &self,
        pages: &mut [Option<usize>; 9],
        count: &mut usize,
        index: usize,
    ) -> Result<()> {
        if pages[..*count].contains(&Some(index)) {
            return Ok(());
        }
        if index >= 64 || *count == pages.len() {
            return Err(self.recovery_catalog_bad());
        }
        pages[*count] = Some(index);
        *count += 1;
        Ok(())
    }

    /// The recovered Row group's whole target set, by the same page rules
    /// as `prepare_row_recovery` and `row_rollback_control` (boundaries
    /// `#row-recovery-s6`): Allocation and Root, then the Job page, Status,
    /// U, the table pair, the CatalogLeaf and H, as the operation takes them.
    fn recovery_row_pages(
        &self,
        selected: Selected,
        pages: &mut [Option<usize>; 9],
        count: &mut usize,
    ) -> Result<()> {
        let bad = || self.recovery_catalog_bad();
        let namespace = self.inner.storage.profile().spec().namespace;
        let mut push = |index: usize| self.recovery_push_page(pages, count, index);
        match selected {
            Selected::RowRollback { status, head, .. } => {
                push(status.page().page_no as usize)?;
                if let Some(head) = head {
                    push(head.page().page_no as usize)?;
                }
            }
            Selected::RowAbort {
                terminal, status, ..
            } => {
                push(2 + terminal / 15)?;
                push(status.page().page_no as usize)?;
            }
            Selected::RowCancel { ordinal } | Selected::RowRetire { ordinal, .. } => {
                let operation = match selected {
                    Selected::RowRetire { operation, .. } => operation,
                    _ => RowOperation::Cancel,
                };
                let job = self.recovery_row_job(ordinal)?;
                push(2 + ordinal / 15)?;
                if matches!(
                    operation,
                    RowOperation::Cancel | RowOperation::Prune | RowOperation::TerminalFinish
                ) {
                    push(job.status().ok_or_else(bad)?.page().page_no as usize)?;
                }
                if operation != RowOperation::TerminalFinish {
                    push(job.undo().ok_or_else(bad)?.page().page_no as usize)?;
                }
                if operation == RowOperation::Prune {
                    let pair = job.pair().ok_or_else(bad)?;
                    push(pair[1].page_no as usize)?;
                    push(pair[0].page_no as usize)?;
                    push(8)?;
                    if let Some(h) = super::super::create_pair::catalog_live::lifecycle::row::recovery_predecessor(
                        self.control.ledger.jobs(),
                        namespace,
                        ordinal,
                    )
                    .map_err(|()| bad())?
                    {
                        let peer = self.recovery_row_job(h)?;
                        push(peer.undo().ok_or_else(bad)?.page().page_no as usize)?;
                    }
                }
            }
            _ => return Err(bad()),
        }
        Ok(())
    }

    /// The original residency machinery over the computed page set.
    fn recovery_resident(
        &mut self,
        pages: &[Option<usize>; 9],
        count: usize,
    ) -> Result<Option<RetentionRecoveryStep>> {
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
        if self.inner.storage.profile().format() != 3 {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        if self.control.ledger.population.reserved() {
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
            Selected::RowCancel { ordinal } | Selected::RowRetire { ordinal, .. } => {
                let operation = match selected {
                    Selected::RowRetire { operation, .. } => operation,
                    _ => RowOperation::Cancel,
                };
                // Every Row step checks its admission against the restored
                // digest again, not only the first.
                self.recovery_rows()?;
                let selected_job = self.recovery_row_job(ordinal)?;
                let job = self.cleanup_job(ordinal, selected_job.generation())?;
                let observation = self.row_recovery_cleanup(owner, ordinal, operation, private)?;
                if observation.operation != operation || observation.job_ordinal != Some(ordinal) {
                    return Err(self.recovery_catalog_bad());
                }
                // A Row RootReserved is the base's `RowUndoSlot` inventory:
                // its Cancel is the CancelReservedFirst step, as Catalog's.
                Ok(if operation == RowOperation::Cancel {
                    RetentionRecoveryStep::CancelledReserved
                } else {
                    RetentionRecoveryStep::RowAdvanced { job, operation }
                })
            }
            Selected::RowRollback { actor, .. } | Selected::RowAbort { actor, .. } => {
                let operation = if matches!(selected, Selected::RowRollback { .. }) {
                    RowOperation::RollbackStart
                } else {
                    RowOperation::AbortEnd
                };
                self.recovery_row_losers()?;
                let observation = self.row_recovery_terminal(owner, actor, operation, private)?;
                if observation.operation != operation || observation.actor != Some(actor) {
                    return Err(self.recovery_catalog_bad());
                }
                Ok(if operation == RowOperation::RollbackStart {
                    RetentionRecoveryStep::RollbackStarted { actor }
                } else {
                    RetentionRecoveryStep::Aborted { actor }
                })
            }
            Selected::RowClr { actor } => {
                self.recovery_row_losers()?;
                Ok(RetentionRecoveryStep::RowClrRequired { actor })
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
