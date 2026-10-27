//! Original whole-set pins survive Commit WAL I/O without content latches.
//! Semantic stage/completion grants remain sealed in the original WAL producer.
//! These common carriers cannot create those grants or acknowledge a Commit.

use super::*;
use crate::{
    storage::wal_io::{DurableCommit, InstalledCommit, StagedCommit, UnstagedCommit},
    transaction::concurrency::{
        CommitInstalling, CommitStage, InstallationKey, PendingInstallation,
    },
};
use foundation::codec::content_digest;

#[derive(Clone, Copy)]
struct StablePage {
    stamp: FrameStamp,
    digest: [u8; 32],
}

/// An unattempted intent over the original pinned/latching target set. The
/// semantic producer must retain its immutable private plan/after-image loan
/// in its enclosing operation; this common carrier does not allocate one.
pub(in crate::storage) struct CommitStageTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    targets: Option<WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N>>,
    stage: Option<CommitStage<'a, 'c, 's>>,
    stable: [Option<StablePage>; N],
}

/// The same physical PinSet, with zero page/version/admission guards. No second
/// registration map or DPT capacity reservation is introduced.
pub(in crate::storage) struct PendingCommitTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd, N>>,
    pending: Option<PendingInstallation<'a, 'c, 's>>,
    stable: [Option<StablePage>; N],
}

/// Authentic Drain completion owns the original version gate and pins, but no
/// page-content guard storage. It cannot be cloned or constructed by callers.
pub(in crate::storage) struct DrainedCommitTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    pins: Option<PinSet<'b, 'c, 's, 'p, 'd, N>>,
    installation: Option<CommitInstalling<'a, 'c, 's>>,
    stable: [Option<StablePage>; N],
}

pub(in crate::storage) struct CommitInstallTargets<'a, 'b, 'c, 's, 'p, 'd, const N: usize> {
    targets: Option<WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N>>,
    installation: Option<CommitInstalling<'a, 'c, 's>>,
    stable: [Option<StablePage>; N],
}

fn release_guards<const N: usize>(targets: &mut WriteTargets<'_, '_, '_, '_, '_, '_, N>) {
    if let Some(pins) = &targets.pins {
        pins.pool.fence_unwind();
    }
    for guard in targets.guards.iter_mut().rev() {
        guard.take();
    }
}

fn release_targets<const N: usize>(
    targets: &mut Option<WriteTargets<'_, '_, '_, '_, '_, '_, N>>,
) -> Result<()> {
    targets.take().map_or(Ok(()), WriteTargets::release_checked)
}

impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    /// Captures actual stable bytes before creating Intent. This is only a
    /// physical preflight; it does not validate a transaction or stage a frame.
    pub(in crate::storage) fn prepare_commit(
        mut self,
    ) -> Result<CommitStageTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        let mut stable = [None; N];
        let checked = (|| {
            let pins = self.pins.as_ref().expect("owned Commit target pins");
            pins.pool.health()?;
            if source_bytes::<N>() > 65_536 {
                return Err(unchanged(Cause::Capacity));
            }
            if self.len() == 0 || self.len() > N || self.version.is_none() {
                return Err(pins.pool.fatal(Cause::Invariant));
            }
            for (at, entry) in stable.iter_mut().enumerate().take(self.len()) {
                pins.registration(at)?;
                let guard = self.guards[at]
                    .as_ref()
                    .ok_or_else(|| pins.pool.fatal(Cause::Invariant))?;
                let stamp = pins.pool.pair(pins.indices[at], guard)?.frame;
                self.matches_stamp(at, &stamp)?;
                *entry = Some(StablePage {
                    stamp,
                    digest: content_digest(guard.bytes),
                });
            }
            Ok(())
        })();
        if let Err(error) = checked {
            return self.reject(error);
        }
        let version = self.version.take().expect("checked original version guard");
        let stage = match version.prepare_commit() {
            Ok(stage) => stage,
            Err(failure) => {
                // Even a fatal preflight returns the held version so page
                // guards are released before the original gate/admission.
                self.version = Some(failure.version);
                return self.reject(failure.error.into());
            }
        };
        Ok(CommitStageTargets {
            targets: Some(self),
            stage: Some(stage),
            stable,
        })
    }
}

impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> CommitStageTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    pub(in crate::storage) fn stage_call_started(&mut self) -> Result<InstallationKey> {
        self.stage
            .as_mut()
            .expect("owned Commit intent")
            .stage_call_started()
            .map_err(DirtyError::from)
    }

    pub(in crate::storage) fn restore_unstaged(
        &mut self,
        receipt: UnstagedCommit<'c, 's>,
    ) -> Result<()> {
        self.stage
            .as_mut()
            .expect("owned Commit intent")
            .restore_unstaged(receipt)
            .map_err(DirtyError::from)
    }

    pub(in crate::storage) fn cancel_unstaged(mut self) -> Result<()> {
        if let Some(targets) = &mut self.targets {
            release_guards(targets);
        }
        let cancel = self
            .stage
            .take()
            .expect("owned unattempted Commit intent")
            .cancel_unstaged()
            .map_err(DirtyError::from);
        combine(cancel, release_targets(&mut self.targets))
    }

    pub(in crate::storage) fn suspend(
        mut self,
        staged: StagedCommit<'c, 's>,
    ) -> Result<PendingCommitTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        let targets = self.targets.as_mut().expect("owned Commit stage targets");
        release_guards(targets);
        let pending = self
            .stage
            .take()
            .expect("owned Commit intent")
            .suspend(staged)
            .map_err(DirtyError::from)?;
        // Only the original PinSet crosses WAL waiting. The emptied write
        // carrier ends here; it has no guards, version or pins left to release.
        Ok(PendingCommitTargets {
            pins: targets.pins.take(),
            pending: Some(pending),
            stable: self.stable,
        })
    }
}

impl<const N: usize> Drop for CommitStageTargets<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        if let Some(targets) = &mut self.targets {
            release_guards(targets);
        }
        // The core guard decides whether an unattempted intent can cancel or
        // an attempted/unwinding operation must retain Abandoned and fence.
        drop(self.stage.take());
        let _ = release_targets(&mut self.targets);
    }
}

impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> PendingCommitTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    /// Waits only for already admitted finite content/apply sections. The
    /// enclosing producer retains its private plan; no shared workspace, FIFO
    /// turn, file I/O, additional pin or DPT allocation is requested here.
    pub(in crate::storage) fn drain_durable(
        mut self,
        durable: &DurableCommit<'c, 's>,
    ) -> Result<DrainedCommitTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        let installation = self
            .pending
            .take()
            .expect("owned pending Commit")
            .begin_drain(durable)
            .and_then(|drain| drain.wait_ready())
            .map_err(DirtyError::from)?;
        Ok(DrainedCommitTargets {
            pins: self.pins.take(),
            installation: Some(installation),
            stable: self.stable,
        })
    }
}

impl<'a, 'b, 'c, 's, 'p, 'd, const N: usize> DrainedCommitTargets<'a, 'b, 'c, 's, 'p, 'd, N> {
    /// Resume after the consuming Drain boundary. No capacity wait belongs
    /// here: the original Installing admission and version gate are held.
    pub(in crate::storage) fn resume(
        mut self,
    ) -> Result<CommitInstallTargets<'a, 'b, 'c, 's, 'p, 'd, N>> {
        // This moves the same PinSet; no reservation or registry edit occurs.
        let targets = WriteTargets {
            guards: std::array::from_fn(|_| None),
            version: None,
            pins: self.pins.take(),
        };
        let mut result = CommitInstallTargets {
            targets: Some(targets),
            installation: self.installation.take(),
            stable: self.stable,
        };
        result.acquire_and_revalidate()?;
        Ok(result)
    }
}

impl<const N: usize> Drop for PendingCommitTargets<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        // Pending Drop fences before physical identities are released.
        drop(self.pending.take());
        if let Some(pins) = &mut self.pins {
            #[cfg(test)]
            if pins.active {
                test_support::before_release(pins.pool);
            }
            let _ = pins.release();
        }
    }
}

impl<const N: usize> Drop for DrainedCommitTargets<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        // Normal unfinished Drop must abandon/fence before returning pins.
        // This ordering does not depend on panic or PinSet::fence_unwind.
        drop(self.installation.take());
        if let Some(pins) = &mut self.pins {
            #[cfg(test)]
            if pins.active {
                test_support::before_release(pins.pool);
            }
            let _ = pins.release();
        }
    }
}

impl<'b, 'c, 's, const N: usize> CommitInstallTargets<'_, 'b, 'c, 's, '_, '_, N> {
    fn acquire_and_revalidate(&mut self) -> Result<()> {
        let targets = self.targets.as_mut().expect("owned Commit install targets");
        let pins = targets.pins.as_ref().expect("retained original pins");
        let pool = pins.pool;
        pool.health()?;
        if targets.version.is_some()
            || targets.guards.iter().any(Option::is_some)
            || !self
                .installation
                .as_ref()
                .expect("owned Commit installation")
                .matches_core(pool.core)
        {
            return Err(pool.fatal(Cause::Invariant));
        }
        for at in 0..pins.len {
            pins.registration(at)?;
            let index = pins.indices[at];
            targets.guards[at] = Some(match pool.frames[index].content.try_write() {
                Ok(guard) => guard,
                // Drain's zero census excludes every live latch owner. Busy
                // here is not ordinary admission pressure and must not spin.
                Err(TryLockError::WouldBlock) => return Err(pool.fatal(Cause::Invariant)),
                Err(TryLockError::Poisoned(error)) => {
                    let failure = pool.fatal(Cause::Poisoned);
                    drop(error.into_inner());
                    return Err(failure);
                }
            });
            let guard = targets.guards[at].as_ref().expect("acquired target guard");
            let actual = pool.pair(index, guard)?.frame;
            let stable = self.stable[at]
                .as_mut()
                .ok_or_else(|| pool.fatal(Cause::Invariant))?;
            let before = stable.stamp;
            if actual.identity != before.identity
                || actual.generation != before.generation
                || actual.page_lsn != before.page_lsn
                || actual.modification_counter != before.modification_counter
                || (actual.rec_lsn != before.rec_lsn && actual.rec_lsn.is_some())
                || content_digest(guard.bytes) != stable.digest
            {
                return Err(pool.fatal(Cause::Invariant));
            }
            // Only lower-image finalize may have changed this field. The new
            // DPT stamp and permit must use this actual clean/dirty state.
            stable.stamp = actual;
        }
        Ok(())
    }

    pub(in crate::storage) fn before(&self, at: usize) -> Result<FrameStamp> {
        let targets = self.targets.as_ref().expect("owned Commit install targets");
        let pins = targets.pins.as_ref().expect("retained original pins");
        pins.pool.health()?;
        if at >= pins.len {
            return Err(unchanged(Cause::Shape));
        }
        self.stable[at]
            .map(|stable| stable.stamp)
            .ok_or_else(|| pins.pool.fatal(Cause::Invariant))
    }

    pub(in crate::storage) fn preflight_dpt(&self, at: usize) -> Result<DptStamp<'b>> {
        self.targets
            .as_ref()
            .expect("owned Commit install targets")
            .preflight_dpt(at, &self.before(at)?)
    }

    fn check_requirement(&self, permit: &InstallGrant<'_, 'c, 's>) -> Result<()> {
        let targets = self.targets.as_ref().expect("owned Commit install targets");
        let pins = targets.pins.as_ref().expect("retained original pins");
        if !self
            .installation
            .as_ref()
            .expect("owned Commit installation")
            .binding()
            .confirms_requirement(&permit.requirement())
        {
            return Err(pins.pool.fatal(Cause::Invariant));
        }
        Ok(())
    }

    pub(in crate::storage) fn install_page(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.check_requirement(permit)?;
        self.targets
            .as_mut()
            .expect("owned Commit install targets")
            .install_page(at, permit)
    }

    pub(in crate::storage) fn install_metadata(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.check_requirement(permit)?;
        self.targets
            .as_mut()
            .expect("owned Commit install targets")
            .install_metadata(at, permit)
    }

    pub(in crate::storage) fn install_dpt(
        &self,
        stamp: DptStamp<'_>,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.check_requirement(permit)?;
        self.targets
            .as_ref()
            .expect("owned Commit install targets")
            .install_dpt(stamp, permit)
    }

    pub(in crate::storage) fn finish(mut self, installed: InstalledCommit<'c, 's>) -> Result<()> {
        if let Some(targets) = &mut self.targets {
            release_guards(targets);
        }
        let finish = self
            .installation
            .take()
            .expect("owned Commit installation")
            .finish(installed)
            .map_err(DirtyError::from);
        combine(finish, release_targets(&mut self.targets))
    }
}

impl<const N: usize> Drop for CommitInstallTargets<'_, '_, '_, '_, '_, '_, N> {
    fn drop(&mut self) {
        if let Some(targets) = &mut self.targets {
            // Even normal unfinished Drop must fence before exposing the
            // partially installed content to another admitted observer.
            if self.installation.is_some()
                && let Some(pins) = &targets.pins
            {
                pins.pool.fence.fence();
            }
            release_guards(targets);
        }
        drop(self.installation.take());
        let _ = release_targets(&mut self.targets);
    }
}

/// Conservative source-value footprint of the common physical chain. The
/// actual terminal producer must additionally count its retained semantic
/// plan, descriptors, frame, core and caller returns. This is not a whole
/// Commit admission or an ABI stack/RSS measurement.
pub(in crate::storage) const fn source_bytes<const N: usize>() -> usize {
    use std::mem::size_of;
    let stage = size_of::<CommitStageTargets<'_, '_, '_, '_, '_, '_, N>>();
    let pending = size_of::<PendingCommitTargets<'_, '_, '_, '_, '_, '_, N>>();
    let install = size_of::<CommitInstallTargets<'_, '_, '_, '_, '_, '_, N>>();
    stage
        + pending
        + size_of::<DrainedCommitTargets<'_, '_, '_, '_, '_, '_, N>>()
        + 2 * size_of::<Result<DrainedCommitTargets<'_, '_, '_, '_, '_, '_, N>>>()
        + install
        + 2 * size_of::<Result<CommitStageTargets<'_, '_, '_, '_, '_, '_, N>>>()
        + 2 * size_of::<Result<PendingCommitTargets<'_, '_, '_, '_, '_, '_, N>>>()
        + 2 * size_of::<Result<CommitInstallTargets<'_, '_, '_, '_, '_, '_, N>>>()
        + target_set_call_bytes::<N>()
        // resume constructs this owned carrier after Drain, while the
        // compact Drained value and its install return are still in scope.
        + size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, N>>()
        + size_of::<[Option<StablePage>; N]>()
        + 4 * size_of::<FrameStamp>()
        + 2 * size_of::<Result<DirtyObservation>>()
        + 2 * size_of::<Result<DptStamp<'_>>>()
        + 2 * size_of::<foundation::codec::ContentHasher>()
        + 512
}

/// Read-only phase observations for the next admission design. The existing
/// production source_bytes and all runtime guards/authority stay unchanged.
#[cfg(test)]
#[derive(Debug)]
pub(in crate::storage) struct CommitPhaseSources {
    pub(in crate::storage) prepare: usize,
    pub(in crate::storage) suspend: usize,
    pub(in crate::storage) pending_retained: usize,
    pub(in crate::storage) drain_wait: usize,
    pub(in crate::storage) resume: usize,
    pub(in crate::storage) install_finish: usize,
}
#[cfg(test)]
pub(in crate::storage) fn phase_sources<const N: usize>() -> CommitPhaseSources {
    use std::mem::size_of;
    type Stage<'a, const N: usize> = CommitStageTargets<'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Pending<'a, const N: usize> = PendingCommitTargets<'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Install<'a, const N: usize> = CommitInstallTargets<'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Drained<'a, const N: usize> = DrainedCommitTargets<'a, 'a, 'a, 'a, 'a, 'a, N>;
    // The actual core is already shared; the common leaf includes every
    // stage/wait/install carrier, conservatively retained as a nested bound.
    let core_leaf = crate::transaction::concurrency::commit_common_source_bytes()
        - size_of::<crate::transaction::concurrency::PageConcurrency<'_>>();
    let physical = target_set_call_bytes::<N>()
        + 4 * size_of::<FrameStamp>()
        + 2 * size_of::<Result<DirtyObservation>>()
        + 2 * size_of::<Result<DptStamp<'_>>>()
        + 2 * size_of::<foundation::codec::ContentHasher>()
        + 512;
    CommitPhaseSources {
        prepare: size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, N>>()
            + size_of::<[Option<StablePage>; N]>()
            + size_of::<Stage<'_, N>>()
            + 2 * size_of::<Result<Stage<'_, N>>>()
            + physical
            + core_leaf,
        suspend: size_of::<Stage<'_, N>>()
            + size_of::<Pending<'_, N>>()
            + 2 * size_of::<Result<Pending<'_, N>>>()
            + size_of::<StagedCommit<'_, '_>>()
            + physical
            + core_leaf,
        pending_retained: size_of::<Pending<'_, N>>(),
        drain_wait: size_of::<Pending<'_, N>>()
            + size_of::<Drained<'_, N>>()
            + 2 * size_of::<Result<Drained<'_, N>>>()
            + core_leaf,
        resume: size_of::<Drained<'_, N>>()
            + size_of::<WriteTargets<'_, '_, '_, '_, '_, '_, N>>()
            + size_of::<Install<'_, N>>()
            + 2 * size_of::<Result<Install<'_, N>>>()
            + physical
            + core_leaf,
        install_finish: size_of::<Install<'_, N>>() + physical + core_leaf,
    }
}

#[cfg(test)]
mod tests;
