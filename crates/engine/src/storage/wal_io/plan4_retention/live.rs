//! Serialized N=0 groups. All candidate checking and codec errors precede the
//! stage; after a complete stage the original owner retains the requirement
//! before the first page, descriptor or DPT publication.
use super::control::{self, ACTOR, AGING_SLOT, LOCATION, PHASE, STABLE, STAGED};
use super::*;
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry},
        page::PageKind,
        retention_page::{JobSpec, JobState},
        semantic_group::{self, RollbackStartExpected},
        semantic_retention::{self, RetentionAfter, RetentionBefore, RetentionExpected},
        status_home,
        transaction_record::TransactionStatusCell,
        wal_frame::FrameExpectation,
        wal_io::{DirtyInstallPermit, WalPrefix},
    },
    transaction::{TxnId, concurrency::PageConcurrency},
};
use foundation::identity::{Generation, ObjectId};

/// Sealed only after stage_verified_required returns and pending owns its proof.
pub(in crate::storage) struct RetentionInstall<'a, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    stamp: FrameStamp,
    requirement: WalRequirement<'s>,
    after: &'a [u8; PAGE_BYTES],
}
impl<'a, 'c, 's> RetentionInstall<'a, 'c, 's> {
    pub(super) fn allocation(
        core: &'c PageConcurrency<'s>,
        stamp: FrameStamp,
        requirement: WalRequirement<'s>,
        after: &'a [u8; PAGE_BYTES],
    ) -> Self {
        Self {
            core,
            stamp,
            requirement,
            after,
        }
    }
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn stamp(&self) -> FrameStamp {
        self.stamp
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn after(&self) -> &'a [u8; PAGE_BYTES] {
        self.after
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    Aging,
    Begin,
    Rollback,
    Abort,
    Finish,
}
struct Selected {
    actor: Option<TxnRef>,
    status: Option<StatusSlotRef>,
    job: Option<JobSlotRef>,
    slot: Option<usize>,
}
fn request(identity: PageIdentity, kind: PageKind) -> TargetRequest {
    TargetRequest {
        identity,
        kind,
        level: 0,
    }
}

impl<'s, 'c, W: WriterNamespace> RetentionWriter<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W> {
    fn enter(&self, owner: &Owner<'c, 's>) -> Result<WalPrefix> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if self.control.pending.is_some() || self.control.population.reserved() {
            return Err(fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        self.storage.wal().prefix().map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Wal(e),
            )
        })
    }
    pub(crate) fn aging_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<RetentionObservation> {
        self.change(owner, Change::Aging, None, None, None)
    }
    pub(crate) fn begin_empty(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        job: JobSlotRef,
    ) -> Result<RetentionObservation> {
        self.change(owner, Change::Begin, Some(status.request), None, Some(job))
    }
    pub(crate) fn rollback_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
    ) -> Result<RetentionObservation> {
        self.change(owner, Change::Rollback, None, Some(actor), None)
    }
    pub(crate) fn abort_end_empty(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
    ) -> Result<RetentionObservation> {
        self.change(owner, Change::Abort, None, Some(actor), None)
    }
    pub(crate) fn finish_empty_terminal(
        &mut self,
        owner: &mut Owner<'c, 's>,
        job: JobSlotRef,
    ) -> Result<RetentionObservation> {
        self.change(owner, Change::Finish, None, None, Some(job))
    }
    fn change(
        &mut self,
        owner: &mut Owner<'c, 's>,
        change: Change,
        supplied: Option<TargetRequest>,
        actor: Option<TxnRef>,
        supplied_job: Option<JobSlotRef>,
    ) -> Result<RetentionObservation> {
        // The legacy N=0 producer owns four scratch pages starting at the
        // old Job tail. Profile2 uses that first page for its persistent XOI
        // cache and must enter the separately checked scope2 producer.
        if self.storage.profile().format() != 1 {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            ));
        }
        let runtime = self.history;
        let mut scope = runtime.replay_scope(owner)?;
        let mut replay = Some(scope.checkout()?);
        let result =
            self.change_with_replay(owner, change, supplied, actor, supplied_job, &mut replay);
        let returned = match replay.take() {
            Some(loan) => loan.return_checked(),
            None => Ok(()),
        };
        merge(merge(result, returned), scope.finish_checked())
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "original operation with its checked replay loan"
    )]
    fn change_with_replay(
        &mut self,
        owner: &mut Owner<'c, 's>,
        change: Change,
        supplied: Option<TargetRequest>,
        actor: Option<TxnRef>,
        supplied_job: Option<JobSlotRef>,
        replay: &mut Option<history::runtime::ReplayLoan<'_, '_, '_, '_>>,
    ) -> Result<RetentionObservation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let prefix = self.enter(owner)?;
        if change == Change::Begin {
            self.control.issued_local = self
                .control
                .issued_local
                .checked_add(1)
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?;
        }
        // This uses the original durable source, and ends its File guard before
        // any target pin. It checks the complete ATT/Status/Job bijection.
        history::validate_live(
            storage,
            self.control,
            replay.as_mut().expect("checked checkout").bytes_mut(),
            prefix,
        )?;
        let (remaining, steps) = self
            .control
            .check_ledger(storage.profile(), prefix)
            .map_err(|e| fatal(storage, RetentionPhase::Census, e.cause))?;
        let extra = if change == Change::Begin {
            semantic_retention::BEGIN_ADMISSION_WAL
        } else if change == Change::Aging {
            crate::storage::retention_obligation::AGING_START_BYTES as u64
        } else {
            0
        };
        if prefix
            .complete_offset
            .checked_add(remaining)
            .and_then(|v| v.checked_add(extra))
            .is_none_or(|v| v > crate::storage::wal_io::WAL_FILE_CAP)
            || prefix
                .complete_lsn
                .checked_add(steps)
                .and_then(|v| v.checked_add(if change == Change::Begin { 4 } else { 1 }))
                .is_none()
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
        }
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = self.fixed_request(1)?;
        if change == Change::Aging {
            return self.apply(
                owner,
                prefix,
                change,
                Selected {
                    actor: None,
                    status: None,
                    job: None,
                    slot: None,
                },
                [allocator, root],
                replay,
            );
        }
        let slot = if change == Change::Begin {
            Some(
                self.control
                    .free_slot()
                    .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))?,
            )
        } else if change == Change::Finish {
            None
        } else {
            Some(
                self.control
                    .slot(actor.ok_or_else(|| {
                        unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
                    })?)
                    .ok_or_else(|| {
                        unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
                    })?,
            )
        };
        let (actor, status, job, target) = if change == Change::Begin {
            let target = supplied
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Membership))?;
            self.status_member(
                target,
                allocator.identity,
                replay.as_ref().expect("checked checkout").bytes(),
            )?;
            let bytes = history::arena_page(
                replay.as_ref().expect("checked checkout").bytes(),
                target.identity.page.page_no as usize,
            );
            let page = status_home::decode_status_home_page(
                bytes,
                target.identity,
                storage.profile().spec().namespace,
            )
            .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
            let home = page
                .cell(0)
                .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
            let TransactionStatusCell::Free(g) = home.core() else {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::WrongState,
                ));
            };
            let generation =
                Generation::new(g.get().checked_add(1).ok_or_else(|| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted)
                })?)
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?;
            let actor = TxnRef::new(
                TxnId {
                    database: prefix.stream.database,
                    timeline: prefix.stream.timeline,
                    writer_epoch: prefix.stream.writer_epoch,
                    local: self.control.issued_local,
                },
                generation,
            )
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            let status = StatusSlotRef::new(target.identity.page, 0, generation)
                .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            (
                actor,
                status,
                supplied_job.ok_or_else(|| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::Membership)
                })?,
                target,
            )
        } else {
            let job = match supplied_job {
                Some(job) => job,
                None => self.find_job(actor.expect("non-begin actor"))?,
            };
            let value = self.job(job)?;
            let (source, status) = match value.state {
                JobState::Leased { source, status, .. } if change != Change::Finish => {
                    (source, status)
                }
                JobState::Ready { source, status, .. } if change == Change::Finish => {
                    (source, status)
                }
                _ => {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::WrongState,
                    ));
                }
            };
            if actor.is_some_and(|a| a != source) {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Membership,
                ));
            }
            let target = request(
                PageIdentity {
                    database: allocator.identity.database,
                    page: status.page(),
                    owner: ObjectId::new(2).expect("Status object"),
                    owner_generation: Generation::new(1).expect("Status owner generation"),
                },
                PageKind::TransactionStatus,
            );
            self.status_member(
                target,
                allocator.identity,
                replay.as_ref().expect("checked checkout").bytes(),
            )?;
            (source, status, job, target)
        };
        let jobs = self.fixed_request(job.page().page_no as usize)?;
        self.apply(
            owner,
            prefix,
            change,
            Selected {
                actor: Some(actor),
                status: Some(status),
                job: Some(job),
                slot,
            },
            [allocator, root, jobs, target],
            replay,
        )
    }
    fn fixed_request(&self, index: usize) -> Result<TargetRequest> {
        let h =
            super::super::genesis::owned_header(&self.storage.core.plan, index).map_err(|_| {
                fatal(
                    self.storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                )
            })?;
        Ok(request(h.identity, h.kind))
    }
    fn status_member(
        &self,
        target: TargetRequest,
        allocator: PageIdentity,
        replay: &[u8],
    ) -> Result<()> {
        let page = target.identity.page.page_no as usize;
        if !(9..64).contains(&page)
            || target.kind != PageKind::TransactionStatus
            || target.level != 0
            || target.identity.database != allocator.database
            || target.identity.page.file != allocator.page.file
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        let view = allocation_page::decode_allocation_page(
            history::arena_page(replay, 0),
            allocator,
            self.storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(e),
            )
        })?;
        if view.entry(page).map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(e),
            )
        })? != (AllocationEntry::Allocated {
            generation: target.identity.page.incarnation,
            object: target.identity.owner,
            object_generation: target.identity.owner_generation,
            kind: target.kind,
        }) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        Ok(())
    }
    fn job(&self, job: JobSlotRef) -> Result<JobSpec> {
        let ordinal = job
            .ordinal()
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Retention(e)))?;
        let value = retention_page::decode_job(
            &self.control.jobs()[ordinal],
            self.storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Retention(e),
            )
        })?
        .spec();
        if value.generation != job.generation() {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        Ok(value)
    }
    fn find_job(&self, actor: TxnRef) -> Result<JobSlotRef> {
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let j = retention_page::decode_job(bytes, self.storage.profile().spec().namespace)
                .map_err(|e| {
                    fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Retention(e),
                    )
                })?
                .spec();
            if matches!(j.state, JobState::Leased { source, .. } | JobState::Ready { source, .. } if source == actor)
            {
                let page = self.fixed_request(2 + ordinal / 15)?.identity.page;
                return JobSlotRef::new(page, (ordinal % 15) as u16, j.generation).map_err(|e| {
                    fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Retention(e),
                    )
                });
            }
        }
        Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::MissingTransaction,
        ))
    }
    #[allow(
        clippy::collapsible_if,
        reason = "keep guarded readback validation distinct from the operation selector"
    )]
    fn apply<const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        change: Change,
        selected: Selected,
        requests: [TargetRequest; N],
        replay: &mut Option<history::runtime::ReplayLoan<'_, '_, '_, '_>>,
    ) -> Result<RetentionObservation> {
        let storage = self.storage;
        let mut stamps = [None; 4];
        {
            for request in requests {
                if self
                    .pool
                    .resident_request(request)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?
                    .is_none()
                {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::NotResident,
                    ));
                }
            }
            let read = self
                .pool
                .reserve_target_set(owner, &requests)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?
                .try_read()
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let copied = (|| {
                for (at, target) in requests.iter().enumerate() {
                    let out = &mut control::before_mut(self.control.bytes)[at];
                    stamps[at] = Some(
                        read.copy_snapshot(at, out)
                            .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                    );
                    if out
                        != history::arena_page(
                            replay.as_ref().expect("checked checkout").bytes(),
                            target.identity.page.page_no as usize,
                        )
                    {
                        return Err(fatal(
                            storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        ));
                    }
                }
                Ok(())
            })();
            merge(
                copied,
                read.release_checked()
                    .map_err(|e| frame(RetentionPhase::Prepare, e)),
            )?;
        }
        // Canonical bytes have been copied and all read guards released. No
        // replay checkout survives stage, durability or a future continuation.
        replay.take().expect("checked checkout").return_checked()?;
        let mut actor_bytes = [0; 48];
        let mut status_bytes = [0; 40];
        if let Some(actor) = selected.actor {
            actor
                .encode(&mut actor_bytes)
                .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Page(e)))?;
        }
        if let Some(status) = selected.status {
            status
                .encode(&mut status_bytes)
                .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Page(e)))?;
        }
        if change == Change::Begin {
            // Prepay shared root/job revision and RAM-counter headroom for
            // every outstanding terminal, not merely this Begin's next tick.
            let mut root_steps = 3u64;
            let mut page_steps = 3u64;
            for (ordinal, cell) in self.control.jobs().iter().enumerate() {
                let value = retention_page::decode_job(cell, storage.profile().spec().namespace)
                    .map_err(|e| {
                        fatal(
                            storage,
                            RetentionPhase::Prepare,
                            RetentionCause::Retention(e),
                        )
                    })?
                    .spec();
                let steps = match value.state {
                    JobState::Free => 0,
                    JobState::Leased { .. } => 2,
                    JobState::Ready { .. } => 1,
                };
                root_steps += steps;
                if ordinal / 15 == (requests[2].identity.page.page_no as usize - 2) {
                    page_steps += steps;
                }
            }
            let (_, root) = retention_page::decode_root_page(
                &control::before_mut(self.control.bytes)[1],
                storage.profile().spec().namespace,
            )
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Retention(e),
                )
            })?;
            if root.spec().revision.checked_add(root_steps).is_none()
                || stamps[1]
                    .expect("root")
                    .modification_counter
                    .checked_add(root_steps)
                    .is_none()
                || stamps[2]
                    .expect("jobs")
                    .modification_counter
                    .checked_add(page_steps)
                    .is_none()
                || stamps[3]
                    .expect("Status")
                    .modification_counter
                    .checked_add(4)
                    .is_none()
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Exhausted,
                ));
            }
        }
        let (frame_space, after_space) = self.operation.split_at_mut(FRAME_BYTES);
        let after: &mut [[u8; PAGE_BYTES]; 3] = after_space
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("three after images");
        let (bank, before) = self.control.bytes.split_at_mut(131_072);
        let before = before.as_chunks::<PAGE_BYTES>().0;
        let before_sender: Option<&[u8; 1536]> = selected.slot.map(|i| {
            control::row(bank, i)[..1536]
                .try_into()
                .expect("descriptor")
        });
        let aging = (control::row(bank, AGING_SLOT)[PHASE] != 0).then(|| {
            control::row(bank, AGING_SLOT)[..1536]
                .try_into()
                .expect("aging descriptor")
        });
        let expected = RetentionExpected {
            frame: FrameExpectation {
                stream: prefix.stream,
                previous_lsn: prefix.complete_lsn,
            },
            profile: storage.profile(),
            actor: selected.actor,
            status: selected.status,
            status_page: (N == 4).then(|| requests[3].identity),
            job: selected.job,
            previous_local: if change == Change::Begin {
                self.control.adopted_local
            } else {
                0
            },
        };
        let (header, length) = if change == Change::Rollback {
            let value = semantic_group::produce_rollback_start(
                RollbackStartExpected {
                    frame: expected.frame,
                    profile: expected.profile,
                    actor: selected.actor.expect("actor"),
                    location: selected.status.expect("status"),
                    page: requests[3].identity,
                },
                prefix.complete_lsn + 1,
                before_sender.expect("retained descriptor"),
                &before[3],
                (&mut frame_space[..semantic_retention::ROLLBACK_BYTES])
                    .try_into()
                    .expect("exact RollbackStart frame"),
                &mut after[2],
            )
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Rollback(e),
                )
            })?;
            (value.header(), semantic_retention::ROLLBACK_BYTES)
        } else {
            let [root_after, jobs_after, status_after] = after;
            let input = RetentionBefore {
                root: &before[1],
                jobs: (N == 4).then(|| &before[2]),
                status: (N == 4).then(|| &before[3]),
                sender: before_sender.filter(|_| change != Change::Begin),
                aging,
                job_cells: control::jobs(bank),
            };
            let output = RetentionAfter {
                root: root_after,
                jobs: (N == 4).then_some(jobs_after),
                status: (N == 4).then_some(status_after),
            };
            let value = match change {
                Change::Aging => semantic_retention::produce_aging_start(
                    expected,
                    input,
                    output,
                    &mut frame_space[..crate::storage::retention_obligation::AGING_START_BYTES],
                ),
                Change::Begin => semantic_retention::produce_begin(
                    expected,
                    input,
                    output,
                    &mut frame_space[..semantic_retention::HANDOFF_BYTES],
                ),
                Change::Abort => semantic_retention::produce_abort_end(
                    expected,
                    input,
                    output,
                    &mut frame_space[..semantic_retention::HANDOFF_BYTES],
                ),
                Change::Finish => semantic_retention::produce_finish(
                    expected,
                    input,
                    output,
                    &mut frame_space[..retention_page::FINISH_BYTES],
                ),
                Change::Rollback => unreachable!("separate unchanged rollback grammar"),
            }
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Semantic(e),
                )
            })?;
            (value.header(), value.bytes().len())
        };
        let mut write = self
            .pool
            .reserve_target_set(owner, &requests)
            .map_err(|e| frame(RetentionPhase::Install, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Install, e))?;
        let installed = (|| {
            for (at, stamp) in stamps.iter().enumerate().take(N) {
                write
                    .matches_stamp(at, &stamp.expect("copied stamp"))
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
            }
            let mut dpt = [None, None, None];
            for at in 1..N {
                if change != Change::Rollback || at == 3 {
                    dpt[at - 1] = Some(
                        write
                            .preflight_dpt(at, &stamps[at].expect("stamp"))
                            .map_err(|e| frame(RetentionPhase::Install, e))?,
                    );
                }
            }
            let requirement = storage
                .wal()
                .stage_verified_required(prefix, header, &frame_space[..length])
                .map_err(|e| RetentionError {
                    phase: RetentionPhase::Stage,
                    effect: e.effect,
                    cause: RetentionCause::Wal(e),
                })?;
            self.control.pending = Some(requirement);
            if let Some(slot) = selected.slot {
                let row = control::row_mut(bank, slot);
                row[LOCATION].copy_from_slice(&status_bytes);
                row[ACTOR].copy_from_slice(&actor_bytes);
                row[PHASE] = STAGED;
            }
            if change == Change::Begin {
                self.control.adopted_local = selected.actor.expect("actor").id().local;
            }
            for at in 1..N {
                if change == Change::Rollback && at != 3 {
                    continue;
                }
                let permit = DirtyInstallPermit::from_retention(RetentionInstall {
                    core: self.pool.bound_core(),
                    stamp: stamps[at].expect("stamp"),
                    requirement,
                    after: &after[at - 1],
                });
                let permit = crate::storage::wal_io::disk_api::legacy_install(permit);
                write
                    .install_page(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                write
                    .install_metadata(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                write
                    .install_dpt(dpt[at - 1].take().expect("preflight DPT"), &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
            }
            if change != Change::Rollback {
                let aging_at = if matches!(change, Change::Begin | Change::Abort) {
                    1952
                } else {
                    352
                };
                let row = control::row_mut(bank, AGING_SLOT);
                row[..1536].copy_from_slice(&frame_space[aging_at..aging_at + 1536]);
                row[PHASE] = STABLE;
                if N == 4 {
                    let view = retention_page::decode_jobs_page(
                        &after[1],
                        storage.profile().spec().namespace,
                        (requests[2].identity.page.page_no - 2) as u8,
                    )
                    .map_err(|e| {
                        fatal(
                            storage,
                            RetentionPhase::Install,
                            RetentionCause::Retention(e),
                        )
                    })?;
                    let base = (requests[2].identity.page.page_no as usize - 2) * 15;
                    for i in 0..view.len() {
                        control::jobs_mut(bank)[base + i]
                            .copy_from_slice(view.cell(i).expect("validated job cell"));
                    }
                }
            }
            if let Some(slot) = selected.slot {
                let at = if change == Change::Rollback { 352 } else { 416 };
                let row = control::row_mut(bank, slot);
                row[..1536].copy_from_slice(&frame_space[at..at + 1536]);
            }
            Ok(())
        })();
        merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
        .map_err(|e| {
            if self.control.pending.is_some() {
                fatal(storage, RetentionPhase::Install, e.cause)
            } else {
                e
            }
        })?;
        #[cfg(test)]
        super::tests::after_install(self.pool);
        let requirement = self.control.pending.expect("stage retained");
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
        // Post-force re-pin performs no source or page I/O and checks every
        // installed image, including all three atomic group targets.
        let read = self
            .pool
            .reserve_target_set(owner, &requests)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?
            .try_read()
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Frame(e)))?;
        let checked = (|| {
            for at in 1..N {
                if change != Change::Rollback || at == 3 {
                    if read
                        .bytes(at)
                        .map_err(|e| frame(RetentionPhase::Durable, e))?
                        != &after[at - 1]
                    {
                        return Err(fatal(
                            storage,
                            RetentionPhase::Durable,
                            RetentionCause::Invariant,
                        ));
                    }
                }
            }
            Ok(())
        })();
        merge(
            checked,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Durable, e)),
        )
        .map_err(|e| fatal(storage, RetentionPhase::Durable, e.cause))?;
        storage
            .health()
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Storage(e)))?;
        if let Some(slot) = selected.slot {
            if change == Change::Abort {
                control::row_mut(bank, slot).fill(0);
            } else {
                control::row_mut(bank, slot)[PHASE] = STABLE;
            }
        }
        self.control.durable = Some(durable);
        self.control.pending = None;
        self.history.installed(requirement)?;
        Ok(RetentionObservation {
            actor: selected.actor,
            status: selected.status,
            job: selected.job,
            lsn: PageLsn(header.lsn),
            issued_local: self.control.issued_local,
            adopted_local: self.control.adopted_local,
        })
    }
}
