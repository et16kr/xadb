//! The original MutationPort's structural group. This is the only factory of
//! the pool capabilities below; bytes or a staged WAL receipt cannot mint them.
use super::*;
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    dirty_frame::{CompletedStructural, StructuralClaimKey, StructuralWriteTargets},
};

impl<'c, 's> GroupPayload<'c, 's> for PreparedStructural<'_, 'c, 's> {
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    fn prefix(&self) -> WalPrefix {
        self.prefix
    }
    fn lsn(&self) -> u64 {
        self.observation.header.lsn
    }
    fn frame(&self) -> &[u8] {
        self.frame
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.frame_digest
    }
    fn cut(&self) -> ControlCut {
        self.control
    }
}

pub(in crate::storage) struct StructuralAuthority<'i, 'c, 's> {
    prepared: &'i PreparedStructural<'i, 'c, 's>,
    work: SystemWorkRef,
    init: [TargetRequest; 2],
    count: usize,
}
impl<'i, 'c, 's> StructuralAuthority<'i, 'c, 's> {
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.prepared.core
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.prepared.binding
    }
    pub(in crate::storage) fn fence(&self) -> &'s SessionFence {
        self.prepared.fence
    }
    pub(in crate::storage) fn work(&self) -> SystemWorkRef {
        self.work
    }
    pub(in crate::storage) fn birth(&self) -> PageLsn {
        PageLsn(self.prepared.lsn())
    }
    pub(in crate::storage) fn init(&self) -> &[TargetRequest; 2] {
        &self.init
    }
    pub(in crate::storage) fn count(&self) -> usize {
        self.count
    }
    pub(in crate::storage) fn targets(&self) -> &[TargetRequest; 5] {
        &self.prepared.requests
    }
}

/// Continuous exclusive ownership of the selected original DWB control.
struct StructuralIoLease<'r, 'c, 's, 'w> {
    original: &'r mut flush::RetentionFlushControl<'c, 's, 'w>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    core: &'c PageConcurrency<'s>,
    selected_generation: u64,
    init: [TargetRequest; 2],
    count: usize,
}
impl<'r, 'c, 's, 'w> StructuralIoLease<'r, 'c, 's, 'w> {
    fn acquire<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, 'w, W>,
        original: &'r mut flush::RetentionFlushControl<'c, 's, 'w>,
        authority: &StructuralAuthority<'_, 'c, 's>,
    ) -> Result<Self> {
        original.check_catalog_pair_io::<W>(&storage.binding, storage.session_fence())?;
        Ok(Self {
            selected_generation: original.io.snapshot().known.generation,
            original,
            binding: &storage.binding,
            fence: storage.session_fence(),
            core: authority.core(),
            init: authority.init,
            count: authority.count,
        })
    }
    fn check<W: WriterNamespace>(
        &mut self,
        authority: &StructuralAuthority<'_, 'c, 's>,
    ) -> Result<()> {
        if !std::ptr::eq(self.core, authority.core())
            || !std::ptr::eq(self.binding, authority.binding())
            || !std::ptr::eq(self.fence, authority.fence())
            || self.init != authority.init
            || self.count != authority.count
            || self.selected_generation != self.original.io.snapshot().known.generation
        {
            self.fence.fence();
            return Err(RetentionError {
                phase: RetentionPhase::Prepare,
                effect: Effect::CallerMustResolve,
                cause: RetentionCause::BindingMismatch,
            });
        }
        self.original
            .check_catalog_pair_io::<W>(self.binding, self.fence)
    }
}

pub(in crate::storage) struct StructuralStageRejection<'i, 'c, 's> {
    key: &'i StructuralClaimKey<'c, 's>,
}
impl<'i, 'c, 's> StructuralStageRejection<'i, 'c, 's> {
    pub(in crate::storage) fn key(&self) -> &'i StructuralClaimKey<'c, 's> {
        self.key
    }
}

pub(in crate::storage) struct StructuralInstallPermit<'i, 'c, 's> {
    key: &'i StructuralClaimKey<'c, 's>,
    prepared: &'i PreparedStructural<'i, 'c, 's>,
    requirement: WalRequirement<'s>,
}
impl<'i, 'c, 's> StructuralInstallPermit<'i, 'c, 's> {
    pub(in crate::storage) fn key(&self) -> &'i StructuralClaimKey<'c, 's> {
        self.key
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn targets(&self) -> &[TargetRequest; 5] {
        &self.prepared.requests
    }
    pub(in crate::storage) fn before(&self) -> &[Option<FrameStamp>; 5] {
        &self.prepared.stamps
    }
    pub(in crate::storage) fn after(&self, at: usize) -> &[u8; PAGE_BYTES] {
        &self.prepared.after[usize::from(self.prepared.roles[at])]
    }
}

pub(in crate::storage) struct StructuralControlCommit<'i, 'a, 'c, 's> {
    authority: &'i StructuralAuthority<'i, 'c, 's>,
    original: &'i mut control::RetentionLedger<'a, 'c, 's>,
    requirement: WalRequirement<'s>,
    descriptor: &'i [u8; 1536],
    applied: bool,
}
impl StructuralControlCommit<'_, '_, '_, '_> {
    pub(in crate::storage) fn apply(
        &mut self,
        key: &StructuralClaimKey<'_, '_>,
        requirement: WalRequirement<'_>,
    ) -> dirty_frame::Result<()> {
        let bad = self.applied
            || !key.matches_authority(self.authority)
            || !requirement.uses_fence(self.authority.fence())
            || requirement.location() != self.requirement.location()
            || check_staged_group(self.authority.prepared, self.original, requirement).is_err();
        if bad {
            self.authority.fence().fence();
            return Err(dirty_frame::DirtyError {
                cause: dirty_frame::Cause::Invariant,
                effect: Effect::CallerMustResolve,
            });
        }
        let row = self.original.row_mut(control::STRUCTURAL_SLOT);
        row.fill(0);
        row[..1536].copy_from_slice(self.descriptor);
        row[1536..1544].copy_from_slice(&self.authority.work.id().to_le_bytes());
        row[control::PHASE] = control::STABLE;
        self.original.bytes[JOB_END..INVENTORY_END]
            .copy_from_slice(self.authority.prepared.inventory_after);
        self.applied = true;
        Ok(())
    }
}

/// Only the prestage path can create this, while its original I/O lease is
/// still borrowed and the allocator has the immutable preparation stamp.
pub(in crate::storage) struct StructuralCancelPermit<'i, 'c, 's, 'w> {
    authority: &'i StructuralAuthority<'i, 'c, 's>,
    original: &'i flush::RetentionFlushControl<'c, 's, 'w>,
    selected_generation: u64,
    allocator_before: FrameStamp,
}
impl<'i, 'c, 's> StructuralCancelPermit<'i, 'c, 's, '_> {
    pub(in crate::storage) fn authority(&self) -> &'i StructuralAuthority<'i, 'c, 's> {
        self.authority
    }
    pub(in crate::storage) fn allocator_before(&self) -> FrameStamp {
        self.allocator_before
    }
    pub(in crate::storage) fn io_matches(&self, binding: &AtomicU8, fence: &SessionFence) -> bool {
        std::ptr::eq(self.authority.binding(), binding)
            && std::ptr::eq(self.authority.fence(), fence)
            && self.selected_generation != 0
            && self.original.io.snapshot().known.generation == self.selected_generation
            && self.original.phase == flush::RetentionFlushPhase::Ready
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[cfg(test)]
    pub(in crate::storage) fn test_structural_prefix(&self) -> WalPrefix {
        self.storage.wal().prefix().unwrap()
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_structural_provenance(&self) -> Vec<u8> {
        self.history.test_structural_provenance()
    }
    /// Copy an original resident target; normal flush still checks its source.
    #[cfg(test)]
    pub(in crate::storage) fn test_structural_target_at(&self, at: usize) -> Plan4Target {
        Plan4Target {
            request: self.pool.target_request(at).unwrap(),
        }
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_structural_pending(&self) -> bool {
        self.control.pending.is_some()
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_structural_fault(&self, kind: u8) -> fault_support::Fault {
        fault_support::arm(kind)
    }
    /// Private structural system operation. It refuses the table's
    /// RootReserved Row root and Catalog debt (`split_row_admitted`).
    pub(in crate::storage) fn split_row(
        &mut self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        target: PageRef,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<split_frame::Observation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        storage
            .health()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Storage(e)))?;
        self.pool
            .check_retention_entry_health()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if !self
            .pool
            .uses_storage_origin(&storage.binding, storage.session_fence())
        {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        if !std::ptr::eq(self.pool, flusher.pool) || !std::ptr::eq(storage, flusher.storage) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let gate = owner
            .close_reads_for_retirement(self.pool.bound_core())
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                cause: RetentionCause::Concurrency(e),
                effect: e.effect,
            })?;
        let reopen = |gate: crate::transaction::concurrency::TerminalReadGate<'c, 's>| {
            gate.reopen_checked().map_err(|e| RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::Concurrency(e),
                effect: e.effect,
            })
        };
        let source = match source::OperationSourceAdmission::catalog_progress(
            self.isolation,
            &self.commit_charges,
            &self.progress_charge,
            &gate,
        ) {
            Ok(source) => source,
            Err(e) => return merge(Err(RetentionError::from(e)), reopen(gate)),
        };
        let result = self.split_row_admitted(owner, table, target, private, flusher);
        let returned = source.finish_checked().map_err(RetentionError::from);
        merge(merge(result, returned), reopen(gate))
    }

    #[inline(never)]
    fn split_row_admitted(
        &mut self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        target: PageRef,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<split_frame::Observation> {
        let storage = self.storage;
        let prepared = self.prepare_row_split(owner, table, target, private)?;
        // Same-cut canonical preparation proved the full tree and original
        // Jobs, and its weights count each Retained root's restore bytes. The
        // table's Row debt past its Forward (Retained, releasing, terminal)
        // is admitted: every current-leaf consumer routes its row
        // (dev3-tree-smo.md#current-leaf-consumers). A RootReserved Row root
        // is refused, since its Forward writes the pair[1] leaf its Reserve
        // found current, and so is any non-Free Catalog Job of the table,
        // whose split-tree semantics are schedule 09/10's.
        for bytes in self.control.jobs() {
            use crate::storage::retention_page::row::{JobScope, JobState};
            let job = crate::storage::retention_page::row::decode_job(
                bytes,
                storage.profile().spec().namespace,
            )
            .map_err(queue_error)?;
            if job.object().is_some_and(|object| {
                object.object == table.object && object.generation == table.generation
            }) && (job.state() == JobState::RootReserved
                || job.scope() == Some(JobScope::Catalog) && job.state() != JobState::Free)
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
        }
        let work = SystemWorkRef::new(
            storage.profile().spec().namespace,
            SystemWorkKind::StructuralChange,
            prepared.lsn(),
            Generation::new(u64::from(prepared.prefix.stream.writer_epoch))
                .map_err(|_| semantic(row::Error::IdentityMismatch))?,
        )
        .map_err(|_| semantic(row::Error::IdentityMismatch))?;
        let count = match prepared.observation.operation {
            smo::Operation::RootSplit => 2,
            smo::Operation::ChildSplit => 1,
        };
        let right = request(prepared.observation.targets[3]);
        let authority = StructuralAuthority {
            prepared: &prepared,
            work,
            init: [
                right,
                if count == 2 {
                    request(prepared.observation.targets[4])
                } else {
                    right
                },
            ],
            count,
        };
        let mut lease = StructuralIoLease::acquire(storage, &mut *flusher.control, &authority)?;
        #[cfg(test)]
        if fault_support::take(1) {
            let next = lease.original.io.snapshot().known.generation + 1;
            super::super::super::super::super::super::flush_io::test_support::seed_headroom(
                &mut lease.original.io,
                None,
                None,
                Some(next),
            );
        }
        let mut key = {
            let selected = self
                .pool
                .structural_candidates(&authority.init[..count], 1, &prepared.requests)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let prefix = storage
                .wal()
                .prefix()
                .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
            check_group(
                &prepared,
                prepared.owner,
                owner,
                self.pool,
                self.control,
                prefix,
            )?;
            lease.check::<W>(&authority)?;
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralSelected,
            );
            #[cfg(test)]
            if fault_support::take(3) {
                dirty_frame::init_test_support::advance_selected_tombstone(self.pool, &selected);
            }
            #[cfg(test)]
            if fault_support::take(6) {
                dirty_frame::init_test_support::advance_selected_clean_counter(
                    self.pool, &selected,
                );
            }
            self.pool
                .prepare_structural(&authority, &selected)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?
        };
        #[cfg(test)]
        dirty_frame::init_test_support::structural_hit(
            self.pool,
            dirty_frame::init_test_support::InitPoint::StructuralClaimed,
        );
        let mut staged = false;
        let result = self.install_structural(owner, &prepared, &authority, &key, &mut staged);
        let (requirement, completed) = match result {
            Ok(completed) => completed,
            Err(e) if !staged && e.effect == Effect::Unchanged => {
                let cancel = (|| {
                    #[cfg(test)]
                    dirty_frame::init_test_support::structural_hit(
                        self.pool,
                        dirty_frame::init_test_support::InitPoint::StructuralCancelChecking,
                    );
                    #[cfg(test)]
                    if fault_support::take(2) {
                        lease.original.phase = flush::RetentionFlushPhase::WaitingSource;
                    }
                    lease.check::<W>(&authority)?;
                    let allocator = prepared
                        .roles
                        .iter()
                        .position(|role| *role == 0)
                        .expect("allocator role");
                    let permit = StructuralCancelPermit {
                        authority: &authority,
                        original: &*lease.original,
                        selected_generation: lease.selected_generation,
                        allocator_before: prepared.stamps[allocator].expect("existing allocator"),
                    };
                    self.pool
                        .cancel_structural(&mut key, permit)
                        .map_err(|error| {
                            fatal(
                                storage,
                                RetentionPhase::Install,
                                RetentionCause::Frame(error),
                            )
                        })
                })()
                .map_err(|error: RetentionError| fatal(storage, error.phase, error.cause));
                #[cfg(test)]
                if cancel.is_ok() {
                    // Reuse of this actual consumed key must fail; the test
                    // does not construct a substitute claim or capability.
                    assert!(matches!(
                        self.pool.claim_structural_targets(owner, &key),
                        Err(error) if error.effect == Effect::Unchanged
                            && error.cause == dirty_frame::Cause::Stale
                    ));
                }
                return merge(Err(e), cancel);
            }
            Err(e) => return Err(fatal(storage, e.phase, e.cause)),
        };
        #[cfg(test)]
        dirty_frame::init_test_support::structural_hit(
            self.pool,
            dirty_frame::init_test_support::InitPoint::StructuralReleased,
        );
        // From here no Unchanged error can be reported as an ordinary refusal.
        let result = (|| {
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
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralDurable,
            );
            self.control.durable = Some(durable);
            let prefix = storage
                .wal()
                .prefix()
                .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
            if prefix.stream != requirement.location().stream
                || prefix.file != requirement.location().file
                || prefix.complete_lsn != requirement.location().lsn
                || prefix.complete_offset != requirement.location().end
            {
                return Err(fatal(
                    storage,
                    RetentionPhase::Durable,
                    RetentionCause::Invariant,
                ));
            }
            let mut audit = self.history.replay_scope(owner)?;
            #[cfg(test)]
            let blocked = fault_support::take(4).then(|| self.history.test_hold_replay());
            #[cfg(test)]
            if fault_support::take(5) {
                // Damage the original installed bank before canonical replay.
                self.control.bytes[34 * 1664 + 1536] ^= 1;
            }
            let replayed = match audit.checkout() {
                Ok(mut replay) => {
                    let checked =
                        history::validate_live(storage, self.control, replay.bytes_mut(), prefix);
                    #[cfg(test)]
                    dirty_frame::init_test_support::structural_hit(
                        self.pool,
                        dirty_frame::init_test_support::InitPoint::StructuralReplayReturning,
                    );
                    merge(checked, replay.return_checked())
                }
                Err(error) => Err(error),
            };
            #[cfg(test)]
            drop(blocked);
            merge(replayed, audit.finish_checked())?;
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralReplayChecked,
            );
            lease.check::<W>(&authority)?;
            self.pool
                .finish_structural(
                    &mut key,
                    completed,
                    self.control
                        .durable
                        .as_ref()
                        .expect("original durable receipt"),
                )
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            self.control.pending = None;
            Ok(prepared.observation)
        })();
        result.map_err(|e| fatal(storage, e.phase, e.cause))
    }

    #[inline(never)]
    fn install_structural(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: &PreparedStructural<'_, 'c, 's>,
        authority: &StructuralAuthority<'_, 'c, 's>,
        key: &StructuralClaimKey<'c, 's>,
        staged: &mut bool,
    ) -> Result<(WalRequirement<'s>, CompletedStructural<'c, 's>)> {
        let storage = self.storage;
        let view = smo::decode_layout(
            prepared.frame,
            FrameExpectation {
                stream: prepared.prefix.stream,
                previous_lsn: prepared.prefix.complete_lsn,
            },
            storage.profile(),
        )
        .map_err(semantic)?;
        let mut write = self
            .pool
            .claim_structural_targets(owner, key)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let result = (|| {
            for at in 0..5 {
                let actual = write
                    .observe(at)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?
                    .map(|o| o.frame);
                if actual != prepared.stamps[at]
                    || actual.is_some()
                        && write
                            .bytes(at)
                            .map_err(|e| frame(RetentionPhase::Prepare, e))?
                            != &prepared.before[usize::from(prepared.roles[at])]
                {
                    return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
                }
            }
            let prefix = storage
                .wal()
                .prefix()
                .map_err(|e| fatal(storage, RetentionPhase::Stage, RetentionCause::Wal(e)))?;
            unchanged_cut(prepared.prefix, prefix, prepared.control, self.control)?;
            write
                .enter_stage()
                .map_err(|e| frame(RetentionPhase::Stage, e))?;
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralStageEntered,
            );
            let requirement = match storage.wal().stage_verified_required(
                prepared.prefix,
                prepared.observation.header,
                prepared.frame,
            ) {
                Ok(requirement) => requirement,
                Err(e) if e.effect == Effect::Unchanged => {
                    write
                        .accept_stage_rejection(StructuralStageRejection { key })
                        .map_err(|e| frame(RetentionPhase::Stage, e))?;
                    return Err(RetentionError {
                        phase: RetentionPhase::Stage,
                        effect: e.effect,
                        cause: RetentionCause::Wal(e),
                    });
                }
                Err(e) => {
                    return Err(fatal(
                        storage,
                        RetentionPhase::Stage,
                        RetentionCause::Wal(e),
                    ));
                }
            };
            *staged = true;
            mark_staged_group(prepared, self.control, requirement)?;
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralStaged,
            );
            let permit = StructuralInstallPermit {
                key,
                prepared,
                requirement,
            };
            let control = StructuralControlCommit {
                authority,
                original: &mut *self.control,
                requirement,
                descriptor: view.descriptor(),
                applied: false,
            };
            let completed = write
                .install(&permit, control)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralHistoryInstalling,
            );
            self.history.installed(requirement)?;
            #[cfg(test)]
            dirty_frame::init_test_support::structural_hit(
                self.pool,
                dirty_frame::init_test_support::InitPoint::StructuralHistoryInstalled,
            );
            Ok((requirement, completed))
        })();
        merge(
            result,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        )
    }
}

#[cfg(test)]
mod fault_support {
    use std::{cell::Cell, marker::PhantomData, rc::Rc};
    thread_local! { static FAULT: Cell<(u8, u8)> = const { Cell::new((0, 0)) }; }
    pub(in crate::storage) struct Fault(PhantomData<Rc<()>>);
    pub(super) fn arm(kind: u8) -> Fault {
        assert!((1..=6).contains(&kind));
        FAULT.with(|s| assert_eq!(s.replace((kind, 0)), (0, 0)));
        Fault(PhantomData)
    }
    pub(super) fn take(kind: u8) -> bool {
        FAULT.with(|s| {
            if s.get().0 == kind {
                s.set((0, 1));
                true
            } else {
                false
            }
        })
    }
    impl Fault {
        pub(in crate::storage) fn hits(&self) -> u8 {
            FAULT.with(|s| s.get().1)
        }
    }
    impl Drop for Fault {
        fn drop(&mut self) {
            FAULT.with(|s| s.set((0, 0)));
        }
    }
}

#[derive(Debug)]
pub(in super::super::super::super) struct SourceCensus {
    pub(in super::super::super::super) entry: usize,
    pub(in super::super::super::super) prepared: usize,
    pub(in super::super::super::super) jobs: usize,
    pub(in super::super::super::super) selection: usize,
    pub(in super::super::super::super) installation: usize,
    pub(in super::super::super::super) replay: usize,
    pub(in super::super::super::super) returning: usize,
    pub(in super::super::super::super) peak: usize,
}

/// Source-value phases, including the original gate/admission and all retained
/// preparation/key/lease values. Physical buffers remain in the caller loan.
pub(in super::super::super::super) fn source_census<W: WriterNamespace>() -> SourceCensus {
    use std::mem::size_of;
    type Outcome<'a> = (WalRequirement<'a>, CompletedStructural<'a, 'a>);
    let entry = size_of::<crate::transaction::concurrency::TerminalReadGate<'_, '_>>()
        + size_of::<source::OperationSourceAdmission<'_, '_, '_, '_>>()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 3 * size_of::<Result<split_frame::Observation>>()
        + size_of::<ObjectHandle>()
        + size_of::<PageRef>()
        + 24 * size_of::<&()>()
        + 16 * size_of::<usize>()
        + 4 * size_of::<Result<()>>()
        + 3 * size_of::<RetentionError>();
    let owned = entry
        + size_of::<PreparedStructural<'_, '_, '_>>()
        + size_of::<StructuralAuthority<'_, '_, '_>>()
        + size_of::<StructuralIoLease<'_, '_, '_, '_>>()
        + size_of::<StructuralClaimKey<'_, '_>>()
        + size_of::<SystemWorkRef>()
        + size_of::<TargetRequest>()
        + 2 * size_of::<WalPrefix>()
        + 16 * size_of::<&()>()
        + 16 * size_of::<u64>();
    let prepared =
        entry + size_of::<Result<PreparedStructural<'_, '_, '_>>>() + super::source_bytes::<W>();
    // Prepared survives this scan; authority/lease/key do not yet exist.
    // The nested bound includes decode_job -> validate_root/budget.
    let jobs = entry
        + size_of::<PreparedStructural<'_, '_, '_>>()
        + size_of::<crate::storage::retention_page::row::JobView<'_>>()
        + 2 * size_of::<
            std::result::Result<
                crate::storage::retention_page::row::JobView<'_>,
                crate::storage::retention_page::RetentionError,
            >,
        >()
        + 16 * size_of::<&()>()
        + 2 * size_of::<ObjectHandle>()
        + crate::storage::retention_page::row::source_bytes();
    let lease = 2 * size_of::<super::super::super::super::super::super::flush_io::IoObservation>()
        + 24 * size_of::<&()>()
        + 8 * size_of::<Result<()>>()
        + 1024;
    let selection = owned
        + size_of::<dirty_frame::StructuralCandidates<'_, '_, '_>>()
        + size_of::<dirty_frame::Result<dirty_frame::StructuralCandidates<'_, '_, '_>>>()
        + size_of::<dirty_frame::Result<StructuralClaimKey<'_, '_>>>()
        + dirty_frame::structural_selection_source_bytes()
            .max(dirty_frame::structural_effect_pool_source_bytes())
            .max(lease);
    let physical = crate::storage::wal_io::DURABILITY_CALL_BYTES
        + 2 * size_of::<std::result::Result<WalRequirement<'_>, crate::storage::wal_io::WalIoError>>(
        )
        + 2 * size_of::<std::result::Result<DurableWal<'_>, crate::storage::wal_io::WalIoError>>();
    let control = 2 * size_of::<ControlCut>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 2 * size_of::<StructuralControlCommit<'_, '_, '_, '_>>()
        + 24 * size_of::<&()>()
        + 8 * size_of::<Result<()>>()
        + 2048;
    let installation = owned
        + 3 * size_of::<Result<Outcome<'_>>>()
        + size_of::<Outcome<'_>>()
        + size_of::<smo::LayoutView<'_>>()
        + size_of::<StructuralWriteTargets<'_, '_, '_, '_, '_, '_, '_>>()
        + size_of::<StructuralInstallPermit<'_, '_, '_>>()
        + size_of::<StructuralStageRejection<'_, '_, '_>>()
        + 2 * size_of::<StructuralCancelPermit<'_, '_, '_, '_>>()
        + dirty_frame::structural_effect_pool_source_bytes()
        + physical
            .max(control)
            .max(lease)
            .max(split_frame::source_census().layout);
    let replay = owned
        + size_of::<Outcome<'_>>()
        + size_of::<DurableWal<'_>>()
        + 2 * size_of::<WalPrefix>()
        + size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>()
        + 6 * size_of::<Result<()>>()
        + history::source_bytes::<W>()
            .peak
            .max(history::runtime::lookup_source_census().returning)
            .max(physical)
            .max(dirty_frame::structural_effect_pool_source_bytes())
            .max(lease);
    let returning = entry
        + source::leaf_source_bytes()
        + crate::transaction::concurrency::terminal_gate_source_bytes();
    SourceCensus {
        entry,
        prepared,
        jobs,
        selection,
        installation,
        replay,
        returning,
        peak: prepared
            .max(jobs)
            .max(selection)
            .max(installation)
            .max(replay)
            .max(returning),
    }
}

pub(in super::super::super::super) fn source_bytes<W: WriterNamespace>() -> usize {
    source_census::<W>().peak
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn structural_effect_source_phases_fit_original_admission_and_callers() {
        fn inspect<W: WriterNamespace>(kind: &str) {
            let actual = source_census::<W>();
            // Independent simultaneous inventory at canonical replay: the
            // prepared group and key still own the original gate, lease and
            // pending completion; these cannot be hidden inside a simple max.
            let replay_floor = size_of::<PreparedStructural<'_, '_, '_>>()
                + size_of::<StructuralClaimKey<'_, '_>>()
                + size_of::<StructuralIoLease<'_, '_, '_, '_>>()
                + size_of::<CompletedStructural<'_, '_>>()
                + size_of::<source::OperationSourceAdmission<'_, '_, '_, '_>>()
                + size_of::<crate::transaction::concurrency::TerminalReadGate<'_, '_>>()
                + size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
                + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
                + history::source_bytes::<W>().peak;
            assert!(actual.replay >= replay_floor);
            assert!(
                actual.selection
                    >= size_of::<PreparedStructural<'_, '_, '_>>()
                        + size_of::<StructuralAuthority<'_, '_, '_>>()
                        + size_of::<StructuralIoLease<'_, '_, '_, '_>>()
                        + dirty_frame::structural_selection_source_bytes()
            );
            assert!(actual.prepared >= actual.entry + super::super::source_bytes::<W>());
            let job_floor = size_of::<PreparedStructural<'_, '_, '_>>()
                + size_of::<crate::storage::retention_page::row::JobView<'_>>()
                + crate::storage::retention_page::row::source_bytes();
            assert!(actual.jobs >= actual.entry + job_floor);
            assert!(actual.installation >= dirty_frame::structural_effect_pool_source_bytes());
            assert!(
                actual.peak
                    >= actual
                        .prepared
                        .max(actual.selection)
                        .max(actual.installation)
                        .max(actual.replay)
                        .max(actual.returning)
            );
            let charge = source::progress_charge_bytes::<W>();
            let caller = resources::source_census::<W>();
            assert!(charge >= actual.peak);
            assert!(caller.structural_effect >= actual.peak);
            assert!(
                caller.peak <= 65_536 && caller.metadata <= 196_608 && caller.whole <= 262_144,
                "caller {caller:?}"
            );
            println!(
                "STRUCTURAL_EFFECT_SOURCE {kind} {actual:?} replay_floor={replay_floor} pool={} charge={charge} caller_peak={} metadata={} whole={}",
                dirty_frame::structural_effect_pool_source_bytes(),
                caller.peak,
                caller.metadata,
                caller.whole
            );
        }
        inspect::<crate::storage::wal_io::plan4_test_support::Writer>("MEMORY");
        inspect::<crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::native::Writer>("NATIVE_TYPE");
    }
}
