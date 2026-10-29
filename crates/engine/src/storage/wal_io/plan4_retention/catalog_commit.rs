//! Actual Catalog Commit continuation on the original pool and WAL runtime.
//! Immutable private install input survives Pending; no replay/control gate or
//! content/version latch is held across the original WAL durability call.
use super::super::*;
use super::catalog_live::{CatalogCommitImage, catalog_apply, catalog_mark_staged};
#[cfg(test)]
use crate::transaction::concurrency::catalog_test_support::{
    Point as CatalogPoint, hit as catalog_hook,
};
use crate::{
    storage::{page::PageKind, retention_page::catalog3 as queue, semantic_catalog::Observation},
    transaction::concurrency::{InstallationKey, PageConcurrency},
};

const TARGETS: usize = 4;

/// Only this adapter creates the input, immediately after the original
/// CommitStageTargets has marked its actual WAL call as started.
impl<'a, 'i, 'c, 's> CatalogCommitAttempt<'a, 'i, 'c, 's> {
    pub(in super::super) fn into_parts(self) -> (&'a PreparedCatalog<'i, 'c, 's>, InstallationKey) {
        (self.prepared, self.key)
    }
}

/// Private-field proof of the actual complete installation, including the
/// original control and history publication. Neither a codec observation nor
/// the durable requirement can construct it. No Clone/Copy is supplied.
impl<'c, 's> CatalogCommitInstalled<'c, 's> {
    pub(in super::super) fn matches(
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
    pub(in super::super) fn origin(
        &self,
    ) -> (&'c PageConcurrency<'s>, &'s AtomicU8, &'s SessionFence) {
        (self.core, self.binding, self.fence)
    }
    pub(in super::super) fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(in super::super) fn q(&self) -> u64 {
        self.q
    }
}

impl<'s, 'c, W: WriterNamespace> RetentionWriter<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W> {
    /// Prepare and commit inside one original source reservation. The private
    /// preparation cannot escape between selection and the authentic stage.
    pub(crate) fn catalog_commit(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: crate::storage::transaction_record::TxnRef,
        private: &mut [u8; crate::storage::semantic_catalog::PRIVATE_BYTES],
    ) -> Result<Observation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        // Guardless checks precede admission; original deep health and ledger
        // validation execute in the private preparer after source reservation.
        storage.health().map_err(|error| {
            fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Storage(error),
            )
        })?;
        self.pool
            .check_retention_entry_health()
            .map_err(|error| frame(RetentionPhase::Prepare, error))?;
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
        let mut gate = Some(
            owner
                .close_reads_for_terminal(self.pool.bound_core())
                .map_err(|error| RetentionError {
                    phase: RetentionPhase::Prepare,
                    cause: RetentionCause::Concurrency(error),
                    effect: error.effect,
                })?,
        );
        let reopen =
            |gate: &mut Option<crate::transaction::concurrency::TerminalReadGate<'c, 's>>| {
                gate.take().map_or(Ok(()), |gate| {
                    gate.reopen_checked().map_err(|error| RetentionError {
                        phase: RetentionPhase::Census,
                        cause: RetentionCause::Concurrency(error),
                        effect: error.effect,
                    })
                })
            };
        let mut source = match source::OperationSourceAdmission::catalog_commit(
            self.isolation,
            &self.commit_charges,
            gate.as_ref().expect("closed read gate"),
        ) {
            Ok(source) => source,
            Err(error) => return merge(Err(RetentionError::from(error)), reopen(&mut gate)),
        };
        #[cfg(test)]
        catalog_hook(self.pool.bound_core(), CatalogPoint::CommitPreparing);
        // A consuming match ends the preparation Result before entering the
        // continuation. actor/private remain charged in every writer window.
        let result = match self.prepare_catalog_commit(owner, actor, private) {
            Ok(prepared) => {
                #[cfg(test)]
                catalog_hook(self.pool.bound_core(), CatalogPoint::CommitPrepared);
                let visibility = self
                    .visibility
                    .expect("private preparer checked visibility");
                self.commit_admitted(owner, prepared, visibility, &mut source, &mut gate)
            }
            Err(error) => Err(error),
        };
        #[cfg(test)]
        catalog_hook(self.pool.bound_core(), CatalogPoint::CommitCompleted);
        let source_cleanup = source.finish_checked().map_err(RetentionError::from);
        merge(merge(result, source_cleanup), reopen(&mut gate))
    }

    /// Original separated-entry fault adapter only. No production caller can
    /// construct or retain Commit preparation outside the synchronous entry.
    #[cfg(test)]
    pub(crate) fn commit_catalog(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        health(storage, self.pool)?;
        if prepared.operation() != crate::storage::semantic_catalog::Operation::Commit {
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
        // Terminal progress closes new reads and waits only for admitted ones
        // before any source guard, pin or effect. It holds no page guard,
        // version gate or ledger lock while waiting; readers never wait on it.
        let mut gate = Some(
            owner
                .close_reads_for_terminal(self.pool.bound_core())
                .map_err(|error| RetentionError {
                    phase: RetentionPhase::Prepare,
                    cause: RetentionCause::Concurrency(error),
                    effect: error.effect,
                })?,
        );
        let reopen =
            |gate: &mut Option<crate::transaction::concurrency::TerminalReadGate<'c, 's>>| {
                gate.take().map_or(Ok(()), |gate| {
                    gate.reopen_checked().map_err(|error| RetentionError {
                        phase: RetentionPhase::Census,
                        cause: RetentionCause::Concurrency(error),
                        effect: error.effect,
                    })
                })
            };
        let mut source = match source::OperationSourceAdmission::catalog_commit(
            self.isolation,
            &self.commit_charges,
            gate.as_ref().expect("closed read gate"),
        ) {
            Ok(source) => source,
            // An unreserved Commit still reopens reads with a checked cleanup.
            Err(error) => return merge(Err(RetentionError::from(error)), reopen(&mut gate)),
        };
        let result = self.commit_admitted(owner, prepared, visibility, &mut source, &mut gate);
        // Return the source charge before reopening reads. An original Fatal
        // wins; otherwise a cleanup failure replaces success or Unchanged.
        let source_cleanup = source.finish_checked().map_err(RetentionError::from);
        merge(merge(result, source_cleanup), reopen(&mut gate))
    }

    fn commit_admitted(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
        visibility: &visibility::BoundVisibility<'_, 'c, 's>,
        source: &mut source::OperationSourceAdmission<'_, '_, 'c, 's>,
        gate: &mut Option<crate::transaction::concurrency::TerminalReadGate<'c, 's>>,
    ) -> Result<Observation> {
        let storage = self.storage;
        {
            let prefix = storage
                .wal()
                .prefix()
                .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
            prepared.check(owner, self.pool, self.control, prefix)?;
        }
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
        let (
            prepared,
            PendingCatalog {
                pending,
                wait,
                key,
                requirement,
            },
        ) = stage_catalog(storage, self.pool, self.control, owner, prepared)?;
        // Shrink to the fixed Pending window before readers may enter again.
        // Both steps follow the WAL stage, so any failure is fatal.
        source
            .enter_commit_pending(&self.commit_charges)
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Stage,
                    RetentionError::from(e).cause,
                )
            })?;
        gate.take()
            .expect("closed read gate")
            .reopen_checked()
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Stage,
                    RetentionCause::Concurrency(e),
                )
            })?;
        #[cfg(test)]
        catalog_hook(prepared.core(), CatalogPoint::Pending);
        let durable = TerminalWal { wal: storage.wal() }
            .wait_catalog(wait)
            .map_err(|e| fatal(storage, RetentionPhase::Durable, RetentionCause::Wal(e)))?;
        #[cfg(test)]
        catalog_hook(prepared.core(), CatalogPoint::Durable);
        let drained = pending
            .drain_durable(&durable)
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
        // Drain excluded new readers and waited for admitted ones, which return
        // their loans first; growing into the install window cannot wait.
        source
            .enter_commit_install(&self.commit_charges, &drained)
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Install,
                    RetentionError::from(e).cause,
                )
            })?;
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
            let permit = InstallGrant {
                kind: InstallKind::Plan4 {
                    core: prepared.core(),
                    stamp: before,
                    requirement,
                    after_counter: before
                        .modification_counter
                        .checked_add(1)
                        .expect("preflight checked successor"),
                    after,
                },
            };
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
        // finish reopened reads. Shrink to the publication window; a read
        // admitted before this sees the install charge and may get Unchanged
        // Capacity, never a wait.
        source
            .enter_commit_publish(&self.commit_charges)
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Install,
                    RetentionError::from(e).cause,
                )
            })?;
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
    wait: CatalogCommitWait<'c, 's>,
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
    let (staged, wait) =
        match (TerminalWal { wal: storage.wal() }).stage_catalog(CatalogCommitAttempt {
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

/// The writer's admission windows [stage, Pending, install, publish],
/// excluding the shared retained owner/core/ports and the admission/gate
/// carriers that source charges in every window. stage runs with new reads
/// closed; Pending may overlap readers; install begins after the original
/// Drain; publish follows install.finish, where reads may overlap again.
pub(in super::super) fn writer_window_sources() -> [usize; 4] {
    let phases = source_phases();
    [
        phases.preflight.max(phases.stage),
        pending_window_source_bytes(),
        phases.install,
        publish_window_source_bytes(),
    ]
}

/// After install.finish returned content, version/admission and pins: the
/// completion receipts, epochs and the Q publication with its conversion.
fn publish_window_source_bytes() -> usize {
    use std::mem::size_of;
    let parent = size_of::<CatalogCommitImage<'_, '_, '_>>()
        + visibility::epoch_commit_carriers()
        + size_of::<WalRequirement<'_>>()
        + size_of::<InstallationKey>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<u64>() // q
        + size_of::<CatalogCommitInstalled<'_, '_>>()
        + size_of::<InstalledCommit<'_, '_>>()
        + size_of::<DurableWal<'_>>()
        + 8 * size_of::<&()>() // self, owner, storage, visibility, source, gate and call borrows
        + 2 * size_of::<Result<Observation>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>();
    // The frontier argument and the publication helper's packet, then the
    // mapped Visibility error; control assignment moves the DurableWal.
    let publish = size_of::<InstalledFrontier<'_, '_>>()
        + visibility::epoch_publication_source_bytes()
        + size_of::<visibility::VisibilityError>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    parent + publish
}

/// From stage return through the consuming Drain. Prepared's stage arrays and
/// the stage tuple have ended and no completion value exists yet. The Drain
/// term subtracts the Pending carrier its call closure already counts.
fn pending_window_source_bytes() -> usize {
    let [parent, wait, drain] = pending_window_parts();
    parent + wait.max(drain)
}

/// [parent, WAL wait call, Drain call] of the Pending window.
pub(in super::super) fn pending_window_parts() -> [usize; 3] {
    use std::mem::size_of;
    let parent = size_of::<CatalogCommitImage<'_, '_, '_>>()
        + visibility::epoch_commit_carriers()
        + size_of::<WalRequirement<'_>>()
        + size_of::<InstallationKey>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + size_of::<u64>() // q, prepared before stage
        + 8 * size_of::<&()>() // self, owner, storage, visibility, source, gate and call borrows
        + 2 * size_of::<Result<Observation>>() // inner and outer returns
        + 2 * size_of::<RetentionError>()
        + size_of::<dirty_frame::PendingCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>();
    // The WAL wait call alone, then the mapped handoff extracting the
    // DurableCommit and converting an attempted-effect error.
    let wait = terminal::catalog_wait_source_bytes()
        + 2 * size_of::<crate::storage::wal_io::Result<DurableCommit<'_, '_>>>()
        + size_of::<Result<DurableCommit<'_, '_>>>()
        + size_of::<crate::storage::wal_io::WalIoError>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    // The consuming Drain moves Pending out of the parent and borrows the
    // extracted DurableCommit; its closure includes Pending's failure Drop.
    let moved = size_of::<dirty_frame::PendingCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>();
    let drain = size_of::<DurableCommit<'_, '_>>()
        + dirty_frame::commit_drain_call_source_bytes::<TARGETS>()
        + size_of::<Result<dirty_frame::DrainedCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>()
        - moved;
    [parent, wait, drain]
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
        (StagedCommit<'c, 's>, CatalogCommitWait<'c, 's>),
        CatalogStageError<'c, 's>,
    >;
    let stage_local = size_of::<PendingCatalog<'_, '_, '_, '_, '_, '_>>()
        + size_of::<Result<(CatalogCommitImage<'_, '_, '_>, PendingCatalog<'_, '_, '_, '_, '_, '_>)>>()
        + size_of::<CatalogCommitImage<'_, '_, '_>>()
        + size_of::<[Option<&[u8; PAGE_BYTES]>; TARGETS]>() // extraction output
        + 6 * size_of::<&()>() // stage arguments and canonical target borrow
        + size_of::<CatalogCommitAttempt<'_, '_, '_, '_>>()
        + size_of::<StagedCommit<'_, '_>>()
        + size_of::<CatalogCommitWait<'_, '_>>()
        + size_of::<Option<UnstagedCommit<'_, '_>>>()
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
        + 2 * size_of::<DurableCommit<'_, '_>>()
        + 2 * size_of::<InstalledCommit<'_, '_>>()
        + size_of::<DurableWal<'_>>()
        + size_of::<crate::storage::wal_io::WalPrefix>()
        + 4 * size_of::<Result<Observation>>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 20 * size_of::<&()>()
        + size_of::<[usize; 8]>();
    let install = 2 * size_of::<FrameStamp>()
        + size_of::<dirty_frame::DptStamp<'_>>()
        + size_of::<InstallGrant<'_, '_, '_>>()
        + size_of::<InstallGrant<'_, '_, '_>>()
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
    let bridge = terminal::catalog_source_bytes();
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
            + size_of::<CatalogCommitWait<'_, '_>>()
            + bridge,
        // Drain returns before guard reconstruction; both calls return before
        // per-page permits/control/history publication. The mapped caller
        // Result is counted in addition to the physical drain return layers.
        // Keep the installed physical closure until finish, including errors.
        install: retained
            + size_of::<CatalogCommitImage<'_, '_, '_>>()
            + (physical.drain
                + size_of::<
                    Result<dirty_frame::DrainedCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>,
                >())
            .max(physical.resume)
            .max(physical.install + install.max(control).max(publication)),
    }
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
    let bridge = terminal::catalog_wait_source_observation::<W>();
    // map_err converts the returned WAL Result before extracting durable.
    // The physical call is over, but include both Results and the owned value,
    // plus the attempted-effect error conversion, without move elision.
    let handoff = size_of::<crate::storage::wal_io::Result<DurableCommit<'_, '_>>>()
        + size_of::<Result<DurableCommit<'_, '_>>>()
        + size_of::<DurableCommit<'_, '_>>()
        + size_of::<crate::storage::wal_io::WalIoError>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    [parent, bridge, handoff, parent + bridge.max(handoff)]
}

/// Observations for the pending writer admission design, not installed charges.
/// Actual preparation and compact Pending carriers are measured separately.
#[cfg(test)]
pub(in super::super) fn phase_source_observations<W: WriterNamespace>()
-> [(&'static str, usize); 11] {
    use std::mem::size_of;
    let physical = dirty_frame::commit_phase_sources::<TARGETS>();
    let [bridge_stage, bridge_wait, bridge_complete] = terminal::catalog_phase_sources();
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
        + size_of::<CatalogStageError<'_, '_>>()
        + 2 * size_of::<StagedCommit<'_, '_>>()
        + size_of::<CatalogCommitWait<'_, '_>>()
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
    let pending_floor = image + physical.pending_retained + size_of::<CatalogCommitWait<'_, '_>>();
    let parent = image + common_parent;
    let pending_wait = parent + physical.pending_retained + bridge_wait;
    let drain_wait = parent
        + size_of::<DurableCommit<'_, '_>>()
        + size_of::<Result<dirty_frame::DrainedCommitTargets<'_, '_, '_, '_, '_, '_, TARGETS>>>()
        + physical.drain_wait;
    let install_locals = size_of::<CatalogCommitInstalled<'_, '_>>()
        + 2 * size_of::<DurableCommit<'_, '_>>()
        + 2 * size_of::<InstalledCommit<'_, '_>>()
        + size_of::<DurableWal<'_>>()
        + 2 * size_of::<FrameStamp>()
        + size_of::<dirty_frame::DptStamp<'_>>()
        + size_of::<InstallGrant<'_, '_, '_>>()
        + size_of::<InstallGrant<'_, '_, '_>>()
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
            super::catalog_live::commit_prepare_source_bytes::<W>(),
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
        println!(
            "C3_CREATE_PARTS {kind} phases={:?}",
            super::super::catalog_live::catalog_live_source_bytes::<W>()
        );
        assert!(shared + phases.peak() <= 65_536);
        // The census uses the largest admitted writer window with its carriers.
        assert_eq!(live.catalog_commit, source::commit_source_peak::<W>());
        assert!(live.catalog_commit >= phases.peak());
        assert!(live.peak <= 65_536 && live.metadata <= 196_608 && live.whole <= 262_144);
        assert!(
            recovered.peak <= 65_536 && recovered.metadata <= 196_608 && recovered.whole <= 262_144
        );
    }
}
