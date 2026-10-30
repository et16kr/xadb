//! The Forward path of a Row write (S2-0 Row write amendment §1 "order" and
//! §6, S2-0b amendment `#row-write-validate-once`): the context the producer
//! lane carries, the Forward `prepare` and `apply`. The caller holds the
//! terminal read gate, the RowWrite admission, the flush lease of the row and
//! the one RowSlotReservation; no step here opens a replay scope. `prepare`
//! and `apply` inherit preflight's one `history::validate_live` through the
//! XRWC digest chain: each recomputes the draft commitment (kind 2) over a
//! typed read of T into the before range and refuses a difference as
//! Before(Stale) with no effect. The producer lane checks no chain.
//!
//! `prepare`, in order:
//! 1. the deep checks of an effect step, as the Cancel's (entry, view, the
//!    reservation's actor, Terminal, ATT slot, table slot, the RootJob's Undo
//!    sequence), with no statement-head check;
//! 2. A's declared metadata (operation, frame length, after count, roles);
//! 3. the draft commitment recompute (`draft_recompute`, the kind-2
//!    recompute a Cancel folds the same way);
//! 4. C's observer over the before images that recompute proved equal to the
//!    pool, comparing A's whole declared frame byte for byte, then A's after
//!    pages against the observer's (a difference is Before);
//! 5. the changed-target identity, the XOI successor and the progress room;
//! 6. the prepared cut: the frame digest, the after digest over the after
//!    pages and XOI, the LSN, the length and the physical roles.
//!
//! `apply`, in order: the deep checks again; the draft commitment recompute;
//! the frame and after digests against the cut, without the observer or the
//! XOI projection (both are fixed in the loan, which only C wrote since
//! `prepare`); the table write bit; the one existing-group install (stage,
//! pages, control through `history::row::record`, durable wait); the
//! statement's revision. Every refusal before the stage is Unchanged, so the
//! wrapper returns the same ReservedRow or PreparedRow; every failure after
//! the stage is Fatal.
//!
//! One suspended caller level holds a return carrier in each step (S2-0b
//! decision 2): `prepare_forward`'s `observation`, the observer's result held
//! through the inventory and progress checks (`PREPARE_SUSPENDED_LEVEL`), and
//! `forward_group`'s `group`, the reassembled group held through the install
//! (`APPLY_SUSPENDED_LEVEL`). The entry checks run beside them, not under
//! them.
use super::super::super::super::super::super::{
    RowControlObservation, RowOperation, RowPageRole, RowProducerContext, RowProducerInput,
    RowProposal, RowSourceBinding, row::RowPreparedCut,
};
use super::super::super::super::row_preflight::leaf_free_bytes;
use super::*;
use crate::storage::{
    page,
    semantic_row::status::StatementBoundary,
    undo_record::UndoLink,
    wal_frame::{FrameKind, WalFrameHeader},
};

/// The issued identity a ReservedRow holds (its RowSlotReservation's scalars):
/// what every Forward step and the Cancel check the control against. Scalars
/// only; no pin, loan or authority.
#[derive(Clone, Copy)]
pub(in super::super::super::super::super::super) struct RowIssued {
    pub(in super::super::super::super::super::super) actor: TxnRef,
    pub(in super::super::super::super::super::super) root_job: JobSlotRef,
    pub(in super::super::super::super::super::super) row: RowId,
    pub(in super::super::super::super::super::super) undo: UndoRef,
    pub(in super::super::super::super::super::super) reserve_lsn: u64,
}

/// The deep checks' scalars an effect step on a reservation (a Forward step
/// or the Cancel) carries into its group: the draft's issued values (the
/// Undo sequence is the RootJob's), the RootJob ordinal, the Terminal and its
/// Status, the ATT slot and the statement.
#[derive(Clone, Copy)]
pub(super) struct ReservedEntry {
    pub(super) owner: OwnerIdentity,
    pub(super) prefix: WalPrefix,
    pub(super) issued: DraftIssued,
    pub(super) ordinal: usize,
    pub(super) status: StatusSlotRef,
    pub(super) slot: usize,
    pub(super) statement: u64,
}

/// The private ranges a Forward step uses (`RowLoanLayout`): A's frame and
/// the five after images (checked, then the observer's or the install's),
/// the before images (refreshed by each recompute), the Catalog leaf copy,
/// the XOI page and C's observer scratch. Neither the candidate nor A's
/// scratch is touched.
struct ForwardLanes<'l> {
    frame: &'l mut [u8; 98_304],
    before: &'l mut [[u8; PAGE_BYTES]; 9],
    after: &'l mut [[u8; PAGE_BYTES]; 5],
    schema: &'l [u8; PAGE_BYTES],
    inventory: &'l mut [u8; PAGE_BYTES],
    scratch: &'l mut [u8; ROW_REPLAY_SCRATCH_BYTES],
}

impl<'l> ForwardLanes<'l> {
    fn split(private: &'l mut [u8; 425_984]) -> Self {
        let (frame, rest) = private.split_at_mut(RowLoanLayout::FRAME.end);
        let (before, rest) = rest.split_at_mut(range_len(RowLoanLayout::BEFORE));
        let (after, rest) = rest.split_at_mut(range_len(RowLoanLayout::AFTER));
        let (_candidate, rest) = rest.split_at_mut(range_len(RowLoanLayout::CANDIDATE));
        let (schema, rest) = rest.split_at_mut(range_len(RowLoanLayout::SCHEMA));
        let (inventory, rest) = rest.split_at_mut(range_len(RowLoanLayout::XOI));
        let (scratch, _) = rest.split_at_mut(range_len(RowLoanLayout::C_SCRATCH));
        Self {
            frame: frame.try_into().expect("frame range"),
            before: before
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("nine before images"),
            after: after
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("five after images"),
            schema: (&*schema).try_into().expect("schema copy page"),
            inventory: inventory.try_into().expect("XOI page"),
            scratch: (&mut scratch[..ROW_REPLAY_SCRATCH_BYTES])
                .try_into()
                .expect("C scratch"),
        }
    }
}

/// The producer's output role of a changed-page role (`row_group::ROOT` …
/// `row_group::LEAF`).
fn page_role(role: RowPageRole) -> u8 {
    match role {
        RowPageRole::Root => ROOT,
        RowPageRole::RootJob => JOB,
        RowPageRole::Status => STATUS,
        RowPageRole::Undo => row_group::UNDO as u8,
        RowPageRole::RowLeaf => row_group::LEAF as u8,
    }
}

/// A's untrusted declared metadata (§6.6 "prepare"): a Forward, five after
/// pages in five distinct roles and a frame length inside the frame range.
/// Returns the length and the declared output roles in A's physical order.
/// Anything else is Unchanged InvalidInput: nothing was read yet.
fn check_proposal(proposal: &RowProposal) -> Result<(usize, [u8; 5])> {
    let declared = proposal.after_roles.map(page_role);
    let named = declared.iter().fold(0u8, |mask, role| mask | (1 << role));
    if proposal.operation != RowOperation::Forward
        || proposal.after_count != 5
        || named != 0x1f
        || !(1..=RowLoanLayout::FRAME.end).contains(&proposal.frame_len)
    {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::RowSemantic(row_semantic::Error::InvalidInput),
        ));
    }
    Ok((proposal.frame_len, declared))
}

/// The after digest of a prepared cut (`RowPreparedCut::after`): the count,
/// the five roles and the five pages in physical order, then the XOI page
/// when given. Without it, the comparison value of A's pages and the
/// observer's.
fn after_fold<'x>(
    roles: &[u8; 5],
    pages: impl Iterator<Item = &'x [u8; PAGE_BYTES]>,
    inventory: Option<&[u8; PAGE_BYTES]>,
) -> [u8; 32] {
    let mut hasher = ContentHasher::new();
    hasher.update(&[5]);
    hasher.update(roles);
    for page in pages {
        hasher.update(page);
    }
    if let Some(inventory) = inventory {
        hasher.update(inventory);
    }
    hasher.digest()
}

/// Five output roles, three bits each, in physical order.
fn pack_roles(order: &[u8; 5]) -> u16 {
    order.iter().enumerate().fold(0, |packed, (at, role)| {
        packed | (u16::from(*role) << (3 * at))
    })
}

/// The roles `pack_roles` packed, if they are five distinct output roles.
fn unpack_roles(packed: u16, count: u8) -> Option<[u8; 5]> {
    let order: [u8; 5] = std::array::from_fn(|at| ((packed >> (3 * at)) & 7) as u8);
    let named = order.iter().fold(0u8, |mask, role| {
        mask | (1u8.checked_shl(u32::from(*role)).unwrap_or(0))
    });
    (count == 5 && packed >> 15 == 0 && named == 0x1f).then_some(order)
}

/// The observation of a Forward group of this row, from the deep checks'
/// scalars and the checked frame: what the observer reports for it, and what
/// `apply` reassembles without the observer.
fn forward_observation(
    entry: &ReservedEntry,
    header: WalFrameHeader,
    order: [u8; 5],
    frame_bytes: usize,
) -> row_group::Observation {
    row_group::Observation {
        operation: row_semantic::Operation::Forward,
        header,
        order,
        count: 5,
        frame_bytes,
        job_ordinal: Some(entry.ordinal),
        actor: Some(entry.issued.actor),
        status: Some(entry.status),
        issued: None,
    }
}

// S2W-R21(2): the next Forward recompute's typed read on this thread reports
// a write target of T not resident, once. No page, control or WAL byte
// changes.
#[cfg(test)]
thread_local! {
    static TEST_MISSING_TARGET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(in super::super::super::super::super::super) fn test_missing_target_once() -> TestOnce {
    TestOnce::arm(&TEST_MISSING_TARGET)
}
#[cfg(test)]
type TestOnce = super::super::super::super::super::super::transaction::writable::TestOnce;

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The producer lane's context (S2-0 §6.6 "producer 입력"), built from
    /// the private copies the draft step refreshed and the control: T and its
    /// role map from the before images (`before_targets`), the schema
    /// context as `validation_lane` rebuilds it with the RowLeaf free bytes
    /// of the before image, the issued identity, the Terminal and its
    /// Status, the census predecessor H, each source's before position and
    /// cell, the five changed roles in physical (before) order, the Forward
    /// header after the draft's complete LSN, the statement's sequence and
    /// mark, and the sealed input. It checks no chain: `prepare`'s recompute
    /// refuses any change before an effect. A copy or a control row that does
    /// not decode contradicts the draft step's own checks (Fatal).
    pub(in super::super::super::super::super::super) fn row_producer_context(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        (table, draft, issued): (RowTableSource, &RowDraftCut, RowIssued),
        private: &[u8; 425_984],
    ) -> Result<RowProducerContext> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let profile = storage.profile();
        let namespace = profile.spec().namespace;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let before: &[[u8; PAGE_BYTES]; 9] = private[RowLoanLayout::BEFORE]
            .as_chunks::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("nine before images");
        // The Catalog leaf request only fills the positions past |T|.
        let mut targets = [target_from_source(storage, Plan4Page::CatalogLeaf)?.request; 9];
        let count =
            before_targets(profile, before, draft.roles, &mut targets).map_err(|_| invariant())?;
        let position = |role| role_position(draft.roles, role).filter(|at| *at < count);
        let leaf_at = position(RowSourceRole::RowLeaf).ok_or_else(invariant)?;
        let leaf_free =
            leaf_free_bytes(&before[leaf_at], targets[leaf_at]).map_err(|_| invariant())?;
        let (schema, _) =
            self.row_validation_context(owner, table, (draft.roles, leaf_free), private)?;
        let ordinal = issued.root_job.ordinal().map_err(|_| invariant())?;
        let job = job3::decode_job(&self.control.jobs()[ordinal], namespace).map_err(|error| {
            fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Retention(error),
            )
        })?;
        let (terminal, status) = self.catalog_preparation().row_terminal(issued.actor)?;
        let predecessor = self.row_predecessor(issued.actor, job.undo_sequence())?;
        // Each source's cell: (slot, offset, length) in its before image.
        let cell = |at: usize, slot: u16| -> Result<Option<(u16, u16, u16)>> {
            let target = targets[at];
            let view = page::decode_page(&before[at], target.identity, target.kind)
                .map_err(|_| invariant())?;
            let range = view.cell_range(usize::from(slot)).ok_or_else(invariant)?;
            let offset = u16::try_from(range.start).map_err(|_| invariant())?;
            let length = u16::try_from(range.len()).map_err(|_| invariant())?;
            Ok(Some((slot, offset, length)))
        };
        let slot = |value: usize| u16::try_from(value % 15).map_err(|_| invariant());
        let mut sources = [const { None }; 9];
        let roles = [
            RowSourceRole::Root,
            RowSourceRole::RootJob,
            RowSourceRole::TerminalJob,
            RowSourceRole::Status,
            RowSourceRole::Undo,
            RowSourceRole::RowLeaf,
            RowSourceRole::Allocator,
            RowSourceRole::TableAnchor,
            RowSourceRole::PreviousUndo,
        ];
        for (index, role) in roles.into_iter().enumerate() {
            let Some(at) = position(role) else {
                continue;
            };
            let target = targets[at];
            let cell = match role {
                RowSourceRole::Root | RowSourceRole::TableAnchor => cell(at, 0)?,
                RowSourceRole::RootJob => cell(at, slot(ordinal)?)?,
                RowSourceRole::TerminalJob => cell(at, slot(terminal)?)?,
                RowSourceRole::Status => cell(at, status.slot())?,
                RowSourceRole::Undo => cell(at, issued.undo.slot())?,
                RowSourceRole::PreviousUndo => {
                    let link = predecessor.ok_or_else(invariant)?;
                    cell(at, link.reference.slot())?
                }
                RowSourceRole::RowLeaf | RowSourceRole::Allocator => None,
            };
            sources[index] = Some(RowSourceBinding {
                before_index: u8::try_from(at).map_err(|_| invariant())?,
                identity: target.identity,
                kind: target.kind,
                level: target.level,
                cell,
            });
        }
        // The five changed roles in physical order: T is in rank order and
        // the producer roles name distinct pages (`row_write_roles`).
        let mut changed = [
            (RowPageRole::Root, RowSourceRole::Root),
            (RowPageRole::RootJob, RowSourceRole::RootJob),
            (RowPageRole::Status, RowSourceRole::Status),
            (RowPageRole::Undo, RowSourceRole::Undo),
            (RowPageRole::RowLeaf, RowSourceRole::RowLeaf),
        ]
        .map(|(page, source)| (position(source), page));
        if changed.iter().any(|(at, _)| at.is_none()) {
            return Err(invariant());
        }
        changed.sort_unstable_by_key(|(at, _)| *at);
        let stream = storage
            .wal()
            .prefix()
            .map_err(|error| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(error)))?
            .stream;
        let previous_lsn = draft.complete_lsn;
        Ok(RowProducerContext {
            schema,
            operation: RowOperation::Forward,
            frame_expected: FrameExpectation {
                stream,
                previous_lsn,
            },
            header: WalFrameHeader {
                stream,
                kind: FrameKind::Update,
                lsn: previous_lsn.checked_add(1).ok_or_else(invariant)?,
                previous_lsn,
                txn: Some(issued.actor.id()),
                previous_txn_lsn: issued.reserve_lsn,
            },
            actor: issued.actor,
            status,
            row: issued.row,
            undo: UndoLink {
                reference: issued.undo,
                sequence: job.undo_sequence(),
            },
            statement_sequence: Some(statement.row_sequence()?),
            planned_mutation: draft.planned_mutation,
            statement_mark: statement.row_mark()?,
            predecessor,
            root_job_ordinal: u8::try_from(ordinal).map_err(|_| invariant())?,
            terminal_job_ordinal: u8::try_from(terminal).map_err(|_| invariant())?,
            before_count: u8::try_from(count).map_err(|_| invariant())?,
            sources,
            after_order: changed.map(|(_, page)| page),
            after_count: 5,
            input: RowProducerInput::Insert {
                canonical_len: usize::from(draft.input_len),
                canonical_digest: draft.input,
            },
        })
    }

    /// The control the producer lane lends (`RowControlObservation`): the 64
    /// Jobs rows, the actor's ATT descriptor and the Aging descriptor when
    /// its row is in use (the phase byte, as the draft commitment folds it).
    pub(in super::super::super::super::super::super) fn row_control_observation(
        &self,
        actor: TxnRef,
    ) -> Result<RowControlObservation<'_>> {
        let storage = self.storage;
        let slot = self
            .control
            .slot(actor)
            .ok_or_else(|| fatal(storage, RetentionPhase::Census, RetentionCause::Invariant))?;
        let aging = self.control.row(control::AGING_SLOT);
        Ok(RowControlObservation {
            jobs: self.control.jobs(),
            transaction: Some(
                self.control.row(slot)[..1536]
                    .try_into()
                    .expect("original transaction descriptor"),
            ),
            aging: (aging[control::PHASE] != 0)
                .then(|| aging[..1536].try_into().expect("original Aging descriptor")),
        })
    }

    /// The census predecessor H of a Row root with Undo sequence `sequence`:
    /// the actor's RootRetained root with the largest smaller sequence, as
    /// the producer and the observer select it.
    fn row_predecessor(&self, actor: TxnRef, sequence: u64) -> Result<Option<UndoLink>> {
        let storage = self.storage;
        let namespace = storage.profile().spec().namespace;
        let mut found: Option<UndoLink> = None;
        for bytes in self.control.jobs() {
            let job = job3::decode_job(bytes, namespace).map_err(|error| {
                fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Retention(error),
                )
            })?;
            if job.source() == Some(actor)
                && job.state() == job3::JobState::RootRetained
                && job.undo_sequence() < sequence
                && found.is_none_or(|link| link.sequence < job.undo_sequence())
            {
                found = Some(UndoLink {
                    reference: job.undo().ok_or_else(|| {
                        fatal(storage, RetentionPhase::Census, RetentionCause::Invariant)
                    })?,
                    sequence: job.undo_sequence(),
                });
            }
        }
        Ok(found)
    }

    /// The deep checks of an effect step on a reservation, one helper for
    /// the Forward `prepare` and `apply` and the Cancel: the entry of an
    /// existing-group preparation (health, idle owner, WAL prefix, whole
    /// ledger), the view and its statement, the actor (the reservation's),
    /// its Terminal and ATT slot, and the table's epoch slot. A complete LSN
    /// other than the draft's is Before(Stale) without rehashing. The actor's
    /// statement head is not checked as reserve checks it: this actor is
    /// RootReserved by the reservation, and the draft commitment folds every
    /// Job row and the actor's ATT row, so any change of them since the
    /// Reserve is Stale. A view whose actor is not the reservation's, or a
    /// draft that is not a Reserve's, contradicts the wrapper (Fatal Census).
    /// The RootJob's Undo sequence, which the commitment folds, is read from
    /// that Job. Its Fatal(Census, Retention) arm is defensive: the whole
    /// ledger check above has already decoded every Job cell and refuses an
    /// undecodable one as Unchanged(Census, Retention), so under the
    /// exclusive writer the arm is not reached; a RootJob that did not decode
    /// here would be the control's corruption, Fatal as every other deep
    /// check's Job decode (S2-c review A6, S2-d review A4).
    pub(super) fn reserved_entry(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        table: RowTableSource,
        (draft, issued): (&RowDraftCut, RowIssued),
    ) -> Result<ReservedEntry> {
        let storage = self.storage;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let preparation = self.catalog_preparation();
        let (identity, prefix) = preparation.existing_entry(owner)?;
        statement.check_catalog_read()?;
        let actor = statement
            .own_actor()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        // The reservation was issued to this statement's actor by a Reserve;
        // anything else contradicts the wrapper's own proof.
        if actor != issued.actor || draft.operation != 0 {
            return Err(invariant());
        }
        let (_, status) = preparation.row_terminal(actor)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        self.check_table_slot(table)?;
        if prefix.complete_lsn != draft.complete_lsn {
            return Err(stale());
        }
        let ordinal = issued.root_job.ordinal().map_err(|_| invariant())?;
        let sequence = job3::decode_job(
            &self.control.jobs()[ordinal],
            storage.profile().spec().namespace,
        )
        .map_err(|error| {
            fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Retention(error),
            )
        })?
        .undo_sequence();
        Ok(ReservedEntry {
            owner: identity,
            prefix,
            issued: DraftIssued {
                actor,
                root_job: issued.root_job,
                row: issued.row,
                undo: issued.undo,
                sequence,
                mutation: draft.planned_mutation,
            },
            ordinal,
            status,
            slot,
            statement: statement.row_sequence()?,
        })
    }

    /// The Forward `prepare` (module doc): the deep checks, then the group
    /// preparation, which returns only the scalar cut. An Unchanged error is
    /// a Before: nothing was staged or registered. Any other error is Fatal.
    pub(in super::super::super::super::super::super) fn row_prepare_forward(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        (table, draft, issued): (RowTableSource, &RowDraftCut, RowIssued),
        proposal: &RowProposal,
        private: &mut [u8; 425_984],
    ) -> Result<RowPreparedCut> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let entry = self.reserved_entry(owner, statement, table, (draft, issued))?;
        self.catalog_preparation().prepare_forward(
            owner,
            &entry,
            (table, draft, proposal),
            ForwardLanes::split(private),
        )
    }

    /// The Forward `apply` (module doc): the deep checks, the group with its
    /// install, then the statement's revision. An Unchanged error is a
    /// Before: nothing was staged, and only the conservative write bit may be
    /// registered. Any other error is Fatal and the fence stays.
    pub(in super::super::super::super::super::super) fn row_apply_forward(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        (table, draft, issued): (RowTableSource, &RowDraftCut, RowIssued),
        cut: &RowPreparedCut,
        private: &mut [u8; 425_984],
    ) -> Result<()> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let entry = self.reserved_entry(owner, statement, table, (draft, issued))?;
        self.forward_group(owner, statement, &entry, (table, draft, cut), private)?;
        // The group is durable: the view's revision may not stay behind.
        statement
            .row_applied()
            .map_err(|error| fatal(storage, RetentionPhase::Install, error.cause))
    }

    /// The reassembled group, the table write bit and the install. The one
    /// suspended level of `apply`: `group` is the slot of
    /// `prepare_forward_apply`'s result, matched by reference so that the
    /// group is never moved into a second slot, and it is held through the
    /// install.
    fn forward_group(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        entry: &ReservedEntry,
        (table, draft, cut): (RowTableSource, &RowDraftCut, &RowPreparedCut),
        private: &mut [u8; 425_984],
    ) -> Result<()> {
        let ForwardLanes {
            frame,
            before,
            after,
            inventory,
            ..
        } = ForwardLanes::split(private);
        let group = self.catalog_preparation().prepare_forward_apply(
            owner,
            entry,
            (draft, cut),
            before,
            (&*frame, &*after, &*inventory),
        );
        let prepared = match group {
            Ok(ref prepared) => prepared,
            Err(error) => return Err(error),
        };
        // The table write bit again, after the recompute and before the stage
        // (§6): a Before after it keeps the conservative bit.
        statement.record_table_write(table.slot)?;
        #[cfg(test)]
        test_stage_refusal()?;
        self.catalog_application()
            .apply_existing(owner, ExistingGroup::RowWrite(prepared))
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// Steps 2 to 6 of `prepare` (module doc). Every refusal is Unchanged
    /// before any stage, except a contradiction of C's own copies, role map
    /// or observer (Fatal). The one suspended level: `observation`, the
    /// observer's result, held through the identity, inventory and progress
    /// checks.
    fn prepare_forward(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &ReservedEntry,
        (table, draft, proposal): (RowTableSource, &RowDraftCut, &RowProposal),
        lanes: ForwardLanes<'_>,
    ) -> Result<RowPreparedCut> {
        let storage = self.storage;
        let profile = storage.profile();
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let (frame_len, declared) = check_proposal(proposal)?;
        let ForwardLanes {
            frame,
            before,
            after,
            schema,
            inventory,
            scratch,
        } = lanes;
        // The Catalog leaf request only fills the positions past |T|.
        let mut requests = [target_from_source(storage, Plan4Page::CatalogLeaf)?.request; 9];
        let mut stamps = [None; 9];
        let count =
            before_targets(profile, before, draft.roles, &mut requests).map_err(|_| invariant())?;
        let roles = row_write_roles(draft.roles, count).ok_or_else(invariant)?;
        self.draft_recompute(
            owner,
            (entry.prefix, entry.slot, entry.issued),
            (&requests[..count], draft),
            before,
            &mut stamps,
        )?;
        let before: &[[u8; PAGE_BYTES]; 9] = before;
        // A's pages in A's physical order, before the observer writes its own.
        let proposed = after_fold(&declared, after.iter(), None);
        let catalog = target_from_source(storage, Plan4Page::CatalogLeaf)?.request;
        let definition =
            table_definition(schema, catalog.identity, table, profile.spec().namespace)
                .map_err(|_| invariant())?;
        let at = |role: u8| {
            roles[..count]
                .iter()
                .position(|actual| *actual == role)
                .ok_or_else(invariant)
        };
        let expected = FrameExpectation {
            stream: entry.prefix.stream,
            previous_lsn: entry.prefix.complete_lsn,
        };
        let bytes: &[u8] = &frame[..frame_len];
        scratch.fill(0);
        let observation = {
            let anchor = role_position(draft.roles, RowSourceRole::TableAnchor)
                .filter(|at| *at < count)
                .ok_or_else(invariant)?;
            let previous = role_position(draft.roles, RowSourceRole::PreviousUndo)
                .filter(|at| *at < count)
                .map(|at| &before[at]);
            let aging = self.control.row(control::AGING_SLOT);
            let inputs = row_group::Inputs {
                profile,
                frame: expected,
                // Replay's boundary: the observer takes no statement mark
                // for a Forward.
                change: row_group::Change::Forward {
                    ordinal: entry.ordinal,
                    statement: StatementBoundary {
                        sequence: entry.statement,
                        mark: None,
                    },
                },
                jobs: self.control.jobs(),
                root: &before[at(ROOT)?],
                jobs_page: Some(&before[at(JOB)?]),
                status: Some(&before[at(STATUS)?]),
                undo: Some(&before[at(row_group::UNDO as u8)?]),
                previous,
                table: Some(row_group::TableSource {
                    handle: table.handle,
                    anchor: table.anchor.page,
                    anchor_image: &before[anchor],
                    leaf_image: Some(&before[at(row_group::LEAF as u8)?]),
                    schema: RowSchemaInput::Catalog(definition),
                }),
                transaction: Some(
                    self.control.row(entry.slot)[..1536]
                        .try_into()
                        .expect("original transaction descriptor"),
                ),
                aging: (aging[control::PHASE] != 0)
                    .then(|| aging[..1536].try_into().expect("original Aging descriptor")),
            };
            match row_group::observe_borrowed(&inputs, bytes, after, scratch) {
                Ok(observation) => observation,
                Err(error) => return Err(row_semantic_error(error)),
            }
        };
        let after: &[[u8; PAGE_BYTES]; 5] = after;
        let order = observation.order;
        let pages = || order.iter().map(|role| &after[usize::from(*role)]);
        // A's declared order and pages against the observer's.
        if usize::from(observation.count) != 5 || after_fold(&order, pages(), None) != proposed {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::RowSemantic(row_semantic::Error::IdentityMismatch),
            ));
        }
        // The observer's own scalars are the reservation's: `apply` rebuilds
        // this observation from them (C's own output: `own_output`).
        if observation != forward_observation(entry, observation.header, order, frame_len) {
            return Err(own_output(storage));
        }
        // Every changed page is a T member at that identity (C's own output:
        // `own_output`).
        for role in &order {
            let target = requests[at(*role).map_err(|_| own_output(storage))?];
            let image = recovery::request_from_image(profile, &after[usize::from(*role)])
                .map_err(|cause| fatal(storage, RetentionPhase::Prepare, cause))?;
            if image != target {
                return Err(own_output(storage));
            }
        }
        let root = &after[usize::from(ROOT)];
        let future = {
            let layout =
                row_semantic::decode_layout(bytes, expected, profile).map_err(|error| {
                    fatal(
                        storage,
                        RetentionPhase::Prepare,
                        RetentionCause::RowSemantic(error),
                    )
                })?;
            prepare_inventory(
                self.control,
                profile,
                observation.header.lsn,
                GroupRows::row(&observation, Some(entry.slot), layout),
                root,
                &before[at(ALLOCATOR_ROLE)?],
                Some(&after[usize::from(JOB)]),
                inventory,
            )
            .map_err(|error| produced(storage, error))?
        };
        check_progress_room(
            entry.prefix,
            (observation.header.lsn, observation.frame_bytes),
            future,
            (&roles[..count], &stamps[..count]),
            &order,
            root,
            profile,
        )
        .map_err(|error| produced(storage, error))?;
        Ok(RowPreparedCut {
            frame: content_digest(bytes),
            after: after_fold(&order, pages(), Some(&*inventory)),
            lsn: observation.header.lsn,
            frame_len: u32::try_from(frame_len).map_err(|_| invariant())?,
            after_roles: pack_roles(&order),
            after_count: 5,
            operation: 0,
        })
    }

    /// `apply`'s group: the draft commitment recompute (T's stamps for the
    /// install), then the frame and after digests and the header LSN against
    /// the prepared cut, and the group over the loan's frame, after images
    /// and XOI page with the observation rebuilt from the deep checks'
    /// scalars. A digest that differs contradicts `prepare`'s own checks of
    /// a loan only C wrote since (Fatal).
    fn prepare_forward_apply<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &ReservedEntry,
        (draft, cut): (&RowDraftCut, &RowPreparedCut),
        before: &mut [[u8; PAGE_BYTES]; 9],
        (frame, after, inventory): (
            &'i [u8; 98_304],
            &'i [[u8; PAGE_BYTES]; 5],
            &'i [u8; PAGE_BYTES],
        ),
    ) -> Result<PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>> {
        let storage = self.storage;
        let profile = storage.profile();
        let census = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let invariant = || fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant);
        // The Catalog leaf request only fills the positions past |T|.
        let mut requests =
            [target_from_source(storage, Plan4Page::CatalogLeaf)?.request; ROW_WRITE_TARGETS];
        let mut stamps = [None; ROW_WRITE_TARGETS];
        let count =
            before_targets(profile, before, draft.roles, &mut requests).map_err(|_| census())?;
        let roles = row_write_roles(draft.roles, count).ok_or_else(census)?;
        self.draft_recompute(
            owner,
            (entry.prefix, entry.slot, entry.issued),
            (&requests[..count], draft),
            before,
            &mut stamps,
        )?;
        let order = unpack_roles(cut.after_roles, cut.after_count).ok_or_else(invariant)?;
        let bytes = usize::try_from(cut.frame_len)
            .ok()
            .and_then(|length| frame.get(..length))
            .ok_or_else(invariant)?;
        let frame_digest = content_digest(bytes);
        if cut.operation != 0
            || frame_digest != cut.frame
            || after_fold(
                &order,
                order.iter().map(|role| &after[usize::from(*role)]),
                Some(inventory),
            ) != cut.after
        {
            return Err(invariant());
        }
        let expected = FrameExpectation {
            stream: entry.prefix.stream,
            previous_lsn: entry.prefix.complete_lsn,
        };
        let header = row_semantic::decode_layout(bytes, expected, profile)
            .map_err(|error| {
                fatal(
                    storage,
                    RetentionPhase::Prepare,
                    RetentionCause::RowSemantic(error),
                )
            })?
            .header();
        if header.lsn != cut.lsn {
            return Err(invariant());
        }
        Ok(PreparedRowGroup {
            payload: RowPayload {
                core: self.pool.bound_core(),
                binding: &storage.binding,
                fence: storage.session_fence(),
                prefix: entry.prefix,
                observation: forward_observation(entry, header, order, bytes.len()),
                slot: Some(entry.slot),
                frame: bytes,
                frame_digest,
                after,
                inventory,
                control: ControlCut::capture(self.control),
            },
            owner: entry.owner,
            requests,
            roles,
            count,
            stamps,
        })
    }

    /// The kind-2 recompute (S2-0b "digest chain", prepare(Forward) and
    /// cancel) with no replay: under one typed read of T (`targets`, |T| of
    /// seven to nine) each pool page is copied into the before range and the
    /// same `DraftSource::commitment` the Reserve recorded (`reserve_draft`)
    /// is folded over those copies, the WAL prefix, the issued identity, the
    /// actor's ATT row (`slot`), the Aging row and the control cut. On
    /// equality it writes T's stamps into `stamps` for the observer's
    /// progress check and the install's recheck. A difference is
    /// Before(Stale); a target that is not resident is Fatal(Invariant); a
    /// commitment that does not fold contradicts the draft (Fatal Census).
    /// One helper for the Forward and the Cancel, so the two never fold the
    /// inputs differently (a permanent Stale between them).
    pub(super) fn draft_recompute(
        &self,
        owner: &mut Owner<'c, 's>,
        (prefix, slot, issued): (WalPrefix, usize, DraftIssued),
        (targets, draft): (&[TargetRequest], &RowDraftCut),
        before: &mut [[u8; PAGE_BYTES]; 9],
        stamps: &mut [Option<FrameStamp>; ROW_WRITE_TARGETS],
    ) -> Result<()> {
        let source = (prefix, slot, issued);
        match targets.len() {
            7 => self.draft_read::<7>(owner, source, (targets, draft), before, stamps),
            8 => self.draft_read::<8>(owner, source, (targets, draft), before, stamps),
            9 => self.draft_read::<9>(owner, source, (targets, draft), before, stamps),
            _ => Err(fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )),
        }
    }

    fn draft_read<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        (prefix, slot, issued): (WalPrefix, usize, DraftIssued),
        (targets, draft): (&[TargetRequest], &RowDraftCut),
        before: &mut [[u8; PAGE_BYTES]; 9],
        stamps: &mut [Option<FrameStamp>; ROW_WRITE_TARGETS],
    ) -> Result<()> {
        let storage = self.storage;
        let requests: [TargetRequest; N] = std::array::from_fn(|at| targets[at]);
        #[cfg(test)]
        if TestOnce::take(&TEST_MISSING_TARGET) {
            let error = dirty_frame::DirtyError {
                cause: dirty_frame::Cause::MissingTarget,
                effect: Effect::Unchanged,
            };
            return Err(resident_read(storage, RetentionPhase::Prepare, error));
        }
        let read = self
            .access
            .read(self.pool, owner, &requests)
            .map_err(|error| resident_read(storage, RetentionPhase::Prepare, error))?;
        let folded = (|| {
            let first = read
                .copy_snapshot(0, &mut before[0])
                .map_err(|error| frame(RetentionPhase::Prepare, error))?;
            let mut observed = [first; N];
            for (at, stamp) in observed.iter_mut().enumerate().skip(1) {
                *stamp = read
                    .copy_snapshot(at, &mut before[at])
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?;
            }
            let folded = observed;
            #[cfg(test)]
            let folded = {
                let mut folded = folded;
                if TestOnce::take(&TEST_STALE_STAMP) {
                    folded[0].modification_counter = folded[0].modification_counter.wrapping_add(1);
                }
                folded
            };
            let aging = self.control.row(control::AGING_SLOT);
            let source = DraftSource {
                prefix,
                targets: &requests,
                stamps: &folded,
                roles: draft.roles,
                issued,
                transaction: self.control.row(slot)[..1536]
                    .try_into()
                    .expect("original transaction descriptor"),
                aging: (aging[control::PHASE] != 0)
                    .then(|| aging[..1536].try_into().expect("original Aging descriptor")),
                jobs: self.control.jobs(),
                pages: before[..N].iter(),
                control: ControlCut::capture(self.control),
            }
            .commitment()
            .map_err(|cause| fatal(storage, RetentionPhase::Census, cause))?;
            if source != draft.source {
                return Err(stale());
            }
            for (stamp, observed) in stamps.iter_mut().zip(observed) {
                *stamp = Some(observed);
            }
            Ok(())
        })();
        merge(
            folded,
            read.release_checked()
                .map_err(|error| frame(RetentionPhase::Prepare, error)),
        )
    }
}

#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// S2W-R24's control half: the live Aging descriptor (control row
    /// `AGING_SLOT`) re-encoded over `root_page` by the Root3-bound encoder,
    /// its progress cursor set to that Root's free cursor (Root cell offset
    /// 72), so the descriptor's cursor and its work digests of the Root page
    /// and cell mirror the changed Root as an install would leave them.
    pub(in super::super::super::super::super::super) fn row_test_mirror_aging(
        &mut self,
        root_page: &[u8; PAGE_BYTES],
    ) {
        use crate::storage::{allocation_page::ReservationOwner, obligation, retention_obligation};
        let profile = self.storage.profile();
        let (_, root) =
            job3::decode_root_page(root_page, profile.spec().namespace).expect("the Root");
        let at = control::AGING_SLOT * control::SLOT_BYTES;
        let raw: &[u8; 1536] = self.control.bytes[at..at + 1536].try_into().unwrap();
        let mut spec = obligation::decode_aging_descriptor(
            raw,
            ReservationOwner::System(root.aging().expect("the Aging reference")),
            retention_obligation::profile_reference(profile),
        )
        .expect("the Aging descriptor");
        let obligation::OwnerState::System(ref mut progress) = spec.owner else {
            panic!("a system owner");
        };
        progress.cursor = root.cursor();
        retention_obligation::row::encode(
            &spec,
            profile,
            root_page,
            &mut self.control.bytes[at..at + 1536],
        )
        .expect("the mirrored Aging descriptor");
    }

    /// S2W-R24's draft half: the kind-2 commitment folded as `draft_read`
    /// folds it, over T copied from the current pool into the loan's before
    /// range and the current control, for a fixture that changed both after
    /// the Reserve. The before range is then the pool's, as after a
    /// recompute.
    pub(in super::super::super::super::super::super) fn row_test_draft_source(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        (table, draft, issued): (RowTableSource, &RowDraftCut, RowIssued),
        private: &mut [u8; 425_984],
    ) -> Result<[u8; 32]> {
        let storage = self.storage;
        let entry = self.reserved_entry(owner, statement, table, (draft, issued))?;
        let ForwardLanes { before, .. } = ForwardLanes::split(private);
        let mut requests = [target_from_source(storage, Plan4Page::CatalogLeaf)?.request; 9];
        let count = before_targets(storage.profile(), before, draft.roles, &mut requests)
            .expect("T from the before images");
        let preparation = self.catalog_preparation();
        match count {
            7 => preparation.test_draft_fold::<7>(owner, &entry, (&requests, draft), before),
            8 => preparation.test_draft_fold::<8>(owner, &entry, (&requests, draft), before),
            _ => preparation.test_draft_fold::<9>(owner, &entry, (&requests, draft), before),
        }
    }
}

#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    fn test_draft_fold<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &ReservedEntry,
        (targets, draft): (&[TargetRequest; 9], &RowDraftCut),
        before: &mut [[u8; PAGE_BYTES]; 9],
    ) -> Result<[u8; 32]> {
        let storage = self.storage;
        let requests: [TargetRequest; N] = std::array::from_fn(|at| targets[at]);
        let read = self
            .access
            .read(self.pool, owner, &requests)
            .map_err(|error| resident_read(storage, RetentionPhase::Prepare, error))?;
        let mut stamps = Vec::with_capacity(N);
        for (at, page) in before.iter_mut().enumerate().take(N) {
            stamps.push(read.copy_snapshot(at, page).expect("a copy of T"));
        }
        read.release_checked().expect("the typed read");
        let stamps: [FrameStamp; N] = stamps.try_into().expect("|T| stamps");
        let aging = self.control.row(control::AGING_SLOT);
        DraftSource {
            prefix: entry.prefix,
            targets: &requests,
            stamps: &stamps,
            roles: draft.roles,
            issued: entry.issued,
            transaction: self.control.row(entry.slot)[..1536]
                .try_into()
                .expect("original transaction descriptor"),
            aging: (aging[control::PHASE] != 0)
                .then(|| aging[..1536].try_into().expect("original Aging descriptor")),
            jobs: self.control.jobs(),
            pages: before[..N].iter(),
            control: ControlCut::capture(self.control),
        }
        .commitment()
        .map_err(|cause| fatal(storage, RetentionPhase::Census, cause))
    }
}

/// The one suspended caller level of the Forward `prepare` that holds an
/// Observation return carrier (S2-0b decision 2), as the census names it.
pub(in super::super::super::super::super::super) const PREPARE_SUSPENDED_LEVEL: &str =
    "prepare_forward.observation: Result<Observation>";

/// The one suspended caller level of `apply` that holds a Row group return
/// carrier (S2-0b decision 2), as the census names it.
pub(in super::super::super::super::super::super) const APPLY_SUSPENDED_LEVEL: &str =
    "forward_group.group: Result<PreparedRowGroup<ROW_WRITE_TARGETS>>";

/// Source-bound inventory of the actual Forward steps, in the repository's
/// size_of sense (not a stack measurement), as `RowReserveBody`: a call's own
/// values are held throughout it and added; the calls it makes one after
/// another are max-combined. The three steps are `ReservedRow::producer_lane`
/// (`row_producer_context`), `ReservedRow::prepare` (`row_prepare_forward`)
/// and `PreparedRow::apply` (`row_apply_forward`). Each step's wrapper, input
/// and result carriers are the Row write census's `carriers`
/// (`row_write_phases`).
#[derive(Clone, Copy, Debug)]
pub(in super::super::super::super::super::super) struct RowForwardBody {
    /// The kind-2 recompute (`draft_recompute` with `draft_read::<9>`): the
    /// typed read of T, the copies' stamps and the draft commitment.
    pub(in super::super::super::super::super::super) recompute: usize,
    /// `reserved_entry`: its own values, then the deep checks (preflight's
    /// `check` term) and the RootJob decode one after another.
    pub(in super::super::super::super::super::super) entry: usize,
    /// The S2-a′ shell and `producer_lane`'s own values: the sources, the
    /// context and control Results and the lane split.
    pub(in super::super::super::super::super::super) lane_shell: usize,
    /// `row_producer_context`: its own values and the largest of its calls
    /// (T from the copies, the RowLeaf free bytes, the validation context,
    /// the RootJob decode, a Jobs scan, a cell decode, the WAL prefix).
    pub(in super::super::super::super::super::super) lane_context: usize,
    /// `row_control_observation`.
    pub(in super::super::super::super::super::super) lane_control: usize,
    /// `lane_shell + max(lane_context, lane_control)`.
    pub(in super::super::super::super::super::super) producer_lane: usize,
    /// The S2-a′ shell and `ReservedRow::prepare`'s own values.
    pub(in super::super::super::super::super::super) prepare_shell: usize,
    /// `prepare`'s Fatal arm after the body (`reserved_fatal`, which takes
    /// the ReservedRow by value).
    pub(in super::super::super::super::super::super) prepare_arms: usize,
    /// `row_prepare_forward`'s own values. It calls `reserved_entry` and
    /// `prepare_forward` one after another.
    pub(in super::super::super::super::super::super) prepare_frame: usize,
    /// The suspended level (`PREPARE_SUSPENDED_LEVEL`) times its carrier: the
    /// observer's Result and the Observation bound from it, held from the
    /// observer through the cut.
    pub(in super::super::super::super::super::super) prepare_held: usize,
    /// `prepare_forward`'s own values other than `prepare_held`.
    pub(in super::super::super::super::super::super) prepare_group: usize,
    /// C's observer of A's frame over the before images: Inputs and
    /// `observe_borrowed`.
    pub(in super::super::super::super::super::super) observe: usize,
    /// `prepare_forward`'s largest call: T, the roles, `recompute`, an after
    /// fold, the table definition, `observe`, the changed-target identity,
    /// the shared inventory/progress term and the cut's digests.
    pub(in super::super::super::super::super::super) prepare_calls: usize,
    /// `prepare_shell + max(prepare_frame + max(entry, prepare_held +
    /// prepare_group + prepare_calls), prepare_arms)`.
    pub(in super::super::super::super::super::super) prepare: usize,
    /// The S2-a′ shell and `PreparedRow::apply`'s own values.
    pub(in super::super::super::super::super::super) apply_shell: usize,
    /// `apply`'s success arm (the ReservedRow moved out of the PreparedRow,
    /// the observation and `release_discharged`) or its Fatal arm, after the
    /// body.
    pub(in super::super::super::super::super::super) apply_arms: usize,
    /// `row_apply_forward`'s own values. It calls `reserved_entry`,
    /// `forward_group` and the statement's revision one after another.
    pub(in super::super::super::super::super::super) apply_frame: usize,
    /// The suspended level (`APPLY_SUSPENDED_LEVEL`) times its return slot:
    /// `forward_group`'s `group`, held from the reassembly through the
    /// install.
    pub(in super::super::super::super::super::super) apply_held: usize,
    /// `forward_group`'s own values other than `apply_held`.
    pub(in super::super::super::super::super::super) apply_group: usize,
    /// `prepare_forward_apply`: its own values and the largest of T, the
    /// roles, `recompute`, the frame and after digests, the layout decode
    /// and the assembly.
    pub(in super::super::super::super::super::super) reassemble: usize,
    /// `record_table_write`.
    pub(in super::super::super::super::super::super) register: usize,
    /// The existing-group install at nine targets (the S2-a′
    /// `group_install`).
    pub(in super::super::super::super::super::super) install: usize,
    /// `apply_shell + max(apply_frame + max(entry, apply_held + apply_group +
    /// max(reassemble, register, install)), apply_arms)`.
    pub(in super::super::super::super::super::super) apply: usize,
}

/// The actual Forward census. `check` and `register` are preflight's callee
/// terms of the same calls (`RowPreflightBody`); `install` and `inventory`
/// are the S2-a′ install and shared inventory terms at nine targets, which
/// `apply` and `prepare` call unchanged. The helper terms (`fold`,
/// `identity`, `targets`, `roles`, `assembled`) are charged as
/// `row_reserve_body` charges the same calls; the validation context is
/// `row_validation_lane_body` at a zero shell, its wrapper values included.
pub(in super::super::super::super) fn row_forward_body<W: WriterNamespace>(
    shell: usize,
    check: usize,
    register: usize,
    (install, inventory): (usize, usize),
) -> RowForwardBody {
    use super::super::super::super::super::super::read::{
        catalog_source_bytes, undo_decode_source_bytes,
    };
    use super::super::super::super::super::super::{ReservedRow, RowMutationObservation};
    use crate::storage::{
        catalog_record::CatalogRecordView, retention_page::RetentionError as JobError,
        session_fence::UnwindFence, wal_io::WalIoError,
    };
    use std::mem::size_of;
    type Read<'a, const N: usize> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Group<'a> = PreparedRowGroup<'a, 'a, 'a, ROW_WRITE_TARGETS>;
    type Cause<T> = std::result::Result<T, RetentionCause>;
    type Job<'a> = std::result::Result<job3::JobView<'a>, JobError>;
    type Res<'a, W> = ReservedRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Loan<'a> = &'a mut [u8; 425_984];
    let reference = size_of::<&()>();
    // ControlCut::capture and a commitment: hashers, the pinned SHA block
    // closure, the encoded actor and references and the digests.
    let fold = 2 * size_of::<ContentHasher>() + 1536 + 24 + 48 + 40 + 4 * size_of::<[u8; 32]>();
    // One `request_from_image` with its page decode, as the lifecycle's
    // `identity` term counts it.
    let identity = 2 * size_of::<TargetRequest>()
        + 2 * size_of::<Cause<TargetRequest>>()
        + 4 * size_of::<crate::storage::page::PageHeader>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<ContentHasher>()
        + 1536;
    // `before_targets` into a caller array.
    let targets = identity + 2 * size_of::<usize>() + 4 * reference;
    let roles = size_of::<Option<[u8; ROW_WRITE_TARGETS]>>() + 6 * size_of::<usize>();
    // The group's assembly as the lifecycle counts it (`row_start`'s
    // `assembled`): the group literal and its Result twice in the assembling
    // call, beside the payload values, the cut capture and the frame digest.
    let assembled = size_of::<Group<'_>>()
        + 2 * size_of::<Result<Group<'_>>>()
        + size_of::<RowPayload<'_, '_, '_>>()
        + fold;
    // One digest over a page run (`content_digest`, `after_fold`).
    let digest = size_of::<ContentHasher>() + 2 * size_of::<[u8; 32]>() + 6 * reference;
    // One Jobs scan (`row_terminal`, `row_predecessor`): the view and its
    // Result beside one Job3 decode, the found value and the loop scalars.
    let scan = size_of::<job3::JobView<'_>>()
        + size_of::<Job<'_>>()
        + size_of::<Option<(usize, StatusSlotRef)>>().max(size_of::<Option<UndoLink>>())
        + 2 * size_of::<usize>()
        + size_of::<RetentionError>()
        + 4 * reference
        + job3::source_bytes();
    // The kind-2 recompute: the dispatch, then `draft_read::<9>`.
    let recompute_frame = size_of::<(WalPrefix, usize, DraftIssued)>()
        + size_of::<[TargetRequest; 9]>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<FrameStamp>()
        + 2 * size_of::<[FrameStamp; 9]>()
        + 2 * size_of::<dirty_frame::Result<FrameStamp>>()
        + size_of::<DraftSource<'_, std::slice::Iter<'_, [u8; PAGE_BYTES]>>>()
        + size_of::<ControlCut>()
        + size_of::<Cause<[u8; 32]>>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<dirty_frame::Result<()>>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closure capture and the Aging row.
        + 12 * reference;
    let recompute = size_of::<(WalPrefix, usize, DraftIssued)>()
        + size_of::<Result<()>>()
        + 6 * reference
        + recompute_frame
        + targets::call_bytes::<9>()
            .max(size_of::<Read<'_, 9>>() + fold.max(size_of::<Cause<[u8; 32]>>()));
    let entry = size_of::<RowTableSource>()
        + size_of::<RowIssued>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Result<(usize, StatusSlotRef)>>()
        + size_of::<Option<usize>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<std::result::Result<usize, JobError>>()
        + size_of::<job3::JobView<'_>>()
        + size_of::<Job<'_>>()
        + size_of::<Result<u64>>()
        + size_of::<ReservedEntry>()
        + 2 * size_of::<RetentionError>()
        + 10 * reference
        // The deep checks, then the RootJob decode.
        + check.max(job3::source_bytes());

    // producer_lane.
    let lane_shell = shell
        + size_of::<(RowTableSource, RowIssued)>()
        + size_of::<Result<RowProducerContext>>()
        + size_of::<RowProducerContext>()
        + size_of::<Result<RowControlObservation<'_>>>()
        + size_of::<RowControlObservation<'_>>()
        + 6 * size_of::<std::result::Result<&[u8; PAGE_BYTES], std::array::TryFromSliceError>>()
        + 2 * size_of::<RetentionError>()
        // Storage, the loan, seven lent slices (two words each) and the
        // split's rests.
        + 22 * reference;
    let context_frame = size_of::<crate::storage::control::Profile>()
        + size_of::<crate::api::Namespace>()
        + size_of::<UnwindFence<'_>>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<[TargetRequest; 9]>()
        + size_of::<Cause<usize>>()
        + 3 * size_of::<Option<usize>>()
        + size_of::<Cause<u16>>()
        + size_of::<Result<(RowPreflightContext, usize)>>()
        + size_of::<RowPreflightContext>()
        + size_of::<std::result::Result<usize, JobError>>()
        + size_of::<job3::JobView<'_>>()
        + size_of::<Job<'_>>()
        + size_of::<Result<(usize, StatusSlotRef)>>()
        + size_of::<Result<Option<UndoLink>>>()
        + size_of::<Option<UndoLink>>()
        + 2 * size_of::<Result<Option<(u16, u16, u16)>>>()
        + size_of::<[Option<RowSourceBinding>; 9]>()
        + size_of::<[RowSourceRole; 9]>()
        + size_of::<[(Option<usize>, RowPageRole); 5]>()
        + size_of::<std::result::Result<WalPrefix, WalIoError>>()
        + size_of::<Result<u64>>()
        + size_of::<Result<Option<UndoRef>>>()
        + size_of::<RowProducerContext>()
        + size_of::<[usize; 8]>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closures' captures and the
        // before images.
        + 16 * reference;
    // One cell: the page decode beside its range.
    let cell = size_of::<std::ops::Range<usize>>()
        + size_of::<Option<std::ops::Range<usize>>>()
        + 2 * size_of::<std::result::Result<u16, std::num::TryFromIntError>>()
        + 4 * reference
        + undo_decode_source_bytes();
    let leaf_free = size_of::<Cause<u16>>()
        + 2 * size_of::<usize>()
        + 4 * reference
        + row_group::tree_source_bytes();
    let lane_context = context_frame
        + targets
            .max(leaf_free)
            .max(row_validation_lane_body::<W>(0))
            .max(job3::source_bytes())
            .max(scan)
            .max(cell);
    let lane_control = size_of::<Option<usize>>()
        + size_of::<Result<RowControlObservation<'_>>>()
        + size_of::<RetentionError>()
        + 6 * reference;
    let producer_lane = lane_shell + lane_context.max(lane_control);

    // prepare.
    let prepare_shell = shell
        + size_of::<(RowTableSource, RowIssued)>()
        + 2 * size_of::<Result<RowPreparedCut>>()
        + 2 * size_of::<RetentionError>()
        + 8 * reference;
    // `reserved_fatal`: the ReservedRow it takes, the cleanup and the merge.
    let fatal = size_of::<Res<'_, W>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 4 * reference;
    let prepare_arms = fatal;
    let prepare_frame = size_of::<RowTableSource>()
        + size_of::<RowIssued>()
        + size_of::<UnwindFence<'_>>()
        + size_of::<ReservedEntry>()
        + size_of::<Result<ReservedEntry>>()
        + size_of::<ForwardLanes<'_>>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<Result<RowPreparedCut>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, five arguments and storage.
        + 7 * reference;
    let prepare_held = ROW_WRITE_SUSPENDED_LEVELS
        * (size_of::<std::result::Result<row_group::Observation, row_semantic::Error>>()
            + size_of::<row_group::Observation>());
    let prepare_group = size_of::<RowTableSource>()
        + size_of::<crate::storage::control::Profile>()
        + size_of::<Result<(usize, [u8; 5])>>()
        + size_of::<(usize, [u8; 5])>()
        + size_of::<ForwardLanes<'_>>()
        + 2 * size_of::<Result<Plan4Target>>()
        + size_of::<[TargetRequest; ROW_WRITE_TARGETS]>()
        + size_of::<[Option<FrameStamp>; ROW_WRITE_TARGETS]>()
        + size_of::<Cause<usize>>()
        + size_of::<[u8; ROW_WRITE_TARGETS]>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<[u8; 32]>()
        + size_of::<TargetRequest>()
        + size_of::<TableDefinitionView<'_>>()
        + size_of::<Cause<TableDefinitionView<'_>>>()
        + size_of::<FrameExpectation>()
        + size_of::<Option<usize>>()
        + size_of::<Result<usize>>()
        + size_of::<row_group::Observation>()
        + size_of::<(u64, u64)>()
        + size_of::<Result<(u64, u64)>>()
        + size_of::<RowPreparedCut>()
        + 6 * size_of::<usize>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closures' captures, the Aging
        // row, before, after, frame bytes, order and root.
        + 20 * reference;
    let observe = size_of::<row_group::Inputs<'_>>()
        + size_of::<&mut [u8; ROW_REPLAY_SCRATCH_BYTES]>()
        + row_group::observe_borrowed_source_bytes();
    // The table record and its schema decode (`table_definition`).
    let schema = catalog_source_bytes::<W>()
        + 2 * size_of::<Cause<&[u8]>>()
        + 2 * size_of::<std::result::Result<CatalogRecordView<'_>, CatalogError>>();
    let prepare_calls = targets
        .max(roles)
        .max(recompute)
        .max(digest)
        .max(schema)
        .max(observe)
        .max(identity)
        .max(inventory);
    let prepare = prepare_shell
        + (prepare_frame + entry.max(prepare_held + prepare_group + prepare_calls))
            .max(prepare_arms);

    // apply.
    let apply_shell = shell
        + size_of::<(RowTableSource, RowIssued)>()
        + size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 8 * reference;
    // The success arm: the ReservedRow moved out of the PreparedRow, the
    // observation and the in-place release with its Results and the loan.
    let discharged = size_of::<Res<'_, W>>()
        + size_of::<RowMutationObservation>()
        + 2 * size_of::<Result<Loan<'_>>>()
        + 3 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 6 * reference;
    let apply_arms = discharged.max(fatal);
    let apply_frame = size_of::<RowTableSource>()
        + size_of::<RowIssued>()
        + size_of::<UnwindFence<'_>>()
        + size_of::<ReservedEntry>()
        + size_of::<Result<ReservedEntry>>()
        + 3 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, five arguments and storage.
        + 7 * reference;
    let apply_held = ROW_WRITE_SUSPENDED_LEVELS * size_of::<Result<Group<'_>>>();
    let apply_group = size_of::<RowTableSource>()
        + size_of::<ForwardLanes<'_>>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<ExistingGroup<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, five arguments and the prepared reference.
        + 7 * reference;
    let reassemble_frame = size_of::<crate::storage::control::Profile>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<[TargetRequest; ROW_WRITE_TARGETS]>()
        + size_of::<[Option<FrameStamp>; ROW_WRITE_TARGETS]>()
        + size_of::<Cause<usize>>()
        + size_of::<[u8; ROW_WRITE_TARGETS]>()
        + size_of::<Result<()>>()
        + size_of::<Option<[u8; 5]>>()
        + size_of::<Option<&[u8]>>()
        + 2 * size_of::<[u8; 32]>()
        + size_of::<FrameExpectation>()
        + size_of::<row_semantic::LayoutView<'_>>()
        + size_of::<std::result::Result<row_semantic::LayoutView<'_>, row_semantic::Error>>()
        + size_of::<WalFrameHeader>()
        + size_of::<row_group::Observation>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage and the closures' captures.
        + 14 * reference;
    let reassemble = reassemble_frame
        + targets
            .max(roles)
            .max(recompute)
            .max(digest)
            .max(row_group::layout_source_bytes())
            .max(assembled);
    let register = register + 2 * size_of::<Result<()>>();
    let apply = apply_shell
        + (apply_frame
            + entry.max(apply_held + apply_group + reassemble.max(register).max(install)))
        .max(apply_arms);
    RowForwardBody {
        recompute,
        entry,
        lane_shell,
        lane_context,
        lane_control,
        producer_lane,
        prepare_shell,
        prepare_arms,
        prepare_frame,
        prepare_held,
        prepare_group,
        observe,
        prepare_calls,
        prepare,
        apply_shell,
        apply_arms,
        apply_frame,
        apply_held,
        apply_group,
        reassemble,
        register,
        install,
        apply,
    }
}
