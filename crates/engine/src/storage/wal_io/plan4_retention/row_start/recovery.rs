//! Recovered Row cleanup groups (XADB-MODULE-ROW-RECOVERY-001, boundaries
//! `#row-recovery-s6`): C's Row Cancel of a RootReserved, committed Prune of
//! the actor's head root, ReleaseRole, RootFree and TerminalFinish. Each is
//! one scope3 group of C's producer over the original typed read of its whole
//! target set, inside the canonical replay scope that proves every before (a
//! proof-only target is observed in place) equal to the replay arena. The
//! producer inputs the arena alone supplies (the table source and the H Undo)
//! are taken from that arena after this proof. Only the recovered cleanup of
//! the closed facade calls this; the group installs through the one
//! existing-group body as `ExistingGroup::RowRecovery`.
use super::*;

/// The selected recovered target set: the ATT slot of a handoff and the N
/// (target, group role) pairs.
type RecoverySelection<const N: usize> = (Option<usize>, [(TargetRequest, u8); N]);

impl<'i, 'c, 's, const T: usize> PreparedRowGroup<'i, 'c, 's, T> {
    /// The recovered Row cleanup operations: the install body admits them for
    /// a `RowRecovery` payload, the kind only this module builds.
    pub(in super::super) fn recovery(&self) -> bool {
        matches!(
            self.payload.observation.operation,
            row_semantic::Operation::Cancel
                | row_semantic::Operation::Prune
                | row_semantic::Operation::ReleaseRole
                | row_semantic::Operation::RootFree
                | row_semantic::Operation::TerminalFinish
        )
    }
}

/// The predecessor H of a Row root: the same actor's RootRetained root with
/// the largest smaller Undo sequence (the producer's `cells::predecessor`).
pub(in super::super::super::super::super) fn recovery_predecessor(
    jobs: &[[u8; JOB_BYTES]; JOB_COUNT],
    namespace: crate::api::Namespace,
    ordinal: usize,
) -> std::result::Result<Option<usize>, ()> {
    let job = job3::decode_job(jobs.get(ordinal).ok_or(())?, namespace).map_err(|_| ())?;
    let actor = job.source().ok_or(())?;
    let mut found: Option<(usize, u64)> = None;
    for (other, cell) in jobs.iter().enumerate() {
        let peer = job3::decode_job(cell, namespace).map_err(|_| ())?;
        if other != ordinal
            && peer.scope() == Some(JobScope::Row)
            && peer.source() == Some(actor)
            && peer.state() == job3::JobState::RootRetained
            && peer.undo_sequence() < job.undo_sequence()
            && found.is_none_or(|(_, sequence)| peer.undo_sequence() > sequence)
        {
            found = Some((other, peer.undo_sequence()));
        }
    }
    Ok(found.map(|(other, _)| other))
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// The whole target set of one recovered Row cleanup group, by operation:
    /// Cancel takes Allocation, Root, RootJob page, Status and U (the latter
    /// two read proofs); ReleaseRole/RootFree take Allocation, Root, RootJob
    /// page and U; TerminalFinish Allocation, Root, Terminal page and Status;
    /// committed Prune the five changed pages, Allocation and the proofs
    /// TableAnchor, CatalogLeaf and, when H exists, H's Undo.
    fn prepare_row_recovery<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>> {
        let invalid = || unchanged(RetentionPhase::Prepare, RetentionCause::Invariant);
        let namespace = self.storage.profile().spec().namespace;
        let previous = if operation == row_semantic::Operation::Prune {
            recovery_predecessor(self.control.jobs(), namespace, ordinal).map_err(|_| invalid())?
        } else {
            None
        };
        match (operation, previous) {
            (row_semantic::Operation::Cancel, _) => {
                self.prepare_row_recovery_n::<5>(owner, ordinal, operation, None, private)
            }
            (
                row_semantic::Operation::ReleaseRole
                | row_semantic::Operation::RootFree
                | row_semantic::Operation::TerminalFinish,
                _,
            ) => self.prepare_row_recovery_n::<4>(owner, ordinal, operation, None, private),
            (row_semantic::Operation::Prune, None) => {
                self.prepare_row_recovery_n::<8>(owner, ordinal, operation, None, private)
            }
            (row_semantic::Operation::Prune, Some(h)) => {
                self.prepare_row_recovery_n::<9>(owner, ordinal, operation, Some(h), private)
            }
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            )),
        }
    }

    fn row_recovery_selection<const N: usize>(
        &self,
        ordinal: usize,
        operation: row_semantic::Operation,
        previous: Option<usize>,
    ) -> Result<RecoverySelection<N>> {
        let storage = self.storage;
        let invalid = || unchanged(RetentionPhase::Prepare, RetentionCause::Invariant);
        let job = job3::decode_job(
            self.control.jobs().get(ordinal).ok_or_else(invalid)?,
            storage.profile().spec().namespace,
        )
        .map_err(queue_error)?;
        if job.scope() != Some(JobScope::Row) {
            return Err(invalid());
        }
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let database = allocator.identity.database;
        let root = target_from_source(storage, Plan4Page::Root)?.request;
        let jobs = target_from_source(
            storage,
            Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            },
        )?
        .request;
        let undo_request = |page: crate::storage::page::PageRef| {
            request(
                PageIdentity {
                    database,
                    page,
                    owner: ObjectId::new(3).expect("Undo owner"),
                    owner_generation: one(),
                },
                PageKind::Undo,
            )
        };
        let status = || -> Result<TargetRequest> {
            Ok(self.catalog_status_request(job.status().ok_or_else(invalid)?))
        };
        let undo = || -> Result<TargetRequest> {
            Ok(undo_request(job.undo().ok_or_else(invalid)?.page()))
        };
        let mut selected = [(allocator, ALLOCATOR_ROLE); N];
        let mut slot = None;
        match operation {
            row_semantic::Operation::Cancel if N == 5 => {
                let actor = job.source().ok_or_else(invalid)?;
                slot = Some(self.control.slot(actor).ok_or_else(|| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
                })?);
                selected.copy_from_slice(&[
                    (allocator, ALLOCATOR_ROLE),
                    (root, ROOT),
                    (jobs, JOB),
                    (status()?, STATUS),
                    (undo()?, row_group::UNDO as u8),
                ]);
            }
            row_semantic::Operation::ReleaseRole | row_semantic::Operation::RootFree if N == 4 => {
                selected.copy_from_slice(&[
                    (allocator, ALLOCATOR_ROLE),
                    (root, ROOT),
                    (jobs, JOB),
                    (undo()?, row_group::UNDO as u8),
                ]);
            }
            row_semantic::Operation::TerminalFinish if N == 4 => {
                selected.copy_from_slice(&[
                    (allocator, ALLOCATOR_ROLE),
                    (root, ROOT),
                    (jobs, JOB),
                    (status()?, STATUS),
                ]);
            }
            row_semantic::Operation::Prune if N == 8 + usize::from(previous.is_some()) => {
                let handle = job.object().ok_or_else(invalid)?;
                let pair = job.pair().ok_or_else(invalid)?;
                let table = |page, kind| {
                    request(
                        PageIdentity {
                            database,
                            page,
                            owner: handle.object,
                            owner_generation: handle.generation,
                        },
                        kind,
                    )
                };
                let catalog = target_from_source(storage, Plan4Page::CatalogLeaf)?.request;
                selected[..8].copy_from_slice(&[
                    (allocator, ALLOCATOR_ROLE),
                    (root, ROOT),
                    (jobs, JOB),
                    (status()?, STATUS),
                    (undo()?, row_group::UNDO as u8),
                    (table(pair[1], PageKind::RowLeaf), row_group::LEAF as u8),
                    (table(pair[0], PageKind::TreeAnchor), PROOF_ROLE),
                    (catalog, PROOF_ROLE),
                ]);
                if let Some(h) = previous {
                    let peer = job3::decode_job(
                        self.control.jobs().get(h).ok_or_else(invalid)?,
                        storage.profile().spec().namespace,
                    )
                    .map_err(queue_error)?;
                    selected[8] = (
                        undo_request(peer.undo().ok_or_else(invalid)?.page()),
                        PROOF_ROLE,
                    );
                }
            }
            _ => return Err(invalid()),
        }
        Ok((slot, selected))
    }

    /// `prepare_existing_row`'s entry checks, canonical replay validation,
    /// typed read/arena equality/allocator membership, XOI successor and
    /// progress room around C's scope3 producer, over a recovered cleanup
    /// target set. Every refusal is Unchanged before any stage.
    fn prepare_row_recovery_n<'i, const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        previous: Option<usize>,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's, ROW_WRITE_TARGETS>> {
        const {
            assert!(N >= 4 && N <= ROW_WRITE_TARGETS);
        }
        let storage = self.storage;
        let profile = storage.profile();
        let _unwind = storage.session_fence().unwind_guard();
        let (owner_identity, prefix) = self.existing_entry(owner)?;
        let (slot, mut selected) =
            self.row_recovery_selection::<N>(ordinal, operation, previous)?;
        selected.sort_unstable_by_key(|(target, _)| target_rank(*target));
        let change = match operation {
            row_semantic::Operation::Cancel => row_group::Change::Cancel { ordinal },
            row_semantic::Operation::Prune => row_group::Change::Prune { ordinal },
            row_semantic::Operation::ReleaseRole => row_group::Change::ReleaseRole { ordinal },
            row_semantic::Operation::RootFree => row_group::Change::RootFree { ordinal },
            row_semantic::Operation::TerminalFinish => {
                row_group::Change::TerminalFinish { ordinal }
            }
            _ => {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
        };
        let expected = FrameExpectation {
            stream: prefix.stream,
            previous_lsn: prefix.complete_lsn,
        };
        let (frame_space, pages) = private.split_at_mut(semantic_catalog::FRAME_BYTES);
        let (after_space, before_space) = pages.split_at_mut(8 * PAGE_BYTES);
        let (after_space, scratch_space) = after_space.split_at_mut(5 * PAGE_BYTES);
        let after: &mut [[u8; PAGE_BYTES]; 5] = after_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("five Row after loans");
        let scratch: &mut [u8; ROW_REPLAY_SCRATCH_BYTES] = (&mut scratch_space
            [..ROW_REPLAY_SCRATCH_BYTES])
            .try_into()
            .expect("Row producer scratch in the unused after loans");
        let before: &mut [[u8; PAGE_BYTES]; 8] = before_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("eight original before loans");
        let (observation, stamps) = {
            let mut audit = self.history.replay_scope(owner)?;
            let mut replay = audit.checkout()?;
            if let Err(error) =
                history::validate_live(storage, self.control, replay.bytes_mut(), prefix)
            {
                let rejected = merge(Err(error), replay.return_checked());
                return merge(rejected, audit.finish_checked());
            }
            let requests: [TargetRequest; N] = std::array::from_fn(|i| selected[i].0);
            let mut stamps = [None; 9];
            let produced = (|| {
                let read = self
                    .access
                    .read(self.pool, owner, &requests)
                    .map_err(|error| frame(RetentionPhase::Prepare, error))?;
                let observed = (|| {
                    let arena = replay.bytes();
                    self.observe_targets(
                        &read,
                        &selected,
                        before,
                        arena,
                        &mut stamps,
                        ALLOCATOR_ROLE,
                    )?;
                    let has = |role| selected.iter().any(|(_, actual)| *actual == role);
                    let job =
                        job3::decode_job(&self.control.jobs()[ordinal], profile.spec().namespace)
                            .map_err(queue_error)?;
                    let table = if operation == row_semantic::Operation::Prune {
                        Some(
                            history::row::table_source(arena, profile, job)
                                .map_err(|cause| unchanged(RetentionPhase::Prepare, cause))?,
                        )
                    } else {
                        None
                    };
                    let previous = previous
                        .map(|h| {
                            let peer =
                                job3::decode_job(&self.control.jobs()[h], profile.spec().namespace)
                                    .map_err(queue_error)?;
                            let page = peer
                                .undo()
                                .ok_or_else(|| {
                                    unchanged(RetentionPhase::Prepare, RetentionCause::Invariant)
                                })?
                                .page();
                            Ok::<_, RetentionError>(history::arena_page(
                                arena,
                                page.page_no as usize,
                            ))
                        })
                        .transpose()?;
                    let inputs = row_group::Inputs {
                        profile,
                        frame: expected,
                        change,
                        jobs: self.control.jobs(),
                        root: &before[usize::from(ROOT)],
                        jobs_page: has(JOB).then_some(&before[usize::from(JOB)]),
                        status: has(STATUS).then_some(&before[usize::from(STATUS)]),
                        undo: has(row_group::UNDO as u8).then_some(&before[row_group::UNDO]),
                        previous,
                        table,
                        transaction: slot.map(|index| {
                            self.control.row(index)[..1536]
                                .try_into()
                                .expect("original transaction descriptor")
                        }),
                        aging: (self.control.row(control::AGING_SLOT)[control::PHASE] != 0).then(
                            || {
                                self.control.row(control::AGING_SLOT)[..1536]
                                    .try_into()
                                    .expect("original Aging descriptor")
                            },
                        ),
                    };
                    row_group::produce_borrowed(&inputs, after, frame_space, scratch)
                        .map_err(row_semantic_error)
                })();
                merge(
                    observed,
                    read.release_checked()
                        .map_err(|error| frame(RetentionPhase::Prepare, error)),
                )
            })();
            let produced = merge(produced, replay.return_checked());
            let observation = merge(produced, audit.finish_checked())?;
            (observation, stamps)
        };
        if observation.operation != operation || observation.job_ordinal != Some(ordinal) {
            // C's own output names another step: its proof contradiction.
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let current = storage
            .wal()
            .prefix()
            .map_err(|error| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(error)))?;
        if current != prefix {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let order = &observation.order[..usize::from(observation.count)];
        // Every changed page is a selected target at that identity.
        for role in order {
            let target = selected
                .iter()
                .find(|(_, actual)| actual == role)
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            let image = recovery::request_from_image(profile, &after[usize::from(*role)])
                .map_err(|cause| unchanged(RetentionPhase::Prepare, cause))?;
            if image != target.0 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        let root = if order.contains(&ROOT) {
            &after[usize::from(ROOT)]
        } else {
            &before[usize::from(ROOT)]
        };
        let (bytes, spare) = frame_space.split_at_mut(observation.frame_bytes);
        let inventory: &mut [u8; PAGE_BYTES] = spare
            .get_mut(..PAGE_BYTES)
            .and_then(|page| page.try_into().ok())
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let future = {
            let layout = row_semantic::decode_layout(bytes, expected, profile)
                .map_err(row_semantic_error)?;
            prepare_inventory(
                self.control,
                profile,
                observation.header.lsn,
                GroupRows::row(&observation, slot, layout),
                root,
                &before[usize::from(ALLOCATOR_ROLE)],
                observation.job_ordinal.map(|_| &after[usize::from(JOB)]),
                inventory,
            )?
        };
        let mut requests = [selected[0].0; ROW_WRITE_TARGETS];
        let mut roles = [ALLOCATOR_ROLE; ROW_WRITE_TARGETS];
        for (at, (request, role)) in selected.into_iter().enumerate() {
            requests[at] = request;
            roles[at] = role;
        }
        check_progress_room(
            prefix,
            (observation.header.lsn, observation.frame_bytes),
            future,
            (&roles[..N], &stamps[..N]),
            order,
            root,
            profile,
        )?;
        Ok(PreparedRowGroup {
            payload: RowPayload {
                core: self.pool.bound_core(),
                binding: &storage.binding,
                fence: storage.session_fence(),
                prefix,
                observation,
                slot,
                frame: bytes,
                frame_digest: content_digest(bytes),
                after,
                inventory,
                control: ControlCut::capture(self.control),
            },
            owner: owner_identity,
            requests,
            roles,
            count: N,
            stamps: std::array::from_fn(|at| stamps[at]),
        })
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The closed recovered facade check of a retiring Row cleanup
    /// (Prune: its U and the actor's committed Q; RootFree: its U;
    /// TerminalFinish: its actor). Cancel and ReleaseRole retire nothing.
    fn check_closed_row_cleanup(
        &self,
        ordinal: usize,
        operation: row_semantic::Operation,
    ) -> Result<()> {
        let namespace = self.storage.profile().spec().namespace;
        let invalid = || unchanged(RetentionPhase::Prepare, RetentionCause::Invariant);
        let job = job3::decode_job(
            self.control.jobs().get(ordinal).ok_or_else(invalid)?,
            namespace,
        )
        .map_err(queue_error)?;
        let (undo, actor, commit_q) = match operation {
            row_semantic::Operation::Cancel | row_semantic::Operation::ReleaseRole => {
                return Ok(());
            }
            row_semantic::Operation::Prune => {
                let source = job.source().ok_or_else(invalid)?;
                let mut q = None;
                for bytes in self.control.jobs() {
                    let peer = job3::decode_job(bytes, namespace).map_err(queue_error)?;
                    if peer.scope() == Some(JobScope::Row)
                        && peer.source() == Some(source)
                        && peer.state() == job3::JobState::TerminalReady
                        && q.replace(peer.commit_q()).is_some()
                    {
                        return Err(invalid());
                    }
                }
                (job.undo(), None, Some(q.ok_or_else(invalid)?))
            }
            row_semantic::Operation::RootFree => (job.undo(), None, None),
            row_semantic::Operation::TerminalFinish => (None, job.source(), None),
            _ => return Err(invalid()),
        };
        if undo.is_none() && actor.is_none() {
            return Err(invalid());
        }
        self.visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?
            .check_closed_row_cleanup(undo, actor, commit_q)
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })
    }

    /// One recovered Row cleanup group (Cancel, Prune, ReleaseRole, RootFree,
    /// TerminalFinish) inside the one gate/progress shell: RowWrite for the
    /// Cancel and Prune write target sets, RowProgress for the four-target
    /// lifecycle cleanup. The closed facade check runs before the
    /// preparation and again just before the install.
    pub(in super::super::super::super::super) fn row_recovery_cleanup(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<row_group::Observation> {
        let class = match operation {
            row_semantic::Operation::Cancel | row_semantic::Operation::Prune => {
                source::OperationClass::RowWrite
            }
            _ => source::OperationClass::RowProgress,
        };
        self.progress_lifecycle(owner, class, flusher, |writer, owner| {
            writer.check_closed_row_cleanup(ordinal, operation)?;
            let prepared = writer
                .catalog_preparation()
                .prepare_row_recovery(owner, ordinal, operation, private)?;
            writer.check_closed_row_cleanup(ordinal, operation)?;
            let observation = prepared.payload.observation;
            writer
                .catalog_application()
                .apply_existing(owner, ExistingGroup::RowRecovery(&prepared))?;
            Ok(observation)
        })
    }

    /// The recovered full RollbackStart (Active or AbortingStatement) or
    /// AbortEnd (AbortingFull, no next) of a Row loser: the original
    /// `row_rollback_control` inside the one RowProgress gate/progress shell.
    pub(in super::super::super::super::super) fn row_recovery_terminal(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<row_group::Observation> {
        if !matches!(
            operation,
            row_semantic::Operation::RollbackStart | row_semantic::Operation::AbortEnd
        ) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            ));
        }
        self.progress_lifecycle(
            owner,
            source::OperationClass::RowProgress,
            flusher,
            |writer, owner| writer.row_rollback_control(owner, actor, None, operation, private),
        )
    }
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(in super::super::super::super::super) fn row_recovery_cleanup(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<row_group::Observation> {
        let (mut writer, _, mut flusher) = self.split();
        writer.row_recovery_cleanup(owner, ordinal, operation, private, &mut flusher)
    }
    pub(in super::super::super::super::super) fn row_recovery_terminal(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        operation: row_semantic::Operation,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<row_group::Observation> {
        let (mut writer, _, mut flusher) = self.split();
        writer.row_recovery_terminal(owner, actor, operation, private, &mut flusher)
    }
}
