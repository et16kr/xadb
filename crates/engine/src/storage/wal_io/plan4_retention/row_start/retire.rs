//! Live Row retirement (DEV3 3.14.1 for the first Row profile, reclamation
//! `#live-row-retirement`): C's Row Prune (DeletePrune for a DELETE root,
//! dev3-row-delete.md#retirement), ReleaseRole, RootFree and TerminalFinish
//! of a finished Row transaction while the session is open.
//!
//! Each call is one scope3 group of the recovered cleanup's preparation
//! (`prepare_row_recovery`: the same producer, whole target set, canonical
//! replay proof and arena inputs) inside the one gate/progress shell, after
//! the residency of that whole target set. A group that retires a reference
//! target (Prune: the head U, `MissingPrune`; DeletePrune: the head U,
//! `ValuePrune`; RootFree: U, `UndoFree`;
//! TerminalFinish: the actor, `TerminalFree`) installs only through the
//! original retirement protocol: the held target set captures Q and the
//! snapshot horizon under its version hold, `begin_retirement` refuses a live
//! reference pin or a snapshot older than the Prune's Q and publishes the
//! retiring marker, and the stage/apply receipts below bind that marker to
//! this group's frame. ReleaseRole retires no reference target and installs
//! through the one existing-group body (`ExistingGroup::RowRelease`). No
//! group publishes an epoch or a Q.
use super::super::super::super::super::lookup_load::RetentionLoadPort as _;
use super::*;

/// What one live Row cleanup call did.
#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "the census counts the inline Observation, without heap allocation"
)]
pub(in crate::storage) enum RowRetireStep {
    /// The group installed and its frame is durable.
    Retired(row_group::Observation),
    /// The residency of the whole target set needs this dirty victim
    /// flushed first. Nothing was prepared, staged or pinned.
    FlushRequired(Plan4Target),
}

/// The completed actual semantic stage of a live Row retirement group. Only
/// `apply_row_retirement_n` constructs it, after the original stage call.
pub(in crate::storage) struct RowRetirementStaged<'r, 'i, 'c, 's> {
    prepared: &'r PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>,
    requirement: WalRequirement<'s>,
}
impl<'r, 'i, 'c, 's> RowRetirementStaged<'r, 'i, 'c, 's> {
    pub(in super::super::super::super::super) fn prepared(
        &self,
    ) -> &'r PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS> {
        self.prepared
    }
    pub(in super::super::super::super::super) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
}
/// The complete installation of a staged live Row retirement group.
pub(in crate::storage) struct RowRetirementApplied<'r, 'i, 'c, 's> {
    staged: RowRetirementStaged<'r, 'i, 'c, 's>,
}
impl<'r, 'i, 'c, 's> RowRetirementApplied<'r, 'i, 'c, 's> {
    pub(in super::super::super::super::super) fn staged(
        &self,
    ) -> &RowRetirementStaged<'r, 'i, 'c, 's> {
        &self.staged
    }
}
/// The original stage call's exact Unchanged refusal of a live Row
/// retirement group; it returns the retiring marker to its intent state.
pub(in crate::storage) struct RowRetirementRejected<'r, 'i, 'c, 's> {
    prepared: &'r PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>,
}
impl<'r, 'i, 'c, 's> RowRetirementRejected<'r, 'i, 'c, 's> {
    pub(in super::super::super::super::super) fn prepared(
        &self,
    ) -> &'r PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS> {
        self.prepared
    }
}

impl<'i, 'c, 's, const T: usize> PreparedRowGroup<'i, 'c, 's, T> {
    /// The live ReleaseRole, the one Row cleanup group the existing-group
    /// body installs outside the closed recovered facade.
    pub(in super::super) fn release_role(&self) -> bool {
        self.payload.observation.operation == row_semantic::Operation::ReleaseRole
    }
    /// The original pool core, binding and fence of the prepared group.
    pub(in super::super::super::super::super) fn retirement_origin(
        &self,
    ) -> (&'c PageConcurrency<'s>, &'s AtomicU8, &'s SessionFence) {
        (self.payload.core, self.payload.binding, self.payload.fence)
    }
    pub(in super::super::super::super::super) fn retirement_owner(&self) -> OwnerIdentity {
        self.owner
    }
    pub(in super::super::super::super::super) fn retirement_observation(
        &self,
    ) -> row_group::Observation {
        self.payload.observation
    }
    pub(in super::super::super::super::super) fn retirement_targets(&self) -> &[TargetRequest] {
        self.targets()
    }
    /// The group role of target `at`: a producer output role, the
    /// Allocation proof or a proof-only target.
    pub(in super::super::super::super::super) fn retirement_role(&self, at: usize) -> Option<u8> {
        (at < self.count).then(|| self.roles[at])
    }
    pub(in super::super::super::super::super) fn retirement_stamp(
        &self,
        at: usize,
    ) -> Option<FrameStamp> {
        self.stamp(at)
    }
    pub(in super::super::super::super::super) fn retirement_digest(&self) -> [u8; 32] {
        self.payload.frame_digest
    }
    pub(in super::super::super::super::super) fn confirms_retirement_requirement(
        &self,
        requirement: WalRequirement<'_>,
    ) -> bool {
        confirms_group(&self.payload, requirement)
    }
}

fn visibility_error(error: visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Install,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The next live Row cleanup step outside `skip`, or None. Only a
    /// finished actor (its Terminal Job TerminalReady) is selected, first
    /// match by Job ordinal, with the recovered selection's per-root order:
    /// the head RootRetained root of a committed actor (Prune, or DeletePrune
    /// for a DELETE root by its Job effect), a releasing
    /// root (ReleaseRole until its cursor is 3, then RootFree), and only
    /// when no root step is left, a TerminalReady Job whose actor has no
    /// root (TerminalFinish). A caller passes the ordinals whose step was
    /// just refused `Visibility(Busy)` (a Prune beside an older snapshot) in
    /// `skip`, so one held actor does not hide the others. The list names
    /// Job cells. It stays valid across installed steps while no Job is
    /// allocated between the calls (no Begin or Row write on this writer):
    /// a cleanup step only frees cells, and a skipped cell can be freed only
    /// by its own step, which is not selected. After an allocation a listed
    /// cell may hold another actor's Job. Reads the control
    /// Jobs only; the producer checks every precondition again under the
    /// canonical replay proof. Two TerminalReady Jobs of one actor or two
    /// head roots are an invariant breach (Fatal), as in recovery.
    pub(in crate::storage) fn row_cleanup_next(
        &self,
        skip: &[usize],
    ) -> Result<Option<(usize, row_semantic::Operation)>> {
        let storage = self.storage;
        let namespace = storage.profile().spec().namespace;
        let jobs = self.control.jobs();
        let decode = |bytes| {
            job3::decode_job(bytes, namespace).map_err(|error| {
                fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Retention(error),
                )
            })
        };
        // The actor's TerminalReady outcome (1 committed, 2 aborted).
        let finished = |actor| -> Result<Option<u8>> {
            let mut found = None;
            for bytes in jobs {
                let job = decode(bytes)?;
                if job.scope() == Some(JobScope::Row)
                    && job.source() == Some(actor)
                    && job.state() == job3::JobState::TerminalReady
                    && found.replace(job.outcome()).is_some()
                {
                    return Err(fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
            }
            Ok(found)
        };
        for (ordinal, bytes) in jobs.iter().enumerate() {
            if skip.contains(&ordinal) {
                continue;
            }
            let job = decode(bytes)?;
            if job.scope() != Some(JobScope::Row) {
                continue;
            }
            let Some(actor) = job.source() else {
                continue;
            };
            let operation = match job.state() {
                job3::JobState::RootRetained => {
                    if finished(actor)? != Some(1) {
                        continue;
                    }
                    let mut head = true;
                    for (at, bytes) in jobs.iter().enumerate() {
                        let peer = decode(bytes)?;
                        if at == ordinal
                            || peer.scope() != Some(JobScope::Row)
                            || peer.source() != Some(actor)
                            || peer.state() != job3::JobState::RootRetained
                        {
                            continue;
                        }
                        if peer.undo_sequence() == job.undo_sequence() {
                            return Err(fatal(
                                storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            ));
                        }
                        head &= peer.undo_sequence() < job.undo_sequence();
                    }
                    if !head {
                        continue;
                    }
                    match job.effect() {
                        job3::RowEffect::Insert => row_semantic::Operation::Prune,
                        job3::RowEffect::Delete => row_semantic::Operation::DeletePrune,
                        // No product path makes an UPDATE root before
                        // dev3-row-update.md#stages stage 3, which opens
                        // its UpdatePrune here.
                        job3::RowEffect::Update => {
                            return Err(fatal(
                                storage,
                                RetentionPhase::Census,
                                RetentionCause::Unsupported,
                            ));
                        }
                    }
                }
                job3::JobState::RootReleasingCommitted
                | job3::JobState::RootReleasingCompensated => {
                    if finished(actor)?.is_none() {
                        continue;
                    }
                    if job.release_cursor() < 3 {
                        row_semantic::Operation::ReleaseRole
                    } else {
                        row_semantic::Operation::RootFree
                    }
                }
                _ => continue,
            };
            return Ok(Some((ordinal, operation)));
        }
        for (ordinal, bytes) in jobs.iter().enumerate() {
            if skip.contains(&ordinal) {
                continue;
            }
            let job = decode(bytes)?;
            if job.scope() != Some(JobScope::Row) || job.state() != job3::JobState::TerminalReady {
                continue;
            }
            let actor = job.source();
            if let Some(actor) = actor {
                // A second TerminalReady Job of the same actor is Fatal here too.
                finished(actor)?;
            }
            let mut rooted = false;
            for bytes in jobs {
                let peer = decode(bytes)?;
                rooted |= peer.scope() == Some(JobScope::Row)
                    && peer.source() == actor
                    && peer.state().is_root();
            }
            if !rooted {
                return Ok(Some((ordinal, row_semantic::Operation::TerminalFinish)));
            }
        }
        Ok(None)
    }

    /// One live Row cleanup group (Prune, DeletePrune, ReleaseRole, RootFree
    /// or TerminalFinish of Job `ordinal`) inside the one gate/progress
    /// shell: RowWrite for a Prune's write target set, RowProgress for the
    /// four-target cleanup, as the recovered cleanup. The shell's terminal
    /// read gate stays closed from before the residency to after the durable
    /// wait. A refusal before the stage is Unchanged unless it is an
    /// invariant breach (an undecodable held page, contradicting control
    /// Jobs, a producer contradiction), which is Fatal; a pin or an older
    /// snapshot on the retired target is `Visibility(Busy)`.
    pub(in crate::storage) fn retire_row(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowRetireStep> {
        let class = match operation {
            row_semantic::Operation::Prune | row_semantic::Operation::DeletePrune => {
                source::OperationClass::RowWrite
            }
            row_semantic::Operation::ReleaseRole
            | row_semantic::Operation::RootFree
            | row_semantic::Operation::TerminalFinish => source::OperationClass::RowProgress,
            _ => {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
        };
        self.progress_lifecycle(owner, class, flusher, |writer, owner| {
            writer.check_live_row_cleanup()?;
            if let Some(target) = writer.row_cleanup_resident(owner, ordinal, operation, private)? {
                return Ok(RowRetireStep::FlushRequired(target));
            }
            writer
                .retire_resident(owner, ordinal, operation, private)
                .map(RowRetireStep::Retired)
        })
    }

    /// The preparation and install of one resident live Row cleanup group,
    /// in its own frame: the prepared group and its carriers do not exist
    /// during the residency, so the census counts them only beside the
    /// preparation and the install. The preparation writes the group into
    /// the one `Result` slot below and the install borrows it there, so the
    /// frame holds no `?` branch value and no moved copy of the group
    /// (`carrier_source_bytes`); the Observation is copied straight into
    /// the return slot.
    #[inline(never)]
    fn retire_resident(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<row_group::Observation> {
        let result = self
            .catalog_preparation()
            .prepare_row_recovery(owner, ordinal, operation, private);
        let prepared = match result {
            Ok(ref prepared) => prepared,
            Err(error) => return Err(error),
        };
        if prepared.release_role() {
            self.catalog_application()
                .apply_existing(owner, ExistingGroup::RowRelease(prepared))?;
        } else {
            self.apply_row_retirement(owner, prepared)?;
        }
        Ok(prepared.payload.observation)
    }

    /// The open session: only the live facade has an active epoch lane. The
    /// closed recovered facade keeps its own cleanup (`row_recovery_cleanup`).
    fn check_live_row_cleanup(&self) -> Result<()> {
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        if !visibility.epochs_active().map_err(|error| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: error.effect,
            cause: RetentionCause::Visibility(error),
        })? {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            ));
        }
        Ok(())
    }

    /// The LOAD steps of the whole target set through the one retention LOAD
    /// body, each protecting the whole set; a step that needs a dirty victim
    /// ends with it. No pin survives a step. The Allocation page is the
    /// pool's fixed frame 0, protected but never loaded.
    fn row_cleanup_resident(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Option<Plan4Target>> {
        let (requests, count) = self
            .catalog_preparation()
            .row_cleanup_requests(ordinal, operation)?;
        let requests = &requests[..count];
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let mut cursor = Plan4ResidencyCursor::new();
        for request in requests.iter().filter(|request| **request != allocator) {
            match self.load_lookup_target(
                owner,
                *request,
                &mut cursor,
                requests,
                &mut private[..],
            )? {
                RetentionResidencyStep::Resident { .. } => {}
                RetentionResidencyStep::FlushRequired { target, .. } => return Ok(Some(target)),
            }
        }
        Ok(None)
    }

    fn apply_row_retirement(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: &PreparedRowGroup<'_, 'c, 's, ROW_WRITE_TARGETS>,
    ) -> Result<()> {
        match prepared.targets().len() {
            4 => self.apply_row_retirement_n::<4>(owner, prepared),
            8 => self.apply_row_retirement_n::<8>(owner, prepared),
            9 => self.apply_row_retirement_n::<9>(owner, prepared),
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            )),
        }
    }

    /// The retirement install of one prepared Row group, as
    /// `apply_catalog_retirement_n` without a CLR pair, current pin or epoch
    /// change: held original targets, the verified semantic Before,
    /// `begin_retirement`, the stage, the marker-bound page installation and
    /// control part, then the durable wait.
    fn apply_row_retirement_n<const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: &PreparedRowGroup<'_, 'c, 's, ROW_WRITE_TARGETS>,
    ) -> Result<()> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        health(storage, self.pool)?;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        prepared.check(owner, self.pool, self.control, prefix)?;
        let requests: &[TargetRequest; N] = prepared
            .targets()
            .try_into()
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut write = self
            .pool
            .reserve_target_set(owner, requests)
            .and_then(|pins| pins.try_retirement_write(visibility))
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let installed = (|| {
            let verified = visibility.verify_row_retirement(prepared, &write)?;
            let mut dpt = [const { None }; N];
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                if prepared.after(at).is_some() {
                    *dpt_entry = Some(
                        write
                            .preflight_dpt(at, &prepared.stamp(at).expect("actual before"))
                            .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                    );
                }
            }
            let mut apply = write.begin(verified, None).map_err(|rejected| {
                // No current pin was offered, so none comes back.
                debug_assert!(rejected.current.is_none());
                RetentionError {
                    phase: RetentionPhase::Prepare,
                    effect: rejected.error.effect,
                    cause: RetentionCause::Visibility(rejected.error),
                }
            })?;
            apply.stage_call_started().map_err(visibility_error)?;
            let (stage_prefix, header, frame_bytes) = prepared.stage_parts();
            let requirement =
                match storage
                    .wal()
                    .stage_verified_required(stage_prefix, header, frame_bytes)
                {
                    Ok(requirement) => requirement,
                    Err(error) if error.effect == Effect::Unchanged => {
                        apply
                            .reject_row_stage(RowRetirementRejected { prepared })
                            .map_err(visibility_error)?;
                        if apply.cancel_unstaged().map_err(visibility_error)?.is_some() {
                            return Err(fatal(
                                storage,
                                RetentionPhase::Stage,
                                RetentionCause::Invariant,
                            ));
                        }
                        return Err(RetentionError {
                            phase: RetentionPhase::Stage,
                            effect: error.effect,
                            cause: RetentionCause::Wal(error),
                        });
                    }
                    Err(error) => {
                        return Err(fatal(
                            storage,
                            RetentionPhase::Stage,
                            RetentionCause::Wal(error),
                        ));
                    }
                };
            let staged = RowRetirementStaged {
                prepared,
                requirement,
            };
            apply
                .bind_staged(
                    visibility
                        .staged_row_retirement(&staged)
                        .map_err(visibility_error)?,
                )
                .map_err(visibility_error)?;
            prepared.mark_staged(self.control, requirement)?;
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                let Some(after) = prepared.after(at) else {
                    continue;
                };
                let stamp = prepared.stamp(at).expect("checked existing");
                let permit = InstallGrant {
                    kind: InstallKind::Plan4 {
                        core: self.pool.bound_core(),
                        stamp,
                        requirement,
                        after_counter: stamp
                            .modification_counter
                            .checked_add(1)
                            .expect("preflight checked successor"),
                        after,
                    },
                };
                apply
                    .install_page(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                apply
                    .install_metadata(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                apply
                    .install_dpt(dpt_entry.take().expect("actual DPT preflight"), &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
            }
            prepared.apply_control(self.control, storage.profile(), requirement)?;
            self.history.installed(requirement)?;
            let applied = RowRetirementApplied { staged };
            apply
                .finish_applied(
                    visibility
                        .applied_row_retirement(&applied)
                        .map_err(visibility_error)?,
                )
                .map_err(visibility_error)?;
            Ok(requirement)
        })();
        let requirement = merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
        .map_err(|error| {
            if self.control.pending.is_some() {
                fatal(storage, error.phase, error.cause)
            } else {
                error
            }
        })?;
        let durable = storage
            .wal()
            .wait_durable(&requirement)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        if !durable.confirms(&requirement) {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        self.control.durable = Some(durable);
        self.control.pending = None;
        Ok(())
    }
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in crate::storage) fn row_retire(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<RowRetireStep> {
        let (mut writer, _, mut flusher) = self.split();
        writer.retire_row(owner, ordinal, operation, private, &mut flusher)
    }
}

/// The live Row retirement's own entry packet and result slots, live through
/// every phase: the ordinal/operation and borrowed arguments and their
/// closure captures, the `Result<RowRetireStep>` of `retire_row`, of the
/// shell and of its body, and the body frame's slots for the residency's
/// `Result<Option<Plan4Target>>` and `retire_resident`'s
/// `Result<Observation>`.
pub(in super::super::super) fn caller_source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<usize>()
        + 2 * size_of::<row_semantic::Operation>()
        + size_of::<source::OperationClass>()
        + 8 * size_of::<&()>()
        + 3 * size_of::<Result<RowRetireStep>>()
        + size_of::<Result<Option<Plan4Target>>>()
        + size_of::<Result<row_group::Observation>>()
}

/// What `retire_resident` holds from the preparation's call to the end of
/// the install: the one Result slot the preparation writes the prepared
/// group into (the install borrows it there: no `?` branch value and no
/// moved copy of the group exist), the borrow, the error moved out of a
/// refused preparation, the install's group argument and the frame's
/// arguments. Its own `Result<Observation>` slot is the caller's, in
/// `caller_source_bytes`, and the Observation is copied straight into it.
/// The preparation's own arrays and return carriers are in the Row group
/// terms.
pub(in super::super::super) fn carrier_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<Result<PreparedRowGroup<'_, '_, '_, ROW_WRITE_TARGETS>>>()
        + size_of::<&PreparedRowGroup<'_, '_, '_, ROW_WRITE_TARGETS>>()
        + size_of::<RetentionError>()
        + size_of::<ExistingGroup<'_, '_, '_, '_>>()
        + 2 * size_of::<usize>()
        + 4 * size_of::<&()>()
}

/// One residency pass: the selected request array and its Result, the
/// cursor and one LOAD step's Result (the pass's own Result slot is the
/// caller's, in `caller_source_bytes`); the LOAD body itself is
/// `lookup_load::source_bytes`, added by the caller.
pub(in super::super::super) fn residency_source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<[TargetRequest; ROW_WRITE_TARGETS]>()
        + 2 * size_of::<Result<([TargetRequest; ROW_WRITE_TARGETS], usize)>>()
        + size_of::<[(TargetRequest, u8); ROW_WRITE_TARGETS]>()
        + size_of::<job3::JobView<'_>>()
        + size_of::<Plan4ResidencyCursor>()
        + 2 * size_of::<Result<RetentionResidencyStep>>()
        + 4 * size_of::<usize>()
        + 4 * size_of::<&()>()
}

/// The retirement install at nine targets beside the caller's shared
/// scalars: the sealed Row receipts, the visibility receipts and the held
/// original target set, then the consecutive physical closures (retirement
/// targets, the Row control part, the visibility leaf, the durable wait).
pub(in super::super::super) fn install_source_bytes(control: usize, physical: usize) -> usize {
    use crate::storage::dirty_frame;
    use std::mem::size_of;
    size_of::<RowRetirementStaged<'_, '_, '_, '_>>()
        + size_of::<RowRetirementApplied<'_, '_, '_, '_>>()
        + size_of::<RowRetirementRejected<'_, '_, '_, '_>>()
        + 2 * size_of::<InstallGrant<'_, '_, '_>>()
        + 2 * size_of::<visibility::VerifiedRetirement<'_, '_>>()
        + 2 * size_of::<visibility::StagedRetirement<'_, '_>>()
        + 2 * size_of::<visibility::AppliedRetirement<'_, '_>>()
        + 2 * size_of::<visibility::RetirementReject<'_, '_, '_, '_>>()
        + 3 * size_of::<[Option<dirty_frame::DptStamp<'_>>; ROW_WRITE_TARGETS]>()
        + 3 * size_of::<[TargetRequest; ROW_WRITE_TARGETS]>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 3 * size_of::<DurableWal<'_>>()
        + 2 * size_of::<FrameStamp>()
        + 2 * size_of::<WalPrefix>()
        + 8 * size_of::<Result<()>>()
        + 6 * size_of::<RetentionError>()
        + 16 * size_of::<usize>()
        + 16 * size_of::<&[u8]>()
        + dirty_frame::retirement_source_bytes::<ROW_WRITE_TARGETS>()
            .max(dirty_frame::target_set_call_bytes::<ROW_WRITE_TARGETS>())
        + control
            .max(physical)
            .max(visibility::source_census().peak_leaf)
}
