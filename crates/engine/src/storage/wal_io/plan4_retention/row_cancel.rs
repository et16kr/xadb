//! Cancel of a first Row INSERT's reservation (S2-0 Row write amendment §1
//! "order", the refusal table and §6, S2-0b amendment
//! `#row-write-validate-once`). Its caller, `ReservedRow::cancel`, holds the
//! terminal read gate, the RowWrite admission, the flush lease and the one
//! `RowSlotReservation` of the row; this body opens no replay scope and
//! inherits preflight's one `history::validate_live` through the digest
//! chain. In order:
//! 1. the deep checks of an effect step (entry, view, actor, Terminal, ATT
//!    slot, table slot), the reservation's actor and RootJob slot;
//! 2. under one typed read of T, the draft commitment again through
//!    `DraftSource::commitment`, copying T into the before range; a
//!    difference is Before(Stale);
//! 3. C's Cancel producer (`Change::Cancel` of the reserved RootJob) over
//!    those before images, the changed-target identity, the XOI successor
//!    and the progress room;
//! 4. the one existing-group install (stage, pages, control through
//!    `history::row::record`, durable wait).
//!
//! The Cancel frees the RootJob and returns nothing the Reserve issued: the
//! Root keeps its RowId and Undo sequence high-water, and the Undo container
//! keeps its generation. The table write bit stays (§6). Every refusal
//! before the stage is Unchanged, so the wrapper returns the same
//! ReservedRow, except a contradiction of the wrapper's own proof or of the
//! ledger check (Fatal Census); every failure after the stage is Fatal.
//!
//! One suspended caller level holds a Row group return carrier while the
//! group is prepared (S2-0b decision 2): `cancel_group`'s `group`, the slot
//! of `prepare_cancel`'s result, held through the install. No helper between
//! them returns the group or an Observation, and the entry checks run beside
//! `cancel_group`, not under it; the body census (`row_cancel_body`) names
//! that level (`CANCEL_SUSPENDED_LEVEL`).
use super::*;

/// The reservation a Cancel discharges, as the wrapper's one
/// `RowSlotReservation` holds it: scalars only, no pin, loan or authority.
#[derive(Clone, Copy)]
pub(in super::super::super::super::super::super) struct RowCancelIssued {
    pub(in super::super::super::super::super::super) actor: TxnRef,
    pub(in super::super::super::super::super::super) root_job: JobSlotRef,
    pub(in super::super::super::super::super::super) row: RowId,
    pub(in super::super::super::super::super::super) undo: UndoRef,
}

/// The deep checks' scalars the Cancel carries into its preparation.
#[derive(Clone, Copy)]
struct CancelEntry {
    owner: OwnerIdentity,
    prefix: WalPrefix,
    slot: usize,
    ordinal: usize,
    issued: DraftIssued,
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The Cancel body (module doc). `draft` is the ReservedRow's cut and
    /// `issued` its reservation. An Unchanged error is a Before: nothing was
    /// staged. Any other error is Fatal and the fence stays. The entry
    /// checks and the group are two calls one after another, so the group's
    /// return slot (`cancel_group`) is not held across the deep checks.
    pub(in super::super::super::super::super::super) fn row_cancel(
        &mut self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        (table, draft, issued): (RowTableSource, &RowDraftCut, RowCancelIssued),
        private: &mut [u8; 425_984],
    ) -> Result<()> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let entry = self.cancel_entry(owner, statement, table, (draft, issued))?;
        self.cancel_group(owner, &entry, draft, private)
    }

    /// Steps 2 to 4 of the module doc. The one suspended level: `group` is
    /// the slot of `prepare_cancel`'s result, matched by reference so that
    /// the group is never moved into a second slot, and it is held through
    /// the install.
    fn cancel_group(
        &mut self,
        owner: &mut Owner<'c, 's>,
        entry: &CancelEntry,
        draft: &RowDraftCut,
        private: &mut [u8; 425_984],
    ) -> Result<()> {
        let ReserveLanes { before, output, .. } = ReserveLanes::split(private);
        let group = self
            .catalog_preparation()
            .prepare_cancel(owner, entry, draft, before, output);
        let prepared = match group {
            Ok(ref prepared) => prepared,
            Err(error) => return Err(error),
        };
        // The producer's own Cancel output: the reserved RootJob and the
        // caller's actor, nothing issued.
        let observation = &prepared.payload.observation;
        if observation.operation != row_semantic::Operation::Cancel
            || observation.job_ordinal != Some(entry.ordinal)
            || observation.actor != Some(entry.issued.actor)
            || observation.issued.is_some()
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        self.catalog_application()
            .apply_existing(owner, ExistingGroup::RowWrite(prepared))
    }

    /// The deep checks of an effect step, in preflight's order: the entry of
    /// an existing-group preparation (health, idle owner, WAL prefix, whole
    /// ledger), the view and its statement, the actor (the reservation's),
    /// its Terminal and ATT slot, and the table's epoch slot. A complete LSN
    /// other than the cut's is Before(Stale) without rehashing. The actor's
    /// head is not checked as reserve checks it: this actor is RootReserved
    /// by the reservation, and the draft commitment folds every Job row, so
    /// any change of them since the Reserve is Stale. The RootJob's Undo
    /// sequence, which the commitment folds, is read from that Job. The whole
    /// ledger check above has already decoded every Job cell (an undecodable
    /// one is its Unchanged refusal), so a RootJob that does not decode here
    /// contradicts that check under the exclusive writer: the control's
    /// corruption, Fatal as every other deep check's Job decode.
    fn cancel_entry(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        table: RowTableSource,
        (draft, issued): (&RowDraftCut, RowCancelIssued),
    ) -> Result<CancelEntry> {
        let storage = self.storage;
        let preparation = self.catalog_preparation();
        let (identity, prefix) = preparation.existing_entry(owner)?;
        statement.check_catalog_read()?;
        let actor = statement
            .own_actor()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        // The reservation was issued to this statement's actor; another one
        // contradicts the wrapper's own proof.
        if actor != issued.actor {
            return Err(fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            ));
        }
        preparation.row_terminal(actor)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        self.check_table_slot(table)?;
        if prefix.complete_lsn != draft.complete_lsn {
            return Err(stale());
        }
        let ordinal = issued
            .root_job
            .ordinal()
            .map_err(|_| fatal(storage, RetentionPhase::Census, RetentionCause::Invariant))?;
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
        Ok(CancelEntry {
            owner: identity,
            prefix,
            slot,
            ordinal,
            issued: DraftIssued {
                actor,
                root_job: issued.root_job,
                row: issued.row,
                undo: issued.undo,
                sequence,
                mutation: draft.planned_mutation,
            },
        })
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// The Cancel group over the whole write target set T, steps 2 and 3 of
    /// the module doc. Every refusal is Unchanged before any stage, except a
    /// contradiction of the draft's role map or the reservation (Fatal
    /// Census). T, its stamps and the roles are filled in place, in the
    /// arrays the group keeps.
    fn prepare_cancel<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &CancelEntry,
        draft: &RowDraftCut,
        before: &'i mut [[u8; PAGE_BYTES]; 9],
        output: ReserveOutput<'i>,
    ) -> Result<PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>> {
        let storage = self.storage;
        let profile = storage.profile();
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        // The Catalog leaf request only fills the positions past |T|.
        let mut requests =
            [target_from_source(storage, Plan4Page::CatalogLeaf)?.request; ROW_WRITE_TARGETS];
        let mut stamps = [None; ROW_WRITE_TARGETS];
        let (count, roles) =
            self.cancel_sources(owner, entry, draft, before, (&mut requests, &mut stamps))?;
        let before: &'i [[u8; PAGE_BYTES]; 9] = before;
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
            let aging = self.control.row(control::AGING_SLOT);
            let inputs = row_group::Inputs {
                profile,
                frame: expected,
                change: row_group::Change::Cancel {
                    ordinal: entry.ordinal,
                },
                jobs: self.control.jobs(),
                root: &before[at(ROOT)?],
                jobs_page: Some(&before[at(JOB)?]),
                status: Some(&before[at(STATUS)?]),
                undo: Some(&before[at(row_group::UNDO as u8)?]),
                previous: None,
                table: None,
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
        // Every changed page is a T member at that identity.
        for role in order {
            let target = requests[at(*role)
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?];
            let image = recovery::request_from_image(profile, &after[usize::from(*role)])
                .map_err(|cause| unchanged(RetentionPhase::Prepare, cause))?;
            if image != target {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        let root = if order.contains(&ROOT) {
            &after[usize::from(ROOT)]
        } else {
            &before[at(ROOT)?]
        };
        let bytes = frame
            .get(..observation.frame_bytes)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let future = {
            let layout = row_semantic::decode_layout(bytes, expected, profile)
                .map_err(row_semantic_error)?;
            prepare_inventory(
                self.control,
                profile,
                observation.header.lsn,
                GroupRows::row(&observation, Some(entry.slot), layout),
                root,
                &before[at(ALLOCATOR_ROLE)?],
                observation.job_ordinal.map(|_| &after[usize::from(JOB)]),
                inventory,
            )?
        };
        check_progress_room(
            entry.prefix,
            (observation.header.lsn, observation.frame_bytes),
            future,
            (&roles[..count], &stamps[..count]),
            order,
            root,
            profile,
        )?;
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

    /// Step 2 of the module doc, before the producer: T in before order into
    /// `requests` and its group roles, the reservation's RootJob page and
    /// Undo container at their roles, then the draft recompute (T's stamps
    /// into `stamps`, T's pool pages into `before`). Returns |T| and the
    /// roles.
    fn cancel_sources(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &CancelEntry,
        draft: &RowDraftCut,
        before: &mut [[u8; PAGE_BYTES]; 9],
        (requests, stamps): (
            &mut [TargetRequest; ROW_WRITE_TARGETS],
            &mut [Option<FrameStamp>; ROW_WRITE_TARGETS],
        ),
    ) -> Result<(usize, [u8; ROW_WRITE_TARGETS])> {
        let storage = self.storage;
        let profile = storage.profile();
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let count =
            before_targets(profile, before, draft.roles, requests).map_err(|_| invariant())?;
        let roles = row_write_roles(draft.roles, count).ok_or_else(invariant)?;
        // The role map names the reservation's RootJob page and container.
        let page = |role: RowSourceRole| {
            role_position(draft.roles, role)
                .filter(|at| *at < count)
                .map(|at| requests[at].identity.page)
        };
        if page(RowSourceRole::RootJob) != Some(entry.issued.root_job.page())
            || page(RowSourceRole::Undo) != Some(entry.issued.undo.page())
        {
            return Err(invariant());
        }
        let targets = &requests[..count];
        match count {
            7 => self.cancel_recompute::<7>(owner, entry, (targets, draft), before, stamps),
            8 => self.cancel_recompute::<8>(owner, entry, (targets, draft), before, stamps),
            9 => self.cancel_recompute::<9>(owner, entry, (targets, draft), before, stamps),
            _ => Err(invariant()),
        }?;
        Ok((count, roles))
    }

    /// The kind-2 recompute (S2-0b "digest chain", prepare(Forward)·cancel):
    /// under one typed read of T (`N` = |T|), with no replay, each pool page
    /// is copied into the before range and the same `DraftSource::commitment`
    /// the Reserve recorded is folded over those copies, the WAL prefix, the
    /// issued identity and the control. On equality it writes T's stamps into
    /// `stamps` for the install's recheck. A difference is Before(Stale); a
    /// target that is not resident is Fatal(Invariant).
    fn cancel_recompute<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        entry: &CancelEntry,
        (targets, draft): (&[TargetRequest], &RowDraftCut),
        before: &mut [[u8; PAGE_BYTES]; 9],
        stamps: &mut [Option<FrameStamp>; ROW_WRITE_TARGETS],
    ) -> Result<()> {
        let storage = self.storage;
        let requests: [TargetRequest; N] = std::array::from_fn(|at| targets[at]);
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
            let aging = self.control.row(control::AGING_SLOT);
            let source = DraftSource {
                prefix: entry.prefix,
                targets: &requests,
                stamps: &observed,
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

/// The one suspended caller level of the Cancel that holds a Row group return
/// carrier (S2-0b decision 2), as the census names it.
pub(in super::super::super::super::super::super) const CANCEL_SUSPENDED_LEVEL: &str =
    "cancel_group.group: Result<PreparedRowGroup<ROW_WRITE_TARGETS>>";

/// Source-bound inventory of the actual Cancel (`ReservedRow::cancel` and
/// `row_cancel`), in the repository's size_of sense (not a stack
/// measurement), as `RowReserveBody`: a call's own values are held
/// throughout it and added; the calls it makes one after another are
/// max-combined. The phase's wrapper and result carriers are the Row write
/// census's `carriers` (`row_write_phases`).
#[derive(Clone, Copy, Debug)]
pub(in super::super::super::super::super::super) struct RowCancelBody {
    /// The S2-a′ shell (admission, gate, their carriers and the unwind
    /// fences) and `ReservedRow::cancel`'s own values, its release included.
    pub(in super::super::super::super::super::super) shell: usize,
    /// `row_cancel`'s own values: arguments, the entry and the Results. It
    /// calls `cancel_entry` and `cancel_group` one after another.
    pub(in super::super::super::super::super::super) frame: usize,
    /// `cancel_entry`: its own values, the deep checks (preflight's `check`
    /// term) and the RootJob decode after them.
    pub(in super::super::super::super::super::super) entry: usize,
    /// The suspended level (`CANCEL_SUSPENDED_LEVEL`) times its return slot:
    /// `cancel_group`'s `group`, held from the preparation through the
    /// install.
    pub(in super::super::super::super::super::super) held: usize,
    /// `cancel_group`'s own values other than `held`: arguments, lanes, the
    /// install's group argument and Results.
    pub(in super::super::super::super::super::super) group: usize,
    /// `cancel_recompute::<9>`: the typed read of T, the copies' stamps and
    /// the draft commitment.
    pub(in super::super::super::super::super::super) recompute: usize,
    /// `cancel_sources`: its own values and the largest of T, the roles and
    /// `recompute`.
    pub(in super::super::super::super::super::super) sources: usize,
    /// C's Cancel producer over the before images: Inputs and
    /// `produce_borrowed`.
    pub(in super::super::super::super::super::super) produce: usize,
    /// `prepare_cancel`: its own values and the largest of its calls
    /// (`sources`, `produce`, the changed-target identity, the shared
    /// inventory/progress term and the assembly).
    pub(in super::super::super::super::super::super) prepare: usize,
    /// The existing-group install at nine targets (the S2-a′
    /// `group_install`).
    pub(in super::super::super::super::super::super) install: usize,
    /// `shell + frame + max(entry, held + group + max(prepare, install))`.
    pub(in super::super::super::super::super::super) body: usize,
}

/// The actual Cancel census. `check` is preflight's callee term of the same
/// deep checks (`RowPreflightBody`); `install` and `inventory` are the S2-a′
/// install and shared inventory terms at nine targets, which this body calls
/// unchanged. The helper terms (`fold`, `identity`, `targets`, `roles`,
/// `assembled`) are charged as `row_reserve_body` charges the same calls.
pub(in super::super::super::super) fn row_cancel_body<W: WriterNamespace>(
    shell: usize,
    check: usize,
    (install, inventory): (usize, usize),
) -> RowCancelBody {
    use crate::storage::{retention_page::RetentionError as JobError, session_fence::UnwindFence};
    use std::mem::size_of;
    type Read<'a, const N: usize> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Group<'a> = PreparedRowGroup<'a, 'a, 'a, ROW_WRITE_TARGETS>;
    type Cause<T> = std::result::Result<T, RetentionCause>;
    let reference = size_of::<&()>();
    // ControlCut::capture and a commitment: hashers, the pinned SHA block
    // closure, the encoded actor and references and the digests.
    let fold = 2 * size_of::<ContentHasher>() + 1536 + 24 + 48 + 40 + 4 * size_of::<[u8; 32]>();
    // One `request_from_image`.
    let identity = 2 * size_of::<TargetRequest>() + 2 * size_of::<Cause<TargetRequest>>();
    // `before_targets` into a caller array.
    let targets = identity + 2 * size_of::<usize>() + 4 * reference;
    let roles = size_of::<Option<[u8; ROW_WRITE_TARGETS]>>() + 6 * size_of::<usize>();
    // The group literal is built in the caller's slot (`held`); its payload
    // values, the cut capture and the frame digest.
    let assembled = size_of::<RowPayload<'_, '_, '_>>() + fold;
    // The wrapper: the table source and the reservation it passes, the
    // body's Result, the in-place release's admission, gate and cleanup
    // Results and the returned loan, the Fatal carriers and its references.
    let shell = shell
        + size_of::<RowTableSource>()
        + size_of::<RowCancelIssued>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<Result<&mut [u8; 425_984]>>()
        + 2 * size_of::<RetentionError>()
        + 10 * reference;
    let frame = size_of::<RowTableSource>()
        + size_of::<RowCancelIssued>()
        + size_of::<UnwindFence<'_>>()
        + size_of::<CancelEntry>()
        + size_of::<Result<CancelEntry>>()
        + size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, three arguments, the cut and storage.
        + 6 * reference;
    let entry = size_of::<RowTableSource>()
        + size_of::<RowCancelIssued>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Result<(usize, StatusSlotRef)>>()
        + size_of::<Option<usize>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<std::result::Result<usize, JobError>>()
        + size_of::<job3::JobView<'_>>()
        + size_of::<std::result::Result<job3::JobView<'_>, JobError>>()
        + size_of::<CancelEntry>()
        + 2 * size_of::<RetentionError>()
        + 8 * reference
        // The deep checks, then the RootJob decode.
        + check.max(job3::source_bytes());
    let held = ROW_WRITE_SUSPENDED_LEVELS * size_of::<Result<Group<'_>>>();
    let group = size_of::<ReserveLanes<'_>>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<ExistingGroup<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        // Receiver, four arguments, the prepared and observation references.
        + 7 * reference;
    let recompute_frame = size_of::<[TargetRequest; 9]>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<FrameStamp>()
        + size_of::<[FrameStamp; 9]>()
        + 2 * size_of::<dirty_frame::Result<FrameStamp>>()
        + size_of::<DraftSource<'_, std::slice::Iter<'_, [u8; PAGE_BYTES]>>>()
        + size_of::<ControlCut>()
        + size_of::<Cause<[u8; 32]>>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<dirty_frame::Result<()>>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closure capture and the Aging row.
        + 12 * reference;
    let recompute = recompute_frame
        + targets::call_bytes::<9>()
            .max(size_of::<Read<'_, 9>>() + fold.max(size_of::<Cause<[u8; 32]>>()));
    let sources_frame = size_of::<crate::storage::control::Profile>()
        + size_of::<usize>()
        + size_of::<Cause<usize>>()
        + size_of::<[u8; ROW_WRITE_TARGETS]>()
        + 2 * size_of::<Option<PageRef>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<Result<(usize, [u8; ROW_WRITE_TARGETS])>>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closures' captures and T's slice.
        + 14 * reference;
    let sources = sources_frame + targets.max(roles).max(recompute);
    let produce = size_of::<row_group::Inputs<'_>>()
        + size_of::<&mut [u8; ROW_REPLAY_SCRATCH_BYTES]>()
        + row_group::produce_borrowed_source_bytes();
    // The produce callee's Result is the produce term's
    // (`produce_borrowed_source_bytes`); the inventory call's Result is the
    // inventory term's.
    let prepare_frame = size_of::<ReserveOutput<'_>>()
        + size_of::<crate::storage::control::Profile>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<[TargetRequest; ROW_WRITE_TARGETS]>()
        + size_of::<[Option<FrameStamp>; ROW_WRITE_TARGETS]>()
        + size_of::<(usize, [u8; ROW_WRITE_TARGETS])>()
        + size_of::<Result<(usize, [u8; ROW_WRITE_TARGETS])>>()
        + size_of::<FrameExpectation>()
        + size_of::<row_group::Observation>()
        + size_of::<Option<&[u8]>>()
        + size_of::<(u64, u64)>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<Result<usize>>()
        + 6 * size_of::<usize>()
        + 4 * size_of::<RetentionError>()
        // Receiver, arguments, storage, the closures' captures, the Aging
        // row, order, root, before, after and frame.
        + 20 * reference;
    let prepare = prepare_frame
        + sources
            .max(produce)
            .max(identity)
            .max(inventory)
            .max(assembled);
    let body = shell + frame + entry.max(held + group + prepare.max(install));
    RowCancelBody {
        shell,
        frame,
        entry,
        held,
        group,
        recompute,
        sources,
        produce,
        prepare,
        install,
        body,
    }
}
