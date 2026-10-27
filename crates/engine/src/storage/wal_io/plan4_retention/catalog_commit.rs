//! Actual Catalog Commit continuation on the original pool and WAL runtime.
//! Immutable private install input survives Pending; no replay/control gate or
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
        if crate::transaction::concurrency::core_source_bytes() + source_bytes() > 65_536 {
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
        let mut install = pending
            .resume_durable(&durable)
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
/// storage owner/controller, original core and caller's page/frame backing.
/// The enclosing session census charges the original core once for every
/// operation class. No stack/RSS or concurrent admission claim.
pub(in super::super) fn source_bytes() -> usize {
    source_phases().peak()
}

/// Consecutive closures of commit_catalog; this is not concurrent admission.
/// Only the stage helper's returned boundary makes its locals disjoint from
/// waiting/resume. The shared outer values stay charged in every phase.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct CatalogCommitSourcePhases {
    pub(in super::super) preflight: usize,
    pub(in super::super) stage: usize,
    pub(in super::super) pending: usize,
    pub(in super::super) install: usize,
}
impl CatalogCommitSourcePhases {
    pub(in super::super) fn peak(self) -> usize {
        self.preflight
            .max(self.stage)
            .max(self.pending)
            .max(self.install)
    }
}

pub(in super::super) fn source_phases() -> CatalogCommitSourcePhases {
    use std::mem::size_of;
    type Stage<'c, 's> = std::result::Result<
        (
            crate::storage::wal_io::StagedCommit<'c, 's>,
            commit_bridge::CatalogCommitWait<'c, 's>,
        ),
        commit_bridge::CatalogStageError<'c, 's>,
    >;
    let stage_local = size_of::<PendingCatalog<'_, '_, '_, '_, '_, '_>>()
        + size_of::<Result<(CatalogCommitImage<'_, '_, '_>, PendingCatalog<'_, '_, '_, '_, '_, '_>)>>()
        + size_of::<CatalogCommitImage<'_, '_, '_>>()
        + size_of::<[Option<&[u8; PAGE_BYTES]>; TARGETS]>() // extraction output
        + 6 * size_of::<&()>() // stage arguments and canonical target borrow
        + size_of::<CatalogCommitAttempt<'_, '_, '_, '_>>()
        + size_of::<crate::storage::wal_io::StagedCommit<'_, '_>>()
        + size_of::<commit_bridge::CatalogCommitWait<'_, '_>>()
        + size_of::<Option<crate::storage::wal_io::UnstagedCommit<'_, '_>>>()
        + 2 * size_of::<Stage<'_, '_>>();
    // Deliberately retain even late completion values across every phase.
    // Control/epoch validation, unwind and caller return carriers are never
    // discounted. Only the consuming stage boundary ends PreparedCatalog's
    // target/stamp arrays; its immutable install payload moves into the image.
    let retained = visibility::epoch_commit_carriers()
        + size_of::<Observation>()
        + size_of::<WalRequirement<'_>>()
        + size_of::<InstallationKey>()
        + size_of::<CatalogCommitInstalled<'_, '_>>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
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
    let physical = dirty_frame::commit_source_phases::<TARGETS>();
    let control = super::catalog_live::catalog_control_source_bytes();
    let bridge = commit_bridge::catalog_source_bytes();
    let publication = history::runtime::source_bytes()
        .max(visibility::source_census().peak_leaf)
        .max(visibility::epoch_transition_source_bytes())
        .max(bridge);
    CatalogCommitSourcePhases {
        preflight: retained
            + size_of::<PreparedCatalog<'_, '_, '_>>()
            + root.max(control).max(publication),
        // Both mark_staged and bridge calls run while the stage target set is
        // retained. The stage closure includes its Pending success return.
        stage: retained
            + size_of::<PreparedCatalog<'_, '_, '_>>()
            + stage_local
            + physical.stage
            + control.max(bridge),
        // Original pins and all outer values survive the WAL call, but the
        // stage helper and its Result have returned before this phase starts.
        pending: retained
            + size_of::<CatalogCommitImage<'_, '_, '_>>()
            + physical.pending
            + size_of::<commit_bridge::CatalogCommitWait<'_, '_>>()
            + bridge,
        // resume returns before per-page permits/control/history publication.
        // Keep the installed physical closure until finish, including errors.
        install: retained
            + size_of::<CatalogCommitImage<'_, '_, '_>>()
            + physical
                .resume
                .max(physical.install + install.max(control).max(publication)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_preparation_has_bounded_pending_image() {
        use std::mem::size_of;
        let prepared = size_of::<PreparedCatalog<'_, '_, '_>>();
        let image = size_of::<CatalogCommitImage<'_, '_, '_>>();
        let phases = source_phases();
        println!("C2_COMMIT_IMAGE prepared={prepared} image={image} phases={phases:?}");
        // The nine preparation targets and stamps must not be retained by
        // the compact continuation. Its four borrowed after images are small.
        assert!(image + 1024 <= prepared);
        inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
        #[cfg(target_os = "linux")]
        inspect::<super::super::super::super::native::Writer>("native-type");
    }

    fn inspect<W: WriterNamespace>(kind: &str) {
        let shared = source::shared_bytes::<W>();
        let live = resources::source_census::<W>();
        let recovered = super::super::super::super::super::plan4_recovery::source_census::<W>();
        let phases = source_phases();
        println!(
            "C2_COMMIT_IMAGE_BOUND {kind} shared={shared} live={live:?} recovered={recovered:?}"
        );
        assert!(shared + phases.peak() <= 65_536);
        assert_eq!(live.catalog_commit, phases.peak());
        assert!(live.peak <= 65_536 && live.metadata <= 196_608 && live.whole <= 262_144);
        assert!(
            recovered.peak <= 65_536 && recovered.metadata <= 196_608 && recovered.whole <= 262_144
        );
    }
}
