//! RowReserve, the first effect of a Row write (S2-0 Row write amendment §1
//! "order" and §6, S2-0b amendment `#row-write-validate-once`), and the
//! context `validation_lane` rebuilds (S2-b decision A7). Its caller,
//! `PreflightRow::reserve`, holds the terminal read gate, the RowWrite
//! admission and the flush lease of the row; this body opens no replay scope
//! and inherits preflight's one `history::validate_live` through the digest
//! chain. In order:
//! 1. the deep checks of an effect step (entry, view, actor, head, Terminal,
//!    ATT slot, table slot);
//! 2. under one typed read of the Catalog leaf and T, with no copy, the
//!    preflight commitment again through `PreflightSource::commitment`; a
//!    difference is Before(Stale);
//! 3. A's candidate against the plan and the RowLeaf free bytes (S2W-R04);
//! 4. C's RowReserve producer over the before images that commitment proved
//!    equal to the pool (Free Undo reservation, RootJob lease, issued RowId,
//!    sequence and mutation), the changed-target identity, the XOI successor
//!    and the progress room;
//! 5. the table write bit, then the one existing-group install (stage, pages,
//!    control through `history::row::record`, durable wait);
//! 6. the draft commitment over a typed read of T after the install, which
//!    refreshes the before images for the next step.
//!
//! Every refusal before the stage is Unchanged, so the wrapper returns the
//! same PreflightRow; every failure after the install is Fatal.
//!
//! One suspended caller level holds a Row group return carrier while the
//! group is prepared (S2-0b decision 2): `reserve_group`'s `group`, the slot
//! of `prepare_reserve`'s result, held through the install. No helper between
//! them returns the group or an Observation, and the entry checks and the
//! draft run beside `reserve_group`, not under it; the body census
//! (`row_reserve_body`) names that level (`RESERVE_SUSPENDED_LEVEL`).
use super::super::super::super::super::row::{RowDraftCut, RowPreflightCut, RowRoleMap};
use super::super::super::row_preflight::{
    FreeUndo, PreflightSource, RowTableSource, fold_identity, fold_prefix, fold_stamp, free_slot0,
    table_index_count, table_value,
};
use super::*;
use crate::storage::{
    catalog_record::{
        self, CatalogDefinitionView, CatalogError, CatalogExpectation, TableDefinitionView,
    },
    control::Profile,
    dirty_frame,
    overflow::INLINE_ROW_BYTES,
    retention_page::JobSlotRef,
    row_record::{ROW_HEADER_BYTES, RowSchemaInput},
    tree_page::TreeExpectation,
    undo_ref::UndoRef,
};
use foundation::{codec::ContentHasher, identity::RowId};

#[path = "row_cancel.rs"]
pub(in super::super::super::super::super) mod cancel;

/// The smallest canonical row the RowReserve producer accepts (its
/// `5..=INLINE_ROW_BYTES` bound); a plan outside it is a forged candidate.
const MIN_CANONICAL: usize = 5;

#[cfg(test)]
#[path = "row_write_tests.rs"]
mod tests;

#[path = "row_forward.rs"]
pub(in super::super::super::super::super) mod forward;

/// What a completed RowReserve hands `PreflightRow::reserve`: the issued
/// identity of the one `RowSlotReservation` and the draft cut. Scalars only;
/// no pin, loan or authority.
pub(in super::super::super::super::super) struct RowReserved {
    pub(in super::super::super::super::super) actor: TxnRef,
    pub(in super::super::super::super::super) root_job: JobSlotRef,
    pub(in super::super::super::super::super) row: RowId,
    pub(in super::super::super::super::super) undo: UndoRef,
    pub(in super::super::super::super::super) lsn: u64,
    pub(in super::super::super::super::super) draft: RowDraftCut,
}

/// The deep checks' scalars an effect step carries into its preparation.
#[derive(Clone, Copy)]
struct ReserveEntry {
    owner: OwnerIdentity,
    prefix: WalPrefix,
    actor: TxnRef,
    sequence: u64,
    terminal: usize,
    slot: usize,
}

/// What the installed RowReserve hands the draft step: the producer's issued
/// values, the RootJob ordinal and the group's LSN.
#[derive(Clone, Copy)]
struct ReserveInstalled {
    issued: row_group::Issued,
    ordinal: usize,
    lsn: u64,
}

/// What `reserve_sources` hands the producer step besides T and its stamps:
/// |T|, the group roles and the table definition.
struct ReserveSources<'x> {
    count: usize,
    roles: [u8; ROW_WRITE_TARGETS],
    definition: TableDefinitionView<'x>,
}

/// The private ranges RowReserve uses (`RowLoanLayout`): the before images
/// (read, then refreshed after the install), A's candidate and the Catalog
/// leaf copy (read only), and the group's outputs. A scratch is not touched.
struct ReserveLanes<'l> {
    before: &'l mut [[u8; PAGE_BYTES]; 9],
    candidate: &'l [u8; PAGE_BYTES],
    schema: &'l [u8; PAGE_BYTES],
    output: ReserveOutput<'l>,
}

/// The ranges the prepared group borrows: the frame, the five after images,
/// the XOI projection and C's producer scratch.
struct ReserveOutput<'i> {
    frame: &'i mut [u8],
    after: &'i mut [[u8; PAGE_BYTES]; 5],
    inventory: &'i mut [u8; PAGE_BYTES],
    scratch: &'i mut [u8; ROW_REPLAY_SCRATCH_BYTES],
}

impl<'l> ReserveLanes<'l> {
    fn split(private: &'l mut [u8; 425_984]) -> Self {
        let (frame, rest) = private.split_at_mut(RowLoanLayout::FRAME.end);
        let (before, rest) = rest.split_at_mut(range_len(RowLoanLayout::BEFORE));
        let (after, rest) = rest.split_at_mut(range_len(RowLoanLayout::AFTER));
        let (candidate, rest) = rest.split_at_mut(PAGE_BYTES);
        let (schema, rest) = rest.split_at_mut(PAGE_BYTES);
        let (inventory, rest) = rest.split_at_mut(PAGE_BYTES);
        let (scratch, _) = rest.split_at_mut(ROW_REPLAY_SCRATCH_BYTES);
        Self {
            before: before
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("nine before images"),
            candidate: (&*candidate).try_into().expect("candidate page"),
            schema: (&*schema).try_into().expect("schema copy page"),
            output: ReserveOutput {
                frame,
                after: after
                    .as_chunks_mut::<PAGE_BYTES>()
                    .0
                    .try_into()
                    .expect("five after images"),
                inventory: inventory.try_into().expect("XOI page"),
                scratch: scratch.try_into().expect("C scratch"),
            },
        }
    }
}

/// Before(Stale): a source the commitment or a recheck found changed since
/// preflight (refusal table, S2-0b supplement). The cause is the frame
/// layer's own stale-source cause; nothing was staged.
fn stale() -> RetentionError {
    let error = dirty_frame::DirtyError {
        cause: dirty_frame::Cause::Stale,
        effect: Effect::Unchanged,
    };
    frame(RetentionPhase::Prepare, error)
}

/// A typed read of a write target after a Ready preflight. A target that is
/// not resident is Fatal(Invariant): under the gate, the lease and the
/// exclusive port no path evicts it (S2-0 §1 "preflight Ready 뒤의
/// 비상주"). Any other cause keeps its own effect.
fn resident_read<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    phase: RetentionPhase,
    error: dirty_frame::DirtyError,
) -> RetentionError {
    if error.cause == dirty_frame::Cause::MissingTarget {
        fatal(storage, phase, RetentionCause::Invariant)
    } else {
        frame(phase, error)
    }
}

/// C's decode of a page it produced (an after image) or of a before image the
/// commitment proved, before the stage: a failure is C's defect, not a
/// refusal, so it is Fatal with its own cause, as the deep checks' decodes are
/// (S2-b2 review A5, 5767b2e). The shared XOI and progress-room helpers report
/// such a decode as an Unchanged `Retention` cause; only the Row write path
/// maps it here, so the Catalog and Row lifecycle callers keep their mapping.
/// Their Capacity, Exhausted and `Base` refusals stay as they are.
fn produced<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    error: RetentionError,
) -> RetentionError {
    match error.cause {
        RetentionCause::Retention(_) if error.effect == Effect::Unchanged => {
            fatal(storage, RetentionPhase::Prepare, error.cause)
        }
        _ => error,
    }
}

/// C's own pre-stage self-check of its producer's or observer's output, in
/// every Row write step (reserve, the Forward `prepare`, the Cancel): a
/// changed role outside T, an after image whose identity is not its T
/// target, a frame length past the frame range, or an observation whose
/// scalars are not the step's. None depends on A's input or on the state;
/// each contradicts C's own proof, so it is Fatal(Prepare, Invariant) as C's
/// decode of its output is (`produced`, S2-b2 review A5), and the fence
/// stays (S2-c review A4). The S2-0 refusal table has no row for it: its
/// "prepare·apply·cancel의 stage 전 거절 → Before(같은 wrapper)" row is read as
/// a refusal of A's input or of the state, and its own-defect rows (an
/// arena or RootReserved mismatch, Invariant) are Fatal.
fn own_output<W: WriterNamespace>(storage: &MutableStorageOwner<'_, '_, W>) -> RetentionError {
    fatal(storage, RetentionPhase::Prepare, RetentionCause::Invariant)
}

/// The before position of a logical source role in a role map.
fn role_position(roles: RowRoleMap, role: RowSourceRole) -> Option<usize> {
    let at = (roles >> (4 * role.index())) & 0xF;
    (at != 0xF).then_some(at as usize)
}

/// T in before order into `targets`: the role map's count and the before
/// images' headers (S2-b decision A7). Preflight copied T there in rank
/// order and nothing writes the range until the draft step refreshes it with
/// the same pages, so a count, a header or an order that does not hold is a
/// contradiction. Positions from the count on are left as they are. Returns
/// the count.
fn before_targets(
    profile: Profile,
    before: &[[u8; PAGE_BYTES]; 9],
    roles: RowRoleMap,
    targets: &mut [TargetRequest; 9],
) -> std::result::Result<usize, RetentionCause> {
    let count = usize::try_from(roles >> 36).map_err(|_| RetentionCause::Invariant)?;
    if !(7..=9).contains(&count) {
        return Err(RetentionCause::Invariant);
    }
    for at in 0..count {
        targets[at] = recovery::request_from_image(profile, &before[at])?;
        if at > 0 && target_rank(targets[at - 1]) >= target_rank(targets[at]) {
            return Err(RetentionCause::Invariant);
        }
    }
    Ok(count)
}

/// The issued Table's definition in the Catalog leaf copy (`table_value`
/// and its record decode), for the producer's schema.
fn table_definition<'x>(
    schema: &'x [u8; PAGE_BYTES],
    identity: PageIdentity,
    table: RowTableSource,
    namespace: crate::api::Namespace,
) -> std::result::Result<TableDefinitionView<'x>, RetentionCause> {
    let value = table_value(schema, identity, table, namespace)?;
    let record = catalog_record::decode_catalog_record(
        value,
        CatalogExpectation {
            namespace,
            object: table.handle.object,
        },
    )
    .map_err(RetentionCause::Catalog)?;
    match record.definition() {
        CatalogDefinitionView::Table(definition) => Ok(definition),
        CatalogDefinitionView::Index(_) => Err(RetentionCause::Invariant),
    }
}

/// The group role of each position of T, from the role map (one nibble per
/// `RowSourceRole`, in its order): the producer's output role for Root,
/// RootJob, Status, Undo and RowLeaf, the allocator's proof role, and the
/// proof role for the TerminalJob page (when it is not the RootJob page), the
/// table anchor and the head's Undo. None if a role is out of T, two
/// producer roles share a page, or a role RowReserve needs is absent.
fn row_write_roles(map: u64, count: usize) -> Option<[u8; ROW_WRITE_TARGETS]> {
    // By `RowSourceRole` index.
    const GROUP: [u8; 9] = [
        ROOT,
        JOB,
        PROOF_ROLE,
        STATUS,
        row_group::UNDO as u8,
        row_group::LEAF as u8,
        ALLOCATOR_ROLE,
        PROOF_ROLE,
        PROOF_ROLE,
    ];
    // Roles RowReserve needs: all but the TerminalJob (it may share the
    // RootJob page) and the head's Undo (an actor with no head has none).
    const REQUIRED: [usize; 7] = [0, 1, 3, 4, 5, 6, 7];
    let mut roles = [PROOF_ROLE; ROW_WRITE_TARGETS];
    let mut named = 0u16;
    // Proof roles first, so a producer role on the same page replaces one.
    for index in [2, 7, 8, 0, 1, 3, 4, 5, 6] {
        let at = (map >> (4 * index)) & 0xF;
        if at == 0xF {
            if REQUIRED.contains(&index) {
                return None;
            }
            continue;
        }
        let at = usize::try_from(at).ok().filter(|at| *at < count)?;
        if GROUP[index] != PROOF_ROLE {
            if named & (1 << at) != 0 {
                return None;
            }
            named |= 1 << at;
        }
        roles[at] = GROUP[index];
    }
    Some(roles)
}

/// A's candidate against its untrusted plan (S2W-R04, §6.6 "reserve"): the
/// issued table's SchemaEpoch, a length the producer accepts and the SHA-256
/// of the candidate range, else InvalidInput; then the RowLeaf free bytes of
/// the cut by A's own rule (four cell-header bytes, eight key bytes, the row
/// header and a slot), else Capacity. Both are Before.
fn check_candidate(
    input: &RowInputPlan,
    candidate: &[u8; PAGE_BYTES],
    table: RowTableSource,
    cut: &RowPreflightCut,
) -> Result<()> {
    let length = input.encoded_len;
    if input.schema_epoch != table.handle.schema_epoch
        || !(MIN_CANONICAL..=INLINE_ROW_BYTES).contains(&length)
        || content_digest(&candidate[..length]) != input.digest
    {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::RowSemantic(row_semantic::Error::InvalidInput),
        ));
    }
    if length + ROW_HEADER_BYTES + 16 > usize::from(cut.leaf_free) {
        return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
    }
    Ok(())
}

/// The issued identity a draft commitment folds.
#[derive(Clone, Copy)]
pub(super) struct DraftIssued {
    pub(super) actor: TxnRef,
    pub(super) root_job: JobSlotRef,
    pub(super) row: RowId,
    pub(super) undo: UndoRef,
    pub(super) sequence: u64,
    pub(super) mutation: u64,
}

/// The values the draft commitment folds (`RowDraftCut::source`, kind 2), in
/// the order its doc gives. `pages` yields T's pages in before order from a
/// typed read of the pool after the last installed group (never the after
/// images in the private loan), so `prepare`, `apply` and `cancel`, which
/// read the pool, recompute it with this same body through one helper
/// (`draft_recompute`, S2-c and S2-d).
pub(super) struct DraftSource<'x, P> {
    pub(super) prefix: WalPrefix,
    pub(super) targets: &'x [TargetRequest],
    pub(super) stamps: &'x [FrameStamp],
    pub(super) roles: RowRoleMap,
    pub(super) issued: DraftIssued,
    pub(super) transaction: &'x [u8; 1536],
    pub(super) aging: Option<&'x [u8; 1536]>,
    pub(super) jobs: &'x [[u8; JOB_BYTES]; JOB_COUNT],
    pub(super) pages: P,
    pub(super) control: ControlCut,
}
impl<'x, P: Iterator<Item = &'x [u8; PAGE_BYTES]>> DraftSource<'x, P> {
    /// Kind 2 of the XRWC commitment: the prefix and T as kind 1 folds them;
    /// the role map; the actor (48 B), the RootJob slot (40 B), the RowId,
    /// the UndoRef (40 B) and its sequence, the planned mutation; the actor
    /// descriptor, then a presence byte and the Aging descriptor; the 64 Jobs
    /// rows; the page digests of T; the `ControlCut` fields.
    pub(super) fn commitment(self) -> std::result::Result<[u8; 32], RetentionCause> {
        let mut hasher = ContentHasher::new();
        hasher.update(b"XRWC");
        hasher.update(&[1, 2]);
        fold_prefix(&mut hasher, self.prefix);
        for (request, stamp) in self.targets.iter().zip(self.stamps) {
            fold_identity(&mut hasher, request.identity);
            hasher.update(&[request.kind as u8, request.level]);
            fold_stamp(&mut hasher, stamp);
        }
        hasher.update(&self.roles.to_le_bytes());
        let issued = self.issued;
        let mut actor = [0; 48];
        issued
            .actor
            .encode(&mut actor)
            .map_err(RetentionCause::Page)?;
        hasher.update(&actor);
        let mut reference = [0; 40];
        issued
            .root_job
            .encode(&mut reference)
            .map_err(RetentionCause::Retention)?;
        hasher.update(&reference);
        hasher.update(&issued.row.get().to_le_bytes());
        issued
            .undo
            .encode(&mut reference)
            .map_err(RetentionCause::Page)?;
        hasher.update(&reference);
        hasher.update(&issued.sequence.to_le_bytes());
        hasher.update(&issued.mutation.to_le_bytes());
        hasher.update(self.transaction);
        match self.aging {
            None => hasher.update(&[0]),
            Some(aging) => {
                hasher.update(&[1]);
                hasher.update(aging);
            }
        }
        hasher.update(self.jobs.as_flattened());
        for page in self.pages {
            hasher.update(&content_digest(page));
        }
        hasher.update(&self.control.bank);
        hasher.update(&self.control.jobs);
        hasher.update(&self.control.inventory);
        hasher.update(&self.control.issued.to_le_bytes());
        hasher.update(&self.control.adopted.to_le_bytes());
        Ok(hasher.digest())
    }
}

// S2W-R07, S2W-R21(1): the next reserve recompute or kind-2 recompute
// (`draft_recompute`) on this thread sees T's first stamp with its
// modification counter moved by one, once. No page, control or WAL byte
// changes; the next recompute sees the pool again.
#[cfg(test)]
thread_local! {
    static TEST_STALE_STAMP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(in super::super::super::super::super) fn test_stale_stamp_once()
-> super::super::super::super::super::transaction::writable::TestOnce {
    super::super::super::super::super::transaction::writable::TestOnce::arm(&TEST_STALE_STAMP)
}

// The next Row write install on this thread (reserve's or apply's) is refused
// Unchanged (Stage, Busy) after the table write bit and before the
// existing-group install, once: a pre-stage refusal after the registration
// (S2-b2 review A6, S2W-R11). No page, control or WAL byte changes.
#[cfg(test)]
thread_local! {
    static TEST_STAGE_REFUSAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(in super::super::super::super::super) fn test_stage_refusal_once()
-> super::super::super::super::super::transaction::writable::TestOnce {
    super::super::super::super::super::transaction::writable::TestOnce::arm(&TEST_STAGE_REFUSAL)
}
#[cfg(test)]
fn test_stage_refusal() -> Result<()> {
    if super::super::super::super::super::transaction::writable::TestOnce::take(&TEST_STAGE_REFUSAL)
    {
        return Err(unchanged(RetentionPhase::Stage, RetentionCause::Busy));
    }
    Ok(())
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The RowReserve body (module doc). `cut` is the PreflightRow's; `input`
    /// is A's untrusted plan. An Unchanged error is a Before: nothing was
    /// staged, and only the conservative write bit may be registered. Any
    /// other error is Fatal and the fence stays. The entry checks, the group
    /// and the draft are three calls one after another, so the group's
    /// return slot (`reserve_group`) is not held across the deep checks or
    /// the draft fold.
    pub(in super::super::super::super::super) fn row_reserve(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        (table, cut, input): (RowTableSource, &RowPreflightCut, &RowInputPlan),
        private: &mut [u8; 425_984],
    ) -> Result<RowReserved> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let entry = self.reserve_entry(owner, statement, table, cut)?;
        let installed =
            self.reserve_group(owner, statement, &entry, (table, cut, input), private)?;
        self.reserve_draft(owner, &entry, (cut, input), installed, private)
    }

    /// Steps 2 to 5 of the module doc: the group preparation, the table
    /// write bit and the install. The one suspended level: `group` is the
    /// slot of `prepare_reserve`'s result, matched by reference so that the
    /// group is never moved into a second slot, and it is held through the
    /// install. Returns the issued values the draft needs.
    fn reserve_group(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        entry: &ReserveEntry,
        (table, cut, input): (RowTableSource, &RowPreflightCut, &RowInputPlan),
        private: &mut [u8; 425_984],
    ) -> Result<ReserveInstalled> {
        let ReserveLanes {
            before,
            candidate,
            schema,
            output,
        } = ReserveLanes::split(private);
        let group = self.catalog_preparation().prepare_reserve(
            owner,
            entry,
            (table, cut, input),
            (before, candidate, schema),
            output,
        );
        let prepared = match group {
            Ok(ref prepared) => prepared,
            Err(error) => return Err(error),
        };
        // The producer's own Reserve output: its issued values, its RootJob
        // ordinal, which is the cut's RootJob (preflight chose it by the
        // producer's rule over the Root and Jobs the commitment proved), and
        // the caller's actor (S2-b2 review A2).
        let observation = &prepared.payload.observation;
        let installed = match (observation.issued, observation.job_ordinal) {
            (Some(issued), Some(ordinal))
                if observation.actor == Some(entry.actor)
                    && ordinal == usize::from(cut.root_job) =>
            {
                ReserveInstalled {
                    issued,
                    ordinal,
                    lsn: observation.header.lsn,
                }
            }
            _ => return Err(own_output(self.storage)),
        };
        // The table write bit, after the table recheck and before the stage
        // (§6): a Before after it keeps the conservative bit.
        statement.record_table_write(table.slot)?;
        #[cfg(test)]
        test_stage_refusal()?;
        self.catalog_application()
            .apply_existing(owner, ExistingGroup::RowWrite(prepared))?;
        Ok(installed)
    }

    /// The deep checks of an effect step, in preflight's order: the entry of
    /// an existing-group preparation (health, idle owner, WAL prefix, whole
    /// ledger), the view and its statement, the actor's head (a Reserved or
    /// Aborting actor is Unchanged WrongState, a Job of the actor without the
    /// Row recipe Fatal), its Terminal and ATT slot, and the table's epoch
    /// slot. A complete LSN other than the cut's is Before(Stale) without
    /// rehashing.
    fn reserve_entry(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        table: RowTableSource,
        cut: &RowPreflightCut,
    ) -> Result<ReserveEntry> {
        let preparation = self.catalog_preparation();
        let (identity, prefix) = preparation.existing_entry(owner)?;
        statement.check_catalog_read()?;
        let actor = statement
            .own_actor()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        let sequence = statement.row_sequence()?;
        self.row_statement_head(actor, RetentionPhase::Prepare)?;
        let (terminal, _) = preparation.row_terminal(actor)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        self.check_table_slot(table)?;
        if prefix.complete_lsn != cut.complete_lsn {
            return Err(stale());
        }
        Ok(ReserveEntry {
            owner: identity,
            prefix,
            actor,
            sequence,
            terminal,
            slot,
        })
    }

    /// After the install: one typed read of T copies each pool page into the
    /// before range (the next step's before images) and folds the draft
    /// commitment over those pages and the control. Every failure here is
    /// Fatal(Install): the group is durable (refusal table, S2-0b).
    fn reserve_draft(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &ReserveEntry,
        (cut, input): (&RowPreflightCut, &RowInputPlan),
        installed: ReserveInstalled,
        private: &mut [u8; 425_984],
    ) -> Result<RowReserved> {
        let storage = self.storage;
        let profile = storage.profile();
        let installed_fatal = |cause| fatal(storage, RetentionPhase::Install, cause);
        let ReserveInstalled {
            issued,
            ordinal,
            lsn,
        } = installed;
        let job = job3::decode_job(&self.control.jobs()[ordinal], profile.spec().namespace)
            .map_err(|error| installed_fatal(RetentionCause::Retention(error)))?;
        let index =
            u8::try_from(ordinal / 15).map_err(|_| installed_fatal(RetentionCause::Invariant))?;
        let page = target_from_source(storage, Plan4Page::Jobs { index })
            .map_err(|error| installed_fatal(error.cause))?
            .request
            .identity
            .page;
        let slot =
            u16::try_from(ordinal % 15).map_err(|_| installed_fatal(RetentionCause::Invariant))?;
        let root_job = JobSlotRef::new(page, slot, job.generation())
            .map_err(|error| installed_fatal(RetentionCause::Retention(error)))?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|error| installed_fatal(RetentionCause::Wal(error)))?;
        if prefix.complete_lsn != lsn {
            return Err(installed_fatal(RetentionCause::Invariant));
        }
        let before: &mut [[u8; PAGE_BYTES]; 9] = private[RowLoanLayout::BEFORE]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("nine before images");
        // The Catalog leaf request only fills the positions past |T|.
        let filler = target_from_source(storage, Plan4Page::CatalogLeaf)
            .map_err(|error| installed_fatal(error.cause))?
            .request;
        let mut targets = [filler; 9];
        let count =
            before_targets(profile, before, cut.roles, &mut targets).map_err(installed_fatal)?;
        let issued = DraftIssued {
            actor: entry.actor,
            root_job,
            row: issued.row,
            undo: issued.undo,
            sequence: issued.sequence,
            mutation: issued.mutation,
        };
        let source = match count {
            7 => self.draft_fold::<7>(owner, (prefix, entry.slot), &targets, (cut, issued), before),
            8 => self.draft_fold::<8>(owner, (prefix, entry.slot), &targets, (cut, issued), before),
            9 => self.draft_fold::<9>(owner, (prefix, entry.slot), &targets, (cut, issued), before),
            _ => Err(installed_fatal(RetentionCause::Invariant)),
        }?;
        Ok(RowReserved {
            actor: entry.actor,
            root_job,
            row: issued.row,
            undo: issued.undo,
            lsn,
            draft: RowDraftCut {
                source,
                input: input.digest,
                complete_lsn: prefix.complete_lsn,
                planned_mutation: issued.mutation,
                roles: cut.roles,
                input_len: u16::try_from(input.encoded_len)
                    .map_err(|_| installed_fatal(RetentionCause::Invariant))?,
                operation: 0,
            },
        })
    }

    /// The draft fold under one typed read of T (`N` = |T|).
    fn draft_fold<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        (prefix, slot): (WalPrefix, usize),
        targets: &[TargetRequest; 9],
        (cut, issued): (&RowPreflightCut, DraftIssued),
        before: &mut [[u8; PAGE_BYTES]; 9],
    ) -> Result<[u8; 32]> {
        let storage = self.storage;
        let installed = |cause| fatal(storage, RetentionPhase::Install, cause);
        let requests: [TargetRequest; N] = std::array::from_fn(|at| targets[at]);
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &requests)
            .map_err(|error| installed(RetentionCause::Frame(error)))?;
        let folded = (|| {
            let first = read
                .copy_snapshot(0, &mut before[0])
                .map_err(|error| installed(RetentionCause::Frame(error)))?;
            let mut stamps = [first; N];
            for (at, stamp) in stamps.iter_mut().enumerate().skip(1) {
                *stamp = read
                    .copy_snapshot(at, &mut before[at])
                    .map_err(|error| installed(RetentionCause::Frame(error)))?;
            }
            let aging = self.control.row(control::AGING_SLOT);
            DraftSource {
                prefix,
                targets: &requests,
                stamps: &stamps,
                roles: cut.roles,
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
            .map_err(installed)
        })();
        merge(
            folded,
            read.release_checked()
                .map_err(|error| installed(RetentionCause::Frame(error))),
        )
    }

    /// `validation_lane`'s context (S2-b decision A7, `RowPreflightCut`),
    /// rebuilt from the private copies preflight left: the schema cell and the
    /// index count from the Catalog leaf copy in the schema range, the leaf
    /// expectation from the role map and the RowLeaf before header, the free
    /// bytes from the cut, and the Catalog leaf stamp observed again under the
    /// same gate and lease. It computes no page or control digest and checks
    /// nothing against the cut's commitment: `reserve`'s recompute refuses any
    /// change before an effect. A copy that does not decode contradicts
    /// preflight's own checks of it (Fatal); nothing is written. Returns the
    /// context and the RowLeaf's before position. `roles` and `leaf_free` are
    /// the preflight cut's, or the draft's role map and the RowLeaf free
    /// bytes of its before image for the producer lane (S2-c), which rebuilds
    /// the same context from the same copies.
    pub(in super::super::super::super::super) fn row_validation_context(
        &self,
        owner: &mut Owner<'c, 's>,
        table: RowTableSource,
        (roles, leaf_free): (RowRoleMap, u16),
        private: &[u8; 425_984],
    ) -> Result<(RowPreflightContext, usize)> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let profile = storage.profile();
        let namespace = profile.spec().namespace;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let catalog = target_from_source(storage, Plan4Page::CatalogLeaf)?.request;
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[catalog])
            .map_err(|error| resident_read(storage, RetentionPhase::Prepare, error))?;
        let stamp = read
            .observe(0)
            .map(|observed| observed.frame)
            .map_err(|error| frame(RetentionPhase::Prepare, error));
        let stamp = merge(
            stamp,
            read.release_checked()
                .map_err(|error| frame(RetentionPhase::Prepare, error)),
        )?;
        let schema: &[u8; PAGE_BYTES] = private[RowLoanLayout::SCHEMA]
            .try_into()
            .expect("schema copy page");
        let value =
            table_value(schema, catalog.identity, table, namespace).map_err(|_| invariant())?;
        let offset = (value.as_ptr() as usize)
            .checked_sub(schema.as_ptr() as usize)
            .and_then(|offset| u16::try_from(offset).ok())
            .ok_or_else(invariant)?;
        let length = u16::try_from(value.len()).map_err(|_| invariant())?;
        let index_count = table_index_count(schema, catalog.identity, table, namespace)
            .map_err(|_| invariant())?;
        let count = usize::try_from(roles >> 36).map_err(|_| invariant())?;
        let leaf_at = role_position(roles, RowSourceRole::RowLeaf)
            .filter(|at| *at < count && count <= 9)
            .ok_or_else(invariant)?;
        let start = RowLoanLayout::BEFORE.start + leaf_at * PAGE_BYTES;
        let leaf = recovery::request_from_image(
            profile,
            private[start..start + PAGE_BYTES]
                .try_into()
                .expect("RowLeaf before image"),
        )
        .map_err(|_| invariant())?;
        if leaf.kind != PageKind::RowLeaf || leaf.level != 0 {
            return Err(invariant());
        }
        let context = RowPreflightContext {
            profile,
            table: table.handle,
            catalog_expected: CatalogExpectation {
                namespace,
                object: table.handle.object,
            },
            schema_cell: (offset, length),
            leaf_expected: TreeExpectation {
                identity: leaf.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
            leaf_free_bytes: usize::from(leaf_free),
            canonical_capacity: INLINE_ROW_BYTES,
            index_count,
            catalog_source: (stamp, content_digest(value)),
        };
        Ok((context, leaf_at))
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// The RowReserve group over the whole write target set T, steps 2 to 4
    /// of the module doc. Every refusal is Unchanged before any stage, except
    /// a contradiction of preflight's own copies or role map (Fatal Census)
    /// and C's decode or self-check of its own output (Fatal Prepare,
    /// `produced` and `own_output`). T, its stamps and the roles are filled
    /// in place, in the arrays the group keeps, so no by-value array is
    /// carried between the steps.
    fn prepare_reserve<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &ReserveEntry,
        (table, cut, input): (RowTableSource, &RowPreflightCut, &RowInputPlan),
        (before, candidate, schema): (
            &'i [[u8; PAGE_BYTES]; 9],
            &'i [u8; PAGE_BYTES],
            &'i [u8; PAGE_BYTES],
        ),
        output: ReserveOutput<'i>,
    ) -> Result<PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>> {
        let storage = self.storage;
        let profile = storage.profile();
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        // The Catalog leaf request only fills the positions past |T|.
        let mut requests =
            [target_from_source(storage, Plan4Page::CatalogLeaf)?.request; ROW_WRITE_TARGETS];
        let mut stamps = [None; ROW_WRITE_TARGETS];
        let ReserveSources {
            count,
            roles,
            definition,
        } = self.reserve_sources(
            owner,
            entry,
            (table, cut, input),
            (before, candidate, schema),
            (&mut requests, &mut stamps),
        )?;
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
        let ReserveOutput {
            frame,
            after,
            inventory,
            scratch,
        } = output;
        let observation = {
            let undo = at(row_group::UNDO as u8)?;
            let anchor = role_position(cut.roles, RowSourceRole::TableAnchor)
                .filter(|at| *at < count)
                .ok_or_else(invariant)?;
            let plan = row_group::ReservePlan {
                terminal: entry.terminal,
                container: requests[undo].identity.page,
                table: table.handle,
                statement: entry.sequence,
                canonical: &candidate[..input.encoded_len],
            };
            let aging = self.control.row(control::AGING_SLOT);
            let inputs = row_group::Inputs {
                profile,
                frame: expected,
                change: row_group::Change::Reserve(&plan),
                jobs: self.control.jobs(),
                root: &before[at(ROOT)?],
                jobs_page: Some(&before[at(JOB)?]),
                status: Some(&before[at(STATUS)?]),
                undo: Some(&before[undo]),
                previous: None,
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
            match row_group::produce_borrowed(&inputs, after, frame, scratch) {
                Ok(observation) => observation,
                Err(error) => return Err(row_semantic_error(error)),
            }
        };
        let after: &'i [[u8; PAGE_BYTES]; 5] = after;
        let frame: &'i [u8] = frame;
        let order = &observation.order[..usize::from(observation.count)];
        // Every changed page is a T member at that identity (C's own output:
        // `own_output`).
        for role in order {
            let target = requests[at(*role).map_err(|_| own_output(storage))?];
            let image = recovery::request_from_image(profile, &after[usize::from(*role)])
                .map_err(|cause| fatal(storage, RetentionPhase::Prepare, cause))?;
            if image != target {
                return Err(own_output(storage));
            }
        }
        let root = if order.contains(&ROOT) {
            &after[usize::from(ROOT)]
        } else {
            &before[at(ROOT)?]
        };
        let bytes = frame
            .get(..observation.frame_bytes)
            .ok_or_else(|| own_output(storage))?;
        let future = {
            let layout =
                row_semantic::decode_layout(bytes, expected, profile).map_err(|error| {
                    fatal(
                        storage,
                        RetentionPhase::Prepare,
                        RetentionCause::RowSemantic(error),
                    )
                })?;
            // The allocator BEFORE image, whose digest the commitment folds
            // (S2-b1 review A2d); XOI is written here, after it.
            prepare_inventory(
                self.control,
                profile,
                observation.header.lsn,
                GroupRows::row(&observation, Some(entry.slot), layout),
                root,
                &before[at(ALLOCATOR_ROLE)?],
                observation.job_ordinal.map(|_| &after[usize::from(JOB)]),
                inventory,
            )
            .map_err(|error| produced(storage, error))?
        };
        check_progress_room(
            entry.prefix,
            (observation.header.lsn, observation.frame_bytes),
            future,
            (&roles[..count], &stamps[..count]),
            order,
            root,
            profile,
        )
        .map_err(|error| produced(storage, error))?;
        Ok(PreparedRowGroup {
            payload: RowPayload {
                core: self.pool.bound_core(),
                binding: &storage.binding,
                fence: storage.session_fence(),
                prefix: entry.prefix,
                observation,
                slot: Some(entry.slot),
                frame: bytes,
                frame_digest: content_digest(bytes),
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

    /// Steps 2 and 3 of the module doc, before the producer: T in before
    /// order into `requests` and its group roles, the commitment recompute
    /// (T's stamps into `stamps`), A's candidate (S2W-R04) and the table
    /// definition from the Catalog leaf copy, whose record value the
    /// recompute just proved equal to the pool's.
    fn reserve_sources<'x>(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &ReserveEntry,
        (table, cut, input): (RowTableSource, &RowPreflightCut, &RowInputPlan),
        (before, candidate, schema): (
            &[[u8; PAGE_BYTES]; 9],
            &[u8; PAGE_BYTES],
            &'x [u8; PAGE_BYTES],
        ),
        (requests, stamps): (
            &mut [TargetRequest; ROW_WRITE_TARGETS],
            &mut [Option<FrameStamp>; ROW_WRITE_TARGETS],
        ),
    ) -> Result<ReserveSources<'x>> {
        let storage = self.storage;
        let profile = storage.profile();
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let count =
            before_targets(profile, before, cut.roles, requests).map_err(|_| invariant())?;
        let roles = row_write_roles(cut.roles, count).ok_or_else(invariant)?;
        let targets = &requests[..count];
        match count {
            7 => self.reserve_recompute::<8>(owner, entry.prefix, targets, (cut, table), stamps),
            8 => self.reserve_recompute::<9>(owner, entry.prefix, targets, (cut, table), stamps),
            9 => self.reserve_recompute::<10>(owner, entry.prefix, targets, (cut, table), stamps),
            _ => Err(invariant()),
        }?;
        check_candidate(input, candidate, table, cut)?;
        let catalog = target_from_source(storage, Plan4Page::CatalogLeaf)?.request;
        let definition =
            table_definition(schema, catalog.identity, table, profile.spec().namespace)
                .map_err(|_| invariant())?;
        Ok(ReserveSources {
            count,
            roles,
            definition,
        })
    }

    /// The kind-1 recompute (S2-0b "digest chain"): under one typed read of
    /// the Catalog leaf and T (`N` = |T| + 1), with no replay and no copy,
    /// the same `PreflightSource::commitment` over the pool pages (S2-b1
    /// review A2a). The table record digest and the Free Undo's slot0 are
    /// read from the pool; the RootJob, the role map and the free bytes are
    /// the cut's. On equality it writes T's stamps into `stamps` for the
    /// install's recheck. A difference, or a pool page those reads find
    /// changed, is Before(Stale). Neither the schema nor the before range is
    /// written (A2b), so a retry from `validation_lane` rebuilds from
    /// preflight's copies.
    fn reserve_recompute<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        targets: &[TargetRequest],
        (cut, table): (&RowPreflightCut, RowTableSource),
        stamps: &mut [Option<FrameStamp>; ROW_WRITE_TARGETS],
    ) -> Result<()> {
        let storage = self.storage;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let catalog = target_from_source(storage, Plan4Page::CatalogLeaf)?.request;
        let mut requests = [catalog; N];
        requests[1..].copy_from_slice(targets);
        requests.sort_unstable_by_key(|request| target_rank(*request));
        let read = self
            .access
            .read(self.pool, owner, &requests)
            .map_err(|error| resident_read(storage, RetentionPhase::Prepare, error))?;
        let observed = (|| {
            let mut observed = [None; 9];
            let mut pages = [None; 9];
            let mut leaf = None;
            let mut at_before = 0;
            for (at, request) in requests.iter().enumerate() {
                let stamp = read
                    .observe(at)
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?
                    .frame;
                let bytes = read
                    .bytes(at)
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?;
                if *request == catalog {
                    leaf = Some((stamp, bytes));
                    continue;
                }
                if targets.get(at_before) != Some(request) {
                    return Err(invariant());
                }
                observed[at_before] = Some(stamp);
                pages[at_before] = Some(bytes);
                at_before += 1;
            }
            #[cfg(test)]
            if super::super::super::super::super::transaction::writable::TestOnce::take(
                &TEST_STALE_STAMP,
            ) && let Some(stamp) = &mut observed[0]
            {
                stamp.modification_counter = stamp.modification_counter.wrapping_add(1);
            }
            let (leaf_stamp, leaf_bytes) = leaf.ok_or_else(invariant)?;
            let folded: [FrameStamp; 9] =
                std::array::from_fn(|at| observed[at].unwrap_or(leaf_stamp));
            let namespace = storage.profile().spec().namespace;
            let schema = table_value(leaf_bytes, catalog.identity, table, namespace)
                .map(content_digest)
                .map_err(|_| stale())?;
            let undo_at = role_position(cut.roles, RowSourceRole::Undo)
                .filter(|at| *at < targets.len())
                .ok_or_else(invariant)?;
            let undo = targets[undo_at];
            let slot0 = pages[undo_at]
                .and_then(|bytes| free_slot0(bytes, undo.identity))
                .ok_or_else(stale)?;
            let source = PreflightSource {
                prefix,
                targets,
                stamps: &folded[..targets.len()],
                roles: cut.roles,
                undo: FreeUndo {
                    request: undo,
                    slot0,
                },
                root_job: cut.root_job,
                catalog: (leaf_stamp, schema),
                leaf_free: cut.leaf_free,
                pages: pages.iter().flatten().copied(),
                control: ControlCut::capture(self.control),
            }
            .commitment();
            if source != cut.source {
                return Err(stale());
            }
            stamps[..targets.len()].copy_from_slice(&observed[..targets.len()]);
            Ok(())
        })();
        merge(
            observed,
            read.release_checked()
                .map_err(|error| frame(RetentionPhase::Prepare, error)),
        )
    }
}

/// The one suspended caller level of RowReserve that holds a Row group return
/// carrier (S2-0b decision 2), as the census names it.
pub(in super::super::super::super::super) const RESERVE_SUSPENDED_LEVEL: &str =
    "reserve_group.group: Result<PreparedRowGroup<ROW_WRITE_TARGETS>>";

/// Source-bound inventory of the actual RowReserve (`PreflightRow::reserve`
/// and `row_reserve`), in the repository's size_of sense (not a stack
/// measurement), as `RowPreflightBody`: a call's own values are held
/// throughout it and added; the calls it makes one after another are
/// max-combined. The phase's wrapper, input, reservation, draft and result
/// carriers are the Row write census's `carriers` (`row_write_phases`).
#[derive(Clone, Copy, Debug)]
pub(in super::super::super::super::super) struct RowReserveBody {
    /// The S2-a′ shell (admission, gate, their carriers and the unwind
    /// fences) and `PreflightRow::reserve`'s own values.
    pub(in super::super::super::super::super) shell: usize,
    /// `row_reserve`'s own values: arguments, the entry, the installed
    /// values and the Results. It calls `reserve_entry`, `reserve_group` and
    /// `reserve_draft` one after another.
    pub(in super::super::super::super::super) frame: usize,
    /// `reserve_entry`: its own values and the deep checks (preflight's
    /// `check` term).
    pub(in super::super::super::super::super) entry: usize,
    /// The suspended level (`RESERVE_SUSPENDED_LEVEL`) times its return
    /// slot: `reserve_group`'s `group`, held from the preparation through
    /// the install.
    pub(in super::super::super::super::super) held: usize,
    /// `reserve_group`'s own values other than `held`: arguments, lanes,
    /// the installed values, the install's group argument and Results.
    pub(in super::super::super::super::super) group: usize,
    /// `reserve_recompute::<10>`: the typed read of the Catalog leaf and T,
    /// the table record, slot0 and the commitment.
    pub(in super::super::super::super::super) recompute: usize,
    /// `reserve_sources`: its own values and the largest of T, the roles,
    /// `recompute`, the candidate check and the table definition.
    pub(in super::super::super::super::super) sources: usize,
    /// C's RowReserve producer over the before images: Inputs and
    /// `produce_borrowed`.
    pub(in super::super::super::super::super) produce: usize,
    /// `prepare_reserve`: its own values and the largest of its calls
    /// (`sources`, `produce`, the changed-target identity, the shared
    /// inventory/progress term and the assembly).
    pub(in super::super::super::super::super) prepare: usize,
    /// `record_table_write`.
    pub(in super::super::super::super::super) register: usize,
    /// The existing-group install at nine targets (the S2-a′
    /// `group_install`).
    pub(in super::super::super::super::super) install: usize,
    /// `reserve_draft` and its `draft_fold::<9>`.
    pub(in super::super::super::super::super) draft: usize,
    /// `shell + frame + max(entry, held + group + max(prepare, register,
    /// install), draft)`.
    pub(in super::super::super::super::super) body: usize,
}

/// The actual RowReserve census. `check` and `register` are preflight's
/// callee terms of the same calls (`RowPreflightBody`); `install` and
/// `inventory` are the S2-a′ install and shared inventory terms at nine
/// targets, which this body calls unchanged.
pub(in super::super::super) fn row_reserve_body<W: WriterNamespace>(
    shell: usize,
    check: usize,
    register: usize,
    (install, inventory): (usize, usize),
) -> RowReserveBody {
    use super::super::super::super::super::read::{catalog_source_bytes, undo_decode_source_bytes};
    use crate::storage::{
        catalog_record::CatalogRecordView, dirty_frame::DirtyObservation, wal_io::WalIoError,
    };
    use std::mem::size_of;
    type Read<'a, const N: usize> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Group<'a> = PreparedRowGroup<'a, 'a, 'a, ROW_WRITE_TARGETS>;
    type Pages<'a> =
        std::iter::Copied<std::iter::Flatten<std::slice::Iter<'a, Option<&'a [u8; PAGE_BYTES]>>>>;
    type Cause<T> = std::result::Result<T, RetentionCause>;
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
    let shell = shell
        + size_of::<RowTableSource>()
        + 2 * size_of::<Result<RowReserved>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 8 * reference;
    // Each consumed result: the callee's Result and the bound value; the
    // draft's Result is forwarded as the body's.
    let frame = size_of::<RowTableSource>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<ReserveEntry>()
        + size_of::<Result<ReserveEntry>>()
        + size_of::<ReserveInstalled>()
        + size_of::<Result<ReserveInstalled>>()
        + size_of::<Result<RowReserved>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, four arguments and storage.
        + 6 * reference;
    let entry = size_of::<RowTableSource>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Result<u64>>()
        + size_of::<Result<Option<UndoRef>>>()
        + size_of::<Result<(usize, StatusSlotRef)>>()
        + size_of::<Option<usize>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<ReserveEntry>()
        + 2 * size_of::<RetentionError>()
        + 6 * reference
        + check;
    let held = ROW_WRITE_SUSPENDED_LEVELS * size_of::<Result<Group<'_>>>();
    let group = size_of::<RowTableSource>()
        + size_of::<ReserveLanes<'_>>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<ReserveInstalled>()
        + size_of::<ExistingGroup<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, five arguments, the prepared and observation references.
        + 8 * reference;
    let recompute_frame = size_of::<WalPrefix>()
        + size_of::<RowTableSource>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<[TargetRequest; 10]>()
        + 2 * size_of::<Result<()>>()
        + size_of::<[Option<FrameStamp>; 9]>()
        + size_of::<[Option<&[u8; PAGE_BYTES]>; 9]>()
        + size_of::<Option<(FrameStamp, &[u8; PAGE_BYTES])>>()
        + 3 * size_of::<usize>()
        + 2 * size_of::<FrameStamp>()
        + size_of::<dirty_frame::Result<DirtyObservation>>()
        + size_of::<dirty_frame::Result<&[u8; PAGE_BYTES]>>()
        + size_of::<[FrameStamp; 9]>()
        + size_of::<Result<[u8; 32]>>()
        + size_of::<TargetRequest>()
        + size_of::<Option<Generation>>()
        + size_of::<PreflightSource<'_, Pages<'_>>>()
        + size_of::<ControlCut>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<dirty_frame::Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 8 * reference;
    let recompute = recompute_frame
        + targets::call_bytes::<10>().max(
            size_of::<Read<'_, 10>>()
                + (catalog_source_bytes::<W>() + 2 * size_of::<Cause<&[u8]>>())
                    .max(undo_decode_source_bytes())
                    .max(fold),
        );
    let produce = size_of::<row_group::Inputs<'_>>()
        + size_of::<&mut [u8; ROW_REPLAY_SCRATCH_BYTES]>()
        + row_group::produce_borrowed_source_bytes();
    // The table record and its schema decode (`table_definition`).
    let schema = catalog_source_bytes::<W>()
        + 2 * size_of::<Cause<&[u8]>>()
        + 2 * size_of::<std::result::Result<CatalogRecordView<'_>, CatalogError>>();
    // The candidate digest beside its plan.
    let candidate = 2 * size_of::<ContentHasher>() + 2 * size_of::<[u8; 32]>() + 6 * reference;
    let roles = size_of::<Option<[u8; ROW_WRITE_TARGETS]>>() + 6 * size_of::<usize>();
    // The group's assembly as the lifecycle counts it (`row_start`'s
    // `assembled`): the group literal and its Result twice in the assembling
    // call, beside the payload values, the cut capture and the frame digest.
    let assembled = size_of::<Group<'_>>()
        + 2 * size_of::<Result<Group<'_>>>()
        + size_of::<RowPayload<'_, '_, '_>>()
        + fold;
    // `reserve_sources`: its own values, then T, the roles, the recompute,
    // the candidate and the table definition one after another.
    let sources_frame = size_of::<RowTableSource>()
        + size_of::<crate::storage::control::Profile>()
        + size_of::<usize>()
        + size_of::<Cause<usize>>()
        + size_of::<[u8; ROW_WRITE_TARGETS]>()
        + 2 * size_of::<Result<()>>()
        + size_of::<TargetRequest>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<TableDefinitionView<'_>>()
        + size_of::<Cause<TableDefinitionView<'_>>>()
        + size_of::<ReserveSources<'_>>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closure capture and T's slice.
        + 14 * reference;
    let sources = sources_frame + targets.max(roles).max(recompute).max(candidate).max(schema);
    // The produce callee's Result is the produce term's
    // (`produce_borrowed_source_bytes`); the inventory call's Result is the
    // inventory term's.
    let prepare_frame = size_of::<RowTableSource>()
        + size_of::<ReserveOutput<'_>>()
        + size_of::<crate::storage::control::Profile>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<[TargetRequest; ROW_WRITE_TARGETS]>()
        + size_of::<[Option<FrameStamp>; ROW_WRITE_TARGETS]>()
        + size_of::<ReserveSources<'_>>()
        + size_of::<Result<ReserveSources<'_>>>()
        + size_of::<FrameExpectation>()
        + size_of::<row_group::ReservePlan<'_>>()
        + size_of::<row_group::Observation>()
        + size_of::<Option<&[u8]>>()
        + size_of::<(u64, u64)>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<Result<usize>>()
        + 6 * size_of::<usize>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closures' captures, the Aging
        // row, order, root, after and frame.
        + 20 * reference;
    let prepare = prepare_frame
        + sources
            .max(produce)
            .max(identity)
            .max(inventory)
            .max(assembled);
    let register = register + 2 * size_of::<Result<()>>();
    let fold_frame = size_of::<(WalPrefix, usize)>()
        + size_of::<DraftIssued>()
        + size_of::<[TargetRequest; 9]>()
        + 2 * size_of::<Result<[u8; 32]>>()
        + 2 * size_of::<FrameStamp>()
        + size_of::<[FrameStamp; 9]>()
        + size_of::<DraftSource<'_, std::slice::Iter<'_, [u8; PAGE_BYTES]>>>()
        + size_of::<ControlCut>()
        + 2 * size_of::<dirty_frame::Result<FrameStamp>>()
        + size_of::<dirty_frame::Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 8 * reference;
    let draft_fold = fold_frame
        + targets::call_bytes::<9>()
            .max(size_of::<Read<'_, 9>>() + fold.max(size_of::<Cause<[u8; 32]>>()));
    let draft_frame = size_of::<ReserveInstalled>()
        + size_of::<job3::JobView<'_>>()
        + size_of::<
            std::result::Result<job3::JobView<'_>, crate::storage::retention_page::RetentionError>,
        >()
        + 2 * size_of::<Result<Plan4Target>>()
        + size_of::<PageRef>()
        + size_of::<JobSlotRef>()
        + size_of::<std::result::Result<JobSlotRef, crate::storage::retention_page::RetentionError>>(
        )
        + size_of::<WalPrefix>()
        + size_of::<std::result::Result<WalPrefix, WalIoError>>()
        + 2 * size_of::<TargetRequest>()
        + size_of::<[TargetRequest; 9]>()
        + 2 * size_of::<Cause<usize>>()
        + size_of::<DraftIssued>()
        + 2 * size_of::<Result<[u8; 32]>>()
        + size_of::<RowReserved>()
        + 6 * size_of::<usize>()
        + 4 * size_of::<RetentionError>()
        + 10 * reference;
    let draft = draft_frame + job3::source_bytes().max(targets).max(draft_fold);
    let body = shell
        + frame
        + entry
            .max(held + group + prepare.max(register).max(install))
            .max(draft);
    RowReserveBody {
        shell,
        frame,
        entry,
        held,
        group,
        recompute,
        sources,
        produce,
        prepare,
        register,
        install,
        draft,
        body,
    }
}

/// Source-bound inventory of the actual `validation_lane`
/// (`row_validation_context` and the lane split): the wrapper's own values,
/// the context frame and the largest of its calls (the one-target read of
/// the Catalog leaf, the table record and index count over the schema copy,
/// the RowLeaf header). Its result carriers are the phase's.
pub(in super::super::super) fn row_validation_lane_body<W: WriterNamespace>(shell: usize) -> usize {
    use super::super::super::super::super::read::catalog_source_bytes;
    use std::mem::size_of;
    type Read<'a> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, 1>;
    let reference = size_of::<&()>();
    let wrapper = size_of::<RowTableSource>()
        + 2 * size_of::<Result<(RowPreflightContext, usize)>>()
        + size_of::<RowValidationLane<'_>>()
        + 10 * reference;
    let frame = size_of::<crate::storage::control::Profile>()
        + size_of::<crate::api::Namespace>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<Result<Plan4Target>>()
        + 2 * size_of::<Result<FrameStamp>>()
        + size_of::<std::result::Result<&[u8], RetentionCause>>()
        + size_of::<std::result::Result<u16, RetentionCause>>()
        + size_of::<std::result::Result<TargetRequest, RetentionCause>>()
        + size_of::<RowPreflightContext>()
        + 6 * size_of::<usize>()
        + 2 * size_of::<RetentionError>()
        + 10 * reference;
    let calls = targets::call_bytes::<1>()
        .max(size_of::<Read<'_>>() + 2 * size_of::<dirty_frame::Result<FrameStamp>>())
        .max(
            catalog_source_bytes::<W>() + 2 * size_of::<std::result::Result<u16, RetentionCause>>(),
        )
        .max(2 * size_of::<TargetRequest>() + 4 * reference);
    shell + wrapper + frame + calls
}
