//! Actual sealed retirement receipts. Only this live stage/install path can
//! issue them; physical intervals or parsed native bytes cannot construct one.
use super::*;

type CatalogJobSelection<const N: usize> = (Option<usize>, [(TargetRequest, u8); N]);

pub(in crate::storage) struct CatalogRetirementStaged<'r, 'i, 'c, 's> {
    prepared: &'r PreparedCatalog<'i, 'c, 's>,
    requirement: WalRequirement<'s>,
}
impl<'r, 'i, 'c, 's> CatalogRetirementStaged<'r, 'i, 'c, 's> {
    pub(in super::super::super) fn prepared(&self) -> &'r PreparedCatalog<'i, 'c, 's> {
        self.prepared
    }
    pub(in super::super::super) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
}
pub(in crate::storage) struct CatalogRetirementApplied<'r, 'i, 'c, 's> {
    staged: CatalogRetirementStaged<'r, 'i, 'c, 's>,
}
impl<'r, 'i, 'c, 's> CatalogRetirementApplied<'r, 'i, 'c, 's> {
    pub(in super::super::super) fn staged(&self) -> &CatalogRetirementStaged<'r, 'i, 'c, 's> {
        &self.staged
    }
}
pub(in crate::storage) struct CatalogRetirementRejected<'r, 'i, 'c, 's> {
    prepared: &'r PreparedCatalog<'i, 'c, 's>,
}
impl<'r, 'i, 'c, 's> CatalogRetirementRejected<'r, 'i, 'c, 's> {
    pub(in super::super::super) fn prepared(&self) -> &'r PreparedCatalog<'i, 'c, 's> {
        self.prepared
    }
}

fn visibility_error(error: visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Install,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}

fn release_references(
    current: Option<visibility::UndoPin<'_, '_, '_, '_>>,
    lease: Option<visibility::ReferenceLease<'_, '_, '_, '_>>,
) -> Result<()> {
    // Attempt both checked returns even when the pin return fences the session.
    // The original merge policy preserves a fatal first error over later cleanup.
    let pin = current.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    let lease = lease.map_or(Ok(()), |lease| {
        lease.release_checked().map_err(visibility_error)
    });
    merge(pin, lease)
}

impl<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn prepare_catalog_job<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_catalog_job(owner, ordinal, operation, private)
    }

    pub(crate) fn catalog_cancel_reserved(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        // Recovery has no in-process TxPairClaimKey to cancel. A live reservation
        // must consume ReservedCatalogCreate::cancel instead.
        if !self
            .inner
            .pool
            .handoff_quiescent()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let prepared = self.prepare_catalog_job(owner, ordinal, Operation::Cancel, private)?;
        self.apply_existing_catalog(owner, prepared)
    }
    pub(crate) fn catalog_release_role(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let prepared = self.prepare_catalog_job(owner, ordinal, Operation::ReleaseRole, private)?;
        self.apply_existing_catalog(owner, prepared)
    }
    pub(crate) fn prepare_catalog_retirement<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_catalog_retirement(owner, ordinal, operation, private)
    }
    pub(crate) fn catalog_retire(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Observation> {
        let prepared = self.prepare_catalog_retirement(owner, ordinal, operation, private)?;
        let (mut writer, _, mut flusher) = self.split();
        writer.apply_catalog_retirement(owner, prepared, &mut flusher)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    fn prepare_catalog_job<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        match operation {
            Operation::ReleaseRole | Operation::RootFree | Operation::TerminalFinish => {
                self.prepare_catalog_job_n::<4>(owner, ordinal, operation, private)
            }
            Operation::Cancel | Operation::Prune => {
                self.prepare_catalog_job_n::<7>(owner, ordinal, operation, private)
            }
            Operation::Clr => self.prepare_catalog_job_n::<9>(owner, ordinal, operation, private),
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            )),
        }
    }

    fn prepare_catalog_job_n<'i, const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        // Job/identity decoding and selection end before the nested canonical
        // replay. Only this selected array, slot and operation survive.
        let (slot, selected) = self.catalog_job_selection::<N>(ordinal, operation)?;
        let change = match operation {
            Operation::ReleaseRole => Change::ReleaseRole { ordinal },
            Operation::RootFree => Change::RootFree { ordinal },
            Operation::TerminalFinish => Change::TerminalFinish { ordinal },
            Operation::Cancel => Change::Cancel { ordinal },
            Operation::Prune => Change::Prune { ordinal },
            Operation::Clr => Change::Clr { ordinal },
            _ => {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
        };
        self.prepare_existing_catalog(owner, change, slot, selected, private)
    }

    fn catalog_job_selection<const N: usize>(
        &self,
        ordinal: usize,
        operation: Operation,
    ) -> Result<CatalogJobSelection<N>> {
        let invalid = || unchanged(RetentionPhase::Prepare, RetentionCause::Invariant);
        let job = queue::decode_job(
            self.control.jobs().get(ordinal).ok_or_else(invalid)?,
            self.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?;
        let actor = job.source().ok_or_else(invalid)?;
        let status = job.status().ok_or_else(invalid)?;
        let undo = job.undo();
        let pair = job.pair();
        let object = job.object();
        let slot = if matches!(operation, Operation::Cancel | Operation::Clr) {
            Some(self.control.slot(actor).ok_or_else(invalid)?)
        } else {
            None
        };
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let jobs = target_from_source(
            self.storage,
            Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            },
        )?
        .request;
        let status = request(
            PageIdentity {
                database: self.storage.profile().spec().namespace.database,
                page: status.page(),
                owner: ObjectId::new(2).expect("Status owner"),
                owner_generation: one(),
            },
            PageKind::TransactionStatus,
        );
        let undo = undo.map(|undo| {
            request(
                PageIdentity {
                    database: allocator.identity.database,
                    page: undo.page(),
                    owner: ObjectId::new(3).expect("Undo owner"),
                    owner_generation: one(),
                },
                PageKind::Undo,
            )
        });
        let mut selected = [(allocator, 0); N];
        match operation {
            Operation::ReleaseRole | Operation::RootFree | Operation::TerminalFinish if N == 4 => {
                selected.copy_from_slice(&[
                    (allocator, 0),
                    (root, 1),
                    (jobs, 2),
                    if operation == Operation::TerminalFinish {
                        (status, 4)
                    } else {
                        (undo.ok_or_else(invalid)?, 5)
                    },
                ]);
            }
            Operation::Cancel | Operation::Prune | Operation::Clr
                if N == (if operation == Operation::Clr { 9 } else { 7 }) =>
            {
                let catalog = target_from_source(self.storage, Plan4Page::CatalogLeaf)?.request;
                let anchor = target_from_source(self.storage, Plan4Page::CatalogAnchor)?.request;
                selected[..7].copy_from_slice(&[
                    (allocator, 0),
                    (root, 1),
                    (jobs, 2),
                    (catalog, 3),
                    (status, 4),
                    (undo.ok_or_else(invalid)?, 5),
                    (anchor, PROOF_ROLE),
                ]);
                if operation == Operation::Clr {
                    let pair = pair.ok_or_else(invalid)?;
                    let object = object.ok_or_else(invalid)?;
                    for side in 0..2 {
                        selected[7 + side] = (
                            request(
                                PageIdentity {
                                    database: allocator.identity.database,
                                    page: pair[side],
                                    owner: object.object,
                                    owner_generation: object.generation,
                                },
                                if side == 0 {
                                    PageKind::TreeAnchor
                                } else {
                                    PageKind::RowLeaf
                                },
                            ),
                            (6 + side) as u8,
                        );
                    }
                }
            }
            _ => return Err(invalid()),
        }
        Ok((slot, selected))
    }

    pub(crate) fn prepare_catalog_retirement<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        if !matches!(
            operation,
            Operation::Clr | Operation::Prune | Operation::RootFree | Operation::TerminalFinish
        ) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        self.prepare_catalog_job(owner, ordinal, operation, private)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    pub(crate) fn prepare_catalog_retirement<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        ordinal: usize,
        operation: Operation,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        self.catalog_preparation()
            .prepare_catalog_retirement(owner, ordinal, operation, private)
    }

    /// The caller may retain an original reader/snapshot while this writer and
    /// the original flusher are borrowed. No second pool or mutable owner exists.
    pub(crate) fn apply_catalog_retirement(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        match prepared.targets().len() {
            4 => self.apply_catalog_retirement_n::<4>(owner, prepared, flusher),
            7 => self.apply_catalog_retirement_n::<7>(owner, prepared, flusher),
            9 => self.apply_catalog_retirement_n::<9>(owner, prepared, flusher),
            _ => Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            )),
        }
    }
    fn apply_catalog_retirement_n<const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
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
        let epoch_change = visibility
            .prepare_epoch_change(&prepared, storage.profile())
            .map_err(visibility_error)?;
        if !std::ptr::eq(flusher.pool, self.pool) || !std::ptr::eq(flusher.storage, storage) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let pair = if prepared.operation() == Operation::Clr {
            flusher
                .control
                .io
                .preflight::<W>()
                .map_err(|e| RetentionError {
                    phase: RetentionPhase::Prepare,
                    effect: e.effect,
                    cause: RetentionCause::Flush(e),
                })?;
            if flusher.control.phase != flush::RetentionFlushPhase::Ready {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
            }
            let ordinal = prepared
                .observation
                .job_ordinal
                .expect("actual CLR ordinal");
            let page = queue::decode_jobs_page(
                prepared.before_role(2).expect("actual Job Before"),
                storage.profile().spec().namespace,
                (ordinal / 15) as u8,
            )
            .map_err(queue_error)?;
            let job = queue::decode_job(
                page.cell(ordinal % 15).expect("actual Job cell"),
                storage.profile().spec().namespace,
            )
            .map_err(queue_error)?;
            let requests: [TargetRequest; 2] = [6u8, 7u8].map(|role| {
                prepared.requests[prepared.roles[..prepared.count]
                    .iter()
                    .position(|value| *value == role)
                    .expect("actual pair role")]
            });
            Some((requests, PageLsn(job.lease_lsn())))
        } else {
            None
        };
        let requests: &[TargetRequest; N] = prepared
            .targets()
            .try_into()
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut lease = if pair.is_some() {
            Some(
                visibility
                    .reserve_references(owner)
                    .map_err(visibility_error)?,
            )
        } else {
            None
        };
        let mut current = None;
        if let Some(active_lease) = lease.as_mut() {
            let captured = (|| {
                let read = self
                    .pool
                    .reserve_target_set(owner, requests)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?
                    .try_read()
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?;
                let captured = (|| {
                    let source = visibility.catalog_rollback_source(&prepared, &read)?;
                    // Retain the real pin before releasing the read guards, so
                    // a guard-return error still reaches its checked return.
                    current = Some(
                        active_lease
                            .capture_rollback(source)
                            .map_err(visibility_error)?,
                    );
                    Ok(())
                })();
                merge(
                    captured,
                    read.release_checked()
                        .map_err(|e| frame(RetentionPhase::Prepare, e)),
                )
            })();
            if let Err(error) = captured {
                return merge(Err(error), release_references(current.take(), lease.take()));
            }
        }
        let mut write = match self
            .pool
            .reserve_target_set(owner, requests)
            .and_then(|pins| pins.try_retirement_write(visibility))
        {
            Ok(write) => write,
            Err(error) => {
                return merge(
                    Err(frame(RetentionPhase::Prepare, error)),
                    release_references(current.take(), lease.take()),
                );
            }
        };
        let installed = (|| {
            let verified = visibility.verify_catalog_retirement(&prepared, &write)?;
            if let Some((pair, _)) = pair {
                write
                    .preflight_catalog_pair(&pair)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            }
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
            let mut apply = match write.begin(verified, current.take()) {
                Ok(apply) => apply,
                Err(rejected) => {
                    current = rejected.current;
                    return Err(visibility_error(rejected.error));
                }
            };
            apply.stage_call_started().map_err(visibility_error)?;
            #[cfg(test)]
            crate::transaction::concurrency::catalog_test_support::hit(
                self.pool.bound_core(),
                crate::transaction::concurrency::catalog_test_support::Point::RetirementIntent,
            );
            let requirement = match storage.wal().stage_verified_required(
                prepared.prefix,
                prepared.observation.header,
                prepared.frame,
            ) {
                Ok(requirement) => requirement,
                Err(error) if error.effect == Effect::Unchanged => {
                    apply
                        .reject_catalog_stage(CatalogRetirementRejected {
                            prepared: &prepared,
                        })
                        .map_err(visibility_error)?;
                    current = apply.cancel_unstaged().map_err(visibility_error)?;
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
            let staged = CatalogRetirementStaged {
                prepared: &prepared,
                requirement,
            };
            apply
                .bind_staged(
                    visibility
                        .staged_catalog_retirement(&staged)
                        .map_err(visibility_error)?,
                )
                .map_err(visibility_error)?;
            catalog_mark_staged(&prepared, self.control, requirement)?;
            #[cfg(test)]
            let mut first_page = true;
            for (at, dpt_entry) in dpt.iter_mut().enumerate() {
                let Some(after) = prepared.after(at) else {
                    continue;
                };
                let permit =
                    DirtyInstallPermit::from_retention(live::RetentionInstall::allocation(
                        self.pool.bound_core(),
                        prepared.stamp(at).expect("checked existing"),
                        requirement,
                        after,
                    ));
                apply
                    .install_page(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                #[cfg(test)]
                if first_page {
                    first_page = false;
                    crate::transaction::concurrency::catalog_test_support::hit(
                        self.pool.bound_core(),
                        crate::transaction::concurrency::catalog_test_support::Point::RetirementFirstPage,
                    );
                }
                apply
                    .install_metadata(at, &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
                apply
                    .install_dpt(dpt_entry.take().expect("actual DPT preflight"), &permit)
                    .map_err(|e| frame(RetentionPhase::Install, e))?;
            }
            catalog_apply(&prepared, self.control, storage.profile(), requirement)?;
            self.history.installed(requirement)?;
            let applied = CatalogRetirementApplied { staged };
            apply
                .finish_applied(
                    visibility
                        .applied_catalog_retirement(&applied)
                        .map_err(visibility_error)?,
                )
                .map_err(visibility_error)?;
            Ok(requirement)
        })();
        let installed = merge(
            installed,
            write
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Install, e)),
        );
        let cleanup = release_references(current.take(), lease.take());
        let requirement = merge(installed, cleanup).map_err(|error| {
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
        if let Some((pair, reserve)) = pair {
            let allocator_at = prepared.roles[..prepared.count]
                .iter()
                .position(|role| *role == 0)
                .expect("CLR allocator");
            let read = self
                .pool
                .reserve_target_set(owner, &[prepared.requests[allocator_at]])
                .and_then(|pins| pins.try_read())
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
            let observed = read
                .observe(0)
                .map(|value| value.frame)
                .map_err(|e| frame(RetentionPhase::Install, e));
            let allocator_after = merge(
                observed,
                read.release_checked()
                    .map_err(|e| frame(RetentionPhase::Install, e)),
            )?;
            let selected_generation = flusher.control.io.snapshot().known.generation;
            let before = pair.map(|request| {
                prepared.stamp(
                    prepared
                        .targets()
                        .iter()
                        .position(|value| *value == request)
                        .expect("CLR pair target"),
                )
            });
            self.pool
                .retire_tx_pair(super::super::TxPairRetirePermit {
                    core: self.pool.bound_core(),
                    actor: prepared.observation.actor.expect("actual CLR actor"),
                    reserve,
                    io: super::super::PairIoQuiescence {
                        original: flusher.control,
                        binding: &storage.binding,
                        fence: storage.session_fence(),
                        selected_generation,
                        pair,
                    },
                    requirement,
                    allocator_after,
                    before,
                })
                .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
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
        Ok(prepared.observation)
    }
}

/// Only the caller's selected copy and compact dispatch locals survive into
/// preparation. The callee also charges its own selected input; neither array
/// is subtracted merely because its elements implement Copy.
pub(in super::super::super) fn prepare_caller_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<[(TargetRequest, u8); 9]>()
        + size_of::<Option<usize>>()
        + size_of::<Change<'_>>()
        + 2 * size_of::<Operation>()
        + 4 * size_of::<usize>()
        + 9 * size_of::<&[u8]>()
        + 2 * size_of::<RetentionError>()
}

/// Selection's larger Job/identity packet and all array/Result returns end
/// before shared preparation begins. It has only queue/registry leaf calls.
pub(in super::super::super) fn selection_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<queue::JobView<'_>>()
        + size_of::<TxnRef>()
        + size_of::<StatusSlotRef>()
        + size_of::<Option<crate::storage::undo_ref::UndoRef>>()
        + size_of::<Option<[PageRef; 2]>>()
        + size_of::<Option<ObjectHandle>>()
        + size_of::<Option<usize>>()
        + 6 * size_of::<TargetRequest>()
        + size_of::<Option<TargetRequest>>()
        + size_of::<[(TargetRequest, u8); 7]>()
        + size_of::<[PageRef; 2]>()
        + size_of::<ObjectHandle>()
        + size_of::<Change<'_>>()
        + 2 * size_of::<Operation>()
        + 4 * size_of::<usize>()
        + 9 * size_of::<&[u8]>()
        + 2 * size_of::<RetentionError>()
        + size_of::<[(TargetRequest, u8); 9]>()
        + 2 * size_of::<Result<(Option<usize>, [(TargetRequest, u8); 9])>>()
        + queue::source_bytes().max(dirty_frame::target_set_call_bytes::<1>())
}

pub(in super::super::super) fn source_bytes() -> usize {
    use crate::storage::dirty_frame;
    use std::mem::size_of;
    visibility::epoch_change_carriers()
        + size_of::<PreparedCatalog<'_, '_, '_>>()
        + 2 * size_of::<Result<PreparedCatalog<'_, '_, '_>>>()
        + size_of::<CatalogRetirementStaged<'_, '_, '_, '_>>()
        + size_of::<CatalogRetirementApplied<'_, '_, '_, '_>>()
        + size_of::<CatalogRetirementRejected<'_, '_, '_, '_>>()
        + 2 * size_of::<visibility::ReferenceLease<'_, '_, '_, '_>>()
        + 2 * size_of::<visibility::UndoPin<'_, '_, '_, '_>>()
        + 2 * size_of::<visibility::VerifiedRetirement<'_, '_>>()
        + 2 * size_of::<visibility::StagedRetirement<'_, '_>>()
        + 2 * size_of::<visibility::AppliedRetirement<'_, '_>>()
        + 2 * size_of::<super::super::TxPairRetirePermit<'_, '_, '_, '_>>()
        + 2 * size_of::<super::super::PairIoQuiescence<'_, '_, '_, '_>>()
        + 3 * size_of::<[Option<dirty_frame::DptStamp<'_>>; 9]>()
        + 2 * size_of::<[Option<FrameStamp>; 2]>()
        + 3 * size_of::<[TargetRequest; 9]>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 3 * size_of::<DurableWal<'_>>()
        + 4 * size_of::<crate::storage::page::PageView<'_>>()
        + 4 * size_of::<queue::JobView<'_>>()
        + 2 * size_of::<Observation>()
        + 4 * size_of::<WalPrefix>()
        + 12 * size_of::<Result<()>>()
        + 10 * size_of::<RetentionError>()
        + 32 * size_of::<usize>()
        + 32 * size_of::<&[u8]>()
        + dirty_frame::retirement_source_bytes::<9>()
        + dirty_frame::target_set_call_bytes::<9>()
        + catalog_control_source_bytes()
            .max(dirty_frame::tx_pair_source_census())
            .max(visibility::source_census().peak_leaf)
            .max(visibility::epoch_transition_source_bytes())
            .max(
                crate::storage::wal_io::DURABILITY_CALL_BYTES
                    + 2 * size_of::<
                        std::result::Result<WalRequirement<'_>, crate::storage::wal_io::WalIoError>,
                    >()
                    + 2 * size_of::<
                        std::result::Result<DurableWal<'_>, crate::storage::wal_io::WalIoError>,
                    >(),
            )
}
