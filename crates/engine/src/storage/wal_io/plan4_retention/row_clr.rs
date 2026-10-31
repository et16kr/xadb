//! CLR effect steps share the Forward observer, digest chain and whole install.
//! Authentication comes from the original rollback's actor and retained head,
//! rather than an INSERT reservation or a newly created statement.
use super::*;
use crate::storage::transaction_record::TransactionState;

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in super::super::super::super::super::super::super) fn row_rollback_predecessor(
        &self,
        actor: TxnRef,
        sequence: u64,
    ) -> Result<Option<UndoLink>> {
        self.row_predecessor(actor, sequence)
    }
    fn clr_entry(
        &self,
        owner: &mut Owner<'c, 's>,
        table: RowTableSource,
        draft: &RowDraftCut,
        issued: RowIssued,
    ) -> Result<ReservedEntry> {
        let bad = || {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let (identity, prefix) = self.catalog_preparation().existing_entry(owner)?;
        if draft.operation != 1 {
            return Err(bad());
        }
        let tx = self.row_rollback_state(issued.actor)?;
        if !matches!(tx.state(), TransactionState::Aborting(_))
            || tx.undo_head() != Some(issued.undo)
        {
            return Err(bad());
        }
        let (_, status) = self.catalog_preparation().row_terminal(issued.actor)?;
        let slot = self.control.slot(issued.actor).ok_or_else(bad)?;
        self.check_table_slot(table)?;
        if prefix.complete_lsn != draft.complete_lsn {
            return Err(stale());
        }
        let ordinal = issued.root_job.ordinal().map_err(|_| bad())?;
        let job = job3::decode_job(
            &self.control.jobs()[ordinal],
            self.storage.profile().spec().namespace,
        )
        .map_err(|_| bad())?;
        if job.state() != job3::JobState::RootRetained
            || job.source() != Some(issued.actor)
            || job.undo() != Some(issued.undo)
            || job.row_id() != Some(issued.row)
            || job.mutation() != draft.planned_mutation
        {
            return Err(bad());
        }
        Ok(ReservedEntry {
            owner: identity,
            prefix,
            issued: DraftIssued {
                actor: issued.actor,
                root_job: issued.root_job,
                row: issued.row,
                undo: issued.undo,
                sequence: job.undo_sequence(),
                mutation: job.mutation(),
            },
            ordinal,
            status,
            slot,
            statement: job.statement(),
        })
    }
    pub(in super::super::super::super::super::super::super) fn row_prepare_clr(
        &self,
        owner: &mut Owner<'c, 's>,
        table: RowTableSource,
        draft: &RowDraftCut,
        issued: RowIssued,
        proposal: &RowProposal,
        private: &mut [u8; 425_984],
    ) -> Result<RowPreparedCut> {
        let entry = self.clr_entry(owner, table, draft, issued)?;
        self.catalog_preparation().prepare_forward(
            owner,
            &entry,
            (table, draft, proposal),
            ForwardLanes::split(private),
        )
    }
    pub(in super::super::super::super::super::super::super) fn row_apply_clr(
        &mut self,
        owner: &mut Owner<'c, 's>,
        table: RowTableSource,
        draft: &RowDraftCut,
        issued: RowIssued,
        cut: &RowPreparedCut,
        private: &mut [u8; 425_984],
    ) -> Result<()> {
        let entry = self.clr_entry(owner, table, draft, issued)?;
        let ForwardLanes {
            frame,
            before,
            after,
            inventory,
            ..
        } = ForwardLanes::split(private);
        let group = self.catalog_preparation().prepare_forward_apply(
            owner,
            &entry,
            (draft, cut),
            before,
            (&*frame, &*after, &*inventory),
        )?;
        #[cfg(test)]
        test_stage_refusal()?;
        self.catalog_application()
            .apply_existing(owner, ExistingGroup::RowWrite(&group))
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn rollback_state(&self, actor: TxnRef) -> Result<TransactionState> {
        let slot = self
            .control
            .slot(actor)
            .ok_or_else(|| own_output(self.storage))?;
        let descriptor = crate::storage::obligation::decode_descriptor(
            self.control.row(slot)[..1536].try_into().expect("ATT"),
            crate::storage::allocation_page::ReservationOwner::Transaction(actor),
            crate::storage::retention_obligation::profile_reference(self.storage.profile()),
        )
        .map_err(|_| own_output(self.storage))?;
        let crate::storage::obligation::OwnerState::Transaction(tx) = descriptor.owner else {
            return Err(own_output(self.storage));
        };
        Ok(tx.state())
    }
}
impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in super::super::super::super::super::super::super) fn row_rollback_chain_lsn(
        &self,
        actor: TxnRef,
    ) -> Result<u64> {
        let slot = self
            .control
            .slot(actor)
            .ok_or_else(|| own_output(self.storage))?;
        let descriptor = crate::storage::obligation::decode_descriptor(
            self.control.row(slot)[..1536].try_into().expect("ATT"),
            crate::storage::allocation_page::ReservationOwner::Transaction(actor),
            crate::storage::retention_obligation::profile_reference(self.storage.profile()),
        )
        .map_err(|_| own_output(self.storage))?;
        Ok(descriptor.chain.apply)
    }
}
