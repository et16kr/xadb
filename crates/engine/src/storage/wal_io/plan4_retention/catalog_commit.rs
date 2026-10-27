//! Actual Catalog Commit continuation on the original pool and WAL runtime.
//! Immutable private preparation survives Pending; no replay/control gate or
//! content/version latch is held across the original WAL durability call.
use super::super::*;
use super::catalog_live::{
    CatalogCommitImage, PreparedCatalog, catalog_apply, catalog_mark_staged,
};
#[cfg(test)]
use crate::transaction::concurrency::catalog_test_support::{
    Point as CatalogPoint, hit as catalog_hook,
};
use crate::{
    storage::{
        page::PageKind,
        retention_page::catalog as queue,
        semantic_catalog::{Observation, Operation},
        session_fence::SessionFence,
        wal_io::{DirtyInstallPermit, commit_bridge},
    },
    transaction::concurrency::{InstallationKey, PageConcurrency},
};
use std::sync::atomic::AtomicU8;

const TARGETS: usize = 4;

/// Only this adapter creates the input, immediately after the original
/// CommitStageTargets has marked its actual WAL call as started.
pub(in crate::storage) struct CatalogCommitAttempt<'a, 'i, 'c, 's> {
    prepared: &'a PreparedCatalog<'i, 'c, 's>,
    key: InstallationKey,
}
impl<'a, 'i, 'c, 's> CatalogCommitAttempt<'a, 'i, 'c, 's> {
    pub(in crate::storage::wal_io) fn into_parts(
        self,
    ) -> (&'a PreparedCatalog<'i, 'c, 's>, InstallationKey) {
        (self.prepared, self.key)
    }
}

/// Private-field proof of the actual complete installation, including the
/// original control and history publication. Neither a codec observation nor
/// the durable requirement can construct it. No Clone/Copy is supplied.
pub(in crate::storage) struct CatalogCommitInstalled<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    key: InstallationKey,
    requirement: WalRequirement<'s>,
    digest: [u8; 32],
    q: u64,
}
impl<'c, 's> CatalogCommitInstalled<'c, 's> {
    pub(in crate::storage::wal_io) fn matches(
        &self,
        core: &PageConcurrency<'_>,
        key: InstallationKey,
        requirement: WalRequirement<'_>,
        digest: [u8; 32],
    ) -> bool {
        std::ptr::eq(self.core, core)
            && key == self.key
            && requirement.uses_fence(self.fence)
            && requirement.location() == self.requirement.location()
            && digest == self.digest
    }
    pub(in crate::storage::wal_io) fn origin(
        &self,
    ) -> (&'c PageConcurrency<'s>, &'s AtomicU8, &'s SessionFence) {
        (self.core, self.binding, self.fence)
    }
    pub(in crate::storage::wal_io) fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(in crate::storage::wal_io) fn q(&self) -> u64 {
        self.q
    }
}

impl<'s, 'c, W: WriterNamespace> RetentionWriter<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W> {
    pub(crate) fn commit_catalog(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        health(storage, self.pool)?;
        if prepared.operation() != Operation::Commit {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            ));
        }
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        if !visibility.matches_core(self.pool.bound_core()) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        prepared.check(owner, self.pool, self.control, prefix)?;
        if prepared.targets().len() != TARGETS {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let q = commit_q(&prepared, storage.profile())?;
        let epochs = visibility
            .prepare_commit_epochs(&prepared, self.control, storage.profile(), q)
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })?;
        if source_bytes() > 65_536 {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
        }

        let (
            prepared,
            PendingCatalog {
                pending,
                wait,
                key,
                requirement,
            },
        ) = stage_catalog(storage, self.pool, self.control, owner, prepared)?;
        #[cfg(test)]
        catalog_hook(prepared.core(), CatalogPoint::Pending);
        let durable = storage
            .wal()
            .wait_catalog_commit(wait)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        #[cfg(test)]
        catalog_hook(prepared.core(), CatalogPoint::Durable);
        let drained = pending
            .drain_durable(&durable)
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
        #[cfg(test)]
        {
            catalog_hook(prepared.core(), CatalogPoint::DrainedBeforeResume);
            if crate::transaction::concurrency::catalog_test_support::drop_drained(prepared.core())
            {
                drop(drained);
                return Err(fatal(
                    storage,
                    RetentionPhase::Install,
                    RetentionCause::Invariant,
                ));
            }
        }
        let mut install = drained
            .resume()
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;

        // Original target order is the canonical physical order. Each permit
        // uses the re-resolved recLSN, including a Pending Some -> None clean.
        #[cfg(test)]
        let mut installed_pages = 0;
        for at in 0..TARGETS {
            let Some(after) = prepared.after(at) else {
                continue;
            };
            let before = install
                .before(at)
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
            let dpt = install
                .preflight_dpt(at)
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
            let permit = DirtyInstallPermit::from_retention(live::RetentionInstall::allocation(
                prepared.core(),
                before,
                requirement,
                after,
            ));
            install
                .install_page(at, &permit)
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
            #[cfg(test)]
            {
                installed_pages += 1;
                if installed_pages == 2 {
                    catalog_hook(prepared.core(), CatalogPoint::InstalledSecondPage);
                }
            }
            install
                .install_metadata(at, &permit)
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
            install
                .install_dpt(dpt, &permit)
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
        }
        catalog_apply(
            prepared.payload(),
            self.control,
            storage.profile(),
            requirement,
        )
        .map_err(|e| fatal(storage, e.phase, e.cause))?;
        self.history
            .installed(requirement)
            .map_err(|e| fatal(storage, e.phase, e.cause))?;
        #[cfg(test)]
        catalog_hook(prepared.core(), CatalogPoint::InstalledBeforeFinish);
        let completed = CatalogCommitInstalled {
            core: prepared.core(),
            binding: prepared.binding(),
            fence: prepared.fence(),
            key,
            requirement,
            digest: prepared.frame_digest(),
            q,
        };
        let (installed, durable) = durable
            .complete_catalog(&completed)
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Wal(e)))?;
        // Return content, version/admission and pins before the small Q
        // publication. A newly registered snapshot in this gap keeps old Q
        // and resolves the already installed Committed(q) through its Undo.
        install
            .finish(installed)
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
        #[cfg(test)]
        catalog_hook(prepared.core(), CatalogPoint::BeforePublish);
        visibility
            .publish_catalog_epochs(visibility::InstalledFrontier::catalog(&completed), epochs)
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Install,
                    RetentionCause::Visibility(e),
                )
            })?;
        self.control.durable = Some(durable);
        self.control.pending = None;
        Ok(prepared.observation())
    }
}

/// The actual stage result, retaining only the original pins and WAL bindings.
/// Preparation locals and rejected-stage carriers end before the wait begins.
struct PendingCatalog<'u, 'b, 'c, 's, 'p, 'd> {
    pending: dirty_frame::PendingCommitTargets<'u, 'b, 'c, 's, 'p, 'd, TARGETS>,
    wait: commit_bridge::CatalogCommitWait<'c, 's>,
    key: InstallationKey,
    requirement: WalRequirement<'s>,
}

fn stage_catalog<'i, 'u, 'b, 'a, 's, 'c, 'p, 'd, W: WriterNamespace>(
    storage: &'s MutableStorageOwner<'_, '_, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    control: &mut control::RetentionLedger<'a, 'c, 's>,
    owner: &'u mut Owner<'c, 's>,
    prepared: PreparedCatalog<'i, 'c, 's>,
) -> Result<(
    CatalogCommitImage<'i, 'c, 's>,
    PendingCatalog<'u, 'b, 'c, 's, 'p, 'd>,
)> {
    let requests: &[TargetRequest; TARGETS] = prepared
        .targets()
        .try_into()
        .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
    // All stale/overflow/DPT checks precede Intent and the actual stage.
    // These proof-only DPT stamps are not retained across Pending: a
    // flusher is allowed to clean an unchanged page in that interval.
    let write = pool
        .reserve_target_set(owner, requests)
        .map_err(|e| frame(RetentionPhase::Prepare, e))?
        .try_write()
        .map_err(|e| frame(RetentionPhase::Prepare, e))?;
    let checked = (|| {
        for at in 0..TARGETS {
            let stamp = prepared
                .stamp(at)
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            write
                .matches_stamp(at, &stamp)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            if prepared.after(at).is_some() {
                if stamp.page_lsn.0 >= prepared.observation().header.lsn {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::Invariant,
                    ));
                }
                let _ = write
                    .preflight_dpt(at, &stamp)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            }
        }
        Ok(())
    })();
    if let Err(error) = checked {
        return merge(
            Err(error),
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        );
    }
    let mut stage = write
        .prepare_commit()
        .map_err(|e| frame(RetentionPhase::Prepare, e))?;
    let key = stage
        .stage_call_started()
        .map_err(|e| frame(RetentionPhase::Stage, e))?;
    #[cfg(test)]
    catalog_hook(prepared.core(), CatalogPoint::StageCallStarted);
    let (staged, wait) = match storage.wal().stage_catalog_commit(CatalogCommitAttempt {
        prepared: &prepared,
        key,
    }) {
        Ok(staged) => staged,
        Err(rejected) => {
            let Some(unstaged) = rejected.unstaged else {
                return Err(fatal(
                    storage,
                    RetentionPhase::Stage,
                    RetentionCause::Wal(rejected.error),
                ));
            };
            stage
                .restore_unstaged(unstaged)
                .map_err(|e| fatal(storage, RetentionPhase::Stage, RetentionCause::Frame(e)))?;
            let error = RetentionError {
                phase: RetentionPhase::Stage,
                effect: rejected.error.effect,
                cause: RetentionCause::Wal(rejected.error),
            };
            return merge(
                Err(error),
                stage
                    .cancel_unstaged()
                    .map_err(|e| frame(RetentionPhase::Stage, e)),
            );
        }
    };
    let requirement = wait.requirement();
    // The exact staged requirement is retained before releasing any lock;
    // old ATT/descriptor/Jobs bytes remain stable during Pending.
    catalog_mark_staged(&prepared, control, requirement)
        .map_err(|e| fatal(storage, e.phase, e.cause))?;
    let pending = stage
        .suspend(staged)
        .map_err(|e| fatal(storage, RetentionPhase::Stage, RetentionCause::Frame(e)))?;
    let image = prepared.into_commit_image();
    Ok((
        image,
        PendingCatalog {
            pending,
            wait,
            key,
            requirement,
        },
    ))
}

fn commit_q(
    prepared: &PreparedCatalog<'_, '_, '_>,
    profile: crate::storage::control::Profile,
) -> Result<u64> {
    let mut q = None;
    let mut kinds = [false; TARGETS];
    for (at, request) in prepared.targets().iter().enumerate() {
        let role = match request.kind {
            PageKind::Allocation if prepared.after(at).is_none() => 0,
            PageKind::RetentionRoot if prepared.after(at).is_some() => 1,
            PageKind::RetentionJobs if prepared.after(at).is_some() => 2,
            PageKind::TransactionStatus if prepared.after(at).is_some() => 3,
            _ => {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        };
        if kinds[role] {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        kinds[role] = true;
        if role == 1 {
            let (_, before) = queue::decode_root_page(
                prepared
                    .before(at)
                    .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?,
                profile.spec().namespace,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Retention(e)))?;
            let (header, after) = queue::decode_root_page(
                prepared.after(at).expect("checked Root after"),
                profile.spec().namespace,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Retention(e)))?;
            if before.commit_c().checked_add(1) != Some(after.commit_c())
                || header.lsn.0 != prepared.observation().header.lsn
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
            q = Some(after.commit_c());
        }
    }
    if kinds != [true; TARGETS] || prepared.observation().count != 3 {
        return Err(unchanged(
            RetentionPhase::Prepare,
            RetentionCause::Invariant,
        ));
    }
    q.ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))
}

/// Source-value closure of this adapter, excluding the original retained
/// owner/controller and caller's page/frame backing. No stack/RSS claim.
pub(in super::super) fn source_bytes() -> usize {
    use std::mem::size_of;
    type Stage<'c, 's> = std::result::Result<
        (
            crate::storage::wal_io::StagedCommit<'c, 's>,
            commit_bridge::CatalogCommitWait<'c, 's>,
        ),
        commit_bridge::CatalogStageError<'c, 's>,
    >;
    let retained = visibility::epoch_commit_carriers()
        + size_of::<PendingCatalog<'_, '_, '_, '_, '_, '_>>()
        + size_of::<Result<(CatalogCommitImage<'_, '_, '_>, PendingCatalog<'_, '_, '_, '_, '_, '_>)>>()
        + size_of::<CatalogCommitImage<'_, '_, '_>>()
        + std::mem::size_of::<[Option<&[u8; PAGE_BYTES]>; TARGETS]>() // extraction output
        + 6 * size_of::<&()>() // stage_catalog borrowed arguments and canonical target borrow
        + size_of::<PreparedCatalog<'_, '_, '_>>()
        + size_of::<Result<dirty_frame::DrainedCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>>()
        + size_of::<Observation>()
        + size_of::<WalRequirement<'_>>()
        + size_of::<InstallationKey>()
        + size_of::<CatalogCommitAttempt<'_, '_, '_, '_>>()
        + size_of::<CatalogCommitInstalled<'_, '_>>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<crate::storage::wal_io::StagedCommit<'_, '_>>()
        + size_of::<commit_bridge::CatalogCommitWait<'_, '_>>()
        + size_of::<Option<crate::storage::wal_io::UnstagedCommit<'_, '_>>>()
        + 2 * size_of::<Stage<'_, '_>>()
        + 2 * size_of::<crate::storage::wal_io::DurableCommit<'_, '_>>()
        + 2 * size_of::<crate::storage::wal_io::InstalledCommit<'_, '_>>()
        + size_of::<DurableWal<'_>>()
        + size_of::<crate::storage::wal_io::WalPrefix>()
        + 4 * size_of::<Result<Observation>>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 20 * size_of::<&()>()
        + size_of::<[usize; 8]>();
    let install = 2 * size_of::<FrameStamp>()
        + size_of::<dirty_frame::DptStamp<'_>>()
        + size_of::<live::RetentionInstall<'_, '_, '_>>()
        + size_of::<DirtyInstallPermit<'_, '_, '_>>()
        + 2 * size_of::<dirty_frame::Result<dirty_frame::DptStamp<'_>>>()
        + 2 * size_of::<dirty_frame::Result<FrameStamp>>();
    let root = 4 * size_of::<queue::RootView<'_>>()
        + 4 * size_of::<crate::storage::page::PageHeader>()
        + 4 * size_of::<
            std::result::Result<
                (crate::storage::page::PageHeader, queue::RootView<'_>),
                retention_page::RetentionError,
            >,
        >()
        + 4 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<foundation::codec::ContentHasher>()
        + size_of::<[bool; TARGETS]>()
        + size_of::<Option<u64>>()
        + size_of::<[u64; 8]>();
    retained
        + dirty_frame::commit_source_bytes::<TARGETS>()
        // The installer loop's stamp/permit/DPT values end before control,
        // history and Q publication. These are consecutive nested calls,
        // while PreparedCatalog and the common chain remain charged above.
        + install
            .max(root)
            .max(super::catalog_live::catalog_control_source_bytes())
            .max(history::runtime::source_bytes())
            .max(visibility::source_census().peak_leaf)
            .max(visibility::epoch_transition_source_bytes())
            .max(commit_bridge::catalog_source_bytes())
}

/// Observations for the pending writer admission design, not installed charges.
/// Actual preparation and compact Pending carriers are measured separately.
#[cfg(test)]
pub(in super::super) fn phase_source_observations<W: WriterNamespace>()
-> [(&'static str, usize); 11] {
    use std::mem::size_of;
    let physical = dirty_frame::commit_phase_sources::<TARGETS>();
    let [bridge_stage, bridge_wait, bridge_complete] = commit_bridge::catalog_phase_sources();
    let prepared = size_of::<PreparedCatalog<'_, '_, '_>>();
    let image = size_of::<CatalogCommitImage<'_, '_, '_>>();
    let common_parent = visibility::epoch_commit_carriers()
        + size_of::<crate::storage::wal_io::WalPrefix>()
        + size_of::<WalRequirement<'_>>()
        + size_of::<InstallationKey>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 4 * size_of::<Result<Observation>>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 20 * size_of::<&()>()
        + size_of::<[usize; 8]>();
    let pre_stage = prepared
        + common_parent
        + queue::source_bytes().max(visibility::epoch_transition_source_bytes())
        + 4 * size_of::<queue::RootView<'_>>()
        + 4 * size_of::<crate::storage::page::PageHeader>()
        + 4 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<foundation::codec::ContentHasher>();
    let stage_locals = size_of::<CatalogCommitAttempt<'_, '_, '_, '_>>()
        + size_of::<commit_bridge::CatalogStageError<'_, '_>>()
        + 2 * size_of::<crate::storage::wal_io::StagedCommit<'_, '_>>()
        + size_of::<commit_bridge::CatalogCommitWait<'_, '_>>()
        + size_of::<PendingCatalog<'_, '_, '_, '_, '_, '_>>()
        + size_of::<
            Result<(
                CatalogCommitImage<'_, '_, '_>,
                PendingCatalog<'_, '_, '_, '_, '_, '_>,
            )>,
        >()
        + size_of::<CatalogCommitImage<'_, '_, '_>>()
        + std::mem::size_of::<[Option<&[u8; PAGE_BYTES]>; TARGETS]>() // extraction output
        + 6 * size_of::<&()>();
    let stage = prepared
        + common_parent
        + stage_locals
        + physical
            .prepare
            .max(
                size_of::<dirty_frame::CommitStageTargets<'_, '_, '_, '_, '_, '_, TARGETS>>()
                    + bridge_stage,
            )
            .max(physical.suspend + super::catalog_live::catalog_control_source_bytes());
    // A lower bound from only three necessarily retained owned values. It is
    // deliberately NOT the full Pending envelope or a reservation size.
    let pending_floor =
        image + physical.pending_retained + size_of::<commit_bridge::CatalogCommitWait<'_, '_>>();
    let parent = image + common_parent;
    let pending_wait = parent + physical.pending_retained + bridge_wait;
    let drain_wait = parent
        + size_of::<crate::storage::wal_io::DurableCommit<'_, '_>>()
        + size_of::<Result<dirty_frame::DrainedCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>>()
        + physical.drain_wait;
    let install_locals = size_of::<CatalogCommitInstalled<'_, '_>>()
        + 2 * size_of::<crate::storage::wal_io::DurableCommit<'_, '_>>()
        + 2 * size_of::<crate::storage::wal_io::InstalledCommit<'_, '_>>()
        + size_of::<DurableWal<'_>>()
        + 2 * size_of::<FrameStamp>()
        + size_of::<dirty_frame::DptStamp<'_>>()
        + size_of::<live::RetentionInstall<'_, '_, '_>>()
        + size_of::<DirtyInstallPermit<'_, '_, '_>>()
        + 2 * size_of::<dirty_frame::Result<dirty_frame::DptStamp<'_>>>()
        + 2 * size_of::<dirty_frame::Result<FrameStamp>>();
    let install = parent
        + install_locals
        + physical.resume.max(
            physical.install_finish
                + super::catalog_live::catalog_control_source_bytes()
                    .max(history::runtime::source_bytes())
                    .max(bridge_complete),
        );
    let publish = parent
        + size_of::<CatalogCommitInstalled<'_, '_>>()
        + size_of::<DurableWal<'_>>()
        + visibility::epoch_transition_source_bytes();
    [
        ("prepared_actual_nine", prepared),
        ("pending_image", image),
        (
            "prepare_common_nine",
            super::catalog_live::commit_prepare_source_observation::<W>(),
        ),
        ("pre_stage", pre_stage),
        ("stage_return", stage),
        ("pending_three_value_floor", pending_floor),
        ("pending_wal_wait", pending_wait),
        ("drain_wait", drain_wait),
        ("resume_install_finish", install),
        ("publish_return", publish),
        ("production_broad", source_bytes()),
    ]
}

/// Actual Pending parent, after stage's tuple/Prepared temporaries have ended.
/// This is an observation, not a reservation or permission to overlap readers.
#[cfg(test)]
pub(in super::super) fn wait_source_observation<W: WriterNamespace>() -> [usize; 4] {
    use std::mem::size_of;
    let parent = size_of::<CatalogCommitImage<'_, '_, '_>>()
        + visibility::epoch_commit_retained_carrier()
        + size_of::<crate::storage::wal_io::WalPrefix>()
        + size_of::<WalRequirement<'_>>()
        + size_of::<InstallationKey>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<u64>() // q, prepared before stage
        + 6 * size_of::<&()>() // self, owner, storage, visibility and call borrows
        + size_of::<Result<Observation>>() // outer return, including fatal conversion
        + 2 * size_of::<RetentionError>()
        + size_of::<dirty_frame::PendingCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>();
    let bridge = commit_bridge::catalog_wait_source_observation::<W>();
    // map_err converts the returned WAL Result before extracting durable.
    // The physical call is over, but include both Results and the owned value,
    // plus the attempted-effect error conversion, without move elision.
    let handoff = size_of::<
        crate::storage::wal_io::Result<crate::storage::wal_io::DurableCommit<'_, '_>>,
    >() + size_of::<Result<crate::storage::wal_io::DurableCommit<'_, '_>>>()
        + size_of::<crate::storage::wal_io::DurableCommit<'_, '_>>()
        + size_of::<crate::storage::wal_io::WalIoError>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    [parent, bridge, handoff, parent + bridge.max(handoff)]
}
