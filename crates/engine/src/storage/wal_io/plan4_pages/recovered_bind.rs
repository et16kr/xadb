//! The recovered owner claims its original binding once and installs one pool.
//! The recovery module owns the seed; ordinary PageView observations cannot
//! invoke this path with replacement bytes, a new fence or a made-up WAL prefix.
use super::*;
use crate::storage::wal_io::RecoveryPoolSeed;

impl<'m, 'w, W: WriterNamespace> MutableStorageOwner<'m, 'w, W> {
    pub(in crate::storage) fn bind_plan4_recovery_pages<'s, 'c, 'p, 'd: 'p, 'o>(
        &'s self,
        core: &'c PageConcurrency<'s>,
        frames: &'p mut [DirtyFrame<'d>],
        operation: &'o mut [u8; OPERATION_BYTES],
        seed: &RecoveryPoolSeed<'_>,
    ) -> Result<Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let phase = Plan4PagePhase::Preflight;
        self.health().map_err(|e| storage_error(phase, e))?;
        if self.core.plan.kind != BootstrapPlanKind::RetentionCatalogBootstrap {
            return Err(unchanged(phase, Plan4PageCause::UnsupportedPlan));
        }
        if !core.uses_fence(self.session_fence())
            || !std::ptr::eq(seed.binding(), &self.binding)
            || !std::ptr::eq(seed.fence(), self.session_fence())
        {
            return Err(unchanged(phase, Plan4PageCause::BindingMismatch));
        }
        if !(4..=dirty_frame::MAX_FRAMES).contains(&frames.len()) {
            return Err(unchanged(phase, Plan4PageCause::Shape));
        }
        for frame in frames.iter_mut() {
            frame
                .check_empty()
                .map_err(|e| fatal(self, phase, Plan4PageCause::Frame(e)))?;
        }
        let prefix = self.wal().prefix().map_err(|e| {
            fatal(
                self,
                phase,
                Plan4PageCause::Storage(MutableStorageError {
                    effect: e.effect,
                    cause: super::super::Cause::Wal(e),
                }),
            )
        })?;
        if self.core.reader_active.load(Ordering::Acquire)
            || self.profile() != self.plan.profile
            || self.core.plan.selected != self.plan.filemap
            || prefix.stream.writer_epoch < 2
            || prefix.complete_lsn < seed.lsn().0
            || prefix.complete_lsn != prefix.synced_lsn
            || prefix.complete_offset != 4096
            || prefix.synced_offset != 4096
        {
            return Err(fatal(self, phase, Plan4PageCause::Invariant));
        }
        {
            let append = self.wal().state(wal::Phase::Observe).map_err(|e| {
                fatal(
                    self,
                    phase,
                    Plan4PageCause::Storage(MutableStorageError {
                        effect: e.effect,
                        cause: super::super::Cause::Wal(e),
                    }),
                )
            })?;
            if !matches!(append.state, wal::SlotState::Idle) || append.slot.is_none() {
                return Err(fatal(self, phase, Plan4PageCause::Invariant));
            }
        }
        // The recovery seed is issued only by an acquired owner whose concrete
        // recovery resource census includes this constructor and its return.
        self.health().map_err(|e| storage_error(phase, e))?;
        match self
            .binding
            .compare_exchange(AVAILABLE, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(CLAIMED | BOUND) => return Err(unchanged(phase, Plan4PageCause::AlreadyBound)),
            Err(ABANDONED) => return Err(fatal(self, phase, Plan4PageCause::Fenced)),
            Err(_) => return Err(fatal(self, phase, Plan4PageCause::Invariant)),
        }
        let mut claim = BindingClaim {
            storage: self,
            complete: false,
        };
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Poisoned))?
            .take()
            .ok_or_else(|| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Invariant))?;
        let pool = SharedPool::new_recovered_plan4(
            core,
            self.session_fence(),
            &self.binding,
            frames,
            seed,
        )
        .map_err(|e| fatal(self, Plan4PagePhase::Bind, Plan4PageCause::Frame(e)))?;
        let bound = Plan4Pages {
            storage: self,
            pool,
            operation,
            _flush_workspace: Some(workspace),
        };
        self.health()
            .map_err(|e| storage_error(Plan4PagePhase::Bind, e))?;
        self.binding.store(BOUND, Ordering::Release);
        claim.complete = true;
        Ok(bound)
    }
}
