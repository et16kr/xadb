//! Catalog rollback/terminal calls use one original progress reservation.
//! Ordinary application borrows the original resources; it creates no owner.
use super::*;

#[path = "row_start.rs"]
pub(in super::super::super) mod row;
#[path = "catalog_start.rs"]
mod start;

#[derive(Clone, Copy)]
enum CatalogLifecycle<'q> {
    Begin(&'q Plan4Target),
    AgingStart,
    RollbackStart(TxnRef),
    AbortEnd(TxnRef),
    CancelReserved(usize),
    ReleaseRole(usize),
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn actor_catalog_job(&self, actor: TxnRef, terminal: bool) -> Result<usize> {
        let mut found = None;
        // Catalog selection skips every Row-scope cell (another actor's).
        for (ordinal, bytes) in self.control.jobs().iter().enumerate() {
            let Some(job) =
                queue::decode_catalog_cell(bytes, self.storage.profile().spec().namespace)
                    .map_err(queue_error)?
            else {
                continue;
            };
            if job.source() == Some(actor)
                && job.state().is_terminal() == terminal
                && found.replace(ordinal).is_some()
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        found.ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction))
    }

    pub(super) fn catalog_status_request(&self, status: StatusSlotRef) -> TargetRequest {
        request(
            PageIdentity {
                database: self.storage.profile().spec().namespace.database,
                page: status.page(),
                owner: ObjectId::new(2).expect("Status owner"),
                owner_generation: one(),
            },
            PageKind::TransactionStatus,
        )
    }

    fn prepare_catalog_rollback_start<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        let terminal = self.actor_catalog_job(actor, true)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let terminal_job = queue::decode_job(
            &self.control.jobs()[terminal],
            self.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?;
        let status = self.catalog_status_request(
            terminal_job
                .status()
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?,
        );
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let mut root_job = None;
        for bytes in self.control.jobs() {
            let Some(job) =
                queue::decode_catalog_cell(bytes, self.storage.profile().spec().namespace)
                    .map_err(queue_error)?
            else {
                continue;
            };
            if job.source() == Some(actor)
                && job.state().is_root()
                && root_job.replace(job).is_some()
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
        }
        let prepared = if let Some(job) = root_job {
            let undo = job
                .undo()
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            let target = request(
                PageIdentity {
                    database: allocator.identity.database,
                    page: undo.page(),
                    owner: ObjectId::new(3).expect("Undo owner"),
                    owner_generation: one(),
                },
                PageKind::Undo,
            );
            self.prepare_existing_catalog(
                owner,
                Change::RollbackStart { terminal },
                Some(slot),
                [(allocator, 0), (root, 1), (status, 4), (target, 5)],
                private,
            )?
        } else {
            self.prepare_existing_catalog(
                owner,
                Change::RollbackStart { terminal },
                Some(slot),
                [(allocator, 0), (root, 1), (status, 4)],
                private,
            )?
        };
        Ok(prepared)
    }

    fn prepare_catalog_abort_end<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        let terminal = self.actor_catalog_job(actor, true)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let status = queue::decode_job(
            &self.control.jobs()[terminal],
            self.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?
        .status()
        .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let job = target_from_source(
            self.storage,
            Plan4Page::Jobs {
                index: (terminal / 15) as u8,
            },
        )?
        .request;
        let prepared = self.prepare_existing_catalog(
            owner,
            Change::AbortEnd { terminal },
            Some(slot),
            [
                (allocator, 0),
                (root, 1),
                (job, 2),
                (self.catalog_status_request(status), 4),
            ],
            private,
        )?;
        Ok(prepared)
    }

    fn prepare_catalog_cancel_reserved<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        // Recovery has no in-process TxPairClaimKey to cancel. A live reservation
        // must consume ReservedCatalogCreate::cancel instead.
        if !self
            .pool
            .handoff_quiescent()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let prepared = self.prepare_catalog_job(owner, ordinal, Operation::Cancel, private)?;
        Ok(prepared)
    }

    fn prepare_catalog_release_role<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        let prepared = self.prepare_catalog_job(owner, ordinal, Operation::ReleaseRole, private)?;
        Ok(prepared)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Synchronous terminal work; the original flusher is exclusively borrowed
    /// through checked source/gate cleanup, with its existing state preserved.
    pub(crate) fn catalog_rollback_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.catalog_lifecycle(
            owner,
            CatalogLifecycle::RollbackStart(actor),
            private,
            flusher,
        )
    }

    /// Synchronous terminal work; the original flusher is exclusively borrowed
    /// through checked source/gate cleanup, with its existing state preserved.
    pub(crate) fn catalog_abort_end(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.catalog_lifecycle(owner, CatalogLifecycle::AbortEnd(actor), private, flusher)
    }

    /// Synchronous terminal work; the original flusher is exclusively borrowed
    /// through checked source/gate cleanup, with its existing state preserved.
    pub(crate) fn catalog_cancel_reserved(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.catalog_lifecycle(
            owner,
            CatalogLifecycle::CancelReserved(ordinal),
            private,
            flusher,
        )
    }

    /// Synchronous terminal work; the original flusher is exclusively borrowed
    /// through checked source/gate cleanup, with its existing state preserved.
    pub(crate) fn catalog_release_role(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.catalog_lifecycle(
            owner,
            CatalogLifecycle::ReleaseRole(ordinal),
            private,
            flusher,
        )
    }
    fn catalog_lifecycle(
        &mut self,
        owner: &mut Owner<'c, 's>,
        action: CatalogLifecycle<'_>,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.progress_lifecycle(
            owner,
            source::OperationClass::CatalogProgress,
            flusher,
            |writer, owner| {
                let prepared = match action {
                    CatalogLifecycle::Begin(status) => {
                        writer.prepare_catalog_begin(owner, status, private)
                    }
                    CatalogLifecycle::AgingStart => writer
                        .catalog_preparation()
                        .prepare_catalog_aging_start(owner, private),
                    CatalogLifecycle::RollbackStart(actor) => writer
                        .catalog_preparation()
                        .prepare_catalog_rollback_start(owner, actor, private),
                    CatalogLifecycle::AbortEnd(actor) => writer
                        .catalog_preparation()
                        .prepare_catalog_abort_end(owner, actor, private),
                    CatalogLifecycle::CancelReserved(ordinal) => writer
                        .catalog_preparation()
                        .prepare_catalog_cancel_reserved(owner, ordinal, private),
                    CatalogLifecycle::ReleaseRole(ordinal) => writer
                        .catalog_preparation()
                        .prepare_catalog_release_role(owner, ordinal, private),
                }?;
                #[cfg(test)]
                crate::transaction::concurrency::catalog_test_support::hit(
                    writer.pool.bound_core(),
                    crate::transaction::concurrency::catalog_test_support::Point::LifecyclePrepared,
                );
                writer
                    .catalog_application()
                    .apply_existing_catalog(owner, prepared)
            },
        )
    }

    /// The one gate/progress shell of the synchronous lifecycle calls of
    /// either scope: guardless health/origin checks, the original closed read
    /// gate, the fixed progress class (Catalog or Row) and the original flusher
    /// binding precede `body`, and checked source/gate cleanup follows it.
    pub(super) fn progress_lifecycle<T>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        class: source::OperationClass,
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
        body: impl FnOnce(&mut Self, &mut Owner<'c, 's>) -> Result<T>,
    ) -> Result<T> {
        let _unwind = self.storage.session_fence().unwind_guard();
        // Only guardless health/origin checks precede source admission. The
        // existing preparer retains the deep registry/control validation.
        self.storage.health().map_err(|error| {
            fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Storage(error),
            )
        })?;
        self.pool
            .check_retention_entry_health()
            .map_err(|error| frame(RetentionPhase::Prepare, error))?;
        if !self
            .pool
            .uses_storage_origin(&self.storage.binding, self.storage.session_fence())
        {
            return Err(fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let gate = owner
            .close_reads_for_retirement(self.pool.bound_core())
            .map_err(|error| RetentionError {
                phase: RetentionPhase::Prepare,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })?;
        let reopen = |gate: crate::transaction::concurrency::TerminalReadGate<'c, 's>| {
            gate.reopen_checked().map_err(|error| RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })
        };
        let source = match source::OperationSourceAdmission::progress(
            self.isolation,
            &self.commit_charges,
            &self.progress_charge,
            &gate,
            class,
        ) {
            Ok(source) => source,
            Err(error) => return merge(Err(RetentionError::from(error)), reopen(gate)),
        };
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LifecyclePreparing,
        );
        let result = (|| {
            if !std::ptr::eq(flusher.pool, self.pool)
                || !std::ptr::eq(flusher.storage, self.storage)
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::BindingMismatch,
                ));
            }
            body(self, owner)
        })();
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LifecycleCompleted,
        );
        let returned = source.finish_checked().map_err(RetentionError::from);
        merge(merge(result, returned), reopen(gate))
    }

    pub(super) fn catalog_application(
        &mut self,
    ) -> CatalogApplication<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogApplication {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            access: CatalogTargetAccess::Ordinary,
        }
    }
}

pub(super) struct CatalogApplication<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    control: &'b mut control::RetentionLedger<'a, 'c, 's>,
    history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
    pub(super) access: CatalogTargetAccess<'b, 'c, 's>,
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(super) fn catalog_application(
        &mut self,
    ) -> CatalogApplication<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogApplication {
            storage: self.inner.storage,
            pool: &self.inner.pool,
            control: &mut self.control.ledger,
            history: &self.history,
            visibility: self.catalog_visibility.as_ref(),
            access: CatalogTargetAccess::Ordinary,
        }
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogCreateAccess<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn catalog_application(
        &mut self,
    ) -> CatalogApplication<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogApplication {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            access: CatalogTargetAccess::Ordinary,
        }
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogApplication<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn apply_existing_catalog(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        self.apply_existing(owner, ExistingGroup::Catalog(&prepared))?;
        Ok(prepared.payload.observation)
    }

    /// The one existing-group install body of either scope; the payload kind
    /// selects only the group-specific values (see `ExistingGroup`).
    pub(super) fn apply_existing(
        &mut self,
        owner: &mut Owner<'c, 's>,
        group: ExistingGroup<'_, '_, 'c, 's>,
    ) -> Result<()> {
        match group.targets().len() {
            2 => self.apply_existing_n::<2>(owner, group),
            3 => self.apply_existing_n::<3>(owner, group),
            4 => self.apply_existing_n::<4>(owner, group),
            5 => self.apply_existing_n::<5>(owner, group),
            6 => self.apply_existing_n::<6>(owner, group),
            7 => self.apply_existing_n::<7>(owner, group),
            8 => self.apply_existing_n::<8>(owner, group),
            9 => self.apply_existing_n::<9>(owner, group),
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            )),
        }
    }

    fn apply_existing_n<const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        group: ExistingGroup<'_, '_, 'c, 's>,
    ) -> Result<()> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        // Commit must pass the real durable continuation, and reference removal
        // must pass original retirement admission. Neither uses this fast path;
        // a Row group installs only the supplied lifecycle and Row write
        // operations.
        if !group.ordinary() {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        group.check(owner, self.pool, self.control, prefix)?;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        // Live Cancel retains its delta in the linear reservation until pair
        // cleanup; the closed recovery Cancel has no active epoch map yet.
        // Row Begin/RollbackStart/AbortEnd publish no epoch (ROW-BEGIN-001),
        // nor do RowReserve, its Forward and its Cancel (the Row epoch is
        // Commit's, S3).
        let epoch_change = match group {
            ExistingGroup::Catalog(prepared) if prepared.operation() != Operation::Cancel => {
                visibility
                    .prepare_epoch_change(prepared, storage.profile())
                    .map_err(|e| RetentionError {
                        phase: RetentionPhase::Prepare,
                        effect: e.effect,
                        cause: RetentionCause::Visibility(e),
                    })?
            }
            _ => None,
        };
        let requests: &[TargetRequest; N] = group
            .targets()
            .try_into()
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut write = self
            .access
            .write(self.pool, owner, requests)
            .map_err(|e| frame(RetentionPhase::Install, e))?;
        let installed = (|| {
            let mut dpt = [const { None }; N];
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                let stamp = group
                    .stamp(at)
                    .ok_or_else(|| unchanged(RetentionPhase::Install, RetentionCause::Invariant))?;
                write
                    .matches_stamp(at, &stamp)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                if group.after(at).is_some() {
                    *dpt_entry = Some(
                        write
                            .preflight_dpt(at, &stamp)
                            .map_err(|e| frame(RetentionPhase::Install, e))?,
                    );
                }
            }
            #[cfg(test)]
            crate::transaction::concurrency::catalog_test_support::hit(
                self.pool.bound_core(),
                crate::transaction::concurrency::catalog_test_support::Point::ExistingStageCallStarted,
            );
            let (stage_prefix, header, frame_bytes) = group.stage_parts();
            let requirement = storage
                .wal()
                .stage_verified_required(stage_prefix, header, frame_bytes)
                .map_err(|e| {
                    if e.effect == Effect::Unchanged {
                        RetentionError {
                            phase: RetentionPhase::Stage,
                            effect: e.effect,
                            cause: RetentionCause::Wal(e),
                        }
                    } else {
                        fatal(storage, RetentionPhase::Stage, RetentionCause::Wal(e))
                    }
                })?;
            group.mark_staged(self.control, requirement)?;
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                let Some(after) = group.after(at) else {
                    continue;
                };
                let permit = InstallGrant {
                    kind: InstallKind::Plan4 {
                        core: self.pool.bound_core(),
                        stamp: group.stamp(at).expect("checked existing"),
                        requirement,
                        after_counter: group
                            .stamp(at)
                            .expect("checked existing")
                            .modification_counter
                            .checked_add(1)
                            .expect("preflight checked successor"),
                        after,
                    },
                };
                write
                    .install_page(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                write
                    .install_metadata(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                write
                    .install_dpt(dpt_entry.take().expect("preflight DPT"), &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
            }
            group.apply_control(self.control, storage.profile(), requirement)?;
            self.history.installed(requirement)?;
            Ok(requirement)
        })();
        let requirement = merge(
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
        visibility.apply_epoch_change(epoch_change).map_err(|e| {
            fatal(
                storage,
                RetentionPhase::Install,
                RetentionCause::Visibility(e),
            )
        })?;
        self.control.durable = Some(durable);
        self.control.pending = None;
        Ok(())
    }
}

/// The private payload kind of the one existing-group install body
/// (ROW-BEGIN-001: one body, payload {Catalog, Row}, no copy). It selects
/// only the group's own values: its admitted operations, owner/cut check,
/// targets, stamps, after images, staged frame, stage mark and control part.
#[derive(Clone, Copy)]
pub(super) enum ExistingGroup<'p, 'i, 'c, 's> {
    Catalog(&'p PreparedCatalog<'i, 'c, 's>),
    Row(&'p row::PreparedRowGroup<'i, 'c, 's>),
    /// A Row write group over the whole write target set (S2-b2: RowReserve;
    /// S2-c: its Forward; S2-d: its Cancel).
    RowWrite(&'p row::PreparedRowGroup<'i, 'c, 's, { row::ROW_WRITE_TARGETS }>),
    /// A recovered Row cleanup group of the closed facade (boundaries
    /// `#row-recovery-s6`): Cancel, Prune, ReleaseRole, RootFree and
    /// TerminalFinish, built only by the recovered preparation.
    RowRecovery(&'p row::PreparedRowGroup<'i, 'c, 's, { row::ROW_WRITE_TARGETS }>),
}
impl<'p, 'i, 'c, 's> ExistingGroup<'p, 'i, 'c, 's> {
    fn ordinary(self) -> bool {
        match self {
            Self::Catalog(prepared) => !matches!(
                prepared.operation(),
                Operation::Commit
                    | Operation::Forward
                    | Operation::Clr
                    | Operation::Prune
                    | Operation::RootFree
                    | Operation::TerminalFinish
            ),
            Self::Row(prepared) => prepared.lifecycle(),
            Self::RowWrite(prepared) => prepared.row_write(),
            Self::RowRecovery(prepared) => prepared.recovery(),
        }
    }
    fn check(
        self,
        owner: &Owner<'c, 's>,
        pool: &SharedPool<'c, 's, '_, '_>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()> {
        match self {
            Self::Catalog(prepared) => prepared.check(owner, pool, control, prefix),
            Self::Row(prepared) => prepared.check(owner, pool, control, prefix),
            Self::RowWrite(prepared) => prepared.check(owner, pool, control, prefix),
            Self::RowRecovery(prepared) => prepared.check(owner, pool, control, prefix),
        }
    }
    fn targets(self) -> &'p [TargetRequest] {
        match self {
            Self::Catalog(prepared) => prepared.targets(),
            Self::Row(prepared) => prepared.targets(),
            Self::RowWrite(prepared) => prepared.targets(),
            Self::RowRecovery(prepared) => prepared.targets(),
        }
    }
    fn stamp(self, at: usize) -> Option<FrameStamp> {
        match self {
            Self::Catalog(prepared) => prepared.stamp(at),
            Self::Row(prepared) => prepared.stamp(at),
            Self::RowWrite(prepared) => prepared.stamp(at),
            Self::RowRecovery(prepared) => prepared.stamp(at),
        }
    }
    fn after(self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        match self {
            Self::Catalog(prepared) => prepared.after(at),
            Self::Row(prepared) => prepared.after(at),
            Self::RowWrite(prepared) => prepared.after(at),
            Self::RowRecovery(prepared) => prepared.after(at),
        }
    }
    fn stage_parts(
        self,
    ) -> (
        WalPrefix,
        crate::storage::wal_frame::WalFrameHeader,
        &'i [u8],
    ) {
        match self {
            Self::Catalog(prepared) => (
                prepared.payload.prefix,
                prepared.payload.observation.header,
                prepared.payload.frame,
            ),
            Self::Row(prepared) => prepared.stage_parts(),
            Self::RowWrite(prepared) => prepared.stage_parts(),
            Self::RowRecovery(prepared) => prepared.stage_parts(),
        }
    }
    fn mark_staged(
        self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        match self {
            Self::Catalog(prepared) => catalog_mark_staged(prepared, control, requirement),
            Self::Row(prepared) => prepared.mark_staged(control, requirement),
            Self::RowWrite(prepared) => prepared.mark_staged(control, requirement),
            Self::RowRecovery(prepared) => prepared.mark_staged(control, requirement),
        }
    }
    fn apply_control(
        self,
        control: &mut control::RetentionLedger<'_, '_, '_>,
        profile: crate::storage::control::Profile,
        requirement: WalRequirement<'_>,
    ) -> Result<()> {
        match self {
            Self::Catalog(prepared) => {
                catalog_apply(&prepared.payload, control, profile, requirement)
            }
            Self::Row(prepared) => prepared.apply_control(control, profile, requirement),
            Self::RowWrite(prepared) => prepared.apply_control(control, profile, requirement),
            Self::RowRecovery(prepared) => prepared.apply_control(control, profile, requirement),
        }
    }
}

/// Only the temporary application view/forwarder is new. It is smaller than a
/// three-port split and never invokes the split-time census while Prepared is live.
pub(super) fn application_carrier_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    2 * size_of::<CatalogApplication<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 3 * size_of::<Result<Observation>>()
        + 6 * size_of::<&()>()
        // The borrowed payload kind of the one install body and its result,
        // forwarded once more by the count dispatch.
        + 2 * size_of::<ExistingGroup<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
}

pub(in super::super::super) fn entry_carrier_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    // Public forwarding action and private match packet; public and exclusive
    // actor/ordinal packets are distinct from the nested preparer arguments.
    // Count public typed entry, private dispatch, closure and exclusive refs;
    // Begin adds public/exclusive/preparer status references.
    2 * size_of::<CatalogLifecycle<'_>>()
        + 2 * size_of::<TxnRef>()
        + 2 * size_of::<usize>()
        + (4 + 4 + 5 + 3 + 3) * size_of::<&()>()
        + size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        // The shared shell's class and its body closure (action/private
        // captures, then the writer/owner arguments it is called with).
        + size_of::<source::OperationClass>()
        + 4 * size_of::<&()>()
}

/// Cancel's registry observation is sequential with selection and preparation.
/// Its original entry_registry validation is covered by the existing guarded
/// retention-entry owner bound; no guard survives into canonical preparation.
pub(super) fn quiescence_source_bytes() -> usize {
    use std::mem::size_of;
    let (waiting, guarded, returned) = dirty_frame::retention_entry_source_phases();
    waiting.max(guarded).max(returned)
        + 2 * size_of::<std::result::Result<bool, dirty_frame::DirtyError>>()
        + 3 * size_of::<&()>()
}

pub(in super::super::super) fn start_selection_source_phases<W: WriterNamespace>()
-> (usize, usize, usize) {
    start::selection_source_phases::<W>()
}
