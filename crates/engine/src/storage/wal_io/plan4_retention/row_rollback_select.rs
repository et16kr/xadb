//! Select a retained Row head from the original ATT/Jobs and prove its sources
//! against canonical replay. No Free Undo or new reservation is introduced.
use super::super::super::super::row::RowDraftCut;
use super::super::lifecycle::row::write::forward::RowIssued;
use super::super::lifecycle::row::write::{DraftIssued, DraftSource};
use super::*;
use crate::storage::retention_page::JobSlotRef;
use crate::storage::{
    allocation_page::ReservationOwner,
    obligation::{self, OwnerState},
    transaction_record::TransactionStatus,
    undo_record::UndoLink,
};

pub(in super::super::super::super) struct RollbackCandidate {
    pub(in super::super::super::super) draft: RowDraftCut,
    pub(in super::super::super::super) table: RowTableSource,
    pub(in super::super::super::super) issued: RowIssued,
    pub(in super::super::super::super) next: Option<UndoLink>,
}
pub(in super::super::super::super) enum RollbackSelection {
    Candidate(RollbackCandidate),
    Flush(Plan4Target),
    Complete,
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in super::super::super::super) fn row_rollback_state(
        &self,
        actor: TxnRef,
    ) -> Result<TransactionStatus> {
        let bad = || {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let descriptor = obligation::decode_descriptor(
            self.control.row(slot)[..1536].try_into().expect("ATT"),
            ReservationOwner::Transaction(actor),
            crate::storage::retention_obligation::profile_reference(self.storage.profile()),
        )
        .map_err(|_| bad())?;
        let OwnerState::Transaction(tx) = descriptor.owner else {
            return Err(bad());
        };
        Ok(tx)
    }

    pub(in super::super::super::super) fn row_rollback_select(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        mark: Option<UndoRef>,
        private: &mut [u8; 425_984],
    ) -> Result<RollbackSelection> {
        let storage = self.storage;
        let bad = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let (_, prefix) = self.catalog_preparation().existing_entry(owner)?;
        let tx = self.row_rollback_state(actor)?;
        let Some(head) = tx.undo_head().filter(|head| Some(*head) != mark) else {
            return Ok(RollbackSelection::Complete);
        };
        let namespace = storage.profile().spec().namespace;
        let mut selected = None;
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let job = job3::decode_job(bytes, namespace).map_err(|_| bad())?;
            if job.source() == Some(actor)
                && job.undo() == Some(head)
                && job.state() == job3::JobState::RootRetained
            {
                if selected.replace(ordinal).is_some() {
                    return Err(bad());
                }
            }
        }
        let ordinal = selected.ok_or_else(bad)?;
        let job = job3::decode_job(&self.control.jobs()[ordinal], namespace).map_err(|_| bad())?;
        let handle = job.object().ok_or_else(bad)?;
        let pair = job.pair().ok_or_else(bad)?;
        let slot = match self
            .visibility
            .ok_or_else(bad)?
            .table_slot(handle)
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })? {
            TableSlot::Published(slot) => slot,
            _ => return Err(bad()),
        };
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let table = RowTableSource {
            handle,
            slot,
            anchor: PageIdentity {
                database: allocator.identity.database,
                page: pair[0],
                owner: handle.object,
                owner_generation: handle.generation,
            },
        };
        self.check_table_slot(table)?;
        let (terminal, status) = self.catalog_preparation().row_terminal(actor)?;
        let mut sources = RowSources::new(
            target_from_source(storage, Plan4Page::CatalogLeaf)?.request,
            allocator,
        );
        self.queue_root_anchor(&mut sources, table)?;
        sources.queue(storage, self.jobs_request(ordinal)?, RowSourceRole::RootJob)?;
        sources.queue(
            storage,
            self.jobs_request(terminal)?,
            RowSourceRole::TerminalJob,
        )?;
        sources.queue(
            storage,
            self.catalog_preparation().catalog_status_request(status),
            RowSourceRole::Status,
        )?;
        sources.queue(
            storage,
            request(
                PageIdentity {
                    database: allocator.identity.database,
                    page: pair[1],
                    owner: handle.object,
                    owner_generation: handle.generation,
                },
                PageKind::RowLeaf,
            ),
            RowSourceRole::RowLeaf,
        )?;
        sources.queue(
            storage,
            undo_request(allocator.identity, head.page()),
            RowSourceRole::Undo,
        )?;
        let next = self.row_rollback_predecessor(actor, job.undo_sequence())?;
        if let Some(link) = next {
            sources.queue(
                storage,
                undo_request(allocator.identity, link.reference.page()),
                RowSourceRole::PreviousUndo,
            )?;
        }
        let mut cursor = Plan4ResidencyCursor::new();
        let mut lanes = PreflightLanes::split(private);
        if let Some(target) = self.make_resident(owner, &mut sources, &mut cursor, lanes.scratch)? {
            return Ok(RollbackSelection::Flush(target));
        }
        let targets = RowTargets::new(&sources);
        let root_job = JobSlotRef::new(
            self.jobs_request(ordinal)?.identity.page,
            (ordinal % 15) as u16,
            job.generation(),
        )
        .map_err(|_| bad())?;
        let issued = RowIssued {
            actor,
            root_job,
            row: job.row_id().ok_or_else(bad)?,
            undo: head,
            reserve_lsn: self.row_rollback_chain_lsn(actor)?,
        };
        let draft_issued = DraftIssued {
            actor,
            root_job,
            row: issued.row,
            undo: head,
            sequence: job.undo_sequence(),
            mutation: job.mutation(),
        };
        let mut audit = self.history.replay_scope(owner)?;
        let mut replay = audit.checkout()?;
        let result = (|| {
            history::validate_live(storage, self.control, replay.bytes_mut(), prefix)?;
            match targets.count {
                7 => self.rollback_read::<8>(
                    owner,
                    prefix,
                    &targets,
                    sources.catalog(),
                    table,
                    draft_issued,
                    replay.bytes(),
                    &mut lanes,
                ),
                8 => self.rollback_read::<9>(
                    owner,
                    prefix,
                    &targets,
                    sources.catalog(),
                    table,
                    draft_issued,
                    replay.bytes(),
                    &mut lanes,
                ),
                9 => self.rollback_read::<10>(
                    owner,
                    prefix,
                    &targets,
                    sources.catalog(),
                    table,
                    draft_issued,
                    replay.bytes(),
                    &mut lanes,
                ),
                _ => Err(bad()),
            }
        })();
        let source = merge(
            merge(result, replay.return_checked()),
            audit.finish_checked(),
        )?;
        Ok(RollbackSelection::Candidate(RollbackCandidate {
            draft: RowDraftCut {
                source,
                input: *job.projection_digest(),
                complete_lsn: prefix.complete_lsn,
                planned_mutation: job.mutation(),
                roles: targets.roles,
                input_len: u16::try_from(job.cell_bytes()).map_err(|_| bad())?,
                operation: 1,
            },
            table,
            issued,
            next,
        }))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "one typed read under the original canonical replay checkout"
    )]
    fn rollback_read<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        targets: &RowTargets,
        catalog: TargetRequest,
        table: RowTableSource,
        issued: DraftIssued,
        arena: &[u8],
        lanes: &mut PreflightLanes<'_>,
    ) -> Result<[u8; 32]> {
        let bad = || {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let mut requests = [catalog; N];
        requests[1..].copy_from_slice(targets.before());
        requests.sort_unstable_by_key(|r| target_rank(*r));
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &requests)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let result = (|| {
            let mut stamps = [None; 9];
            let mut at_before = 0;
            for (at, request) in requests.iter().enumerate() {
                if *request == catalog {
                    read.copy_snapshot(at, lanes.schema)
                        .map_err(|e| frame(RetentionPhase::Prepare, e))?;
                    continue;
                }
                if targets.before().get(at_before) != Some(request) {
                    return Err(bad());
                }
                stamps[at_before] = Some(
                    read.copy_snapshot(at, &mut lanes.before[at_before])
                        .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                );
                if &lanes.before[at_before]
                    != history::arena_page(arena, request.identity.page.page_no as usize)
                {
                    return Err(bad());
                }
                at_before += 1;
            }
            let stamps: [FrameStamp; 9] = std::array::from_fn(|at| {
                stamps[at].unwrap_or_else(|| stamps[0].expect("nonempty T"))
            });
            let allocator = targets.at(RowSourceRole::Allocator).ok_or_else(bad)?;
            self.check_membership(
                &lanes.before[allocator],
                targets.before()[allocator].identity,
                &requests,
            )?;
            self.check_table_record(lanes.schema, catalog.identity, table)?;
            let slot = self.control.slot(issued.actor).ok_or_else(bad)?;
            let aging = self.control.row(control::AGING_SLOT);
            DraftSource {
                prefix,
                targets: targets.before(),
                stamps: &stamps[..targets.count],
                roles: targets.roles,
                issued,
                transaction: self.control.row(slot)[..1536].try_into().expect("ATT"),
                aging: (aging[control::PHASE] != 0)
                    .then(|| aging[..1536].try_into().expect("aging")),
                jobs: self.control.jobs(),
                pages: lanes.before[..targets.count].iter(),
                control: ControlCut::capture(self.control),
            }
            .commitment()
            .map_err(|_| bad())
        })();
        merge(
            result,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Residency for a lifecycle control group, including the retained head's
    /// proof for RollbackStart. No gate or source reservation is acquired here.
    pub(in super::super::super::super) fn row_rollback_warm(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; 425_984],
    ) -> Result<Option<Plan4Target>> {
        let storage = self.storage;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let (terminal, status) = self.catalog_preparation().row_terminal(actor)?;
        let mut sources = RowSources::new(
            target_from_source(storage, Plan4Page::CatalogLeaf)?.request,
            allocator,
        );
        sources.queue(
            storage,
            target_from_source(storage, Plan4Page::Root)?.request,
            RowSourceRole::Root,
        )?;
        sources.queue(
            storage,
            self.jobs_request(terminal)?,
            RowSourceRole::TerminalJob,
        )?;
        sources.queue(
            storage,
            self.catalog_preparation().catalog_status_request(status),
            RowSourceRole::Status,
        )?;
        if let Some(head) = self.row_rollback_state(actor)?.undo_head() {
            sources.queue(
                storage,
                undo_request(allocator.identity, head.page()),
                RowSourceRole::Undo,
            )?;
        }
        self.make_resident(
            owner,
            &mut sources,
            &mut Plan4ResidencyCursor::new(),
            &mut private[RowLoanLayout::FRAME],
        )
    }
    pub(in super::super::super::super) fn row_rollback_status_target(
        &self,
        actor: TxnRef,
    ) -> Result<TargetRequest> {
        let (_, status) = self.catalog_preparation().row_terminal(actor)?;
        Ok(self.catalog_preparation().catalog_status_request(status))
    }
}
